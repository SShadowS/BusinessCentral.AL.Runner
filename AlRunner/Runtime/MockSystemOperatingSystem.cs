namespace AlRunner.Runtime;

/// <summary>
/// Stub for <c>ALSystemOperatingSystem</c>. The real type's
/// <c>ALHyperlink</c> reaches into <c>NavSession</c> to dispatch an
/// OS-level URL open and throws <c>NullReferenceException</c> in
/// standalone mode — there's no session or client surface.
///
/// MockSystemOperatingSystem makes Hyperlink a no-op so tests that
/// exercise documentation-linking code paths (a common pattern in
/// AL rules that call <c>Hyperlink(WikiUrl)</c> from
/// <c>ShowMoreDetails</c>) don't crash.
/// </summary>
public static class MockSystemOperatingSystem
{
    public static void ALHyperlink(string hyperlink, System.Guid automationId)
    {
        // No-op: there's no client to open the URL.
    }

    public static void ALHyperlink(string hyperlink)
    {
        // No-op.
    }
}
