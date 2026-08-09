//! Wire-level discriminant of a type-7 extension record.

use crate::spss::sav::dictionary_format::{
    EXTENSION_SUBTYPE_CHARACTER_ENCODING, EXTENSION_SUBTYPE_DATA_FILE_ATTRIBUTES,
    EXTENSION_SUBTYPE_DISPLAY_PARAMETERS, EXTENSION_SUBTYPE_EXTENDED_NUMBER_OF_CASES,
    EXTENSION_SUBTYPE_EXTRA_PRODUCT_INFO, EXTENSION_SUBTYPE_FLOAT_INFO,
    EXTENSION_SUBTYPE_LONG_STRING_MISSING_VALUES, EXTENSION_SUBTYPE_LONG_STRING_VALUE_LABELS,
    EXTENSION_SUBTYPE_LONG_VARIABLE_NAMES, EXTENSION_SUBTYPE_MACHINE_INTEGER_INFO,
    EXTENSION_SUBTYPE_MULTIPLE_RESPONSE_SETS, EXTENSION_SUBTYPE_MULTIPLE_RESPONSE_SETS_EXTENDED,
    EXTENSION_SUBTYPE_UUID, EXTENSION_SUBTYPE_VARIABLE_ATTRIBUTES, EXTENSION_SUBTYPE_VARIABLE_SETS,
    EXTENSION_SUBTYPE_VERY_LONG_STRINGS,
};

/// Which kind of type-7 extension record a subtype code names.
///
/// The `subtype` field of a type-7 record is an `i32` on disk.
/// `ExtensionSubtype` is the typed form of that field, and names
/// exactly the subtypes this library parses — one variant per
/// [`ExtensionRecord`](crate::spss::sav::extensions::extension_record::ExtensionRecord)
/// variant, except that subtypes 7 and 19 are separate here (they
/// share a single record variant) and every code outside the set
/// collapses to [`Unrecognized`](Self::Unrecognized).
///
/// That correspondence is exact and load-bearing: a record whose
/// subtype is `Unrecognized` always decodes to
/// [`ExtensionRecord::Unknown`](crate::spss::sav::extensions::extension_record::ExtensionRecord::Unknown),
/// and a record whose subtype is anything else never does. Nothing is
/// lost by folding unrecognized codes together — the code itself
/// survives on
/// [`UnknownExtension::subtype`](crate::spss::sav::extensions::unknown_extension::UnknownExtension::subtype),
/// and grouping them means a caller can skip every unrecognized
/// extension with a single entry.
///
/// Subtypes PSPP records as observed in the wild but does not parse —
/// 6 (date info, probably related to `USE`) and 24 (XML describing
/// on-screen display) — are deliberately absent, and read as
/// `Unrecognized` like any other unhandled code. Naming them here would
/// break the correspondence above without gaining anything: their codes
/// survive on `UnknownExtension` either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ExtensionSubtype {
    /// Subtype 3 — integer-typed environment metadata.
    MachineIntegerInfo,
    /// Subtype 4 — float sentinel values.
    FloatInfo,
    /// Subtype 5 — named variable groupings.
    VariableSets,
    /// Subtype 7 — multiple response sets, pre-v14 format.
    MultipleResponseSets,
    /// Subtype 10 — extra product information.
    ExtraProductInfo,
    /// Subtype 11 — per-variable display parameters.
    DisplayParameters,
    /// Subtype 12 — file UUID.
    Uuid,
    /// Subtype 13 — long-variable-name mappings.
    LongVariableNames,
    /// Subtype 14 — very-long-string widths.
    VeryLongStrings,
    /// Subtype 16 — extended number of cases.
    ExtendedNumberOfCases,
    /// Subtype 17 — file-level custom attributes.
    FileAttributes,
    /// Subtype 18 — per-variable custom attributes.
    VariableAttributes,
    /// Subtype 19 — multiple response sets, post-v14 format.
    MultipleResponseSetsExtended,
    /// Subtype 20 — declared character encoding label.
    CharacterEncoding,
    /// Subtype 21 — long value labels.
    LongValueLabels,
    /// Subtype 22 — long missing values.
    LongMissingValues,
    /// Any subtype code this library does not parse.
    Unrecognized,
}

impl ExtensionSubtype {
    /// The on-disk subtype code, or `None` for
    /// [`Unrecognized`](Self::Unrecognized), which stands for every
    /// code at once rather than any single one.
    #[must_use]
    pub fn code(self) -> Option<i32> {
        let code = match self {
            Self::MachineIntegerInfo => EXTENSION_SUBTYPE_MACHINE_INTEGER_INFO,
            Self::FloatInfo => EXTENSION_SUBTYPE_FLOAT_INFO,
            Self::VariableSets => EXTENSION_SUBTYPE_VARIABLE_SETS,
            Self::MultipleResponseSets => EXTENSION_SUBTYPE_MULTIPLE_RESPONSE_SETS,
            Self::ExtraProductInfo => EXTENSION_SUBTYPE_EXTRA_PRODUCT_INFO,
            Self::DisplayParameters => EXTENSION_SUBTYPE_DISPLAY_PARAMETERS,
            Self::Uuid => EXTENSION_SUBTYPE_UUID,
            Self::LongVariableNames => EXTENSION_SUBTYPE_LONG_VARIABLE_NAMES,
            Self::VeryLongStrings => EXTENSION_SUBTYPE_VERY_LONG_STRINGS,
            Self::ExtendedNumberOfCases => EXTENSION_SUBTYPE_EXTENDED_NUMBER_OF_CASES,
            Self::FileAttributes => EXTENSION_SUBTYPE_DATA_FILE_ATTRIBUTES,
            Self::VariableAttributes => EXTENSION_SUBTYPE_VARIABLE_ATTRIBUTES,
            Self::MultipleResponseSetsExtended => EXTENSION_SUBTYPE_MULTIPLE_RESPONSE_SETS_EXTENDED,
            Self::CharacterEncoding => EXTENSION_SUBTYPE_CHARACTER_ENCODING,
            Self::LongValueLabels => EXTENSION_SUBTYPE_LONG_STRING_VALUE_LABELS,
            Self::LongMissingValues => EXTENSION_SUBTYPE_LONG_STRING_MISSING_VALUES,
            Self::Unrecognized => return None,
        };
        Some(code)
    }

    /// Classifies an on-disk subtype code. Any code the reader has no
    /// parser for yields [`Unrecognized`](Self::Unrecognized).
    #[must_use]
    pub fn from_code(code: i32) -> Self {
        match code {
            EXTENSION_SUBTYPE_MACHINE_INTEGER_INFO => Self::MachineIntegerInfo,
            EXTENSION_SUBTYPE_FLOAT_INFO => Self::FloatInfo,
            EXTENSION_SUBTYPE_VARIABLE_SETS => Self::VariableSets,
            EXTENSION_SUBTYPE_MULTIPLE_RESPONSE_SETS => Self::MultipleResponseSets,
            EXTENSION_SUBTYPE_EXTRA_PRODUCT_INFO => Self::ExtraProductInfo,
            EXTENSION_SUBTYPE_DISPLAY_PARAMETERS => Self::DisplayParameters,
            EXTENSION_SUBTYPE_UUID => Self::Uuid,
            EXTENSION_SUBTYPE_LONG_VARIABLE_NAMES => Self::LongVariableNames,
            EXTENSION_SUBTYPE_VERY_LONG_STRINGS => Self::VeryLongStrings,
            EXTENSION_SUBTYPE_EXTENDED_NUMBER_OF_CASES => Self::ExtendedNumberOfCases,
            EXTENSION_SUBTYPE_DATA_FILE_ATTRIBUTES => Self::FileAttributes,
            EXTENSION_SUBTYPE_VARIABLE_ATTRIBUTES => Self::VariableAttributes,
            EXTENSION_SUBTYPE_MULTIPLE_RESPONSE_SETS_EXTENDED => Self::MultipleResponseSetsExtended,
            EXTENSION_SUBTYPE_CHARACTER_ENCODING => Self::CharacterEncoding,
            EXTENSION_SUBTYPE_LONG_STRING_VALUE_LABELS => Self::LongValueLabels,
            EXTENSION_SUBTYPE_LONG_STRING_MISSING_VALUES => Self::LongMissingValues,
            _ => Self::Unrecognized,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every named subtype, for exhaustiveness-style sweeps. A new
    /// variant that is not added here fails
    /// [`every_named_subtype_is_listed`].
    const NAMED: &[ExtensionSubtype] = &[
        ExtensionSubtype::MachineIntegerInfo,
        ExtensionSubtype::FloatInfo,
        ExtensionSubtype::VariableSets,
        ExtensionSubtype::MultipleResponseSets,
        ExtensionSubtype::ExtraProductInfo,
        ExtensionSubtype::DisplayParameters,
        ExtensionSubtype::Uuid,
        ExtensionSubtype::LongVariableNames,
        ExtensionSubtype::VeryLongStrings,
        ExtensionSubtype::ExtendedNumberOfCases,
        ExtensionSubtype::FileAttributes,
        ExtensionSubtype::VariableAttributes,
        ExtensionSubtype::MultipleResponseSetsExtended,
        ExtensionSubtype::CharacterEncoding,
        ExtensionSubtype::LongValueLabels,
        ExtensionSubtype::LongMissingValues,
    ];

    #[test]
    fn named_subtypes_round_trip_through_their_code() {
        for &subtype in NAMED {
            let code = subtype.code().expect("named subtype has a code");
            assert_eq!(ExtensionSubtype::from_code(code), subtype);
        }
    }

    #[test]
    fn named_subtype_codes_are_distinct() {
        let mut codes: Vec<i32> = NAMED.iter().filter_map(|s| s.code()).collect();
        let total = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), total, "two variants claim the same code");
    }

    /// Guards `NAMED` against drifting out of date. Every variant
    /// except `Unrecognized` must carry a code, and the sweep must
    /// cover all of them.
    #[test]
    fn every_named_subtype_is_listed() {
        // Codes 1..=40 covers the whole assigned range with room to
        // spare; anything classified as named must appear in NAMED.
        for code in 1..=40 {
            let subtype = ExtensionSubtype::from_code(code);
            if subtype == ExtensionSubtype::Unrecognized {
                continue;
            }
            assert!(NAMED.contains(&subtype), "{subtype:?} missing from NAMED");
        }
    }

    #[test]
    fn unrecognized_has_no_code() {
        assert_eq!(ExtensionSubtype::Unrecognized.code(), None);
    }

    #[test]
    fn unparsed_subtypes_are_unrecognized() {
        // 6 and 24 are subtypes PSPP has observed but does not parse;
        // 8, 15 and 23 have no evidence behind them at all. All read
        // the same way, which is the point — the reader does not have
        // to know which is which.
        for code in [6, 8, 15, 23, 24, 0, -1, i32::MAX] {
            assert_eq!(
                ExtensionSubtype::from_code(code),
                ExtensionSubtype::Unrecognized,
                "code {code} should be unrecognized",
            );
        }
    }
}
