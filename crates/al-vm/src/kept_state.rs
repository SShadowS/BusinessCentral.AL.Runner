//! What this crate keeps beyond a run, read for the daemon clean check (al-testrun `daemon_state.rs`, al-runner issue
//! 8180). Report only: nothing here changes what it reads.

use crate::Engine;

impl Engine {
    /// The value of type `T` [`Engine::derived`] holds, without making one when there is none.
    pub fn derived_if_made<T: std::any::Any + Send + Sync>(&self) -> Option<std::sync::Arc<T>> {
        let slots = self.derived.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let slot = slots.get(&std::any::TypeId::of::<T>())?.clone();
        drop(slots);
        slot.downcast::<T>().ok()
    }
}

/// This crate's process-wide flags a run sets and nothing clears (each spares a guard until the first value it guards
/// exists): a later request of the process runs with them set.
pub fn process_flag_lines() -> Vec<String> {
    vec![format!("streams kept {}", crate::temp_blob_share::streams_kept())]
}
