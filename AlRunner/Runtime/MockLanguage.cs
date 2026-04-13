namespace AlRunner.Runtime;

/// <summary>
/// Standalone replacement for BC language APIs (GlobalLanguage).
/// ALSystemLanguage.get_ALGlobalLanguage / set_ALGlobalLanguage crash because
/// there is no live BC session context in standalone mode. This class provides
/// an in-memory static field with a sensible default (1033 = ENU).
/// </summary>
public static class MockLanguage
{
    private static int _globalLanguage = 1033; // ENU default

    /// <summary>
    /// Replaces ALSystemLanguage.ALGlobalLanguage (get/set).
    /// Default is 1033 (English US — ENU), matching the BC default in a fresh environment.
    /// </summary>
    public static int ALGlobalLanguage
    {
        get => _globalLanguage;
        set => _globalLanguage = value;
    }

    /// <summary>
    /// Resets the language back to the ENU default between tests.
    /// Called by Executor.ResetAll() between test runs.
    /// </summary>
    public static void Reset()
    {
        _globalLanguage = 1033;
    }
}
