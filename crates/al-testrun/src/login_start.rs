//! Login starts (al-runner issue 8211, PERF-01 step 3): a test session whose login inputs ([`crate::login_replay::Key`])
//! were seen before takes what that login left (a [`Start`]) in place of running it again. The first session of a key runs
//! the login and keeps its al-vm `LoginImage` with the open's writes and assumption marks; a later session of the key
//! thaws the image into itself in `runtime/run.rs` `open_company`, when [`why_not`] finds nothing against it. A key whose
//! session the image refuses (`NotImaged`) runs the real login every time. `RunOptions::login_replay_off`
//! (`ALRUNNER_LOGIN_REPLAY=0`) turns it off; under the login replay check (`ALRUNNER_LOGIN_REPLAY_CHECK=1`) no session
//! takes a start.
//!
//! The starts live on the engine ([`starts`], `Engine::derived`): at most [`MAX_STARTS`] and [`MAX_BYTES`] of
//! `LoginImage::kept_bytes`, least recently used out; one start over [`MAX_START_BYTES`] is not kept (`too large`).
//!
//! PERF-01 step 3 A1: the interface only. Nothing is kept and nothing is taken: `take` finds nothing, `record` keeps
//! nothing, and `open_company` runs every login as before.
// A1 stubs: used from A2 (the store) and A3 (the replay path) on.
#![allow(dead_code)]

use crate::install::CompanyOpenWrites;
use crate::login_replay::Key;
use al_builtins::assumptions::HitBits;
use al_vm::{Engine, LoginImage, NotImaged};
use std::sync::{Arc, Mutex};

mod outside;

/// The most starts an engine keeps (ADR-073 Phase 1).
pub(crate) const MAX_STARTS: usize = 8;
/// The most bytes (`LoginImage::kept_bytes`) the starts of an engine hold together.
pub(crate) const MAX_BYTES: u64 = 64 << 20;
/// The most bytes one start may hold: a larger one is not kept (`too large`), so one start never empties the others.
pub(crate) const MAX_START_BYTES: u64 = 16 << 20;
/// The most keys an engine remembers as refused (no image), with why.
pub(crate) const MAX_REFUSED: usize = 64;

/// What the raised login left in the first session of a key: the session image, what the open noted for the tests, and
/// the assumption marks the open made (a replay raises them again, so a flagged pass stays flagged).
pub(crate) struct Start {
    pub image: LoginImage,
    pub wrote: CompanyOpenWrites,
    pub hits: HitBits,
}

// A start is kept on the engine and taken by sessions of any thread.
const _: fn() = || {
    fn both<T: Send + Sync>() {}
    both::<Start>();
};

/// Why a session does not take a start (design section 3.4), each counted in the report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotTaken {
    /// No start for the key, or the key's session was not imaged.
    NoStart,
    /// The run notes the code it runs: `--explain` or an observer, coverage, the impact trace, the profiler.
    NotesCode,
    /// A stability perturbation or a set clock start: shapes the check never measured.
    Perturbed,
    /// Single instances of the codeunit before were moved in (the strict-order lane).
    MovedIn,
    /// The session is not a thaw target (`Session::thaw_target_why`).
    Target(&'static str),
    /// The store the session starts from has another shape than the start's (`LoginImage::base_shape`).
    BaseShape,
}

/// The starts of one engine (see the module comment).
#[derive(Default)]
pub(crate) struct Starts {
    inner: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// The kept starts, the one used last at the end.
    kept: Vec<(Key, Arc<Start>)>,
    /// The keys whose session was not imaged, with why.
    refused: Vec<(Key, String)>,
    tally: Tally,
}

/// What the starts of an engine did, for the report.
#[derive(Default)]
struct Tally {
    replayed: u64,
    recorded: u64,
    /// Per reason (`NotImaged::reason`, or `too large`), the keys whose start was not kept.
    not_kept: std::collections::BTreeMap<String, u64>,
    /// Per reason, the sessions with a start that did not take it.
    not_taken: std::collections::BTreeMap<String, u64>,
}

impl Starts {
    /// Whether `key` has a start or was refused: its session is then not frozen again. A1: always `false`.
    pub(crate) fn knows(&self, _key: &Key) -> bool {
        false
    }

    /// The start of `key`, the one used last from now on. A1: always `None`.
    pub(crate) fn take(&self, _key: &Key) -> Option<Arc<Start>> {
        None
    }

    /// Keeps the start of `key` (or why its session has none) within the caps. A1: keeps nothing.
    pub(crate) fn record(&self, _key: Key, _start: Result<Start, NotImaged>) {}

    /// Counts a session that had a start and did not take it.
    pub(crate) fn not_taken(&self, _why: NotTaken) {}

    /// The bytes the kept starts hold (`LoginImage::kept_bytes`), for the memory pool's meter. A1: 0.
    pub(crate) fn bytes(&self) -> u64 {
        0
    }

    /// How many sessions took a start. A1: 0.
    pub(crate) fn replayed(&self) -> u64 {
        0
    }

    /// How many starts are kept. A1: 0.
    pub(crate) fn kept(&self) -> usize {
        0
    }

    /// The report line (`login start: ...`): `None` before a start was recorded or taken. A1: `None`.
    pub(crate) fn line(&self) -> Option<String> {
        None
    }
}

/// The starts of `engine`.
pub(crate) fn starts(engine: &Engine) -> Arc<Starts> {
    engine.derived(Starts::default)
}

/// Why `session` does not take the start `start` of its key; `None`: it takes it. `notes_code`, `perturbed`, `moved_in`:
/// what `run_codeunit_inner` knows of the run. A1: always `Some(NotTaken::NoStart)`.
pub(crate) fn why_not(
    _session: &al_vm::Session,
    _start: &Start,
    _notes_code: bool,
    _perturbed: bool,
    _moved_in: bool,
) -> Option<NotTaken> {
    Some(NotTaken::NoStart)
}

#[cfg(test)]
thread_local! {
    /// While `Some`, `open_company` adds each test session's state after it opened the company, run or replayed
    /// ([`opened`]): a test compares a replayed session with a real one.
    pub(crate) static OPENED: std::cell::RefCell<Option<Vec<crate::login_replay::LoginState>>> =
        const { std::cell::RefCell::new(None) };
    /// While `true`, a start is taken without its assumption marks: a test shows the marks' test bites.
    pub(crate) static DROP_HITS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Notes `session`'s state after it opened the company in [`OPENED`], when a test asked for it.
#[cfg(test)]
pub(crate) fn opened(session: &mut al_vm::Session, wrote: &CompanyOpenWrites, hits: HitBits) {
    if OPENED.with(|o| o.borrow().is_some()) {
        let state = crate::login_replay::LoginState::of(session, wrote, hits);
        OPENED.with(|o| o.borrow_mut().as_mut().map(|v| v.push(state)));
    }
}
