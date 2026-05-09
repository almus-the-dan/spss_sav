//! Pure parse helpers for the SAV file header.
//!
//! Each function takes already-read bytes (plus the byte offset at
//! which they were read, for error reporting) and returns the parsed
//! value or a [`SavError`]. The I/O itself stays in the caller — that
//! lets the sync and async header readers share the same logic
//! without duplicating parsing alongside two flavors of read
//! machinery.

use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::compression::Compression;
use crate::spss::sav::float_format::FloatFormat;
use crate::spss::sav::sav_creation_timestamp::SavCreationTimestamp;
use crate::spss::sav::sav_error::Result;

/// Validates the 4-byte `rec_type` magic and returns the implied
/// compression family (`Bytecode` for `$FL2`, `Zlib` for `$FL3`).
///
/// The actual compression in use is the value read from the
/// `compression` field at offset 72; the caller surfaces mismatches
/// between that field and the magic bytes as a warning.
#[allow(dead_code)] // exercised once the header reader lands.
pub(super) fn parse_magic(_bytes: [u8; 4], _position: u64) -> Result<Compression> {
    todo!("body lands with the header reader phase")
}

/// Decodes the 60-byte `prod_name` field as a trimmed ASCII/Latin-1
/// string. Trailing spaces and NULs are stripped.
#[allow(dead_code)] // exercised once the header reader lands.
pub(super) fn parse_product_name(_bytes: &[u8]) -> String {
    todo!("body lands with the header reader phase")
}

/// Determines the file's integer byte order from the 4-byte
/// `layout_code` field. Tries little-endian first; if neither
/// little-endian nor big-endian decodes to one of the canonical
/// values (`2` or `3`), returns a format error.
#[allow(dead_code)] // exercised once the header reader lands.
pub(super) fn parse_layout_code(_bytes: [u8; 4], _position: u64) -> Result<ByteOrder> {
    todo!("body lands with the header reader phase")
}

/// Validates the 4-byte `compression` field against the value
/// implied by the magic bytes. Returns the authoritative
/// [`Compression`] (taken from the magic bytes, per the spec).
///
/// The caller decides whether to surface a
/// [`SavWarning::CompressionMismatch`](crate::spss::sav::sav_warning::SavWarning::CompressionMismatch)
/// when the field disagrees with the magic.
#[allow(dead_code)] // exercised once the header reader lands.
pub(super) fn parse_compression_code(
    _bytes: [u8; 4],
    _byte_order: ByteOrder,
    _magic_compression: Compression,
    _position: u64,
) -> Result<Compression> {
    todo!("body lands with the header reader phase")
}

/// Decodes the 4-byte 1-based `weight_index` field. Returns `None`
/// when the field is `0` (no weight variable).
#[allow(dead_code)] // exercised once the header reader lands.
pub(super) fn parse_weight_index(_bytes: [u8; 4], _byte_order: ByteOrder) -> Option<usize> {
    todo!("body lands with the header reader phase")
}

/// Decodes the 4-byte `ncases` field. Returns `None` when the
/// declared count is `-1` (the writer failed to seek back).
#[allow(dead_code)] // exercised once the header reader lands.
pub(super) fn parse_case_count(_bytes: [u8; 4], _byte_order: ByteOrder) -> Option<u32> {
    todo!("body lands with the header reader phase")
}

/// Decodes the 8-byte `bias` field by trying IEEE 754, IBM HFP,
/// and VAX `D_float` in turn. Returns the recognized
/// [`FloatFormat`] alongside the decoded bias. Errors when none
/// of the three formats decodes to the canonical value (`100.0`).
#[allow(dead_code)] // exercised once the header reader lands.
pub(super) fn parse_bias(
    _bytes: [u8; 8],
    _byte_order: ByteOrder,
    _position: u64,
) -> Result<(FloatFormat, f64)> {
    todo!("body lands with the header reader phase")
}

/// Parses the 9-byte `creation_date` and 8-byte `creation_time`
/// fields into a [`SavCreationTimestamp`]. Falls back to
/// [`SavCreationTimestamp::Raw`] when either field is unparseable.
#[allow(dead_code)] // exercised once the header reader lands.
pub(super) fn parse_creation_timestamp(
    _date_bytes: [u8; 9],
    _time_bytes: [u8; 8],
) -> SavCreationTimestamp {
    todo!("body lands with the header reader phase")
}

/// Decodes the 64-byte `file_label` field. Trailing spaces are
/// stripped.
#[allow(dead_code)] // exercised once the header reader lands.
pub(super) fn parse_file_label(_bytes: &[u8]) -> String {
    todo!("body lands with the header reader phase")
}
