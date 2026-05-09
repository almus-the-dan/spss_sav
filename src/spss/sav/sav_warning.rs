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
}
