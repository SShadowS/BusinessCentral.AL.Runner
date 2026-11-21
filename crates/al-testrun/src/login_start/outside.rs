//! Session state outside al-vm's `Session` that al-testrun keeps (PERF-01 step 3, design section 3.3): every `static` of
//! al-testrun, each listed with what a login start does with it, as `crates/al-vm/src/login_image/fields.rs` lists those of
//! al-vm, al-builtins and al-data. PERF-01 step 3 A1: a stub; part (b) writes the list and extends al-test-cli
//! `tests/outside_session_state.rs` to scan al-testrun against it.
