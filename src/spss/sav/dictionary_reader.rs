//! Streaming reader for the SAV dictionary section.
//!
//! Sits between [`HeaderReader`](crate::spss::sav::header_reader::HeaderReader)
//! and the (future) record reader. Yields one
//! [`DictionaryRecord`] at a time —
//! variable records, value-label sets, document records, and
//! extension records freely interleaved between the header and the
//! `999` end-of-dictionary marker.

use std::collections::HashSet;
use std::io::Read;

use encoding_rs::Encoding;

use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::dictionary_format::{
    DICTIONARY_TERMINATOR_FILLER_LEN, DOCUMENT_LINE_LEN, EXTENSION_SUBTYPE_CHARACTER_ENCODING,
    EXTENSION_SUBTYPE_DATA_FILE_ATTRIBUTES, EXTENSION_SUBTYPE_DISPLAY_PARAMETERS,
    EXTENSION_SUBTYPE_EXTENDED_NUMBER_OF_CASES, EXTENSION_SUBTYPE_FLOAT_INFO,
    EXTENSION_SUBTYPE_LONG_STRING_MISSING_VALUES, EXTENSION_SUBTYPE_LONG_STRING_VALUE_LABELS,
    EXTENSION_SUBTYPE_LONG_VARIABLE_NAMES, EXTENSION_SUBTYPE_MACHINE_INTEGER_INFO,
    EXTENSION_SUBTYPE_VARIABLE_ATTRIBUTES, EXTENSION_SUBTYPE_VERY_LONG_STRINGS,
    MISSING_VALUE_ENTRY_LEN, RECORD_TYPE_DICTIONARY_TERMINATOR, RECORD_TYPE_DOCUMENT,
    RECORD_TYPE_EXTENSION, RECORD_TYPE_VALUE_LABEL, RECORD_TYPE_VALUE_LABEL_VARIABLES,
    RECORD_TYPE_VARIABLE, VALUE_LABEL_LABEL_LEN_FIELD_LEN, VALUE_LABEL_VALUE_LEN,
    VARIABLE_HAS_LABEL_OFFSET, VARIABLE_LABEL_PADDING, VARIABLE_MISSING_VALUE_COUNT_OFFSET,
    VARIABLE_PRINT_FORMAT_OFFSET, VARIABLE_RECORD_BODY_LEN, VARIABLE_SHORT_NAME_LEN,
    VARIABLE_SHORT_NAME_OFFSET, VARIABLE_TYPE_OFFSET, VARIABLE_WRITE_FORMAT_OFFSET,
};
use crate::spss::sav::dictionary_parse::{
    VariableTypeCode, compose_raw_missing_values, normalize_value_label_variable_indices,
    parse_character_encoding, parse_data_file_attributes, parse_display_parameters,
    parse_extended_number_of_cases, parse_float_sentinels, parse_has_label,
    parse_long_string_missing_values, parse_long_string_value_labels, parse_long_variable_names,
    parse_machine_integer_info, parse_missing_value_count, parse_sav_format, parse_short_name,
    parse_value_label_entry, parse_variable_attributes, parse_variable_type,
    parse_very_long_strings, value_label_entry_size,
};
use crate::spss::sav::dictionary_record::DictionaryRecord;
use crate::spss::sav::document_record::DocumentRecord;
use crate::spss::sav::extensions::extension_record::ExtensionRecord;
use crate::spss::sav::extensions::machine_integer_info::MachineIntegerInfo;
use crate::spss::sav::extensions::unknown_extension::UnknownExtension;
use crate::spss::sav::raw_value_label_entry::RawValueLabelEntry;
use crate::spss::sav::raw_value_label_set::RawValueLabelSet;
use crate::spss::sav::reader_state::{ReaderState, u32_as_usize};
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
    /// 0-based physical record positions of each primary (non-
    /// continuation) variable record observed so far, in declaration
    /// order. Used to translate a type-4 record's 1-based physical
    /// variable indices into 0-based logical positions.
    primaries: Vec<u32>,
    /// Count of all type-2 records observed so far, primaries and
    /// continuations alike. The next primary's 0-based physical
    /// position before this counter is incremented.
    physical_variable_count: u32,
}

/// All state a per-subtype helper needs when handling one type-7
/// extension record. The envelope is built once in
/// [`DictionaryReader::read_extension_record`] and consumed by the
/// dispatched helper, which extracts whichever fields its subtype
/// requires.
struct ExtensionEnvelope {
    subtype: i32,
    element_size: u32,
    element_count: u32,
    element_size_usize: usize,
    element_count_usize: usize,
    element_size_position: u64,
    payload: Vec<u8>,
    byte_order: ByteOrder,
    encoding: &'static Encoding,
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
            primaries: Vec::new(),
            physical_variable_count: 0,
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
            let record_type = self.state.read_i32(byte_order, Section::Dictionary)?;

            // Non-variable record kinds are not allowed while the
            // previous string variable's continuation run is still
            // pending. Variable records (type 2) need to read their
            // body before we can tell if they're a continuation or a
            // primary, so they validate themselves in
            // `read_variable_record`.
            if record_type != RECORD_TYPE_VARIABLE && self.pending_continuations > 0 {
                let error = SavError::format(
                    Section::Dictionary,
                    position,
                    FormatErrorKind::MissingContinuationRecord {
                        expected_remaining: self.pending_continuations,
                    },
                );
                return Err(error);
            }

            match record_type {
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
                    let record = self.read_value_label_record(byte_order, encoding)?;
                    return Ok(Some(record));
                }
                RECORD_TYPE_VALUE_LABEL_VARIABLES => {
                    let error = SavError::format(
                        Section::Dictionary,
                        position,
                        FormatErrorKind::UnpairedValueLabelRecord {
                            saw: RECORD_TYPE_VALUE_LABEL_VARIABLES,
                        },
                    );
                    return Err(error);
                }
                RECORD_TYPE_DOCUMENT => {
                    let record = self.read_document_record(byte_order, encoding)?;
                    return Ok(Some(record));
                }
                RECORD_TYPE_EXTENSION => {
                    let record = self.read_extension_record(byte_order, encoding)?;
                    return Ok(Some(record));
                }
                value => {
                    let error = SavError::format(
                        Section::Dictionary,
                        position,
                        FormatErrorKind::UnknownRecordType { value },
                    );
                    return Err(error);
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
            self.physical_variable_count += 1;
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

        self.primaries.push(self.physical_variable_count);
        self.physical_variable_count += 1;
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
        let label_len =
            self.state
                .read_u32_as_usize(byte_order, Section::Dictionary, Field::VariableLabel)?;
        let padded_len = label_len.div_ceil(VARIABLE_LABEL_PADDING) * VARIABLE_LABEL_PADDING;
        let bytes = self.state.read_exact(padded_len, Section::Dictionary)?;
        let (cow, _, _) = encoding.decode(&bytes[..label_len]);
        Ok(cow.into_owned())
    }

    /// Reads a type-6 document record body — a `u32` line count
    /// followed by that many fixed-width [`DOCUMENT_LINE_LEN`]-byte
    /// lines, decoded through the file's active encoding.
    fn read_document_record(
        &mut self,
        byte_order: ByteOrder,
        encoding: &'static Encoding,
    ) -> Result<DictionaryRecord> {
        let line_count =
            self.state
                .read_u32_as_usize(byte_order, Section::Dictionary, Field::DocumentLine)?;

        let mut lines: Vec<String> = Vec::with_capacity(line_count);
        for _ in 0..line_count {
            let bytes = self
                .state
                .read_exact(DOCUMENT_LINE_LEN, Section::Dictionary)?;
            let (decoded, _, _) = encoding.decode(bytes);
            lines.push(decoded.into_owned());
        }

        let record = DocumentRecord::builder().lines(lines).build();
        let record = DictionaryRecord::Document(record);
        Ok(record)
    }

    /// Reads a type-7 extension record envelope (`subtype`,
    /// `element_size`, `element_count`) followed by its
    /// `element_size * element_count`-byte payload, then dispatches
    /// to the per-subtype helper. Unrecognized subtypes fall through
    /// to [`read_unknown_extension`](Self::read_unknown_extension),
    /// which preserves the payload verbatim and pushes a
    /// [`SavWarning::UnknownExtensionSubtype`].
    fn read_extension_record(
        &mut self,
        byte_order: ByteOrder,
        encoding: &'static Encoding,
    ) -> Result<DictionaryRecord> {
        let envelope = self.read_extension_envelope(byte_order, encoding)?;
        match envelope.subtype {
            EXTENSION_SUBTYPE_MACHINE_INTEGER_INFO => self.read_machine_integer_info(&envelope),
            EXTENSION_SUBTYPE_FLOAT_INFO => read_float_info(&envelope),
            EXTENSION_SUBTYPE_EXTENDED_NUMBER_OF_CASES => read_extended_number_of_cases(&envelope),
            EXTENSION_SUBTYPE_CHARACTER_ENCODING => read_character_encoding(&envelope),
            EXTENSION_SUBTYPE_LONG_VARIABLE_NAMES => read_long_variable_names(&envelope),
            EXTENSION_SUBTYPE_VERY_LONG_STRINGS => read_very_long_strings(&envelope),
            EXTENSION_SUBTYPE_DISPLAY_PARAMETERS => read_display_parameters(&envelope),
            EXTENSION_SUBTYPE_DATA_FILE_ATTRIBUTES => read_data_file_attributes(&envelope),
            EXTENSION_SUBTYPE_VARIABLE_ATTRIBUTES => read_variable_attributes(&envelope),
            EXTENSION_SUBTYPE_LONG_STRING_VALUE_LABELS => read_long_string_value_labels(&envelope),
            EXTENSION_SUBTYPE_LONG_STRING_MISSING_VALUES => {
                read_long_string_missing_values(&envelope)
            }
            _ => Ok(self.read_unknown_extension(envelope)),
        }
    }

    /// Reads the type-7 envelope fields (`subtype`, `element_size`,
    /// `element_count`) and the declared `element_size *
    /// element_count`-byte payload into an [`ExtensionEnvelope`] for
    /// the per-subtype helpers to consume.
    fn read_extension_envelope(
        &mut self,
        byte_order: ByteOrder,
        encoding: &'static Encoding,
    ) -> Result<ExtensionEnvelope> {
        let subtype = self.state.read_i32(byte_order, Section::Dictionary)?;
        let element_size_position = self.state.position();
        let element_size = self.state.read_u32(byte_order, Section::Dictionary)?;
        let element_count_position = self.state.position();
        let element_count = self.state.read_u32(byte_order, Section::Dictionary)?;
        let element_size_usize = u32_as_usize(
            element_size,
            element_size_position,
            Section::Dictionary,
            Field::ExtensionElementSize,
        )?;
        let element_count_usize = u32_as_usize(
            element_count,
            element_count_position,
            Section::Dictionary,
            Field::ExtensionElementCount,
        )?;
        let payload = self.read_extension_payload(element_size_usize, element_count_usize)?;
        let envelope = ExtensionEnvelope {
            subtype,
            element_size,
            element_count,
            element_size_usize,
            element_count_usize,
            element_size_position,
            payload,
            byte_order,
            encoding,
        };
        Ok(envelope)
    }

    /// Subtype 3 — also drives integer-info cross-checks against the
    /// header before yielding the typed record. Stateful (pushes
    /// warnings via [`cross_check_machine_integer_info`](Self::cross_check_machine_integer_info)),
    /// so it's a method instead of a module-level free function.
    fn read_machine_integer_info(
        &mut self,
        envelope: &ExtensionEnvelope,
    ) -> Result<DictionaryRecord> {
        let info = parse_machine_integer_info(
            envelope.element_size,
            envelope.element_count,
            &envelope.payload,
            envelope.byte_order,
            envelope.element_size_position,
        )?;
        self.cross_check_machine_integer_info(&info, envelope.byte_order);
        let record = ExtensionRecord::MachineIntegerInfo(info);
        let record = DictionaryRecord::Extension(record);
        Ok(record)
    }

    /// Fallback for unrecognized subtypes. Owns the envelope so it
    /// can move the payload into [`UnknownExtension`] without an
    /// extra allocation, and pushes a
    /// [`SavWarning::UnknownExtensionSubtype`] so callers can
    /// distinguish "carried verbatim" from "interpreted".
    fn read_unknown_extension(&mut self, envelope: ExtensionEnvelope) -> DictionaryRecord {
        let subtype_u32 = envelope.subtype.cast_unsigned();
        let warning = SavWarning::UnknownExtensionSubtype {
            subtype: subtype_u32,
        };
        self.state.warnings_mut().push(warning);
        let unknown = UnknownExtension::builder()
            .subtype(subtype_u32)
            .element_size(envelope.element_size_usize)
            .element_count(envelope.element_count_usize)
            .payload(envelope.payload)
            .build();
        let record = ExtensionRecord::Unknown(unknown);
        DictionaryRecord::Extension(record)
    }

    /// Compares the byte-order and floating-point codes carried by
    /// an extension subtype-3 record against the values the header
    /// reader already determined. Emits a
    /// [`SavWarning::HeaderByteOrderMismatch`] or
    /// [`SavWarning::HeaderFloatFormatMismatch`] for each
    /// disagreement; the header-derived values stay authoritative
    /// for downstream decoding. Unknown codes (those for which the
    /// typed accessors return `None`) are tolerated silently — the
    /// record's raw code remains available on
    /// [`MachineIntegerInfo`].
    fn cross_check_machine_integer_info(
        &mut self,
        info: &MachineIntegerInfo,
        header_byte_order: ByteOrder,
    ) {
        if let Some(record_byte_order) = info.endianness_kind()
            && record_byte_order != header_byte_order
        {
            let warning = SavWarning::HeaderByteOrderMismatch {
                record_value: info.endianness(),
            };
            self.state.warnings_mut().push(warning);
        }
        if let Some(record_format) = info.floating_point_representation_kind()
            && record_format != self.header.float_format()
        {
            let warning = SavWarning::HeaderFloatFormatMismatch {
                record_value: info.floating_point_representation(),
            };
            self.state.warnings_mut().push(warning);
        }
    }

    fn read_extension_payload(
        &mut self,
        element_size_usize: usize,
        element_count_usize: usize,
    ) -> Result<Vec<u8>> {
        let payload_position = self.state.position();
        let payload_len = element_size_usize
            .checked_mul(element_count_usize)
            .ok_or_else(|| {
                SavError::format(
                    Section::Dictionary,
                    payload_position,
                    FormatErrorKind::FieldTooLarge {
                        field: Field::ExtensionElementCount,
                    },
                )
            })?;
        let payload = self
            .state
            .read_exact(payload_len, Section::Dictionary)?
            .to_vec();
        Ok(payload)
    }

    /// Reads a type-3 value-label record body, the immediately
    /// following type-4 record, and combines them into a single
    /// [`DictionaryRecord::ValueLabelSet`].
    ///
    /// The type-3 record carries an unsigned `label_count` followed by
    /// that many entries, each an 8-byte value, a `u8` `unpadded_len`,
    /// and the padded label bytes. The type-4 record carries an
    /// unsigned `variable_count` followed by that many 1-based
    /// physical variable indices; the indices are normalized here
    /// into 0-based logical positions via
    /// [`normalize_value_label_variable_indices`].
    fn read_value_label_record(
        &mut self,
        byte_order: ByteOrder,
        encoding: &'static Encoding,
    ) -> Result<DictionaryRecord> {
        let label_count = self.state.read_u32_as_usize(
            byte_order,
            Section::Dictionary,
            Field::ValueLabelEntry,
        )?;

        let entries = self.read_raw_value_label_entries(encoding, label_count)?;

        // The very next record-type tag must be a type-4. Anything
        // else — including EOF, the dictionary terminator, or any
        // other dictionary record kind — is an unpaired-type-3
        // violation.
        let pair_position = self.state.position();
        let next_record_type = self.state.read_i32(byte_order, Section::Dictionary)?;
        if next_record_type != RECORD_TYPE_VALUE_LABEL_VARIABLES {
            let error = SavError::format(
                Section::Dictionary,
                pair_position,
                FormatErrorKind::UnpairedValueLabelRecord {
                    saw: next_record_type,
                },
            );
            return Err(error);
        }

        let variable_count =
            self.state
                .read_u32_as_usize(byte_order, Section::Dictionary, Field::VariableCount)?;
        let indices_position = self.state.position();
        let raw_indices = self.read_raw_variable_indexes(byte_order, variable_count)?;

        if variable_count == 0 {
            self.state
                .warnings_mut()
                .push(SavWarning::EmptyValueLabelVariables);
        }

        let variable_indices = normalize_value_label_variable_indices(
            &raw_indices,
            &self.primaries,
            indices_position,
        )?;

        let set = RawValueLabelSet::builder()
            .entries(entries)
            .variable_indices(variable_indices)
            .build();
        let set = DictionaryRecord::ValueLabelSet(set);
        Ok(set)
    }

    fn read_raw_value_label_entries(
        &mut self,
        encoding: &'static Encoding,
        label_count: usize,
    ) -> Result<Vec<RawValueLabelEntry>> {
        let mut entries: Vec<RawValueLabelEntry> = Vec::with_capacity(label_count);
        let mut seen_keys: HashSet<[u8; VALUE_LABEL_VALUE_LEN]> =
            HashSet::with_capacity(label_count);
        for _ in 0..label_count {
            let value: [u8; VALUE_LABEL_VALUE_LEN] = self.state.read_array(Section::Dictionary)?;
            let entry = self.read_raw_value_label_entry(encoding, value)?;
            if !seen_keys.insert(value) {
                self.state
                    .warnings_mut()
                    .push(SavWarning::DuplicateValueLabelKey { key: value });
            }
            entries.push(entry);
        }
        Ok(entries)
    }

    fn read_raw_value_label_entry(
        &mut self,
        encoding: &'static Encoding,
        value: [u8; 8],
    ) -> Result<RawValueLabelEntry> {
        let unpadded_len = self.state.read_u8(Section::Dictionary)?;
        let label_bytes_len = value_label_entry_size(unpadded_len)
            - VALUE_LABEL_VALUE_LEN
            - VALUE_LABEL_LABEL_LEN_FIELD_LEN;
        let label_bytes = self
            .state
            .read_exact(label_bytes_len, Section::Dictionary)?;
        let entry = parse_value_label_entry(value, unpadded_len, label_bytes, encoding);
        Ok(entry)
    }

    fn read_raw_variable_indexes(
        &mut self,
        byte_order: ByteOrder,
        variable_count: usize,
    ) -> Result<Vec<u32>> {
        let mut raw_indices: Vec<u32> = Vec::with_capacity(variable_count);
        for _ in 0..variable_count {
            let index = self.state.read_u32(byte_order, Section::Dictionary)?;
            raw_indices.push(index);
        }
        Ok(raw_indices)
    }
}

fn four_bytes(body: &[u8], offset: usize) -> [u8; 4] {
    body[offset..offset + 4]
        .try_into()
        .expect("four-byte slice has the requested length")
}

/// Subtype 4 — float-sentinel values (system-missing / highest /
/// lowest), carried verbatim from the envelope.
fn read_float_info(envelope: &ExtensionEnvelope) -> Result<DictionaryRecord> {
    let sentinels = parse_float_sentinels(
        envelope.element_size,
        envelope.element_count,
        &envelope.payload,
        envelope.element_size_position,
    )?;
    let record = ExtensionRecord::FloatInfo(sentinels);
    let record = DictionaryRecord::Extension(record);
    Ok(record)
}

/// Subtype 16 — extended (64-bit) case count.
fn read_extended_number_of_cases(envelope: &ExtensionEnvelope) -> Result<DictionaryRecord> {
    let extended = parse_extended_number_of_cases(
        envelope.element_size,
        envelope.element_count,
        &envelope.payload,
        envelope.byte_order,
        envelope.element_size_position,
    )?;
    let record = ExtensionRecord::ExtendedNumberOfCases(extended);
    let record = DictionaryRecord::Extension(record);
    Ok(record)
}

/// Subtype 20 — declared character encoding label.
fn read_character_encoding(envelope: &ExtensionEnvelope) -> Result<DictionaryRecord> {
    let name = parse_character_encoding(
        envelope.element_size,
        &envelope.payload,
        envelope.element_size_position,
    )?;
    let record = ExtensionRecord::CharacterEncoding(name);
    let record = DictionaryRecord::Extension(record);
    Ok(record)
}

/// Subtype 13 — long-variable-name mappings.
fn read_long_variable_names(envelope: &ExtensionEnvelope) -> Result<DictionaryRecord> {
    let mappings = parse_long_variable_names(
        envelope.element_size,
        &envelope.payload,
        envelope.encoding,
        envelope.element_size_position,
    )?;
    let record = ExtensionRecord::LongVariableNames(mappings);
    let record = DictionaryRecord::Extension(record);
    Ok(record)
}

/// Subtype 14 — very-long-string width declarations.
fn read_very_long_strings(envelope: &ExtensionEnvelope) -> Result<DictionaryRecord> {
    let declarations = parse_very_long_strings(
        envelope.element_size,
        &envelope.payload,
        envelope.encoding,
        envelope.element_size_position,
    )?;
    let record = ExtensionRecord::VeryLongStrings(declarations);
    let record = DictionaryRecord::Extension(record);
    Ok(record)
}

/// Subtype 17 — file-level custom attributes.
fn read_data_file_attributes(envelope: &ExtensionEnvelope) -> Result<DictionaryRecord> {
    let attributes = parse_data_file_attributes(
        envelope.element_size,
        &envelope.payload,
        envelope.encoding,
        envelope.element_size_position,
    )?;
    let record = ExtensionRecord::FileAttributes(attributes);
    let record = DictionaryRecord::Extension(record);
    Ok(record)
}

/// Subtype 18 — per-variable custom attributes.
fn read_variable_attributes(envelope: &ExtensionEnvelope) -> Result<DictionaryRecord> {
    let records = parse_variable_attributes(
        envelope.element_size,
        &envelope.payload,
        envelope.encoding,
        envelope.element_size_position,
    )?;
    let record = ExtensionRecord::VariableAttributes(records);
    let record = DictionaryRecord::Extension(record);
    Ok(record)
}

/// Subtype 21 — long string value labels.
fn read_long_string_value_labels(envelope: &ExtensionEnvelope) -> Result<DictionaryRecord> {
    let records = parse_long_string_value_labels(
        envelope.element_size,
        &envelope.payload,
        envelope.byte_order,
        envelope.encoding,
        envelope.element_size_position,
    )?;
    let record = ExtensionRecord::LongValueLabels(records);
    let record = DictionaryRecord::Extension(record);
    Ok(record)
}

/// Subtype 22 — long string missing values.
fn read_long_string_missing_values(envelope: &ExtensionEnvelope) -> Result<DictionaryRecord> {
    let records = parse_long_string_missing_values(
        envelope.element_size,
        &envelope.payload,
        envelope.byte_order,
        envelope.encoding,
        envelope.element_size_position,
    )?;
    let record = ExtensionRecord::LongMissingValues(records);
    let record = DictionaryRecord::Extension(record);
    Ok(record)
}

/// Subtype 11 — raw per-variable display parameters.
fn read_display_parameters(envelope: &ExtensionEnvelope) -> Result<DictionaryRecord> {
    let raw = parse_display_parameters(
        envelope.element_size,
        &envelope.payload,
        envelope.byte_order,
        envelope.element_size_position,
    )?;
    let record = ExtensionRecord::DisplayParameters(raw);
    let record = DictionaryRecord::Extension(record);
    Ok(record)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::spss::sav::byte_order::ByteOrder;
    use crate::spss::sav::dictionary_record::DictionaryRecord;
    use crate::spss::sav::extensions::extension_record::ExtensionRecord;
    use crate::spss::sav::raw_missing_values::RawMissingValues;
    use crate::spss::sav::sav_error;
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

    fn write_u32(buf: &mut Vec<u8>, byte_order: ByteOrder, value: u32) {
        match byte_order {
            ByteOrder::LittleEndian => buf.extend_from_slice(&value.to_le_bytes()),
            ByteOrder::BigEndian => buf.extend_from_slice(&value.to_be_bytes()),
        }
    }

    /// Appends one on-disk value-label entry — 8-byte value, 1-byte
    /// unpadded length, padded label bytes — using zero padding.
    fn write_value_label_entry(buf: &mut Vec<u8>, value: [u8; 8], label: &[u8]) {
        buf.extend_from_slice(&value);
        let unpadded_len = u8::try_from(label.len()).expect("test label fits in u8");
        buf.push(unpadded_len);
        buf.extend_from_slice(label);
        let used = 1 + label.len();
        let pad = (8 - (used % 8)) % 8;
        buf.extend_from_slice(&vec![0u8; pad]);
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

    /// Writes one numeric variable followed by a type-3 + type-4
    /// pair binding the value-label set to that variable.
    fn build_numeric_var_with_value_labels(
        byte_order: ByteOrder,
        labels: &[(f64, &[u8])],
        target_variable_indices: &[u32],
    ) -> Vec<u8> {
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
        write_rec_type(&mut bytes, byte_order, 3);
        write_u32(&mut bytes, byte_order, u32::try_from(labels.len()).unwrap());
        for (value, label) in labels {
            let value_bytes = match byte_order {
                ByteOrder::LittleEndian => value.to_le_bytes(),
                ByteOrder::BigEndian => value.to_be_bytes(),
            };
            write_value_label_entry(&mut bytes, value_bytes, label);
        }
        write_rec_type(&mut bytes, byte_order, 4);
        write_u32(
            &mut bytes,
            byte_order,
            u32::try_from(target_variable_indices.len()).unwrap(),
        );
        for &idx in target_variable_indices {
            write_u32(&mut bytes, byte_order, idx);
        }
        write_terminator(&mut bytes, byte_order);
        bytes
    }

    fn read_all(bytes: Vec<u8>) -> (Vec<DictionaryRecord>, Vec<SavWarning>) {
        let mut dict = open(bytes);
        let mut records = Vec::new();
        let mut warnings: Vec<SavWarning> = Vec::new();
        while let Some(record) = dict.read_record().unwrap() {
            records.push(record);
            warnings.extend(dict.warnings().iter().cloned());
        }
        warnings.extend(dict.warnings().iter().cloned());
        (records, warnings)
    }

    #[test]
    fn numeric_value_label_set_single_variable() {
        let byte_order = ByteOrder::LittleEndian;
        let bytes =
            build_numeric_var_with_value_labels(byte_order, &[(1.0, b"one"), (2.0, b"two")], &[1]);
        let (records, _) = read_all(bytes);
        assert_eq!(records.len(), 2);
        let DictionaryRecord::ValueLabelSet(set) = &records[1] else {
            panic!("expected ValueLabelSet, got {:?}", records[1]);
        };
        assert_eq!(set.entries().len(), 2);
        assert_eq!(set.entries()[0].value(), 1.0_f64.to_le_bytes());
        assert_eq!(set.entries()[0].label(), "one");
        assert_eq!(set.entries()[1].value(), 2.0_f64.to_le_bytes());
        assert_eq!(set.entries()[1].label(), "two");
        assert_eq!(set.variable_indices(), &[0]);
    }

    #[test]
    fn string_value_label_set_single_variable() {
        // String variable, width 4. Value keys are 8-byte padded
        // strings.
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 2);
        bytes.extend(build_variable_body(
            byte_order,
            4,
            0,
            0,
            pack_format(1, 4, 0),
            pack_format(1, 4, 0),
            *b"SEX     ",
        ));
        write_rec_type(&mut bytes, byte_order, 3);
        write_u32(&mut bytes, byte_order, 2);
        write_value_label_entry(&mut bytes, *b"M\0\0\0\0\0\0\0", b"Male");
        write_value_label_entry(&mut bytes, *b"F\0\0\0\0\0\0\0", b"Female");
        write_rec_type(&mut bytes, byte_order, 4);
        write_u32(&mut bytes, byte_order, 1);
        write_u32(&mut bytes, byte_order, 1);
        write_terminator(&mut bytes, byte_order);

        let (records, _) = read_all(bytes);
        let DictionaryRecord::ValueLabelSet(set) = &records[1] else {
            panic!("expected ValueLabelSet");
        };
        assert_eq!(set.entries()[0].value(), *b"M\0\0\0\0\0\0\0");
        assert_eq!(set.entries()[0].label(), "Male");
        assert_eq!(set.entries()[1].label(), "Female");
        assert_eq!(set.variable_indices(), &[0]);
    }

    #[test]
    fn value_label_set_multi_variable_type_4() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // Two numeric variables.
        for name in [*b"V1      ", *b"V2      "] {
            write_rec_type(&mut bytes, byte_order, 2);
            bytes.extend(build_variable_body(
                byte_order,
                0,
                0,
                0,
                pack_format(5, 8, 2),
                pack_format(5, 8, 2),
                name,
            ));
        }
        write_rec_type(&mut bytes, byte_order, 3);
        write_u32(&mut bytes, byte_order, 1);
        write_value_label_entry(&mut bytes, 1.0_f64.to_le_bytes(), b"one");
        write_rec_type(&mut bytes, byte_order, 4);
        write_u32(&mut bytes, byte_order, 2);
        write_u32(&mut bytes, byte_order, 1);
        write_u32(&mut bytes, byte_order, 2);
        write_terminator(&mut bytes, byte_order);

        let (records, _) = read_all(bytes);
        let DictionaryRecord::ValueLabelSet(set) = records.last().unwrap() else {
            panic!("expected ValueLabelSet last");
        };
        assert_eq!(set.variable_indices(), &[0, 1]);
    }

    #[test]
    fn type_4_after_long_string_indexes_logical_position() {
        // A width-32 string (4 physical records) followed by a
        // numeric variable. The numeric's 1-based physical position
        // is 5; it should normalize to 0-based logical 1.
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
            bytes.extend(build_variable_body(byte_order, -1, 0, 0, 0, 0, [0; 8]));
        }
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
        write_rec_type(&mut bytes, byte_order, 3);
        write_u32(&mut bytes, byte_order, 1);
        write_value_label_entry(&mut bytes, 1.0_f64.to_le_bytes(), b"one");
        write_rec_type(&mut bytes, byte_order, 4);
        write_u32(&mut bytes, byte_order, 1);
        write_u32(&mut bytes, byte_order, 5);
        write_terminator(&mut bytes, byte_order);

        let (records, _) = read_all(bytes);
        let DictionaryRecord::ValueLabelSet(set) = records.last().unwrap() else {
            panic!("expected ValueLabelSet");
        };
        assert_eq!(set.variable_indices(), &[1]);
    }

    #[test]
    fn stray_type_4_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 4);
        write_u32(&mut bytes, byte_order, 1);
        write_u32(&mut bytes, byte_order, 1);

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => {
                assert_eq!(
                    e.kind(),
                    FormatErrorKind::UnpairedValueLabelRecord { saw: 4 }
                );
            }
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn type_3_without_type_4_errors() {
        // A type-3 record followed by the terminator (999) instead
        // of a type-4. The reader must error.
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // A variable so the type-3 isn't structurally orphaned.
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
        write_rec_type(&mut bytes, byte_order, 3);
        write_u32(&mut bytes, byte_order, 1);
        write_value_label_entry(&mut bytes, 1.0_f64.to_le_bytes(), b"one");
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        dict.read_record().unwrap();
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => {
                assert_eq!(
                    e.kind(),
                    FormatErrorKind::UnpairedValueLabelRecord { saw: 999 },
                );
            }
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn type_4_with_dangling_index_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let bytes = build_numeric_var_with_value_labels(
            byte_order,
            &[(1.0, b"one")],
            // 2 is past the only variable.
            &[2],
        );
        let mut dict = open(bytes);
        dict.read_record().unwrap();
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(e.kind(), FormatErrorKind::DanglingValueLabel),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn type_4_with_zero_index_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let bytes = build_numeric_var_with_value_labels(
            byte_order,
            &[(1.0, b"one")],
            // 0 is invalid (indices are 1-based).
            &[0],
        );
        let mut dict = open(bytes);
        dict.read_record().unwrap();
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(e.kind(), FormatErrorKind::DanglingValueLabel),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn type_4_referencing_continuation_position_errors() {
        // Width-32 string => primary at physical 0, continuations at
        // 1, 2, 3. A type-4 index of 2 (1-based) lands on physical 1,
        // a continuation — must error.
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
            bytes.extend(build_variable_body(byte_order, -1, 0, 0, 0, 0, [0; 8]));
        }
        write_rec_type(&mut bytes, byte_order, 3);
        write_u32(&mut bytes, byte_order, 1);
        write_value_label_entry(&mut bytes, [0; 8], b"x");
        write_rec_type(&mut bytes, byte_order, 4);
        write_u32(&mut bytes, byte_order, 1);
        write_u32(&mut bytes, byte_order, 2);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        dict.read_record().unwrap();
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(e.kind(), FormatErrorKind::DanglingValueLabel),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn empty_type_4_warns() {
        let byte_order = ByteOrder::LittleEndian;
        let bytes = build_numeric_var_with_value_labels(byte_order, &[(1.0, b"one")], &[]);
        let mut dict = open(bytes);
        dict.read_record().unwrap();
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::ValueLabelSet(set) = record else {
            panic!("expected ValueLabelSet");
        };
        assert_eq!(set.variable_indices(), &[] as &[u32]);
        assert!(
            dict.warnings()
                .iter()
                .any(|w| matches!(w, SavWarning::EmptyValueLabelVariables))
        );
    }

    #[test]
    fn duplicate_variable_indices_preserved() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        for name in [*b"V1      ", *b"V2      ", *b"V3      "] {
            write_rec_type(&mut bytes, byte_order, 2);
            bytes.extend(build_variable_body(
                byte_order,
                0,
                0,
                0,
                pack_format(5, 8, 2),
                pack_format(5, 8, 2),
                name,
            ));
        }
        write_rec_type(&mut bytes, byte_order, 3);
        write_u32(&mut bytes, byte_order, 1);
        write_value_label_entry(&mut bytes, 1.0_f64.to_le_bytes(), b"one");
        write_rec_type(&mut bytes, byte_order, 4);
        write_u32(&mut bytes, byte_order, 3);
        write_u32(&mut bytes, byte_order, 1);
        write_u32(&mut bytes, byte_order, 3);
        write_u32(&mut bytes, byte_order, 1);
        write_terminator(&mut bytes, byte_order);

        let (records, _) = read_all(bytes);
        let DictionaryRecord::ValueLabelSet(set) = records.last().unwrap() else {
            panic!("expected ValueLabelSet");
        };
        assert_eq!(set.variable_indices(), &[0, 2, 0]);
    }

    #[test]
    fn duplicate_keys_preserved_and_warned() {
        let byte_order = ByteOrder::LittleEndian;
        let bytes = build_numeric_var_with_value_labels(
            byte_order,
            &[(5.0, b"first"), (5.0, b"second"), (5.0, b"third")],
            &[1],
        );
        let mut dict = open(bytes);
        dict.read_record().unwrap();
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::ValueLabelSet(set) = record else {
            panic!("expected ValueLabelSet");
        };
        assert_eq!(set.entries().len(), 3);
        let duplicates = dict
            .warnings()
            .iter()
            .filter(|w| matches!(w, SavWarning::DuplicateValueLabelKey { .. }))
            .count();
        // Two duplicates of the first occurrence.
        assert_eq!(duplicates, 2);
    }

    /// Appends one 80-byte document line, space-padding `text` up
    /// to the on-disk width.
    fn write_document_line(buf: &mut Vec<u8>, text: &[u8]) {
        assert!(text.len() <= 80, "test line exceeds 80 bytes");
        buf.extend_from_slice(text);
        buf.extend_from_slice(&vec![b' '; 80 - text.len()]);
    }

    #[test]
    fn document_record_single_line() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 6);
        write_u32(&mut bytes, byte_order, 1);
        write_document_line(&mut bytes, b"Hello, world!");
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Document(doc) = record else {
            panic!("expected Document, got {record:?}");
        };
        assert_eq!(doc.lines().len(), 1);
        // Trailing spaces are preserved verbatim.
        assert_eq!(
            doc.lines()[0],
            "Hello, world!                                                                   ",
        );
        assert_eq!(doc.lines()[0].len(), 80);
        assert!(dict.read_record().unwrap().is_none());
    }

    #[test]
    fn document_record_multiple_lines_in_order() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 6);
        write_u32(&mut bytes, byte_order, 3);
        write_document_line(&mut bytes, b"line one");
        write_document_line(&mut bytes, b"line two");
        write_document_line(&mut bytes, b"line three");
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Document(doc) = dict.read_record().unwrap().unwrap() else {
            panic!("expected Document");
        };
        assert_eq!(doc.lines().len(), 3);
        assert!(doc.lines()[0].starts_with("line one"));
        assert!(doc.lines()[1].starts_with("line two"));
        assert!(doc.lines()[2].starts_with("line three"));
    }

    #[test]
    fn document_record_empty_is_accepted_without_warning() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 6);
        write_u32(&mut bytes, byte_order, 0);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Document(doc) = dict.read_record().unwrap().unwrap() else {
            panic!("expected Document");
        };
        assert!(doc.lines().is_empty());
        assert!(dict.warnings().is_empty());
        assert!(dict.read_record().unwrap().is_none());
    }

    #[test]
    fn document_record_decoded_through_active_encoding() {
        // 0xE9 is `é` in Windows-1252 (the default header-fallback
        // encoding used by the test scaffolding) but invalid as
        // standalone UTF-8.
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 6);
        write_u32(&mut bytes, byte_order, 1);
        let mut line = vec![0xE9];
        line.extend_from_slice(b"clair");
        write_document_line(&mut bytes, &line);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Document(doc) = dict.read_record().unwrap().unwrap() else {
            panic!("expected Document");
        };
        assert!(doc.lines()[0].starts_with("éclair"));
    }

    #[test]
    fn document_record_yields_one_per_occurrence() {
        // Two type-6 records back-to-back. Each surfaces as its own
        // DictionaryRecord::Document; reconciling into a single
        // schema-level document is the finalizer's concern.
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 6);
        write_u32(&mut bytes, byte_order, 1);
        write_document_line(&mut bytes, b"first record");
        write_rec_type(&mut bytes, byte_order, 6);
        write_u32(&mut bytes, byte_order, 1);
        write_document_line(&mut bytes, b"second record");
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let first = dict.read_record().unwrap().unwrap();
        let second = dict.read_record().unwrap().unwrap();
        assert!(matches!(first, DictionaryRecord::Document(_)));
        assert!(matches!(second, DictionaryRecord::Document(_)));
        assert!(dict.read_record().unwrap().is_none());
    }

    #[test]
    fn document_record_interleaved_with_other_dictionary_records() {
        // Variable → document → value-label set, all under one
        // dictionary section. Confirms the dispatch handles a
        // type-6 record positioned between unrelated record kinds.
        let byte_order = ByteOrder::LittleEndian;
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
        write_rec_type(&mut bytes, byte_order, 6);
        write_u32(&mut bytes, byte_order, 1);
        write_document_line(&mut bytes, b"a note");
        write_rec_type(&mut bytes, byte_order, 3);
        write_u32(&mut bytes, byte_order, 1);
        write_value_label_entry(&mut bytes, 1.0_f64.to_le_bytes(), b"one");
        write_rec_type(&mut bytes, byte_order, 4);
        write_u32(&mut bytes, byte_order, 1);
        write_u32(&mut bytes, byte_order, 1);
        write_terminator(&mut bytes, byte_order);

        let (records, _) = read_all(bytes);
        assert_eq!(records.len(), 3);
        assert!(matches!(records[0], DictionaryRecord::Variable(_)));
        assert!(matches!(records[1], DictionaryRecord::Document(_)));
        assert!(matches!(records[2], DictionaryRecord::ValueLabelSet(_)));
    }

    /// Appends one type-7 extension record envelope plus payload.
    fn write_extension_record(
        buf: &mut Vec<u8>,
        byte_order: ByteOrder,
        subtype: i32,
        element_size: u32,
        element_count: u32,
        payload: &[u8],
    ) {
        write_rec_type(buf, byte_order, 7);
        match byte_order {
            ByteOrder::LittleEndian => buf.extend_from_slice(&subtype.to_le_bytes()),
            ByteOrder::BigEndian => buf.extend_from_slice(&subtype.to_be_bytes()),
        }
        write_u32(buf, byte_order, element_size);
        write_u32(buf, byte_order, element_count);
        buf.extend_from_slice(payload);
    }

    #[test]
    fn extension_record_unknown_subtype_round_trip() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x12, 0x34];
        write_extension_record(&mut bytes, byte_order, 9999, 2, 3, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let extension = match record {
            DictionaryRecord::Extension(ext) => ext,
            other => panic!("expected Extension, got {other:?}"),
        };
        let unknown = match extension {
            ExtensionRecord::Unknown(u) => u,
            other => panic!("expected Unknown, got {other:?}"),
        };
        assert_eq!(unknown.subtype(), 9999);
        assert_eq!(unknown.element_size(), 2);
        assert_eq!(unknown.element_count(), 3);
        assert_eq!(unknown.payload(), payload.as_slice());
    }

    #[test]
    fn extension_record_emits_unknown_subtype_warning() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 4321, 1, 1, &[0]);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        dict.read_record().unwrap();
        assert!(matches!(
            dict.warnings(),
            &[SavWarning::UnknownExtensionSubtype { subtype: 4321 }]
        ));
    }

    #[test]
    fn extension_record_empty_payload() {
        // element_count == 0 means zero-byte payload, which is valid.
        // Use a subtype outside the wired set so the Unknown fallback
        // applies regardless of which subtypes get implemented next.
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 12345, 4, 0, &[]);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::Unknown(unknown)) = record else {
            panic!("expected Unknown extension");
        };
        assert_eq!(unknown.element_count(), 0);
        assert!(unknown.payload().is_empty());
    }

    #[test]
    fn extension_record_interleaved_with_other_dictionary_records() {
        let byte_order = ByteOrder::LittleEndian;
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
        write_extension_record(&mut bytes, byte_order, 20, 1, 5, b"UTF-8");
        write_rec_type(&mut bytes, byte_order, 6);
        write_u32(&mut bytes, byte_order, 1);
        write_document_line(&mut bytes, b"note");
        write_terminator(&mut bytes, byte_order);

        let (records, _) = read_all(bytes);
        assert_eq!(records.len(), 3);
        assert!(matches!(records[0], DictionaryRecord::Variable(_)));
        assert!(matches!(
            records[1],
            DictionaryRecord::Extension(ExtensionRecord::CharacterEncoding(_))
        ));
        assert!(matches!(records[2], DictionaryRecord::Document(_)));
    }

    #[test]
    fn extension_record_big_endian() {
        // Use an unrecognized subtype, so the Unknown
        // fallback applies. (Subtype 5 used to be a safe choice but
        // now has its own strict-envelope dispatch arm.)
        let byte_order = ByteOrder::BigEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 12345, 1, 4, b"abcd");
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::Unknown(unknown)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected Unknown extension");
        };
        assert_eq!(unknown.subtype(), 12345);
        assert_eq!(unknown.element_size(), 1);
        assert_eq!(unknown.element_count(), 4);
        assert_eq!(unknown.payload(), b"abcd");
    }

    #[test]
    fn extension_dispatch_does_not_intercept_unknown_subtypes() {
        // Subtype 999 is still unknown; the subtype-3 dispatch
        // arm must not catch it.
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 999, 4, 2, &[1, 2, 3, 4, 5, 6, 7, 8]);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        assert!(matches!(
            record,
            DictionaryRecord::Extension(ExtensionRecord::Unknown(_))
        ));
        assert!(matches!(
            dict.warnings(),
            &[SavWarning::UnknownExtensionSubtype { subtype: 999 }]
        ));
    }

    #[test]
    fn extension_subtype_4_float_sentinels() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // Three known IEEE 754 bit patterns: a NaN (system missing
        // in IEEE files), -Inf (LOWEST), +Inf (HIGHEST).
        let sys = f64::from_bits(0xFFF8_0000_0000_0000);
        let high = f64::INFINITY;
        let low = f64::NEG_INFINITY;
        let mut payload = Vec::with_capacity(24);
        payload.extend_from_slice(&sys.to_le_bytes());
        payload.extend_from_slice(&high.to_le_bytes());
        payload.extend_from_slice(&low.to_le_bytes());
        write_extension_record(&mut bytes, byte_order, 4, 8, 3, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::FloatInfo(sentinels)) = record else {
            panic!("expected FloatInfo, got {record:?}");
        };
        assert_eq!(sentinels.system_missing(), sys.to_le_bytes());
        assert_eq!(sentinels.highest(), high.to_le_bytes());
        assert_eq!(sentinels.lowest(), low.to_le_bytes());
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn extension_subtype_4_preserves_arbitrary_bit_patterns() {
        // Use a non-canonical NaN to confirm the bytes are not
        // normalized through any IEEE decode path.
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let sys = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x7F];
        let high = [0xDE, 0xAD, 0xBE, 0xEF, 0xDE, 0xAD, 0xBE, 0xEF];
        let low = [0xFE, 0xED, 0xFA, 0xCE, 0xFE, 0xED, 0xFA, 0xCE];
        let mut payload = Vec::with_capacity(24);
        payload.extend_from_slice(&sys);
        payload.extend_from_slice(&high);
        payload.extend_from_slice(&low);
        write_extension_record(&mut bytes, byte_order, 4, 8, 3, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::FloatInfo(sentinels)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected FloatInfo");
        };
        assert_eq!(sentinels.system_missing(), sys);
        assert_eq!(sentinels.highest(), high);
        assert_eq!(sentinels.lowest(), low);
    }

    #[test]
    fn extension_subtype_4_carries_bytes_verbatim_regardless_of_byte_order() {
        // Even with big-endian header byte order, the sentinel
        // bytes are stored verbatim — no byte-swapping is applied
        // here because the float-format decode is the consumer's
        // concern.
        let byte_order = ByteOrder::BigEndian;
        let mut bytes = build_header(byte_order);
        let payload: [u8; 24] = std::array::from_fn(|i| u8::try_from(i).unwrap());
        write_extension_record(&mut bytes, byte_order, 4, 8, 3, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::FloatInfo(sentinels)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected FloatInfo");
        };
        assert_eq!(sentinels.system_missing(), [0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(sentinels.highest(), [8, 9, 10, 11, 12, 13, 14, 15]);
        assert_eq!(sentinels.lowest(), [16, 17, 18, 19, 20, 21, 22, 23]);
    }

    #[test]
    fn extension_subtype_4_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // Spec says element_size must be 8; pass 4 instead.
        write_extension_record(&mut bytes, byte_order, 4, 4, 3, &[0; 12]);

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: sav_error::Field::ExtensionElementSize,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn extension_subtype_4_wrong_element_count_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // Spec says element_count must be 3; pass 2 instead.
        write_extension_record(&mut bytes, byte_order, 4, 8, 2, &[0; 16]);

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: sav_error::Field::ExtensionElementCount,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    /// Builds a 32-byte subtype-3 payload from 8 i32 fields in the
    /// given byte order.
    fn build_machine_integer_info_payload(byte_order: ByteOrder, fields: [i32; 8]) -> Vec<u8> {
        let to_bytes = |v: i32| match byte_order {
            ByteOrder::LittleEndian => v.to_le_bytes(),
            ByteOrder::BigEndian => v.to_be_bytes(),
        };
        let mut buf = Vec::with_capacity(32);
        for value in fields {
            buf.extend_from_slice(&to_bytes(value));
        }
        buf
    }

    #[test]
    fn extension_subtype_3_machine_integer_info() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // version 25.0.0, machine code 720, IEEE/standard compression,
        // little-endian, character code 1252 (Windows-1252).
        let fields = [25, 0, 0, 720, 1, 1, 2, 1252];
        let payload = build_machine_integer_info_payload(byte_order, fields);
        write_extension_record(&mut bytes, byte_order, 3, 4, 8, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::MachineIntegerInfo(info)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected MachineIntegerInfo");
        };
        assert_eq!(info.version_major(), 25);
        assert_eq!(info.version_minor(), 0);
        assert_eq!(info.version_revision(), 0);
        assert_eq!(info.machine_code(), 720);
        assert_eq!(info.floating_point_representation(), 1);
        assert_eq!(info.compression_code(), 1);
        assert_eq!(info.endianness(), 2);
        assert_eq!(info.character_code(), 1252);
        assert_eq!(
            info.floating_point_representation_kind(),
            Some(crate::spss::sav::float_format::FloatFormat::Ieee754),
        );
        assert_eq!(info.endianness_kind(), Some(ByteOrder::LittleEndian));
        // Header byte order (LE) matches record (2 → LE), float format
        // matches (IEEE), so no cross-check warnings.
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn extension_subtype_3_big_endian_payload() {
        let byte_order = ByteOrder::BigEndian;
        let mut bytes = build_header(byte_order);
        let fields = [25, 0, 0, 720, 1, 1, 1, 1252];
        let payload = build_machine_integer_info_payload(byte_order, fields);
        write_extension_record(&mut bytes, byte_order, 3, 4, 8, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::MachineIntegerInfo(info)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected MachineIntegerInfo");
        };
        assert_eq!(info.machine_code(), 720);
        assert_eq!(info.endianness_kind(), Some(ByteOrder::BigEndian));
    }

    #[test]
    fn extension_subtype_3_unknown_codes_return_none_from_typed_accessors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // 99 for both floating-point representation and endianness —
        // neither maps onto a recognized enum variant.
        let fields = [25, 0, 0, 720, 99, 1, 99, 1252];
        let payload = build_machine_integer_info_payload(byte_order, fields);
        write_extension_record(&mut bytes, byte_order, 3, 4, 8, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::MachineIntegerInfo(info)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected MachineIntegerInfo");
        };
        assert_eq!(info.floating_point_representation(), 99);
        assert_eq!(info.endianness(), 99);
        assert!(info.floating_point_representation_kind().is_none());
        assert!(info.endianness_kind().is_none());
        // Unknown codes don't trigger the cross-check warnings —
        // there's nothing to compare against.
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn extension_subtype_3_byte_order_mismatch_warns() {
        // Header is little-endian, but the record claims big-endian
        // (code 1).
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let fields = [25, 0, 0, 720, 1, 1, 1, 1252];
        let payload = build_machine_integer_info_payload(byte_order, fields);
        write_extension_record(&mut bytes, byte_order, 3, 4, 8, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        dict.read_record().unwrap().unwrap();
        assert!(matches!(
            dict.warnings(),
            &[SavWarning::HeaderByteOrderMismatch { record_value: 1 }]
        ));
    }

    #[test]
    fn extension_subtype_3_float_format_mismatch_warns() {
        // Header is IEEE 754 (the default test fixture uses IEEE
        // bias 100.0); the record claims IBM HFP (code 2).
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let fields = [25, 0, 0, 720, 2, 1, 2, 1252];
        let payload = build_machine_integer_info_payload(byte_order, fields);
        write_extension_record(&mut bytes, byte_order, 3, 4, 8, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        dict.read_record().unwrap().unwrap();
        assert!(matches!(
            dict.warnings(),
            &[SavWarning::HeaderFloatFormatMismatch { record_value: 2 }]
        ));
    }

    #[test]
    fn extension_subtype_3_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // Spec says element_size must be 4; pass 8 instead.
        write_extension_record(&mut bytes, byte_order, 3, 8, 8, &[0; 64]);

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: sav_error::Field::ExtensionElementSize,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn extension_subtype_3_wrong_element_count_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // Spec says element_count must be 8; pass 4 instead.
        write_extension_record(&mut bytes, byte_order, 3, 4, 4, &[0; 16]);

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: sav_error::Field::ExtensionElementCount,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    /// Builds a 16-byte subtype-16 payload (version flag + case count).
    fn build_extended_number_of_cases_payload(
        byte_order: ByteOrder,
        version: i64,
        count: i64,
    ) -> Vec<u8> {
        let to_bytes = |v: i64| match byte_order {
            ByteOrder::LittleEndian => v.to_le_bytes(),
            ByteOrder::BigEndian => v.to_be_bytes(),
        };
        let version_bytes = to_bytes(version);
        let count_bytes = to_bytes(count);
        let mut buf = Vec::with_capacity(version_bytes.len() + count_bytes.len());
        buf.extend_from_slice(&version_bytes);
        buf.extend_from_slice(&count_bytes);
        buf
    }

    #[test]
    fn extension_subtype_16_extended_number_of_cases() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = build_extended_number_of_cases_payload(byte_order, 1, 1_234_567_890);
        write_extension_record(&mut bytes, byte_order, 16, 8, 2, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::ExtendedNumberOfCases(extended)) = record
        else {
            panic!("expected ExtendedNumberOfCases, got {record:?}");
        };
        assert_eq!(extended.version(), 1);
        assert_eq!(extended.count(), 1_234_567_890);
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn extension_subtype_16_big_endian() {
        let byte_order = ByteOrder::BigEndian;
        let mut bytes = build_header(byte_order);
        let payload = build_extended_number_of_cases_payload(byte_order, 1, -42);
        write_extension_record(&mut bytes, byte_order, 16, 8, 2, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::ExtendedNumberOfCases(extended)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected ExtendedNumberOfCases");
        };
        assert_eq!(extended.count(), -42);
    }

    #[test]
    fn extension_subtype_16_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // Spec says element_size must be 8; pass 4 instead.
        write_extension_record(&mut bytes, byte_order, 16, 4, 2, &[0; 8]);

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: sav_error::Field::ExtensionElementSize,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn extension_subtype_16_wrong_element_count_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // Spec says element_count must be 2; pass 1 instead.
        write_extension_record(&mut bytes, byte_order, 16, 8, 1, &[0; 8]);

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: sav_error::Field::ExtensionElementCount,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn extension_subtype_20_character_encoding_utf8() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 20, 1, 5, b"UTF-8");
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::CharacterEncoding(name)) = record else {
            panic!("expected CharacterEncoding, got {record:?}");
        };
        assert_eq!(name, "UTF-8");
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn extension_subtype_20_windows_1252() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 20, 1, 12, b"windows-1252");
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::CharacterEncoding(name)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected CharacterEncoding");
        };
        assert_eq!(name, "windows-1252");
    }

    #[test]
    fn extension_subtype_20_trims_trailing_padding() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 20, 1, 8, b"UTF-8\0  ");
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::CharacterEncoding(name)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected CharacterEncoding");
        };
        assert_eq!(name, "UTF-8");
    }

    #[test]
    fn extension_subtype_20_big_endian() {
        let byte_order = ByteOrder::BigEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 20, 1, 5, b"UTF-8");
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::CharacterEncoding(name)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected CharacterEncoding");
        };
        assert_eq!(name, "UTF-8");
    }

    #[test]
    fn extension_subtype_20_empty_payload_yields_empty_string() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 20, 1, 0, &[]);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::CharacterEncoding(name)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected CharacterEncoding");
        };
        assert!(name.is_empty());
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn extension_subtype_20_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // Spec says element_size must be 1; pass 4 instead.
        write_extension_record(&mut bytes, byte_order, 20, 4, 2, &[0; 8]);

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: sav_error::Field::ExtensionElementSize,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn extension_subtype_13_single_mapping() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"V1=Variable1";
        write_extension_record(
            &mut bytes,
            byte_order,
            13,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::LongVariableNames(mappings)) = record
        else {
            panic!("expected LongVariableNames, got {record:?}");
        };
        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].short_name(), "V1");
        assert_eq!(mappings[0].long_name(), "Variable1");
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn extension_subtype_13_multiple_mappings_in_order() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"V1=Alpha\tV2=Beta\tV3=Gamma";
        write_extension_record(
            &mut bytes,
            byte_order,
            13,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::LongVariableNames(mappings)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected LongVariableNames");
        };
        let names: Vec<(&str, &str)> = mappings
            .iter()
            .map(|m| (m.short_name(), m.long_name()))
            .collect();
        assert_eq!(
            names,
            vec![("V1", "Alpha"), ("V2", "Beta"), ("V3", "Gamma")]
        );
    }

    #[test]
    fn extension_subtype_13_optional_trailing_separator() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"V1=Alpha\tV2=Beta\t";
        write_extension_record(
            &mut bytes,
            byte_order,
            13,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::LongVariableNames(mappings)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected LongVariableNames");
        };
        assert_eq!(mappings.len(), 2);
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn extension_subtype_13_big_endian() {
        let byte_order = ByteOrder::BigEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"V1=Long";
        write_extension_record(
            &mut bytes,
            byte_order,
            13,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::LongVariableNames(mappings)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected LongVariableNames");
        };
        assert_eq!(mappings[0].long_name(), "Long");
    }

    #[test]
    fn extension_subtype_13_decoded_through_active_encoding() {
        // 0xE9 = é in Windows-1252 (the test scaffolding's default
        // header encoding), invalid as standalone UTF-8.
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let mut payload = Vec::new();
        payload.extend_from_slice(b"V1=caf\xE9");
        write_extension_record(
            &mut bytes,
            byte_order,
            13,
            1,
            u32::try_from(payload.len()).unwrap(),
            &payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::LongVariableNames(mappings)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected LongVariableNames");
        };
        assert_eq!(mappings[0].long_name(), "café");
    }

    #[test]
    fn extension_subtype_13_empty_payload_yields_empty_vec() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 13, 1, 0, &[]);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::LongVariableNames(mappings)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected LongVariableNames");
        };
        assert!(mappings.is_empty());
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn extension_subtype_13_missing_equals_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"V1=ok\tBADPAIR";
        write_extension_record(
            &mut bytes,
            byte_order,
            13,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: sav_error::Field::LongVariableNamePair,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn extension_subtype_13_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 13, 4, 2, &[0; 8]);

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: sav_error::Field::ExtensionElementSize,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn extension_subtype_14_single_declaration() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"RESPONSE=00226\0\t";
        write_extension_record(
            &mut bytes,
            byte_order,
            14,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::VeryLongStrings(declarations)) = record
        else {
            panic!("expected VeryLongStrings, got {record:?}");
        };
        assert_eq!(declarations.len(), 1);
        assert_eq!(declarations[0].short_name(), "RESPONSE");
        assert_eq!(declarations[0].width(), 226);
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn extension_subtype_14_multiple_declarations_in_order() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"V1=00300\0\tV2=01000\0\tV3=32767\0\t";
        write_extension_record(
            &mut bytes,
            byte_order,
            14,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::VeryLongStrings(declarations)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected VeryLongStrings");
        };
        let pairs: Vec<(&str, u32)> = declarations
            .iter()
            .map(|d| (d.short_name(), d.width()))
            .collect();
        assert_eq!(pairs, vec![("V1", 300), ("V2", 1000), ("V3", 32767)]);
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn extension_subtype_14_big_endian() {
        let byte_order = ByteOrder::BigEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"V1=00300\0\t";
        write_extension_record(
            &mut bytes,
            byte_order,
            14,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::VeryLongStrings(declarations)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected VeryLongStrings");
        };
        assert_eq!(declarations.len(), 1);
        assert_eq!(declarations[0].width(), 300);
    }

    #[test]
    fn extension_subtype_14_empty_payload_yields_empty_vec() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 14, 1, 0, &[]);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::VeryLongStrings(declarations)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected VeryLongStrings");
        };
        assert!(declarations.is_empty());
    }

    #[test]
    fn extension_subtype_14_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 14, 4, 2, &[0; 8]);

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: sav_error::Field::ExtensionElementSize,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn extension_subtype_14_malformed_pair_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"V1=300\tV2=abc";
        write_extension_record(
            &mut bytes,
            byte_order,
            14,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: sav_error::Field::VeryLongStringPair,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    /// Builds a subtype-11 payload from a flat `(measure, [width,]
    /// alignment)` u32 stream.
    fn build_display_parameters_payload(byte_order: ByteOrder, values: &[u32]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(values.len() * 4);
        for &v in values {
            match byte_order {
                ByteOrder::LittleEndian => buf.extend_from_slice(&v.to_le_bytes()),
                ByteOrder::BigEndian => buf.extend_from_slice(&v.to_be_bytes()),
            }
        }
        buf
    }

    #[test]
    fn extension_subtype_11_two_tuple_form() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // Two variables, 2-tuple form: (measure, alignment) each.
        let values = [1u32, 0, 3, 1];
        let payload = build_display_parameters_payload(byte_order, &values);
        write_extension_record(
            &mut bytes,
            byte_order,
            11,
            4,
            u32::try_from(values.len()).unwrap(),
            &payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::DisplayParameters(raw)) = record else {
            panic!("expected DisplayParameters, got {record:?}");
        };
        assert_eq!(raw.values(), &values);
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn extension_subtype_11_three_tuple_form() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // One variable, 3-tuple form: (measure, width, alignment).
        let values = [2u32, 12, 1];
        let payload = build_display_parameters_payload(byte_order, &values);
        write_extension_record(
            &mut bytes,
            byte_order,
            11,
            4,
            u32::try_from(values.len()).unwrap(),
            &payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::DisplayParameters(raw)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected DisplayParameters");
        };
        assert_eq!(raw.values(), &values);
    }

    #[test]
    fn extension_subtype_11_big_endian() {
        let byte_order = ByteOrder::BigEndian;
        let mut bytes = build_header(byte_order);
        let values = [1u32, 0, 3, 8, 1];
        let payload = build_display_parameters_payload(byte_order, &values);
        write_extension_record(
            &mut bytes,
            byte_order,
            11,
            4,
            u32::try_from(values.len()).unwrap(),
            &payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::DisplayParameters(raw)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected DisplayParameters");
        };
        assert_eq!(raw.values(), &values);
    }

    #[test]
    fn extension_subtype_11_empty_payload_yields_empty_values() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 11, 4, 0, &[]);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::DisplayParameters(raw)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected DisplayParameters");
        };
        assert!(raw.values().is_empty());
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn extension_subtype_11_preserves_unrecognized_codes() {
        // measure = 99 (not 0..=3), alignment = 7 (not 0..=2).
        // Streaming preserves them; finalization will bucket them
        // into MeasurementLevel::Unknown / Alignment::Unknown when
        // it produces typed VariableDisplay values.
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let values = [99u32, 7];
        let payload = build_display_parameters_payload(byte_order, &values);
        write_extension_record(
            &mut bytes,
            byte_order,
            11,
            4,
            u32::try_from(values.len()).unwrap(),
            &payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::DisplayParameters(raw)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected DisplayParameters");
        };
        assert_eq!(raw.values(), &values);
    }

    #[test]
    fn extension_subtype_11_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // Spec says element_size must be 4; pass 8 instead.
        write_extension_record(&mut bytes, byte_order, 11, 8, 2, &[0; 16]);

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: sav_error::Field::ExtensionElementSize,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn extension_subtype_17_data_file_attributes() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"owner('Alice'\n)version('3'\n)";
        write_extension_record(
            &mut bytes,
            byte_order,
            17,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::FileAttributes(attributes)) = record
        else {
            panic!("expected FileAttributes, got {record:?}");
        };
        assert_eq!(attributes.len(), 2);
        assert_eq!(attributes[0].name(), "owner");
        assert_eq!(attributes[0].values(), &["Alice".to_string()]);
        assert_eq!(attributes[1].name(), "version");
        assert_eq!(attributes[1].values(), &["3".to_string()]);
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn extension_subtype_17_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 17, 4, 2, &[0; 8]);

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: sav_error::Field::ExtensionElementSize,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn extension_subtype_18_variable_attributes() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"weight:$@Role('0'\n)/height:units('cm'\n)";
        write_extension_record(
            &mut bytes,
            byte_order,
            18,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::VariableAttributes(records)) = record
        else {
            panic!("expected VariableAttributes, got {record:?}");
        };
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].variable_name(), "weight");
        assert_eq!(records[0].attributes()[0].name(), "$@Role");
        assert_eq!(records[0].attributes()[0].values(), &["0".to_string()]);
        assert_eq!(records[1].variable_name(), "height");
        assert_eq!(records[1].attributes()[0].values(), &["cm".to_string()]);
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn extension_subtype_18_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 18, 4, 2, &[0; 8]);

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: sav_error::Field::ExtensionElementSize,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn extension_subtype_21_long_string_value_labels() {
        let byte_order = ByteOrder::LittleEndian;
        let mut payload = Vec::new();
        payload.extend_from_slice(&3u32.to_le_bytes()); // var name length
        payload.extend_from_slice(b"abc");
        payload.extend_from_slice(&20u32.to_le_bytes()); // width
        payload.extend_from_slice(&1u32.to_le_bytes()); // label count
        payload.extend_from_slice(&2u32.to_le_bytes()); // value length
        payload.extend_from_slice(b"hi");
        payload.extend_from_slice(&5u32.to_le_bytes()); // label length
        payload.extend_from_slice(b"Hello");

        let mut bytes = build_header(byte_order);
        write_extension_record(
            &mut bytes,
            byte_order,
            21,
            1,
            u32::try_from(payload.len()).unwrap(),
            &payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::LongValueLabels(records)) = record else {
            panic!("expected LongValueLabels, got {record:?}");
        };
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].variable_name(), "abc");
        assert_eq!(records[0].width(), 20);
        assert_eq!(records[0].labels()[0].value(), b"hi");
        assert_eq!(records[0].labels()[0].label(), "Hello");
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn extension_subtype_21_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 21, 4, 2, &[0; 8]);

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: sav_error::Field::ExtensionElementSize,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn extension_subtype_22_long_string_missing_values() {
        let byte_order = ByteOrder::LittleEndian;
        let mut payload = Vec::new();
        payload.extend_from_slice(&3u32.to_le_bytes()); // var name length
        payload.extend_from_slice(b"abc");
        payload.push(2); // n_missing
        payload.extend_from_slice(&4u32.to_le_bytes()); // value width
        payload.extend_from_slice(b"MISS");
        payload.extend_from_slice(b"GONE");

        let mut bytes = build_header(byte_order);
        write_extension_record(
            &mut bytes,
            byte_order,
            22,
            1,
            u32::try_from(payload.len()).unwrap(),
            &payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::LongMissingValues(records)) = record else {
            panic!("expected LongMissingValues, got {record:?}");
        };
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].variable_name(), "abc");
        assert_eq!(records[0].values(), &[b"MISS".to_vec(), b"GONE".to_vec()]);
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn extension_subtype_22_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 22, 4, 2, &[0; 8]);

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: sav_error::Field::ExtensionElementSize,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }
}
