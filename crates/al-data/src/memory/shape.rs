//! What a store holds besides the contents of its rows, as a replay of the login checks it before it puts a row delta
//! on the store (al-vm `login_image`, `BaseShape`, al-runner issue 8211): the committed table entries with their row
//! counts (an empty entry is not an absent table, and a row delta changes a table row by row only where it has one),
//! and whether an access watch is on (it would see the replay's writes).

use super::MemStore;

impl MemStore {
    /// (company, table object, rows) of every committed table entry, sorted. Marks nothing as read.
    pub fn committed_entries(&self) -> Vec<(String, u32, usize)> {
        let mut out: Vec<(String, u32, usize)> = (self.committed.iter())
            .map(|((company, object), t)| (company.to_string(), *object, t.len()))
            .collect();
        out.sort();
        out
    }

    /// Whether an access watch is on (`MemStore::watch`, `MemStore::watch_window`).
    pub fn watching(&self) -> bool {
        self.watch.is_some()
    }
}
