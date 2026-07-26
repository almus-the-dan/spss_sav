//! Reader for the 176-byte SAV file header.
//!
//! First phase of the SAV reader typestate chain. Created via
//! [`SavReader::from_path`](crate::spss::sav::sav_reader::SavReader::from_path)
//! (or the other `from_*` entry points). Call
//! [`read_header`](HeaderReader::read_header) to parse the header
//! and advance to the dictionary phase.

use std::io::Read;

use encoding_rs::Encoding;

use crate::spss::sav::dictionary_reader::DictionaryReader;
use crate::spss::sav::encoding_strategy::EncodingStrategy;
use crate::spss::sav::header_format::{
    BIAS_LEN, COMPRESSION_OFFSET, CREATION_DATE_LEN, CREATION_DATE_OFFSET, CREATION_TIME_LEN,
    CREATION_TIME_OFFSET, FILE_LABEL_LEN, FILE_LABEL_OFFSET, LAYOUT_CODE_OFFSET, NCASES_OFFSET,
    NOMINAL_CASE_SIZE_OFFSET, PRODUCT_NAME_LEN, PRODUCT_NAME_OFFSET, RECORD_TYPE_LEN,
    RECORD_TYPE_OFFSET, TRAILING_PADDING_LEN, WEIGHT_INDEX_OFFSET,
};
use crate::spss::sav::header_parse::{
    parse_bias, parse_case_count, parse_file_label, parse_layout_code, parse_magic,
    parse_nominal_case_size, parse_product_name, parse_weight_index, resolve_compression,
};
use crate::spss::sav::reader_state::ReaderState;
use crate::spss::sav::sav_creation_timestamp::SavCreationTimestamp;
use crate::spss::sav::sav_error::{Result, Section};
use crate::spss::sav::sav_header::SavHeader;
use crate::spss::sav::sav_warning::SavWarning;

/// Encoding used at header-read time, before any extension record
/// declares the file's actual encoding.
const HEADER_FALLBACK_ENCODING: &Encoding = encoding_rs::WINDOWS_1252;

/// The encoding to decode with before the file's own declaration is
/// reachable.
///
/// Interim behavior: both declaration sites (subtype 20 and subtype 3)
/// live at the very end of the dictionary, so the reader cannot honor
/// them while streaming and applies the strategy's best guess instead.
/// Deferred decoding replaces every call site with real resolution;
/// until then a `Declared` strategy with no `unspecified` fallback
/// silently gets `windows-1252` rather than the error it asks for.
fn interim_encoding(strategy: EncodingStrategy) -> &'static Encoding {
    match strategy {
        EncodingStrategy::Override(encoding) => encoding,
        EncodingStrategy::Declared { unspecified, .. } => {
            unspecified.unwrap_or(HEADER_FALLBACK_ENCODING)
        }
    }
}

/// Entry point for reading a SAV file.
///
/// Created via
/// [`SavReader::from_path`](crate::spss::sav::sav_reader::SavReader::from_path)
/// (or [`from_file`](crate::spss::sav::sav_reader::SavReader::from_file)
/// /
/// [`from_reader`](crate::spss::sav::sav_reader::SavReader::from_reader)),
/// then call [`read_header`](Self::read_header) to parse the file
/// header and advance to the dictionary phase.
#[derive(Debug)]
pub struct HeaderReader<R> {
    state: ReaderState<R>,
    encoding_strategy: EncodingStrategy,
}

impl<R> HeaderReader<R> {
    /// Constructs a new header reader, forwarding the encoding
    /// strategy from the upstream
    /// [`SavReader`](crate::spss::sav::sav_reader::SavReader).
    ///
    /// The initial encoding stored on `ReaderState` is the strategy's
    /// interim guess — see [`interim_encoding`]. The dictionary phase
    /// replaces it once the file's declared encoding becomes known
    /// (subtype 20, then subtype 3).
    pub(crate) fn new(reader: R, encoding_strategy: EncodingStrategy) -> Self {
        let encoding = interim_encoding(encoding_strategy);
        let state = ReaderState::new(reader, encoding);
        Self {
            state,
            encoding_strategy,
        }
    }

    /// The encoding strategy supplied via
    /// [`SavReader::encoding_strategy`](crate::spss::sav::sav_reader::SavReader::encoding_strategy).
    #[must_use]
    #[inline]
    pub fn encoding_strategy(&self) -> EncodingStrategy {
        self.encoding_strategy
    }

    /// Warnings accumulated so far. Empty before
    /// [`read_header`](Self::read_header) is called.
    #[must_use]
    #[inline]
    pub fn warnings(&self) -> &[SavWarning] {
        self.state.warnings()
    }
}

impl<R: Read> HeaderReader<R> {
    /// Parses the 176-byte file header and transitions to the
    /// dictionary phase.
    ///
    /// # Errors
    ///
    /// Returns [`SavError::Io`](crate::spss::sav::sav_error::SavError::Io)
    /// on read failures and
    /// [`SavError::Format`](crate::spss::sav::sav_error::SavError::Format)
    /// when the header bytes do not match a recognized SAV layout
    /// (bad magic, unreadable layout code, unknown float format).
    ///
    /// # Panics
    ///
    /// Debug builds assert that each header field lands at its
    /// spec-defined offset; a panic here would indicate a bug in
    /// the reader rather than a malformed file. Release builds skip
    /// these checks.
    pub fn read_header(mut self) -> Result<DictionaryReader<R>> {
        self.state.warnings_mut().clear();
        let encoding = self.state.encoding();

        let record_type_position = self.state.position();
        debug_assert_eq!(
            usize::try_from(record_type_position).unwrap(),
            RECORD_TYPE_OFFSET
        );
        let record_type = self.state.read_array::<RECORD_TYPE_LEN>(Section::Header)?;
        let magic = parse_magic(record_type, record_type_position)?;

        let product_name_position = self.state.position();
        debug_assert_eq!(
            usize::try_from(product_name_position).unwrap(),
            PRODUCT_NAME_OFFSET
        );
        let product_name = {
            let bytes = self.state.read_exact(PRODUCT_NAME_LEN, Section::Header)?;
            parse_product_name(bytes, encoding)
        };

        let layout_code_position = self.state.position();
        debug_assert_eq!(
            usize::try_from(layout_code_position).unwrap(),
            LAYOUT_CODE_OFFSET
        );
        let layout_code_bytes = self.state.read_array::<4>(Section::Header)?;
        let byte_order = parse_layout_code(layout_code_bytes, layout_code_position)?;
        self.state.set_byte_order(byte_order);

        debug_assert_eq!(
            usize::try_from(self.state.position()).unwrap(),
            NOMINAL_CASE_SIZE_OFFSET
        );
        let nominal_case_size_value = self.state.read_i32(byte_order, Section::Header)?;
        let nominal_case_size = parse_nominal_case_size(nominal_case_size_value);

        debug_assert_eq!(
            usize::try_from(self.state.position()).unwrap(),
            COMPRESSION_OFFSET
        );
        let compression_code = self.state.read_i32(byte_order, Section::Header)?;
        let (compression, compression_warning) =
            resolve_compression(compression_code, magic, record_type);
        if let Some(warning) = compression_warning {
            self.state.warnings_mut().push(warning);
        }

        debug_assert_eq!(
            usize::try_from(self.state.position()).unwrap(),
            WEIGHT_INDEX_OFFSET
        );
        let weight_index_value = self.state.read_i32(byte_order, Section::Header)?;
        let weight_variable_index_one_based = parse_weight_index(weight_index_value);
        let weight_variable_index = weight_variable_index_one_based.map(|i| i - 1);

        debug_assert_eq!(
            usize::try_from(self.state.position()).unwrap(),
            NCASES_OFFSET
        );
        let case_count_value = self.state.read_i32(byte_order, Section::Header)?;
        let case_count = parse_case_count(case_count_value);

        let bias_position = self.state.position();
        let bias_bytes = self.state.read_array::<BIAS_LEN>(Section::Header)?;
        let (float_format, bias) = parse_bias(bias_bytes, byte_order, bias_position)?;

        debug_assert_eq!(
            usize::try_from(self.state.position()).unwrap(),
            CREATION_DATE_OFFSET
        );
        let date_bytes = self
            .state
            .read_array::<CREATION_DATE_LEN>(Section::Header)?;

        debug_assert_eq!(
            usize::try_from(self.state.position()).unwrap(),
            CREATION_TIME_OFFSET
        );
        let time_bytes = self
            .state
            .read_array::<CREATION_TIME_LEN>(Section::Header)?;

        let creation_timestamp = SavCreationTimestamp::from_header_bytes(date_bytes, time_bytes);

        debug_assert_eq!(
            usize::try_from(self.state.position()).unwrap(),
            FILE_LABEL_OFFSET
        );
        let file_label = {
            let bytes = self.state.read_exact(FILE_LABEL_LEN, Section::Header)?;
            parse_file_label(bytes, encoding)
        };

        self.state.skip(TRAILING_PADDING_LEN, Section::Header)?;

        let header = SavHeader::builder()
            .product_name(product_name)
            .file_label(file_label)
            .creation_timestamp(creation_timestamp)
            .compression(compression)
            .byte_order(byte_order)
            .float_format(float_format)
            .bias(bias)
            .case_count(case_count)
            .nominal_case_size(nominal_case_size)
            .build();

        let reader = DictionaryReader::new(self.state, header, weight_variable_index);
        Ok(reader)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::spss::sav::byte_order::ByteOrder;
    use crate::spss::sav::compression::Compression;
    use crate::spss::sav::encoding_strategy::EncodingStrategy;
    use crate::spss::sav::float_format::FloatFormat;
    use crate::spss::sav::sav_creation_timestamp::SavCreationTimestamp;
    use crate::spss::sav::sav_error::{FormatErrorKind, SavError};
    use crate::spss::sav::sav_reader::SavReader;
    use crate::spss::sav::sav_warning::SavWarning;

    use super::interim_encoding;

    /// Builder for known-good SAV header byte sequences in tests.
    struct HeaderBytes {
        rec_type: [u8; 4],
        prod_name: String,
        byte_order: ByteOrder,
        nominal_case_size: i32,
        compression_code: i32,
        weight_index: i32,
        ncases: i32,
        bias: f64,
        creation_date: [u8; 9],
        creation_time: [u8; 8],
        file_label: String,
    }

    impl HeaderBytes {
        fn new() -> Self {
            Self {
                rec_type: *b"$FL2",
                prod_name: "@(#) SPSS DATA FILE spss_sav 0.1.0".to_string(),
                byte_order: ByteOrder::LittleEndian,
                nominal_case_size: 5,
                compression_code: 1,
                weight_index: 0,
                ncases: 100,
                bias: 100.0,
                creation_date: *b"01 Jan 24",
                creation_time: *b"13:45:30",
                file_label: "Test dataset".to_string(),
            }
        }

        fn build(&self) -> Vec<u8> {
            let mut buf = Vec::with_capacity(176);
            buf.extend_from_slice(&self.rec_type);

            let mut prod = [b' '; 60];
            let bytes = self.prod_name.as_bytes();
            let len = bytes.len().min(60);
            prod[..len].copy_from_slice(&bytes[..len]);
            buf.extend_from_slice(&prod);

            buf.extend_from_slice(&self.write_i32(2));
            buf.extend_from_slice(&self.write_i32(self.nominal_case_size));
            buf.extend_from_slice(&self.write_i32(self.compression_code));
            buf.extend_from_slice(&self.write_i32(self.weight_index));
            buf.extend_from_slice(&self.write_i32(self.ncases));
            buf.extend_from_slice(&self.write_f64(self.bias));
            buf.extend_from_slice(&self.creation_date);
            buf.extend_from_slice(&self.creation_time);

            let mut label = [b' '; 64];
            let bytes = self.file_label.as_bytes();
            let len = bytes.len().min(64);
            label[..len].copy_from_slice(&bytes[..len]);
            buf.extend_from_slice(&label);

            buf.extend_from_slice(&[0u8; 3]);
            assert_eq!(buf.len(), 176);
            buf
        }

        fn write_i32(&self, v: i32) -> [u8; 4] {
            match self.byte_order {
                ByteOrder::LittleEndian => v.to_le_bytes(),
                ByteOrder::BigEndian => v.to_be_bytes(),
            }
        }

        fn write_f64(&self, v: f64) -> [u8; 8] {
            match self.byte_order {
                ByteOrder::LittleEndian => v.to_le_bytes(),
                ByteOrder::BigEndian => v.to_be_bytes(),
            }
        }
    }

    fn read(
        bytes: Vec<u8>,
    ) -> Result<crate::spss::sav::dictionary_reader::DictionaryReader<Cursor<Vec<u8>>>, SavError>
    {
        SavReader::new()
            .from_reader(Cursor::new(bytes))
            .read_header()
    }

    fn read_with(
        bytes: Vec<u8>,
        strategy: EncodingStrategy,
    ) -> Result<crate::spss::sav::dictionary_reader::DictionaryReader<Cursor<Vec<u8>>>, SavError>
    {
        SavReader::new()
            .encoding_strategy(strategy)
            .from_reader(Cursor::new(bytes))
            .read_header()
    }

    // -- Happy paths --------------------------------------------------------

    #[test]
    fn little_endian_bytecode_round_trip() {
        let dict = read(HeaderBytes::new().build()).unwrap();
        let header = dict.header();
        assert_eq!(header.byte_order(), ByteOrder::LittleEndian);
        assert_eq!(header.compression(), Compression::Bytecode);
        assert_eq!(header.float_format(), FloatFormat::Ieee754);
        assert!((header.bias() - 100.0).abs() < f64::EPSILON);
        assert_eq!(header.case_count(), Some(100));
        assert_eq!(header.nominal_case_size(), Some(5));
        assert_eq!(header.file_label(), "Test dataset");
        assert!(header.product_name().starts_with("@(#) SPSS DATA FILE"));
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn big_endian_uncompressed_round_trip() {
        let mut h = HeaderBytes::new();
        h.byte_order = ByteOrder::BigEndian;
        h.compression_code = 0;
        let dict = read(h.build()).unwrap();
        assert_eq!(dict.header().byte_order(), ByteOrder::BigEndian);
        assert_eq!(dict.header().compression(), Compression::None);
    }

    #[test]
    fn zsav_round_trip() {
        let mut h = HeaderBytes::new();
        h.rec_type = *b"$FL3";
        h.compression_code = 2;
        let dict = read(h.build()).unwrap();
        assert_eq!(dict.header().compression(), Compression::Zlib);
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn parsed_creation_timestamp() {
        let dict = read(HeaderBytes::new().build()).unwrap();
        match dict.header().creation_timestamp() {
            SavCreationTimestamp::Parsed(parsed) => {
                assert_eq!(parsed.day(), 1);
                assert_eq!(parsed.month(), 1);
                assert_eq!(parsed.year(), 24);
                assert_eq!(parsed.hour(), 13);
                assert_eq!(parsed.minute(), 45);
                assert_eq!(parsed.second(), 30);
            }
            unparsed @ SavCreationTimestamp::Unparsed { .. } => {
                panic!("expected Parsed, got {unparsed:?}")
            }
        }
    }

    #[test]
    fn unparseable_creation_timestamp_falls_back_to_raw() {
        let mut h = HeaderBytes::new();
        h.creation_date = *b"garbage  ";
        let dict = read(h.build()).unwrap();
        match dict.header().creation_timestamp() {
            SavCreationTimestamp::Unparsed { date, time } => {
                assert!(date.starts_with("garbage"));
                assert!(time.starts_with("13:45:30"));
            }
            other @ SavCreationTimestamp::Parsed(_) => {
                panic!("expected Unparsed, got {other:?}")
            }
        }
    }

    #[test]
    fn weight_index_resolves_to_zero_based() {
        let mut h = HeaderBytes::new();
        h.weight_index = 3;
        let dict = read(h.build()).unwrap();
        assert_eq!(dict.weight_variable_index(), Some(2));
    }

    #[test]
    fn weight_index_zero_means_no_weight() {
        let dict = read(HeaderBytes::new().build()).unwrap();
        assert_eq!(dict.weight_variable_index(), None);
    }

    #[test]
    fn negative_case_count_becomes_none() {
        let mut h = HeaderBytes::new();
        h.ncases = -1;
        let dict = read(h.build()).unwrap();
        assert_eq!(dict.header().case_count(), None);
    }

    #[test]
    fn negative_nominal_case_size_becomes_none() {
        let mut h = HeaderBytes::new();
        h.nominal_case_size = -1;
        let dict = read(h.build()).unwrap();
        assert_eq!(dict.header().nominal_case_size(), None);
    }

    #[test]
    fn file_label_trimmed() {
        let mut h = HeaderBytes::new();
        h.file_label = "tight".to_string();
        let dict = read(h.build()).unwrap();
        assert_eq!(dict.header().file_label(), "tight");
    }

    // -- Compression reconciliation ----------------------------------------

    #[test]
    fn fl2_with_zlib_code_warns_and_reads_zlib() {
        let mut h = HeaderBytes::new();
        h.rec_type = *b"$FL2";
        h.compression_code = 2;
        let dict = read(h.build()).unwrap();
        assert_eq!(dict.header().compression(), Compression::Zlib);
        assert!(matches!(
            dict.warnings(),
            &[SavWarning::CompressionMismatch { rec_type, code: 2 }] if &rec_type == b"$FL2"
        ));
    }

    #[test]
    fn fl3_with_none_code_warns_and_reads_none() {
        let mut h = HeaderBytes::new();
        h.rec_type = *b"$FL3";
        h.compression_code = 0;
        let dict = read(h.build()).unwrap();
        assert_eq!(dict.header().compression(), Compression::None);
        assert!(matches!(
            dict.warnings(),
            &[SavWarning::CompressionMismatch { rec_type, code: 0 }] if &rec_type == b"$FL3"
        ));
    }

    #[test]
    fn unknown_compression_code_warns_and_reads_none() {
        let mut h = HeaderBytes::new();
        h.compression_code = 7;
        let dict = read(h.build()).unwrap();
        assert_eq!(dict.header().compression(), Compression::None);
        assert!(matches!(
            dict.warnings(),
            &[SavWarning::UnknownCompressionCode { code: 7 }]
        ));
    }

    // -- Error cases -------------------------------------------------------

    #[test]
    fn invalid_magic_fails() {
        let mut h = HeaderBytes::new();
        h.rec_type = *b"NOPE";
        let err = read(h.build()).unwrap_err();
        assert!(matches!(
            err,
            SavError::Format(ref e) if e.kind() == FormatErrorKind::InvalidMagic
        ));
    }

    #[test]
    fn unreadable_layout_code_fails() {
        let bytes = HeaderBytes::new().build();
        let mut bytes = bytes;
        // Overwrite the layout-code field with garbage that decodes
        // to neither 2 nor 3 in either byte order.
        bytes[64..68].copy_from_slice(&999_i32.to_le_bytes());
        let err = read(bytes).unwrap_err();
        assert!(matches!(
            err,
            SavError::Format(ref e) if e.kind() == FormatErrorKind::UnreadableLayoutCode
        ));
    }

    #[test]
    fn unknown_float_format_fails() {
        let mut h = HeaderBytes::new();
        h.bias = 99.0;
        let err = read(h.build()).unwrap_err();
        assert!(matches!(
            err,
            SavError::Format(ref e) if e.kind() == FormatErrorKind::UnknownFloatFormat
        ));
    }

    #[test]
    fn truncated_header_fails() {
        let bytes: Vec<u8> = b"$FL2".to_vec();
        let err = read(bytes).unwrap_err();
        assert!(matches!(err, SavError::Io { .. }));
    }

    // -- Encoding strategy --------------------------------------------------

    /// An override reaches the header decode immediately, because it
    /// needs nothing from the file. This holds both before and after
    /// string decoding is deferred.
    #[test]
    fn override_decodes_the_file_label_with_the_supplied_encoding() {
        let mut bytes = HeaderBytes::new();
        bytes.file_label = "Fichier de démonstration".to_string();
        let dict = read_with(
            bytes.build(),
            EncodingStrategy::Override(encoding_rs::UTF_8),
        )
        .expect("read header");
        assert_eq!(dict.header().file_label(), "Fichier de démonstration");
    }

    #[test]
    fn interim_encoding_prefers_the_override() {
        assert_eq!(
            interim_encoding(EncodingStrategy::Override(encoding_rs::UTF_8)),
            encoding_rs::UTF_8
        );
    }

    #[test]
    fn interim_encoding_uses_the_unspecified_fallback() {
        let strategy = EncodingStrategy::Declared {
            unspecified: Some(encoding_rs::UTF_8),
            unrecognized: None,
        };
        assert_eq!(interim_encoding(strategy), encoding_rs::UTF_8);
    }

    /// Interim only: a strategy that asks to fail on an undeclared
    /// encoding cannot be honored until decoding is deferred, so it
    /// silently guesses instead.
    #[test]
    fn interim_encoding_without_a_fallback_guesses_windows_1252() {
        let strategy = EncodingStrategy::Declared {
            unspecified: None,
            unrecognized: None,
        };
        assert_eq!(interim_encoding(strategy), encoding_rs::WINDOWS_1252);
    }
}
