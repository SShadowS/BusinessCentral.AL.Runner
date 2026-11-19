//! The daemon clean check of `al-runner serve` (al-runner issue 8180), internal: `ALRUNNER_DAEMON_CLEAN_CHECK=1` (or
//! `replay`, or `next`). It measures; no answer and no kept state changes.
//!
//! After a `runTests` request with a refused test (`notRun` above 0 in its summary) or a test the time limit cut (a `test`
//! line with a `clock`, issue 8334; [`cause`]), the daemon waits for the next request. It answers that one as always,
//! then starts a fresh daemon (this program, `serve`, the same options) and feeds it:
//! - with `1` or `replay`: every earlier request that had no refused or cut test, each with the seed this daemon gave it;
//!   the fresh daemon's state after them is compared with this daemon's state right after the refused or cut request
//!   (when there was one: a daemon that ran nothing has kept nothing yet);
//! - with `next`: nothing (as a client that restarts the daemon after a refusal or a cut);
//!
//! then the next request with this daemon's seed. Both daemons' answer lines to it (without times and what says whether
//! the compile was kept) and their state after it are compared. The state is al-testrun `daemon_state::render` plus the
//! front end's (`WarmCtx::kept_lines`); the fresh daemon runs with `ALRUNNER_DAEMON_CLEAN_CHECK=twin` and writes its state
//! as a `daemonState` line after the `ready` line and after each request. The report line goes to stderr after each
//! comparison (`daemon_state::Check::line`), for the cause of the request compared.

use crate::run::WarmCtx;
use al_testrun::daemon_state::{self, Cause, DaemonState};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;

/// The answer lines of the request in progress, while the check is on.
static ANSWER: Mutex<Option<Vec<String>>> = Mutex::new(None);

/// The keys of an answer line that differ between two runs of one request on equal daemons: the times, and whether the
/// compile of an earlier request was used or updated, and where the seed came from (the fresh daemon is given it).
const NOT_COMPARED: [&str; 6] = [
    "durationMs",
    "wallSeconds",
    "timings",
    "cached",
    "incremental",
    "seedSource",
];

/// Which requests the fresh daemon runs before the next request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Every earlier request without a refused test.
    Replay,
    /// None.
    Next,
    /// This is the fresh daemon: it writes its state after each request.
    Twin,
}

/// The check of one daemon (see the module comment).
pub struct DaemonClean {
    mode: Mode,
    /// The daemon's own options, for the fresh daemon.
    daemon: Vec<String>,
    /// The earlier requests without a refused or cut test, each with the seed this daemon gave it.
    log: Vec<Value>,
    /// The cause of the last refused or cut request and this daemon's state right after it, while no later request was
    /// compared.
    pending: Option<(Cause, DaemonState)>,
    check: daemon_state::Check,
}

impl DaemonClean {
    /// The check `ALRUNNER_DAEMON_CLEAN_CHECK` asks for (`value`), `None` when it is not set; `daemon`: the daemon's options.
    pub fn of(value: Option<&str>, daemon: &[String]) -> Result<Option<DaemonClean>, String> {
        let mode = match value {
            None => return Ok(None),
            Some("1" | "replay") => Mode::Replay,
            Some("next") => Mode::Next,
            Some("twin") => Mode::Twin,
            Some(v) => return Err(format!("ALRUNNER_DAEMON_CLEAN_CHECK is 1, replay or next, not `{v}`")),
        };
        *ANSWER.lock().unwrap_or_else(|e| e.into_inner()) = Some(Vec::new());
        Ok(Some(DaemonClean {
            mode,
            daemon: daemon.to_vec(),
            log: Vec::new(),
            pending: None,
            check: Default::default(),
        }))
    }

    /// The daemon is ready: the fresh daemon writes the state it starts with.
    pub fn ready(&self, ctx: &WarmCtx) {
        if self.mode == Mode::Twin {
            crate::serve::out(&state_json(&state(ctx)));
        }
        take_answer();
    }

    /// The daemon answered `request` (one line of stdin): compare, or keep what the next comparison needs.
    pub fn after_request(&mut self, request: &Value, ctx: &WarmCtx) {
        let answer = take_answer();
        let now = state(ctx);
        if self.mode == Mode::Twin {
            crate::serve::out(&state_json(&now));
            return;
        }
        let summary = answer.iter().rev().find_map(|l| {
            let v: Value = serde_json::from_str(l).ok()?;
            (v["type"] == "summary").then_some(v)
        });
        let seeded = with_seed(request, summary.as_ref());
        if let Some((cause, after)) = self.pending.take() {
            self.compare(cause, &after, &seeded, &answer, &now);
        }
        match cause(&answer, summary.as_ref()) {
            Some(c) => self.pending = Some((c, now)),
            None => self.log.push(seeded),
        }
    }

    /// The end of input or `shutdown`: a refused or cut request no later request followed is named as not compared.
    pub fn end(&mut self) {
        if let Some((cause, _)) = self.pending.take() {
            self.check
                .note_not_compared(cause, format!("no request followed the {} one", cause.word()));
            eprintln!("{}", self.check.line(self.mode_text(), cause));
        }
    }

    fn mode_text(&self) -> &'static str {
        match self.mode {
            Mode::Replay => "replay",
            Mode::Next => "next",
            Mode::Twin => "twin",
        }
    }

    /// Runs the fresh daemon over the replayed requests and `next`, and notes what differs.
    fn compare(&mut self, cause: Cause, after: &DaemonState, next: &Value, answer: &[String], now: &DaemonState) {
        let replay: &[Value] = if self.mode == Mode::Replay { &self.log } else { &[] };
        match Fresh::run(&self.daemon, replay, next) {
            Err(why) => self.check.note_not_compared(cause, why),
            Ok(fresh) => {
                // A fresh daemon that ran nothing kept nothing yet: only a replay gives a state to compare with.
                if !replay.is_empty() {
                    self.check
                        .note_after(cause, &daemon_state::differences(None, after, &fresh.before_next));
                }
                let answers = Some((answer, fresh.answer.as_slice()));
                self.check
                    .note_next(cause, &daemon_state::differences(answers, now, &fresh.after_next));
            }
        }
        eprintln!("{}", self.check.line(self.mode_text(), cause));
    }
}

/// Why the request with answer lines `answer` and summary `summary` is compared with a fresh daemon (issue 8334): `Cut`
/// when one of its `test` lines is a test the time limit stopped (`notRun` and a `clock`, output.rs `json_row`), else
/// `Refused` when its summary counts a test not run (`notRun` above 0, issue 8180); `None` otherwise.
fn cause(answer: &[String], summary: Option<&Value>) -> Option<Cause> {
    let cut = answer.iter().any(|l| {
        serde_json::from_str::<Value>(l)
            .is_ok_and(|v| v["type"] == "test" && v["notRun"] == true && v["clock"].is_string())
    });
    let refused = summary.and_then(|s| s["notRun"].as_u64()).unwrap_or(0) > 0;
    match (cut, refused) {
        (true, _) => Some(Cause::Cut),
        (false, true) => Some(Cause::Refused),
        (false, false) => None,
    }
}

/// Notes `line`, written to stdout, as part of the answer in progress (only while the check is on).
pub fn record(line: &Value) {
    if let Some(a) = ANSWER.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        a.push(comparable(line));
    }
}

fn take_answer() -> Vec<String> {
    ANSWER
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
        .map(std::mem::take)
        .unwrap_or_default()
}

/// `line` without the keys of [`NOT_COMPARED`], also in an entry summary's object.
fn comparable(line: &Value) -> String {
    let mut v = line.clone();
    if let Some(o) = v.as_object_mut() {
        for key in NOT_COMPARED {
            o.remove(key);
        }
    }
    v.to_string()
}

/// `request` with the seed the daemon gave it (the `seed` of its summary), so another daemon runs it alike.
fn with_seed(request: &Value, summary: Option<&Value>) -> Value {
    let mut r = request.clone();
    if let (Some(o), Some(seed)) = (r.as_object_mut(), summary.and_then(|s| s.get("seed"))) {
        o.insert("seed".into(), seed.clone());
    }
    r
}

/// This daemon's kept state: al-testrun's and the front end's.
fn state(ctx: &WarmCtx) -> DaemonState {
    let mut s = daemon_state::render(&ctx.runtime);
    s.kinds[daemon_state::FRONT_END] = ctx.kept_lines();
    s
}

fn state_json(s: &DaemonState) -> Value {
    json!({"type": "daemonState", "kinds": s.kinds, "opaque": s.opaque})
}

fn state_of(v: &Value) -> Option<DaemonState> {
    let strings =
        |v: &Value| -> Option<Vec<String>> { v.as_array()?.iter().map(|s| s.as_str().map(String::from)).collect() };
    Some(DaemonState {
        kinds: v["kinds"].as_array()?.iter().map(strings).collect::<Option<_>>()?,
        opaque: strings(&v["opaque"])?,
    })
}

/// What the fresh daemon answered and kept.
struct Fresh {
    /// Its state after the replayed requests, before the next one.
    before_next: DaemonState,
    /// Its answer lines to the next request.
    answer: Vec<String>,
    after_next: DaemonState,
}

/// A fresh daemon (`serve` with the same options), fed one request at a time: stdin stays open until the last answer.
struct Twin {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl Fresh {
    fn run(daemon: &[String], replay: &[Value], next: &Value) -> Result<Fresh, String> {
        let mut t = Twin::start(daemon)?;
        let fresh = (|| {
            let ready = t.line()?;
            if ready.get("ready").is_none() {
                return Err(format!("the fresh daemon's first line is not `ready`: {ready}"));
            }
            let mut before_next = t.state()?;
            for r in replay {
                t.answer(r)?;
                before_next = t.state()?;
            }
            let answer = t.answer(next)?;
            let after_next = t.state()?;
            Ok(Fresh {
                before_next,
                answer,
                after_next,
            })
        })();
        t.stop();
        fresh
    }
}

impl Twin {
    fn start(daemon: &[String]) -> Result<Twin, String> {
        let exe = std::env::current_exe().map_err(|e| format!("the fresh daemon: {e}"))?;
        let mut child = Command::new(exe)
            .arg("serve")
            .args(daemon)
            .env("ALRUNNER_DAEMON_CLEAN_CHECK", "twin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("the fresh daemon did not start: {e}"))?;
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().ok_or("the fresh daemon has no stdout")?);
        Ok(Twin { child, stdin, stdout })
    }

    fn line(&mut self) -> Result<Value, String> {
        let mut s = String::new();
        match self.stdout.read_line(&mut s) {
            Ok(n) if n > 0 => {
                serde_json::from_str(&s).map_err(|e| format!("the fresh daemon wrote `{}`: {e}", s.trim()))
            }
            _ => Err("the fresh daemon closed its output".into()),
        }
    }

    /// Sends `request` and reads its answer lines up to the summary or an error.
    fn answer(&mut self, request: &Value) -> Result<Vec<String>, String> {
        let stdin = self.stdin.as_mut().ok_or("the fresh daemon's input is closed")?;
        writeln!(stdin, "{request}")
            .and_then(|_| stdin.flush())
            .map_err(|e| format!("the fresh daemon: {e}"))?;
        let mut out = Vec::new();
        loop {
            let l = self.line()?;
            out.push(comparable(&l));
            if l["type"] == "summary" || l.get("error").is_some() {
                return Ok(out);
            }
        }
    }

    /// The `daemonState` line that follows each answer.
    fn state(&mut self) -> Result<DaemonState, String> {
        let l = self.line()?;
        (l["type"] == "daemonState")
            .then(|| state_of(&l))
            .flatten()
            .ok_or_else(|| format!("the fresh daemon wrote no state: {l}"))
    }

    fn stop(mut self) {
        if let Some(mut stdin) = self.stdin.take() {
            let _ = writeln!(stdin, "{}", json!({"command": "shutdown"}));
            let _ = stdin.flush();
        }
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_names_a_mode_or_is_refused() {
        assert!(DaemonClean::of(None, &[]).unwrap().is_none());
        assert_eq!(DaemonClean::of(Some("1"), &[]).unwrap().unwrap().mode, Mode::Replay);
        assert_eq!(DaemonClean::of(Some("next"), &[]).unwrap().unwrap().mode, Mode::Next);
        assert_eq!(
            DaemonClean::of(Some("yes"), &[]).err().as_deref(),
            Some("ALRUNNER_DAEMON_CLEAN_CHECK is 1, replay or next, not `yes`")
        );
    }

    #[test]
    fn an_answer_line_is_compared_without_its_times_and_the_kept_compile() {
        let line = json!({"type": "summary", "passed": 1, "wallSeconds": 0.5, "timings": {}, "cached": true, "seedSource": "fresh"});
        assert_eq!(comparable(&line), r#"{"passed":1,"type":"summary"}"#);
        let test = json!({"type": "test", "name": "A", "durationMs": 3, "status": "pass"});
        assert_eq!(comparable(&test), r#"{"name":"A","status":"pass","type":"test"}"#);
    }

    #[test]
    fn the_fresh_daemon_gets_the_seed_the_daemon_gave_the_request() {
        let r = json!({"command": "runTests"});
        let summary = json!({"type": "summary", "seed": 42});
        assert_eq!(
            with_seed(&r, Some(&summary)),
            json!({"command": "runTests", "seed": 42})
        );
        assert_eq!(with_seed(&r, None), r);
    }

    fn answer(lines: &[Value]) -> Vec<String> {
        lines.iter().map(comparable).collect()
    }

    #[test]
    fn a_request_with_a_clock_row_is_a_cut() {
        let cut = json!({"type": "test", "name": "A", "notRun": true, "clock": "cpu", "status": "error"});
        let refused = json!({"type": "test", "name": "B", "notRun": true, "status": "error"});
        let summary = json!({"type": "summary", "notRun": 2});
        assert_eq!(
            cause(&answer(&[refused, cut, summary.clone()]), Some(&summary)),
            Some(Cause::Cut)
        );
    }

    #[test]
    fn a_request_with_only_refused_rows_is_refused() {
        let refused = json!({"type": "test", "name": "B", "notRun": true, "status": "error"});
        let summary = json!({"type": "summary", "notRun": 1});
        assert_eq!(
            cause(&answer(&[refused, summary.clone()]), Some(&summary)),
            Some(Cause::Refused)
        );
    }

    #[test]
    fn a_request_without_a_test_not_run_has_no_cause() {
        let passed = json!({"type": "test", "name": "A", "status": "pass"});
        let summary = json!({"type": "summary", "notRun": 0, "passed": 1});
        assert_eq!(cause(&answer(&[passed, summary.clone()]), Some(&summary)), None);
        assert_eq!(cause(&[], None), None);
    }

    #[test]
    fn a_state_line_reads_back_as_the_state_it_was_written_from() {
        let s = DaemonState {
            kinds: vec![vec!["a".into()], Vec::new()],
            opaque: vec!["why".into()],
        };
        assert_eq!(state_of(&state_json(&s)), Some(s));
    }
}
