// The scans of production code stop at the first `#[cfg(test)]` of a file; a test module in a file of its own starts with one.
#![cfg(test)]
//! The daemon clean check (`daemon_state.rs`, al-runner issue 8180) on a planted leak per kind of state: each daemon is a
//! thread of its own (what a serve daemon keeps between requests lives on its request thread or in its `Warm`), the
//! refused test is a real one (a `Random` the runner refuses after the test wrote a row and set the language), and the
//! fresh daemon is a thread that did not run it. Without the plant both render the same.

use super::*;
use crate::daemon_state::{Cause, Check, DaemonState, differences, render};

const SRC: &str = r#"
table 50801 "DC Row" { fields { field(1; K; Integer) { } } keys { key(PK; K) { Clustered = true; } } }
codeunit 50802 "DC Refused"
{
    procedure Run() var R: Record "DC Row"; D: Decimal; I: Integer;
    begin R.K := 1; R.Insert(); GlobalLanguage(1031); D := -2147483647; D := D - 1; I := Random(D); end;
}
codeunit 50803 "DC Disabled Tests"
{
    Subtype = Test;
    RequiredTestIsolation = Disabled;
    [Test] procedure Refused() var C: Codeunit "DC Refused"; begin C.Run(); end;
}
codeunit 50804 "DC Tests"
{
    Subtype = Test;
    [Test] procedure Refused() var C: Codeunit "DC Refused"; begin C.Run(); end;
}
"#;

fn compiled() -> (Arc<Engine>, Arc<Lowered>) {
    let (ws, l) = al_sema::lower::compile_apps(
        vec![al_sema::workspace::AppSource {
            id: "dc".into(),
            name: "dc".into(),
            dependencies: vec![],
            files: vec![("dc.al".into(), SRC.into())],
        }],
        vec![al_sema::AppKind::Workspace],
        &Default::default(),
    );
    assert!(ws.diagnostics.is_empty(), "{:?}", ws.diagnostics);
    let blocked = l
        .unsupported
        .iter()
        .map(|o| o.iter().map(|r| r.first().map(|r| r.to_string())).collect())
        .collect();
    (Engine::new(l.program.clone(), blocked), Arc::new(l))
}

/// What a daemon whose request thread ran `request` keeps: `request` runs on a thread of its own, which then renders.
fn daemon(request: impl FnOnce() + Send + 'static) -> DaemonState {
    std::thread::spawn(move || {
        request();
        // The process has one run clock, which a test running beside this one may start: start it, so both daemons have it.
        crate::host::SystemClock::run_start();
        render(&Warm::new())
    })
    .join()
    .unwrap()
}

/// Runs test codeunit `name` as a request does (`run_codeunit_core`, from no baseline), under `isolation`; with `plant`,
/// a refused test is not rolled back (`stopped_state::SKIP_ROLL_BACK`). Its one test is refused.
fn run_refused(engine: &Arc<Engine>, l: &Lowered, name: &str, isolation: TestIsolation, plant: bool) {
    crate::stopped_state::SKIP_ROLL_BACK.with(|s| s.set(plant));
    let oi = l.program.objects.iter().position(|o| o.name == name).unwrap();
    let outcomes = run_codeunit_core(
        engine,
        l,
        oi,
        None,
        isolation,
        None,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Scratch::default(),
        None,
        false,
    );
    crate::stopped_state::SKIP_ROLL_BACK.with(|s| s.set(false));
    assert!(matches!(outcomes[..], [(_, Outcome::NotRun(_))]), "{outcomes:?}");
}

/// Keeps `baseline` as the install baseline of this thread (`baseline_cache::read_kept`), as a daemon keeps the baseline
/// it read for its next request.
fn keep_store(baseline: al_vm::Baseline) {
    let run = crate::baseline::BaselineRun {
        baseline,
        unseeded: Default::default(),
        filled: 0,
        notes: Vec::new(),
        ran: 0,
        total: 0,
        restored: Vec::new(),
        trace: None,
        login: Vec::new(),
        read_resources: false,
    };
    let dir = tempfile::tempdir().unwrap();
    crate::baseline_cache::write(dir.path(), "dc", &run, 0);
    assert!(crate::baseline_cache::read_kept(dir.path(), "dc", "frame", "prepared", |r| r).is_some());
}

/// The report line of one comparison of the state after a refused request with a fresh daemon's.
fn line(kept: &DaemonState, fresh: &DaemonState) -> String {
    let mut check = Check::default();
    check.note_after(Cause::Refused, &differences(None, kept, fresh));
    check.line("unit", Cause::Refused)
}

const CLEAN: &str =
    "daemon clean check (unit): after the refused request: 1 compared, 0 differ; next request: 0 compared, 0 differ";

// The store: the next request starts from the store the daemon keeps (the install baseline). A refused test runs in a
// session restored from it and leaves it as it was; the plant keeps the refused test's session, not rolled back, as it.
#[test]
fn a_kept_store_holding_the_rows_of_a_refused_test_is_reported() {
    let (engine, l) = compiled();
    let baseline = |engine: &Arc<Engine>| Session::new(engine.clone()).capture_baseline();
    let fresh = {
        let engine = engine.clone();
        daemon(move || keep_store(baseline(&engine)))
    };
    let refused = |engine: Arc<Engine>, l: Arc<Lowered>, plant: bool| {
        move || {
            let kept = baseline(&engine);
            let mut session = Session::new(engine.clone());
            session.restore_baseline(&kept);
            crate::stopped_state::SKIP_ROLL_BACK.with(|s| s.set(plant));
            let carried = session.carried_state();
            let o = l.program.objects.iter().position(|o| o.name == "DC Refused").unwrap();
            let r = l.program.objects[o]
                .routines
                .iter()
                .position(|r| r.name == "Run")
                .unwrap();
            let inst = session.instance(o);
            let res = session.call(o, r, inst, Vec::new());
            assert!(matches!(res, Err(Signal::Unsupported(_))), "{res:?}");
            crate::stopped_state::roll_back_refused_test(&mut session, carried);
            crate::stopped_state::SKIP_ROLL_BACK.with(|s| s.set(false));
            // A clean daemon keeps the store it had; the plant keeps the refused test's.
            keep_store(if plant { session.capture_baseline() } else { kept });
            drop(session);
            al_builtins::instance_cycles::collect();
        }
    };
    let clean = daemon(refused(engine.clone(), l.clone(), false));
    assert_eq!(line(&clean, &fresh), CLEAN);
    let planted = daemon(refused(engine, l, true));
    let report = line(&planted, &fresh);
    eprintln!("{report}");
    assert!(
        report.contains("1 differ (not comparable 0, answers 0, store 1,"),
        "{report}"
    );
    assert!(
        report.contains("first: store 3: kept row CRONUS International Ltd.|"),
        "{report}"
    );
}

// The language: a session the runner stopped leaves its language on the request thread, where the filters of the next
// session's code are made (al-builtins `filter::filter_language`) until a new session sets it.
#[test]
fn a_language_a_refused_test_left_on_the_request_thread_is_reported() {
    let (engine, l) = compiled();
    let fresh = daemon(|| {});
    let (e, l2) = (engine.clone(), l.clone());
    let clean = daemon(move || run_refused(&e, &l2, "DC Tests", TestIsolation::Codeunit, false));
    assert_eq!(line(&clean, &fresh), CLEAN);
    let planted = daemon(move || run_refused(&engine, &l, "DC Tests", TestIsolation::Codeunit, true));
    let report = line(&planted, &fresh);
    eprintln!("{report}");
    assert!(
        report.contains("first: language 1: filter language 1031 turkic code false | filter language 1033"),
        "{report}"
    );
}

// The per-codeunit session state: a codeunit under RequiredTestIsolation = Disabled leaves its state for the codeunits
// after it on its thread (`run::CARRIED`); a refused test of it leaves that state unfinished. No plant: a request leaves it
// on the request thread as it is.
#[test]
fn the_state_a_disabled_codeunit_with_a_refused_test_left_on_the_request_thread_is_reported() {
    let (engine, l) = compiled();
    let fresh = daemon(|| {});
    let left = daemon(move || run_refused(&engine, &l, "DC Disabled Tests", TestIsolation::Disabled, false));
    let report = line(&left, &fresh);
    eprintln!("{report}");
    assert!(report.contains("session 1,"), "{report}");
    assert!(report.contains("first: session 1: disabled carry tables"), "{report}");
}

// The clock: the instant the run clock started, set once per process (`host::SystemClock`); a daemon that started it on
// another day than a fresh daemon does is reported. The process has one clock, so the two daemons here are its lines.
#[test]
fn a_run_clock_that_started_at_another_instant_is_reported() {
    let at = |start: Option<i64>| {
        let mut s = daemon(|| {});
        s.kinds[crate::daemon_state::KINDS.iter().position(|k| *k == "clock").unwrap()] =
            crate::daemon_state::clock_lines(start, false);
        s
    };
    let day = 86_400_000;
    assert_eq!(line(&at(Some(5 * day)), &at(Some(5 * day))), CLEAN);
    let report = line(&at(Some(5 * day)), &at(Some(6 * day)));
    eprintln!("{report}");
    assert!(
        report.contains(
            "first: clock 1: run clock start Some(432000000) real clock false | run clock start Some(518400000)"
        ),
        "{report}"
    );
}

// Equal daemons compare equal, also on the answers to one request.
#[test]
fn equal_daemons_compare_equal() {
    let a = daemon(|| {});
    let b = daemon(|| {});
    let answer = vec!["{\"type\":\"summary\"}".to_string()];
    assert_eq!(differences(Some((&answer, &answer)), &a, &b), Vec::new());
    let other = vec!["{\"type\":\"summary\",\"passed\":1}".to_string()];
    assert_eq!(
        differences(Some((&answer, &other)), &a, &b),
        vec![(
            1,
            "answer line 1: {\"type\":\"summary\"} | {\"type\":\"summary\",\"passed\":1}".to_string()
        )]
    );
}

// The report line names the cause of the request it compared (issue 8334): a cut request's tallies are its own, and the
// files folder it left is named once noted.
#[test]
fn the_report_line_names_a_cut_request_as_cut() {
    let mut check = Check::default();
    check.note_after(Cause::Cut, &[]);
    check.note_next(Cause::Cut, &[]);
    assert_eq!(
        check.line("replay", Cause::Cut),
        "daemon clean check (replay): after the cut request: 1 compared, 0 differ; next request: 1 compared, 0 differ"
    );
    check.note_left(Cause::Cut, &[(0, "files 1: file cut.txt 0 1f | (none)".into())]);
    check.note_not_compared(Cause::Cut, "no request followed the cut one".into());
    assert_eq!(
        check.line("next", Cause::Cut),
        "daemon clean check (next): after the cut request: 1 compared, 0 differ; next request: 1 compared, 0 differ; \
         left outside the daemon: 1 compared, 1 differ (files 1; first: files 1: file cut.txt 0 1f | (none)); \
         not compared 1 (first: no request followed the cut one)"
    );
    // The refused requests have their own tallies.
    assert_eq!(
        check.line("next", Cause::Refused),
        "daemon clean check (next): after the refused request: 0 compared, 0 differ; next request: 0 compared, 0 differ"
    );
}

// Without a files folder, a refused request's line is the line of issue 8180, byte for byte.
#[test]
fn a_refused_request_keeps_its_report_line() {
    let mut check = Check::default();
    check.note_after(Cause::Refused, &[]);
    check.note_next(Cause::Refused, &[]);
    check.note_after(Cause::Cut, &[(2, "store 1: a | b".into())]);
    assert_eq!(
        check.line("replay", Cause::Refused),
        "daemon clean check (replay): after the refused request: 1 compared, 0 differ; next request: 1 compared, 0 differ"
    );
    check.note_not_compared(Cause::Refused, "no request followed the refused one".into());
    assert_eq!(
        check.line("replay", Cause::Refused),
        "daemon clean check (replay): after the refused request: 1 compared, 0 differ; next request: 1 compared, 0 \
         differ; not compared 1 (first: no request followed the refused one)"
    );
}

// A helper process (a .NET sidecar or the render process) a request left alive is reported in `process`. The process
// holds one count, so the two daemons here are its lines, as for the clock.
#[test]
fn a_helper_process_left_alive_is_reported() {
    let at = |alive: usize| {
        let mut s = daemon(|| {});
        s.kinds[crate::daemon_state::KINDS.iter().position(|k| *k == "process").unwrap()] =
            crate::daemon_state::helper_lines(alive);
        s
    };
    assert_eq!(line(&at(0), &at(0)), CLEAN);
    let report = line(&at(1), &at(0));
    eprintln!("{report}");
    assert!(report.contains("process 1,"), "{report}");
    assert!(
        report.contains("first: process 1: helper processes alive 1 | helper processes alive 0"),
        "{report}"
    );
}
