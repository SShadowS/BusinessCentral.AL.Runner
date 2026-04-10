using Xunit;

namespace AlRunner.Tests;

public class IntegrationTests
{
    [Fact]
    public async Task PureFunctionTests_Pass()
    {
        var result = await CliRunner.RunTestCaseAsync("01-pure-function");

        Assert.Equal(0, result.ExitCode);
        Assert.Contains("PASS", result.StdOut);
        Assert.Contains("passed", result.StdOut);
    }

    [Fact]
    public async Task RecordOperationTests_Pass()
    {
        var result = await CliRunner.RunTestCaseAsync("02-record-operations");

        Assert.Equal(0, result.ExitCode);
        Assert.Contains("PASS", result.StdOut);
    }

    [Fact]
    public async Task IntentionalFailure_ReturnsNonZero()
    {
        var result = await CliRunner.RunTestCaseAsync("06-intentional-failure");

        Assert.NotEqual(0, result.ExitCode);
        Assert.Contains("FAIL", result.StdOut);
    }

    [Fact]
    public async Task HelpFlag_PrintsUsage()
    {
        var result = await CliRunner.RunAsync("--help");

        Assert.Equal(0, result.ExitCode);
        Assert.Contains("Usage:", result.StdErr);
    }

    [Fact]
    public async Task NoArgs_ReturnsNonZero()
    {
        var result = await CliRunner.RunAsync("");

        Assert.Equal(1, result.ExitCode);
    }

    [Fact]
    public async Task CoverageFlag_PrintsCoverageReport()
    {
        var result = await CliRunner.RunTestCaseAsync("01-pure-function", "--coverage");

        Assert.Equal(0, result.ExitCode);
        Assert.Contains("Coverage:", result.StdOut);
    }
}
