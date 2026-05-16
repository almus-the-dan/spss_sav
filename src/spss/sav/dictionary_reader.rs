//! Streaming reader for the SAV dictionary section.
//!
//! Sits between [`HeaderReader`](crate::spss::sav::header_reader::HeaderReader)
//! and the (future) record reader. Yields one
//! [`DictionaryRecord`] at a time —
//! variable records, value-label sets, document records, and
//! extension records freely interleaved between the header and the
//! `999` end-of-dictionary marker.

use std::io::Read;

use encoding_rs::Encoding;

use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::dictionary_format::{
    DICTIONARY_TERMINATOR_FILLER_LEN, MISSING_VALUE_ENTRY_LEN, RECORD_TYPE_DICTIONARY_TERMINATOR,
    RECORD_TYPE_DOCUMENT, RECORD_TYPE_EXTENSION, RECORD_TYPE_VALUE_LABEL, RECORD_TYPE_VARIABLE,
    VARIABLE_HAS_LABEL_OFFSET, VARIABLE_LABEL_PADDING, VARIABLE_MISSING_VALUE_COUNT_OFFSET,
    VARIABLE_PRINT_FORMAT_OFFSET, VARIABLE_RECORD_BODY_LEN, VARIABLE_SHORT_NAME_LEN,
    VARIABLE_SHORT_NAME_OFFSET, VARIABLE_TYPE_OFFSET, VARIABLE_WRITE_FORMAT_OFFSET,
};
use crate::spss::sav::dictionary_parse::{
    VariableTypeCode, compose_raw_missing_values, parse_has_label, parse_missing_value_count,
    parse_sav_format, parse_short_name, parse_variable_type,
};
use crate::spss::sav::dictionary_record::DictionaryRecord;
use crate::spss::sav::reader_state::ReaderState;
use crate::spss::sav::record_reader::RecordReader;
use crate::spss::sav::sav_error::{Field, FormatErrorKind, Result, SavError, Section};
use crate::spss::sav::sav_header::SavHeader;
use crate::spss::sav::sav_variable_header::SavVariableHeader;
use crate::spss::sav::sav_warning::SavWarning;
use crate::spss::sav::variable_type::VariableType;

/// Streaming reader for the SAV dictionary section.
///
/// Created by
/// [`HeaderReader::read_header`](crate::spss::sav::header_reader::HeaderReader::read_header).
/// Pull individual records via [`read_record`](Self::read_record)
/// until it returns `Ok(None)` (the `999` marker), or skip
/// straight to record reading via
/// [`into_record_reader`](Self::into_record_reader) which
/// auto-consumes any remaining dictionary records.
#[derive(Debug)]
pub struct DictionaryReader<R> {
    state: ReaderState<R>,
    header: SavHeader,
    #[allow(dead_code)] // exercised once the record reader phase lands.
    weight_variable_index: Option<usize>,
    variable_count: usize,
    /// Number of continuation records still expected for the most
    /// recent string-variable primary. Decrements as each
    /// continuation arrives; must reach `0` before any other record
    /// kind (including the dictionary terminator) is accepted.
    pending_continuations: u32,
}

impl<R> DictionaryReader<R> {
    pub(crate) fn new(
        state: ReaderState<R>,
        header: SavHeader,
        weight_variable_index: Option<usize>,
    ) -> Self {
        Self {
            state,
            header,
            weight_variable_index,
            variable_count: 0,
            pending_continuations: 0,
        }
    }

    /// 0-based index of the declared weight variable, if any.
    /// Surfaced via [`SavHeader::weight_variable`] (the long name)
    /// only after the dictionary phase finalizes; before then,
    /// callers can inspect the raw index here.
    #[allow(dead_code)] // exercised once the record reader phase lands.
    #[must_use]
    #[inline]
    pub(crate) fn weight_variable_index(&self) -> Option<usize> {
        self.weight_variable_index
    }

    /// The file header parsed by the upstream
    /// [`HeaderReader`](crate::spss::sav::header_reader::HeaderReader).
    /// `weight_variable` and any other extension-derived fields
    /// stay empty until the dictionary phase finalizes them.
    #[must_use]
    #[inline]
    pub fn header(&self) -> &SavHeader {
        &self.header
    }

    /// Warnings accumulated by the most recent
    /// [`read_record`](Self::read_record) call (or by
    /// [`HeaderReader::read_header`](crate::spss::sav::header_reader::HeaderReader::read_header)
    /// for the first call). Cleared at the start of each
    /// `read_record` invocation.
    #[must_use]
    #[inline]
    pub fn warnings(&self) -> &[SavWarning] {
        self.state.warnings()
    }
}

impl<R: Read> DictionaryReader<R> {
    /// Reads the next dictionary record. Returns `Ok(None)` once
    /// the `999` end-of-dictionary marker has been consumed.
    ///
    /// String-variable continuation records (type-2 with `type ==
    /// -1`) are consumed silently; the caller never sees them.
    ///
    /// # Errors
    ///
    /// Returns [`SavError::Io`] on read failures and
    /// [`SavError::Format`] when the bytes do not match a recognized
    /// record shape.
    ///
    /// # Panics
    ///
    /// Panics if the upstream
    /// [`HeaderReader`](crate::spss::sav::header_reader::HeaderReader)
    /// did not record a byte order before transitioning. The reader
    /// typestate chain guarantees this; a panic here would indicate
    /// a bug in the library rather than a malformed file.
    pub fn read_record(&mut self) -> Result<Option<DictionaryRecord>> {
        self.state.warnings_mut().clear();
        let byte_order = self
            .state
            .byte_order()
            .expect("byte order is set by the header reader");
        let encoding = self.state.encoding();

        loop {
            let position = self.state.position();
            let rec_type = self.state.read_i32(byte_order, Section::Dictionary)?;

            // Non-variable record kinds are not allowed while the
            // previous string variable's continuation run is still
            // pending. Variable records (type 2) need to read their
            // body before we can tell if they're a continuation or a
            // primary, so they validate themselves in
            // `read_variable_record`.
            if rec_type != RECORD_TYPE_VARIABLE && self.pending_continuations > 0 {
                return Err(SavError::format(
                    Section::Dictionary,
                    position,
                    FormatErrorKind::MissingContinuationRecord {
                        expected_remaining: self.pending_continuations,
                    },
                ));
            }

            match rec_type {
                RECORD_TYPE_VARIABLE => {
                    if let Some(record) =
                        self.read_variable_record(position, byte_order, encoding)?
                    {
                        return Ok(Some(record));
                    }
                    // Continuation record — loop to read the next.
                }
                RECORD_TYPE_DICTIONARY_TERMINATOR => {
                    self.state
                        .skip(DICTIONARY_TERMINATOR_FILLER_LEN, Section::Dictionary)?;
                    return Ok(None);
                }
                RECORD_TYPE_VALUE_LABEL => {
                    todo!("value-label record handling lands in Phase 5(b)")
                }
                RECORD_TYPE_DOCUMENT => {
                    todo!("document record handling lands in Phase 5(c)")
                }
                RECORD_TYPE_EXTENSION => {
                    todo!("extension record handling lands in Phase 5(d)")
                }
                value => {
                    return Err(SavError::format(
                        Section::Dictionary,
                        position,
                        FormatErrorKind::UnknownRecordType { value },
                    ));
                }
            }
        }
    }

    /// Auto-consumes any remaining dictionary records, finalizes
    /// the schema, and transitions to record reading.
    ///
    /// # Errors
    ///
    /// Returns whatever [`read_record`](Self::read_record) would
    /// return for any record consumed during finalization.
    pub fn into_record_reader(self) -> Result<RecordReader<R>> {
        todo!("body lands with the record reader phase")
    }

    /// Reads a type-2 variable record's body plus any trailing
    /// label and missing-value blocks. Returns `Ok(None)` when the
    /// record is a continuation (collapsed silently into the
    /// previous logical variable).
    fn read_variable_record(
        &mut self,
        position: u64,
        byte_order: ByteOrder,
        encoding: &'static Encoding,
    ) -> Result<Option<DictionaryRecord>> {
        let body: [u8; VARIABLE_RECORD_BODY_LEN] = self.state.read_array(Section::Dictionary)?;

        let type_value = four_bytes(&body, VARIABLE_TYPE_OFFSET);
        let type_value = byte_order.read_i32(type_value);
        let type_code = parse_variable_type(type_value, position)?;

        if matches!(type_code, VariableTypeCode::Continuation) {
            if self.pending_continuations == 0 {
                let error = SavError::format(
                    Section::Dictionary,
                    position,
                    FormatErrorKind::UnexpectedContinuationRecord,
                );
                return Err(error);
            }
            self.pending_continuations -= 1;
            return Ok(None);
        }

        // Non-continuation primary: the previous string variable's
        // continuation run must already be complete.
        if self.pending_continuations > 0 {
            let error = SavError::format(
                Section::Dictionary,
                position,
                FormatErrorKind::MissingContinuationRecord {
                    expected_remaining: self.pending_continuations,
                },
            );
            return Err(error);
        }

        let has_label = four_bytes(&body, VARIABLE_HAS_LABEL_OFFSET);
        let has_label = byte_order.read_i32(has_label);
        let has_label = parse_has_label(has_label);

        let missing_count = four_bytes(&body, VARIABLE_MISSING_VALUE_COUNT_OFFSET);
        let missing_count = byte_order.read_i32(missing_count);
        if missing_count == -1 {
            let variable_index = u32::try_from(self.variable_count).unwrap_or(u32::MAX);
            let warning = SavWarning::InvalidMissingValueCount {
                variable_index,
                value: missing_count,
            };
            self.state.warnings_mut().push(warning);
        }
        let missing_count = parse_missing_value_count(missing_count, position)?;

        let print_packed = four_bytes(&body, VARIABLE_PRINT_FORMAT_OFFSET);
        let print_packed = byte_order.read_u32(print_packed);
        let print_format = parse_sav_format(print_packed);

        let write_packed = four_bytes(&body, VARIABLE_WRITE_FORMAT_OFFSET);
        let write_packed = byte_order.read_u32(write_packed);
        let write_format = parse_sav_format(write_packed);

        let name_bytes: [u8; 8] = body
            [VARIABLE_SHORT_NAME_OFFSET..VARIABLE_SHORT_NAME_OFFSET + VARIABLE_SHORT_NAME_LEN]
            .try_into()
            .expect("short-name slice is exactly 8 bytes");
        let short_name = parse_short_name(name_bytes, encoding);

        let label = if has_label {
            let label = self.read_variable_label(byte_order, encoding)?;
            Some(label)
        } else {
            None
        };

        let entry_count = missing_count.entry_count();
        let mut entries: Vec<[u8; 8]> = Vec::with_capacity(entry_count);
        for _ in 0..entry_count {
            let entry: [u8; MISSING_VALUE_ENTRY_LEN] =
                self.state.read_array(Section::Dictionary)?;
            entries.push(entry);
        }
        let missing_values = compose_raw_missing_values(missing_count, entries);

        let variable_type = match type_code {
            VariableTypeCode::Numeric => VariableType::Numeric,
            VariableTypeCode::String(width) => VariableType::String(u16::from(width)),
            VariableTypeCode::Continuation => unreachable!("handled above"),
        };

        // Set the continuation expectation for the next records.
        // A numeric primary owns no continuations; a string primary
        // of width W needs `ceil(W/8) - 1` continuations on disk.
        self.pending_continuations = match type_code {
            VariableTypeCode::Numeric => 0,
            VariableTypeCode::String(width) => u32::from(width).div_ceil(8) - 1,
            VariableTypeCode::Continuation => unreachable!("handled above"),
        };

        let mut builder = SavVariableHeader::builder()
            .short_name(short_name)
            .variable_type(variable_type)
            .missing_values(missing_values)
            .print_format(print_format)
            .write_format(write_format);
        if let Some(label) = label {
            builder = builder.label(label);
        }
        let header = builder.build();

        self.variable_count += 1;
        let record = DictionaryRecord::Variable(header);
        Ok(Some(record))
    }

    /// Reads the 4-byte `label_len` field followed by the padded
    /// label bytes (rounded up to the next multiple of 4).
    fn read_variable_label(
        &mut self,
        byte_order: ByteOrder,
        encoding: &'static Encoding,
    ) -> Result<String> {
        let position = self.state.position();
        let label_len = self.state.read_u32(byte_order, Section::Dictionary)?;
        let label_len = usize::try_from(label_len).map_err(|_| {
            SavError::format(
                Section::Dictionary,
                position,
                FormatErrorKind::FieldTooLarge {
                    field: Field::VariableLabel,
                },
            )
        })?;
        let padded_len = label_len.div_ceil(VARIABLE_LABEL_PADDING) * VARIABLE_LABEL_PADDING;
        let bytes = self.state.read_exact(padded_len, Section::Dictionary)?;
        let (cow, _, _) = encoding.decode(&bytes[..label_len]);
        Ok(cow.into_owned())
    }
}

fn four_bytes(body: &[u8], offset: usize) -> [u8; 4] {
    body[offset..offset + 4]
        .try_into()
        .expect("four-byte slice has the requested length")
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::spss::sav::byte_order::ByteOrder;
    use crate::spss::sav::dictionary_record::DictionaryRecord;
    use crate::spss::sav::raw_missing_values::RawMissingValues;
    use crate::spss::sav::sav_error::{FormatErrorKind, SavError};
    use crate::spss::sav::sav_format_kind::SavFormatKind;
    use crate::spss::sav::sav_reader::SavReader;
    use crate::spss::sav::sav_warning::SavWarning;
    use crate::spss::sav::variable_type::VariableType;

    use super::DictionaryReader;

    /// Builds a minimal valid 176-byte SAV header (uncompressed,
    /// little-endian, IEEE 754, bias 100.0).
    fn build_header(byte_order: ByteOrder) -> Vec<u8> {
        let i32_bytes = |v: i32| match byte_order {
            ByteOrder::LittleEndian => v.to_le_bytes(),
            ByteOrder::BigEndian => v.to_be_bytes(),
        };
        let f64_bytes = |v: f64| match byte_order {
            ByteOrder::LittleEndian => v.to_le_bytes(),
            ByteOrder::BigEndian => v.to_be_bytes(),
        };
        let mut buf = Vec::with_capacity(176);
        buf.extend_from_slice(b"$FL2");
        let mut prod = [b' '; 60];
        prod[..18].copy_from_slice(b"@(#) SPSS DATA FIL");
        buf.extend_from_slice(&prod);
        buf.extend_from_slice(&i32_bytes(2)); // layout_code
        buf.extend_from_slice(&i32_bytes(1)); // nominal_case_size
        buf.extend_from_slice(&i32_bytes(0)); // compression
        buf.extend_from_slice(&i32_bytes(0)); // weight_index
        buf.extend_from_slice(&i32_bytes(0)); // ncases
        buf.extend_from_slice(&f64_bytes(100.0)); // bias
        buf.extend_from_slice(b"01 Jan 24");
        buf.extend_from_slice(b"13:45:30");
        let mut label = [b' '; 64];
        label[..4].copy_from_slice(b"Test");
        buf.extend_from_slice(&label);
        buf.extend_from_slice(&[0u8; 3]);
        assert_eq!(buf.len(), 176);
        buf
    }

    /// Packs a `(kind_byte, width, decimals)` triple into the
    /// on-disk 4-byte format code (byte 0 = decimals, byte 1 =
    /// width, byte 2 = kind, byte 3 = 0).
    fn pack_format(kind: u8, width: u8, decimals: u8) -> u32 {
        u32::from_le_bytes([decimals, width, kind, 0])
    }

    /// Builds one variable record body (28 bytes) — caller appends
    /// any label and missing-value blocks themselves.
    fn build_variable_body(
        byte_order: ByteOrder,
        type_value: i32,
        has_label: i32,
        n_missing: i32,
        print: u32,
        write: u32,
        name: [u8; 8],
    ) -> Vec<u8> {
        let i32_bytes = |v: i32| match byte_order {
            ByteOrder::LittleEndian => v.to_le_bytes(),
            ByteOrder::BigEndian => v.to_be_bytes(),
        };
        let u32_bytes = |v: u32| match byte_order {
            ByteOrder::LittleEndian => v.to_le_bytes(),
            ByteOrder::BigEndian => v.to_be_bytes(),
        };
        let mut buf = Vec::with_capacity(28);
        buf.extend_from_slice(&i32_bytes(type_value));
        buf.extend_from_slice(&i32_bytes(has_label));
        buf.extend_from_slice(&i32_bytes(n_missing));
        buf.extend_from_slice(&u32_bytes(print));
        buf.extend_from_slice(&u32_bytes(write));
        buf.extend_from_slice(&name);
        buf
    }

    fn open(bytes: Vec<u8>) -> DictionaryReader<Cursor<Vec<u8>>> {
        SavReader::new()
            .from_reader(Cursor::new(bytes))
            .read_header()
            .unwrap()
    }

    fn write_rec_type(buf: &mut Vec<u8>, byte_order: ByteOrder, value: i32) {
        match byte_order {
            ByteOrder::LittleEndian => buf.extend_from_slice(&value.to_le_bytes()),
            ByteOrder::BigEndian => buf.extend_from_slice(&value.to_be_bytes()),
        }
    }

    fn write_padded_label(buf: &mut Vec<u8>, byte_order: ByteOrder, label: &[u8]) {
        let len_bytes = match byte_order {
            ByteOrder::LittleEndian => u32::try_from(label.len()).unwrap().to_le_bytes(),
            ByteOrder::BigEndian => u32::try_from(label.len()).unwrap().to_be_bytes(),
        };
        buf.extend_from_slice(&len_bytes);
        buf.extend_from_slice(label);
        let pad = (4 - (label.len() % 4)) % 4;
        buf.extend_from_slice(&vec![0u8; pad]);
    }

    fn write_terminator(buf: &mut Vec<u8>, byte_order: ByteOrder) {
        write_rec_type(buf, byte_order, 999);
        buf.extend_from_slice(&[0u8; 4]);
    }

    #[test]
    fn numeric_variable_no_label_no_missing() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(
            byte_order,
            0, // numeric
            0, // no label
            0, // no missing
            pack_format(5, 8, 2),
            pack_format(5, 8, 2),
            *b"AGE     ",
        ));
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        match record {
            DictionaryRecord::Variable(header) => {
                assert_eq!(header.short_name(), "AGE");
                assert_eq!(header.variable_type(), VariableType::Numeric);
                assert!(header.label().is_none());
                assert_eq!(header.missing_values(), &RawMissingValues::None);
                assert_eq!(header.print_format().kind(), SavFormatKind::F);
                assert_eq!(header.print_format().width(), 8);
                assert_eq!(header.print_format().decimals(), 2);
                assert_eq!(header.write_format().kind(), SavFormatKind::F);
            }
            _ => panic!("expected Variable record"),
        }
        assert!(dict.warnings().is_empty());

        assert!(dict.read_record().unwrap().is_none());
    }

    #[test]
    fn short_string_variable() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(
            byte_order,
            8, // string width 8
            0,
            0,
            pack_format(1, 8, 0), // A8
            pack_format(1, 8, 0),
            *b"NAME    ",
        ));
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        match record {
            DictionaryRecord::Variable(header) => {
                assert_eq!(header.short_name(), "NAME");
                assert_eq!(header.variable_type(), VariableType::String(8));
                assert_eq!(header.print_format().kind(), SavFormatKind::A);
            }
            _ => panic!("expected Variable record"),
        }
    }

    #[test]
    fn long_string_collapses_continuation_records() {
        // Width-32 string needs 4 segments: one normal + 3 continuations.
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(
            byte_order,
            32,
            0,
            0,
            pack_format(1, 32, 0),
            pack_format(1, 32, 0),
            *b"DESC    ",
        ));
        for _ in 0..3 {
            write_rec_type(&mut bytes, byte_order, 2);
            bytes.extend(build_variable_body(
                byte_order, -1, // continuation
                0, 0, 0, 0, [0; 8],
            ));
        }
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        match record {
            DictionaryRecord::Variable(header) => {
                assert_eq!(header.short_name(), "DESC");
                assert_eq!(header.variable_type(), VariableType::String(32));
            }
            _ => panic!("expected Variable record"),
        }
        // The three continuation records are consumed silently;
        // the next read_record sees the terminator.
        assert!(dict.read_record().unwrap().is_none());
    }

    #[test]
    fn variable_label_decoded() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(
            byte_order,
            0,
            1, // has_label
            0,
            pack_format(5, 8, 2),
            pack_format(5, 8, 2),
            *b"AGE     ",
        ));
        write_padded_label(&mut bytes, byte_order, b"Age in years");
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        match record {
            DictionaryRecord::Variable(header) => {
                assert_eq!(header.label(), Some("Age in years"));
            }
            _ => panic!("expected Variable record"),
        }
    }

    #[test]
    fn variable_label_padded_to_four_byte_boundary() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(
            byte_order,
            0,
            1,
            0,
            pack_format(5, 8, 2),
            pack_format(5, 8, 2),
            *b"V1      ",
        ));
        // 5-byte label "hello" pads up to 8 bytes on disk.
        write_padded_label(&mut bytes, byte_order, b"hello");
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        match record {
            DictionaryRecord::Variable(header) => {
                assert_eq!(header.label(), Some("hello"));
            }
            _ => panic!("expected Variable record"),
        }
    }

    #[test]
    fn three_discrete_missing_values() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(
            byte_order,
            0,
            0,
            3, // 3 discrete
            pack_format(5, 8, 2),
            pack_format(5, 8, 2),
            *b"V1      ",
        ));
        let v1 = 9.0_f64.to_le_bytes();
        let v2 = 99.0_f64.to_le_bytes();
        let v3 = 999.0_f64.to_le_bytes();
        bytes.extend_from_slice(&v1);
        bytes.extend_from_slice(&v2);
        bytes.extend_from_slice(&v3);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Variable(header) = record else {
            panic!("expected Variable record")
        };
        match header.missing_values() {
            RawMissingValues::Discrete(entries) => {
                assert_eq!(entries.len(), 3);
                assert_eq!(entries[0], v1);
                assert_eq!(entries[1], v2);
                assert_eq!(entries[2], v3);
            }
            other => panic!("expected Discrete(3), got {other:?}"),
        }
    }

    #[test]
    fn range_missing_values() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(
            byte_order,
            0,
            0,
            -2, // range
            pack_format(5, 8, 2),
            pack_format(5, 8, 2),
            *b"V1      ",
        ));
        bytes.extend_from_slice(&(-1.0_f64).to_le_bytes());
        bytes.extend_from_slice(&1.0_f64.to_le_bytes());
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Variable(header) = record else {
            panic!("expected Variable record")
        };
        match header.missing_values() {
            RawMissingValues::Range { low, high } => {
                assert_eq!(*low, (-1.0_f64).to_le_bytes());
                assert_eq!(*high, 1.0_f64.to_le_bytes());
            }
            other => panic!("expected Range, got {other:?}"),
        }
    }

    #[test]
    fn range_with_discrete_missing_values() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(
            byte_order,
            0,
            0,
            -3,
            pack_format(5, 8, 2),
            pack_format(5, 8, 2),
            *b"V1      ",
        ));
        bytes.extend_from_slice(&(-99.0_f64).to_le_bytes());
        bytes.extend_from_slice(&(-1.0_f64).to_le_bytes());
        bytes.extend_from_slice(&9999.0_f64.to_le_bytes());
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Variable(header) = record else {
            panic!("expected Variable record")
        };
        match header.missing_values() {
            RawMissingValues::RangeWithDiscrete {
                low,
                high,
                discrete,
            } => {
                assert_eq!(*low, (-99.0_f64).to_le_bytes());
                assert_eq!(*high, (-1.0_f64).to_le_bytes());
                assert_eq!(*discrete, 9999.0_f64.to_le_bytes());
            }
            other => panic!("expected RangeWithDiscrete, got {other:?}"),
        }
    }

    #[test]
    fn minus_one_missing_count_warns_and_treats_as_one_discrete() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(
            byte_order,
            0,
            0,
            -1, // undocumented quirk
            pack_format(5, 8, 2),
            pack_format(5, 8, 2),
            *b"V1      ",
        ));
        bytes.extend_from_slice(&9.0_f64.to_le_bytes());
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Variable(header) = record else {
            panic!("expected Variable record")
        };
        match header.missing_values() {
            RawMissingValues::Discrete(entries) => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0], 9.0_f64.to_le_bytes());
            }
            other => panic!("expected Discrete(1), got {other:?}"),
        }
        assert!(matches!(
            dict.warnings(),
            &[SavWarning::InvalidMissingValueCount {
                variable_index: 0,
                value: -1
            }]
        ));
    }

    #[test]
    fn warnings_are_cleared_between_calls() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // First record with -1 quirk.
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(
            byte_order,
            0,
            0,
            -1,
            pack_format(5, 8, 2),
            pack_format(5, 8, 2),
            *b"V1      ",
        ));
        bytes.extend_from_slice(&9.0_f64.to_le_bytes());
        // Second record, no quirk.
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(
            byte_order,
            0,
            0,
            0,
            pack_format(5, 8, 2),
            pack_format(5, 8, 2),
            *b"V2      ",
        ));
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        dict.read_record().unwrap();
        assert_eq!(dict.warnings().len(), 1);
        dict.read_record().unwrap();
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn big_endian_variable_record() {
        let byte_order = ByteOrder::BigEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(
            byte_order,
            0,
            0,
            0,
            pack_format(5, 8, 2),
            pack_format(5, 8, 2),
            *b"V1      ",
        ));
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        match record {
            DictionaryRecord::Variable(header) => {
                assert_eq!(header.short_name(), "V1");
                assert_eq!(header.print_format().kind(), SavFormatKind::F);
                assert_eq!(header.print_format().width(), 8);
                assert_eq!(header.print_format().decimals(), 2);
            }
            _ => panic!("expected Variable record"),
        }
    }

    #[test]
    fn continuation_as_first_record_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(
            byte_order, -1, // continuation with no preceding variable
            0, 0, 0, 0, [0; 8],
        ));
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => {
                assert_eq!(e.kind(), FormatErrorKind::UnexpectedContinuationRecord);
            }
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn unknown_record_type_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 42);

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => {
                assert_eq!(e.kind(), FormatErrorKind::UnknownRecordType { value: 42 });
            }
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn invalid_variable_type_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(
            byte_order,
            256, // out of range
            0,
            0,
            0,
            0,
            *b"V1      ",
        ));

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => match e.kind() {
                FormatErrorKind::UnexpectedValue { .. } => {}
                other => panic!("expected UnexpectedValue, got {other:?}"),
            },
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn invalid_missing_count_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(
            byte_order,
            0,
            0,
            4, // out of range
            pack_format(5, 8, 2),
            pack_format(5, 8, 2),
            *b"V1      ",
        ));

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => match e.kind() {
                FormatErrorKind::UnexpectedValue { .. } => {}
                other => panic!("expected UnexpectedValue, got {other:?}"),
            },
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn terminator_at_first_record_returns_none() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        assert!(dict.read_record().unwrap().is_none());
    }

    #[test]
    fn continuation_after_numeric_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // Numeric primary owns 0 continuations.
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(
            byte_order,
            0,
            0,
            0,
            pack_format(5, 8, 2),
            pack_format(5, 8, 2),
            *b"AGE     ",
        ));
        // Stray continuation — must be rejected.
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(byte_order, -1, 0, 0, 0, 0, [0; 8]));
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        dict.read_record().unwrap();
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => {
                assert_eq!(e.kind(), FormatErrorKind::UnexpectedContinuationRecord);
            }
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn too_few_continuations_errors_at_terminator() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // Width-32 string needs 3 continuations; provide only 2.
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(
            byte_order,
            32,
            0,
            0,
            pack_format(1, 32, 0),
            pack_format(1, 32, 0),
            *b"DESC    ",
        ));
        for _ in 0..2 {
            write_rec_type(&mut bytes, byte_order, 2);
            bytes.extend(build_variable_body(byte_order, -1, 0, 0, 0, 0, [0; 8]));
        }
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        dict.read_record().unwrap();
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => {
                assert_eq!(
                    e.kind(),
                    FormatErrorKind::MissingContinuationRecord {
                        expected_remaining: 1
                    }
                );
            }
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn extra_continuation_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // Width-32 string needs 3 continuations; provide 4.
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(
            byte_order,
            32,
            0,
            0,
            pack_format(1, 32, 0),
            pack_format(1, 32, 0),
            *b"DESC    ",
        ));
        for _ in 0..4 {
            write_rec_type(&mut bytes, byte_order, 2);
            bytes.extend(build_variable_body(byte_order, -1, 0, 0, 0, 0, [0; 8]));
        }
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        dict.read_record().unwrap();
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => {
                assert_eq!(e.kind(), FormatErrorKind::UnexpectedContinuationRecord);
            }
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn terminator_mid_continuation_run_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // Width-16 string needs 1 continuation, but we skip straight
        // to the terminator instead of supplying it.
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(
            byte_order,
            16,
            0,
            0,
            pack_format(1, 16, 0),
            pack_format(1, 16, 0),
            *b"DESC    ",
        ));
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        dict.read_record().unwrap();
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => {
                assert_eq!(
                    e.kind(),
                    FormatErrorKind::MissingContinuationRecord {
                        expected_remaining: 1
                    }
                );
            }
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn width_eight_string_needs_no_continuations() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // Width-8 string fits in exactly one segment; 0 continuations.
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(
            byte_order,
            8,
            0,
            0,
            pack_format(1, 8, 0),
            pack_format(1, 8, 0),
            *b"NAME    ",
        ));
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let _ = dict.read_record().unwrap().unwrap();
        // No continuations expected — terminator should succeed.
        assert!(dict.read_record().unwrap().is_none());
    }
}
