//! The digest of committed rows: the one place the login key hashes rows (al-runner issue 8211; PERF-34, design
//! `/coord/handoff/PERF-01/step4-key-design.md` section 1).
//!
//! Stage 1 hashes each row's `value_codec::encode_row` bytes (al-builtins, the one owner of "a row as bytes") straight
//! into SHA-256 through one reused buffer: no String and no Debug text per row. A value that has no byte form
//! (`value_codec::encode` answers false: a SqlCount, a placeholder's size, a handle) gives [`NoKey`], never a lossy
//! digest, and the session runs its login.
//!
//! **A0, inert:** the functions still hash each row's Debug text, one line per row as step 3 did (`{company}|{object}|{row:?}`,
//! [`digest`] over the lines), so every digest and every key is the one step 3 gave, and no value refuses a key. The
//! counters under `cfg(test)` say which form a digest took.

use super::{LinesDigest, digest};

/// Why a session has no login key: a row holds a value with no byte form, named by its kind (as `Describe` names it).
/// The session then runs its login, counted in the starts line (`login_start::NotTaken::NoKey`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NoKey(pub(crate) &'static str);

/// The digest of the committed rows `rows` of table `object` in `company`, in key order (the per-table digest of the key's
/// `database` part, `key_parts.rs`).
pub(crate) fn table_digest<'a>(
    company: &str,
    object: u32,
    rows: impl Iterator<Item = &'a al_data::Row>,
) -> Result<LinesDigest, NoKey> {
    // A0: the Debug lines of step 3.
    let lines: Vec<String> = rows.map(|row| debug_line(company, object, row)).collect();
    Ok(digest(&lines))
}

/// The digest of every row `store` holds now, the uncommitted changes included (the key's `uncommitted` part).
pub(crate) fn current_digest(store: &al_data::memory::MemStore) -> Result<LinesDigest, NoKey> {
    // A0: the Debug lines of step 3, table after table in the store's order.
    let mut lines = Vec::new();
    for t in store.export_current() {
        lines.extend(t.rows.iter().map(|(_, row)| debug_line(&t.company, t.object, row)));
    }
    Ok(digest(&lines))
}

/// One row as step 3 wrote it into a digest: company, table object and the row with its system fields.
fn debug_line(company: &str, object: u32, row: &al_data::Row) -> String {
    #[cfg(test)]
    counts::DEBUG_ROWS.with(|c| c.set(c.get() + 1));
    format!("{company}|{object}|{row:?}")
}

/// How many rows the digests of this thread wrote as Debug text and as bytes, for tests: stage 1 writes none as Debug text.
#[cfg(test)]
pub(crate) mod counts {
    use std::cell::Cell;

    thread_local! {
        pub(super) static DEBUG_ROWS: Cell<u64> = const { Cell::new(0) };
        pub(super) static ENCODED_ROWS: Cell<u64> = const { Cell::new(0) };
    }

    /// (rows written as Debug text, rows written as bytes) on this thread since the last [`reset`].
    pub(crate) fn rows() -> (u64, u64) {
        (DEBUG_ROWS.with(Cell::get), ENCODED_ROWS.with(Cell::get))
    }

    /// Both counts back to 0.
    pub(crate) fn reset() {
        DEBUG_ROWS.with(|c| c.set(0));
        ENCODED_ROWS.with(|c| c.set(0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use al_builtins::value::Value;

    // A guard that holds before and after stage 1: one content gives one digest, and each row is written once, in one of
    // the two forms.
    #[test]
    fn one_content_gives_one_digest_and_writes_each_row_once() {
        let rows = |n: i32| -> Vec<al_data::Row> {
            (0..n)
                .map(|i| vec![Value::Integer(i), Value::Text("a".into())])
                .collect()
        };
        counts::reset();
        let a = table_digest("C", 7, rows(3).iter());
        let b = table_digest("C", 7, rows(3).iter());
        assert_eq!(a, b);
        assert_ne!(a, table_digest("C", 7, rows(2).iter()));
        let (debug, encoded) = counts::rows();
        assert_eq!(debug + encoded, 8);
    }

    fn rows(n: i32) -> Vec<al_data::Row> {
        (0..n)
            .map(|i| vec![Value::Integer(i), Value::Text("a".into())])
            .collect()
    }

    /// Step 3's digest of the same rows: the Debug lines.
    fn step3_digest(company: &str, object: u32, rows: &[al_data::Row]) -> LinesDigest {
        let lines: Vec<String> = rows.iter().map(|r| format!("{company}|{object}|{r:?}")).collect();
        digest(&lines)
    }

    fn blob_row(tweak: impl Fn(&mut al_builtins::stream::BlobBuf)) -> al_data::Row {
        let b = al_builtins::stream::new_blob();
        al_builtins::stream::write_bytes(&b, b"xy");
        tweak(&mut b.borrow_mut());
        vec![Value::Integer(1), Value::Blob(b)]
    }

    // A3 (RED): a value with no byte form gives no key, never a lossy digest. The stub gives Ok.
    #[test]
    fn a_value_with_no_byte_form_gives_no_key() {
        use al_builtins::value::{SqlCount, SqlForm, SqlType};
        let count = Value::SqlCount(SqlCount {
            n: 3,
            form: SqlForm::Delta,
            ty: SqlType::BigInteger,
        });
        let size = Value::PlaceholderSize("mark".into(), SqlType::Integer);
        for (what, v) in [("SqlCount", count), ("PlaceholderSize", size)] {
            let table = vec![vec![Value::Integer(1)], vec![Value::Integer(2), v.clone()]];
            let got = table_digest("C", 7, table.iter());
            assert!(
                matches!(got, Err(NoKey(kind)) if !kind.is_empty()),
                "{what}: table {got:?}"
            );
            let store = al_data::memory::MemStore::from_exported(vec![exported(table)]);
            let got = current_digest(&store);
            assert!(
                matches!(got, Err(NoKey(kind)) if !kind.is_empty()),
                "{what}: current {got:?}"
            );
        }
    }

    fn exported(rows: Vec<al_data::Row>) -> al_data::memory::ExportedTable {
        al_data::memory::ExportedTable {
            company: "C".into(),
            object: 7,
            pk_storage: Default::default(),
            rows: (rows.into_iter().enumerate())
                .map(|(i, r)| (vec![Value::Integer(i as i32)], r))
                .collect(),
        }
    }

    // A4 (GUARD, probably green today: Debug renders every BlobBuf field): a field of a Blob that differs is another digest.
    #[test]
    fn a_blob_field_difference_gives_another_digest() {
        let digest_of = |r: al_data::Row| table_digest("C", 7, [r].iter()).expect("a digest");
        let base = digest_of(blob_row(|_| {}));
        assert_eq!(base, digest_of(blob_row(|_| {})), "GUARD equal blobs");
        assert_ne!(
            base,
            digest_of(blob_row(|b| b.utf16_binary = true)),
            "GUARD utf16_binary"
        );
        assert_ne!(
            base,
            digest_of(blob_row(|b| b.render_pending = Some("m".into()))),
            "GUARD render_pending"
        );
        assert_ne!(base, digest_of(blob_row(|b| b.bytes = b"xz".to_vec())), "GUARD bytes");
    }

    // A5 (GUARD): one content built twice, from two stores (other stamps, other pointers), gives one digest.
    #[test]
    fn one_content_built_twice_from_two_stores_gives_one_digest() {
        let a = al_data::memory::MemStore::from_exported(vec![exported(rows(4))]);
        let b = al_data::memory::MemStore::from_exported(vec![exported(rows(4))]);
        assert_eq!(current_digest(&a), current_digest(&b), "GUARD");
        assert!(current_digest(&a).is_ok(), "GUARD");
    }

    // A6 (RED): every row is written as bytes and none as Debug text. The stub gives (n, 0).
    #[test]
    fn every_row_of_a_table_digest_is_encoded_and_none_is_debug_rendered() {
        counts::reset();
        let r = rows(5);
        assert!(table_digest("C", 7, r.iter()).is_ok());
        assert_eq!(counts::rows(), (0, 5), "table_digest");
    }

    // A6 (RED), the key's uncommitted part: the same for `current_digest`.
    #[test]
    fn every_row_of_the_current_digest_is_encoded_and_none_is_debug_rendered() {
        counts::reset();
        let store = al_data::memory::MemStore::from_exported(vec![exported(rows(6))]);
        assert!(current_digest(&store).is_ok());
        assert_eq!(counts::rows(), (0, 6), "current_digest");
    }

    // A8 (RED): the digest of the byte form is never step 3's digest of the same rows (a version tag), so a start
    // recorded by another binary can never meet a new key. The stub gives step 3's digest.
    #[test]
    fn the_new_digest_is_never_step_threes() {
        for n in [1, 3] {
            let r = rows(n);
            assert_ne!(
                table_digest("C", 7, r.iter()).unwrap(),
                step3_digest("C", 7, &r),
                "{n} rows"
            );
        }
    }
}
