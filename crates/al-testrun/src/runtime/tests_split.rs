//! Where the tests phase of a run spent its time, summed over its test codeunits (`Phases::split`, the
//! `timings.testsParts` of a server answer). A measurement only: no answer depends on it.
//!
//! Each test codeunit's run (`run.rs` `run_codeunit_inner`) marks the steps it reaches on its [`UnitClock`]; `run_unit`
//! (`mod.rs`) turns the marks into a [`TestsSplit`] from its own start and end, and the run sums them.

use std::time::{Duration, Instant};

/// The marks one test codeunit's run takes as it reaches each step (`Scratch::clock`).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct UnitClock {
    /// The session is made and holds the install baseline (`new_session`, `set_up_session`).
    session: Option<Instant>,
    /// The session opened the company: the install steps and the login (`Start::open`).
    opened: Option<Instant>,
    /// The first test method started (`test_started` of the first test).
    first_test: Option<Instant>,
    /// The last test method ended (its row's time taken).
    last_end: Option<Instant>,
    /// The tests' own times, as their rows report them.
    bodies: Duration,
    /// Of `bodies`: the end of each test's isolation (`isolation::after_test`), which its row's time includes.
    rollback: Duration,
}

impl UnitClock {
    /// The session is made.
    pub(crate) fn session_made(&mut self) {
        self.session = Some(Instant::now());
    }

    /// The session opened the company (or failed to).
    pub(crate) fn opened(&mut self) {
        self.opened = Some(Instant::now());
    }

    /// A test method started at `at`.
    pub(crate) fn test_started(&mut self, at: Instant) {
        self.first_test.get_or_insert(at);
    }

    /// The test method that started at `at` took `took`, as its row says.
    pub(crate) fn test_ended(&mut self, at: Instant, took: Duration) {
        self.last_end = Some(at + took);
        self.bodies += took;
    }

    /// Runs `f`, the end of a test's isolation, and counts its time.
    pub(crate) fn rollback<R>(&mut self, f: impl FnOnce() -> R) -> R {
        let from = Instant::now();
        let out = f();
        self.rollback += from.elapsed();
        out
    }
}

/// The tests phase by step, summed over test codeunits. `entry_start + reset + bodies` is the codeunits' own time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TestsSplit {
    /// From the codeunit's start to its first test method: [`TestsSplit::session`] + `login` + `on_run`.
    pub session: Duration,
    /// The install steps and the company open, the login included (`Start::open`).
    pub login: Duration,
    /// After the open, up to the first test method: the runner's steps before the codeunit, OnRun, the start of the
    /// isolation (Function's checkpoint).
    pub on_run: Duration,
    /// The test methods' own times, as their rows report them (`durationMs`).
    pub bodies: Duration,
    /// Of `bodies`: the end of each test's isolation (Function's checkpoint restore), which its row includes.
    pub rollback_in_bodies: Duration,
    /// Between the test methods, outside their rows (the next test's handlers and runner steps before its clock).
    pub between: Duration,
    /// After the last test method to the codeunit's result: the runner's steps after the codeunit, the session's end.
    pub end: Duration,
    /// The test codeunits counted.
    pub units: usize,
}

impl TestsSplit {
    /// One codeunit's run that started at `started` and ended at `ended`, from the marks `c` it took. A step it did not
    /// reach takes no time: the time after the last mark it reached counts for the step after it.
    pub(crate) fn of(started: Instant, c: &UnitClock, ended: Instant) -> TestsSplit {
        let session = c.session.unwrap_or(ended).max(started);
        let opened = c.opened.unwrap_or(ended).max(session);
        let first = c.first_test.unwrap_or(ended).max(opened);
        let last = c.last_end.unwrap_or(first).clamp(first, ended.max(first));
        TestsSplit {
            session: session - started,
            login: opened - session,
            on_run: first - opened,
            bodies: c.bodies,
            rollback_in_bodies: c.rollback,
            between: (last - first).saturating_sub(c.bodies),
            end: ended.saturating_duration_since(last),
            units: 1,
        }
    }

    /// From the codeunit's start to its first test method.
    pub fn entry_start(&self) -> Duration {
        self.session + self.login + self.on_run
    }

    /// Outside the test methods' rows after the first one started: between them and after the last.
    pub fn reset(&self) -> Duration {
        self.between + self.end
    }
}

impl std::ops::AddAssign for TestsSplit {
    fn add_assign(&mut self, o: TestsSplit) {
        self.session += o.session;
        self.login += o.login;
        self.on_run += o.on_run;
        self.bodies += o.bodies;
        self.rollback_in_bodies += o.rollback_in_bodies;
        self.between += o.between;
        self.end += o.end;
        self.units += o.units;
    }
}

impl std::iter::Sum for TestsSplit {
    fn sum<I: Iterator<Item = TestsSplit>>(parts: I) -> TestsSplit {
        let mut total = TestsSplit::default();
        for p in parts {
            total += p;
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_parts_of_one_codeunit_add_up_to_its_time() {
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        let c = UnitClock {
            session: Some(t0 + ms(5)),
            opened: Some(t0 + ms(20)),
            first_test: Some(t0 + ms(30)),
            last_end: Some(t0 + ms(70)),
            bodies: ms(35),
            rollback: ms(3),
        };
        let s = TestsSplit::of(t0, &c, t0 + ms(80));
        assert_eq!((s.session, s.login, s.on_run), (ms(5), ms(15), ms(10)));
        assert_eq!((s.bodies, s.between, s.end), (ms(35), ms(5), ms(10)));
        assert_eq!(s.entry_start() + s.bodies + s.reset(), ms(80));
    }

    #[test]
    fn a_codeunit_whose_open_was_refused_counts_the_rest_as_its_login() {
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        let c = UnitClock {
            session: Some(t0 + ms(5)),
            ..Default::default()
        };
        let s = TestsSplit::of(t0, &c, t0 + ms(40));
        assert_eq!(
            (s.session, s.login, s.on_run, s.bodies, s.between, s.end),
            (ms(5), ms(35), ms(0), ms(0), ms(0), ms(0))
        );
    }
}
