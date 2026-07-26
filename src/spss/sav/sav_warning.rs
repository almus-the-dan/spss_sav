//! Recoverable issues raised during SAV reading or writing.

/// A recoverable issue surfaced during SAV processing.
///
/// Warnings are accumulated in the reader/writer's per-phase state
/// and exposed via `.warnings()` after each operation. They never
/// halt processing — they only annotate it. The `Vec<SavWarning>` is
/// reused across phases and cleared at the start of each
/// `read_*`/`write_*` call.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum SavWarning {
    /// A `(format-kind, width, decimals)` triple does not match any
    /// valid SPSS display-format combination. The triple is preserved
    /// verbatim; the writer's `finish()` surfaces accumulated
    /// warnings.
    InvalidFormatCombination {
        /// Format-kind code as written.
        kind: u8,
        /// Declared width.
        width: u8,
        /// Declared decimal places.
        decimals: u8,
    },
    /// The reader's encoding strategy was `Override` and the override
    /// differed from the encoding the file declared.
    EncodingOverridden {
        /// Encoding label declared by the file.
        declared: &'static str,
        /// Encoding label actually used by the reader.
        used: &'static str,
    },
    /// An extension subtype this library does not yet recognize was
    /// preserved verbatim in
    /// [`ExtensionRecord::Unknown`](crate::spss::sav::extensions::extension_record::ExtensionRecord::Unknown).
    UnknownExtensionSubtype {
        /// The subtype number from the extension record header.
        subtype: u32,
    },
    /// The on-disk measurement-level byte was outside the canonical
    /// `0..=3` range. The raw byte is preserved verbatim in the
    /// resulting
    /// [`MeasurementLevel::Unknown`](crate::spss::sav::measurement_level::MeasurementLevel::Unknown).
    UnknownMeasurementLevel {
        /// 0-based variable index.
        variable_index: u32,
        /// Raw byte read from disk.
        byte: u8,
    },
    /// The header's `compression` field declared a different
    /// compression scheme than the magic bytes implied. The
    /// `compression` field is taken as authoritative (matching
    /// `ReadStat`'s behavior).
    CompressionMismatch {
        /// Magic bytes from the start of the file (`$FL2` or `$FL3`).
        rec_type: [u8; 4],
        /// Raw `compression` field value.
        code: i32,
    },
    /// The header's `compression` field held a value outside the
    /// recognized set (`0` = none, `1` = bytecode, `2` = zlib). The
    /// reader treats the file as uncompressed.
    UnknownCompressionCode {
        /// Raw `compression` field value.
        code: i32,
    },
    /// The variable record's `n_missing_values` field carried an
    /// undocumented `-1`. The dictionary reader treats it as a
    /// single discrete missing value (matching `ReadStat`'s data
    /// outcome).
    InvalidMissingValueCount {
        /// 0-based index of the variable in the order it was yielded
        /// by the dictionary reader.
        variable_index: u32,
        /// Raw `n_missing_values` field value.
        value: i32,
    },
    /// A type-4 value-label-variable record declared zero
    /// variables. The reader accepts the empty list (matching
    /// `ReadStat`'s data outcome — the resulting set attaches to no
    /// variable at finalization and is effectively discarded).
    EmptyValueLabelVariables,
    /// A type-3 value-label record carried two or more entries
    /// keyed to the same 8-byte value. The reader preserves all
    /// entries on
    /// [`RawValueLabelSet`](crate::spss::sav::raw_value_label_set::RawValueLabelSet);
    /// resolution at lookup time follows the existing first-wins
    /// rule on
    /// [`ValueLabelSet::label_for`](crate::spss::sav::value_label_set::ValueLabelSet::label_for).
    /// One warning is emitted per duplicate occurrence.
    DuplicateValueLabelKey {
        /// Raw 8-byte key that was repeated.
        key: [u8; 8],
    },
    /// The byte-order code in an extension subtype-3
    /// (`MachineIntegerInfo`) record disagreed with the byte order
    /// the header reader detected from the layout-code field. The
    /// header-detected byte order is taken as authoritative; the
    /// record is still surfaced verbatim.
    HeaderByteOrderMismatch {
        /// Raw `endianness` field value from the subtype-3 record.
        record_value: i32,
    },
    /// The floating-point-representation code in an extension
    /// subtype-3 (`MachineIntegerInfo`) record disagreed with the
    /// float format the header reader detected from the bias field.
    /// The header-detected format is taken as authoritative; the
    /// record is still surfaced verbatim.
    HeaderFloatFormatMismatch {
        /// Raw `floating_point_representation` field value from the
        /// subtype-3 record.
        record_value: i32,
    },
    /// The character encoding record (subtype 20) and the machine
    /// integer info record's `character_code` (subtype 3) both resolved
    /// to an encoding, but not to the same one. The subtype-20 label is
    /// taken as authoritative, matching PSPP.
    EncodingDeclarationMismatch {
        /// Encoding label declared by the subtype-20 record.
        label: String,
        /// Raw `character_code` value from the subtype-3 record.
        character_code: i32,
    },
    /// The file declared a character encoding that does not resolve to
    /// a supported encoding. The reader falls through to the other
    /// declaration site, then to its `unrecognized` fallback; without
    /// one, the read fails with
    /// [`SavError::EncodingUnrecognized`](crate::spss::sav::sav_error::SavError::EncodingUnrecognized)
    /// instead. One warning is emitted per unresolvable declaration.
    EncodingDeclarationUnrecognized {
        /// The declaration as the file wrote it — a subtype-20 label
        /// such as `"UTF-9"`, or a subtype-3 `character_code` rendered
        /// as `"character_code 437"`.
        declaration: String,
    },
    /// The file declared no character encoding, so the reader applied
    /// its `unspecified` fallback.
    EncodingUnspecified {
        /// Name of the encoding the reader fell back to.
        used: &'static str,
    },
}
