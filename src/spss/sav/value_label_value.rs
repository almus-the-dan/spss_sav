//! Typed key for a value-label entry.

use encoding_rs::Encoding;

use crate::spss::sav::sav_error::Result;

/// The 8-byte key of a value-label entry.
///
/// SAV value-label keys are always eight bytes on disk: an `f64`
/// for numeric variables, or a fixed eight-byte string slot
/// (right-padded with spaces) for short-string variables.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValueLabelValue {
    /// Numeric key.
    Numeric(f64),
    /// String key — eight raw bytes from the file, in the file's
    /// declared encoding.
    String([u8; 8]),
}

impl ValueLabelValue {
    /// Constructs a [`String`](Self::String) key by encoding `s` with
    /// `encoding`. The encoded bytes are right-padded with spaces
    /// (`0x20`) to fill the eight-byte slot.
    ///
    /// # Errors
    ///
    /// - [`SavError::StringTooLong`] when the encoded form exceeds
    ///   eight bytes.
    /// - [`SavError::InvalidEncoding`] when `s` contains characters
    ///   that cannot be represented in `encoding`.
    ///
    /// [`SavError::StringTooLong`]: crate::spss::sav::sav_error::SavError::StringTooLong
    /// [`SavError::InvalidEncoding`]: crate::spss::sav::sav_error::SavError::InvalidEncoding
    pub fn from_str(s: &str, encoding: &'static Encoding) -> Result<Self> {
        let _ = (s, encoding);
        todo!()
    }
}
