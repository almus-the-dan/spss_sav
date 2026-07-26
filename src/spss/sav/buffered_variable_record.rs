//! A type-2 variable record held undecoded until the encoding is known.

use crate::spss::sav::dictionary_format::VARIABLE_RECORD_BODY_LEN;

/// A type-2 variable record, structurally delimited but neither
/// validated nor decoded.
///
/// Buffering a record only requires knowing where it ends, which means
/// reading `has_var_label` and `n_missing_values` to size the trailing
/// blocks. Nothing is *checked*: the variable type code, the
/// missing-value count, and the continuation run length are all
/// validated when the record is decoded, so a semantically malformed
/// record still fails from
/// [`read_record`](crate::spss::sav::dictionary_reader::DictionaryReader::read_record)
/// rather than from
/// [`read_header`](crate::spss::sav::header_reader::HeaderReader::read_header).
///
/// Only two fields need the encoding — the 8-byte short name inside
/// `body` and the variable label. Everything else in the record is
/// numeric, and `missing_values` stays raw permanently (see
/// [`RawMissingValues`](crate::spss::sav::raw_missing_values::RawMissingValues)).
#[allow(dead_code)] // populated when the header reader defers decoding.
pub(crate) struct BufferedVariableRecord {
    /// The fixed-width record body, verbatim.
    pub(crate) body: [u8; VARIABLE_RECORD_BODY_LEN],
    /// Raw variable-label bytes, present when `has_var_label` was set.
    /// Padding is trimmed at decode time, not here.
    pub(crate) label: Option<Vec<u8>>,
    /// The raw missing-value slots, `n_missing_values.abs()` of them.
    /// The signed count itself stays in `body`, since its sign selects
    /// between a discrete list and a range.
    pub(crate) missing_values: Vec<[u8; 8]>,
}
