// AlRunner/Runtime/IterationTracker.cs
namespace AlRunner.Runtime;

/// <summary>
/// Static collector for per-iteration data during loop execution.
/// When enabled, injected code calls EnterLoop/EnterIteration/EndIteration/ExitLoop
/// to capture variable values, messages, and executed lines per iteration.
/// Mirrors the ValueCapture and MessageCapture patterns.
/// </summary>
public static class IterationTracker
{
    private static bool _enabled;
    private static readonly List<LoopRecord> _loops = new();
    private static readonly Stack<ActiveLoop> _loopStack = new();
    private static int _nextLoopId;

    public static bool Enabled => _enabled;
    public static void Enable() => _enabled = true;
    public static void Disable() => _enabled = false;

    public static void Reset()
    {
        _loops.Clear();
        _loopStack.Clear();
        _nextLoopId = 0;
    }

    public static int EnterLoop(int sourceStartLine, int sourceEndLine)
    {
        if (!_enabled) return -1;

        var loopId = _nextLoopId++;
        int? parentLoopId = _loopStack.Count > 0 ? _loopStack.Peek().LoopId : null;
        int? parentIteration = _loopStack.Count > 0 ? _loopStack.Peek().CurrentIteration : null;

        var record = new LoopRecord
        {
            LoopId = loopId,
            SourceStartLine = sourceStartLine,
            SourceEndLine = sourceEndLine,
            ParentLoopId = parentLoopId,
            ParentIteration = parentIteration,
        };
        _loops.Add(record);

        _loopStack.Push(new ActiveLoop
        {
            LoopId = loopId,
            Record = record,
        });

        return loopId;
    }

    public static void EnterIteration(int loopId)
    {
        if (!_enabled) return;
        if (_loopStack.Count == 0 || _loopStack.Peek().LoopId != loopId) return;

        var active = _loopStack.Peek();
        active.CurrentIteration++;
        active.ValueSnapshotBefore = ValueCapture.GetCaptures().Count;
        active.MessageSnapshotBefore = MessageCapture.GetMessages().Count;
        active.HitStatementsBefore = AlScope.GetHitStatements();
    }

    public static void EndIteration(int loopId)
    {
        if (!_enabled) return;
        if (_loopStack.Count == 0 || _loopStack.Peek().LoopId != loopId) return;

        var active = _loopStack.Peek();

        // Captured values added during this iteration
        var allValues = ValueCapture.GetCaptures();
        var iterValues = new List<CapturedValueSnapshot>();
        for (int i = active.ValueSnapshotBefore; i < allValues.Count; i++)
        {
            var v = allValues[i];
            iterValues.Add(new CapturedValueSnapshot { VariableName = v.VariableName, Value = v.Value ?? "" });
        }

        // Messages added during this iteration
        var allMessages = MessageCapture.GetMessages();
        var iterMessages = new List<string>();
        for (int i = active.MessageSnapshotBefore; i < allMessages.Count; i++)
            iterMessages.Add(allMessages[i]);

        // Lines hit during this iteration (new hits since snapshot)
        var currentHits = AlScope.GetHitStatements();
        var iterLines = new List<int>();
        foreach (var hit in currentHits)
        {
            if (active.HitStatementsBefore == null || !active.HitStatementsBefore.Contains(hit))
                iterLines.Add(hit.Id);
        }

        active.Record.Steps.Add(new IterationStep
        {
            Iteration = active.CurrentIteration,
            CapturedValues = iterValues,
            Messages = iterMessages,
            LinesExecuted = iterLines,
        });
    }

    public static void ExitLoop(int loopId)
    {
        if (!_enabled) return;
        if (_loopStack.Count == 0 || _loopStack.Peek().LoopId != loopId) return;

        var active = _loopStack.Pop();
        active.Record.IterationCount = active.CurrentIteration;
    }

    public static List<LoopRecord> GetLoops() => new(_loops);

    // --- Data classes ---

    public class LoopRecord
    {
        public int LoopId { get; init; }
        public int SourceStartLine { get; init; }
        public int SourceEndLine { get; init; }
        public int? ParentLoopId { get; init; }
        public int? ParentIteration { get; init; }
        public int IterationCount { get; set; }
        public List<IterationStep> Steps { get; init; } = new();
    }

    public class IterationStep
    {
        public int Iteration { get; init; }
        public List<CapturedValueSnapshot> CapturedValues { get; init; } = new();
        public List<string> Messages { get; init; } = new();
        public List<int> LinesExecuted { get; init; } = new();
    }

    public class CapturedValueSnapshot
    {
        public string VariableName { get; init; } = "";
        public string Value { get; init; } = "";
    }

    private class ActiveLoop
    {
        public int LoopId { get; init; }
        public LoopRecord Record { get; init; } = null!;
        public int CurrentIteration { get; set; }
        public int ValueSnapshotBefore { get; set; }
        public int MessageSnapshotBefore { get; set; }
        public HashSet<(string Type, int Id)>? HitStatementsBefore { get; set; }
    }
}
