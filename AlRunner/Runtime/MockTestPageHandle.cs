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
