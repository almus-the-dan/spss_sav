//! One undecoded entry from a type-3 value-label record.

/// A single value-label pair, carried verbatim until the encoding is
/// known.
///
/// The counterpart to
/// [`RawValueLabelEntry`](crate::spss::sav::raw_value_label_entry::RawValueLabelEntry),
/// which differs only in holding a decoded `String` label. "Raw" in
/// that type's name refers to the 8-byte *value* staying raw
/// permanently; here it is the *label* that is merely not decoded yet.
#[allow(dead_code)] // populated when the header reader defers decoding.
pub(crate) struct BufferedValueLabelEntry {
    /// Raw 8-byte value key, exactly as it appeared on disk.
    pub(crate) value: [u8; 8],
    /// Raw label bytes, with the length prefix consumed and the
    /// trailing padding still attached.
    pub(crate) label: Vec<u8>,
}
