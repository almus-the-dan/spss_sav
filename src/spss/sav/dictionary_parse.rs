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

use crate::spss::sav::byte_cursor::ByteCursor;
use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::dictionary_format::{
    ATTRIBUTE_NAME_TERMINATOR, ATTRIBUTE_VALUE_QUOTE, ATTRIBUTE_VALUE_TERMINATOR,
    ATTRIBUTE_VALUES_CLOSE,
    DATA_FILE_ATTRIBUTES_ELEMENT_SIZE, DISPLAY_PARAMETERS_ELEMENT_SIZE,
    EXTENDED_NUMBER_OF_CASES_COUNT_OFFSET, EXTENDED_NUMBER_OF_CASES_ELEMENT_COUNT,
    EXTENDED_NUMBER_OF_CASES_ELEMENT_SIZE, EXTENDED_NUMBER_OF_CASES_VERSION_OFFSET,
    EXTRA_PRODUCT_INFO_ELEMENT_SIZE, FLOAT_SENTINELS_ELEMENT_COUNT, FLOAT_SENTINELS_ELEMENT_SIZE,
    FLOAT_SENTINELS_HIGHEST_OFFSET,
    FLOAT_SENTINELS_LOWEST_OFFSET, FLOAT_SENTINELS_SYSTEM_MISSING_OFFSET,
    FORMAT_CODE_DECIMALS_BYTE, FORMAT_CODE_KIND_BYTE, FORMAT_CODE_WIDTH_BYTE,
    LONG_STRING_MISSING_VALUE_MAX_COUNT, LONG_STRING_MISSING_VALUES_ELEMENT_SIZE,
    LONG_STRING_VALUE_LABELS_ELEMENT_SIZE, LONG_VARIABLE_NAMES_ELEMENT_SIZE,
    LONG_VARIABLE_NAMES_KEY_VALUE_SEPARATOR,
    LONG_VARIABLE_NAMES_PAIR_SEPARATOR, MACHINE_INTEGER_INFO_ELEMENT_COUNT,
    MACHINE_INTEGER_INFO_ELEMENT_SIZE, MULTIPLE_RESPONSE_SET_TYPE_CATEGORY,
    MULTIPLE_RESPONSE_SET_TYPE_DICHOTOMY_COUNTED_VALUES,
    MULTIPLE_RESPONSE_SET_TYPE_DICHOTOMY_VARIABLE_LABELS, MULTIPLE_RESPONSE_SETS_ELEMENT_SIZE,
    MULTIPLE_RESPONSE_SETS_FIELD_SEPARATOR, MULTIPLE_RESPONSE_SETS_LINE_SEPARATOR,
    MULTIPLE_RESPONSE_SETS_NAME_TERMINATOR, UUID_ELEMENT_SIZE, VALUE_LABEL_ENTRY_ALIGNMENT,
    VALUE_LABEL_LABEL_LEN_FIELD_LEN, VALUE_LABEL_VALUE_LEN, VARIABLE_TYPE_CONTINUATION,
    VARIABLE_ATTRIBUTES_ELEMENT_SIZE, VARIABLE_ATTRIBUTES_NAME_TERMINATOR,
    VARIABLE_ATTRIBUTES_SET_SEPARATOR, VARIABLE_SETS_ELEMENT_SIZE,
    VARIABLE_SETS_LINE_CARRIAGE_RETURN, VARIABLE_SETS_LINE_SEPARATOR,
    VARIABLE_SETS_MEMBER_SEPARATOR, VARIABLE_SETS_NAME_TERMINATOR, VARIABLE_TYPE_NUMERIC,
    VARIABLE_TYPE_STRING_MAX,
    VERY_LONG_STRINGS_ELEMENT_SIZE, VERY_LONG_STRINGS_KEY_VALUE_SEPARATOR,
    VERY_LONG_STRINGS_PAIR_PADDING, VERY_LONG_STRINGS_PAIR_SEPARATOR,
};
use crate::spss::sav::extensions::extended_number_of_cases::ExtendedNumberOfCases;
use crate::spss::sav::extensions::extra_product_info::ExtraProductInfo;
use crate::spss::sav::extensions::file_attribute::FileAttribute;
use crate::spss::sav::extensions::float_sentinels::FloatSentinels;
use crate::spss::sav::extensions::long_missing_value_record::LongMissingValueRecord;
use crate::spss::sav::extensions::long_value_label::LongValueLabel;
use crate::spss::sav::extensions::long_value_label_record::LongValueLabelRecord;
use crate::spss::sav::extensions::long_variable_name::LongVariableName;
use crate::spss::sav::extensions::category_label_source::CategoryLabelSource;
use crate::spss::sav::extensions::machine_integer_info::MachineIntegerInfo;
use crate::spss::sav::extensions::multiple_response_set::MultipleResponseSet;
use crate::spss::sav::extensions::multiple_response_set_kind::MultipleResponseSetKind;
use crate::spss::sav::extensions::raw_display_parameters::RawDisplayParameters;
use crate::spss::sav::extensions::variable_attribute_entry::VariableAttributeEntry;
use crate::spss::sav::extensions::uuid::Uuid;
use crate::spss::sav::extensions::variable_attribute_record::VariableAttributeRecord;
use crate::spss::sav::extensions::variable_set::VariableSet;
use crate::spss::sav::extensions::variable_sets::VariableSets;
use crate::spss::sav::extensions::very_long_string::VeryLongString;
use crate::spss::sav::raw_missing_values::RawMissingValues;
use crate::spss::sav::raw_value_label_entry::RawValueLabelEntry;
use crate::spss::sav::reader_state::u32_as_usize;
use crate::spss::sav::sav_error::{Field, FormatErrorKind, Result, SavError, Section};
use crate::spss::sav::sav_format::SavFormat;
use crate::spss::sav::sav_format_kind::SavFormatKind;
use crate::spss::sav::text_field::decode_trimmed;

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

/// Parses an extension subtype-10 payload (extra product information)
/// into an [`ExtraProductInfo`].
///
/// The payload is a free-form text string whose byte length is exact,
/// so it is decoded through `encoding` verbatim — no trailing padding
/// is trimmed.
///
/// # Errors
///
/// Returns [`FormatErrorKind::UnexpectedValue`] tagged
/// [`Field::ExtensionElementSize`] when `actual_size != 1`.
pub(super) fn parse_extra_product_info(
    actual_size: u32,
    payload: &[u8],
    encoding: &'static Encoding,
    position: u64,
) -> Result<ExtraProductInfo> {
    if actual_size != EXTRA_PRODUCT_INFO_ELEMENT_SIZE {
        return Err(unexpected_value_error(position, Field::ExtensionElementSize));
    }
    let (text, _, _) = encoding.decode(payload);
    let info = ExtraProductInfo::builder().text(text.into_owned()).build();
    Ok(info)
}

/// Parses an extension subtype-12 payload (a file UUID) into a
/// [`Uuid`].
///
/// The payload is the UUID's RFC 4122 text form. It is decoded through
/// `encoding` verbatim — the string is neither trimmed nor validated
/// against RFC 4122, preserving its exact bytes and letter case.
///
/// # Errors
///
/// Returns [`FormatErrorKind::UnexpectedValue`] tagged
/// [`Field::ExtensionElementSize`] when `actual_size != 1`.
pub(super) fn parse_uuid(
    actual_size: u32,
    payload: &[u8],
    encoding: &'static Encoding,
    position: u64,
) -> Result<Uuid> {
    if actual_size != UUID_ELEMENT_SIZE {
        return Err(unexpected_value_error(position, Field::ExtensionElementSize));
    }
    let (text, _, _) = encoding.decode(payload);
    let uuid = Uuid::builder().text(text.into_owned()).build();
    Ok(uuid)
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

/// Parses an extension subtype-17 (data file attributes) payload into
/// its list of [`FileAttribute`]s.
///
/// The payload is a single attribute set: one or more attributes
/// concatenated. Each attribute is a name (everything up to the next
/// `(`) followed, inside parentheses, by one or more values, each a
/// single-quoted string terminated by a line feed (`0x0a`). Only the
/// single outer quote pair is stripped from each value; interior
/// bytes (including any doubled `''`) are kept verbatim. Names are
/// preserved verbatim, including any `[n]` array-index suffix — the
/// index collapse is deferred to schema finalization. Both names and
/// values are decoded through `encoding`.
///
/// # Errors
/// - [`Field::ExtensionElementSize`] when `actual_size` isn't `1`.
/// - [`Field::FileAttribute`] on a structurally malformed attribute
///   (missing `(`, unterminated value group, or a value not properly
///   quoted) — exact policy pinned in the implementation.
pub(super) fn parse_data_file_attributes(
    actual_size: u32,
    payload: &[u8],
    encoding: &'static Encoding,
    position: u64,
) -> Result<Vec<FileAttribute>> {
    if actual_size != DATA_FILE_ATTRIBUTES_ELEMENT_SIZE {
        return Err(unexpected_value_error(position, Field::ExtensionElementSize));
    }
    let mut cursor = payload;
    let mut attributes: Vec<FileAttribute> = Vec::new();
    // Subtype 17 is a single attribute set spanning the whole payload;
    // it has no `/` set separator, so `parse_attribute_set` consumes
    // everything in one pass. The loop only re-enters on the malformed
    // case where a stray `/` ended the set early, in which case we
    // simply continue with the remainder.
    while !cursor.is_empty() {
        let set = parse_attribute_set(&mut cursor, encoding, position, Field::FileAttribute)?;
        for (name, values) in set {
            let attribute = FileAttribute::builder().name(name).values(values).build();
            attributes.push(attribute);
        }
    }
    Ok(attributes)
}

/// Parses an extension subtype-18 (variable attributes) payload into
/// its list of [`VariableAttributeRecord`]s.
///
/// The payload is a sequence of `variable_name:attribute-set` groups,
/// each after the first delimited from the previous by `/`. The
/// attribute set within each group uses the same grammar as subtype
/// 17 (see [`parse_data_file_attributes`]). Variable names and
/// attribute contents are decoded through `encoding`.
///
/// # Errors
/// - [`Field::ExtensionElementSize`] when `actual_size` isn't `1`.
/// - [`Field::VariableAttribute`] on a structurally malformed group
///   (missing `:`, missing `(`, unterminated value group, or a value
///   not properly quoted) — exact policy pinned in the
///   implementation.
pub(super) fn parse_variable_attributes(
    actual_size: u32,
    payload: &[u8],
    encoding: &'static Encoding,
    position: u64,
) -> Result<Vec<VariableAttributeRecord>> {
    if actual_size != VARIABLE_ATTRIBUTES_ELEMENT_SIZE {
        return Err(unexpected_value_error(position, Field::ExtensionElementSize));
    }
    let mut cursor = payload;
    let mut records: Vec<VariableAttributeRecord> = Vec::new();
    while !cursor.is_empty() {
        let record = parse_variable_attribute(&mut cursor, encoding, position)?;
        records.push(record);
    }
    Ok(records)
}

/// Parses one `variable_name:attribute-set` group from `cursor` into a
/// [`VariableAttributeRecord`], advancing the cursor past the group
/// (including its trailing `/` set separator, if present). The name
/// runs up to the `:` that precedes its attributes.
fn parse_variable_attribute(
    cursor: &mut &[u8],
    encoding: &'static Encoding,
    position: u64,
) -> Result<VariableAttributeRecord> {
    let name_end = cursor
        .iter()
        .position(|&b| b == VARIABLE_ATTRIBUTES_NAME_TERMINATOR);
    let Some(name_end) = name_end
    else {
        return Err(unexpected_value_error(position, Field::VariableAttribute));
    };
    let name_bytes = &cursor[..name_end];
    if name_bytes.is_empty() {
        return Err(unexpected_value_error(position, Field::VariableAttribute));
    }
    *cursor = &cursor[name_end + 1..];
    let set = parse_attribute_set(cursor, encoding, position, Field::VariableAttribute)?;
    let (variable_name, _, _) = encoding.decode(name_bytes);
    let mut builder = VariableAttributeRecord::builder().variable_name(variable_name.into_owned());
    for (name, values) in set {
        let entry = VariableAttributeEntry::builder()
            .name(name)
            .values(values)
            .build();
        builder = builder.attribute(entry);
    }
    Ok(builder.build())
}

/// Builds a dictionary-section `UnexpectedValue` format error tagged
/// with `field`. Shared by the text (subtypes 17/18) and binary
/// (subtypes 21/22) extension parsers.
fn unexpected_value_error(position: u64, field: Field) -> SavError {
    SavError::format(
        Section::Dictionary,
        position,
        FormatErrorKind::UnexpectedValue { field },
    )
}

/// Parses one attribute set (the grammar shared by subtypes 17 and
/// 18) from `cursor`, consuming attributes until the cursor is
/// exhausted or a [`VARIABLE_ATTRIBUTES_SET_SEPARATOR`] (`/`) is
/// reached — the separator is consumed. Returns each attribute as its
/// verbatim (still-`[n]`-suffixed) name paired with its list of
/// values, each value's single outer quote pair already stripped.
/// `field` tags any structural error.
fn parse_attribute_set(
    cursor: &mut &[u8],
    encoding: &'static Encoding,
    position: u64,
    field: Field,
) -> Result<Vec<(String, Vec<String>)>> {
    let mut attributes: Vec<(String, Vec<String>)> = Vec::new();
    while let Some(&first) = cursor.first() {
        if first == VARIABLE_ATTRIBUTES_SET_SEPARATOR {
            *cursor = &cursor[1..];
            break;
        }
        // The attribute name runs up to the `(` that opens its values.
        let name_end = cursor.iter().position(|&b| b == ATTRIBUTE_NAME_TERMINATOR);
        let Some(name_end) = name_end else {
            return Err(unexpected_value_error(position, field));
        };
        let name_bytes = &cursor[..name_end];
        if name_bytes.is_empty() {
            return Err(unexpected_value_error(position, field));
        }
        *cursor = &cursor[name_end + 1..];
        let values = parse_attribute_values(cursor, encoding, position, field)?;
        let (name, _, _) = encoding.decode(name_bytes);
        attributes.push((name.into_owned(), values));
    }
    Ok(attributes)
}

/// Parses the parenthesized value list of one attribute, starting with
/// `cursor` positioned just past the opening `(`. Each value is
/// terminated by a line feed; the list ends at the closing `)`, which
/// is consumed. `field` tags any structural error.
fn parse_attribute_values(
    cursor: &mut &[u8],
    encoding: &'static Encoding,
    position: u64,
    field: Field,
) -> Result<Vec<String>> {
    let mut values: Vec<String> = Vec::new();
    loop {
        let value_end = cursor.iter().position(|&b| b == ATTRIBUTE_VALUE_TERMINATOR);
        let Some(value_end) = value_end else {
            return Err(unexpected_value_error(position, field));
        };
        let value = decode_attribute_value(&cursor[..value_end], encoding);
        values.push(value);
        *cursor = &cursor[value_end + 1..];
        match cursor.first() {
            Some(&b) if b == ATTRIBUTE_VALUES_CLOSE => {
                *cursor = &cursor[1..];
                return Ok(values);
            }
            Some(_) => {}
            None => return Err(unexpected_value_error(position, field)),
        }
    }
}

/// Strips the single outer single-quote pair (if both present) from an
/// attribute value and decodes it through `encoding`. Interior bytes
/// are kept verbatim — doubled quotes are not un-doubled, matching
/// PSPP, since values are line-feed-delimited rather than
/// quote-delimited.
fn decode_attribute_value(bytes: &[u8], encoding: &'static Encoding) -> String {
    let inner = if bytes.len() >= 2
        && bytes[0] == ATTRIBUTE_VALUE_QUOTE
        && bytes[bytes.len() - 1] == ATTRIBUTE_VALUE_QUOTE
    {
        &bytes[1..bytes.len() - 1]
    } else {
        bytes
    };
    let (value, _, _) = encoding.decode(inner);
    value.into_owned()
}

/// Parses an extension subtype-5 payload (named variable groupings)
/// into its [`VariableSets`].
///
/// The payload is one set per line: a set name, `=`, a space, then the
/// members' long variable names separated by spaces; each line ends
/// with a line feed, optionally preceded by a carriage return. A set
/// may have no members. Blank lines (including the trailing line
/// feed's empty segment) are skipped. Names and members are decoded
/// through `encoding`; members are not validated against the schema.
///
/// # Errors
///
/// * [`Field::ExtensionElementSize`] when `actual_size != 1`.
/// * [`Field::VariableSet`] when a non-empty line lacks a `=` or has
///   an empty set name.
pub(super) fn parse_variable_sets(
    actual_size: u32,
    payload: &[u8],
    encoding: &'static Encoding,
    position: u64,
) -> Result<VariableSets> {
    if actual_size != VARIABLE_SETS_ELEMENT_SIZE {
        return Err(unexpected_value_error(position, Field::ExtensionElementSize));
    }
    let mut builder = VariableSets::builder();
    for line in payload.split(|&b| b == VARIABLE_SETS_LINE_SEPARATOR) {
        // A carriage return may precede the line feed; a trailing line
        // feed leaves a final empty segment. Neither is a set.
        let line = match line.split_last() {
            Some((&VARIABLE_SETS_LINE_CARRIAGE_RETURN, head)) => head,
            _ => line,
        };
        if line.is_empty() {
            continue;
        }
        let set = parse_variable_set(line, encoding, position)?;
        builder = builder.set(set);
    }
    Ok(builder.build())
}

/// Parses one `name= members...` line into a [`VariableSet`]. The name
/// runs up to the first `=`; a single space after it (the `= `
/// separator) is skipped, and the remaining space-separated tokens are
/// the members (empty tokens from repeated spaces are dropped).
fn parse_variable_set(
    line: &[u8],
    encoding: &'static Encoding,
    position: u64,
) -> Result<VariableSet> {
    let field = Field::VariableSet;
    let name_end = line.iter().position(|&b| b == VARIABLE_SETS_NAME_TERMINATOR);
    let Some(name_end) = name_end else {
        return Err(unexpected_value_error(position, field));
    };
    let name_bytes = &line[..name_end];
    if name_bytes.is_empty() {
        return Err(unexpected_value_error(position, field));
    }
    let members = &line[name_end + 1..];
    // Skip the single space that follows the `=`; any further empty
    // tokens (from repeated spaces) are dropped below.
    let members = match members.split_first() {
        Some((&VARIABLE_SETS_MEMBER_SEPARATOR, rest)) => rest,
        _ => members,
    };
    let mut builder = VariableSet::builder();
    let (name, _, _) = encoding.decode(name_bytes);
    builder = builder.name(name.into_owned());
    let members = members.split(|&b| b == VARIABLE_SETS_MEMBER_SEPARATOR);
    for member in members {
        if member.is_empty() {
            continue;
        }
        let (member, _, _) = encoding.decode(member);
        builder = builder.variable(member.into_owned());
    }
    Ok(builder.build())
}

/// Parses an extension subtype-7 or subtype-19 payload (multiple
/// response sets) into its [`MultipleResponseSet`]s.
///
/// The payload is one set per line, each terminated by a line feed. A
/// line is `$name=<type>...`, where `<type>` is `C` (multiple
/// category), `D` (multiple dichotomy, category labels from variable
/// labels), or `E` (multiple dichotomy, category labels from counted
/// values — subtype 19 only). Counted values and the set label are
/// "counted strings" (`<decimal-length> <space> <that-many-bytes>`);
/// member variable names follow, space-separated. Names (including the
/// leading `$`), labels, counted values, and members are decoded
/// through `encoding`.
///
/// # Errors
///
/// * [`Field::ExtensionElementSize`] when `actual_size != 1`.
/// * [`Field::MultipleResponseSet`] on a malformed set line (missing
///   `=`, empty name, unknown type letter, malformed counted string,
///   or a non-numeric `E` label source).
pub(super) fn parse_multiple_response_sets(
    actual_size: u32,
    payload: &[u8],
    encoding: &'static Encoding,
    position: u64,
) -> Result<Vec<MultipleResponseSet>> {
    if actual_size != MULTIPLE_RESPONSE_SETS_ELEMENT_SIZE {
        return Err(unexpected_value_error(position, Field::ExtensionElementSize));
    }
    let mut sets: Vec<MultipleResponseSet> = Vec::new();
    let lines = payload.split(|&b| b == MULTIPLE_RESPONSE_SETS_LINE_SEPARATOR);
    for line in lines {
        // The trailing line feed leaves a final empty segment.
        if line.is_empty() {
            continue;
        }
        let set = parse_multiple_response_set(line, encoding, position)?;
        sets.push(set);
    }
    Ok(sets)
}

/// Parses one `$name=<type>...` set line into a
/// [`MultipleResponseSet`].
fn parse_multiple_response_set(
    line: &[u8],
    encoding: &'static Encoding,
    position: u64,
) -> Result<MultipleResponseSet> {
    let field = Field::MultipleResponseSet;
    let mut cursor = line;

    let name_bytes = take_before(&mut cursor, MULTIPLE_RESPONSE_SETS_NAME_TERMINATOR)
        .ok_or_else(|| unexpected_value_error(position, field))?;
    if name_bytes.is_empty() {
        return Err(unexpected_value_error(position, field));
    }

    let (&type_byte, after_type) = cursor
        .split_first()
        .ok_or_else(|| unexpected_value_error(position, field))?;
    cursor = after_type;

    let kind = match type_byte {
        MULTIPLE_RESPONSE_SET_TYPE_CATEGORY => read_category_kind(&mut cursor, position, field)?,
        MULTIPLE_RESPONSE_SET_TYPE_DICHOTOMY_VARIABLE_LABELS => {
            read_dichotomy_variable_labels_kind(&mut cursor, encoding, position, field)?
        }
        MULTIPLE_RESPONSE_SET_TYPE_DICHOTOMY_COUNTED_VALUES => {
            read_dichotomy_counted_values_kind(&mut cursor, encoding, position, field)?
        }
        _ => return Err(unexpected_value_error(position, field)),
    };

    let label_bytes = read_counted_string(&mut cursor, position, field)?;
    let (label, _, _) = encoding.decode(label_bytes);
    let (name, _, _) = encoding.decode(name_bytes);

    let mut builder = MultipleResponseSet::builder()
        .name(name.into_owned())
        .label(label.into_owned())
        .kind(kind);
    // The remaining bytes are the member names, space-separated (the
    // leading separator and any empty tokens are dropped).
    let members = cursor.split(|&b| b == MULTIPLE_RESPONSE_SETS_FIELD_SEPARATOR);
    for member in members {
        if member.is_empty() {
            continue;
        }
        let (member, _, _) = encoding.decode(member);
        builder = builder.variable(member.into_owned());
    }
    Ok(builder.build())
}

/// Reads the wire type `C` body — a multiple-category set. It has no
/// counted value; a single field separator precedes the label, which
/// the caller reads next.
fn read_category_kind(
    cursor: &mut &[u8],
    position: u64,
    field: Field,
) -> Result<MultipleResponseSetKind> {
    expect_field_separator(cursor, position, field)?;
    Ok(MultipleResponseSetKind::MultipleCategory)
}

/// Reads the wire type `D` body — a multiple-dichotomy set whose
/// category labels come from variable labels. The counted value (a
/// counted string) immediately follows the type letter, then a field
/// separator before the label.
fn read_dichotomy_variable_labels_kind(
    cursor: &mut &[u8],
    encoding: &'static Encoding,
    position: u64,
    field: Field,
) -> Result<MultipleResponseSetKind> {
    let counted = read_counted_string(cursor, position, field)?;
    expect_field_separator(cursor, position, field)?;
    let (counted_value, _, _) = encoding.decode(counted);
    let kind = MultipleResponseSetKind::MultipleDichotomy {
        counted_value: counted_value.into_owned(),
        category_labels: CategoryLabelSource::VariableLabels,
    };
    Ok(kind)
}

/// Reads the wire type `E` body — a multiple-dichotomy set whose
/// category labels come from counted values (subtype 19 only). A
/// separator and label-source number precede the counted value, which
/// is followed by a separator before the label.
fn read_dichotomy_counted_values_kind(
    cursor: &mut &[u8],
    encoding: &'static Encoding,
    position: u64,
    field: Field,
) -> Result<MultipleResponseSetKind> {
    expect_field_separator(cursor, position, field)?;
    let source_bytes = take_before(cursor, MULTIPLE_RESPONSE_SETS_FIELD_SEPARATOR)
        .ok_or_else(|| unexpected_value_error(position, field))?;
    let label_source =
        parse_ascii_u32(source_bytes).ok_or_else(|| unexpected_value_error(position, field))?;
    let counted = read_counted_string(cursor, position, field)?;
    expect_field_separator(cursor, position, field)?;
    let (counted_value, _, _) = encoding.decode(counted);
    let kind = MultipleResponseSetKind::MultipleDichotomy {
        counted_value: counted_value.into_owned(),
        category_labels: CategoryLabelSource::CountedValues { label_source },
    };
    Ok(kind)
}

/// Reads a "counted string" (`<decimal-length> <space> <that-many
/// -bytes>`) from the front of `cursor`, advancing past it. Errors,
/// tagged `field`, when the length is missing/non-numeric or the bytes
/// run past the end.
fn read_counted_string<'a>(
    cursor: &mut &'a [u8],
    position: u64,
    field: Field,
) -> Result<&'a [u8]> {
    let length_bytes = take_before(cursor, MULTIPLE_RESPONSE_SETS_FIELD_SEPARATOR)
        .ok_or_else(|| unexpected_value_error(position, field))?;
    let length =
        parse_ascii_u32(length_bytes).ok_or_else(|| unexpected_value_error(position, field))?;
    let length = u32_as_usize(length, position, Section::Dictionary, field)?;
    if cursor.len() < length {
        return Err(unexpected_value_error(position, field));
    }
    let (bytes, rest) = cursor.split_at(length);
    *cursor = rest;
    Ok(bytes)
}

/// Splits the bytes before the first `delim` off the front of
/// `cursor`, advancing past both the prefix and the delimiter. Returns
/// `None` when `delim` is absent.
fn take_before<'a>(cursor: &mut &'a [u8], delim: u8) -> Option<&'a [u8]> {
    let position = cursor.iter().position(|&b| b == delim)?;
    let (head, rest) = cursor.split_at(position);
    *cursor = &rest[1..];
    Some(head)
}

/// Consumes a single [`MULTIPLE_RESPONSE_SETS_FIELD_SEPARATOR`] from
/// the front of `cursor`. Errors, tagged `field`, when the next byte
/// isn't that separator.
fn expect_field_separator(cursor: &mut &[u8], position: u64, field: Field) -> Result<()> {
    match cursor.split_first() {
        Some((&b, rest)) if b == MULTIPLE_RESPONSE_SETS_FIELD_SEPARATOR => {
            *cursor = rest;
            Ok(())
        }
        _ => Err(unexpected_value_error(position, field)),
    }
}

/// Parses `bytes` as an unsigned decimal integer. Returns `None` when
/// empty, non-digit, or overflowing `u32`.
fn parse_ascii_u32(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() {
        return None;
    }
    let mut value: u32 = 0;
    for &byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value
            .checked_mul(10)?
            .checked_add(u32::from(byte - b'0'))?;
    }
    Some(value)
}

/// Parses an extension subtype-21 payload (long string value labels)
/// into one [`LongValueLabelRecord`] per variable.
///
/// The payload repeats until exhausted: a `u32`-length-prefixed
/// variable name, a `u32` declared width, a `u32` label count, then
/// that many `(value, label)` pairs where each of value and label is a
/// `u32`-length-prefixed byte string. All `u32` fields honor
/// `byte_order`. Variable names and labels are decoded through
/// `encoding`; the value bytes are kept verbatim.
///
/// # Errors
///
/// * [`Field::ExtensionElementSize`] when `actual_size != 1`.
/// * [`Field::LongValueLabel`] when the payload is truncated (a
///   length prefix or its bytes run past the end).
pub(super) fn parse_long_string_value_labels(
    actual_size: u32,
    payload: &[u8],
    byte_order: ByteOrder,
    encoding: &'static Encoding,
    position: u64,
) -> Result<Vec<LongValueLabelRecord>> {
    if actual_size != LONG_STRING_VALUE_LABELS_ELEMENT_SIZE {
        return Err(unexpected_value_error(position, Field::ExtensionElementSize));
    }
    let mut cursor = ByteCursor::new(payload, Section::Dictionary, position);
    let mut records: Vec<LongValueLabelRecord> = Vec::new();
    while !cursor.is_empty() {
        let record = parse_long_value_label_record(&mut cursor, byte_order, encoding)?;
        records.push(record);
    }
    Ok(records)
}

/// Parses one per-variable [`LongValueLabelRecord`] from `cursor`,
/// advancing past it: a `u32`-length-prefixed variable name, a `u32`
/// declared width, a `u32` label count, then that many `(value,
/// label)` pairs.
fn parse_long_value_label_record(
    cursor: &mut ByteCursor<'_>,
    byte_order: ByteOrder,
    encoding: &'static Encoding,
) -> Result<LongValueLabelRecord> {
    let field = Field::LongValueLabel;
    let name_bytes = cursor.take_length_prefixed(byte_order, field)?;
    let width = cursor.take_u32(byte_order, field)?;
    let label_count = cursor.take_u32(byte_order, field)?;
    let mut labels: Vec<LongValueLabel> = Vec::new();
    for _ in 0..label_count {
        let label = parse_long_value_label(cursor, byte_order, encoding)?;
        labels.push(label);
    }
    let (variable_name, _, _) = encoding.decode(name_bytes);
    let record = LongValueLabelRecord::builder()
        .variable_name(variable_name.into_owned())
        .width(width)
        .labels(labels)
        .build();
    Ok(record)
}

/// Parses one `(value, label)` pair from `cursor`, advancing past it.
/// Both value and label are `u32`-length-prefixed byte strings; the
/// value bytes are kept verbatim and the label is decoded through
/// `encoding`.
fn parse_long_value_label(
    cursor: &mut ByteCursor<'_>,
    byte_order: ByteOrder,
    encoding: &'static Encoding,
) -> Result<LongValueLabel> {
    let field = Field::LongValueLabel;
    let value = cursor.take_length_prefixed(byte_order, field)?;
    let value = value.to_vec();
    let label_bytes = cursor.take_length_prefixed(byte_order, field)?;
    let (label, _, _) = encoding.decode(label_bytes);
    let label = LongValueLabel::builder()
        .value(value)
        .label(label.into_owned())
        .build();
    Ok(label)
}

/// Parses an extension subtype-22 payload (long string missing values)
/// into one [`LongMissingValueRecord`] per variable.
///
/// The payload repeats until exhausted: a `u32`-length-prefixed
/// variable name, a single count byte (`1..=`
/// [`LONG_STRING_MISSING_VALUE_MAX_COUNT`]), a `u32` width shared by
/// every value, then that many raw values each `width` bytes long. The
/// `u32` fields honor `byte_order`. Variable names are decoded through
/// `encoding`; the value bytes are kept verbatim.
///
/// # Errors
///
/// * [`Field::ExtensionElementSize`] when `actual_size != 1`.
/// * [`Field::LongMissingValueCount`] when the count byte is not
///   `1..=3` (matching `ReadStat`).
/// * [`Field::LongMissingValue`] when the payload is truncated.
pub(super) fn parse_long_string_missing_values(
    actual_size: u32,
    payload: &[u8],
    byte_order: ByteOrder,
    encoding: &'static Encoding,
    position: u64,
) -> Result<Vec<LongMissingValueRecord>> {
    if actual_size != LONG_STRING_MISSING_VALUES_ELEMENT_SIZE {
        return Err(unexpected_value_error(position, Field::ExtensionElementSize));
    }
    let mut cursor = ByteCursor::new(payload, Section::Dictionary, position);
    let mut records: Vec<LongMissingValueRecord> = Vec::new();
    while !cursor.is_empty() {
        let record = parse_long_missing_value_record(&mut cursor, byte_order, encoding)?;
        records.push(record);
    }
    Ok(records)
}

/// Parses one per-variable [`LongMissingValueRecord`] from `cursor`,
/// advancing past it: a `u32`-length-prefixed variable name, a single
/// count byte (`1..=`[`LONG_STRING_MISSING_VALUE_MAX_COUNT`]), a `u32`
/// width shared by every value, then that many raw values each `width`
/// bytes long. The value bytes are kept verbatim; the variable name is
/// decoded through `encoding`.
fn parse_long_missing_value_record(
    cursor: &mut ByteCursor<'_>,
    byte_order: ByteOrder,
    encoding: &'static Encoding,
) -> Result<LongMissingValueRecord> {
    let field = Field::LongMissingValue;
    let name_bytes = cursor.take_length_prefixed(byte_order, field)?;
    let count = cursor.take_u8(field)?;
    if count == 0 || count > LONG_STRING_MISSING_VALUE_MAX_COUNT {
        return Err(cursor.unexpected_value(Field::LongMissingValueCount));
    }
    let width = cursor.take_u32_as_usize(byte_order, field)?;
    let mut values: Vec<Vec<u8>> = Vec::new();
    for _ in 0..count {
        let value_bytes = cursor.take_bytes(width, field)?;
        let value = value_bytes.to_vec();
        values.push(value);
    }
    let (variable_name, _, _) = encoding.decode(name_bytes);
    let record = LongMissingValueRecord::builder()
        .variable_name(variable_name.into_owned())
        .values(values)
        .build();
    Ok(record)
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
    fn parse_extra_product_info_keeps_text_verbatim() {
        // Trailing spaces must be preserved (the length is exact).
        let payload = b"Acme Stats 4.3  ";
        let result = parse_extra_product_info(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.text(), "Acme Stats 4.3  ");
    }

    #[test]
    fn parse_extra_product_info_decodes_through_supplied_encoding() {
        // 0xE9 = é in Windows-1252, invalid in standalone UTF-8.
        let payload = b"Caf\xE9 Analytics";
        let result = parse_extra_product_info(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.text(), "Café Analytics");
    }

    #[test]
    fn parse_extra_product_info_empty_payload_yields_empty_text() {
        let result = parse_extra_product_info(1, &[], encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.text(), "");
    }

    #[test]
    fn parse_extra_product_info_rejects_wrong_element_size() {
        let err = parse_extra_product_info(4, b"prod", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::ExtensionElementSize);
    }

    #[test]
    fn parse_uuid_keeps_mixed_case_text_verbatim() {
        let payload = b"F81D4fae-7DEC-11d0-a765-00A0C91E6BF6";
        let result = parse_uuid(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.text(), "F81D4fae-7DEC-11d0-a765-00A0C91E6BF6");
    }

    #[test]
    fn parse_uuid_does_not_validate_or_trim() {
        // Not a valid UUID and has trailing space; both are kept as-is.
        let payload = b"not-a-uuid ";
        let result = parse_uuid(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.text(), "not-a-uuid ");
    }

    #[test]
    fn parse_uuid_empty_payload_yields_empty_text() {
        let result = parse_uuid(1, &[], encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.text(), "");
    }

    #[test]
    fn parse_uuid_rejects_wrong_element_size() {
        let err = parse_uuid(4, b"uuid", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::ExtensionElementSize);
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
    fn parse_data_file_attributes_single_attribute_single_value() {
        let payload = b"attr('value'\n)";
        let result =
            parse_data_file_attributes(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name(), "attr");
        assert_eq!(result[0].values(), &["value".to_string()]);
    }

    #[test]
    fn parse_data_file_attributes_multiple_attributes_in_order() {
        let payload = b"a('1'\n)b('2'\n)";
        let result =
            parse_data_file_attributes(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name(), "a");
        assert_eq!(result[0].values(), &["1".to_string()]);
        assert_eq!(result[1].name(), "b");
        assert_eq!(result[1].values(), &["2".to_string()]);
    }

    #[test]
    fn parse_data_file_attributes_multiple_values_in_one_attribute() {
        let payload = b"a('1'\n'2'\n'3'\n)";
        let result =
            parse_data_file_attributes(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].values(),
            &["1".to_string(), "2".to_string(), "3".to_string()]
        );
    }

    #[test]
    fn parse_data_file_attributes_keeps_index_suffix_verbatim() {
        // PSPP stores multi-valued attributes as fred[1]/fred[2]; the
        // wire layer keeps the suffix verbatim, deferring the array
        // collapse to finalization.
        let payload = b"fred[1]('23'\n)fred[2]('34'\n)";
        let result =
            parse_data_file_attributes(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name(), "fred[1]");
        assert_eq!(result[0].values(), &["23".to_string()]);
        assert_eq!(result[1].name(), "fred[2]");
        assert_eq!(result[1].values(), &["34".to_string()]);
    }

    #[test]
    fn parse_data_file_attributes_strips_only_outer_quotes() {
        // Interior doubled quotes are kept verbatim (values are
        // line-feed-delimited, not quote-delimited), matching PSPP.
        let payload = b"a('it''s'\n)";
        let result =
            parse_data_file_attributes(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result[0].values(), &["it''s".to_string()]);
    }

    #[test]
    fn parse_data_file_attributes_unquoted_value_kept_verbatim() {
        let payload = b"a(bare\n)";
        let result =
            parse_data_file_attributes(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result[0].values(), &["bare".to_string()]);
    }

    #[test]
    fn parse_data_file_attributes_decodes_through_supplied_encoding() {
        // 0xE9 = é in Windows-1252, invalid in standalone UTF-8.
        let payload = b"a('caf\xE9'\n)";
        let result =
            parse_data_file_attributes(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result[0].values(), &["café".to_string()]);
    }

    #[test]
    fn parse_data_file_attributes_empty_payload_yields_empty_vec() {
        let result = parse_data_file_attributes(1, &[], encoding_rs::WINDOWS_1252, 0).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_data_file_attributes_rejects_wrong_element_size() {
        let err =
            parse_data_file_attributes(4, b"a('1'\n)", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::ExtensionElementSize);
    }

    #[test]
    fn parse_data_file_attributes_rejects_missing_open_paren() {
        let err = parse_data_file_attributes(1, b"attr", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::FileAttribute);
    }

    #[test]
    fn parse_data_file_attributes_rejects_empty_name() {
        let err =
            parse_data_file_attributes(1, b"('1'\n)", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::FileAttribute);
    }

    #[test]
    fn parse_data_file_attributes_rejects_unterminated_value() {
        // No line feed before the closing paren / end of payload.
        let err =
            parse_data_file_attributes(1, b"a('1')", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::FileAttribute);
    }

    #[test]
    fn parse_data_file_attributes_rejects_missing_close_paren() {
        let err =
            parse_data_file_attributes(1, b"a('1'\n", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::FileAttribute);
    }

    #[test]
    fn parse_variable_attributes_single_variable_single_attribute() {
        let payload = b"var:a('1'\n)";
        let result = parse_variable_attributes(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].variable_name(), "var");
        assert_eq!(result[0].attributes().len(), 1);
        assert_eq!(result[0].attributes()[0].name(), "a");
        assert_eq!(result[0].attributes()[0].values(), &["1".to_string()]);
    }

    #[test]
    fn parse_variable_attributes_multiple_variables_slash_delimited() {
        let payload = b"v1:a('1'\n)/v2:b('2'\n)";
        let result = parse_variable_attributes(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].variable_name(), "v1");
        assert_eq!(result[0].attributes()[0].name(), "a");
        assert_eq!(result[1].variable_name(), "v2");
        assert_eq!(result[1].attributes()[0].name(), "b");
    }

    #[test]
    fn parse_variable_attributes_multiple_attributes_per_variable() {
        let payload = b"v:a('1'\n)b('2'\n)";
        let result = parse_variable_attributes(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].attributes().len(), 2);
        assert_eq!(result[0].attributes()[0].name(), "a");
        assert_eq!(result[0].attributes()[1].name(), "b");
    }

    #[test]
    fn parse_variable_attributes_trailing_slash_accepted() {
        let payload = b"v:a('1'\n)/";
        let result = parse_variable_attributes(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].variable_name(), "v");
    }

    #[test]
    fn parse_variable_attributes_slash_inside_value_is_not_a_separator() {
        // A `/` before a value's line feed is content, not the set
        // delimiter, so it stays in the single record's value.
        let payload = b"v:a('a/b'\n)";
        let result = parse_variable_attributes(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].attributes()[0].values(), &["a/b".to_string()]);
    }

    #[test]
    fn parse_variable_attributes_empty_payload_yields_empty_vec() {
        let result = parse_variable_attributes(1, &[], encoding_rs::WINDOWS_1252, 0).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_variable_attributes_rejects_wrong_element_size() {
        let err =
            parse_variable_attributes(4, b"v:a('1'\n)", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::ExtensionElementSize);
    }

    #[test]
    fn parse_variable_attributes_rejects_missing_colon() {
        let err =
            parse_variable_attributes(1, b"a('1'\n)", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::VariableAttribute);
    }

    #[test]
    fn parse_variable_attributes_rejects_empty_variable_name() {
        let err =
            parse_variable_attributes(1, b":a('1'\n)", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::VariableAttribute);
    }

    #[test]
    fn parse_variable_attributes_rejects_malformed_attribute() {
        let err =
            parse_variable_attributes(1, b"v:a('1')", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::VariableAttribute);
    }

    fn assert_unexpected_value_error(err: &SavError, expected: Field) {
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue { field: expected }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    /// Appends a `u32`-length-prefixed byte string in `byte_order`.
    fn push_prefixed(buf: &mut Vec<u8>, bytes: &[u8], byte_order: ByteOrder) {
        let len = u32::try_from(bytes.len()).unwrap();
        push_u32(buf, len, byte_order);
        buf.extend_from_slice(bytes);
    }

    /// Appends a `u32` in `byte_order`.
    fn push_u32(buf: &mut Vec<u8>, value: u32, byte_order: ByteOrder) {
        match byte_order {
            ByteOrder::LittleEndian => buf.extend_from_slice(&value.to_le_bytes()),
            ByteOrder::BigEndian => buf.extend_from_slice(&value.to_be_bytes()),
        }
    }

    #[test]
    fn parse_long_string_value_labels_single_variable_single_label() {
        let byte_order = ByteOrder::LittleEndian;
        let mut payload = Vec::new();
        push_prefixed(&mut payload, b"longvar", byte_order);
        push_u32(&mut payload, 20, byte_order); // width
        push_u32(&mut payload, 1, byte_order); // label count
        push_prefixed(&mut payload, b"code01", byte_order); // value
        push_prefixed(&mut payload, b"First", byte_order); // label

        let result =
            parse_long_string_value_labels(1, &payload, byte_order, encoding_rs::WINDOWS_1252, 0)
                .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].variable_name(), "longvar");
        assert_eq!(result[0].width(), 20);
        assert_eq!(result[0].labels().len(), 1);
        assert_eq!(result[0].labels()[0].value(), b"code01");
        assert_eq!(result[0].labels()[0].label(), "First");
    }

    #[test]
    fn parse_long_string_value_labels_multiple_labels_and_variables() {
        let byte_order = ByteOrder::LittleEndian;
        let mut payload = Vec::new();
        push_prefixed(&mut payload, b"v1", byte_order);
        push_u32(&mut payload, 10, byte_order);
        push_u32(&mut payload, 2, byte_order);
        push_prefixed(&mut payload, b"a", byte_order);
        push_prefixed(&mut payload, b"Apple", byte_order);
        push_prefixed(&mut payload, b"b", byte_order);
        push_prefixed(&mut payload, b"Banana", byte_order);
        push_prefixed(&mut payload, b"v2", byte_order);
        push_u32(&mut payload, 12, byte_order);
        push_u32(&mut payload, 1, byte_order);
        push_prefixed(&mut payload, b"z", byte_order);
        push_prefixed(&mut payload, b"Zed", byte_order);

        let result =
            parse_long_string_value_labels(1, &payload, byte_order, encoding_rs::WINDOWS_1252, 0)
                .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].labels().len(), 2);
        assert_eq!(result[0].labels()[1].label(), "Banana");
        assert_eq!(result[1].variable_name(), "v2");
        assert_eq!(result[1].labels()[0].value(), b"z");
    }

    #[test]
    fn parse_long_string_value_labels_big_endian() {
        let byte_order = ByteOrder::BigEndian;
        let mut payload = Vec::new();
        push_prefixed(&mut payload, b"v", byte_order);
        push_u32(&mut payload, 9, byte_order);
        push_u32(&mut payload, 1, byte_order);
        push_prefixed(&mut payload, b"x", byte_order);
        push_prefixed(&mut payload, b"Label", byte_order);

        let result =
            parse_long_string_value_labels(1, &payload, byte_order, encoding_rs::WINDOWS_1252, 0)
                .unwrap();
        assert_eq!(result[0].width(), 9);
        assert_eq!(result[0].labels()[0].label(), "Label");
    }

    #[test]
    fn parse_long_string_value_labels_keeps_value_bytes_verbatim() {
        let byte_order = ByteOrder::LittleEndian;
        let mut payload = Vec::new();
        push_prefixed(&mut payload, b"v", byte_order);
        push_u32(&mut payload, 8, byte_order);
        push_u32(&mut payload, 1, byte_order);
        push_prefixed(&mut payload, b"ab   ", byte_order); // trailing spaces preserved
        push_prefixed(&mut payload, b"L", byte_order);

        let result =
            parse_long_string_value_labels(1, &payload, byte_order, encoding_rs::WINDOWS_1252, 0)
                .unwrap();
        assert_eq!(result[0].labels()[0].value(), b"ab   ");
    }

    #[test]
    fn parse_long_string_value_labels_empty_payload_yields_empty_vec() {
        let result = parse_long_string_value_labels(
            1,
            &[],
            ByteOrder::LittleEndian,
            encoding_rs::WINDOWS_1252,
            0,
        )
        .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_long_string_value_labels_rejects_wrong_element_size() {
        let err = parse_long_string_value_labels(
            4,
            &[0; 4],
            ByteOrder::LittleEndian,
            encoding_rs::WINDOWS_1252,
            0,
        )
        .unwrap_err();
        assert_unexpected_value_error(&err, Field::ExtensionElementSize);
    }

    #[test]
    fn parse_long_string_value_labels_rejects_truncated_payload() {
        let byte_order = ByteOrder::LittleEndian;
        let mut payload = Vec::new();
        push_prefixed(&mut payload, b"v", byte_order);
        push_u32(&mut payload, 8, byte_order);
        push_u32(&mut payload, 1, byte_order);
        push_u32(&mut payload, 5, byte_order); // value length 5, but no bytes follow
        let err =
            parse_long_string_value_labels(1, &payload, byte_order, encoding_rs::WINDOWS_1252, 0)
                .unwrap_err();
        assert_unexpected_value_error(&err, Field::LongValueLabel);
    }

    #[test]
    fn parse_long_string_missing_values_single_variable() {
        let byte_order = ByteOrder::LittleEndian;
        let mut payload = Vec::new();
        push_prefixed(&mut payload, b"longvar", byte_order);
        payload.push(2); // n_missing
        push_u32(&mut payload, 3, byte_order); // width
        payload.extend_from_slice(b"XXX");
        payload.extend_from_slice(b"YYY");

        let result =
            parse_long_string_missing_values(1, &payload, byte_order, encoding_rs::WINDOWS_1252, 0)
                .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].variable_name(), "longvar");
        assert_eq!(result[0].values(), &[b"XXX".to_vec(), b"YYY".to_vec()]);
    }

    #[test]
    fn parse_long_string_missing_values_big_endian_multiple_variables() {
        let byte_order = ByteOrder::BigEndian;
        let mut payload = Vec::new();
        push_prefixed(&mut payload, b"v1", byte_order);
        payload.push(1);
        push_u32(&mut payload, 2, byte_order);
        payload.extend_from_slice(b"ab");
        push_prefixed(&mut payload, b"v2", byte_order);
        payload.push(3);
        push_u32(&mut payload, 1, byte_order);
        payload.extend_from_slice(b"xyz");

        let result =
            parse_long_string_missing_values(1, &payload, byte_order, encoding_rs::WINDOWS_1252, 0)
                .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].values(), &[b"ab".to_vec()]);
        assert_eq!(
            result[1].values(),
            &[b"x".to_vec(), b"y".to_vec(), b"z".to_vec()]
        );
    }

    #[test]
    fn parse_long_string_missing_values_rejects_zero_count() {
        let byte_order = ByteOrder::LittleEndian;
        let mut payload = Vec::new();
        push_prefixed(&mut payload, b"v", byte_order);
        payload.push(0);
        push_u32(&mut payload, 1, byte_order);
        let err =
            parse_long_string_missing_values(1, &payload, byte_order, encoding_rs::WINDOWS_1252, 0)
                .unwrap_err();
        assert_unexpected_value_error(&err, Field::LongMissingValueCount);
    }

    #[test]
    fn parse_long_string_missing_values_rejects_count_above_three() {
        let byte_order = ByteOrder::LittleEndian;
        let mut payload = Vec::new();
        push_prefixed(&mut payload, b"v", byte_order);
        payload.push(4);
        push_u32(&mut payload, 1, byte_order);
        payload.extend_from_slice(b"abcd");
        let err =
            parse_long_string_missing_values(1, &payload, byte_order, encoding_rs::WINDOWS_1252, 0)
                .unwrap_err();
        assert_unexpected_value_error(&err, Field::LongMissingValueCount);
    }

    #[test]
    fn parse_long_string_missing_values_rejects_wrong_element_size() {
        let err = parse_long_string_missing_values(
            4,
            &[0; 4],
            ByteOrder::LittleEndian,
            encoding_rs::WINDOWS_1252,
            0,
        )
        .unwrap_err();
        assert_unexpected_value_error(&err, Field::ExtensionElementSize);
    }

    #[test]
    fn parse_long_string_missing_values_rejects_truncated_values() {
        let byte_order = ByteOrder::LittleEndian;
        let mut payload = Vec::new();
        push_prefixed(&mut payload, b"v", byte_order);
        payload.push(2);
        push_u32(&mut payload, 3, byte_order); // width 3, but only 3 bytes for one value
        payload.extend_from_slice(b"XXX");
        let err =
            parse_long_string_missing_values(1, &payload, byte_order, encoding_rs::WINDOWS_1252, 0)
                .unwrap_err();
        assert_unexpected_value_error(&err, Field::LongMissingValue);
    }

    #[test]
    fn parse_long_string_missing_values_empty_payload_yields_empty_vec() {
        let result = parse_long_string_missing_values(
            1,
            &[],
            ByteOrder::LittleEndian,
            encoding_rs::WINDOWS_1252,
            0,
        )
        .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_variable_sets_single_set_with_members() {
        let payload = b"demographics= age sex region\n";
        let result = parse_variable_sets(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.sets().len(), 1);
        assert_eq!(result.sets()[0].name(), "demographics");
        assert_eq!(
            result.sets()[0].variables(),
            &["age".to_string(), "sex".to_string(), "region".to_string()]
        );
    }

    #[test]
    fn parse_variable_sets_multiple_sets() {
        let payload = b"grp1= a b\ngrp2= c\n";
        let result = parse_variable_sets(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.sets().len(), 2);
        assert_eq!(result.sets()[0].name(), "grp1");
        assert_eq!(result.sets()[1].name(), "grp2");
        assert_eq!(result.sets()[1].variables(), &["c".to_string()]);
    }

    #[test]
    fn parse_variable_sets_empty_set_has_no_members() {
        let payload = b"empty= \n";
        let result = parse_variable_sets(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.sets().len(), 1);
        assert_eq!(result.sets()[0].name(), "empty");
        assert!(result.sets()[0].variables().is_empty());
    }

    #[test]
    fn parse_variable_sets_strips_carriage_return() {
        let payload = b"grp= a b\r\n";
        let result = parse_variable_sets(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.sets()[0].variables(), &["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn parse_variable_sets_ignores_repeated_and_trailing_spaces() {
        let payload = b"grp=  a   b \n";
        let result = parse_variable_sets(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.sets()[0].variables(), &["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn parse_variable_sets_decodes_through_supplied_encoding() {
        // 0xE9 = é in Windows-1252, invalid in standalone UTF-8.
        let payload = b"caf\xE9= r\xE9gion\n";
        let result = parse_variable_sets(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.sets()[0].name(), "café");
        assert_eq!(result.sets()[0].variables(), &["région".to_string()]);
    }

    #[test]
    fn parse_variable_sets_empty_payload_yields_no_sets() {
        let result = parse_variable_sets(1, &[], encoding_rs::WINDOWS_1252, 0).unwrap();
        assert!(result.sets().is_empty());
    }

    #[test]
    fn parse_variable_sets_accepts_final_line_without_line_feed() {
        let payload = b"grp= a b";
        let result = parse_variable_sets(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.sets().len(), 1);
        assert_eq!(result.sets()[0].variables(), &["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn parse_variable_sets_rejects_wrong_element_size() {
        let err = parse_variable_sets(4, b"grp= a\n", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::ExtensionElementSize);
    }

    #[test]
    fn parse_variable_sets_rejects_line_without_equals() {
        let err = parse_variable_sets(1, b"noequals\n", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::VariableSet);
    }

    #[test]
    fn parse_variable_sets_rejects_empty_set_name() {
        let err = parse_variable_sets(1, b"= a b\n", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::VariableSet);
    }

    #[test]
    fn parse_multiple_response_sets_dichotomy_variable_labels() {
        let payload = b"$dich=D1 1 13 Dichotomy set q1 q2 q3\n";
        let result = parse_multiple_response_sets(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        let set = &result[0];
        assert_eq!(set.name(), "$dich"); // leading '$' preserved
        assert_eq!(set.label(), "Dichotomy set");
        assert_eq!(
            set.variables(),
            ["q1".to_string(), "q2".to_string(), "q3".to_string()]
        );
        let MultipleResponseSetKind::MultipleDichotomy {
            counted_value,
            category_labels,
        } = set.kind()
        else {
            panic!("expected dichotomy, got {:?}", set.kind());
        };
        assert_eq!(counted_value.as_str(), "1");
        assert_eq!(*category_labels, CategoryLabelSource::VariableLabels);
    }

    #[test]
    fn parse_multiple_response_sets_category() {
        let payload = b"$cat=C 12 Category set q1 q2\n";
        let result = parse_multiple_response_sets(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        let set = &result[0];
        assert_eq!(set.name(), "$cat");
        assert_eq!(set.label(), "Category set");
        assert_eq!(set.variables(), ["q1".to_string(), "q2".to_string()]);
        assert_eq!(*set.kind(), MultipleResponseSetKind::MultipleCategory);
    }

    #[test]
    fn parse_multiple_response_sets_dichotomy_counted_values() {
        let payload = b"$counted=E 1 1 1 0  q2 q3\n";
        let result = parse_multiple_response_sets(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        let set = &result[0];
        assert_eq!(set.name(), "$counted");
        assert_eq!(set.label(), ""); // empty label (counted string length 0)
        assert_eq!(set.variables(), ["q2".to_string(), "q3".to_string()]);
        let MultipleResponseSetKind::MultipleDichotomy {
            counted_value,
            category_labels,
        } = set.kind()
        else {
            panic!("expected dichotomy, got {:?}", set.kind());
        };
        assert_eq!(counted_value.as_str(), "1");
        assert_eq!(
            *category_labels,
            CategoryLabelSource::CountedValues { label_source: 1 }
        );
    }

    #[test]
    fn parse_multiple_response_sets_label_source_eleven() {
        // LABELSOURCE=VARLABEL is encoded as 11.
        let payload = b"$e=E 11 1 1 3 lbl a b\n";
        let result = parse_multiple_response_sets(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        let MultipleResponseSetKind::MultipleDichotomy {
            category_labels: CategoryLabelSource::CountedValues { label_source },
            ..
        } = result[0].kind()
        else {
            panic!("expected counted-values dichotomy, got {:?}", result[0].kind());
        };
        assert_eq!(*label_source, 11);
        assert_eq!(result[0].label(), "lbl");
        assert_eq!(result[0].variables(), ["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn parse_multiple_response_sets_multiple_sets_and_string_counted_value() {
        // Two sets on two lines; the first has a multi-byte string
        // counted value ("yes", length 3) whose internal space-free
        // bytes are read by length.
        let payload = b"$a=D3 yes 5 Label q1\n$b=C 3 Two q2 q3\n";
        let result = parse_multiple_response_sets(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 2);
        let MultipleResponseSetKind::MultipleDichotomy { counted_value, .. } = result[0].kind()
        else {
            panic!("expected dichotomy, got {:?}", result[0].kind());
        };
        assert_eq!(counted_value.as_str(), "yes");
        assert_eq!(result[0].variables(), ["q1".to_string()]);
        assert_eq!(*result[1].kind(), MultipleResponseSetKind::MultipleCategory);
        assert_eq!(result[1].label(), "Two");
    }

    #[test]
    fn parse_multiple_response_sets_decodes_through_supplied_encoding() {
        // 0xE9 = é in Windows-1252; the label byte length counts bytes.
        let payload = b"$s=C 5 caf\xE9x q1\n";
        let result = parse_multiple_response_sets(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result[0].label(), "caféx");
    }

    #[test]
    fn parse_multiple_response_sets_empty_payload_yields_no_sets() {
        let result = parse_multiple_response_sets(1, &[], encoding_rs::WINDOWS_1252, 0).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_multiple_response_sets_rejects_wrong_element_size() {
        let err =
            parse_multiple_response_sets(4, b"$a=C 1 x q1\n", encoding_rs::WINDOWS_1252, 0)
                .unwrap_err();
        assert_unexpected_value_error(&err, Field::ExtensionElementSize);
    }

    #[test]
    fn parse_multiple_response_sets_rejects_missing_equals() {
        let err =
            parse_multiple_response_sets(1, b"$noeq\n", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::MultipleResponseSet);
    }

    #[test]
    fn parse_multiple_response_sets_rejects_unknown_type_letter() {
        let err =
            parse_multiple_response_sets(1, b"$a=X 1 x q1\n", encoding_rs::WINDOWS_1252, 0)
                .unwrap_err();
        assert_unexpected_value_error(&err, Field::MultipleResponseSet);
    }

    #[test]
    fn parse_multiple_response_sets_rejects_counted_string_running_past_end() {
        // Label length 9 but only "Label" (5 bytes) remain.
        let err =
            parse_multiple_response_sets(1, b"$a=C 9 Label\n", encoding_rs::WINDOWS_1252, 0)
                .unwrap_err();
        assert_unexpected_value_error(&err, Field::MultipleResponseSet);
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
