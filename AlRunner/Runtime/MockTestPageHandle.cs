using Microsoft.Dynamics.Nav.Runtime;
using Microsoft.Dynamics.Nav.Types;

namespace AlRunner.Runtime;

/// <summary>
/// Mock for NavTestPageHandle — the BC type emitted for TestPage variables in test codeunits.
///
/// BC generates: new NavTestPageHandle(this, pageId)
/// Rewriter transforms to: new MockTestPageHandle(pageId)
///
/// Supports:
/// - ALOpenEdit(), ALOpenView(), ALOpenNew(), ALClose() — lifecycle no-ops
/// - ALTrap() — marks page as expecting modal open (no-op)
/// - GetField(fieldHash) — returns MockTestPageField for value get/set
/// - GetBuiltInAction(FormResult) — returns MockTestPageAction for OK/Cancel/Close
/// - ModalResult — tracks the FormResult set by action invocation (OK/Cancel)
/// </summary>
public class MockTestPageHandle
{
    public int PageId { get; }

    private readonly Dictionary<int, MockTestPageField> _fields = new();

    /// <summary>
    /// The modal result set by invoking a built-in action (OK, Cancel, etc.).
    /// Defaults to LookupOK (3) — same as BC when the handler completes without
    /// explicitly invoking Cancel.
    /// </summary>
    public FormResult ModalResult { get; set; } = FormResult.LookupOK;

    public MockTestPageHandle() { }

    public MockTestPageHandle(int pageId)
    {
        PageId = pageId;
    }

    // Lifecycle methods — no-ops in standalone mode
    public void ALOpenEdit() { }
    public void ALOpenView() { }
    public void ALOpenNew() { }
    public void ALClose() { }
    public void ALTrap() { }

    /// <summary>
    /// Returns the page caption. Stub returns "TestPage" since the runner
    /// does not have page metadata infrastructure.
    /// </summary>
    public string ALCaption => "TestPage";

    /// <summary>
    /// Navigates to the first record on the page. Stub returns true.
    /// </summary>
    public bool ALFirst() => true;

    /// <summary>
    /// Navigates to the record matching the given key values. Stub returns true.
    /// BC emits: ALGoToKey(DataError.TrapError, ALCompiler.ToNavValue(...))
    /// </summary>
    public bool ALGoToKey(DataError errorLevel, params NavValue[] keyValues) => true;

    /// <summary>
    /// Returns a filter object for the TestPage. BC emits:
    ///   tP.ALFilter.ALSetFilter(fieldNo, filterValue)
    /// </summary>
    public MockTestPageFilter ALFilter { get; } = new();

    /// <summary>
    /// Returns a MockTestPageField for the given field hash.
    /// BC generates field hashes (not field IDs) for TestPage field access.
    /// The field stores values in memory for get/set assertions.
    /// </summary>
    public MockTestPageField GetField(int fieldHash)
    {
        if (!_fields.TryGetValue(fieldHash, out var field))
        {
            field = new MockTestPageField(fieldHash);
            _fields[fieldHash] = field;
        }
        return field;
    }

    /// <summary>
    /// Returns a MockTestPageAction for a custom page action.
    /// BC emits <c>tP.Target.GetAction(actionHash).ALInvoke()</c> for
    /// <c>TestPage.MyAction.Invoke()</c>. Custom actions in standalone mode
    /// are no-ops — the returned action's ALInvoke() does nothing.
    /// </summary>
    public MockTestPageAction GetAction(int actionHash)
    {
        return new MockTestPageAction();
    }

    /// <summary>
    /// Returns a MockTestPageAction for built-in actions (OK, Cancel, Close, etc.).
    /// BC casts FormResult enum values: GetBuiltInAction((FormResult)1) for OK.
    /// The action is linked back to this handle so ALInvoke() can set the ModalResult.
    /// </summary>
    public MockTestPageAction GetBuiltInAction(object formResult)
    {
        var fr = (FormResult)formResult;
        return new MockTestPageAction(this, fr);
    }
}

/// <summary>
/// Mock for TestPage field access. BC generates:
///   tP.GetField(fieldHash).ALSetValue(this.Session, value)
///   tP.GetField(fieldHash).ALValue
///
/// Stores the last set value as a NavValue and returns it via ALValue.
/// </summary>
public class MockTestPageField
{
    private readonly int _fieldHash;
    private NavValue _value;

    public MockTestPageField(int fieldHash)
    {
        _fieldHash = fieldHash;
        _value = new NavText("");
    }

    /// <summary>
    /// Set the field value. BC passes (session, navValue) — session is null in standalone mode.
    /// </summary>
    public void ALSetValue(object? session, NavValue value)
    {
        _value = value;
    }

    /// <summary>
    /// Get the current field value as a NavValue.
    /// BC reads this to pass to Assert.AreEqual via ALCompiler.ToVariant.
    /// </summary>
    public NavValue ALValue => _value;

    /// <summary>
    /// ALCaption — returns the field caption. Stub: empty string.
    /// BC emits <c>tP.GetField(hash).ALCaption</c> for TestPage field Caption reads.
    /// </summary>
    public NavText ALCaption => new NavText("");

    /// <summary>
    /// ALVisible — whether the field is visible on the page. Stub: always true.
    /// BC emits <c>tP.GetField(hash).ALVisible()</c> as a method call for reads.
    /// </summary>
    public bool ALVisible() => true;

    /// <summary>
    /// ALEditable — whether the field is editable on the page. Stub: always true.
    /// BC emits <c>tP.GetField(hash).ALEditable()</c> as a method call for reads.
    /// </summary>
    public bool ALEditable() => true;

    /// <summary>
    /// ALLookup — triggers the lookup action on the field. No-op in standalone mode.
    /// BC emits <c>tP.GetField(hash).ALLookup()</c>.
    /// </summary>
    public void ALLookup() { }

    /// <summary>
    /// ALDrilldown — triggers the drilldown action on the field. No-op in standalone mode.
    /// BC emits <c>tP.GetField(hash).ALDrilldown()</c>.
    /// </summary>
    public void ALDrilldown() { }
}

/// <summary>
/// Mock for TestPage built-in actions (OK, Cancel, Close).
/// BC generates: tP.GetBuiltInAction((FormResult)1).ALInvoke()
///
/// When ALInvoke() is called, sets the parent MockTestPageHandle's ModalResult
/// to the corresponding FormResult. This allows RunModal interception to return
/// the correct result based on whether the handler invoked OK or Cancel.
/// </summary>
public class MockTestPageAction
{
    private readonly MockTestPageHandle? _parent;
    private readonly FormResult _result;

    /// <summary>Parameterless ctor for backward compat (non-modal usage).</summary>
    public MockTestPageAction() { _result = FormResult.LookupOK; }

    /// <summary>Create an action linked to a TestPage handle with a specific result.</summary>
    public MockTestPageAction(MockTestPageHandle parent, FormResult result)
    {
        _parent = parent;
        _result = result;
    }

    public void ALInvoke()
    {
        if (_parent != null)
        {
            // Map the "page action" FormResult to the "modal return" FormResult.
            // BC maps OK (1) -> LookupOK (3), Cancel (2) -> LookupCancel (4).
            _parent.ModalResult = _result switch
            {
                FormResult.OK => FormResult.LookupOK,
                FormResult.Cancel => FormResult.LookupCancel,
                _ => _result
            };
        }
    }
}

/// <summary>
/// Mock for TestPage.Filter property. BC emits:
///   tP.ALFilter.ALSetFilter(fieldNo, filterValue)
///
/// This is a no-op stub — the runner does not track TestPage-level filters.
/// </summary>
public class MockTestPageFilter
{
    /// <summary>
    /// Sets a filter on the given field. No-op in standalone mode.
    /// BC emits: ALSetFilter(fieldNo, filterExpression)
    /// </summary>
    public void ALSetFilter(int fieldNo, string filterExpression) { }
}
