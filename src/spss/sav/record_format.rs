//! On-disk byte layout of the SAV data-record section.
//!
//! Three schemes share this section, selected by the header's
//! compression code:
//!
//! - **`CompressionKind::None`** — rows are written back to back, each
//!   exactly [`DataLayout::row_len`](crate::spss::sav::data_layout::DataLayout::row_len)
//!   bytes. Nothing here applies.
//! - **`CompressionKind::Bytecode`** — the row bytes are produced by the
//!   command stream described below.
//! - **`CompressionKind::Zlib`** — the *same* command stream, carried
//!   inside the ZSAV block container described at the bottom of this
//!   module. Verified against a PSPP `/ZCOMPRESSED` file: inflating
//!   every block and concatenating the output yields a byte-identical
//!   command stream to the one a `/COMPRESSED` file writes.
//!
//! # The command stream
//!
//! A continuous stream of 8-byte **command groups**. Each byte of a
//! group is one command, read left to right; the verbatim payloads a
//! group refers to follow *after* the whole group, in order.
//!
//! Rows are **not** delimited by the stream. A row ends once the
//! commands have produced
//! [`row_len`](crate::spss::sav::data_layout::DataLayout::row_len)
//! bytes of output, which means a command group routinely straddles a
//! row boundary and its unconsumed tail carries into the next row. A
//! decoder that reads a fresh group per row is wrong.

// The layout constants land as one set so the format is documented in
// one place. The command codes are consumed by the bytecode decoder.
// The tests below already exercise the relationships between them.
#![allow(dead_code)]

use crate::spss::sav::segment_layout::DATA_UNIT_LEN;

/// Number of command bytes in one bytecode command group.
pub(crate) const COMMAND_GROUP_LEN: usize = 8;

/// Padding. Consumes no input and produces no output.
///
/// PSPP writes these to fill out the final group of the stream.
pub(crate) const COMMAND_PADDING: u8 = 0;

/// Smallest command code standing for an inline numeric value.
pub(crate) const COMMAND_INLINE_MIN: u8 = 1;

/// Largest command code standing for an inline numeric value.
///
/// With the canonical bias of `100`, codes
/// [`COMMAND_INLINE_MIN`]`..=`[`COMMAND_INLINE_MAX`] cover the integral
/// values `-99..=151` — the range `ReadStat`'s compressor guards with
/// `value > -100 && value < 152`.
pub(crate) const COMMAND_INLINE_MAX: u8 = 251;

/// End of the data section.
///
/// **Optional.** PSPP does not write it: neither
/// `tests/fixtures/compression_bytecode.sav` nor
/// `compression_zlib.sav` contains the byte at all, and both simply run
/// out of stream after the last row. Treat it as an early stop, never
/// as the thing that proves the data ended.
pub(crate) const COMMAND_END_OF_DATA: u8 = 252;

/// The next eight bytes of the stream are one verbatim data unit.
pub(crate) const COMMAND_VERBATIM: u8 = 253;

/// One data unit of eight spaces.
pub(crate) const COMMAND_ALL_SPACES: u8 = 254;

/// One data unit holding the file's system-missing sentinel.
pub(crate) const COMMAND_SYSTEM_MISSING: u8 = 255;

/// The eight-space data unit [`COMMAND_ALL_SPACES`] expands to.
pub(crate) const EIGHT_SPACES: [u8; DATA_UNIT_LEN] = [b' '; DATA_UNIT_LEN];

// ---- ZSAV block container ------------------------------------------
//
// Field names and widths match ReadStat's `struct zheader`,
// `struct ztrailer` and `struct ztrailer_entry`; the offsets below were
// independently measured off a PSPP-written fixture.

/// Bytes in the ZSAV header, which opens the data section.
pub(crate) const ZHEADER_LEN: usize = 24;

/// Offset of the `i64` recording the ZSAV header's own file position.
///
/// Self-referential, and therefore a free validity check: it must equal
/// the file offset the header was read from.
pub(crate) const ZHEADER_SELF_OFFSET: usize = 0;

/// Offset of the `i64` file position of the ZSAV trailer.
pub(crate) const ZHEADER_TRAILER_POSITION_OFFSET: usize = 8;

/// Offset of the `i64` byte length of the ZSAV trailer.
pub(crate) const ZHEADER_TRAILER_LEN_OFFSET: usize = 16;

/// Bytes in the ZSAV trailer's fixed prefix, before its block table.
pub(crate) const ZTRAILER_HEADER_LEN: usize = 24;

/// Offset of the trailer's `i64` compression bias.
///
/// PSPP writes the *negation* of the header's bias here — `-100`
/// against a header bias of `100.0`. Do not validate it for equality.
pub(crate) const ZTRAILER_BIAS_OFFSET: usize = 0;

/// Offset of the trailer's `i64` reserved zero field.
pub(crate) const ZTRAILER_ZERO_OFFSET: usize = 8;

/// Offset of the trailer's `i32` nominal uncompressed block size.
pub(crate) const ZTRAILER_BLOCK_SIZE_OFFSET: usize = 16;

/// Offset of the trailer's `i32` block count.
pub(crate) const ZTRAILER_BLOCK_COUNT_OFFSET: usize = 20;

/// Bytes in one ZSAV trailer block-table entry.
pub(crate) const ZTRAILER_ENTRY_LEN: usize = 24;

/// Offset within an entry of its `i64` uncompressed file position.
pub(crate) const ZTRAILER_ENTRY_UNCOMPRESSED_POSITION_OFFSET: usize = 0;

/// Offset within an entry of its `i64` compressed file position.
pub(crate) const ZTRAILER_ENTRY_COMPRESSED_POSITION_OFFSET: usize = 8;

/// Offset within an entry of its `i32` uncompressed byte length.
pub(crate) const ZTRAILER_ENTRY_UNCOMPRESSED_LEN_OFFSET: usize = 16;

/// Offset within an entry of its `i32` compressed byte length.
pub(crate) const ZTRAILER_ENTRY_COMPRESSED_LEN_OFFSET: usize = 20;

/// The uncompressed block size PSPP writes.
///
/// Only a nominal figure — the last block is short, and a reader must
/// take each block's length from its own table entry rather than from
/// this. Kept so the writer has the value it should emit.
pub(crate) const ZTRAILER_BLOCK_SIZE_DEFAULT: i32 = 0x003f_f000;

#[cfg(test)]
mod tests {
    use super::*;

    /// Every command code is distinct and the inline range does not
    /// collide with any of the four special codes.
    #[test]
    fn command_codes_do_not_overlap() {
        let special = [
            COMMAND_PADDING,
            COMMAND_END_OF_DATA,
            COMMAND_VERBATIM,
            COMMAND_ALL_SPACES,
            COMMAND_SYSTEM_MISSING,
        ];
        for (index, code) in special.iter().enumerate() {
            assert!(
                !special[..index].contains(code),
                "duplicate special code {code}",
            );
            assert!(
                !(COMMAND_INLINE_MIN..=COMMAND_INLINE_MAX).contains(code),
                "special code {code} falls in the inline range",
            );
        }
    }

    /// The inline range plus the special codes tile the whole byte
    /// range, so no command byte is unaccounted for.
    #[test]
    fn command_codes_tile_the_byte_range() {
        let mut covered = [false; 256];
        for code in [
            COMMAND_PADDING,
            COMMAND_END_OF_DATA,
            COMMAND_VERBATIM,
            COMMAND_ALL_SPACES,
            COMMAND_SYSTEM_MISSING,
        ] {
            covered[usize::from(code)] = true;
        }
        for code in COMMAND_INLINE_MIN..=COMMAND_INLINE_MAX {
            covered[usize::from(code)] = true;
        }
        assert!(
            covered.iter().all(|seen| *seen),
            "a command code is unhandled"
        );
    }

    /// With the canonical bias the inline codes cover exactly the range
    /// the compressor emits them for. Stated in integers: the
    /// endpoints are whole numbers, and the decoder's `f64` arithmetic
    /// represents them exactly.
    #[test]
    fn inline_codes_cover_the_canonical_range() {
        let bias = 100_i32;
        assert_eq!(i32::from(COMMAND_INLINE_MIN) - bias, -99);
        assert_eq!(i32::from(COMMAND_INLINE_MAX) - bias, 151);
    }

    /// The ZSAV field offsets tile their records with no gap or
    /// overlap.
    #[test]
    fn zsav_field_offsets_tile_their_records() {
        let zheader = [
            (ZHEADER_SELF_OFFSET, 8),
            (ZHEADER_TRAILER_POSITION_OFFSET, 8),
            (ZHEADER_TRAILER_LEN_OFFSET, 8),
        ];
        assert_tiles(&zheader, ZHEADER_LEN);

        let ztrailer = [
            (ZTRAILER_BIAS_OFFSET, 8),
            (ZTRAILER_ZERO_OFFSET, 8),
            (ZTRAILER_BLOCK_SIZE_OFFSET, 4),
            (ZTRAILER_BLOCK_COUNT_OFFSET, 4),
        ];
        assert_tiles(&ztrailer, ZTRAILER_HEADER_LEN);

        let entry = [
            (ZTRAILER_ENTRY_UNCOMPRESSED_POSITION_OFFSET, 8),
            (ZTRAILER_ENTRY_COMPRESSED_POSITION_OFFSET, 8),
            (ZTRAILER_ENTRY_UNCOMPRESSED_LEN_OFFSET, 4),
            (ZTRAILER_ENTRY_COMPRESSED_LEN_OFFSET, 4),
        ];
        assert_tiles(&entry, ZTRAILER_ENTRY_LEN);
    }

    /// Asserts `fields` covers `0..total` exactly once.
    fn assert_tiles(fields: &[(usize, usize)], total: usize) {
        let mut covered = vec![false; total];
        for &(offset, len) in fields {
            for byte in &mut covered[offset..offset + len] {
                assert!(!*byte, "overlapping field at offset {offset}");
                *byte = true;
            }
        }
        assert!(covered.iter().all(|seen| *seen), "a field is uncovered");
    }
}
