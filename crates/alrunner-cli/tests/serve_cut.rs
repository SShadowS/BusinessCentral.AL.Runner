//! A serve daemon after a request the test time limit cut (upstream issue 8334), with `ALRUNNER_DAEMON_CLEAN_CHECK` on
//! (PERF-09's check, extended by PERF-15 to name the cause of the request it compares and to list what a request left
//! in the `--files-dir` folder and how many helper processes are alive).
//!
//! The runner is compared with itself, so no corpus case pins these lines. Every cutter is invented AL: it writes
//! the state of PERF-09's refusers first (a row, a SingleInstance text, isolated storage, WorkDate, the language), then
//! enters `repeat <step> until false` under `testTimeoutSecs: 1`. A cut lands only at a loop head (`Op::CheckStop`),
//! so the test ends after at least one whole step with that step's effect in place, whatever the machine's speed.
//! Each test asserts that the cutter's `test` line is `notRun` with a `clock`, so a refusal for another reason cannot
//! pass as a cut. Sequence per place: reader, cutter, reader.

mod support;

use serde_json::{Value, json};
use std::process::Stdio;
use support::server::{Server, app};

/// The state a cutter writes first and the reader prints; shared by every place.
const STATE: &str = r#"
table 50880 "Cut Row" { fields { field(1; K; Integer) { } } keys { key(PK; K) { Clustered = true; } } }
codeunit 50881 "Cut Single"
{
    SingleInstance = true;
    var X: Text; L: List of [Text];
    procedure Set(V: Text) begin X := V; end;
    procedure Add(V: Text) begin X += V; L.Add(V); end;
    procedure Get(): Text begin exit(StrSubstNo('%1 items=%2', X, L.Count())); end;
}
codeunit 50882 "Cut State"
{
    procedure Write() var R: Record "Cut Row"; S: Codeunit "Cut Single";
    begin R.K := 7; R.Insert(); S.Set('leak'); IsolatedStorage.Set('k', 'leak'); WorkDate(20200102D); GlobalLanguage(1031); end;
    procedure Read(): Text var R: Record "Cut Row"; S: Codeunit "Cut Single"; V: Text;
    begin
        if not IsolatedStorage.Get('k', V) then V := '(none)';
        exit(StrSubstNo('rows=%1 single=%2 storage=%3 workdate=%4 lang=%5', R.Count(), S.Get(), V, WorkDate(), GlobalLanguage()));
    end;
}
codeunit 50883 "Cut Reader" { Subtype = Test; [Test] procedure Read() var C: Codeunit "Cut State"; begin Error(C.Read()); end; }
"#;

/// Places 1 to 4 and 6 (the default target): the step in the loop is named by the codeunit.
const CUTCHECK: &str = r#"
// 1: a write, the transaction stays open
codeunit 50884 "Cut Write" { Subtype = Test; [Test] procedure Cut() var C: Codeunit "Cut State"; R: Record "Cut Row"; begin C.Write(); R.K := 1000; repeat R.K += 1; R.Insert(); until false; end; }
// 2: between commits
codeunit 50885 "Cut Commits" { Subtype = Test; [Test] procedure Cut() var C: Codeunit "Cut State"; R: Record "Cut Row"; begin C.Write(); R.K := 1000; repeat R.K += 1; R.Insert(); Commit(); until false; end; }
// 3: work of a SingleInstance codeunit holding a Text and a List
codeunit 50886 "Cut Singles" { Subtype = Test; [Test] procedure Cut() var C: Codeunit "Cut State"; S: Codeunit "Cut Single"; begin C.Write(); repeat S.Add('x'); until false; end; }
// 4: the loop of 1 in the OnRun trigger of the test codeunit
codeunit 50887 "Cut OnRun" { Subtype = Test; trigger OnRun() var C: Codeunit "Cut State"; R: Record "Cut Row"; begin C.Write(); R.K := 1000; repeat R.K += 1; R.Insert(); until false; end; [Test] procedure Quiet() begin end; }
// 6: a file created and written, never closed
codeunit 50888 "Cut File" { Subtype = Test; [Test] procedure Cut() var C: Codeunit "Cut State"; R: Record "Cut Row"; F: File; begin C.Write(); F.Create('cut.txt'); F.TextMode(true); F.Write('x'); R.K := 1000; repeat R.K += 1; R.Insert(); until false; end; }
"#;

/// Place 5 (BC 28 target, the .NET 8 sidecar): a DotNet StringBuilder held in a SingleInstance codeunit.
const CUTDOTNET: &str = r#"
dotnet { assembly(mscorlib) { type(System.Text.StringBuilder; SB) { } } }
codeunit 50889 "Cut Sidecar Single"
{
    SingleInstance = true;
    var B: DotNet SB;
    procedure Append(T: Text) begin if IsNull(B) then B := B.StringBuilder(); B.Append(T); end;
    procedure Get(): Text begin if IsNull(B) then exit('(null)'); exit(B.ToString()); end;
}
codeunit 50890 "Cut Dotnet" { Subtype = Test; [Test] procedure Cut() var C: Codeunit "Cut State"; S: Codeunit "Cut Sidecar Single"; begin C.Write(); repeat S.Append('x'); until false; end; }
codeunit 50891 "Cut Dotnet Reader" { Subtype = Test; [Test] procedure Read() var C: Codeunit "Cut State"; S: Codeunit "Cut Sidecar Single"; begin Error('%1 sb=%2', C.Read(), S.Get()); end; }
"#;

/// Place 7: a report over the state's table (the runner refuses a dataitem over Integer outside the workspace), saved as Xml into a Blob, in the loop.
const CUTREPORT: &str = r#"
table 50892 "Cut Blob" { fields { field(1; K; Integer) { } field(2; B; Blob) { } } keys { key(PK; K) { Clustered = true; } } }
report 50893 "Cut Report"
{
    UseRequestPage = false;
    dataset { dataitem(Row; "Cut Row") { column(Num; K) { } } }
}
codeunit 50894 "Cut Report Save" { Subtype = Test; [Test] procedure Cut() var C: Codeunit "Cut State"; T: Record "Cut Blob"; O: OutStream; begin C.Write(); repeat T.B.CreateOutStream(O); Report.SaveAs(Report::"Cut Report", '', ReportFormat::Xml, O); until false; end; }
"#;

/// What one place measured: the check's lines on stderr, and the answers to the three requests.
struct Cut {
    lines: Vec<String>,
    answers: Vec<Vec<Value>>,
}

impl Cut {
    /// The `test` line of the cutter, the second request's only one.
    fn cutter(&self) -> &Value {
        self.answers[1]
            .iter()
            .find(|l| l["type"] == "test")
            .unwrap_or_else(|| panic!("the cutter's request has no test line: {:?}", self.answers[1]))
    }

    /// The text the reader failed with before and after the cutter: the state the daemon showed.
    fn reader(&self, request: usize) -> String {
        self.answers[request]
            .iter()
            .find(|l| l["type"] == "test")
            .and_then(|l| l["message"].as_str())
            .unwrap_or_else(|| panic!("no reader message: {:?}", self.answers[request]))
            .to_string()
    }

    /// The cutter was cut by the time limit: not run, with a clock, and the readers saw the same state.
    fn assert_cut_and_readers_agree(&self) {
        let c = self.cutter();
        assert_eq!(c["notRun"], true, "the cutter was not cut: {c}");
        assert!(
            c.get("clock").is_some(),
            "the cutter has no clock key, so it was not the time limit: {c}"
        );
        assert_eq!(self.reader(0), self.reader(2));
    }
}

/// Runs the reader (`Codeunit50883.`), the cutter (`cutter`, with `testTimeoutSecs: 1`) and the reader again on one
/// daemon with `ALRUNNER_DAEMON_CLEAN_CHECK=mode`. `extra` are the daemon's options after `--no-download`.
fn cut_check(mode: &str, sources: &[(&str, &str)], reader: &str, cutter: &str, extra: &[&str]) -> Cut {
    let d = tempfile::tempdir().unwrap();
    let mut paths = Vec::new();
    for (name, al) in sources {
        paths.push(app(d.path(), name, &format!("{STATE}{al}")));
    }
    let log = d.path().join("stderr.txt");
    let mut args = vec!["--no-download", "--no-test-data"];
    args.extend_from_slice(extra);
    let mut s = Server::start_with_stderr(
        &args,
        &[("ALRUNNER_DAEMON_CLEAN_CHECK", mode)],
        Stdio::from(std::fs::File::create(&log).unwrap()),
    );
    assert_eq!(s.line(), json!({"ready": true}), "the daemon writes no state line");
    let mut answers = Vec::new();
    for (test, secs) in [(reader, None), (cutter, Some(1)), (reader, None)] {
        let mut request = json!({"command": "runTests", "sourcePaths": paths, "test": test});
        if let Some(secs) = secs {
            request["testTimeoutSecs"] = json!(secs);
        }
        s.send(request);
        answers.push(s.until_summary());
    }
    s.send(json!({"command": "shutdown"}));
    assert_eq!(s.line()["status"], "shutting down");
    s.child.wait().unwrap();
    let text = std::fs::read_to_string(&log).unwrap();
    let lines = text
        .lines()
        .filter(|l| l.starts_with("daemon clean check"))
        .map(String::from)
        .collect();
    Cut { lines, answers }
}

/// The check on `cutter` of `CUTCHECK`, in the default target.
fn on_cutcheck(mode: &str, cutter: &str, extra: &[&str]) -> Cut {
    cut_check(mode, &[("cutcheck", CUTCHECK)], "Codeunit50883.", cutter, extra)
}

fn clean(mode: &str, after: usize, next: usize) -> String {
    format!(
        "daemon clean check ({mode}): after the cut request: {after} compared, 0 differ; next request: {next} compared, 0 differ"
    )
}

/// Place 1: a write with the transaction open. The cut rolls the rows back (`stopped_state.rs`), so the daemon keeps
/// what it kept: the check names the request as cut, and the next request is clean in both modes.
#[test]
fn the_daemon_clean_check_names_a_request_the_time_limit_cut() {
    let cut = on_cutcheck("1", "Codeunit50884.", &[]);
    cut.assert_cut_and_readers_agree();
    assert_eq!(cut.lines, vec![clean("replay", 1, 1)], "{:?}", cut.answers);
    let cut = on_cutcheck("next", "Codeunit50884.", &[]);
    cut.assert_cut_and_readers_agree();
    assert_eq!(cut.lines, vec![clean("next", 0, 1)], "{:?}", cut.answers);
}

/// Place 2: committed rows between two statements of the loop belong to the session's copy of the baseline.
#[test]
fn a_daemon_is_clean_after_a_cut_between_commits() {
    let cut = on_cutcheck("1", "Codeunit50885.", &[]);
    cut.assert_cut_and_readers_agree();
    assert_eq!(cut.lines, vec![clean("replay", 1, 1)], "{:?}", cut.answers);
}

/// Place 3: a SingleInstance codeunit's Text and List live in the session, which the cut drops.
#[test]
fn a_daemon_is_clean_after_a_cut_in_a_single_instance_codeunit() {
    let cut = on_cutcheck("1", "Codeunit50886.", &[]);
    cut.assert_cut_and_readers_agree();
    assert_eq!(cut.lines, vec![clean("replay", 1, 1)], "{:?}", cut.answers);
}

/// Place 4: the loop in the OnRun trigger of the test codeunit (a time limit of its own), every test gets the row.
#[test]
fn a_daemon_is_clean_after_a_cut_in_an_onrun_trigger() {
    let cut = on_cutcheck("1", "Codeunit50887.", &[]);
    cut.assert_cut_and_readers_agree();
    assert_eq!(cut.lines, vec![clean("replay", 1, 1)], "{:?}", cut.answers);
}

/// Place 5: a DotNet handle live in the session's sidecar. Needs the .NET 8 sidecar (at `/opt/dotnet` here) and a
/// BC 28 target (BC 29 needs .NET 10). Run with `--ignored`. Neither daemon has a helper process alive afterwards.
#[test]
#[ignore = "needs the .NET 8 sidecar and a BC 28 target"]
fn a_daemon_is_clean_after_a_cut_around_a_dotnet_call() {
    let cut = cut_check(
        "1",
        &[("cutdotnet", CUTDOTNET)],
        "Codeunit50891.",
        "Codeunit50890.",
        &["--bc-version", "28"],
    );
    cut.assert_cut_and_readers_agree();
    assert!(cut.reader(0).ends_with("sb=(null)"), "{}", cut.reader(0));
    assert_eq!(cut.reader(0), cut.reader(2));
    assert_eq!(cut.lines, vec![clean("replay", 1, 1)], "{:?}", cut.answers);
}

/// Place 6: `File.Create` writes an empty file at once (al-builtins `file.rs:167`) and the content only at `Close`,
/// so a cut between them leaves an empty `cut.txt` in the `--files-dir` folder. The folder is the client's: the fresh
/// daemon has the same one, so only the before-against-after comparison on this daemon sees it.
#[test]
fn a_cut_during_a_file_write_leaves_the_file_in_the_folder_and_the_check_names_it() {
    let files = tempfile::tempdir().unwrap();
    let dir = files.path().to_str().unwrap();
    let cut = on_cutcheck("1", "Codeunit50888.", &["--files-dir", dir]);
    cut.assert_cut_and_readers_agree();
    assert!(files.path().join("cut.txt").exists(), "the file the cut left");
    assert_eq!(cut.lines.len(), 1, "{:?}", cut.lines);
    let line = &cut.lines[0];
    assert!(
        line.starts_with(
            "daemon clean check (replay): after the cut request: 1 compared, 0 differ; next request: 1 compared, 0 differ; \
             left outside the daemon: 1 compared, 1 differ ("
        ),
        "{line}"
    );
    assert!(line.contains("files 1; first: files 1: file cut.txt 0 "), "{line}");
}

/// Place 7: a report saved as Xml in the loop. MEASURED, not predicted (see the write-up
/// `/coord/handoff/PERF-15/timeout-clean.md`): the line below is what the run gave.
#[test]
fn a_daemon_after_a_cut_during_a_report_save_is_compared() {
    let cut = cut_check(
        "1",
        &[("cutreport", CUTREPORT)],
        "Codeunit50883.",
        "Codeunit50894.",
        &[],
    );
    cut.assert_cut_and_readers_agree();
    assert_eq!(cut.lines, vec![clean("replay", 1, 1)], "{:?}", cut.answers);
}
