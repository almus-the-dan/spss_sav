//! Pure parse helpers for the SAV file header.
//!
//! Each function takes already-read bytes (plus the byte offset at
//! which they were read, for error reporting) and returns the parsed
//! value or a [`SavError`]. The I/O itself stays in the caller — that
//! lets the sync and async header readers share the same logic
//! without duplicating parsing alongside two flavors of read
//! machinery.

use encoding_rs::Encoding;

use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::compression::Compression;
use crate::spss::sav::float_format::FloatFormat;
use crate::spss::sav::header_format::{CANONICAL_BIAS, LAYOUT_CODE_VALUES, MAGIC_FL2, MAGIC_FL3};
use crate::spss::sav::sav_error::{FormatErrorKind, Result, SavError, Section};
use crate::spss::sav::sav_warning::SavWarning;

/// Which magic-bytes family the file starts with.
///
/// `$FL2` is the regular-SAV family (uncompressed or bytecode);
/// `$FL3` is the ZSAV family (zlib-compressed). The actual
/// compression in use is the value read from the `compression`
/// field at offset 72; see [`resolve_compression`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MagicKind {
    /// `$FL2` — regular SAV (compression code `0` or `1`).
    Fl2,
    /// `$FL3` — ZSAV (compression code `2`).
    Fl3,
}

/// Validates the 4-byte `rec_type` magic and classifies the file.
pub(super) fn parse_magic(bytes: [u8; 4], position: u64) -> Result<MagicKind> {
    if &bytes == MAGIC_FL2 {
        Ok(MagicKind::Fl2)
    } else if &bytes == MAGIC_FL3 {
        Ok(MagicKind::Fl3)
    } else {
        Err(SavError::format(
            Section::Header,
            position,
            FormatErrorKind::InvalidMagic,
        ))
    }
}

/// Decodes the 60-byte `prod_name` field through the supplied
/// encoding and trims trailing spaces and NULs.
pub(super) fn parse_product_name(bytes: &[u8], encoding: &'static Encoding) -> String {
    decode_trimmed(bytes, encoding)
}

/// Determines the file's integer byte order from the 4-byte
/// `layout_code` field. Tries little-endian first; if neither
/// little-endian nor big-endian decodes to one of the canonical
/// values (`2` or `3`), returns
/// [`FormatErrorKind::UnreadableLayoutCode`].
pub(super) fn parse_layout_code(bytes: [u8; 4], position: u64) -> Result<ByteOrder> {
    let little = i32::from_le_bytes(bytes);
    if LAYOUT_CODE_VALUES.contains(&little) {
        return Ok(ByteOrder::LittleEndian);
    }
    let big = i32::from_be_bytes(bytes);
    if LAYOUT_CODE_VALUES.contains(&big) {
        return Ok(ByteOrder::BigEndian);
    }
    let error = SavError::format(
        Section::Header,
        position,
        FormatErrorKind::UnreadableLayoutCode,
    );
    Err(error)
}

/// Reconciles the 4-byte `compression` field with the magic-bytes
/// family. Code is authoritative (matching `ReadStat`); a
/// disagreement with the magic surfaces as
/// [`SavWarning::CompressionMismatch`], an unrecognized code as
/// [`SavWarning::UnknownCompressionCode`].
///
/// Returns the resolved [`Compression`] together with the warning
/// to push (if any).
pub(super) fn resolve_compression(
    code: i32,
    magic: MagicKind,
    rec_type: [u8; 4],
) -> (Compression, Option<SavWarning>) {
    match code {
        0 => match magic {
            MagicKind::Fl2 => (Compression::None, None),
            MagicKind::Fl3 => (
                Compression::None,
                Some(SavWarning::CompressionMismatch { rec_type, code }),
            ),
        },
        1 => match magic {
            MagicKind::Fl2 => (Compression::Bytecode, None),
            MagicKind::Fl3 => (
                Compression::Bytecode,
                Some(SavWarning::CompressionMismatch { rec_type, code }),
            ),
        },
        2 => match magic {
            MagicKind::Fl3 => (Compression::Zlib, None),
            MagicKind::Fl2 => (
                Compression::Zlib,
                Some(SavWarning::CompressionMismatch { rec_type, code }),
            ),
        },
        _ => (
            Compression::None,
            Some(SavWarning::UnknownCompressionCode { code }),
        ),
    }
}

/// Decodes the 1-based `weight_index`. Returns `None` when the
/// field is `0` (no weight variable) or any negative value.
pub(super) fn parse_weight_index(value: i32) -> Option<usize> {
    if value > 0 {
        usize::try_from(value).ok()
    } else {
        None
    }
}

/// Decodes the `ncases` field. Returns `None` when the declared
/// count is negative (typically `-1`, signaling "writer failed to
/// seek back").
pub(super) fn parse_case_count(value: i32) -> Option<u32> {
    u32::try_from(value).ok()
}

/// Decodes the `nominal_case_size` field. Returns `None` when the
/// declared count is negative.
pub(super) fn parse_nominal_case_size(value: i32) -> Option<u32> {
    u32::try_from(value).ok()
}

/// Decodes the 8-byte `bias` field by trying IEEE 754, IBM HFP,
/// and VAX `D_float` in turn. Returns the recognized
/// [`FloatFormat`] alongside the decoded bias. Errors with
/// [`FormatErrorKind::UnknownFloatFormat`] when none of the three
/// decodes to the canonical value (`100.0`).
#[allow(clippy::float_cmp)] // canonical bias compares against the spec's exact `100.0` sentinel
pub(super) fn parse_bias(
    bytes: [u8; 8],
    byte_order: ByteOrder,
    position: u64,
) -> Result<(FloatFormat, f64)> {
    let ieee = byte_order.read_f64(bytes);
    if ieee == CANONICAL_BIAS {
        return Ok((FloatFormat::Ieee754, ieee));
    }
    let ibm = decode_ibm_hfp_bias(bytes, byte_order);
    if ibm == CANONICAL_BIAS {
        return Ok((FloatFormat::IbmHfp, ibm));
    }
    if let Some(value) = decode_vax_bias(bytes, byte_order)
        && value == CANONICAL_BIAS
    {
        return Ok((FloatFormat::Vax, value));
    }
    let error = SavError::format(
        Section::Header,
        position,
        FormatErrorKind::UnknownFloatFormat,
    );
    Err(error)
}

/// Decodes the 64-byte `file_label` field through the supplied
/// encoding and trims trailing spaces and NULs.
pub(super) fn parse_file_label(bytes: &[u8], encoding: &'static Encoding) -> String {
    decode_trimmed(bytes, encoding)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Decodes a fixed-width byte string through `encoding` and trims
/// trailing whitespace and NULs.
fn decode_trimmed(bytes: &[u8], encoding: &'static Encoding) -> String {
    let trimmed = trim_trailing_padding(bytes);
    let (cow, _, _) = encoding.decode(trimmed);
    cow.into_owned()
}

fn trim_trailing_padding(bytes: &[u8]) -> &[u8] {
    let end = bytes
        .iter()
        .rposition(|&b| b != b' ' && b != 0)
        .map_or(0, |p| p + 1);
    &bytes[..end]
}

fn decode_ibm_hfp_bias(bytes: [u8; 8], byte_order: ByteOrder) -> f64 {
    let ibm = match byte_order {
        ByteOrder::BigEndian => ibm_hfp::IbmFloat64::from_be_bytes(bytes),
        ByteOrder::LittleEndian => ibm_hfp::IbmFloat64::from_le_bytes(bytes),
    };
    f64::from(ibm)
}

fn decode_vax_bias(_bytes: [u8; 8], _byte_order: ByteOrder) -> Option<f64> {
    // VAX D_float decoding is deferred — the wire-to-bit-pattern
    // mapping needs care around 16-bit word swapping that the
    // `vax-floating` crate does internally only when the bytes are
    // already in VAX-native order. Until we have a known-good VAX
    // SAV fixture to test against, returning `None` is preferable
    // to silently misidentifying. Files with VAX-encoded biases
    // will surface as `FormatErrorKind::UnknownFloatFormat`.
    None
}
