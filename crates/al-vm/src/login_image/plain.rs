//! A [`Value`] that holds no handle, as owned data any thread may hold (the session image, al-runner issue 8211): the plain
//! mirror of `Value` and of a record filter. It lives with its only user rather than next to `Value` in al-builtins: there,
//! its conversions alone, which no run without the login replay calls, changed how al-builtins' hot functions inline and
//! cost every run about 0.1% more instructions.

use al_builtins::filter::{FieldFilter, FilterExpr};
use al_builtins::text::Txt;
use al_builtins::value::{TextConstFrom, Value};
use rust_decimal::Decimal;
use std::sync::Arc;

/// A value that holds no handle, as owned data any thread may hold. One variant per plain `Value` variant.
#[derive(Debug, Clone)]
pub(crate) enum Plain {
    Null,
    Boolean(bool),
    Integer(i32),
    BigInteger(i64),
    Decimal(Decimal),
    Byte(u8),
    Char(u16),
    Text(Txt),
    TextConst(Txt, TextConstFrom),
    Code(Txt),
    Date(i32, bool),
    Time(i64),
    DateTime(i64),
    Duration(i64),
    Guid([u8; 16]),
    DateFormula(String),
    Option(i32, Arc<[String]>),
    Enum(u32, i32),
    Unknown(Arc<str>),
}

impl Plain {
    /// `None` for a value that holds a handle (a Blob included: its buffer is a node of the arena).
    pub(crate) fn of(v: &Value) -> Option<Plain> {
        let _ = v;
        None
    }

    /// The value this plain value is.
    pub(crate) fn value(&self) -> Value {
        Value::Null
    }
}

/// `FilterExpr` with `Plain` in place of `Value` (filter.rs:10-48), one variant per variant.
#[derive(Debug, Clone)]
pub(crate) enum PlainFilterExpr {
    Range(Option<Plain>, Option<Plain>),
    Eq(Plain),
    RangeSame(Plain),
    Ne(Plain),
    Lt(Plain),
    Le(Plain),
    Gt(Plain),
    Ge(Plain),
    Or(Vec<PlainFilterExpr>),
    And(Vec<PlainFilterExpr>),
    Like(String, bool),
    NotLike(String, bool),
    AtSignAlone(bool),
    EmptyViewValue,
    NumCode(Box<PlainFilterExpr>),
    UnpinnedMatch(Box<PlainFilterExpr>, String),
    UnpinnedText(Box<PlainFilterExpr>),
}

/// A [`FieldFilter`] whose values are plain.
#[derive(Debug, Clone)]
pub(crate) struct PlainFilter {
    pub(crate) lang: i32,
    pub(crate) field: usize,
    pub(crate) expr: PlainFilterExpr,
    pub(crate) group: i32,
}

impl PlainFilter {
    /// `None` for a filter one of whose values holds a handle.
    pub(crate) fn of(f: &FieldFilter) -> Option<PlainFilter> {
        let _ = f;
        None
    }

    /// The filter this plain filter is.
    pub(crate) fn filter(&self) -> FieldFilter {
        FieldFilter {
            lang: self.lang,
            field: self.field,
            expr: FilterExpr::EmptyViewValue,
            group: self.group,
        }
    }
}

#[cfg(test)]
mod tests;
