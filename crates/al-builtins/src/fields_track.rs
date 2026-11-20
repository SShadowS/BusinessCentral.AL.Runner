//! PERF-22b measurement, throwaway: what a Lazy-eligible fresh record buffer meets first (design.md section 9). al-vm
//! `records/init_count.rs` decides eligibility and makes the tracked buffer; this module owns the buffer type and the
//! counts. Off unless `enable` was called; off, the cost is one load and branch per access of a record's fields.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::value::Value;

static ON: AtomicBool = AtomicBool::new(false);
/// Per table id: [lazy_eligible, untracked, a, b, c].
static COUNTS: Mutex<BTreeMap<u32, [u64; 5]>> = Mutex::new(BTreeMap::new());
const ELIGIBLE: usize = 0;
const UNTRACKED: usize = 1;

/// What a tracked buffer met first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum First {
    /// Dropped or replaced whole, no access before.
    A = 2,
    /// A read of non-Blob slots only, at an instrumented read site.
    B = 3,
    /// Anything else: a Blob slot, a write, a clone, a move out as a `Vec`, or any read the instrumented sites do not name.
    C = 4,
}

pub fn enable() {
    ON.store(true, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ON.load(Ordering::Relaxed)
}

fn bump(table: u32, i: usize) {
    COUNTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(table)
        .or_default()[i] += 1;
}

/// A Lazy-eligible `initial_fields` buffer of `table` that stays a `Vec` (not put in a record through `Fields::tracked`):
/// counted as eligible and untracked; the line counts it in `c`.
pub fn note_untracked(table: u32) {
    bump(table, ELIGIBLE);
    bump(table, UNTRACKED);
}

/// The summary line, cumulative since the process started; `None` while off.
pub fn lazy_line() -> Option<String> {
    enabled().then(|| line(&COUNTS.lock().unwrap_or_else(|e| e.into_inner())))
}

fn line(counts: &BTreeMap<u32, [u64; 5]>) -> String {
    let mut sum = [0u64; 5];
    for c in counts.values() {
        for i in 0..5 {
            sum[i] += c[i];
        }
    }
    let mut rows: Vec<(&u32, &[u64; 5])> = counts.iter().collect();
    rows.sort_by(|x, y| y.1[ELIGIBLE].cmp(&x.1[ELIGIBLE]).then(x.0.cmp(y.0)));
    let top: Vec<String> = rows
        .iter()
        .take(20)
        .map(|(t, c)| format!("{t}:{}:a={}:b={}:c={}", c[0], c[2], c[3], c[4] + c[1]))
        .collect();
    let open = sum[0] - sum[1] - sum[2] - sum[3] - sum[4];
    format!(
        "record-init-lazy: scope=since_start b_split=partial lazy_eligible={} a={} b={} c={} open={open} c_untracked={} top=[{}]",
        sum[0],
        sum[2],
        sum[3],
        sum[4] + sum[1],
        sum[1],
        top.join(", ")
    )
}

/// A record's field buffer: today's `Vec<Value>`, plus a measurement tracker (the table id of a tracked fresh buffer that
/// has met nothing yet, else 0). Every access but drop goes through `Deref`/`DerefMut` or the `note_*` read hooks.
#[derive(Default)]
pub struct Fields {
    vec: Vec<Value>,
    track: Cell<u32>,
}

impl Fields {
    /// A fresh Lazy-eligible buffer of table `table` (an AL id, never 0), tracked until its first event.
    pub fn tracked(vec: Vec<Value>, table: u32) -> Fields {
        bump(table, ELIGIBLE);
        Fields {
            vec,
            track: Cell::new(table),
        }
    }

    #[cold]
    fn first(&self, what: First) {
        let table = self.track.replace(0);
        bump(table, what as usize);
    }

    #[inline]
    fn touch(&self) {
        if self.track.get() != 0 {
            self.first(First::C);
        }
    }

    /// A read of slot `i` by an instrumented site: `B` unless the slot is a Blob.
    #[inline]
    pub fn note_slot_read(&self, i: usize) {
        if self.track.get() != 0 {
            let blob = matches!(self.vec.get(i), Some(Value::Blob(_)));
            self.first(if blob { First::C } else { First::B });
        }
    }

    /// A read of non-Blob slots only (for example the primary key) by an instrumented site.
    #[inline]
    pub fn note_plain_read(&self) {
        if self.track.get() != 0 {
            self.first(First::B);
        }
    }

    /// The buffer as a `Vec`: a move out, counted as `C`.
    pub fn into_vec(mut self) -> Vec<Value> {
        self.touch();
        std::mem::take(&mut self.vec)
    }

    /// A deep copy (a record clone): reads every slot.
    pub fn dup(&self) -> Fields {
        Fields::from(self.to_vec())
    }
}

impl Drop for Fields {
    fn drop(&mut self) {
        if self.track.get() != 0 {
            self.first(First::A);
        }
    }
}

impl std::fmt::Debug for Fields {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.deref().fmt(f)
    }
}

impl From<Vec<Value>> for Fields {
    fn from(vec: Vec<Value>) -> Self {
        Fields {
            vec,
            track: Cell::new(0),
        }
    }
}

impl Deref for Fields {
    type Target = Vec<Value>;
    #[inline]
    fn deref(&self) -> &Vec<Value> {
        self.touch();
        &self.vec
    }
}

impl DerefMut for Fields {
    #[inline]
    fn deref_mut(&mut self) -> &mut Vec<Value> {
        self.touch();
        &mut self.vec
    }
}

/// `RecordState`'s clone, written out because `Fields` has no `Clone` (so that `.fields.clone()` stays a `Vec<Value>`).
#[allow(clippy::clone_on_copy)]
impl Clone for crate::value::RecordState {
    fn clone(&self) -> Self {
        crate::value::RecordState {
            table: self.table.clone(),
            temporary: self.temporary.clone(),
            temp_id: self.temp_id.clone(),
            company: self.company.clone(),
            change_company_called: self.change_company_called.clone(),
            crm: self.crm.clone(),
            fields: self.fields.dup(),
            filters: self.filters.clone(),
            filter_group: self.filter_group.clone(),
            marks: self.marks.clone(),
            marked_only: self.marked_only.clone(),
            read_isolation: self.read_isolation.clone(),
            security_filtering: self.security_filtering.clone(),
            declared_security_filtering: self.declared_security_filtering.clone(),
            declared_security_filtering_unpinned: self.declared_security_filtering_unpinned.clone(),
            security_filtering_unpinned: self.security_filtering_unpinned.clone(),
            security_filtering_array: self.security_filtering_array.clone(),
            key: self.key.clone(),
            adhoc: self.adhoc.clone(),
            ascending: self.ascending.clone(),
            field_descending: self.field_descending.clone(),
            position: self.position.clone(),
            auto_calc: self.auto_calc.clone(),
            load: self.load.clone(),
            touched: self.touched.clone(),
            assigned: self.assigned.clone(),
            inserted: self.inserted.clone(),
            dirty_base: self.dirty_base.clone(),
            is_dirty_base: self.is_dirty_base.clone(),
            last_modify: self.last_modify.clone(),
            failed_insert: self.failed_insert.clone(),
            row_origin: self.row_origin.clone(),
            own_read: self.own_read.clone(),
            rolled_read: self.rolled_read.clone(),
            probed_source: self.probed_source.clone(),
            globals: self.globals.clone(),
            unpinned_buffer: self.unpinned_buffer.clone(),
            view_assumption: self.view_assumption.clone(),
            run_page_link: self.run_page_link.clone(),
        }
    }
}

impl<'a> IntoIterator for &'a Fields {
    type Item = &'a Value;
    type IntoIter = std::slice::Iter<'a, Value>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut Fields {
    type Item = &'a mut Value;
    type IntoIter = std::slice::IterMut<'a, Value>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(table: u32) -> [u64; 5] {
        COUNTS.lock().unwrap().get(&table).copied().unwrap_or_default()
    }

    #[test]
    fn off_by_default() {
        // No test in this crate calls `enable`.
        assert!(lazy_line().is_none());
        // An untracked buffer (all buffers while off: al-vm makes tracked ones only when on) counts nothing.
        let mut plain = Fields::from(vec![Value::Integer(1)]);
        plain[0] = Value::Integer(2);
        plain.note_slot_read(0);
        drop(plain.dup());
        drop(plain);
        // Only the ids of the classification test below ever appear.
        assert!(
            COUNTS
                .lock()
                .unwrap()
                .keys()
                .all(|t| (9_000_001..=9_000_007).contains(t))
        );
    }

    #[test]
    fn the_first_event_classifies_the_buffer() {
        // Each case uses its own table id, so the tests do not share counts.
        let blob = || vec![Value::Integer(0), Value::Blob(Default::default())];
        drop(Fields::tracked(blob(), 9_000_001));
        assert_eq!(counts(9_000_001), [1, 0, 1, 0, 0]);
        // Replaced whole, as `r.fields = row.into()` does.
        let mut slot = [Fields::tracked(blob(), 9_000_002)];
        slot[0] = Fields::from(vec![]);
        assert_eq!(counts(9_000_002), [1, 0, 1, 0, 0]);
        let f = Fields::tracked(blob(), 9_000_003);
        f.note_slot_read(0);
        let _ = &f[1];
        drop(f);
        assert_eq!(counts(9_000_003), [1, 0, 0, 1, 0]);
        let f = Fields::tracked(blob(), 9_000_004);
        f.note_slot_read(1);
        drop(f);
        assert_eq!(counts(9_000_004), [1, 0, 0, 0, 1]);
        let mut f = Fields::tracked(blob(), 9_000_005);
        f[0] = Value::Integer(1);
        assert_eq!(counts(9_000_005), [1, 0, 0, 0, 1]);
        let f = Fields::tracked(blob(), 9_000_006);
        let copy = f.dup();
        drop((f, copy));
        assert_eq!(counts(9_000_006), [1, 0, 0, 0, 1]);
        note_untracked(9_000_007);
        let mut m = BTreeMap::new();
        m.insert(9_000_007, counts(9_000_007));
        m.insert(9_000_001, counts(9_000_001));
        assert_eq!(
            line(&m),
            "record-init-lazy: scope=since_start b_split=partial lazy_eligible=2 a=1 b=0 c=1 open=0 c_untracked=1 \
             top=[9000001:1:a=1:b=0:c=0, 9000007:1:a=0:b=0:c=1]"
        );
    }
}
