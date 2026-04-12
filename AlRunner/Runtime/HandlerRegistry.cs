using System.Reflection;
using Microsoft.Dynamics.Nav.Runtime;
using Microsoft.Dynamics.Nav.Types;

namespace AlRunner.Runtime;

/// <summary>
/// Registry for test handler functions (ConfirmHandler, MessageHandler, etc.).
///
/// In BC, test codeunits declare handler functions with attributes like
/// [ConfirmHandler] and [MessageHandler]. When the code under test calls
/// Confirm() or Message(), the BC test framework dispatches to the registered
/// handler instead of showing UI.
///
/// The Executor registers handlers before each test by reading the Handlers
/// property from the [NavTest] attribute and finding matching [NavHandler]
/// methods on the test codeunit.
/// </summary>
public static class HandlerRegistry
{
    // The parent codeunit instance (test codeunit) that owns the handler methods
    private static object? _parentInstance;

    // Registered confirm handler: method that takes (NavText question, ByRef<bool> reply)
    private static MethodInfo? _confirmHandler;

    // Registered message handler: method that takes (NavText msg)
    private static MethodInfo? _messageHandler;

    /// <summary>
    /// Register handlers for the current test. Called by the Executor before each test.
    /// </summary>
    /// <param name="parentInstance">The test codeunit instance</param>
    /// <param name="parentType">The test codeunit type</param>
    /// <param name="handlerNames">Comma-separated handler method names from [NavTest].Handlers</param>
    public static void RegisterHandlers(object parentInstance, Type parentType, string? handlerNames)
    {
        Reset();
        if (string.IsNullOrWhiteSpace(handlerNames))
            return;

        _parentInstance = parentInstance;

        var names = handlerNames.Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
        foreach (var name in names)
        {
            // Find the method on the parent type
            var method = parentType.GetMethod(name,
                BindingFlags.Public | BindingFlags.Instance | BindingFlags.DeclaredOnly);
            if (method == null) continue;

            // Check for [NavHandler] attribute to determine handler type
            var handlerAttr = method.GetCustomAttributes()
                .FirstOrDefault(a => a.GetType().Name == "NavHandlerAttribute");
            if (handlerAttr != null)
            {
                // Read the HandlerType property (NavHandlerType enum)
                var handlerTypeProp = handlerAttr.GetType().GetProperty("HandlerType");
                if (handlerTypeProp != null)
                {
                    var handlerType = handlerTypeProp.GetValue(handlerAttr);
                    var handlerTypeName = handlerType?.ToString() ?? "";

                    if (handlerTypeName == "Confirm")
                        _confirmHandler = method;
                    else if (handlerTypeName == "Message")
                        _messageHandler = method;
                }
            }
        }
    }

    /// <summary>
    /// Invoke the registered confirm handler, if any.
    /// Returns (true, reply) if a handler was found and invoked.
    /// Returns (false, default) if no handler is registered.
    /// </summary>
    public static (bool Handled, bool Reply) InvokeConfirmHandler(string question)
    {
        if (_confirmHandler == null || _parentInstance == null)
            return (false, false);

        // The handler signature is: ConfirmYesHandler(NavText question, ByRef<bool> reply)
        // ByRef<T> is a delegate-based wrapper with getter/setter fields. Default construction
        // leaves the delegates null, causing NullReferenceException. We wire them to local storage.
        var parameters = _confirmHandler.GetParameters();
        if (parameters.Length < 2)
            return (false, false);

        var byRefType = parameters[1].ParameterType;
        var byRef = Activator.CreateInstance(byRefType)!;

        // Create a backing store: bool[] with one element
        var storage = new bool[] { false };

        // Find the setter and getter delegate fields and wire them to our storage
        foreach (var field in byRefType.GetFields(BindingFlags.NonPublic | BindingFlags.Public | BindingFlags.Instance))
        {
            var ft = field.FieldType;
            // Setter: Action<bool> or equivalent delegate
            if (ft.Name.Contains("Action") || (field.Name.Contains("set") || field.Name.Contains("Set")))
            {
                try
                {
                    Action<bool> setter = v => storage[0] = v;
                    field.SetValue(byRef, Delegate.CreateDelegate(ft, setter.Target!, setter.Method));
                }
                catch { /* try next field */ }
            }
            // Getter: Func<bool> or equivalent delegate
            if (ft.Name.Contains("Func") || (field.Name.Contains("get") || field.Name.Contains("Get")))
            {
                try
                {
                    Func<bool> getter = () => storage[0];
                    field.SetValue(byRef, Delegate.CreateDelegate(ft, getter.Target!, getter.Method));
                }
                catch { /* try next field */ }
            }
        }

        try
        {
            _confirmHandler.Invoke(_parentInstance, new object[] { new NavText(question), byRef });
        }
        catch (TargetInvocationException tie) when (tie.InnerException != null)
        {
            throw tie.InnerException;
        }

        return (true, storage[0]);
    }

    /// <summary>
    /// Invoke the registered message handler, if any.
    /// Returns true if a handler was found and invoked.
    /// </summary>
    public static bool InvokeMessageHandler(string message)
    {
        if (_messageHandler == null || _parentInstance == null)
            return false;

        try
        {
            _messageHandler.Invoke(_parentInstance, new object[] { new NavText(message) });
        }
        catch (TargetInvocationException tie) when (tie.InnerException != null)
        {
            throw tie.InnerException;
        }

        return true;
    }

    /// <summary>
    /// Check if a confirm handler is registered.
    /// </summary>
    public static bool HasConfirmHandler => _confirmHandler != null;

    /// <summary>
    /// Check if a message handler is registered.
    /// </summary>
    public static bool HasMessageHandler => _messageHandler != null;

    /// <summary>
    /// Reset all registered handlers. Called between tests.
    /// </summary>
    public static void Reset()
    {
        _parentInstance = null;
        _confirmHandler = null;
        _messageHandler = null;
    }
}
