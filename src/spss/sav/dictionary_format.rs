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

/// Length of the `line_count` field that prefixes the document
/// lines in a type-6 record. Read as `u32` in the file's byte order.
#[allow(dead_code)] // exercised once the document reader implementation lands.
pub(super) const DOCUMENT_LINE_COUNT_FIELD_LEN: usize = 4;

/// On-disk width of one document line in a type-6 record. Each
/// line occupies exactly this many bytes, decoded through the
/// file's active encoding; trailing space padding is preserved
/// verbatim on the resulting [`String`] (matching `ReadStat` and
/// the spec convention).
#[allow(dead_code)] // exercised once the document reader implementation lands.
pub(super) const DOCUMENT_LINE_LEN: usize = 80;

/// Length of the `subtype` field that opens a type-7 record body,
/// identifying which extension-record kind follows. Read as `i32`
/// in the file's byte order (the on-disk encoding is signed even
/// though all defined subtypes are positive).
#[allow(dead_code)] // exercised once the extension reader implementation lands.
pub(super) const EXTENSION_SUBTYPE_FIELD_LEN: usize = 4;

/// Length of the `element_size` field in a type-7 record body — the
/// size in bytes of one element of the payload. Read as `u32` in
/// the file's byte order.
#[allow(dead_code)] // exercised once the extension reader implementation lands.
pub(super) const EXTENSION_ELEMENT_SIZE_FIELD_LEN: usize = 4;

/// Length of the `element_count` field in a type-7 record body —
/// the number of elements making up the payload. Total payload
/// length is `element_size * element_count`. Read as `u32` in the
/// file's byte order.
#[allow(dead_code)] // exercised once the extension reader implementation lands.
pub(super) const EXTENSION_ELEMENT_COUNT_FIELD_LEN: usize = 4;

/// Extension subtype 3 — integer-typed environment metadata
/// (version numbers, machine code, floating-point representation,
/// compression code, endianness, character-set code). Per PSPP's
/// system file format documentation and `ReadStat`'s
/// `SAV_RECORD_SUBTYPE_INTEGER_INFO`.
#[allow(dead_code)] // exercised by the subtype-3 parser.
pub(super) const EXTENSION_SUBTYPE_MACHINE_INTEGER_INFO: i32 = 3;

/// `element_size` an extension subtype-3 record must declare. Each
/// field is one `i32`.
#[allow(dead_code)] // exercised by the subtype-3 parser.
pub(super) const MACHINE_INTEGER_INFO_ELEMENT_SIZE: u32 = 4;

/// `element_count` an extension subtype-3 record must declare. The
/// payload is exactly eight `i32` fields, in this fixed order:
/// `version_major`, `version_minor`, `version_revision`,
/// `machine_code`, `floating_point_representation`,
/// `compression_code`, `endianness`, `character_code`.
#[allow(dead_code)] // exercised by the subtype-3 parser.
pub(super) const MACHINE_INTEGER_INFO_ELEMENT_COUNT: u32 = 8;

/// Tagged code for IEEE 754 in subtype-3's
/// `floating_point_representation` field.
#[allow(dead_code)] // exercised by the subtype-3 parser.
pub(super) const FLOATING_POINT_REPRESENTATION_IEEE: i32 = 1;

/// Tagged code for IBM hexadecimal floating-point in subtype-3's
/// `floating_point_representation` field.
#[allow(dead_code)] // exercised by the subtype-3 parser.
pub(super) const FLOATING_POINT_REPRESENTATION_IBM_HFP: i32 = 2;

/// Tagged code for VAX floating-point in subtype-3's
/// `floating_point_representation` field.
#[allow(dead_code)] // exercised by the subtype-3 parser.
pub(super) const FLOATING_POINT_REPRESENTATION_VAX: i32 = 3;

/// Tagged code for big-endian byte order in subtype-3's
/// `endianness` field.
#[allow(dead_code)] // exercised by the subtype-3 parser.
pub(super) const ENDIANNESS_BIG_ENDIAN: i32 = 1;

/// Tagged code for little-endian byte order in subtype-3's
/// `endianness` field.
#[allow(dead_code)] // exercised by the subtype-3 parser.
pub(super) const ENDIANNESS_LITTLE_ENDIAN: i32 = 2;

/// Extension subtype 4 — float sentinel values (system missing,
/// highest, lowest), each carried as 8 raw bytes in the file's
/// declared float format.
#[allow(dead_code)] // exercised by the subtype-4 parser.
pub(super) const EXTENSION_SUBTYPE_FLOAT_INFO: i32 = 4;

/// `element_size` an extension subtype-4 record must declare. Each
/// sentinel is 8 bytes.
#[allow(dead_code)] // exercised once the subtype-4 parser lands.
pub(super) const FLOAT_SENTINELS_ELEMENT_SIZE: u32 = 8;

/// `element_count` an extension subtype-4 record must declare. The
/// payload is three sentinels: system missing, highest, lowest.
#[allow(dead_code)] // exercised once the subtype-4 parser lands.
pub(super) const FLOAT_SENTINELS_ELEMENT_COUNT: u32 = 3;

/// Byte offset of the system-missing sentinel within a subtype-4
/// payload.
#[allow(dead_code)] // exercised once the subtype-4 parser lands.
pub(super) const FLOAT_SENTINELS_SYSTEM_MISSING_OFFSET: usize = 0;

/// Byte offset of the `HIGHEST` sentinel within a subtype-4
/// payload.
#[allow(dead_code)] // exercised once the subtype-4 parser lands.
pub(super) const FLOAT_SENTINELS_HIGHEST_OFFSET: usize = 8;

/// Byte offset of the `LOWEST` sentinel within a subtype-4 payload.
#[allow(dead_code)] // exercised by the subtype-4 parser.
pub(super) const FLOAT_SENTINELS_LOWEST_OFFSET: usize = 16;

/// Extension subtype 16 — extended number of cases. Authoritative
/// when the header's `case_count` field is `-1` (used for files
/// with more than `i32::MAX` cases). The payload is two `i64`
/// fields: a version flag (always `1` in `ReadStat`'s writer) and
/// the case count itself.
#[allow(dead_code)] // exercised by the subtype-16 parser.
pub(super) const EXTENSION_SUBTYPE_EXTENDED_NUMBER_OF_CASES: i32 = 16;

/// `element_size` an extension subtype-16 record must declare.
/// Each field is one `i64` = 8 bytes.
#[allow(dead_code)] // exercised by the subtype-16 parser.
pub(super) const EXTENDED_NUMBER_OF_CASES_ELEMENT_SIZE: u32 = 8;

/// `element_count` an extension subtype-16 record must declare.
/// The payload is exactly two `i64`s (version flag + count).
#[allow(dead_code)] // exercised by the subtype-16 parser.
pub(super) const EXTENDED_NUMBER_OF_CASES_ELEMENT_COUNT: u32 = 2;

/// Byte offset of the version-flag `i64` within a subtype-16
/// payload.
#[allow(dead_code)] // exercised by the subtype-16 parser.
pub(super) const EXTENDED_NUMBER_OF_CASES_VERSION_OFFSET: usize = 0;

/// Byte offset of the case-count `i64` within a subtype-16
/// payload.
#[allow(dead_code)] // exercised by the subtype-16 parser.
pub(super) const EXTENDED_NUMBER_OF_CASES_COUNT_OFFSET: usize = 8;

/// Extension subtype 11 — per-variable display parameters
/// (measurement level, optional display width, alignment). The
/// payload is a fixed-`element_size`-of-4 stream of `u32` values
/// holding either 2 or 3 values per variable (the choice depends on
/// the writer and is recovered at schema finalization by comparing
/// `element_count` against the dictionary's variable count). Per
/// PSPP's system file format documentation and `ReadStat`'s
/// `SAV_RECORD_SUBTYPE_VAR_DISPLAY`.
#[allow(dead_code)] // exercised by the subtype-11 parser.
pub(super) const EXTENSION_SUBTYPE_DISPLAY_PARAMETERS: i32 = 11;

/// `element_size` an extension subtype-11 record must declare. Each
/// element is a `u32`.
#[allow(dead_code)] // exercised by the subtype-11 parser.
pub(super) const DISPLAY_PARAMETERS_ELEMENT_SIZE: u32 = 4;

/// Extension subtype 13 — long-variable-name mappings. The payload
/// is a fixed-`element_size`-of-1, variable-`element_count` byte
/// stream of `short=long` pairs joined by [`LONG_VARIABLE_NAMES_PAIR_SEPARATOR`]
/// (a tab byte), with [`LONG_VARIABLE_NAMES_KEY_VALUE_SEPARATOR`]
/// (`=`) between each pair's two halves. A trailing separator is
/// permitted. Per PSPP's system file format documentation and
/// `ReadStat`'s `SAV_RECORD_SUBTYPE_LONG_VAR_NAME`.
#[allow(dead_code)] // exercised by the subtype-13 parser.
pub(super) const EXTENSION_SUBTYPE_LONG_VARIABLE_NAMES: i32 = 13;

/// `element_size` an extension subtype-13 record must declare. Each
/// element is one byte of the `short=long\tshort=long...` stream.
#[allow(dead_code)] // exercised by the subtype-13 parser.
pub(super) const LONG_VARIABLE_NAMES_ELEMENT_SIZE: u32 = 1;

/// Byte separator between adjacent `short=long` pairs in a
/// subtype-13 payload (a literal tab).
#[allow(dead_code)] // exercised by the subtype-13 parser.
pub(super) const LONG_VARIABLE_NAMES_PAIR_SEPARATOR: u8 = b'\t';

/// Byte separator between a pair's short and long halves in a
/// subtype-13 payload (a literal `=`).
#[allow(dead_code)] // exercised by the subtype-13 parser.
pub(super) const LONG_VARIABLE_NAMES_KEY_VALUE_SEPARATOR: u8 = b'=';

/// Extension subtype 14 — very-long-string widths. The payload is a
/// fixed-`element_size`-of-1, variable-`element_count` byte stream
/// of `short=width` pairs joined by
/// [`VERY_LONG_STRINGS_PAIR_SEPARATOR`] (a tab byte), with
/// [`VERY_LONG_STRINGS_KEY_VALUE_SEPARATOR`] (`=`) between each
/// pair's two halves and the width written as ASCII decimal digits.
/// SPSS terminates each pair with a NUL
/// ([`VERY_LONG_STRINGS_PAIR_PADDING`]) before the tab; `ReadStat`'s
/// grammar accepts any run of NULs there and an optional trailing
/// separator. Per PSPP's system file format documentation and
/// `ReadStat`'s `SAV_RECORD_SUBTYPE_VERY_LONG_STR`.
#[allow(dead_code)] // exercised by the subtype-14 parser.
pub(super) const EXTENSION_SUBTYPE_VERY_LONG_STRINGS: i32 = 14;

/// `element_size` an extension subtype-14 record must declare. Each
/// element is one byte of the `short=width\0\tshort=width...`
/// stream.
#[allow(dead_code)] // exercised by the subtype-14 parser.
pub(super) const VERY_LONG_STRINGS_ELEMENT_SIZE: u32 = 1;

/// Byte separator between adjacent `short=width` pairs in a
/// subtype-14 payload (a literal tab).
#[allow(dead_code)] // exercised by the subtype-14 parser.
pub(super) const VERY_LONG_STRINGS_PAIR_SEPARATOR: u8 = b'\t';

/// Byte separator between a pair's short-name and width halves in a
/// subtype-14 payload (a literal `=`).
#[allow(dead_code)] // exercised by the subtype-14 parser.
pub(super) const VERY_LONG_STRINGS_KEY_VALUE_SEPARATOR: u8 = b'=';

/// NUL padding SPSS writes between a pair's width digits and the
/// following [`VERY_LONG_STRINGS_PAIR_SEPARATOR`].
#[allow(dead_code)] // exercised by the subtype-14 parser.
pub(super) const VERY_LONG_STRINGS_PAIR_PADDING: u8 = 0;

/// Extension subtype 20 — the file's declared character encoding
/// name. The payload is a fixed-`element_size`-of-1, variable-
/// `element_count` byte string carrying the encoding label in ASCII
/// (e.g., `"UTF-8"`, `"windows-1252"`). Per PSPP's system file
/// format documentation; `ReadStat` defines the same subtype number
/// (`SAV_RECORD_SUBTYPE_CHAR_ENCODING = 20`) but does not parse the
/// payload — it falls back to subtype 3's numeric `character_code`
/// instead.
#[allow(dead_code)] // exercised by the subtype-20 parser.
pub(super) const EXTENSION_SUBTYPE_CHARACTER_ENCODING: i32 = 20;

/// `element_size` an extension subtype-20 record must declare. Each
/// element is one byte of the encoding name.
#[allow(dead_code)] // exercised by the subtype-20 parser.
pub(super) const CHARACTER_ENCODING_ELEMENT_SIZE: u32 = 1;

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
