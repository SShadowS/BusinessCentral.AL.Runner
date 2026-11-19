//! What the internal state checks share (`login_replay.rs`, al-runner issue 8211; `daemon_state.rs`, issue 8180): the
//! committed rows of a store as lines, the first difference of two line lists, and the tally of compared states by kind of
//! difference. Each check names its own kinds; this module only counts them.

/// Every committed row of `store`, one line each: company, table object and the row with its system fields.
pub(crate) fn committed_rows(store: &al_data::memory::MemStore) -> Vec<String> {
    let tables = store.export_committed().unwrap_or_default();
    let mut out = Vec::new();
    for t in &tables {
        for (_, row) in &t.rows {
            out.push(format!("{}|{}|{}", t.company, t.object, al_vm::state_content(row)));
        }
    }
    out
}

/// The first line where `x` and `y` differ, as `<what> <n>: <x's line> | <y's line>` (n counts from 1; `(none)` past the
/// end of a list); `None` when they are the same.
pub(crate) fn line_difference(x: &[String], y: &[String], what: &str) -> Option<String> {
    let n = x
        .iter()
        .zip(y)
        .position(|(p, q)| p != q)
        .unwrap_or(x.len().min(y.len()));
    (x.len() != y.len() || n < x.len()).then(|| {
        format!(
            "{what} {}: {} | {}",
            n + 1,
            x.get(n).map_or("(none)", String::as_str),
            y.get(n).map_or("(none)", String::as_str)
        )
    })
}

/// The values of two states that have no comparable form (`al_vm::StateLines::opaque`), joined: `not comparable: <why>;
/// ...`. Such a value is never known to be the same on both sides, so either side holding one is a difference.
pub(crate) fn not_comparable(a: &[String], b: &[String]) -> Option<String> {
    let opaque: std::collections::BTreeSet<&String> = a.iter().chain(b).collect();
    (!opaque.is_empty()).then(|| {
        let what: Vec<&str> = opaque.into_iter().map(String::as_str).collect();
        format!("not comparable: {}", what.join("; "))
    })
}

/// The compared states of a check: how many, how many differ, per kind how many differ in it, and the first difference.
#[derive(Default, Clone)]
pub(crate) struct Tally {
    pub(crate) compared: u64,
    pub(crate) differ: u64,
    /// Per kind (the index into the check's kinds), the compared states that differ in it.
    differ_by: Vec<u64>,
    first_difference: Option<String>,
}

impl Tally {
    /// Counts one compared state whose differences are `differs`: (kind index, what differs), in the check's order of
    /// kinds. One kind does not hide another.
    pub(crate) fn note(&mut self, differs: &[(usize, String)]) {
        self.compared += 1;
        if let Some((_, what)) = differs.first() {
            self.differ += 1;
            self.first_difference.get_or_insert(what.clone());
        }
        for (k, _) in differs {
            if self.differ_by.len() <= *k {
                self.differ_by.resize(k + 1, 0);
            }
            self.differ_by[*k] += 1;
        }
    }

    /// `<compared> compared, <differ> differ`, and when one differs, the count per kind of `kinds` and the first difference.
    pub(crate) fn counts(&self, kinds: &[&str]) -> String {
        let by: Vec<String> = kinds
            .iter()
            .enumerate()
            .map(|(k, kind)| format!("{kind} {}", self.differ_by.get(k).copied().unwrap_or(0)))
            .collect();
        format!(
            "{} compared, {} differ{}",
            self.compared,
            self.differ,
            self.first_difference
                .as_ref()
                .map_or(String::new(), |w| format!(" ({}; first: {w})", by.join(", ")))
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(l: &[&str]) -> Vec<String> {
        l.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_first_differing_line_is_named_with_both_sides() {
        assert_eq!(line_difference(&lines(&["a"]), &lines(&["a"]), "row"), None);
        assert_eq!(
            line_difference(&lines(&["a", "b"]), &lines(&["a", "c"]), "row").as_deref(),
            Some("row 2: b | c")
        );
        assert_eq!(
            line_difference(&lines(&["a"]), &lines(&["a", "c"]), "row").as_deref(),
            Some("row 2: (none) | c")
        );
    }

    #[test]
    fn a_tally_counts_each_kind_and_keeps_the_first_difference() {
        let mut t = Tally::default();
        t.note(&[]);
        assert_eq!(t.counts(&["a", "b"]), "1 compared, 0 differ");
        t.note(&[(1, "b 1: x | y".into())]);
        t.note(&[(0, "a 1: p | q".into()), (1, "b 2: x | z".into())]);
        assert_eq!(
            t.counts(&["a", "b"]),
            "3 compared, 2 differ (a 1, b 2; first: b 1: x | y)"
        );
    }
}
