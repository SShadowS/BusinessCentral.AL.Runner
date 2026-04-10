using Xunit;

namespace AlRunner.Tests;

/// <summary>
/// All in-process pipeline tests share static state (MockRecordHandle, ValueCapture, etc.)
/// and must run sequentially. Place them in the same collection to prevent parallel execution.
/// </summary>
[CollectionDefinition("Pipeline", DisableParallelization = true)]
public class PipelineCollection { }
