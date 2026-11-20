//! PERF-22 measurement, throwaway: how often `Session::initial_fields` runs per table, split by whether the planned
//! copy-on-write change could share the vector. Off unless the CLI calls `enable_record_init_count` (ADR-006: this crate
//! reads no environment variable). Off, the cost is one relaxed atomic load per call.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use al_builtins::library_kind::LibraryKind;
use al_builtins::value::Value;

use crate::Session;

static ON: AtomicBool = AtomicBool::new(false);
/// Calls by (table id, reason); the reason is `None` for an eligible call. Every thread counts here.
static COUNTS: Mutex<BTreeMap<(u32, Option<String>), u64>> = Mutex::new(BTreeMap::new());

/// Turns the counter (and the PERF-22b first-access count of al-builtins `fields_track`) on for the whole process.
pub fn enable_record_init_count() {
    ON.store(true, Ordering::Relaxed);
    al_builtins::fields_track::enable();
}

/// The PERF-22b line, cumulative since the process started; `None` while the counter is off.
pub fn record_init_lazy_line() -> Option<String> {
    al_builtins::fields_track::lazy_line()
}

/// The summary line, cumulative since the process started; `None` while the counter is off.
pub fn record_init_count_line() -> Option<String> {
    if !ON.load(Ordering::Relaxed) {
        return None;
    }
    Some(summary_line(&COUNTS.lock().unwrap_or_else(|e| e.into_inner())))
}

fn summary_line(counts: &BTreeMap<(u32, Option<String>), u64>) -> String {
    let total: u64 = counts.values().sum();
    let eligible: u64 = counts.iter().filter(|((_, r), _)| r.is_none()).map(|(_, n)| n).sum();
    let mut ineligible: Vec<(&u32, &String, &u64)> = counts
        .iter()
        .filter_map(|((t, r), n)| r.as_ref().map(|r| (t, r, n)))
        .collect();
    ineligible.sort_by(|a, b| b.2.cmp(a.2).then(a.0.cmp(b.0)));
    let top: Vec<String> = ineligible
        .iter()
        .take(20)
        .map(|(t, r, n)| format!("{t}:{n}:{r}"))
        .collect();
    format!(
        "record-init-count: scope=since_start total={total} eligible={eligible} ineligible={} top_ineligible=[{}]",
        total - eligible,
        top.join(", ")
    )
}

/// Why a fresh record of a table cannot share its initial vector, or `None` when it can: a fresh slot (Blob first,
/// then RecordId, then the first other slot's variant), else an InitValue that `clear_uninstalled_init_values` cleared.
fn ineligible_reason(fields: &[Value], fresh: &[usize], init_value_cleared: bool) -> Option<String> {
    let kinds: Vec<&Value> = fresh.iter().filter_map(|&i| fields.get(i)).collect();
    if kinds.iter().any(|v| matches!(v, Value::Blob(_))) {
        return Some("Blob".into());
    }
    if kinds
        .iter()
        .any(|v| matches!(v, Value::Library(LibraryKind::RecordId, _)))
    {
        return Some("RecordId".into());
    }
    if let Some(v) = kinds.first() {
        return Some(match v {
            Value::Library(k, _) => format!("Library({k:?})"),
            other => {
                let d = format!("{other:?}");
                d.split(['(', ' ', '{']).next().unwrap_or("?").to_string()
            }
        });
    }
    init_value_cleared.then(|| "InitValue".into())
}

/// Lazy-eligible (design.md section 9.2): not Shared-eligible, every fresh slot a Blob, no InitValue cleared.
fn lazy_eligible(fields: &[Value], fresh: &[usize], init_value_cleared: bool) -> bool {
    !fresh.is_empty() && !init_value_cleared && fresh.iter().all(|&i| matches!(fields.get(i), Some(Value::Blob(_))))
}

impl Session {
    /// Counts one `initial_fields` call of `table` (an object index) when the counter is on. Returns the table id when
    /// the buffer is Lazy-eligible (PERF-22b: its fresh slots are all Blobs and no InitValue was cleared), else 0.
    #[inline]
    pub(super) fn count_record_init(&self, table: usize, fields: &[Value], init_value_cleared: bool) -> u32 {
        if !ON.load(Ordering::Relaxed) {
            return 0;
        }
        let fresh = self.initial_cache.get(&table).map(|t| &t.fresh[..]).unwrap_or(&[]);
        let reason = ineligible_reason(fields, fresh, init_value_cleared);
        let id = self.engine.program.objects[table].id.unwrap_or(0);
        let lazy = lazy_eligible(fields, fresh, init_value_cleared);
        *COUNTS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry((id, reason))
            .or_insert(0) += 1;
        if lazy { id.max(1) } else { 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_by_default_nothing_is_counted() {
        // No test in this binary turns the counter on.
        assert!(record_init_count_line().is_none());
        assert!(COUNTS.lock().unwrap().is_empty());
    }

    #[test]
    fn plain_fields_are_eligible_and_fresh_or_cleared_ones_are_not() {
        let plain = vec![Value::Integer(0), Value::Boolean(false)];
        assert_eq!(ineligible_reason(&plain, &[], false), None);
        assert_eq!(ineligible_reason(&plain, &[], true).as_deref(), Some("InitValue"));
        let blob = vec![Value::Integer(0), Value::Blob(Default::default())];
        assert_eq!(ineligible_reason(&blob, &[1], false).as_deref(), Some("Blob"));
        let list = vec![Value::List(Default::default())];
        assert_eq!(ineligible_reason(&list, &[0], false).as_deref(), Some("List"));
        assert!(lazy_eligible(&blob, &[1], false));
        assert!(!lazy_eligible(&blob, &[1], true));
        assert!(!lazy_eligible(&plain, &[], false));
        assert!(!lazy_eligible(&list, &[0], false));
    }

    #[test]
    fn the_line_sums_and_ranks_the_ineligible_tables() {
        let mut m = BTreeMap::new();
        m.insert((50000, None), 7);
        m.insert((50001, Some("Blob".to_string())), 2);
        m.insert((50002, Some("InitValue".to_string())), 5);
        assert_eq!(
            summary_line(&m),
            "record-init-count: scope=since_start total=14 eligible=7 ineligible=7 top_ineligible=[50002:5:InitValue, 50001:2:Blob]"
        );
    }
}
