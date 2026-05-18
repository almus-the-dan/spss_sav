//! On-disk byte layout of the SAV dictionary section.
//!
//! Pure-data constants describing the record-type tags and the byte
//! layouts of each dictionary record kind. No I/O. Both the sync and
//! async dictionary readers and writers share these.
//!
//! The dictionary section is a stream of typed records freely
//! interleaved between the file header and the
//! [`RECORD_TYPE_DICTIONARY_TERMINATOR`] sentinel: variable records
//! (type 2), value-label pairs (type 3 + type 4), document records
//! (type 6), and extension records (type 7).

/// Type-2 record: a single variable's wire-level metadata.
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const RECORD_TYPE_VARIABLE: i32 = 2;

/// Type-3 record: the values and labels of a value-label set. Always
/// followed immediately by a [`RECORD_TYPE_VALUE_LABEL_VARIABLES`]
/// record naming the affected variables; the dictionary reader pairs
/// the two and yields a single
/// [`DictionaryRecord::ValueLabelSet`](crate::spss::sav::dictionary_record::DictionaryRecord::ValueLabelSet).
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const RECORD_TYPE_VALUE_LABEL: i32 = 3;

/// Type-4 record: the variable indices a preceding type-3 record
/// applies to.
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const RECORD_TYPE_VALUE_LABEL_VARIABLES: i32 = 4;

/// Type-6 record: free-text document lines.
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const RECORD_TYPE_DOCUMENT: i32 = 6;

/// Type-7 record: an extension record (a.k.a. "info" record),
/// carrying a 4-byte subtype, a 4-byte element size, and a 4-byte
/// element count before its payload.
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const RECORD_TYPE_EXTENSION: i32 = 7;

/// Type-999 record: terminates the dictionary section. The 4-byte
/// tag is followed by a 4-byte filler that must be consumed before
/// the data-record section begins.
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const RECORD_TYPE_DICTIONARY_TERMINATOR: i32 = 999;

/// Length of the trailing filler after a
/// [`RECORD_TYPE_DICTIONARY_TERMINATOR`] tag, in bytes.
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const DICTIONARY_TERMINATOR_FILLER_LEN: usize = 4;

/// Total length of a variable record's body (i.e., the bytes after
/// the leading 4-byte record-type tag).
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const VARIABLE_RECORD_BODY_LEN: usize = 28;

/// Byte offset of the i32 `type` field within the variable record
/// body.
///
/// Decodes to one of:
/// * [`VARIABLE_TYPE_CONTINUATION`] (`-1`) — extends the previous
///   variable's storage by one 8-byte segment,
/// * [`VARIABLE_TYPE_NUMERIC`] (`0`) — numeric variable,
/// * `1..=`[`VARIABLE_TYPE_STRING_MAX`] (`255`) — string of that
///   width in bytes.
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const VARIABLE_TYPE_OFFSET: usize = 0;

/// Byte offset of the i32 `has_label` flag (`0` or `1`).
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const VARIABLE_HAS_LABEL_OFFSET: usize = 4;

/// Byte offset of the i32 `n_missing_values` field. Encodes
/// `-`[`MISSING_VALUE_COUNT_MAX`]`..=`[`MISSING_VALUE_COUNT_MAX`]:
/// positive values give the discrete count; `-2` is a low/high
/// range; `-3` is a range plus one discrete value.
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const VARIABLE_MISSING_VALUE_COUNT_OFFSET: usize = 8;

/// Byte offset of the 4-byte print-format code.
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const VARIABLE_PRINT_FORMAT_OFFSET: usize = 12;

/// Byte offset of the 4-byte write-format code.
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const VARIABLE_WRITE_FORMAT_OFFSET: usize = 16;

/// Byte offset of the 8-byte short-name field. Decoded through the
/// reader's active encoding and trimmed of trailing spaces and NULs.
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const VARIABLE_SHORT_NAME_OFFSET: usize = 20;

/// Length of the short-name field, in bytes.
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const VARIABLE_SHORT_NAME_LEN: usize = 8;

/// Sentinel value in the variable record's `type` field marking a
/// continuation of the previous logical variable.
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const VARIABLE_TYPE_CONTINUATION: i32 = -1;

/// Sentinel value in the variable record's `type` field marking a
/// numeric variable.
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const VARIABLE_TYPE_NUMERIC: i32 = 0;

/// Maximum width (in bytes) of a string variable encodable in a
/// single variable record. Widths above this need the very-long-
/// string extension record (subtype 14).
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const VARIABLE_TYPE_STRING_MAX: i32 = 255;

/// Maximum absolute value of the variable record's
/// `n_missing_values` field.
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const MISSING_VALUE_COUNT_MAX: i32 = 3;

/// Size of one wire-level missing value entry (numeric f64 or
/// 8-byte string), in bytes.
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const MISSING_VALUE_ENTRY_LEN: usize = 8;

/// Length of the i32 `label_len` field that prefixes the variable
/// label bytes.
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const VARIABLE_LABEL_LENGTH_FIELD_LEN: usize = 4;

/// Alignment (in bytes) of the variable-label data block. The
/// declared `label_len` is padded up to the next multiple of this.
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const VARIABLE_LABEL_PADDING: usize = 4;

/// Length of the `label_count` field that prefixes the value-label
/// entries in a type-3 record. Read as `u32` in the file's byte order.
#[allow(dead_code)] // exercised once the value-label reader implementation lands.
pub(super) const VALUE_LABEL_COUNT_FIELD_LEN: usize = 4;

/// Length of the `value` field at the start of each value-label
/// entry. Holds either an `f64` (numeric variable) or padded byte
/// string (string variable) — the reader cannot tell which until the
/// paired type-4 record ties the set to one or more variables.
#[allow(dead_code)] // exercised once the value-label reader implementation lands.
pub(super) const VALUE_LABEL_VALUE_LEN: usize = 8;

/// Length of the `label_len` byte that prefixes the label string.
/// The label content plus this length byte are padded together to a
/// multiple of [`VALUE_LABEL_ENTRY_ALIGNMENT`].
#[allow(dead_code)] // exercised once the value-label reader implementation lands.
pub(super) const VALUE_LABEL_LABEL_LEN_FIELD_LEN: usize = 1;

/// Alignment (in bytes) of the (length-byte + label) portion of a
/// value-label entry. The declared `label_len`, plus its 1-byte
/// header, is padded up to the next multiple of this.
#[allow(dead_code)] // exercised once the value-label reader implementation lands.
pub(super) const VALUE_LABEL_ENTRY_ALIGNMENT: usize = 8;

/// Length of the `variable_count` field that prefixes the
/// variable-index list in a type-4 record. Read as `u32` in the
/// file's byte order.
#[allow(dead_code)] // exercised once the value-label reader implementation lands.
pub(super) const VALUE_LABEL_VARIABLE_COUNT_FIELD_LEN: usize = 4;

/// Length of one variable-index entry in a type-4 record. The
/// index is the 1-based physical position of a variable in the
/// dictionary section; the reader normalizes it to a 0-based logical
/// index by mapping through the primary-variable positions it has
/// recorded so far.
#[allow(dead_code)] // exercised once the value-label reader implementation lands.
pub(super) const VALUE_LABEL_VARIABLE_INDEX_LEN: usize = 4;

/// Byte position of the decimals byte within a 4-byte format code
/// (after reduction to native byte order).
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const FORMAT_CODE_DECIMALS_BYTE: usize = 0;

/// Byte position of the width byte within a 4-byte format code.
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const FORMAT_CODE_WIDTH_BYTE: usize = 1;

/// Byte position of the kind byte within a 4-byte format code.
#[allow(dead_code)] // exercised once the dictionary reader implementation lands.
pub(super) const FORMAT_CODE_KIND_BYTE: usize = 2;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_record_body_offsets_tile_exactly() {
        assert_eq!(VARIABLE_TYPE_OFFSET, 0);
        assert_eq!(VARIABLE_TYPE_OFFSET + 4, VARIABLE_HAS_LABEL_OFFSET);
        assert_eq!(
            VARIABLE_HAS_LABEL_OFFSET + 4,
            VARIABLE_MISSING_VALUE_COUNT_OFFSET
        );
        assert_eq!(
            VARIABLE_MISSING_VALUE_COUNT_OFFSET + 4,
            VARIABLE_PRINT_FORMAT_OFFSET
        );
        assert_eq!(
            VARIABLE_PRINT_FORMAT_OFFSET + 4,
            VARIABLE_WRITE_FORMAT_OFFSET
        );
        assert_eq!(VARIABLE_WRITE_FORMAT_OFFSET + 4, VARIABLE_SHORT_NAME_OFFSET);
        assert_eq!(
            VARIABLE_SHORT_NAME_OFFSET + VARIABLE_SHORT_NAME_LEN,
            VARIABLE_RECORD_BODY_LEN
        );
    }
}
