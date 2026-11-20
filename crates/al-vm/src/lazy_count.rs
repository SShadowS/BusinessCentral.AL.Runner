//! PERF-28 measurement, throwaway (never for a pull request): what happens first to each Record variable the PERF-28 design
//! would build on first use. Off unless the CLI calls `enable_lazy_record_count` (ADR-006: this crate reads no environment
//! variable). Off, a birth costs one relaxed atomic load and the compiler emits no op.
//!
//! An eligible slot is one the design defers: a record of a stored table (`record_without_state`), not a parameter, not
//! the named return value, while no debugger, no value capture and no not-installed app set is active. Two kinds: the
//! locals of a frame (`call_inner`) and the record globals of a table instance (`table_instance`, `reset_table_globals`).
//!
//! Each eligible record is noted at birth by its address, with a `Weak`. The first touch classifies it:
//! - the compiler emits `Op::LazyTouch` at the start of each statement that names the slot (for a compound statement, its
//!   header only; its bodies are statements of their own), so the first touch is the first statement that names it;
//! - (b) the statement replaces the whole buffer: the slot is the target of `R := ..`, or the receiver of Get, Find,
//!   FindFirst, FindLast, FindSet or Init, or the argument of Clear, as the whole statement or condition (under `not`), and
//!   the statement names the slot nowhere else;
//! - (c) any other statement that names it, and a read of it through another record (`Place::RecGlobal`);
//! - (a) the record was dropped (its `Weak` is dead) before any of these. Found by a sweep at the end of every outermost
//!   call, when the thread's table grows, and before a line is printed. A record not yet swept counts as `open`, never as (a).

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::{Rc, Weak};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use al_builtins::builtin_id::{BuiltinId, ids};
use al_builtins::value::{RecordState, RtType, Value};
use al_ir::{Arg, Expr, Place, Stmt};

use crate::Session;

static ON: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Kind {
    Local,
    Global,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Tally {
    n: u64,
    a: u64,
    b: u64,
    c: u64,
}

/// Births and first-touch classes by (table id, kind), for every thread.
static COUNTS: Mutex<BTreeMap<(u32, Kind), Tally>> = Mutex::new(BTreeMap::new());

/// The eligible records of this thread not touched yet, by address.
struct Live {
    map: HashMap<usize, (Weak<RefCell<RecordState>>, u32, Kind)>,
    sweep_at: usize,
}

impl Drop for Live {
    fn drop(&mut self) {
        sweep_map(&mut self.map);
    }
}

thread_local! {
    static LIVE: RefCell<Live> = RefCell::new(Live { map: HashMap::new(), sweep_at: 1024 });
}

/// Turns the counter on for the whole process. Call before any routine is compiled.
pub fn enable_lazy_record_count() {
    ON.store(true, Ordering::Relaxed);
}

#[inline]
pub(crate) fn on() -> bool {
    ON.load(Ordering::Relaxed)
}

/// The summary line, cumulative since the process started; `None` while the counter is off. Sweeps this thread first.
pub fn lazy_record_count_line() -> Option<String> {
    if !on() {
        return None;
    }
    sweep();
    Some(summary_line(&COUNTS.lock().unwrap_or_else(|e| e.into_inner())))
}

fn add(id: u32, kind: Kind, f: impl FnOnce(&mut Tally)) {
    f(COUNTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry((id, kind))
        .or_default());
}

fn sweep_map(map: &mut HashMap<usize, (Weak<RefCell<RecordState>>, u32, Kind)>) {
    map.retain(|_, (w, id, kind)| {
        let dead = w.strong_count() == 0;
        if dead {
            add(*id, *kind, |t| t.a += 1);
        }
        !dead
    });
}

/// Counts as (a) every noted record of this thread that was dropped untouched.
pub(crate) fn sweep() {
    if !on() {
        return;
    }
    let _ = LIVE.try_with(|l| sweep_map(&mut l.borrow_mut().map));
}

fn birth(rc: &Rc<RefCell<RecordState>>, id: u32, kind: Kind) {
    add(id, kind, |t| t.n += 1);
    let _ = LIVE.try_with(|l| {
        let mut l = l.borrow_mut();
        // A dead record at the same address was dropped untouched.
        if let Some((w, old_id, old_kind)) = l.map.remove(&(Rc::as_ptr(rc) as usize)) {
            if w.strong_count() == 0 {
                add(old_id, old_kind, |t| t.a += 1);
            }
        }
        l.map.insert(Rc::as_ptr(rc) as usize, (Rc::downgrade(rc), id, kind));
        if l.map.len() >= l.sweep_at {
            sweep_map(&mut l.map);
            l.sweep_at = (2 * l.map.len()).max(1024);
        }
    });
}

/// The first touch of `rc`, if it is a noted record: (b) when `replaces`, else (c).
pub(crate) fn touch(rc: &Rc<RefCell<RecordState>>, replaces: bool) {
    let _ = LIVE.try_with(|l| {
        let mut l = l.borrow_mut();
        let key = Rc::as_ptr(rc) as usize;
        let Some((w, _, _)) = l.map.get(&key) else { return };
        if !w.upgrade().is_some_and(|x| Rc::ptr_eq(&x, rc)) {
            return;
        }
        let (_, id, kind) = l.map.remove(&key).expect("present");
        add(id, kind, |t| if replaces { t.b += 1 } else { t.c += 1 });
    });
}

fn summary_line(counts: &BTreeMap<(u32, Kind), Tally>) -> String {
    let total = |k: Kind| {
        counts
            .iter()
            .filter(|((_, kk), _)| *kk == k)
            .fold(Tally::default(), |s, (_, t)| Tally {
                n: s.n + t.n,
                a: s.a + t.a,
                b: s.b + t.b,
                c: s.c + t.c,
            })
    };
    let part = |name: &str, t: Tally| {
        format!(
            "{name}={} a={} b={} c={} open={}",
            t.n,
            t.a,
            t.b,
            t.c,
            t.n.saturating_sub(t.a + t.b + t.c)
        )
    };
    let mut top: Vec<(&(u32, Kind), &Tally)> = counts.iter().collect();
    top.sort_by(|x, y| y.1.n.cmp(&x.1.n).then(x.0.cmp(y.0)));
    let top: Vec<String> = top
        .iter()
        .take(20)
        .map(|((id, k), t)| {
            let k = if *k == Kind::Local { "local" } else { "global" };
            format!("{id}:{k}:{}:a={}:b={}:c={}", t.n, t.a, t.b, t.c)
        })
        .collect();
    format!(
        "lazy-record-count: scope=since_start {} {} top=[{}]",
        part("locals", total(Kind::Local)),
        part("globals", total(Kind::Global)),
        top.join(", ")
    )
}

/// The slots of one routine whose references the compiler watches: its eligible locals and, in a table or table
/// extension, its eligible record globals.
pub(crate) struct Eligible {
    locals: Vec<bool>,
    globals: Vec<bool>,
}

impl Eligible {
    /// `None` while the counter is off.
    pub(crate) fn of(o: &al_ir::Object, r: &al_ir::Routine) -> Option<Eligible> {
        if !on() {
            return None;
        }
        let rec = |t: &RtType| matches!(t, RtType::Record(_, false));
        let locals = (r.locals.iter().enumerate())
            .map(|(i, s)| i >= r.params && Some(i) != r.ret_local && rec(&s.ty))
            .collect();
        let table = matches!(o.kind, al_ir::ObjKind::Table | al_ir::ObjKind::TableExtension);
        let globals = (o.globals.iter().enumerate())
            .map(|(g, s)| table && Some(g) != o.rec_slot && Some(g) != o.xrec_slot && rec(&s.ty))
            .collect();
        Some(Eligible { locals, globals })
    }

    /// The touches statement `s` makes when it starts: (slot, is a global, replaces the whole buffer).
    pub(crate) fn touches(&self, s: &Stmt) -> Vec<(u16, bool, bool)> {
        let Some(head) = header(s) else { return Vec::new() };
        let text = format!("{head:?}");
        let target = replace_target(&head);
        let mut out = Vec::new();
        for (slots, name, global) in [(&self.locals, "Local", false), (&self.globals, "Global", true)] {
            for (i, _) in slots.iter().enumerate().filter(|(_, e)| **e) {
                let n = count_named(&text, name, i);
                if n == 0 {
                    continue;
                }
                let place = if global { Place::Global(i) } else { Place::Local(i) };
                let Ok(slot) = u16::try_from(i) else { continue };
                out.push((slot, global, n == 1 && target.as_ref() == Some(&place)));
            }
        }
        out
    }
}

/// How often `name(i)` stands in `text` as a whole word (not `BaseGlobal(i)`, `Local(12)`).
fn count_named(text: &str, name: &str, i: usize) -> usize {
    let pat = format!("{name}({i})");
    text.match_indices(&pat)
        .filter(|(at, _)| {
            !text[..*at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_')
        })
        .count()
}

/// The part of `s` that runs when it starts: the whole of a simple statement, the header of a compound one.
fn header(s: &Stmt) -> Option<Stmt> {
    Some(match s {
        Stmt::Assign(..) | Stmt::Expr(_) | Stmt::Exit(_) => s.clone(),
        Stmt::If(c, _, _) => Stmt::If(c.clone(), Vec::new(), Vec::new()),
        Stmt::While(c, _) => Stmt::While(c.clone(), Vec::new()),
        // The condition runs after the body; counted at the start (a touch counted early is never (a)).
        Stmt::Repeat(_, c) => Stmt::Repeat(Vec::new(), c.clone()),
        Stmt::For {
            var, from, to, down, ..
        } => Stmt::For {
            var: var.clone(),
            from: from.clone(),
            to: to.clone(),
            down: *down,
            body: Vec::new(),
        },
        Stmt::Foreach { var, collection, .. } => Stmt::Foreach {
            var: var.clone(),
            collection: collection.clone(),
            body: Vec::new(),
        },
        Stmt::Case {
            scrutinee, branches, ..
        } => Stmt::Case {
            scrutinee: scrutinee.clone(),
            branches: branches.iter().map(|(e, _)| (e.clone(), Vec::new())).collect(),
            otherwise: Vec::new(),
        },
        Stmt::AssertError(_) | Stmt::Break | Stmt::Continue | Stmt::Pos(_) | Stmt::Assume(_) | Stmt::TddMark(_) => {
            return None;
        }
    })
}

const REPLACING: [BuiltinId; 7] = [
    ids::RECORD_GET_1,
    ids::RECORD_FIND_1,
    ids::RECORD_FINDFIRST_1,
    ids::RECORD_FINDLAST_1,
    ids::RECORD_FINDSET_1,
    ids::RECORD_FINDSET_2,
    ids::RECORD_INIT_1,
];
const CLEAR: [BuiltinId; 3] = [ids::SYSTEM_CLEAR_1, ids::SYSTEM_CLEAR_2, ids::SYSTEM_CLEAR_3];

/// The local or global whose whole buffer statement header `s` replaces, if that is what it does.
fn replace_target(s: &Stmt) -> Option<Place> {
    match s {
        Stmt::Assign(p @ (Place::Local(_) | Place::Global(_)), _) => Some(p.clone()),
        Stmt::Expr(e) | Stmt::If(e, ..) | Stmt::While(e, _) | Stmt::Exit(Some(e)) => replaced_by(e),
        _ => None,
    }
}

fn replaced_by(e: &Expr) -> Option<Place> {
    match e {
        Expr::Unary(_, x) | Expr::Convert(_, x) => replaced_by(x),
        Expr::Builtin {
            id, receiver: Some(r), ..
        } if REPLACING.contains(id) => place_of(r),
        Expr::Builtin {
            id,
            receiver: None,
            args,
            ..
        } if CLEAR.contains(id) && args.len() == 1 => place_of(&args[0]),
        _ => None,
    }
}

fn place_of(a: &Arg) -> Option<Place> {
    match a {
        Arg::Ref(p) | Arg::Value(Expr::Load(p)) if matches!(p, Place::Local(_) | Place::Global(_)) => Some(p.clone()),
        _ => None,
    }
}

impl Session {
    /// Notes the value `v` slot `s` was just given, if the design would defer the slot (`kind` says where it is).
    #[inline]
    pub(crate) fn lazy_birth(&self, s: &al_ir::Slot, v: &Value, kind: Kind) {
        if on() {
            self.lazy_birth_slow(s, v, kind);
        }
    }

    #[cold]
    fn lazy_birth_slow(&self, s: &al_ir::Slot, v: &Value, kind: Kind) {
        if !self.record_without_state(&s.ty)
            || self.capture.is_some()
            || self.debug.is_some()
            || self.events.not_installed_apps.as_ref().is_some_and(|a| !a.is_empty())
        {
            return;
        }
        if let (Value::Record(rc), RtType::Record(t, _)) = (v, &s.ty) {
            let id = self.engine.program.objects[*t as usize].id.unwrap_or(0);
            birth(rc, id, kind);
        }
    }

    /// `Op::LazyTouch`: the statement that starts names slot `i` (a global of the frame's instance, or a local).
    #[cold]
    pub(crate) fn lazy_touch(&self, i: u16, global: bool, replaces: bool, f: &crate::Frame) {
        let i = usize::from(i);
        if global {
            if let Some(Value::Record(rc)) = f.instance.borrow().globals.get(i) {
                touch(rc, replaces);
            }
        } else if let Some(Value::Record(rc)) = f.locals.get(i) {
            touch(rc, replaces);
        }
    }
}

/// A read through `Place::RecGlobal`: a touch of class (c).
#[inline]
pub(crate) fn touch_other(v: &Value) {
    if on() {
        if let Value::Record(rc) = v {
            touch(rc, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_by_default_nothing_is_counted() {
        // No test in this binary turns the counter on.
        assert!(lazy_record_count_line().is_none());
        assert!(COUNTS.lock().unwrap().is_empty());
    }

    fn eligible() -> Eligible {
        Eligible {
            locals: vec![false, true, true],
            globals: vec![false, false, true],
        }
    }

    fn get(p: Place) -> Expr {
        Expr::Builtin {
            id: ids::RECORD_GET_1,
            receiver: Some(Box::new(Arg::Ref(p))),
            args: vec![Arg::Value(Expr::Const(al_ir::Const::Integer(1)))],
            ret_used: true,
        }
    }

    #[test]
    fn statements_classify_their_touches() {
        let e = eligible();
        // `if L1.Get(1) then L2.X := 1`: the header only, a replacing touch.
        let body = vec![Stmt::Assign(
            Place::Field(Box::new(Place::Local(2)), 0),
            Expr::Const(al_ir::Const::Integer(1)),
        )];
        let s = Stmt::If(get(Place::Local(1)), body.clone(), Vec::new());
        assert_eq!(e.touches(&s), vec![(1, false, true)]);
        // The body statement: a field write, (c).
        assert_eq!(e.touches(&body[0]), vec![(2, false, false)]);
        // `if not G2.Get(1)`: (b) for the global.
        let s = Stmt::If(
            Expr::Unary(al_ir::UnOp::Not, Box::new(get(Place::Global(2)))),
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(e.touches(&s), vec![(2, true, true)]);
        // `L1 := L2`: (b) for L1, (c) for L2; `L1 := L1` names it twice: (c).
        let s = Stmt::Assign(Place::Local(1), Expr::Load(Place::Local(2)));
        assert_eq!(e.touches(&s), vec![(1, false, true), (2, false, false)]);
        let s = Stmt::Assign(Place::Local(1), Expr::Load(Place::Local(1)));
        assert_eq!(e.touches(&s), vec![(1, false, false)]);
        // A parameter (slot 0) and `BaseGlobal(2)` are not watched slots.
        let s = Stmt::Expr(Expr::Load(Place::BaseGlobal(2)));
        assert!(e.touches(&s).is_empty());
        let s = Stmt::Expr(Expr::Load(Place::Local(0)));
        assert!(e.touches(&s).is_empty());
        assert_eq!(count_named("Local(12) Local(1) xLocal(1)", "Local", 1), 1);
    }

    #[test]
    fn the_line_sums_per_kind_and_ranks_tables() {
        let mut m = BTreeMap::new();
        m.insert((18, Kind::Local), Tally { n: 5, a: 2, b: 1, c: 1 });
        m.insert((37, Kind::Global), Tally { n: 7, a: 7, b: 0, c: 0 });
        assert_eq!(
            summary_line(&m),
            "lazy-record-count: scope=since_start locals=5 a=2 b=1 c=1 open=1 globals=7 a=7 b=0 c=0 open=0 \
             top=[37:global:7:a=7:b=0:c=0, 18:local:5:a=2:b=1:c=1]"
        );
    }
}
