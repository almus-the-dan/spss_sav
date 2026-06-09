//! Pure parse helpers for the SAV dictionary section.
//!
//! Each function takes already-read bytes (plus the byte offset at
//! which they were read, for error reporting) and returns the parsed
//! value or a [`SavError`]. The I/O itself stays in the caller — this
//! lets the sync and async dictionary readers share the same logic
//! without duplicating parsing alongside two flavors of read
//! machinery.
//!
//! The orchestration that walks the dictionary section, dispatches by
//! record type, collapses string-variable continuation records, and
//! reconciles extension-derived metadata lives in
//! [`DictionaryReader`](crate::spss::sav::dictionary_reader::DictionaryReader).

use encoding_rs::Encoding;

use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::dictionary_format::{
    CHARACTER_ENCODING_ELEMENT_SIZE, DISPLAY_PARAMETERS_ELEMENT_SIZE,
    EXTENDED_NUMBER_OF_CASES_COUNT_OFFSET, EXTENDED_NUMBER_OF_CASES_ELEMENT_COUNT,
    EXTENDED_NUMBER_OF_CASES_ELEMENT_SIZE, EXTENDED_NUMBER_OF_CASES_VERSION_OFFSET,
    FLOAT_SENTINELS_ELEMENT_COUNT, FLOAT_SENTINELS_ELEMENT_SIZE, FLOAT_SENTINELS_HIGHEST_OFFSET,
    FLOAT_SENTINELS_LOWEST_OFFSET, FLOAT_SENTINELS_SYSTEM_MISSING_OFFSET,
    FORMAT_CODE_DECIMALS_BYTE, FORMAT_CODE_KIND_BYTE, FORMAT_CODE_WIDTH_BYTE,
    LONG_VARIABLE_NAMES_ELEMENT_SIZE, LONG_VARIABLE_NAMES_KEY_VALUE_SEPARATOR,
    LONG_VARIABLE_NAMES_PAIR_SEPARATOR, MACHINE_INTEGER_INFO_ELEMENT_COUNT,
    MACHINE_INTEGER_INFO_ELEMENT_SIZE, VALUE_LABEL_ENTRY_ALIGNMENT,
    VALUE_LABEL_LABEL_LEN_FIELD_LEN, VALUE_LABEL_VALUE_LEN, VARIABLE_TYPE_CONTINUATION,
    VARIABLE_TYPE_NUMERIC, VARIABLE_TYPE_STRING_MAX, VERY_LONG_STRINGS_ELEMENT_SIZE,
    VERY_LONG_STRINGS_KEY_VALUE_SEPARATOR, VERY_LONG_STRINGS_PAIR_PADDING,
    VERY_LONG_STRINGS_PAIR_SEPARATOR,
};
use crate::spss::sav::extensions::extended_number_of_cases::ExtendedNumberOfCases;
use crate::spss::sav::extensions::float_sentinels::FloatSentinels;
use crate::spss::sav::extensions::long_variable_name::LongVariableName;
use crate::spss::sav::extensions::machine_integer_info::MachineIntegerInfo;
use crate::spss::sav::extensions::raw_display_parameters::RawDisplayParameters;
use crate::spss::sav::extensions::very_long_string::VeryLongString;
use crate::spss::sav::raw_missing_values::RawMissingValues;
use crate::spss::sav::raw_value_label_entry::RawValueLabelEntry;
use crate::spss::sav::sav_error::{Field, FormatErrorKind, Result, SavError, Section};
use crate::spss::sav::sav_format::SavFormat;
use crate::spss::sav::sav_format_kind::SavFormatKind;
use crate::spss::sav::text_field::{decode_trimmed, trim_trailing_padding};

/// Classification of the type-2 record's 4-byte `type` field.
///
/// `-1` marks a continuation record extending the previous logical
/// variable's storage by one 8-byte segment; `0` marks a numeric
/// variable; `1..=255` marks a string of that width in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VariableTypeCode {
    /// Continuation of the previous variable (`type == -1`).
    Continuation,
    /// Numeric variable (`type == 0`).
    Numeric,
    /// String of the given width in bytes (`type ∈ 1..=255`).
    String(u8),
}

/// Classification of the type-2 record's `n_missing_values` field.
///
/// Encoding (from the SAV spec):
///
/// * `0` → [`None`](Self::None) — no missing values
/// * `1..=3` → [`Discrete(n)`](Self::Discrete) — `n` discrete missing
///   values follow
/// * `-2` → [`Range`](Self::Range) — a single low/high range
///   follows (2 entries)
/// * `-3` → [`RangeWithDiscrete`](Self::RangeWithDiscrete) — a
///   low/high range plus one discrete value follows (3 entries)
///
/// The undocumented `-1` value, which appears in some files in the
/// wild, decodes to `Discrete(1)` to match `ReadStat`'s data
/// outcome; the dictionary reader emits a corresponding
/// [`SavWarning`](crate::spss::sav::sav_warning::SavWarning) alongside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MissingValueCount {
    /// No missing values.
    None,
    /// `count` discrete missing values (`1..=3`).
    Discrete(u8),
    /// A single low/high range. Two f64 (or two 8-byte string)
    /// entries follow.
    Range,
    /// A low/high range plus one discrete value. Three entries
    /// follow.
    RangeWithDiscrete,
}

impl MissingValueCount {
    /// Number of 8-byte entries that follow after the variable
    /// record body and any label block.
    pub(super) fn entry_count(self) -> usize {
        match self {
            Self::None => 0,
            Self::Discrete(n) => n as usize,
            Self::Range => 2,
            Self::RangeWithDiscrete => 3,
        }
    }
}

/// Decodes the variable record's `type` field.
///
/// # Errors
///
/// Returns [`FormatErrorKind::UnexpectedValue`] with [`Field::VariableType`]
/// for any value outside `{-1, 0, 1..=255}`.
pub(super) fn parse_variable_type(value: i32, position: u64) -> Result<VariableTypeCode> {
    match value {
        VARIABLE_TYPE_CONTINUATION => Ok(VariableTypeCode::Continuation),
        VARIABLE_TYPE_NUMERIC => Ok(VariableTypeCode::Numeric),
        v if (1..=VARIABLE_TYPE_STRING_MAX).contains(&v) => {
            let width = u8::try_from(v).expect("validated to fit in u8 above");
            Ok(VariableTypeCode::String(width))
        }
        _ => Err(SavError::format(
            Section::Dictionary,
            position,
            FormatErrorKind::UnexpectedValue {
                field: Field::VariableType,
            },
        )),
    }
}

/// Decodes the variable record's `n_missing_values` field.
///
/// The caller is responsible for emitting a
/// [`SavWarning::InvalidMissingValueCount`](crate::spss::sav::sav_warning::SavWarning::InvalidMissingValueCount)
/// when the raw value is `-1` — the parse layer only classifies.
///
/// # Errors
///
/// Returns [`FormatErrorKind::UnexpectedValue`] with [`Field::MissingValueCount`]
/// for `|value| > 3`.
pub(super) fn parse_missing_value_count(value: i32, position: u64) -> Result<MissingValueCount> {
    match value {
        0 => Ok(MissingValueCount::None),
        // `-1` is undocumented; treat as a single discrete missing
        // value (matching `ReadStat`). The caller emits the warning.
        -1 | 1 => Ok(MissingValueCount::Discrete(1)),
        2 => Ok(MissingValueCount::Discrete(2)),
        3 => Ok(MissingValueCount::Discrete(3)),
        -2 => Ok(MissingValueCount::Range),
        -3 => Ok(MissingValueCount::RangeWithDiscrete),
        _ => Err(SavError::format(
            Section::Dictionary,
            position,
            FormatErrorKind::UnexpectedValue {
                field: Field::MissingValueCount,
            },
        )),
    }
}

/// Decodes the variable record's `has_label` flag. Treats any
/// non-zero value as `true` (matching `ReadStat`).
pub(super) fn parse_has_label(value: i32) -> bool {
    value != 0
}

/// Decodes the 8-byte short-name field through the supplied encoding
/// and trims trailing spaces and NULs.
pub(super) fn parse_short_name(bytes: [u8; 8], encoding: &'static Encoding) -> String {
    decode_trimmed(&bytes, encoding)
}

/// Decodes a 4-byte packed format code into a [`SavFormat`].
///
/// The packing (after reduction to native byte order) is byte 0 =
/// decimal places, byte 1 = width, byte 2 = format kind, byte 3 =
/// unused. Unrecognized kind bytes round-trip as
/// [`SavFormatKind::Unknown`].
pub(super) fn parse_sav_format(packed: u32) -> SavFormat {
    let bytes = packed.to_le_bytes();
    SavFormat::builder()
        .kind(SavFormatKind::from_byte(bytes[FORMAT_CODE_KIND_BYTE]))
        .width(bytes[FORMAT_CODE_WIDTH_BYTE])
        .decimals(bytes[FORMAT_CODE_DECIMALS_BYTE])
        .build()
}

/// Combines a classified [`MissingValueCount`] with the already-read
/// `[u8; 8]` entries into the wire-level [`RawMissingValues`] carried
/// on a [`SavVariableHeader`](crate::spss::sav::sav_variable_header::SavVariableHeader).
///
/// # Panics
///
/// Panics in debug builds if `entries.len()` does not match
/// `count.entry_count()`. The caller (the dictionary reader)
/// guarantees this by sizing `entries` from the same classification.
pub(super) fn compose_raw_missing_values(
    count: MissingValueCount,
    entries: Vec<[u8; 8]>,
) -> RawMissingValues {
    debug_assert_eq!(entries.len(), count.entry_count());
    match count {
        MissingValueCount::None => RawMissingValues::None,
        MissingValueCount::Discrete(_) => RawMissingValues::Discrete(entries),
        MissingValueCount::Range => RawMissingValues::Range {
            low: entries[0],
            high: entries[1],
        },
        MissingValueCount::RangeWithDiscrete => RawMissingValues::RangeWithDiscrete {
            low: entries[0],
            high: entries[1],
            discrete: entries[2],
        },
    }
}

/// Total on-disk length of one type-3 value-label entry, given the
/// declared `unpadded_len` byte that follows the 8-byte `value`
/// field. The (length byte plus the label) portion is padded up to a
/// multiple of [`VALUE_LABEL_ENTRY_ALIGNMENT`]; the 8-byte value sits
/// at the front of the entry.
///
/// Matches `ReadStat`'s `padded_len = (unpadded_len + 8) / 8 * 8 - 1`,
/// rewritten here as the full entry size including the leading
/// 8-byte value.
#[allow(dead_code)] // exercised once the value-label reader implementation lands.
pub(super) fn value_label_entry_size(unpadded_len: u8) -> usize {
    let with_length_byte = usize::from(unpadded_len) + VALUE_LABEL_LABEL_LEN_FIELD_LEN;
    let aligned = with_length_byte.div_ceil(VALUE_LABEL_ENTRY_ALIGNMENT);
    let padded = aligned * VALUE_LABEL_ENTRY_ALIGNMENT;
    VALUE_LABEL_VALUE_LEN + padded
}

/// Decodes one type-3 value-label entry from its on-disk bytes.
///
/// `value` is the 8 bytes preceding the length byte, carried verbatim
/// (numeric vs. string interpretation is deferred until the paired
/// type-4 record ties this set to a typed variable). `label_bytes`
/// is the padded label portion that follows the length byte; only
/// the first `unpadded_len` bytes are decoded through `encoding`,
/// and any trailing padding is discarded.
///
/// # Panics
///
/// Panics in debug builds if `label_bytes.len() < unpadded_len as
/// usize`. The caller (the dictionary reader) sizes the slice from
/// [`value_label_entry_size`] which guarantees this.
pub(super) fn parse_value_label_entry(
    value: [u8; VALUE_LABEL_VALUE_LEN],
    unpadded_len: u8,
    label_bytes: &[u8],
    encoding: &'static Encoding,
) -> RawValueLabelEntry {
    let unpadded_len = usize::from(unpadded_len);
    debug_assert!(label_bytes.len() >= unpadded_len);
    let (decoded, _, _) = encoding.decode(&label_bytes[..unpadded_len]);
    RawValueLabelEntry::builder()
        .value(value)
        .label(decoded.into_owned())
        .build()
}

/// Translates a type-4 record's 1-based physical variable indices
/// into 0-based logical indices, using `primaries` — the list of
/// 0-based physical record positions of each primary
/// (non-continuation) variable record the dictionary reader has
/// seen so far, in declaration order.
///
/// Duplicates in `raw` are preserved verbatim in the output (per
/// the locked Q&A — the reader carries the on-disk bytes through to
/// `RawValueLabelSet::variable_indices` for round-trip fidelity).
///
/// # Errors
///
/// Returns [`FormatErrorKind::DanglingValueLabel`] for any index
/// that is zero, exceeds the highest physical position recorded, or
/// lands on a string-variable continuation record (i.e., is not
/// present in `primaries`).
pub(super) fn normalize_value_label_variable_indices(
    raw: &[u32],
    primaries: &[u32],
    position: u64,
) -> Result<Vec<u32>> {
    let mut out = Vec::with_capacity(raw.len());
    for &one_based in raw {
        let dangling = || {
            SavError::format(
                Section::Dictionary,
                position,
                FormatErrorKind::DanglingValueLabel,
            )
        };
        if one_based == 0 {
            return Err(dangling());
        }
        let zero_based = one_based - 1;
        // `primaries` is monotonically increasing, so binary search
        // is correct; the linear fallback would also be fine for the
        // small variable counts SAV files carry in practice.
        let logical = primaries
            .binary_search(&zero_based)
            .map_err(|_| dangling())?;
        let logical = u32::try_from(logical).map_err(|_| {
            SavError::format(
                Section::Dictionary,
                position,
                FormatErrorKind::FieldTooLarge {
                    field: Field::VariableCount,
                },
            )
        })?;
        out.push(logical);
    }
    Ok(out)
}

/// Validates that a type-7 record's envelope (`element_size`,
/// `element_count`) matches the shape a specific subtype is
/// supposed to declare.
///
/// # Errors
///
/// Returns [`FormatErrorKind::UnexpectedValue`] tagged with
/// [`Field::ExtensionElementSize`] or
/// [`Field::ExtensionElementCount`] on the first mismatch. The
/// `position` is the byte offset at which the envelope began (the
/// `element_size` field).
pub(super) fn validate_extension_shape(
    actual_size: u32,
    actual_count: u32,
    expected_size: u32,
    expected_count: u32,
    position: u64,
) -> Result<()> {
    if actual_size != expected_size {
        let error = SavError::format(
            Section::Dictionary,
            position,
            FormatErrorKind::UnexpectedValue {
                field: Field::ExtensionElementSize,
            },
        );
        return Err(error);
    }
    if actual_count != expected_count {
        let error = SavError::format(
            Section::Dictionary,
            position,
            FormatErrorKind::UnexpectedValue {
                field: Field::ExtensionElementCount,
            },
        );
        return Err(error);
    }
    Ok(())
}

/// Parses an extension subtype-16 payload (extended number of
/// cases: two `i64` fields — a version flag plus the authoritative
/// case count).
///
/// Validates the envelope against the subtype's spec shape
/// (`element_size == 8`, `element_count == 2`) and decodes both
/// `i64`s in the file's byte order.
///
/// # Errors
///
/// Returns [`FormatErrorKind::UnexpectedValue`] when the envelope
/// shape disagrees with the spec.
///
/// # Panics
///
/// Panics in debug builds if `payload.len()` does not equal
/// `actual_size * actual_count`. The caller (the dictionary reader)
/// reads the payload from those dimensions, so this is a logic
/// invariant.
pub(super) fn parse_extended_number_of_cases(
    actual_size: u32,
    actual_count: u32,
    payload: &[u8],
    byte_order: ByteOrder,
    position: u64,
) -> Result<ExtendedNumberOfCases> {
    validate_extension_shape(
        actual_size,
        actual_count,
        EXTENDED_NUMBER_OF_CASES_ELEMENT_SIZE,
        EXTENDED_NUMBER_OF_CASES_ELEMENT_COUNT,
        position,
    )?;
    debug_assert_eq!(payload.len(), 16);
    let i64_at = |offset: usize| -> i64 {
        let bytes: [u8; 8] = payload[offset..offset + 8]
            .try_into()
            .expect("envelope validation guarantees a 16-byte payload");
        byte_order.read_i64(bytes)
    };
    let record = ExtendedNumberOfCases::builder()
        .version(i64_at(EXTENDED_NUMBER_OF_CASES_VERSION_OFFSET))
        .count(i64_at(EXTENDED_NUMBER_OF_CASES_COUNT_OFFSET))
        .build();
    Ok(record)
}

/// Parses an extension subtype-4 payload (float sentinel values:
/// system missing, highest, lowest).
///
/// Validates the envelope against the subtype's spec shape
/// (`element_size == 8`, `element_count == 3`) and slices the
/// 24-byte payload into three 8-byte slabs. Bytes are carried
/// verbatim — no float-format decode is applied here, so the
/// returned [`FloatSentinels`] round-trips bit-exactly regardless
/// of whether the file uses IEEE 754, IBM HFP, or VAX.
///
/// # Errors
///
/// Returns [`FormatErrorKind::UnexpectedValue`] when the envelope
/// shape disagrees with the spec.
///
/// # Panics
///
/// Panics in debug builds if `payload.len()` does not equal
/// `actual_size * actual_count`. The caller (the dictionary reader)
/// reads the payload from those dimensions, so this is a logic
/// invariant.
pub(super) fn parse_float_sentinels(
    actual_size: u32,
    actual_count: u32,
    payload: &[u8],
    position: u64,
) -> Result<FloatSentinels> {
    validate_extension_shape(
        actual_size,
        actual_count,
        FLOAT_SENTINELS_ELEMENT_SIZE,
        FLOAT_SENTINELS_ELEMENT_COUNT,
        position,
    )?;
    debug_assert_eq!(payload.len(), 24);
    let slab = |offset: usize| -> [u8; 8] {
        payload[offset..offset + 8]
            .try_into()
            .expect("envelope validation guarantees a 24-byte payload")
    };
    let sentinels = FloatSentinels::builder()
        .system_missing(slab(FLOAT_SENTINELS_SYSTEM_MISSING_OFFSET))
        .highest(slab(FLOAT_SENTINELS_HIGHEST_OFFSET))
        .lowest(slab(FLOAT_SENTINELS_LOWEST_OFFSET))
        .build();
    Ok(sentinels)
}

/// Parses an extension subtype-5 payload (machine integer info: 8
/// `i32` fields holding version numbers, machine code,
/// floating-point representation, compression code, endianness, and
/// character-set code).
///
/// Validates the envelope against the subtype's spec shape
/// (`element_size == 4`, `element_count == 8`) and decodes each
/// 4-byte slot as an `i32` in the file's byte order. No tagged-code
/// validation happens here — the typed conveniences on the
/// resulting [`MachineIntegerInfo`] return `None` for unrecognized
/// values, and cross-checks against the header live in the
/// dictionary reader so it can emit warnings against [`SavWarning`].
///
/// [`SavWarning`]: crate::spss::sav::sav_warning::SavWarning
///
/// # Errors
///
/// Returns [`FormatErrorKind::UnexpectedValue`] when the envelope
/// shape disagrees with the spec.
///
/// # Panics
///
/// Panics in debug builds if `payload.len()` does not equal
/// `actual_size * actual_count`. The caller (the dictionary reader)
/// reads the payload from those dimensions, so this is a logic
/// invariant.
pub(super) fn parse_machine_integer_info(
    actual_size: u32,
    actual_count: u32,
    payload: &[u8],
    byte_order: ByteOrder,
    position: u64,
) -> Result<MachineIntegerInfo> {
    validate_extension_shape(
        actual_size,
        actual_count,
        MACHINE_INTEGER_INFO_ELEMENT_SIZE,
        MACHINE_INTEGER_INFO_ELEMENT_COUNT,
        position,
    )?;
    debug_assert_eq!(payload.len(), 32);
    let i32_at = |offset: usize| -> i32 {
        let bytes: [u8; 4] = payload[offset..offset + 4]
            .try_into()
            .expect("envelope validation guarantees a 32-byte payload");
        byte_order.read_i32(bytes)
    };
    let info = MachineIntegerInfo::builder()
        .version_major(i32_at(0))
        .version_minor(i32_at(4))
        .version_revision(i32_at(8))
        .machine_code(i32_at(12))
        .floating_point_representation(i32_at(16))
        .compression_code(i32_at(20))
        .endianness(i32_at(24))
        .character_code(i32_at(28))
        .build();
    Ok(info)
}

/// Parses an extension subtype-20 payload (the file's declared
/// character encoding name).
///
/// The on-disk shape is a fixed-`element_size`-of-1 byte string
/// containing the encoding name in ASCII (e.g., `"UTF-8"`,
/// `"windows-1252"`). The string is not null-terminated; some
/// writers right-pad it with spaces or NULs.
///
/// This helper validates `actual_size == 1`, trims trailing spaces
/// and NULs, and decodes the remaining bytes as UTF-8 with the
/// `String::from_utf8_lossy` replacement strategy. Encoding names
/// are pure ASCII in practice, so a rogue non-ASCII byte becomes
/// U+FFFD rather than failing the read. An empty payload yields an
/// empty string. Reconciliation against the integer-info record's
/// numeric `character_code` and the reader's active encoding belongs
/// to schema finalization, not here.
///
/// # Errors
///
/// Returns [`FormatErrorKind::UnexpectedValue`] tagged
/// [`Field::ExtensionElementSize`] when `actual_size != 1`.
pub(super) fn parse_character_encoding(
    actual_size: u32,
    payload: &[u8],
    position: u64,
) -> Result<String> {
    if actual_size != CHARACTER_ENCODING_ELEMENT_SIZE {
        let error = SavError::format(
            Section::Dictionary,
            position,
            FormatErrorKind::UnexpectedValue {
                field: Field::ExtensionElementSize,
            },
        );
        return Err(error);
    }
    let trimmed = trim_trailing_padding(payload);
    let name = String::from_utf8_lossy(trimmed).into_owned();
    Ok(name)
}

/// Parses an extension subtype-13 payload (long-variable-name
/// mappings).
///
/// The on-disk shape is a fixed-`element_size`-of-1 byte stream of
/// [`LONG_VARIABLE_NAMES_PAIR_SEPARATOR`]-separated pairs, each
/// holding a `short`[`LONG_VARIABLE_NAMES_KEY_VALUE_SEPARATOR`]`long`
/// mapping. A trailing pair separator (the optional terminating tab
/// PSPP's grammar permits) is accepted without warning.
///
/// This helper validates `actual_size == 1` and decodes each half
/// through `encoding`. PSPP's character-class constraints on short
/// names and the 8 / 64 byte length limits are *not* enforced —
/// finalization or user code is responsible for validating that the
/// short names match real variables. Duplicate short names are
/// preserved in declaration order; the streaming layer records what
/// disk said.
///
/// # Errors
///
/// * [`FormatErrorKind::UnexpectedValue`] tagged
///   [`Field::ExtensionElementSize`] when `actual_size != 1`.
/// * [`FormatErrorKind::UnexpectedValue`] tagged
///   [`Field::LongVariableNamePair`] when a non-empty pair lacks
///   a [`LONG_VARIABLE_NAMES_KEY_VALUE_SEPARATOR`], has an empty
///   key, or has an empty value.
pub(super) fn parse_long_variable_names(
    actual_size: u32,
    payload: &[u8],
    encoding: &'static Encoding,
    position: u64,
) -> Result<Vec<LongVariableName>> {
    if actual_size != LONG_VARIABLE_NAMES_ELEMENT_SIZE {
        let error = SavError::format(
            Section::Dictionary,
            position,
            FormatErrorKind::UnexpectedValue {
                field: Field::ExtensionElementSize,
            },
        );
        return Err(error);
    }
    let mut mappings: Vec<LongVariableName> = Vec::new();
    let pairs = payload.split(|&b| b == LONG_VARIABLE_NAMES_PAIR_SEPARATOR);
    for pair in pairs {
        // A trailing pair separator yields a final empty segment;
        // that's the optional terminator PSPP's grammar permits, not
        // a malformed pair.
        if pair.is_empty() {
            continue;
        }
        let eq_index = pair
            .iter()
            .position(|&b| b == LONG_VARIABLE_NAMES_KEY_VALUE_SEPARATOR);
        let Some(eq_index) = eq_index else {
            let error = SavError::format(
                Section::Dictionary,
                position,
                FormatErrorKind::UnexpectedValue {
                    field: Field::LongVariableNamePair,
                },
            );
            return Err(error);
        };
        let (key_bytes, rest) = pair.split_at(eq_index);
        let value_bytes = &rest[1..];
        if key_bytes.is_empty() || value_bytes.is_empty() {
            let error = SavError::format(
                Section::Dictionary,
                position,
                FormatErrorKind::UnexpectedValue {
                    field: Field::LongVariableNamePair,
                },
            );
            return Err(error);
        }
        let (short_cow, _, _) = encoding.decode(key_bytes);
        let (long_cow, _, _) = encoding.decode(value_bytes);
        let mapping = LongVariableName::builder()
            .short_name(short_cow.into_owned())
            .long_name(long_cow.into_owned())
            .build();
        mappings.push(mapping);
    }
    Ok(mappings)
}

/// Parses an extension subtype-14 payload (very-long-string widths)
/// into [`VeryLongString`] declarations.
///
/// The on-disk shape is `short=width` pairs joined by
/// [`VERY_LONG_STRINGS_PAIR_SEPARATOR`] (a tab), with the width
/// written as ASCII decimal digits. SPSS terminates each pair with a
/// NUL before the tab; any run of trailing NULs in a pair is
/// trimmed, and a trailing separator is permitted. The short name is
/// decoded through `encoding`; the width is *not* validated against
/// the schema (that it exceeds 255, or matches the variable's
/// declared segments) — finalization, which knows the variables,
/// reconciles the declarations.
///
/// # Errors
///
/// * [`FormatErrorKind::UnexpectedValue`] tagged
///   [`Field::ExtensionElementSize`] when `actual_size != 1`.
/// * [`FormatErrorKind::UnexpectedValue`] tagged
///   [`Field::VeryLongStringPair`] when a non-empty pair lacks a
///   [`VERY_LONG_STRINGS_KEY_VALUE_SEPARATOR`], has an empty key or
///   width, or has a width that isn't decimal digits or doesn't fit
///   in a `u32`.
pub(super) fn parse_very_long_strings(
    actual_size: u32,
    payload: &[u8],
    encoding: &'static Encoding,
    position: u64,
) -> Result<Vec<VeryLongString>> {
    if actual_size != VERY_LONG_STRINGS_ELEMENT_SIZE {
        let error = SavError::format(
            Section::Dictionary,
            position,
            FormatErrorKind::UnexpectedValue {
                field: Field::ExtensionElementSize,
            },
        );
        return Err(error);
    }
    let pair_error = || {
        SavError::format(
            Section::Dictionary,
            position,
            FormatErrorKind::UnexpectedValue {
                field: Field::VeryLongStringPair,
            },
        )
    };
    let mut declarations: Vec<VeryLongString> = Vec::new();
    let pairs = payload.split(|&b| b == VERY_LONG_STRINGS_PAIR_SEPARATOR);
    for pair in pairs {
        let trimmed_len = pair
            .iter()
            .rposition(|&b| b != VERY_LONG_STRINGS_PAIR_PADDING)
            .map_or(0, |index| index + 1);
        let pair = &pair[..trimmed_len];
        // A pair that's empty after trimming is the NUL terminator
        // of the preceding pair or the optional trailing separator,
        // not a malformed pair.
        if pair.is_empty() {
            continue;
        }
        let eq_index = pair
            .iter()
            .position(|&b| b == VERY_LONG_STRINGS_KEY_VALUE_SEPARATOR);
        let Some(eq_index) = eq_index else {
            return Err(pair_error());
        };
        let (key_bytes, rest) = pair.split_at(eq_index);
        let width_bytes = &rest[1..];
        if key_bytes.is_empty() || width_bytes.is_empty() {
            return Err(pair_error());
        }
        let mut width: u32 = 0;
        for &byte in width_bytes {
            if !byte.is_ascii_digit() {
                return Err(pair_error());
            }
            let digit = u32::from(byte - b'0');
            width = width
                .checked_mul(10)
                .and_then(|w| w.checked_add(digit))
                .ok_or_else(pair_error)?;
        }
        let (short_cow, _, _) = encoding.decode(key_bytes);
        let declaration = VeryLongString::builder()
            .short_name(short_cow.into_owned())
            .width(width)
            .build();
        declarations.push(declaration);
    }
    Ok(declarations)
}

/// Parses an extension subtype-11 payload (per-variable display
/// parameters) into a wire-level [`RawDisplayParameters`].
///
/// The on-disk shape is `element_count` `u32` values, each carrying
/// either a measurement-level code, an optional display-width, or an
/// alignment code. The 2-tuple vs. 3-tuple choice is per-record (all
/// variables get a width or none do) and cannot be recovered without
/// the dictionary's variable count, so the streaming layer preserves
/// the raw values verbatim and defers per-variable slicing to schema
/// finalization.
///
/// # Errors
///
/// Returns [`FormatErrorKind::UnexpectedValue`] tagged
/// [`Field::ExtensionElementSize`] when `actual_size != 4`. The
/// `element_count` is *not* validated here — finalization, which
/// knows the variable count, decides whether the count is consistent
/// with a 2-tuple or 3-tuple form.
///
/// # Panics
///
/// Panics in debug builds if `payload.len()` does not equal
/// `actual_size * payload_count`, where `payload_count` is implied
/// by the caller having read the payload to that exact size.
pub(super) fn parse_display_parameters(
    actual_size: u32,
    payload: &[u8],
    byte_order: ByteOrder,
    position: u64,
) -> Result<RawDisplayParameters> {
    if actual_size != DISPLAY_PARAMETERS_ELEMENT_SIZE {
        let error = SavError::format(
            Section::Dictionary,
            position,
            FormatErrorKind::UnexpectedValue {
                field: Field::ExtensionElementSize,
            },
        );
        return Err(error);
    }
    debug_assert_eq!(payload.len() % 4, 0);
    let values: Vec<u32> = payload
        .chunks_exact(4)
        .map(|chunk| {
            let bytes: [u8; 4] = chunk.try_into().expect("chunks_exact yields 4-byte slices");
            byte_order.read_u32(bytes)
        })
        .collect();
    let record = RawDisplayParameters::builder().values(values).build();
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos() -> u64 {
        0
    }

    #[test]
    fn variable_type_continuation() {
        assert_eq!(
            parse_variable_type(-1, pos()).unwrap(),
            VariableTypeCode::Continuation
        );
    }

    #[test]
    fn variable_type_numeric() {
        assert_eq!(
            parse_variable_type(0, pos()).unwrap(),
            VariableTypeCode::Numeric
        );
    }

    #[test]
    fn variable_type_strings() {
        assert_eq!(
            parse_variable_type(1, pos()).unwrap(),
            VariableTypeCode::String(1)
        );
        assert_eq!(
            parse_variable_type(255, pos()).unwrap(),
            VariableTypeCode::String(255)
        );
    }

    #[test]
    fn variable_type_rejects_out_of_range() {
        for bad in [-2, 256, 1_000, i32::MIN, i32::MAX] {
            let err = parse_variable_type(bad, pos()).unwrap_err();
            match err {
                SavError::Format(e) => {
                    assert_eq!(
                        e.kind(),
                        FormatErrorKind::UnexpectedValue {
                            field: Field::VariableType
                        }
                    );
                }
                _ => panic!("expected Format error for {bad}"),
            }
        }
    }

    #[test]
    fn missing_value_count_known_values() {
        assert_eq!(
            parse_missing_value_count(0, pos()).unwrap(),
            MissingValueCount::None
        );
        assert_eq!(
            parse_missing_value_count(1, pos()).unwrap(),
            MissingValueCount::Discrete(1)
        );
        assert_eq!(
            parse_missing_value_count(2, pos()).unwrap(),
            MissingValueCount::Discrete(2)
        );
        assert_eq!(
            parse_missing_value_count(3, pos()).unwrap(),
            MissingValueCount::Discrete(3)
        );
        assert_eq!(
            parse_missing_value_count(-2, pos()).unwrap(),
            MissingValueCount::Range
        );
        assert_eq!(
            parse_missing_value_count(-3, pos()).unwrap(),
            MissingValueCount::RangeWithDiscrete
        );
    }

    #[test]
    fn missing_value_count_undocumented_minus_one_collapses_to_discrete() {
        assert_eq!(
            parse_missing_value_count(-1, pos()).unwrap(),
            MissingValueCount::Discrete(1)
        );
    }

    #[test]
    fn missing_value_count_rejects_out_of_range() {
        for bad in [-4, 4, 10, i32::MIN, i32::MAX] {
            let err = parse_missing_value_count(bad, pos()).unwrap_err();
            match err {
                SavError::Format(e) => {
                    assert_eq!(
                        e.kind(),
                        FormatErrorKind::UnexpectedValue {
                            field: Field::MissingValueCount
                        }
                    );
                }
                _ => panic!("expected Format error for {bad}"),
            }
        }
    }

    #[test]
    fn missing_value_count_entry_count() {
        assert_eq!(MissingValueCount::None.entry_count(), 0);
        assert_eq!(MissingValueCount::Discrete(1).entry_count(), 1);
        assert_eq!(MissingValueCount::Discrete(2).entry_count(), 2);
        assert_eq!(MissingValueCount::Discrete(3).entry_count(), 3);
        assert_eq!(MissingValueCount::Range.entry_count(), 2);
        assert_eq!(MissingValueCount::RangeWithDiscrete.entry_count(), 3);
    }

    #[test]
    fn has_label_zero_and_nonzero() {
        assert!(!parse_has_label(0));
        assert!(parse_has_label(1));
        // ReadStat treats any non-zero value as true.
        assert!(parse_has_label(2));
        assert!(parse_has_label(-1));
    }

    #[test]
    fn short_name_trims_trailing_spaces() {
        let name = parse_short_name(*b"ABC     ", encoding_rs::WINDOWS_1252);
        assert_eq!(name, "ABC");
    }

    #[test]
    fn short_name_trims_trailing_nuls() {
        let name = parse_short_name(*b"ABC\0\0\0\0\0", encoding_rs::WINDOWS_1252);
        assert_eq!(name, "ABC");
    }

    #[test]
    fn short_name_empty_when_all_padding() {
        let name = parse_short_name([b' '; 8], encoding_rs::WINDOWS_1252);
        assert!(name.is_empty());
    }

    #[test]
    fn short_name_full_eight_bytes() {
        let name = parse_short_name(*b"ABCDEFGH", encoding_rs::WINDOWS_1252);
        assert_eq!(name, "ABCDEFGH");
    }

    #[test]
    fn sav_format_unpacks_decimals_width_kind() {
        // F8.2 in PSPP encoding: kind=F (=5), width=8, decimals=2
        // packing: byte 0 = decimals, byte 1 = width, byte 2 = kind
        let packed = u32::from_le_bytes([2, 8, 5, 0]);
        let fmt = parse_sav_format(packed);
        assert_eq!(fmt.kind(), SavFormatKind::F);
        assert_eq!(fmt.width(), 8);
        assert_eq!(fmt.decimals(), 2);
    }

    #[test]
    fn sav_format_round_trips_unknown_kind() {
        let packed = u32::from_le_bytes([0, 1, 99, 0]);
        let fmt = parse_sav_format(packed);
        assert_eq!(fmt.kind(), SavFormatKind::Unknown(99));
    }

    #[test]
    fn compose_none_yields_none() {
        let result = compose_raw_missing_values(MissingValueCount::None, vec![]);
        assert_eq!(result, RawMissingValues::None);
    }

    #[test]
    fn compose_discrete_preserves_entry_order() {
        let entries = vec![[1; 8], [2; 8], [3; 8]];
        let result = compose_raw_missing_values(MissingValueCount::Discrete(3), entries.clone());
        assert_eq!(result, RawMissingValues::Discrete(entries));
    }

    #[test]
    fn compose_range_splits_endpoints() {
        let result = compose_raw_missing_values(MissingValueCount::Range, vec![[1; 8], [2; 8]]);
        assert_eq!(
            result,
            RawMissingValues::Range {
                low: [1; 8],
                high: [2; 8]
            }
        );
    }

    #[test]
    fn value_label_entry_size_zero_length_label_fills_one_alignment_block() {
        // 8 (value) + 1 (length byte alone, padded to 8) = 16.
        assert_eq!(value_label_entry_size(0), 16);
    }

    #[test]
    fn value_label_entry_size_packs_label_into_first_alignment_block() {
        // length byte + 7 bytes of label fits in exactly one 8-byte
        // block, so total is 8 (value) + 8 = 16.
        assert_eq!(value_label_entry_size(7), 16);
    }

    #[test]
    fn value_label_entry_size_overflows_to_second_alignment_block() {
        // length byte + 8 bytes of label needs 9 bytes → padded to
        // 16, so total is 8 (value) + 16 = 24.
        assert_eq!(value_label_entry_size(8), 24);
    }

    #[test]
    fn value_label_entry_size_maximum_label_length() {
        // Maximum unpadded label length is 255 (it's a u8). 255 + 1
        // = 256, which is already a multiple of 8.
        assert_eq!(value_label_entry_size(255), 8 + 256);
    }

    #[test]
    fn parse_value_label_entry_decodes_label_and_carries_value_verbatim() {
        let value = [1, 2, 3, 4, 5, 6, 7, 8];
        let label = b"hello\0\0\0";
        let entry = parse_value_label_entry(value, 5, label, encoding_rs::WINDOWS_1252);
        assert_eq!(entry.value(), value);
        assert_eq!(entry.label(), "hello");
    }

    #[test]
    fn parse_value_label_entry_ignores_padding_past_unpadded_len() {
        // Trailing bytes after the unpadded length must not appear
        // in the decoded label, even if they are non-NUL.
        let value = [0; 8];
        let label = b"ABCDEFGH";
        let entry = parse_value_label_entry(value, 3, label, encoding_rs::WINDOWS_1252);
        assert_eq!(entry.label(), "ABC");
    }

    #[test]
    fn parse_value_label_entry_empty_label() {
        let entry = parse_value_label_entry([0; 8], 0, &[0u8; 7], encoding_rs::WINDOWS_1252);
        assert!(entry.label().is_empty());
    }

    #[test]
    fn parse_value_label_entry_uses_supplied_encoding() {
        // `0xE9` is `é` in Windows-1252 but invalid in UTF-8.
        let entry = parse_value_label_entry([0; 8], 1, &[0xE9], encoding_rs::WINDOWS_1252);
        assert_eq!(entry.label(), "é");
    }

    #[test]
    fn normalize_value_label_variable_indices_translates_to_logical_positions() {
        // Two variables: a width-32 string (1 primary + 3 continuations)
        // followed by a numeric. The numeric's 0-based physical
        // position is 4, so its 1-based physical position is 5.
        let primaries = vec![0, 4];
        let raw = vec![1, 5];
        let out = normalize_value_label_variable_indices(&raw, &primaries, 0).unwrap();
        assert_eq!(out, vec![0, 1]);
    }

    #[test]
    fn normalize_value_label_variable_indices_preserves_duplicates() {
        let primaries = vec![0, 1, 2];
        let raw = vec![1, 3, 1];
        let out = normalize_value_label_variable_indices(&raw, &primaries, 0).unwrap();
        assert_eq!(out, vec![0, 2, 0]);
    }

    #[test]
    fn normalize_value_label_variable_indices_rejects_zero() {
        let err = normalize_value_label_variable_indices(&[0], &[0], 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(e.kind(), FormatErrorKind::DanglingValueLabel),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn normalize_value_label_variable_indices_rejects_out_of_range() {
        let err = normalize_value_label_variable_indices(&[3], &[0, 1], 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(e.kind(), FormatErrorKind::DanglingValueLabel),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn normalize_value_label_variable_indices_rejects_continuation_position() {
        // Primaries at physical positions 0 and 4 (with continuations
        // filling 1..=3). A type-4 index of 2 (1-based) → 1 (0-based)
        // lands on a continuation and must error.
        let err = normalize_value_label_variable_indices(&[2], &[0, 4], 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(e.kind(), FormatErrorKind::DanglingValueLabel),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn parse_character_encoding_decodes_ascii_name() {
        let name = parse_character_encoding(1, b"UTF-8", 0).unwrap();
        assert_eq!(name, "UTF-8");
    }

    #[test]
    fn parse_character_encoding_trims_trailing_spaces_and_nuls() {
        let name = parse_character_encoding(1, b"UTF-8 \0  ", 0).unwrap();
        assert_eq!(name, "UTF-8");
    }

    #[test]
    fn parse_character_encoding_accepts_empty_payload() {
        let name = parse_character_encoding(1, &[], 0).unwrap();
        assert!(name.is_empty());
    }

    #[test]
    fn parse_character_encoding_lossy_on_non_ascii_bytes() {
        // 0xFF is invalid UTF-8 and not a sensible encoding-name byte;
        // the parser should emit U+FFFD rather than failing the read.
        let name = parse_character_encoding(1, b"A\xFFB", 0).unwrap();
        assert_eq!(name, "A\u{FFFD}B");
    }

    #[test]
    fn parse_character_encoding_rejects_wrong_element_size() {
        let err = parse_character_encoding(4, b"UTF-", 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::ExtensionElementSize
                }
            ),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn parse_long_variable_names_single_pair() {
        let payload = b"V1=Variable1";
        let result = parse_long_variable_names(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].short_name(), "V1");
        assert_eq!(result[0].long_name(), "Variable1");
    }

    #[test]
    fn parse_long_variable_names_multiple_pairs() {
        let payload = b"V1=Variable1\tV2=Variable2\tV3=Variable3";
        let result = parse_long_variable_names(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].short_name(), "V1");
        assert_eq!(result[1].short_name(), "V2");
        assert_eq!(result[2].long_name(), "Variable3");
    }

    #[test]
    fn parse_long_variable_names_trailing_separator_accepted() {
        let payload = b"V1=Variable1\tV2=Variable2\t";
        let result = parse_long_variable_names(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn parse_long_variable_names_embedded_equals_in_value_split_on_first() {
        let payload = b"K=v1=more";
        let result = parse_long_variable_names(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].short_name(), "K");
        assert_eq!(result[0].long_name(), "v1=more");
    }

    #[test]
    fn parse_long_variable_names_empty_payload_yields_empty_vec() {
        let result = parse_long_variable_names(1, &[], encoding_rs::WINDOWS_1252, 0).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_long_variable_names_uses_supplied_encoding() {
        // 0xE9 = é in Windows-1252, invalid in standalone UTF-8.
        let payload = b"K=caf\xE9";
        let result = parse_long_variable_names(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result[0].long_name(), "café");
    }

    #[test]
    fn parse_long_variable_names_preserves_duplicates_in_order() {
        let payload = b"V1=First\tV1=Second";
        let result = parse_long_variable_names(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].long_name(), "First");
        assert_eq!(result[1].long_name(), "Second");
    }

    #[test]
    fn parse_long_variable_names_rejects_missing_equals() {
        let payload = b"V1=ok\tV2only";
        let err = parse_long_variable_names(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::LongVariableNamePair
                }
            ),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn parse_long_variable_names_rejects_empty_key() {
        let payload = b"=Variable1";
        let err = parse_long_variable_names(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::LongVariableNamePair
                }
            ),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn parse_long_variable_names_rejects_empty_value() {
        let payload = b"V1=";
        let err = parse_long_variable_names(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::LongVariableNamePair
                }
            ),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn parse_long_variable_names_rejects_wrong_element_size() {
        let err = parse_long_variable_names(4, b"V1=L", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::ExtensionElementSize
                }
            ),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn parse_very_long_strings_single_pair() {
        let payload = b"RESPONSE=00226";
        let result = parse_very_long_strings(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].short_name(), "RESPONSE");
        assert_eq!(result[0].width(), 226);
    }

    #[test]
    fn parse_very_long_strings_spss_nul_terminated_pairs() {
        // SPSS terminates every pair with a NUL before the tab,
        // including the last.
        let payload = b"V1=00300\0\tV2=01000\0\t";
        let result = parse_very_long_strings(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        let pairs: Vec<(&str, u32)> = result.iter().map(|d| (d.short_name(), d.width())).collect();
        assert_eq!(pairs, vec![("V1", 300), ("V2", 1000)]);
    }

    #[test]
    fn parse_very_long_strings_plain_tab_separators() {
        let payload = b"V1=300\tV2=1000";
        let result = parse_very_long_strings(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[1].width(), 1000);
    }

    #[test]
    fn parse_very_long_strings_multiple_nuls_before_separator() {
        let payload = b"V1=300\0\0\0\tV2=1000";
        let result = parse_very_long_strings(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].width(), 300);
    }

    #[test]
    fn parse_very_long_strings_trailing_nuls_without_separator() {
        let payload = b"V1=300\0\0";
        let result = parse_very_long_strings(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].width(), 300);
    }

    #[test]
    fn parse_very_long_strings_empty_payload_yields_empty_vec() {
        let result = parse_very_long_strings(1, &[], encoding_rs::WINDOWS_1252, 0).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_very_long_strings_decodes_key_through_supplied_encoding() {
        // 0xE9 = é in Windows-1252, invalid in standalone UTF-8.
        let payload = b"CAF\xE9=300";
        let result = parse_very_long_strings(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result[0].short_name(), "CAFé");
    }

    #[test]
    fn parse_very_long_strings_preserves_duplicates_in_order() {
        let payload = b"V1=300\tV1=400";
        let result = parse_very_long_strings(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].width(), 300);
        assert_eq!(result[1].width(), 400);
    }

    #[test]
    fn parse_very_long_strings_maximum_width_accepted() {
        let payload = b"V1=4294967295";
        let result = parse_very_long_strings(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result[0].width(), u32::MAX);
    }

    #[test]
    fn parse_very_long_strings_rejects_missing_equals() {
        let payload = b"V1=300\tV2only";
        let err = parse_very_long_strings(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::VeryLongStringPair
                }
            ),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn parse_very_long_strings_rejects_empty_key() {
        let payload = b"=300";
        let err = parse_very_long_strings(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::VeryLongStringPair
                }
            ),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn parse_very_long_strings_rejects_empty_width() {
        let payload = b"V1=";
        let err = parse_very_long_strings(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::VeryLongStringPair
                }
            ),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn parse_very_long_strings_rejects_non_digit_width() {
        let payload = b"V1=3O0";
        let err = parse_very_long_strings(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::VeryLongStringPair
                }
            ),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn parse_very_long_strings_rejects_width_overflowing_u32() {
        let payload = b"V1=4294967296";
        let err = parse_very_long_strings(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::VeryLongStringPair
                }
            ),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn parse_very_long_strings_interior_nul_preserved_in_key() {
        // NULs are trimmed only from a pair's end; an interior NUL
        // is not silently dropped into a different name. (Like
        // subtype 13, no character-class enforcement at streaming.)
        let payload = b"V\x001=300";
        let result = parse_very_long_strings(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result[0].short_name(), "V\u{0}1");
    }

    #[test]
    fn parse_very_long_strings_rejects_wrong_element_size() {
        let err = parse_very_long_strings(4, b"V1=300", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::ExtensionElementSize
                }
            ),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn parse_display_parameters_collects_two_tuple_form_little_endian() {
        // Two variables in 2-tuple form: (measure, alignment) pairs.
        let mut payload = Vec::new();
        payload.extend_from_slice(&1u32.to_le_bytes()); // measure
        payload.extend_from_slice(&0u32.to_le_bytes()); // alignment
        payload.extend_from_slice(&3u32.to_le_bytes()); // measure
        payload.extend_from_slice(&1u32.to_le_bytes()); // alignment
        let raw = parse_display_parameters(4, &payload, ByteOrder::LittleEndian, 0).unwrap();
        assert_eq!(raw.values(), &[1, 0, 3, 1]);
    }

    #[test]
    fn parse_display_parameters_collects_three_tuple_form_little_endian() {
        // One variable in 3-tuple form: measure, width, alignment.
        let mut payload = Vec::new();
        payload.extend_from_slice(&2u32.to_le_bytes()); // measure
        payload.extend_from_slice(&12u32.to_le_bytes()); // width
        payload.extend_from_slice(&1u32.to_le_bytes()); // alignment
        let raw = parse_display_parameters(4, &payload, ByteOrder::LittleEndian, 0).unwrap();
        assert_eq!(raw.values(), &[2, 12, 1]);
    }

    #[test]
    fn parse_display_parameters_big_endian() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&100u32.to_be_bytes());
        payload.extend_from_slice(&200u32.to_be_bytes());
        let raw = parse_display_parameters(4, &payload, ByteOrder::BigEndian, 0).unwrap();
        assert_eq!(raw.values(), &[100, 200]);
    }

    #[test]
    fn parse_display_parameters_empty_payload_yields_empty_record() {
        let raw = parse_display_parameters(4, &[], ByteOrder::LittleEndian, 0).unwrap();
        assert!(raw.values().is_empty());
    }

    #[test]
    fn parse_display_parameters_preserves_unrecognized_codes() {
        // Codes 99 / 7 don't match any MeasurementLevel or Alignment
        // variant; the streaming layer carries them verbatim so
        // finalization decides whether to bucket them as
        // MeasurementLevel::Unknown(99) / Alignment::Unknown(7).
        let mut payload = Vec::new();
        payload.extend_from_slice(&99u32.to_le_bytes());
        payload.extend_from_slice(&7u32.to_le_bytes());
        let raw = parse_display_parameters(4, &payload, ByteOrder::LittleEndian, 0).unwrap();
        assert_eq!(raw.values(), &[99, 7]);
    }

    #[test]
    fn parse_display_parameters_rejects_wrong_element_size() {
        let err = parse_display_parameters(8, &[0; 16], ByteOrder::LittleEndian, 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::ExtensionElementSize
                }
            ),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn compose_range_with_discrete_splits_all_three() {
        let result = compose_raw_missing_values(
            MissingValueCount::RangeWithDiscrete,
            vec![[1; 8], [2; 8], [3; 8]],
        );
        assert_eq!(
            result,
            RawMissingValues::RangeWithDiscrete {
                low: [1; 8],
                high: [2; 8],
                discrete: [3; 8]
            }
        );
    }
}
