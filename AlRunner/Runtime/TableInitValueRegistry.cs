using System.Text.RegularExpressions;
using Microsoft.Dynamics.Nav.Runtime;

namespace AlRunner.Runtime;

/// <summary>
/// Transpile-time registry of <c>InitValue</c> defaults declared in AL
/// field headers. BC applies these when the platform calls
/// <c>Rec.Init()</c>, but al-runner's MockRecordHandle used to zero the
/// field bag unconditionally. We parse AL source for
/// <c>field(N; Name; Type) { InitValue = X }</c> at pipeline start, then
/// <see cref="MockRecordHandle.ALInit"/> consults this registry.
///
/// Parsing the AL source with regex is coarser than the real BC
/// compiler, but only needs to cover the attributed defaults reachable
/// from test fixtures — anything that compiles without an InitValue
/// stays at the type's zero default.
/// </summary>
public static class TableInitValueRegistry
{
    // `table N "Name" {`  or  `table N Name {`
    private static readonly Regex TableHeader = new(
        @"\btable\s+(\d+)\s+(?:""([^""]+)""|([A-Za-z_][A-Za-z0-9_]*))\s*\{",
        RegexOptions.Compiled | RegexOptions.IgnoreCase);

    // field(id; name; type) { ... }
    // Non-greedy body capture so subsequent fields don't get pulled in.
    private static readonly Regex FieldBlock = new(
        @"\bfield\s*\(\s*(\d+)\s*;\s*(?:""([^""]+)""|([A-Za-z_][A-Za-z0-9_]*))\s*;\s*([^)]+?)\)\s*\{([^}]*)\}",
        RegexOptions.Compiled | RegexOptions.IgnoreCase);

    // InitValue = <rhs>;    (strip any trailing ; or whitespace)
    private static readonly Regex InitValueRegex = new(
        @"\bInitValue\s*=\s*([^;]+?)\s*;",
        RegexOptions.Compiled | RegexOptions.IgnoreCase);

    // tableId -> list of (fieldId, raw AL init expression, field type)
    private static readonly Dictionary<int, List<(int FieldId, string InitExpr, string FieldType)>> _byTable = new();

    public static void Clear() => _byTable.Clear();

    /// <summary>Parse an AL source string and register any InitValue attributes inside its tables.</summary>
    public static void ParseAndRegister(string alSource)
    {
        foreach (Match tm in TableHeader.Matches(alSource))
        {
            if (!int.TryParse(tm.Groups[1].Value, out var tableId)) continue;

            // Find the end of this table block by brace counting.
            int start = tm.Index + tm.Length;
            int depth = 1;
            int i = start;
            while (i < alSource.Length && depth > 0)
            {
                char c = alSource[i];
                if (c == '{') depth++;
                else if (c == '}') depth--;
                i++;
            }
            if (depth != 0) continue;
            var body = alSource.Substring(start, i - start - 1);

            var entries = new List<(int, string, string)>();
            foreach (Match fm in FieldBlock.Matches(body))
            {
                if (!int.TryParse(fm.Groups[1].Value, out var fieldId)) continue;
                var fieldType = fm.Groups[4].Value.Trim();
                var fieldBody = fm.Groups[5].Value;
                var iv = InitValueRegex.Match(fieldBody);
                if (!iv.Success) continue;
                var initExpr = iv.Groups[1].Value.Trim();
                entries.Add((fieldId, initExpr, fieldType));
            }

            if (entries.Count > 0)
                _byTable[tableId] = entries;
        }
    }

    /// <summary>
    /// Apply all declared InitValue defaults for the given table onto the
    /// provided field bag. Called from <see cref="MockRecordHandle.ALInit"/>.
    /// </summary>
    public static void ApplyInitValues(int tableId, Dictionary<int, NavValue> fields)
    {
        if (!_byTable.TryGetValue(tableId, out var entries)) return;

        foreach (var (fieldId, initExpr, fieldType) in entries)
        {
            var navValue = BuildInitValue(initExpr, fieldType);
            if (navValue != null)
                fields[fieldId] = navValue;
        }
    }

    private static NavValue? BuildInitValue(string rawExpr, string fieldType)
    {
        // Boolean literal
        if (rawExpr.Equals("true", StringComparison.OrdinalIgnoreCase))
            return NavBoolean.Create(true);
        if (rawExpr.Equals("false", StringComparison.OrdinalIgnoreCase))
            return NavBoolean.Create(false);

        // Integer literal
        if (int.TryParse(rawExpr, out var asInt))
            return NavInteger.Create(asInt);

        // Decimal literal
        if (decimal.TryParse(rawExpr, System.Globalization.NumberStyles.Any,
                System.Globalization.CultureInfo.InvariantCulture, out var asDec))
            return NavDecimal.Create(new Microsoft.Dynamics.Nav.Runtime.Decimal18(asDec));

        // Single-quoted string literal -> NavText
        if (rawExpr.Length >= 2 && rawExpr[0] == '\'' && rawExpr[^1] == '\'')
            return new NavText(rawExpr.Substring(1, rawExpr.Length - 2));

        // Enum member (bare identifier or quoted). For enum-typed fields,
        // store the ordinal as a NavInteger — MockRecordHandle's option
        // handling reads field values through NavInteger/NavOption paths
        // and AL comparisons coerce both sides.
        var fieldTypeLower = fieldType.ToLowerInvariant();
        if (fieldTypeLower.StartsWith("enum") || fieldTypeLower.StartsWith("option"))
        {
            // Strip surrounding quotes if any
            var memberName = rawExpr.Trim('"', '\'');
            // Find this enum's object ID from the field type declaration.
            // For `Enum "IV Mode"` the type text is `Enum "IV Mode"`.
            var enumNameMatch = Regex.Match(fieldType,
                @"\bEnum\s+(?:""([^""]+)""|([A-Za-z_][A-Za-z0-9_]*))",
                RegexOptions.IgnoreCase);
            if (enumNameMatch.Success)
            {
                var enumName = enumNameMatch.Groups[1].Success
                    ? enumNameMatch.Groups[1].Value
                    : enumNameMatch.Groups[2].Value;
                var ordinal = EnumRegistry.GetOrdinalByName(enumName, memberName);
                if (ordinal.HasValue)
                    return MockRecordHandle.CreateOptionValue(ordinal.Value);
            }
        }

        return null;
    }
}
