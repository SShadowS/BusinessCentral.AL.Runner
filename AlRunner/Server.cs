using System.Reflection;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace AlRunner;

/// <summary>
/// Long-running server mode. Reads JSON requests from stdin, writes JSON responses to stdout.
/// Keeps the transpiler and references warm between invocations.
/// Protocol: one JSON object per line (newline-delimited JSON).
/// </summary>
public class AlRunnerServer
{
    private readonly CompilationCache _cache = new();

    public async Task RunAsync(TextReader input, TextWriter output, CancellationToken ct = default)
    {
        // Pre-warm: load Roslyn references once
        var refsTask = Task.Run(() => RoslynCompiler.LoadReferences(), ct);
        Kernel32Shim.EnsureRegistered();

        // Signal readiness
        await output.WriteLineAsync("{\"ready\":true}");
        await output.FlushAsync();

        while (!ct.IsCancellationRequested)
        {
            var line = await input.ReadLineAsync(ct);
            if (line == null) break; // EOF — client disconnected

            string response;
            try
            {
                var request = JsonSerializer.Deserialize<ServerRequest>(line);
                if (request == null)
                {
                    response = JsonSerializer.Serialize(new { error = "Invalid request" });
                }
                else
                {
                    response = request.Command?.ToLowerInvariant() switch
                    {
                        "runtests" => HandleRunTests(request, refsTask),
                        "shutdown" => HandleShutdown(),
                        _ => JsonSerializer.Serialize(new { error = $"Unknown command: {request.Command}" })
                    };

                    if (request.Command?.ToLowerInvariant() == "shutdown")
                    {
                        await output.WriteLineAsync(response);
                        await output.FlushAsync();
                        break;
                    }
                }
            }
            catch (Exception ex)
            {
                response = JsonSerializer.Serialize(new { error = ex.Message });
            }

            await output.WriteLineAsync(response);
            await output.FlushAsync();
        }
    }

    private string HandleRunTests(ServerRequest request, Task<List<Microsoft.CodeAnalysis.MetadataReference>> refsTask)
    {
        if (request.SourcePaths == null || request.SourcePaths.Length == 0)
            return JsonSerializer.Serialize(new { error = "sourcePaths is required" });

        // Compute hash of all source files to check cache
        var sourceHash = _cache.ComputeHash(request.SourcePaths);
        var cachedAssembly = _cache.TryGet(sourceHash);

        if (cachedAssembly != null)
        {
            // Cache hit — just re-run tests on the cached assembly
            Runtime.MockCodeunitHandle.CurrentAssembly = cachedAssembly;
            var results = Executor.RunTests(cachedAssembly);
            return SerializeServerResponse(results, Executor.ExitCode(results), cached: true);
        }

        // Cache miss — full pipeline
        var options = new PipelineOptions { OutputJson = true };
        options.InputPaths.AddRange(request.SourcePaths);
        if (request.PackagePaths != null)
            options.PackagePaths.AddRange(request.PackagePaths);
        if (request.StubPaths != null)
            options.StubPaths.AddRange(request.StubPaths);

        var pipeline = new AlRunnerPipeline();
        var result = pipeline.Run(options);

        // Cache the compiled assembly if available
        if (result.ExitCode == 0 || result.Tests.Count > 0)
        {
            var assembly = Runtime.MockCodeunitHandle.CurrentAssembly;
            if (assembly != null)
                _cache.Store(sourceHash, assembly);
        }

        return SerializeServerResponse(result.Tests, result.ExitCode, cached: false);
    }

    private static string SerializeServerResponse(List<TestResult> tests, int exitCode, bool cached)
    {
        var output = new
        {
            tests = tests.Select(t => new
            {
                name = t.Name,
                status = t.Status.ToString().ToLowerInvariant(),
                durationMs = t.DurationMs,
                message = t.Message,
                stackTrace = t.StackTrace?.TrimEnd()
            }),
            passed = tests.Count(t => t.Status == TestStatus.Pass),
            failed = tests.Count(t => t.Status == TestStatus.Fail),
            errors = tests.Count(t => t.Status == TestStatus.Error),
            total = tests.Count,
            exitCode,
            cached
        };

        return JsonSerializer.Serialize(output, new JsonSerializerOptions
        {
            DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull
        });
    }

    private static string HandleShutdown()
    {
        return JsonSerializer.Serialize(new { status = "shutting down" });
    }
}

/// <summary>
/// Caches compiled assemblies keyed by a hash of all source file contents.
/// </summary>
public class CompilationCache
{
    private string? _lastHash;
    private Assembly? _lastAssembly;

    public string ComputeHash(string[] sourcePaths)
    {
        using var sha = SHA256.Create();
        var allFiles = new List<string>();
        foreach (var path in sourcePaths)
        {
            if (Directory.Exists(path))
                allFiles.AddRange(Directory.GetFiles(path, "*.al", SearchOption.AllDirectories).OrderBy(f => f));
            else if (File.Exists(path))
                allFiles.Add(path);
        }

        foreach (var file in allFiles)
        {
            var bytes = File.ReadAllBytes(file);
            sha.TransformBlock(bytes, 0, bytes.Length, bytes, 0);
        }
        sha.TransformFinalBlock(Array.Empty<byte>(), 0, 0);

        return Convert.ToHexString(sha.Hash!);
    }

    public Assembly? TryGet(string hash)
    {
        if (_lastHash == hash && _lastAssembly != null)
            return _lastAssembly;
        return null;
    }

    public void Store(string hash, Assembly assembly)
    {
        _lastHash = hash;
        _lastAssembly = assembly;
    }
}

public class ServerRequest
{
    [JsonPropertyName("command")]
    public string? Command { get; set; }

    [JsonPropertyName("sourcePaths")]
    public string[]? SourcePaths { get; set; }

    [JsonPropertyName("packagePaths")]
    public string[]? PackagePaths { get; set; }

    [JsonPropertyName("stubPaths")]
    public string[]? StubPaths { get; set; }
}
