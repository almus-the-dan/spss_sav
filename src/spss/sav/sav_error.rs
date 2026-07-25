//! SAV-format-specific errors.

use core::fmt;

/// Section of a SAV file where an error occurred.
///
/// SAV is structurally three sections: a fixed-size header, a
/// stream of interleaved typed dictionary records terminated by the
/// `999` marker, and the data records that follow. The dictionary
/// records (variables, value-label sets, documents, extensions) are
/// not separable subsections of the file — their kind is captured
/// instead by the specific
/// [`FormatErrorKind`](FormatErrorKind)
/// variant or the relevant
/// [`Field`](Field).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Section {
    /// File header (magic, file label, byte order, creation timestamp).
    Header,
    /// Dictionary section: variable records, value-label records,
    /// document records, and extension records, freely interleaved
    /// between the header and the `999` end-of-dictionary marker.
    Dictionary,
    /// Compressed or uncompressed data records that follow the
    /// dictionary terminator.
    Records,
}

impl fmt::Display for Section {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Header => "header",
            Self::Dictionary => "dictionary",
            Self::Records => "records",
        })
    }
}

/// Specific field within a section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Field {
    /// File magic bytes.
    Magic,
    /// File label string.
    FileLabel,
    /// Creation date string.
    CreationDate,
    /// Creation time string.
    CreationTime,
    /// Variable count in the header.
    VariableCount,
    /// Case (record) count in the header.
    CaseCount,
    /// Compression code in the header.
    CompressionCode,
    /// Bias used by bytecode compression.
    CompressionBias,
    /// Variable name.
    VariableName,
    /// Variable label.
    VariableLabel,
    /// Variable type code from a type-2 record.
    VariableType,
    /// Variable record's `n_missing_values` field.
    MissingValueCount,
    /// Variable display format.
    VariableFormat,
    /// Variable measurement level.
    MeasurementLevel,
    /// Value-label set name.
    ValueLabelName,
    /// Value-label entry.
    ValueLabelEntry,
    /// Document line.
    DocumentLine,
    /// Extension record subtype.
    ExtensionSubtype,
    /// Extension record element size.
    ExtensionElementSize,
    /// Extension record element count.
    ExtensionElementCount,
    /// One `short=long` pair inside an extension subtype-13 record.
    LongVariableNamePair,
    /// One `short=width` pair inside an extension subtype-14 record.
    VeryLongStringPair,
    /// One attribute inside an extension subtype-17 record.
    FileAttribute,
    /// One attribute inside an extension subtype-18 record.
    VariableAttribute,
    /// Cell value within a record.
    CellValue,
}

impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Magic => "magic",
            Self::FileLabel => "file label",
            Self::CreationDate => "creation date",
            Self::CreationTime => "creation time",
            Self::VariableCount => "variable count",
            Self::CaseCount => "case count",
            Self::CompressionCode => "compression code",
            Self::CompressionBias => "compression bias",
            Self::VariableName => "variable name",
            Self::VariableLabel => "variable label",
            Self::VariableType => "variable type code",
            Self::MissingValueCount => "missing value count",
            Self::VariableFormat => "variable format",
            Self::MeasurementLevel => "measurement level",
            Self::ValueLabelName => "value-label name",
            Self::ValueLabelEntry => "value-label entry",
            Self::DocumentLine => "document line",
            Self::ExtensionSubtype => "extension subtype",
            Self::ExtensionElementSize => "extension element size",
            Self::ExtensionElementCount => "extension element count",
            Self::LongVariableNamePair => "long variable name pair",
            Self::VeryLongStringPair => "very long string pair",
            Self::FileAttribute => "file attribute",
            Self::VariableAttribute => "variable attribute",
            Self::CellValue => "cell value",
        })
    }
}

/// Specific kind of SAV format violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FormatErrorKind {
    /// Magic bytes do not match the expected `$FL2` / `$FL3` tag.
    InvalidMagic,
    /// A field contained an unexpected byte value.
    UnexpectedValue {
        /// Which field held the unexpected value.
        field: Field,
    },
    /// A section ended before the expected number of bytes.
    Truncated {
        /// Bytes expected.
        expected: u64,
        /// Bytes actually present.
        actual: u64,
    },
    /// A field's value exceeds the SAV format's representable range.
    FieldTooLarge {
        /// Field that was too large.
        field: Field,
    },
    /// String content is not valid in the file's declared encoding.
    InvalidEncoding {
        /// Field that failed to decode.
        field: Field,
    },
    /// A value-label set references a variable that doesn't exist.
    DanglingValueLabel,
    /// Bytecode compression bias did not match the expected value.
    InvalidCompressionBias,
    /// The header's `layout_code` field decoded to neither `2` nor
    /// `3` under either little-endian or big-endian interpretation.
    UnreadableLayoutCode,
    /// The header's `bias` field could not be decoded as the
    /// canonical value (`100.0`) under any of the recognized
    /// floating-point formats (IEEE 754, IBM HFP, VAX).
    UnknownFloatFormat,
    /// The dictionary section yielded a record-type tag outside the
    /// recognized set (`2`, `3`, `4`, `6`, `7`, `999`).
    UnknownRecordType {
        /// Raw record-type tag.
        value: i32,
    },
    /// A type-2 record carried `type == -1` (continuation) when no
    /// string variable was expecting one — either before any variable
    /// record, or after the previous string variable's continuations
    /// were already exhausted.
    UnexpectedContinuationRecord,
    /// A non-continuation record appeared while the previous string
    /// variable's continuation run was still incomplete.
    MissingContinuationRecord {
        /// Number of continuation records still expected.
        expected_remaining: u32,
    },
    /// A type-3 / type-4 adjacency rule was violated: either a
    /// type-4 record appeared without an immediately preceding
    /// type-3 (`saw == 4`), or a type-3 record was followed by
    /// something other than a type-4 (`saw` carries the offending
    /// record-type tag).
    UnpairedValueLabelRecord {
        /// Record-type tag observed in violation.
        saw: i32,
    },
}

impl fmt::Display for FormatErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMagic => f.write_str("invalid magic bytes"),
            Self::UnexpectedValue { field } => write!(f, "unexpected value in {field}"),
            Self::Truncated { expected, actual } => {
                write!(f, "truncated: expected {expected} bytes, got {actual}")
            }
            Self::FieldTooLarge { field } => {
                write!(f, "{field} value exceeds the SAV format limit")
            }
            Self::InvalidEncoding { field } => write!(f, "invalid encoding in {field}"),
            Self::DanglingValueLabel => f.write_str("value-label references a missing variable"),
            Self::InvalidCompressionBias => f.write_str("bytecode compression bias mismatch"),
            Self::UnreadableLayoutCode => f.write_str("unreadable layout code"),
            Self::UnknownFloatFormat => f.write_str("unknown floating-point format"),
            Self::UnknownRecordType { value } => write!(f, "unknown record type {value}"),
            Self::UnexpectedContinuationRecord => {
                f.write_str("continuation record when none was expected")
            }
            Self::MissingContinuationRecord { expected_remaining } => write!(
                f,
                "string variable expected {expected_remaining} more continuation record(s)",
            ),
            Self::UnpairedValueLabelRecord { saw } => {
                write!(f, "unpaired value-label record (saw record type {saw})")
            }
        }
    }
}

/// A SAV format violation with file context.
#[derive(Debug)]
pub struct FormatError {
    section: Section,
    position: u64,
    kind: FormatErrorKind,
}

impl FormatError {
    pub(crate) fn new(section: Section, position: u64, kind: FormatErrorKind) -> Self {
        Self {
            section,
            position,
            kind,
        }
    }

    /// Section of the file where the error occurred.
    #[must_use]
    #[inline]
    pub fn section(&self) -> Section {
        self.section
    }

    /// Byte offset in the file where the error was detected.
    #[must_use]
    #[inline]
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Specific kind of format violation.
    #[must_use]
    #[inline]
    pub fn kind(&self) -> FormatErrorKind {
        self.kind
    }
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "format error in {} section at byte {}: {}",
            self.section, self.position, self.kind,
        )
    }
}

impl std::error::Error for FormatError {}

/// Unified error type for SAV reading, writing, and value
/// construction.
#[derive(Debug)]
#[non_exhaustive]
pub enum SavError {
    /// I/O error from the underlying reader/writer.
    Io {
        /// Section being processed when the I/O error occurred.
        section: Section,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// File contents violate the SAV format specification.
    Format(FormatError),
    /// A user-supplied string cannot be encoded in the requested
    /// encoding (raised by value-construction helpers such as
    /// [`ValueLabelValue::from_str`](crate::spss::sav::value_label_value::ValueLabelValue::from_str)).
    InvalidEncoding,
    /// A user-supplied string-keyed value-label exceeds the SAV
    /// format's eight-byte slot.
    StringTooLong {
        /// Encoded byte count of the supplied value.
        actual: usize,
    },
    /// A discrete missing-value list contained more than three
    /// entries (the current SAV-format cap).
    TooManyMissingValues {
        /// Number of values supplied by the caller.
        actual: usize,
    },
}

impl SavError {
    /// Constructs an [`Io`](Self::Io) variant tagged with `section`.
    pub(crate) fn io(section: Section, source: std::io::Error) -> Self {
        Self::Io { section, source }
    }

    /// Constructs a [`Format`](Self::Format) variant from its parts.
    pub(crate) fn format(section: Section, position: u64, kind: FormatErrorKind) -> Self {
        Self::Format(FormatError::new(section, position, kind))
    }
}

impl fmt::Display for SavError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { section, source } => {
                write!(f, "I/O error in {section} section: {source}")
            }
            Self::Format(err) => fmt::Display::fmt(err, f),
            Self::InvalidEncoding => {
                f.write_str("string value cannot be encoded in the requested encoding")
            }
            Self::StringTooLong { actual } => write!(
                f,
                "string value-label key is {actual} bytes; the SAV format caps it at 8",
            ),
            Self::TooManyMissingValues { actual } => write!(
                f,
                "discrete missing-value list has {actual} entries; the SAV format caps it at 3",
            ),
        }
    }
}

impl std::error::Error for SavError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Format(err) => Some(err),
            Self::InvalidEncoding
            | Self::StringTooLong { .. }
            | Self::TooManyMissingValues { .. } => None,
        }
    }
}

/// Convenience alias for results from SAV operations.
pub type Result<T> = std::result::Result<T, SavError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_display() {
        assert_eq!(Section::Header.to_string(), "header");
        assert_eq!(Section::Dictionary.to_string(), "dictionary");
        assert_eq!(Section::Records.to_string(), "records");
    }

    #[test]
    fn field_display() {
        assert_eq!(Field::Magic.to_string(), "magic");
        assert_eq!(Field::FileLabel.to_string(), "file label");
        assert_eq!(Field::CompressionBias.to_string(), "compression bias");
        assert_eq!(Field::ValueLabelEntry.to_string(), "value-label entry");
        assert_eq!(
            Field::ExtensionElementCount.to_string(),
            "extension element count",
        );
    }

    #[test]
    fn format_error_kind_display_invalid_magic() {
        assert_eq!(
            FormatErrorKind::InvalidMagic.to_string(),
            "invalid magic bytes"
        );
    }

    #[test]
    fn format_error_kind_display_truncated() {
        let kind = FormatErrorKind::Truncated {
            expected: 10,
            actual: 5,
        };
        assert_eq!(kind.to_string(), "truncated: expected 10 bytes, got 5");
    }

    #[test]
    fn format_error_kind_display_unpaired_value_label() {
        let kind = FormatErrorKind::UnpairedValueLabelRecord { saw: 7 };
        assert_eq!(
            kind.to_string(),
            "unpaired value-label record (saw record type 7)",
        );
    }

    #[test]
    fn format_error_kind_display_unexpected_value() {
        let kind = FormatErrorKind::UnexpectedValue {
            field: Field::CompressionCode,
        };
        assert_eq!(kind.to_string(), "unexpected value in compression code");
    }

    #[test]
    fn format_error_carries_section_position_kind() {
        let err = FormatError::new(Section::Dictionary, 42, FormatErrorKind::InvalidMagic);
        assert_eq!(err.section(), Section::Dictionary);
        assert_eq!(err.position(), 42);
        assert_eq!(err.kind(), FormatErrorKind::InvalidMagic);
    }

    #[test]
    fn format_error_display_includes_context() {
        let err = FormatError::new(Section::Header, 0, FormatErrorKind::InvalidMagic);
        assert_eq!(
            err.to_string(),
            "format error in header section at byte 0: invalid magic bytes",
        );
    }

    #[test]
    fn sav_error_display_invalid_encoding() {
        let err = SavError::InvalidEncoding;
        assert_eq!(
            err.to_string(),
            "string value cannot be encoded in the requested encoding",
        );
    }

    #[test]
    fn sav_error_display_string_too_long() {
        let err = SavError::StringTooLong { actual: 12 };
        assert_eq!(
            err.to_string(),
            "string value-label key is 12 bytes; the SAV format caps it at 8",
        );
    }

    #[test]
    fn sav_error_display_too_many_missing_values() {
        let err = SavError::TooManyMissingValues { actual: 5 };
        assert_eq!(
            err.to_string(),
            "discrete missing-value list has 5 entries; the SAV format caps it at 3",
        );
    }

    #[test]
    fn sav_error_display_io_includes_section() {
        let err = SavError::io(
            Section::Records,
            std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "boom"),
        );
        assert!(err.to_string().contains("records section"));
        assert!(err.to_string().contains("boom"));
    }

    #[test]
    fn sav_error_format_constructor() {
        let err = SavError::format(Section::Header, 8, FormatErrorKind::InvalidCompressionBias);
        match err {
            SavError::Format(format_err) => {
                assert_eq!(format_err.section(), Section::Header);
                assert_eq!(format_err.position(), 8);
                assert_eq!(format_err.kind(), FormatErrorKind::InvalidCompressionBias);
            }
            _ => panic!("expected SavError::Format"),
        }
    }
}
