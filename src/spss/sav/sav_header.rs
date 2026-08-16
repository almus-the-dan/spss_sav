//! SAV file header.

use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::compression::compression_kind::CompressionKind;
use crate::spss::sav::float_encoding::FloatEncoding;
use crate::spss::sav::float_format::FloatFormat;
use crate::spss::sav::sav_creation_timestamp::SavCreationTimestamp;

/// SAV file header.
///
/// `SavHeader` is the user-facing summary of the file's preamble.
/// On the reader side it is populated from the on-disk header; on
/// the writer side the user constructs it via
/// [`SavHeaderBuilder`] to drive how the file is emitted. All
/// file-attribute writer options (compression, byte order, float
/// format, file label, …) live here rather than on the writer
/// itself.
///
/// The weight variable is the one preamble field deliberately absent.
/// The header block stores it as an offset into the data row, and the
/// variable it points at is a property of the dictionary — SPSS spells
/// it `WEIGHT BY`, alongside variable labels and missing values, not
/// alongside the file label. It is reported by
/// [`SavSchema::weight_variable`](crate::spss::sav::sav_schema::SavSchema::weight_variable),
/// which can hand back the variable itself rather than its name.
#[derive(Debug, Clone)]
pub struct SavHeader {
    product_name: String,
    file_label: String,
    creation_timestamp: SavCreationTimestamp,
    compression: CompressionKind,
    byte_order: ByteOrder,
    float_format: FloatFormat,
    bias: f64,
    case_count: Option<u32>,
    nominal_case_size: Option<u32>,
}

impl SavHeader {
    /// Returns a fresh [`SavHeaderBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> SavHeaderBuilder {
        SavHeaderBuilder::default()
    }

    /// Free-text product-name string (the 60-byte `prod_name` header
    /// field). Typically begins with `"@(#) SPSS DATA FILE"` and
    /// identifies the writing software. Empty when no product name was
    /// declared.
    #[must_use]
    #[inline]
    pub fn product_name(&self) -> &str {
        &self.product_name
    }

    /// Free-text file label (≤ 64 bytes on disk).
    #[must_use]
    #[inline]
    pub fn file_label(&self) -> &str {
        &self.file_label
    }

    /// Creation timestamp recorded in the header.
    #[must_use]
    #[inline]
    pub fn creation_timestamp(&self) -> &SavCreationTimestamp {
        &self.creation_timestamp
    }

    /// Compression scheme of the data section.
    #[must_use]
    #[inline]
    pub fn compression(&self) -> CompressionKind {
        self.compression
    }

    /// Byte order of multibyte values.
    #[must_use]
    #[inline]
    pub fn byte_order(&self) -> ByteOrder {
        self.byte_order
    }

    /// On-disk floating-point representation.
    #[must_use]
    #[inline]
    pub fn float_format(&self) -> FloatFormat {
        self.float_format
    }

    /// How this file encodes an `f64` on disk — its
    /// [`float_format`](Self::float_format) paired with its
    /// [`byte_order`](Self::byte_order).
    ///
    /// Derived from the two header fields rather than stored, so there
    /// is exactly one source of truth and no reachable state where the
    /// encoding disagrees with the header it came from. This is also
    /// the only way to obtain a [`FloatEncoding`], which is what makes
    /// the float conversions on
    /// [`FloatSentinels`](crate::spss::sav::extensions::float_sentinels::FloatSentinels)
    /// reachable.
    ///
    /// Both inputs come from the 176-byte preamble, so the encoding is
    /// available the moment
    /// [`HeaderReader::into_dictionary_reader`](crate::spss::sav::header_reader::HeaderReader::into_dictionary_reader)
    /// returns — no dictionary record is needed. Extension subtype 3
    /// declares a floating-point representation too, but the header
    /// stays authoritative: a disagreement raises
    /// [`SavWarning::HeaderFloatFormatMismatch`](crate::spss::sav::sav_warning::SavWarning::HeaderFloatFormatMismatch)
    /// rather than re-binding the encoding, so a consumer that reads it
    /// early and one that reads it late can never disagree.
    #[must_use]
    #[inline]
    pub fn float_encoding(&self) -> FloatEncoding {
        FloatEncoding::new(self.float_format, self.byte_order)
    }

    /// Bytecode-compression bias (typically `100.0`).
    #[must_use]
    #[inline]
    pub fn bias(&self) -> f64 {
        self.bias
    }

    /// Declared case count, or `None` when the file recorded `-1`
    /// ("unknown").
    #[must_use]
    #[inline]
    pub fn case_count(&self) -> Option<u32> {
        self.case_count
    }

    /// Declared variable count from the header's
    /// `nominal_case_size` field, or `None` when the file recorded
    /// `-1` (or any other negative). The actual variable count
    /// always comes from the schema; this accessor exposes the
    /// header-declared value for diagnostics.
    #[must_use]
    #[inline]
    pub fn nominal_case_size(&self) -> Option<u32> {
        self.nominal_case_size
    }
}

/// Builder for [`SavHeader`].
#[derive(Debug, Default, Clone)]
pub struct SavHeaderBuilder {
    product_name: Option<String>,
    file_label: Option<String>,
    creation_timestamp: Option<SavCreationTimestamp>,
    compression: Option<CompressionKind>,
    byte_order: Option<ByteOrder>,
    float_format: Option<FloatFormat>,
    bias: Option<f64>,
    case_count: Option<u32>,
    nominal_case_size: Option<u32>,
}

impl SavHeaderBuilder {
    /// Sets the free-text product-name string (the 60-byte `prod_name`
    /// header field). Empty by default.
    #[must_use]
    #[inline]
    pub fn product_name(mut self, name: impl Into<String>) -> Self {
        self.product_name = Some(name.into());
        self
    }

    /// Sets the free-text file label.
    #[must_use]
    #[inline]
    pub fn file_label(mut self, label: impl Into<String>) -> Self {
        self.file_label = Some(label.into());
        self
    }

    /// Sets the creation timestamp.
    #[must_use]
    #[inline]
    pub fn creation_timestamp(mut self, timestamp: SavCreationTimestamp) -> Self {
        self.creation_timestamp = Some(timestamp);
        self
    }

    /// Sets the compression scheme.
    #[must_use]
    #[inline]
    pub fn compression(mut self, compression: CompressionKind) -> Self {
        self.compression = Some(compression);
        self
    }

    /// Sets the byte order.
    #[must_use]
    #[inline]
    pub fn byte_order(mut self, byte_order: ByteOrder) -> Self {
        self.byte_order = Some(byte_order);
        self
    }

    /// Sets the on-disk floating-point representation.
    #[must_use]
    #[inline]
    pub fn float_format(mut self, float_format: FloatFormat) -> Self {
        self.float_format = Some(float_format);
        self
    }

    /// Sets the bytecode-compression bias.
    #[must_use]
    #[inline]
    pub fn bias(mut self, bias: f64) -> Self {
        self.bias = Some(bias);
        self
    }

    /// Sets the declared case count.
    ///
    /// Crate-internal — the writer patches this in via
    /// `write_record_count_and_finish()` when the underlying writer
    /// also implements [`Seek`](std::io::Seek). The reader populates
    /// it from the header's `case_count` field.
    #[inline]
    pub(crate) fn case_count(mut self, case_count: Option<u32>) -> Self {
        self.case_count = case_count;
        self
    }

    /// Sets the declared `nominal_case_size`.
    ///
    /// Crate-internal — the writer derives this from the schema's
    /// variable count. The reader populates it from the header's
    /// `nominal_case_size` field.
    #[inline]
    pub(crate) fn nominal_case_size(mut self, value: Option<u32>) -> Self {
        self.nominal_case_size = value;
        self
    }

    /// Finalizes this builder into a [`SavHeader`].
    ///
    /// Unset fields take spec-canonical defaults: empty strings for
    /// `product_name` and `file_label`; an empty
    /// [`SavCreationTimestamp::Unparsed`] for the timestamp;
    /// [`CompressionKind::None`], [`FloatFormat::Ieee754`],
    /// `bias = 100.0`. Byte order
    /// defaults to little-endian — the dominant choice in
    /// SPSS-authored files since the late 1990s. Required-vs-
    /// optional checks live at write time, not here.
    #[must_use]
    pub fn build(self) -> SavHeader {
        SavHeader {
            product_name: self.product_name.unwrap_or_default(),
            file_label: self.file_label.unwrap_or_default(),
            creation_timestamp: self.creation_timestamp.unwrap_or_else(|| {
                SavCreationTimestamp::Unparsed {
                    date: String::new(),
                    time: String::new(),
                }
            }),
            compression: self.compression.unwrap_or(CompressionKind::None),
            byte_order: self.byte_order.unwrap_or(ByteOrder::LittleEndian),
            float_format: self.float_format.unwrap_or(FloatFormat::Ieee754),
            bias: self.bias.unwrap_or(100.0),
            case_count: self.case_count,
            nominal_case_size: self.nominal_case_size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_encoding_pairs_the_two_header_fields() {
        let header = SavHeader::builder()
            .byte_order(ByteOrder::BigEndian)
            .float_format(FloatFormat::IbmHfp)
            .build();
        let encoding = header.float_encoding();
        assert_eq!(encoding.format(), FloatFormat::IbmHfp);
        assert_eq!(encoding.byte_order(), ByteOrder::BigEndian);
    }

    #[test]
    fn float_encoding_follows_the_builder_defaults() {
        let encoding = SavHeader::builder().build().float_encoding();
        assert_eq!(encoding.format(), FloatFormat::Ieee754);
        assert_eq!(encoding.byte_order(), ByteOrder::LittleEndian);
    }
}
