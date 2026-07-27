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
use crate::spss::sav::float_encoding::FloatEncoding;
use crate::spss::sav::float_format::FloatFormat;
use crate::spss::sav::header_format::{CANONICAL_BIAS, LAYOUT_CODE_VALUES, MAGIC_FL2, MAGIC_FL3};
use crate::spss::sav::sav_error::{FormatErrorKind, Result, SavError, Section};
use crate::spss::sav::sav_warning::SavWarning;
use crate::spss::sav::text_field::decode_trimmed;

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

/// Identifies the file's float format from its 8-byte `bias` field,
/// which every writer sets to the canonical `100.0`. Returns the
/// recognized [`FloatFormat`] alongside the decoded bias, or
/// [`FormatErrorKind::UnknownFloatFormat`] when no candidate decodes to
/// `100.0`.
///
/// This is the only thing that distinguishes VAX `D_float` from
/// `G_float`: subtype 3 declares a single "VAX" code for both.
#[allow(clippy::float_cmp)] // canonical bias compares against the spec's exact `100.0` sentinel
pub(super) fn parse_bias(
    bytes: [u8; 8],
    byte_order: ByteOrder,
    position: u64,
) -> Result<(FloatFormat, f64)> {
    // Probe each format's encoding of the canonical bias, the way
    // PSPP's `float_identify` does. The encodings of `100.0` are
    // pairwise distinct (asserted by float_encoding's
    // `every_format_encodes_the_canonical_bias_differently`), so at
    // most one can match and the probe order carries no meaning.
    let formats = [
        FloatFormat::Ieee754,
        FloatFormat::IbmHfp,
        FloatFormat::VaxDFloat,
        FloatFormat::VaxGFloat,
    ];
    for format in formats {
        if FloatEncoding::new(format, byte_order).decode(bytes) == CANONICAL_BIAS {
            return Ok((format, CANONICAL_BIAS));
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `100.0` as each format lays it out on disk. Derived from the
    /// format definitions, not from our own encoder, so these double as
    /// a check on the conversion path.
    const IEEE_LITTLE: [u8; 8] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x59, 0x40];
    const IEEE_BIG: [u8; 8] = [0x40, 0x59, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    const IBM_HFP: [u8; 8] = [0x42, 0x64, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    const VAX_D: [u8; 8] = [0xC8, 0x43, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    const VAX_G: [u8; 8] = [0x79, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];

    fn identify(bytes: [u8; 8], byte_order: ByteOrder) -> FloatFormat {
        let (format, bias) = parse_bias(bytes, byte_order, 0).expect("recognized bias");
        assert_eq!(bias.to_bits(), CANONICAL_BIAS.to_bits());
        format
    }

    #[test]
    fn identifies_ieee_in_the_declared_byte_order() {
        assert_eq!(
            identify(IEEE_LITTLE, ByteOrder::LittleEndian),
            FloatFormat::Ieee754,
        );
        assert_eq!(
            identify(IEEE_BIG, ByteOrder::BigEndian),
            FloatFormat::Ieee754,
        );
    }

    #[test]
    fn identifies_the_legacy_formats() {
        assert_eq!(identify(IBM_HFP, ByteOrder::BigEndian), FloatFormat::IbmHfp,);
        assert_eq!(
            identify(VAX_D, ByteOrder::LittleEndian),
            FloatFormat::VaxDFloat,
        );
        assert_eq!(
            identify(VAX_G, ByteOrder::LittleEndian),
            FloatFormat::VaxGFloat,
        );
    }

    #[test]
    fn distinguishes_the_two_vax_encodings() {
        // The whole reason `FloatFormat` splits them: swapping the two
        // byte strings must not still say "VAX".
        assert_ne!(
            identify(VAX_D, ByteOrder::LittleEndian),
            identify(VAX_G, ByteOrder::LittleEndian),
        );
    }

    #[test]
    fn legacy_formats_ignore_the_declared_byte_order() {
        // IBM HFP and VAX layouts are fixed by the format, so a file
        // whose integer fields are little-endian still identifies.
        assert_eq!(
            identify(IBM_HFP, ByteOrder::LittleEndian),
            FloatFormat::IbmHfp,
        );
        assert_eq!(
            identify(VAX_D, ByteOrder::BigEndian),
            FloatFormat::VaxDFloat,
        );
    }

    #[test]
    fn ieee_in_the_wrong_byte_order_is_not_recognized() {
        let err = parse_bias(IEEE_LITTLE, ByteOrder::BigEndian, 0).unwrap_err();
        assert!(matches!(
            err,
            SavError::Format(ref e) if e.kind() == FormatErrorKind::UnknownFloatFormat
        ));
    }

    #[test]
    fn an_unrecognizable_bias_errors() {
        let err = parse_bias([0xAB; 8], ByteOrder::LittleEndian, 0).unwrap_err();
        assert!(matches!(
            err,
            SavError::Format(ref e) if e.kind() == FormatErrorKind::UnknownFloatFormat
        ));
    }
}
