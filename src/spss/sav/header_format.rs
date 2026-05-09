//! On-disk byte layout of the 176-byte SAV file header.
//!
//! Pure-data constants describing where each field sits in the
//! header record. No I/O. Both the sync and async header readers
//! and writers share these.

/// Total size of the SAV header record, in bytes.
#[allow(dead_code)] // exercised by the tile-coverage test and once the writer lands.
pub(super) const HEADER_LEN: usize = 176;

/// Magic bytes for an uncompressed or bytecode-compressed SAV file.
pub(super) const MAGIC_FL2: &[u8; 4] = b"$FL2";

/// Magic bytes for a ZLIB-compressed SAV (ZSAV) file.
pub(super) const MAGIC_FL3: &[u8; 4] = b"$FL3";

/// Byte offset and length of the 4-byte `rec_type` magic field.
pub(super) const RECORD_TYPE_OFFSET: usize = 0;
pub(super) const RECORD_TYPE_LEN: usize = 4;

/// Byte offset and length of the 60-byte `prod_name` field.
pub(super) const PRODUCT_NAME_OFFSET: usize = 4;
pub(super) const PRODUCT_NAME_LEN: usize = 60;

/// Byte offset of the 4-byte little-endian `layout_code` field
/// (used for byte-order detection: equals `2` or `3` in the file's
/// native byte order).
pub(super) const LAYOUT_CODE_OFFSET: usize = 64;

/// Expected `layout_code` values when interpreted in the file's
/// byte order.
pub(super) const LAYOUT_CODE_VALUES: [i32; 2] = [2, 3];

/// Byte offset of the 4-byte `nominal_case_size` field
/// (declared variable count; may disagree with the actual variable
/// record count).
pub(super) const NOMINAL_CASE_SIZE_OFFSET: usize = 68;

/// Byte offset of the 4-byte `compression` field
/// (`0` = none, `1` = bytecode, `2` = ZLIB).
pub(super) const COMPRESSION_OFFSET: usize = 72;

/// Byte offset of the 4-byte `weight_index` field
/// (1-based index into the variable list, or `0` when absent).
pub(super) const WEIGHT_INDEX_OFFSET: usize = 76;

/// Byte offset of the 4-byte `ncases` field
/// (declared case count; `-1` when the writer failed to seek back).
pub(super) const NCASES_OFFSET: usize = 80;

/// Byte offset and length of the 8-byte `bias` field
/// (compression bias, typically `100.0`).
#[allow(dead_code)] // exercised by the tile-coverage test and once the writer lands.
pub(super) const BIAS_OFFSET: usize = 84;
pub(super) const BIAS_LEN: usize = 8;

/// Canonical compression bias.
pub(super) const CANONICAL_BIAS: f64 = 100.0;

/// Byte offset and length of the 9-byte `creation_date` field
/// (`"DD MMM YY"` ASCII).
pub(super) const CREATION_DATE_OFFSET: usize = 92;
pub(super) const CREATION_DATE_LEN: usize = 9;

/// Byte offset and length of the 8-byte `creation_time` field
/// (`"HH:MM:SS"` ASCII).
pub(super) const CREATION_TIME_OFFSET: usize = 101;
pub(super) const CREATION_TIME_LEN: usize = 8;

/// Byte offset and length of the 64-byte space-padded `file_label`
/// field.
pub(super) const FILE_LABEL_OFFSET: usize = 109;
pub(super) const FILE_LABEL_LEN: usize = 64;

/// Byte offset and length of the 3-byte trailing padding.
#[allow(dead_code)] // exercised by the tile-coverage test and once the writer lands.
pub(super) const TRAILING_PADDING_OFFSET: usize = 173;
pub(super) const TRAILING_PADDING_LEN: usize = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_offsets_tile_the_header_exactly() {
        assert_eq!(RECORD_TYPE_OFFSET, 0);
        assert_eq!(RECORD_TYPE_OFFSET + RECORD_TYPE_LEN, PRODUCT_NAME_OFFSET);
        assert_eq!(PRODUCT_NAME_OFFSET + PRODUCT_NAME_LEN, LAYOUT_CODE_OFFSET);
        assert_eq!(LAYOUT_CODE_OFFSET + 4, NOMINAL_CASE_SIZE_OFFSET);
        assert_eq!(NOMINAL_CASE_SIZE_OFFSET + 4, COMPRESSION_OFFSET);
        assert_eq!(COMPRESSION_OFFSET + 4, WEIGHT_INDEX_OFFSET);
        assert_eq!(WEIGHT_INDEX_OFFSET + 4, NCASES_OFFSET);
        assert_eq!(NCASES_OFFSET + 4, BIAS_OFFSET);
        assert_eq!(BIAS_OFFSET + BIAS_LEN, CREATION_DATE_OFFSET);
        assert_eq!(
            CREATION_DATE_OFFSET + CREATION_DATE_LEN,
            CREATION_TIME_OFFSET
        );
        assert_eq!(CREATION_TIME_OFFSET + CREATION_TIME_LEN, FILE_LABEL_OFFSET);
        assert_eq!(FILE_LABEL_OFFSET + FILE_LABEL_LEN, TRAILING_PADDING_OFFSET);
        assert_eq!(TRAILING_PADDING_OFFSET + TRAILING_PADDING_LEN, HEADER_LEN);
    }
}
