//! A single SAV cell value.

use std::borrow::Cow;

use crate::spss::numeric::Numeric;

/// A single cell value from the data section of a SAV file.
///
/// The [`Numeric`](Self::Numeric) variant uses [`Numeric`] to
/// distinguish present `f64` data from missing-value designations.
/// The [`String`](Self::String) variant carries a [`Cow<'a, str>`] —
/// borrowed from the reader's row buffer when the bytes are already
/// valid UTF-8 (zero-copy path), owned when the declared encoding
/// required transcoding.
#[derive(Debug, Clone, PartialEq)]
pub enum Value<'a> {
    /// A numeric cell.
    Numeric(Numeric),
    /// A string cell, decoded using the file's encoding.
    String(Cow<'a, str>),
}

impl<'a> From<&'a str> for Value<'a> {
    #[inline]
    fn from(s: &'a str) -> Self {
        Self::String(Cow::Borrowed(s))
    }
}

impl From<Numeric> for Value<'_> {
    #[inline]
    fn from(value: Numeric) -> Self {
        Self::Numeric(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str_borrows_input() {
        let v = Value::from("hello");
        match v {
            Value::String(Cow::Borrowed(b)) => assert_eq!(b, "hello"),
            _ => panic!("expected Cow::Borrowed"),
        }
    }

    #[test]
    fn from_str_equality() {
        assert_eq!(Value::from("hi"), Value::String(Cow::Borrowed("hi")));
    }

    #[test]
    fn into_works_with_type_inference() {
        let v: Value<'_> = "x".into();
        assert_eq!(v, Value::String(Cow::Borrowed("x")));
    }
}
