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

use crate::spss::sav::dictionary_format::{
    FORMAT_CODE_DECIMALS_BYTE, FORMAT_CODE_KIND_BYTE, FORMAT_CODE_WIDTH_BYTE,
    VARIABLE_TYPE_CONTINUATION, VARIABLE_TYPE_NUMERIC, VARIABLE_TYPE_STRING_MAX,
};
use crate::spss::sav::raw_missing_values::RawMissingValues;
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
