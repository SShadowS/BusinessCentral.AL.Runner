using System.Text.Json;
using Xunit;

namespace AlRunner.Tests;

public class IncrementalTests
{
    private static readonly string RepoRoot = Path.GetFullPath(
        Path.Combine(AppContext.BaseDirectory, "..", "..", "..", ".."));

    private static string TestPath(string testCase, string sub) =>
        Path.Combine(RepoRoot, "tests", testCase, sub);

    [Fact]
    public async Task Server_SecondRun_SameFiles_UsesCachedAssembly()
    {
        await using var server = await CliServer.StartAsync();

        var request = JsonSerializer.Serialize(new
        {
            command = "runTests",
            sourcePaths = new[] { TestPath("01-pure-function", "src"), TestPath("01-pure-function", "test") }
        });

        // First run — cold
        var response1 = await server.SendAsync(request);
        var doc1 = JsonDocument.Parse(response1);
        Assert.Equal(6, doc1.RootElement.GetProperty("passed").GetInt32());
        Assert.False(doc1.RootElement.TryGetProperty("cached", out var c1) && c1.GetBoolean());

        // Second run — same paths, should report cache hit
        var response2 = await server.SendAsync(request);
        var doc2 = JsonDocument.Parse(response2);
        Assert.Equal(6, doc2.RootElement.GetProperty("passed").GetInt32());
        Assert.True(doc2.RootElement.TryGetProperty("cached", out var c2) && c2.GetBoolean(),
            "Expected second run to report cached=true");
    }

    [Fact]
    public async Task Server_DifferentFiles_DoesNotUseStalCache()
    {
        await using var server = await CliServer.StartAsync();

        // Run test case 01
        var request1 = JsonSerializer.Serialize(new
        {
            command = "runTests",
            sourcePaths = new[] { TestPath("01-pure-function", "src"), TestPath("01-pure-function", "test") }
        });
        var response1 = await server.SendAsync(request1);
        var doc1 = JsonDocument.Parse(response1);
        Assert.Equal(6, doc1.RootElement.GetProperty("passed").GetInt32());

        // Run test case 04 — different files, must not return cached results
        var request2 = JsonSerializer.Serialize(new
        {
            command = "runTests",
            sourcePaths = new[] { TestPath("04-asserterror", "src"), TestPath("04-asserterror", "test") }
        });
        var response2 = await server.SendAsync(request2);
        var doc2 = JsonDocument.Parse(response2);
        Assert.Equal(7, doc2.RootElement.GetProperty("passed").GetInt32());
    }
}
