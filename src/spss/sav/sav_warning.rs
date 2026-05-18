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
    /// [`ExtensionRecord::Unknown`](crate::spss::sav::extensions::Unknown).
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
    /// The byte-order code in an extension subtype-5
    /// (`MachineIntegerInfo`) record disagreed with the byte order
    /// the header reader detected from the layout-code field. The
    /// header-detected byte order is taken as authoritative; the
    /// record is still surfaced verbatim.
    HeaderByteOrderMismatch {
        /// Raw `endianness` field value from the subtype-5 record.
        record_value: i32,
    },
    /// The floating-point-representation code in an extension
    /// subtype-5 (`MachineIntegerInfo`) record disagreed with the
    /// float format the header reader detected from the bias field.
    /// The header-detected format is taken as authoritative; the
    /// record is still surfaced verbatim.
    HeaderFloatFormatMismatch {
        /// Raw `floating_point_representation` field value from the
        /// subtype-5 record.
        record_value: i32,
    },
    /// A subsequent extension record carrying float sentinels
    /// (subtype 4 [`FloatSentinels`](crate::spss::sav::extensions::float_sentinels::FloatSentinels)
    /// or subtype 6 [`MachineFloatInfo`](crate::spss::sav::extensions::machine_float_info::MachineFloatInfo))
    /// disagreed with an earlier sentinels-bearing record from
    /// either subtype. SPSS emits both subtypes for cross-check
    /// redundancy, and they're expected to agree; both records still
    /// surface verbatim.
    FloatSentinelsCrossCheckMismatch {
        /// On-disk subtype number of the record that triggered the
        /// mismatch (i.e., the second of the two sentinels-bearing
        /// records observed).
        subtype: u32,
    },
}
