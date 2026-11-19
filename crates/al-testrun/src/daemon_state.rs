//! The daemon clean check (al-runner issues 8180 and 8334): is a kept `serve` daemon, after a request with a refused test
//! or a test the time limit cut ([`Cause`]), in the state a fresh daemon would be in for the next request? This module
//! renders what a daemon keeps for later requests ([`render`], [`DaemonState`]) and compares two daemons ([`differences`],
//! [`Check`]); it changes nothing. The front end (alrunner-cli `daemon_clean.rs`, `ALRUNNER_DAEMON_CLEAN_CHECK=1`) runs
//! the fresh daemon and feeds both.
//!
//! What is rendered, by kind ([`KINDS`]), is what a later request of the same process can read:
//! - `store`: the install baseline kept on this thread (`baseline_cache::read_kept`) and the one carried over an edit,
//!   each with the login records a session over it can replay (`install::replayable_login_records`): those the process
//!   recorded or read for its origin and those its file held when it was read. Not the file's records alone: a daemon
//!   keeps the baseline as read before its first request recorded the login and wrote it back to the file, which a
//!   fresh daemon then reads (`runtime/corpus.rs` `keep_login`), and both replay the same records;
//! - `engines`: the kept engines (`Compiled::kept`): what they serve (their key), and what a run leaves on them that the
//!   next request's take (`corpus.rs` `take_kept_engines`, `Engine::reset_run_state`) does not reset: the login memo's
//!   verdict (`install::LoginMemo`). The fields a run sets are reset there, each named, so they are not rendered;
//! - `session`: what a codeunit's session leaves on this thread: the Disabled-isolation carry (`run::CARRIED`), the lane's
//!   single instances (`run::LANE_SINGLES`), the codeunit instances of this thread still alive
//!   (`instance_cycles::alive_on_this_thread`) and the assumption marks;
//! - `language`: the session language a session left on this thread (`filter::filter_language`, `ops::turkic_code`);
//! - `clock`: the instant the run clock started (`host::SystemClock`), set once per process, and whether the wall clock runs;
//! - `process`: the process-wide settings and flags a request sets (`host.rs`, `rendered_text`, `placeholder_output`,
//!   al-vm `process_flag_lines`), and the helper processes alive (`dotnet::helpers_alive`, issue 8334);
//! - `compile`: the kept compile (`Warm`): its key, files and what it decided about the baseline;
//! - `front end`: what the front end keeps (alrunner-cli fills it: the dependency apps, the kept backup, the license).
//!
//! Not rendered, because a fresh daemon fills them the same from the same program or input, or because they number things
//! per process: the caches of the engine worked out from the program (`Engine::reset_run_state` names them), the
//! other `Engine::derived` values, the login records of baseline origins other than a kept baseline's (`install.rs`
//! `LOGIN_RECORDS`), the shared virtual-table listings (al-vm `platform` `SHARED`), engine uids and baseline origins. The
//! .NET sidecar host is made for each request (alrunner-cli `run.rs` `test_inner`) and shut down at its end; a kept
//! engine holds that host until the next request takes it and sets its own (`corpus.rs` `take_kept_engines`). Whether a
//! helper process outlives a request is measured, not assumed: the `process` line `helper processes alive`.
//!
//! Not rendered either, because they live in the codeunit's session, which a request drops (issue 8334): the locks of the
//! open transaction (al-vm `state.rs:91,113`, `OpenTx` in the session's `TxState`) and the in-memory `File` system (each
//! codeunit's session gets a new host, `runtime/run.rs:437-439`, whose files start as an empty `MemFileSystem`, al-host
//! `lib.rs:896`; with `--files-dir` the files are the folder's, `host.rs:299`, compared by [`outside`]).

use crate::runtime::Warm;
use al_vm::state_content as content;

/// The kinds of state a daemon keeps, in the order [`DaemonState::kinds`] holds them.
pub const KINDS: [&str; 8] = [
    "store",
    "engines",
    "session",
    "language",
    "clock",
    "process",
    "compile",
    "front end",
];

/// The index of the front end's kind in [`KINDS`].
pub const FRONT_END: usize = 7;

/// What a daemon keeps for its later requests: per kind of [`KINDS`], its lines; and the values that have no comparable
/// form, with why.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DaemonState {
    pub kinds: Vec<Vec<String>>,
    pub opaque: Vec<String>,
}

/// What the daemon whose requests run on this thread keeps, with `warm` its kept compile. Read this thread's state: a
/// serve daemon runs its requests on one thread (`baseline_cache::read_kept`), which is the first test thread of each run.
pub fn render(warm: &Warm) -> DaemonState {
    let mut opaque = Vec::new();
    let kinds = vec![
        store(),
        engines(warm),
        session(&mut opaque),
        language(),
        clock(),
        process(),
        compile(warm),
        Vec::new(),
    ];
    DaemonState { kinds, opaque }
}

/// A baseline as lines: its rows (`state_diff::committed_rows`) and the rest it holds (`Baseline::state_lines`).
fn baseline_lines(what: &str, b: &al_vm::Baseline, out: &mut Vec<String>) {
    out.extend(
        crate::state_diff::committed_rows(b.store())
            .into_iter()
            .map(|r| format!("{what} row {r}")),
    );
    out.extend(b.state_lines().into_iter().map(|l| format!("{what} {l}")));
}

fn store() -> Vec<String> {
    crate::baseline_cache::look_kept(|kept, carried| {
        let mut out = Vec::new();
        let run_lines = |what: &str, run: &crate::baseline::BaselineRun, at: i64, out: &mut Vec<String>| {
            out.push(format!(
                "{what} began {at} ran {}/{} filled {} unseeded {} notes {} login {}",
                run.ran,
                run.total,
                run.filled,
                content(&run.unseeded),
                content(&run.notes),
                content(&crate::install::replayable_login_records(
                    run.baseline.origin(),
                    &run.login
                ))
            ));
            out.extend(run.restored_lines().into_iter().map(|l| format!("{what} restored {l}")));
            baseline_lines(what, &run.baseline, out);
        };
        match kept {
            Some((path, run, at)) => {
                let file = path.file_name().map(|f| f.to_string_lossy().into_owned());
                out.push(format!("kept baseline file {}", content(&file)));
                run_lines("kept", run, at, &mut out);
            }
            None => out.push("kept baseline none".into()),
        }
        match carried {
            Some((run, at)) => run_lines("carried", run, at, &mut out),
            None => out.push("carried baseline none".into()),
        }
        out
    })
}

fn engines(warm: &Warm) -> Vec<String> {
    let Some(compiled) = warm.compiled() else {
        return vec!["no compile kept".into()];
    };
    let kept = compiled.kept.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(k) = kept.as_ref() else {
        return vec!["no engines kept".into()];
    };
    let mut out = vec![format!("key {}", k.key)];
    out.push(format!(
        "install engine login memo {}",
        crate::install::memo_line(&k.install)
    ));
    match &k.tests {
        Some(t) => out.push(format!("tests engine login memo {}", crate::install::memo_line(t))),
        None => out.push("tests engine is the install engine".into()),
    }
    out
}

fn session(opaque: &mut Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    crate::runtime::look_carried(|carried| match carried {
        None => out.push("disabled carry none".into()),
        Some(c) => {
            out.push(format!(
                "disabled carry tables {} unfinished {} everything {} storage {} as started {}",
                content(&c.tables),
                content(&c.unfinished),
                c.everything,
                content(&c.storage_why),
                c.as_started
            ));
            match &c.baseline {
                Some(b) => baseline_lines("disabled carry", b, &mut out),
                None => out.push("disabled carry baseline none".into()),
            }
        }
    });
    match crate::runtime::lane_singles() {
        None => out.push("lane singles none".into()),
        Some(false) => out.push("lane singles lane open, none moved".into()),
        Some(true) => opaque.push("lane singles: single instances moved between sessions (a session's objects)".into()),
    }
    out.push(format!(
        "live codeunit instances {}",
        al_builtins::instance_cycles::alive_on_this_thread()
    ));
    // Read without clearing: the marks go back as they were.
    let hits = al_builtins::assumptions::take_hit_bits();
    al_builtins::assumptions::add_hit_bits(hits);
    out.push(format!("assumption marks {}", content(&hits)));
    out
}

fn language() -> Vec<String> {
    vec![format!(
        "filter language {} turkic code {}",
        al_builtins::filter::filter_language(),
        al_builtins::ops::turkic_code()
    )]
}

fn clock() -> Vec<String> {
    clock_lines(crate::host::SystemClock::start_if_started(), crate::host::real_clock())
}

/// The clock lines of a run clock that started at `start` (`None`: not yet), with `real` the wall clock.
pub(crate) fn clock_lines(start: Option<i64>, real: bool) -> Vec<String> {
    vec![format!("run clock start {} real clock {real}", content(&start))]
}

fn process() -> Vec<String> {
    let mut out = vec![
        format!("server url {}", content(&crate::host::server_url())),
        format!("company name {}", content(&crate::host::company_name())),
        format!("companies {}", content(&crate::host::companies())),
        format!(
            "environment {} real files {} publish order known {}",
            crate::host::environment_text(),
            crate::host::uses_real_files(),
            crate::host::publish_order_known()
        ),
        format!(
            "rendered texts {} active {} placeholder sizes made {}",
            al_builtins::rendered_text::renders(),
            al_builtins::rendered_text::active(),
            al_builtins::placeholder_output::sizes_made()
        ),
    ];
    out.extend(al_vm::process_flag_lines());
    out.extend(helper_lines(crate::dotnet::helpers_alive()));
    out
}

/// The process lines of the helper processes (`dotnet::helpers_alive`: session sidecars, standbys, the render process)
/// this process holds, `alive` of them. A warm server starts no standby ahead (alrunner-cli `run.rs:1788-1789`:
/// `looks_pure` holds with a kept compile, so neither `prewarm` of `run.rs:1797-1806` runs), and a session that starts
/// its own process starts standbys only when `prewarm_on_first_use` asked for them (`dotnet.rs:1175-1188`); the pool
/// is closed at the request's end (alrunner-cli `run.rs:2066`, `dotnet.rs:548-549`). So the count read between two
/// requests is not a standby still starting.
pub(crate) fn helper_lines(alive: usize) -> Vec<String> {
    vec![format!("helper processes alive {alive}")]
}

fn compile(warm: &Warm) -> Vec<String> {
    let Some(slot) = warm.kept_slot() else {
        return vec!["no compile kept".into()];
    };
    let c = &slot.compiled;
    let mut out = vec![format!("key {}", slot.key)];
    out.extend(
        slot.files
            .iter()
            .map(|(app, path, hash)| format!("file {app} {path} {hash:x}")),
    );
    out.push(format!(
        "baseline wanted {} baseline {} note {} lean {} selection {}",
        c.baseline_wanted,
        c.baseline,
        content(&c.baseline_note),
        content(&c.lean),
        content(&c.opts_only)
    ));
    out
}

/// The kinds of difference between two daemons, in the order [`differences`] looks at them: a value with no comparable
/// form, the answers to the same request, then each kind of kept state of [`KINDS`].
pub const DIFFERENCE_KINDS: [&str; 10] = [
    "not comparable",
    "answers",
    "store",
    "engines",
    "session",
    "language",
    "clock",
    "process",
    "compile",
    "front end",
];

/// What differs between two daemons: `answers`, the answer lines each gave to one request (`None`: not compared), and the
/// state each kept after it. Per kind of [`DIFFERENCE_KINDS`] that differs, its index and its first difference.
pub fn differences(answers: Option<(&[String], &[String])>, a: &DaemonState, b: &DaemonState) -> Vec<(usize, String)> {
    let lines = crate::state_diff::line_difference;
    let mut out: Vec<Option<String>> = vec![
        crate::state_diff::not_comparable(&a.opaque, &b.opaque),
        answers.and_then(|(x, y)| lines(x, y, "answer line")),
    ];
    let empty = Vec::new();
    for (k, kind) in KINDS.iter().enumerate() {
        let (x, y) = (a.kinds.get(k).unwrap_or(&empty), b.kinds.get(k).unwrap_or(&empty));
        out.push(lines(x, y, kind));
    }
    out.into_iter().enumerate().filter_map(|(k, d)| Some((k, d?))).collect()
}

/// Why a request is compared with a fresh daemon: a test of it was refused (`notRun`), or the time limit cut one (its
/// `test` line has a `clock`; a request with both is `Cut`). The front end (alrunner-cli `daemon_clean.rs` `cause`) names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    Refused,
    Cut,
}

impl Cause {
    /// The word the report line names the cause with.
    pub fn word(self) -> &'static str {
        match self {
            Cause::Refused => "refused",
            Cause::Cut => "cut",
        }
    }
}

/// What a request can leave outside the daemon: the `--files-dir` folder ([`outside`]). A fresh daemon gets the same
/// folder, so this is compared on one daemon, before the request against after it ([`outside_differences`]).
pub const OUTSIDE_KINDS: [&str; 1] = ["files"];

/// What the `--files-dir` folder holds now (`host::files_dir_lines`), compared with [`outside_differences`].
pub fn outside() -> Vec<String> {
    crate::host::files_dir_lines()
}

/// What differs between the files folder `before` a request and `after` it, both [`outside`]: per kind of
/// [`OUTSIDE_KINDS`] that differs, its index and its first difference, naming the folder after the request first (as
/// [`differences`] names this daemon first).
pub fn outside_differences(before: &[String], after: &[String]) -> Vec<(usize, String)> {
    crate::state_diff::line_difference(after, before, OUTSIDE_KINDS[0])
        .map(|d| (0, d))
        .into_iter()
        .collect()
}

/// The tallies of the requests of one [`Cause`].
#[derive(Default)]
struct Tallies {
    after: crate::state_diff::Tally,
    next: crate::state_diff::Tally,
    /// The `--files-dir` folder right after the request against before it; noted only when the daemon has one.
    left: crate::state_diff::Tally,
    /// Requests not compared, with why (no later request, the fresh daemon did not answer).
    not_compared: Vec<String>,
}

/// The check over the requests of one daemon, per [`Cause`]: per refused or cut request, the state right after it compared
/// with a fresh daemon's after the requests before it but the refused and cut ones (`after`), the next request's answers
/// and the state after it compared on both (`next`), and what it left in the files folder (`left`).
#[derive(Default)]
pub struct Check {
    /// Indexed by `Cause as usize`.
    by_cause: [Tallies; 2],
}

impl Check {
    fn of(&mut self, cause: Cause) -> &mut Tallies {
        &mut self.by_cause[cause as usize]
    }

    /// Notes the differences of the state right after a refused or cut request.
    pub fn note_after(&mut self, cause: Cause, differs: &[(usize, String)]) {
        self.of(cause).after.note(differs);
    }

    /// Notes the differences of the next request's answers and the state after it.
    pub fn note_next(&mut self, cause: Cause, differs: &[(usize, String)]) {
        self.of(cause).next.note(differs);
    }

    /// Notes the differences of the files folder right after a refused or cut request against before it
    /// ([`outside_differences`]).
    pub fn note_left(&mut self, cause: Cause, differs: &[(usize, String)]) {
        self.of(cause).left.note(differs);
    }

    /// Notes a refused or cut request that could not be compared, with why.
    pub fn note_not_compared(&mut self, cause: Cause, why: String) {
        self.of(cause).not_compared.push(why);
    }

    /// The report line of the requests of `cause`: `mode` says which requests the fresh daemon ran. The part on the files
    /// folder is there once a request of `cause` noted it (`note_left`: the daemon has a files folder).
    pub fn line(&self, mode: &str, cause: Cause) -> String {
        let t = &self.by_cause[cause as usize];
        let left = match t.left.compared {
            0 => String::new(),
            _ => format!("; left outside the daemon: {}", t.left.counts(&OUTSIDE_KINDS)),
        };
        let not = match t.not_compared.first() {
            None => String::new(),
            Some(first) => format!("; not compared {} (first: {first})", t.not_compared.len()),
        };
        format!(
            "daemon clean check ({mode}): after the {} request: {}; next request: {}{left}{not}",
            cause.word(),
            t.after.counts(&DIFFERENCE_KINDS),
            t.next.counts(&DIFFERENCE_KINDS)
        )
    }
}
