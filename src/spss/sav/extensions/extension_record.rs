//! Top-level extension record enum.

use crate::spss::sav::extensions::data_entry::DataEntry;
use crate::spss::sav::extensions::file_attribute::FileAttribute;
use crate::spss::sav::extensions::float_sentinels::FloatSentinels;
use crate::spss::sav::extensions::long_missing_value_record::LongMissingValueRecord;
use crate::spss::sav::extensions::long_value_label_record::LongValueLabelRecord;
use crate::spss::sav::extensions::long_variable_name::LongVariableName;
use crate::spss::sav::extensions::machine_float_info::MachineFloatInfo;
use crate::spss::sav::extensions::machine_integer_info::MachineIntegerInfo;
use crate::spss::sav::extensions::multiple_response_set::MultipleResponseSet;
use crate::spss::sav::extensions::unknown_extension::UnknownExtension;
use crate::spss::sav::extensions::variable_attribute_record::VariableAttributeRecord;
use crate::spss::sav::extensions::variable_display::VariableDisplay;
use crate::spss::sav::extensions::variable_sets::VariableSets;
use crate::spss::sav::extensions::very_long_string::VeryLongString;

/// One extension record read from a SAV file.
///
/// Extension records carry optional per-file metadata identified by
/// numeric subtype. The reader preserves unrecognized subtypes
/// verbatim via [`Unknown`](Self::Unknown) for round-trip fidelity
/// and surfaces a
/// [`SavWarning::UnknownExtensionSubtype`](crate::spss::sav::sav_warning::SavWarning::UnknownExtensionSubtype)
/// when one is encountered.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ExtensionRecord {
    /// Subtype 3 — total number of cases as an `i64`. Authoritative
    /// when the header's case count is `-1`.
    NumberOfCases(i64),
    /// Subtype 4 — float sentinel values (system missing, highest,
    /// lowest).
    FloatInfo(FloatSentinels),
    /// Subtype 5 — integer-typed environment metadata (version
    /// numbers, machine code, endianness, character encoding code).
    MachineIntegerInfo(MachineIntegerInfo),
    /// Subtype 6 — float-format confirmation (redundant cross-check
    /// against subtype 4).
    MachineFloatInfo(MachineFloatInfo),
    /// Subtype 7 — named variable groupings.
    VariableSets(VariableSets),
    /// Subtype 11 — per-variable display parameters (measurement
    /// level, display width, alignment) in declaration order.
    DisplayParameters(Vec<VariableDisplay>),
    /// Subtype 13 — long-variable-name mappings (short → long name).
    LongVariableNames(Vec<LongVariableName>),
    /// Subtype 14 — very-long-string widths for variables wider than
    /// 255 bytes.
    VeryLongStrings(Vec<VeryLongString>),
    /// Subtype 15 — SPSS Data Entry product information.
    DataEntry(DataEntry),
    /// Subtype 17 — per-variable custom attributes.
    VariableAttributes(Vec<VariableAttributeRecord>),
    /// Subtype 18 — file-level custom attributes.
    FileAttributes(Vec<FileAttribute>),
    /// Subtypes 19 / 7B — multiple response sets (MRSETS).
    MultipleResponseSets(Vec<MultipleResponseSet>),
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
