using System.Xml.Linq;

namespace AlRunner;

/// <summary>
/// Scans package directories for .app files and returns a deduplicated set of
/// <see cref="PackageSpec"/> entries suitable for use as symbol references.
///
/// Deduplication strategy (two passes):
/// 1. By GUID — if the same app GUID appears more than once (e.g. the same file
///    copied into multiple package directories), only the entry with the highest
///    version is kept.
/// 2. By identity (publisher+name+version, case-insensitive) — if the same logical
///    package appears more than once with different GUIDs (the root cause of AL0275
///    self-duplicate errors), only one GUID is kept.  The surviving GUID is chosen
///    deterministically as the lexicographically smallest GUID to make builds
///    reproducible across different file-system orderings.
/// </summary>
public static class PackageScanner
{
    /// <summary>
    /// Scan one or more package directories and return deduplicated package specs.
    /// </summary>
    /// <param name="packageDirs">Directories to search recursively for *.app files.</param>
    /// <param name="excludeGuid">Optional app GUID to exclude (the app being compiled).</param>
    /// <param name="excludeName">Optional app name to exclude (the app being compiled).</param>
    /// <returns>Deduplicated list of package specs, ordered deterministically.</returns>
    public static IReadOnlyList<PackageSpec> ScanForSpecs(
        IEnumerable<string> packageDirs,
        Guid? excludeGuid = null,
        string? excludeName = null)
    {
        // Pass 1: load all app manifests; deduplicate by GUID keeping highest version.
        var byGuid = new Dictionary<Guid, PackageSpec>();

        foreach (var dir in packageDirs)
        {
            if (!Directory.Exists(dir)) continue;
            foreach (var appFile in Directory.GetFiles(dir, "*.app", SearchOption.AllDirectories))
            {
                try
                {
                    var spec = ReadPackageSpec(appFile);
                    if (spec == null) continue;
                    if (excludeGuid.HasValue && spec.AppId == excludeGuid.Value) continue;
                    if (excludeName != null &&
                        string.Equals(spec.Name, excludeName, StringComparison.OrdinalIgnoreCase))
                        continue;

                    if (!byGuid.TryGetValue(spec.AppId, out var existing) ||
                        spec.Version > existing.Version)
                        byGuid[spec.AppId] = spec;
                }
                catch { /* corrupt or unreadable .app — skip silently */ }
            }
        }

        // Pass 2: deduplicate by (publisher|name|version), keeping the lexicographically
        // smallest GUID.  This collapses self-duplicates that differ only in GUID and
        // prevents AL0275 "ambiguous reference" errors at compile time.
        var byIdentity = new Dictionary<string, PackageSpec>(StringComparer.OrdinalIgnoreCase);
        foreach (var spec in byGuid.Values.OrderBy(s => s.AppId))
        {
            var key = $"{spec.Publisher}|{spec.Name}|{spec.Version}";
            if (!byIdentity.ContainsKey(key))
                byIdentity[key] = spec;
            // else: duplicate identity — discard; the first (lowest GUID) survives
        }

        return byIdentity.Values.OrderBy(s => s.AppId).ToList();
    }

    private static PackageSpec? ReadPackageSpec(string appPath)
    {
        var doc = AlTranspiler.LoadNavxManifest(appPath);
        if (doc == null) return null;

        XNamespace ns = "http://schemas.microsoft.com/navx/2015/manifest";
        var appElement = doc.Root?.Element(ns + "App");
        var idStr = appElement?.Attribute("Id")?.Value;
        var name = appElement?.Attribute("Name")?.Value ?? "";
        var publisher = appElement?.Attribute("Publisher")?.Value ?? "";
        var versionStr = appElement?.Attribute("Version")?.Value ?? "1.0.0.0";

        if (idStr == null || !Guid.TryParse(idStr, out var guid)) return null;
        if (!Version.TryParse(versionStr, out var version)) return null;

        return new PackageSpec(publisher, name, version, guid);
    }
}

/// <summary>
/// Represents a resolved .app package (publisher, name, version, GUID) after deduplication.
/// </summary>
public record PackageSpec(string Publisher, string Name, Version Version, Guid AppId);
