using System.Text.RegularExpressions;

namespace AlRunner;

/// <summary>
/// Classifies AL compiler diagnostics — in particular, identifies when an AL0275
/// "ambiguous reference" error is a self-duplicate caused by the same extension
/// being loaded twice under different GUIDs.
/// </summary>
public static class DiagnosticClassifier
{
    // Matches the extension identity embedded in an AL0275 message.
    // Example full message:
    //   'BBW Table' is an ambiguous reference between 'BBW Table' defined by the extension
    //   'Blue Bear Waste by Blue Bear Waste (27.0.0.0)' and 'BBW Table' defined by the
    //   extension 'Blue Bear Waste by Blue Bear Waste (27.0.0.0)'.
    private static readonly Regex ExtensionIdPattern =
        new(@"defined by the extension '([^']+)'", RegexOptions.Compiled);

    /// <summary>
    /// Returns true when an AL0275 diagnostic message describes a self-duplicate:
    /// both sides of the ambiguity reference the same extension identity
    /// (publisher, name, and version are identical, case-insensitively).
    /// </summary>
    public static bool IsSelfDuplicateAmbiguity(string message)
    {
        var ids = ExtractAmbiguityExtensionIds(message);
        if (ids is null) return false;
        return string.Equals(ids.Value.Left, ids.Value.Right, StringComparison.OrdinalIgnoreCase);
    }

    /// <summary>
    /// Parses both extension identity strings from an AL0275 message.
    /// Returns null if the message doesn't match the expected format.
    /// </summary>
    public static (string Left, string Right)? ExtractAmbiguityExtensionIds(string message)
    {
        var matches = ExtensionIdPattern.Matches(message);
        if (matches.Count < 2) return null;
        return (matches[0].Groups[1].Value, matches[1].Groups[1].Value);
    }
}
