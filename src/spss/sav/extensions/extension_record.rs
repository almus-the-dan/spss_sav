//! Top-level extension record enum.

use crate::spss::sav::extensions::data_entry::DataEntry;
use crate::spss::sav::extensions::extended_number_of_cases::ExtendedNumberOfCases;
use crate::spss::sav::extensions::file_attribute::FileAttribute;
use crate::spss::sav::extensions::float_sentinels::FloatSentinels;
use crate::spss::sav::extensions::long_missing_value_record::LongMissingValueRecord;
use crate::spss::sav::extensions::long_value_label_record::LongValueLabelRecord;
use crate::spss::sav::extensions::long_variable_name::LongVariableName;
use crate::spss::sav::extensions::machine_integer_info::MachineIntegerInfo;
use crate::spss::sav::extensions::multiple_response_set::MultipleResponseSet;
use crate::spss::sav::extensions::raw_display_parameters::RawDisplayParameters;
use crate::spss::sav::extensions::unknown_extension::UnknownExtension;
use crate::spss::sav::extensions::variable_attribute_record::VariableAttributeRecord;
use crate::spss::sav::extensions::variable_sets::VariableSets;
use crate::spss::sav::extensions::very_long_string::VeryLongString;

/// One extension record read from a SAV file.
///
/// Subtype-to-variant assignments here mirror PSPP's documented
/// system file format and `ReadStat`'s implementation; the spec md
/// in this repository's reference directory has known errors and is
/// not authoritative. Subtypes that PSPP/`ReadStat` document but
/// this enum doesn't yet carry (e.g., `ProductInfo` = 10, `Uuid` =
/// 12, XML display info = 24) fall through to [`Unknown`](Self::Unknown)
/// for now.
///
/// The reader preserves unrecognized subtypes verbatim via
/// [`Unknown`](Self::Unknown) for round-trip fidelity and surfaces
/// a
/// [`SavWarning::UnknownExtensionSubtype`](crate::spss::sav::sav_warning::SavWarning::UnknownExtensionSubtype)
/// when one is encountered.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ExtensionRecord {
    /// Subtype 3 — integer-typed environment metadata (version
    /// numbers, machine code, endianness, floating-point
    /// representation, character-set code).
    MachineIntegerInfo(MachineIntegerInfo),
    /// Subtype 4 — float sentinel values (system missing, highest,
    /// lowest).
    FloatInfo(FloatSentinels),
    /// Subtype 5 — named variable groupings. Subtype assignment
    /// confirmed against PSPP; parser not yet wired up.
    VariableSets(VariableSets),
    /// Subtype 7 — multiple response sets, pre-v14 format.
    /// Subtype 19 carries the post-v14 form (which adds
    /// `CATEGORYLABELS` support). Parser not yet wired up.
    MultipleResponseSets(Vec<MultipleResponseSet>),
    /// Subtype 11 — per-variable display parameters (measurement
    /// level, display width, alignment) carried verbatim from the
    /// record's payload. Per-variable slicing into typed
    /// [`VariableDisplay`](crate::spss::sav::extensions::variable_display::VariableDisplay)
    /// values happens during schema finalization.
    DisplayParameters(RawDisplayParameters),
    /// Subtype 13 — long-variable-name mappings (short → long name).
    LongVariableNames(Vec<LongVariableName>),
    /// Subtype 14 — very-long-string widths for variables wider
    /// than 255 bytes.
    VeryLongStrings(Vec<VeryLongString>),
    /// Subtype 15 — SPSS Data Entry product information. Subtype
    /// assignment is from the (unreliable) spec md and is not
    /// confirmed against PSPP or `ReadStat`; treat with caution.
    DataEntry(DataEntry),
    /// Subtype 16 — extended number of cases, authoritative when
    /// the header's 32-bit `case_count` field is `-1`.
    ExtendedNumberOfCases(ExtendedNumberOfCases),
    /// Subtype 17 — file-level custom attributes (key-value pairs
    /// attached to the file, not to any particular variable).
    FileAttributes(Vec<FileAttribute>),
    /// Subtype 18 — per-variable custom attributes.
    VariableAttributes(Vec<VariableAttributeRecord>),
    /// Subtype 20 — declared character encoding label.
    CharacterEncoding(String),
    /// Subtype 21 — long value labels (for very-long-string
    /// variables).
    LongValueLabels(Vec<LongValueLabelRecord>),
    /// Subtype 22 — long missing values (for very-long-string
    /// variables).
    LongMissingValues(Vec<LongMissingValueRecord>),
    /// An extension subtype this library does not yet recognize. The
    /// raw bytes are preserved verbatim for round-trip fidelity.
    Unknown(UnknownExtension),
}
