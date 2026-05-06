//! SAV-format-specific errors.

use core::fmt;

/// Section of a SAV file where an error occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Section {
    /// File header (magic, file label, byte order, creation timestamp).
    Header,
    /// Variable records and the schema they form.
    Schema,
    /// Value-label records.
    ValueLabels,
    /// Document records.
    Documents,
    /// Extension records.
    Extensions,
    /// Compressed/uncompressed data records.
    Records,
}

impl fmt::Display for Section {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Header => "header",
            Self::Schema => "schema",
            Self::ValueLabels => "value labels",
            Self::Documents => "documents",
            Self::Extensions => "extensions",
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
            Self::VariableFormat => "variable format",
            Self::MeasurementLevel => "measurement level",
            Self::ValueLabelName => "value-label name",
            Self::ValueLabelEntry => "value-label entry",
            Self::DocumentLine => "document line",
            Self::ExtensionSubtype => "extension subtype",
            Self::ExtensionElementSize => "extension element size",
            Self::ExtensionElementCount => "extension element count",
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
    #[allow(dead_code)] // exercised once the SAV reader/writer land.
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
    #[allow(dead_code)] // exercised once the SAV reader/writer land.
    pub(crate) fn io(section: Section, source: std::io::Error) -> Self {
        Self::Io { section, source }
    }

    /// Constructs a [`Format`](Self::Format) variant from its parts.
    #[allow(dead_code)] // exercised once the SAV reader/writer land.
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
