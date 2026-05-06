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

impl<'a> Value<'a> {
    /// Convenience constructor for a borrowed string cell. Equivalent
    /// to `Value::String(Cow::Borrowed(s))`.
    #[must_use]
    #[inline]
    pub fn string(s: &'a str) -> Self {
        Self::String(Cow::Borrowed(s))
    }
}
