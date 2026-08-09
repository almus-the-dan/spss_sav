//! Top-level extension record enum.

use crate::spss::sav::extensions::character_encoding::CharacterEncoding;
use crate::spss::sav::extensions::extended_number_of_cases::ExtendedNumberOfCases;
use crate::spss::sav::extensions::extension_subtype::ExtensionSubtype;
use crate::spss::sav::extensions::extra_product_info::ExtraProductInfo;
use crate::spss::sav::extensions::file_attributes::FileAttributes;
use crate::spss::sav::extensions::float_sentinels::FloatSentinels;
use crate::spss::sav::extensions::long_missing_values::LongMissingValues;
use crate::spss::sav::extensions::long_value_labels::LongValueLabels;
use crate::spss::sav::extensions::long_variable_names::LongVariableNames;
use crate::spss::sav::extensions::machine_integer_info::MachineIntegerInfo;
use crate::spss::sav::extensions::multiple_response_sets::MultipleResponseSets;
use crate::spss::sav::extensions::raw_display_parameters::RawDisplayParameters;
use crate::spss::sav::extensions::unknown_extension::UnknownExtension;
use crate::spss::sav::extensions::uuid::Uuid;
use crate::spss::sav::extensions::variable_attributes::VariableAttributes;
use crate::spss::sav::extensions::variable_sets::VariableSets;
use crate::spss::sav::extensions::very_long_strings::VeryLongStrings;

/// One extension record read from a SAV file.
///
/// Subtype-to-variant assignments here mirror PSPP's documented
/// system file format and `ReadStat`'s implementation; the spec md
/// in this repository's reference directory has known errors and is
/// not authoritative. Subtypes that this enum doesn't yet carry
/// (e.g., XML display info = 24) fall through to [`Unknown`](Self::Unknown)
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
    /// Subtype 5 — named variable groupings.
    VariableSets(VariableSets),
    /// Subtype 7 — multiple response sets, pre-v14 format.
    /// Subtype 19 carries the post-v14 form (which adds
    /// `CATEGORYLABELS` support). Parser not yet wired up.
    MultipleResponseSets(MultipleResponseSets),
    /// Subtype 10 — extra product information (a free-form string
    /// identifying the writing product, beyond the header's product
    /// name).
    ExtraProductInfo(ExtraProductInfo),
    /// Subtype 12 — a file UUID in RFC 4122 text form.
    Uuid(Uuid),
    /// Subtype 11 — per-variable display parameters (measurement
    /// level, display width, alignment) carried verbatim from the
    /// record's payload. Per-variable slicing into typed
    /// [`VariableDisplay`](crate::spss::sav::extensions::variable_display::VariableDisplay)
    /// values happens during schema finalization.
    DisplayParameters(RawDisplayParameters),
    /// Subtype 13 — long-variable-name mappings (short → long name).
    LongVariableNames(LongVariableNames),
    /// Subtype 14 — very-long-string widths for variables wider
    /// than 255 bytes.
    VeryLongStrings(VeryLongStrings),
    /// Subtype 16 — extended number of cases, authoritative when
    /// the header's 32-bit `case_count` field is `-1`.
    ExtendedNumberOfCases(ExtendedNumberOfCases),
    /// Subtype 17 — file-level custom attributes (key-value pairs
    /// attached to the file, not to any particular variable).
    FileAttributes(FileAttributes),
    /// Subtype 18 — per-variable custom attributes.
    VariableAttributes(VariableAttributes),
    /// Subtype 20 — declared character encoding label.
    CharacterEncoding(CharacterEncoding),
    /// Subtype 21 — long value labels (for very-long-string
    /// variables).
    LongValueLabels(LongValueLabels),
    /// Subtype 22 — long missing values (for very-long-string
    /// variables).
    LongMissingValues(LongMissingValues),
    /// An extension subtype this library does not yet recognize. The
    /// raw bytes are preserved verbatim for round-trip fidelity.
    Unknown(UnknownExtension),
}

impl ExtensionRecord {
    /// Which subtype this record carries.
    ///
    /// [`Unknown`](Self::Unknown) maps to
    /// [`ExtensionSubtype::Unrecognized`] — the record's actual on-disk
    /// code stays available via
    /// [`UnknownExtension::subtype`](crate::spss::sav::extensions::unknown_extension::UnknownExtension::subtype).
    ///
    /// Subtypes 7 and 19 share the
    /// [`MultipleResponseSets`](Self::MultipleResponseSets) variant, so
    /// this reports 7 for both. A caller that needs to tell the two
    /// apart has to look at the record as it streams; nothing
    /// downstream of parsing distinguishes them.
    #[must_use]
    pub fn subtype(&self) -> ExtensionSubtype {
        match self {
            Self::MachineIntegerInfo(_) => ExtensionSubtype::MachineIntegerInfo,
            Self::FloatInfo(_) => ExtensionSubtype::FloatInfo,
            Self::VariableSets(_) => ExtensionSubtype::VariableSets,
            Self::MultipleResponseSets(_) => ExtensionSubtype::MultipleResponseSets,
            Self::ExtraProductInfo(_) => ExtensionSubtype::ExtraProductInfo,
            Self::Uuid(_) => ExtensionSubtype::Uuid,
            Self::DisplayParameters(_) => ExtensionSubtype::DisplayParameters,
            Self::LongVariableNames(_) => ExtensionSubtype::LongVariableNames,
            Self::VeryLongStrings(_) => ExtensionSubtype::VeryLongStrings,
            Self::ExtendedNumberOfCases(_) => ExtensionSubtype::ExtendedNumberOfCases,
            Self::FileAttributes(_) => ExtensionSubtype::FileAttributes,
            Self::VariableAttributes(_) => ExtensionSubtype::VariableAttributes,
            Self::CharacterEncoding(_) => ExtensionSubtype::CharacterEncoding,
            Self::LongValueLabels(_) => ExtensionSubtype::LongValueLabels,
            Self::LongMissingValues(_) => ExtensionSubtype::LongMissingValues,
            Self::Unknown(_) => ExtensionSubtype::Unrecognized,
        }
    }
}
