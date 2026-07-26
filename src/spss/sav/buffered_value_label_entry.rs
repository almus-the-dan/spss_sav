//! One undecoded entry from a type-3 value-label record.

use crate::spss::sav::dictionary_format::VALUE_LABEL_VALUE_LEN;

/// A single value-label pair, carried verbatim until the encoding is
/// known.
///
/// The counterpart to
/// [`RawValueLabelEntry`](crate::spss::sav::raw_value_label_entry::RawValueLabelEntry),
/// which differs only in holding a decoded `String` label. "Raw" in
/// that type's name refers to the 8-byte *value* staying raw
/// permanently; here it is the *label* that is merely not decoded yet.
#[derive(Debug)]
pub(crate) struct BufferedValueLabelEntry {
    /// Raw 8-byte value key, exactly as it appeared on disk.
    pub(crate) value: [u8; VALUE_LABEL_VALUE_LEN],
    /// Raw label bytes with the on-disk padding already removed, so
    /// decoding is a straight byte-to-text conversion.
    pub(crate) label: Vec<u8>,
}
