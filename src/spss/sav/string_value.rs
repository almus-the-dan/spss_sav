//! A string cell's value: raw bytes, decoded on demand.

use std::borrow::Cow;

use encoding_rs::Encoding;

/// The value of a string cell.
///
/// Carries the cell's bytes with their trailing padding already
/// stripped, plus the encoding needed to read them as text. Both halves
/// are public surface on purpose:
///
/// - [`text`](Self::text) is what most callers want, and decodes
///   through the file's encoding.
/// - [`raw`](Self::raw) is what a caller needs to match the cell
///   against its own metadata. A variable's missing values
///   ([`MissingValueSpecification::String`](crate::spss::sav::missing_value_specification::MissingValueSpecification::String))
///   and its long-string value-label keys
///   ([`ValueLabelValue::LongString`](crate::spss::sav::value_label_value::ValueLabelValue::LongString))
///   are deliberately kept as raw bytes, because a key that is not
///   valid in the declared encoding still has to compare equal to the
///   cell holding it. Without `raw`, every such lookup would have to
///   re-encode the decoded text and hope the round trip was faithful.
///
/// # Borrowing
///
/// The bytes borrow the reader's row buffer for the common case of a
/// variable stored in one segment, and are owned when the value had to
/// be reassembled from the several segments of a very long string. The
/// borrow ends the next time a record is read from the parent reader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringValue<'a> {
    raw: Cow<'a, [u8]>,
    encoding: &'static Encoding,
}

impl<'a> StringValue<'a> {
    /// Wraps `raw` as a string cell read through `encoding`.
    pub(crate) fn new(raw: Cow<'a, [u8]>, encoding: &'static Encoding) -> Self {
        Self { raw, encoding }
    }

    /// The cell's bytes, with trailing spaces and NULs already
    /// stripped, exactly as they appear on disk.
    ///
    /// Compare against these rather than against [`text`](Self::text)
    /// when matching a cell to a value label or a missing value; see
    /// the type-level docs for why those keys stay undecoded.
    #[must_use]
    #[inline]
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }

    /// The encoding the cell's bytes are written in.
    #[must_use]
    #[inline]
    pub fn encoding(&self) -> &'static Encoding {
        self.encoding
    }

    /// The cell decoded to text.
    ///
    /// Borrows when the bytes are already valid UTF-8 and the file's
    /// encoding is UTF-8, and allocates otherwise. Bytes that are not
    /// valid in the declared encoding become U+FFFD rather than an
    /// error, matching how the dictionary reader treats malformed text.
    ///
    /// Decodes on every call — hold the result if you need it twice.
    #[must_use]
    #[inline]
    pub fn text(&self) -> Cow<'_, str> {
        let (text, _, _) = self.encoding.decode(&self.raw);
        text
    }

    /// Takes ownership of the cell's bytes so the value outlives the
    /// row buffer it was read from.
    #[must_use]
    pub fn into_owned(self) -> StringValue<'static> {
        StringValue {
            raw: Cow::Owned(self.raw.into_owned()),
            encoding: self.encoding,
        }
    }
}

impl<'a> From<&'a str> for StringValue<'a> {
    /// Wraps borrowed UTF-8 text as a string cell. Provided for
    /// building expected values in tests and for callers assembling a
    /// record by hand; a cell read from a file gets the file's own
    /// encoding instead.
    #[inline]
    fn from(text: &'a str) -> Self {
        Self::new(Cow::Borrowed(text.as_bytes()), encoding_rs::UTF_8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_returns_the_undecoded_bytes() {
        let value = StringValue::new(Cow::Borrowed(b"alpha"), encoding_rs::WINDOWS_1252);
        assert_eq!(value.raw(), b"alpha");
        assert_eq!(value.text(), "alpha");
    }

    /// The case `raw` exists for: bytes that decode to something whose
    /// re-encoding would not be byte-identical still compare exactly.
    #[test]
    fn raw_survives_an_encoding_that_text_would_not_round_trip() {
        // 0x81 is unassigned in windows-1252, so decoding is lossy.
        let value = StringValue::new(Cow::Borrowed(b"a\x81b"), encoding_rs::WINDOWS_1252);
        assert_eq!(value.raw(), b"a\x81b");
        assert_ne!(value.text().as_bytes(), value.raw());
    }

    #[test]
    fn text_decodes_through_the_declared_encoding() {
        // windows-1252 0xE9 = é
        let value = StringValue::new(Cow::Borrowed(b"caf\xE9"), encoding_rs::WINDOWS_1252);
        assert_eq!(value.text(), "café");
    }

    #[test]
    fn from_str_borrows_and_reads_back_as_utf8() {
        let value = StringValue::from("hello");
        assert_eq!(value.raw(), b"hello");
        assert_eq!(value.text(), "hello");
        assert_eq!(value.encoding(), encoding_rs::UTF_8);
    }

    #[test]
    fn into_owned_detaches_from_the_source_buffer() {
        let owned = {
            let buffer = b"beta".to_vec();
            StringValue::new(Cow::Borrowed(&buffer), encoding_rs::UTF_8).into_owned()
        };
        assert_eq!(owned.raw(), b"beta");
    }

    /// Equality is byte equality, so a borrowed and an owned value with
    /// the same content compare equal.
    #[test]
    fn borrowed_and_owned_compare_equal() {
        let borrowed = StringValue::new(Cow::Borrowed(b"x"), encoding_rs::UTF_8);
        let owned = StringValue::new(Cow::Owned(b"x".to_vec()), encoding_rs::UTF_8);
        assert_eq!(borrowed, owned);
    }
}
