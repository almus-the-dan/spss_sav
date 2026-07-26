//! Buffering the dictionary section so its text can be decoded later.
//!
//! A SAV file declares its encoding in extension records that sit at the
//! *end* of the dictionary, after every string they govern. The reader
//! therefore cannot decode as it streams: it walks the whole dictionary
//! first, holding each record undecoded, and only then resolves the
//! encoding and starts handing records out. Both PSPP and `ReadStat` do
//! the same thing — PSPP with undecoded intermediate records, `ReadStat`
//! with a seek-based first pass.
//!
//! Walking the records requires reading whatever sizes their trailing
//! blocks from, which means all structural validation happens here
//! rather than at hand-out time. That is deliberate: a record whose
//! structure does not parse means the reader has lost sync with the
//! record stream, so nothing after it can be trusted either.
//!
//! Per-subtype *payload* validation is the exception and stays at
//! hand-out time, because extension records are self-delimiting through
//! `element_size * element_count` — their contents are never needed to
//! find the next record.

use std::collections::HashSet;
use std::io::Read;

use crate::spss::sav::buffered_dictionary_record::BufferedDictionaryRecord;
use crate::spss::sav::buffered_document_record::BufferedDocumentRecord;
use crate::spss::sav::buffered_record_payload::BufferedRecordPayload;
use crate::spss::sav::buffered_value_label_entry::BufferedValueLabelEntry;
use crate::spss::sav::buffered_value_label_set::BufferedValueLabelSet;
use crate::spss::sav::buffered_variable_record::BufferedVariableRecord;
use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::dictionary_format::{
    DICTIONARY_TERMINATOR_FILLER_LEN, DOCUMENT_LINE_LEN, EXTENSION_SUBTYPE_CHARACTER_ENCODING,
    EXTENSION_SUBTYPE_MACHINE_INTEGER_INFO, MISSING_VALUE_ENTRY_LEN,
    RECORD_TYPE_DICTIONARY_TERMINATOR, RECORD_TYPE_DOCUMENT, RECORD_TYPE_EXTENSION,
    RECORD_TYPE_VALUE_LABEL, RECORD_TYPE_VALUE_LABEL_VARIABLES, RECORD_TYPE_VARIABLE,
    VALUE_LABEL_LABEL_LEN_FIELD_LEN, VALUE_LABEL_VALUE_LEN, VARIABLE_HAS_LABEL_OFFSET,
    VARIABLE_LABEL_PADDING, VARIABLE_MISSING_VALUE_COUNT_OFFSET, VARIABLE_PRINT_FORMAT_OFFSET,
    VARIABLE_RECORD_BODY_LEN, VARIABLE_SHORT_NAME_LEN, VARIABLE_SHORT_NAME_OFFSET,
    VARIABLE_TYPE_OFFSET, VARIABLE_WRITE_FORMAT_OFFSET,
};
use crate::spss::sav::dictionary_parse::{
    VariableTypeCode, compose_raw_missing_values, normalize_value_label_variable_indices,
    parse_has_label, parse_missing_value_count, parse_sav_format, parse_variable_type,
    value_label_entry_size,
};
use crate::spss::sav::extension_envelope::ExtensionEnvelope;
use crate::spss::sav::extensions::{character_encoding, machine_integer_info};
use crate::spss::sav::reader_state::{ReaderState, u32_as_usize};
use crate::spss::sav::sav_error::{Field, FormatErrorKind, Result, SavError, Section};
use crate::spss::sav::sav_warning::SavWarning;
use crate::spss::sav::variable_type::VariableType;

/// Every dictionary record of a SAV file, read off the wire but not yet
/// decoded, together with whatever the file declared about its encoding.
///
/// Records are handed out one at a time by
/// [`next_record`](Self::next_record), which empties each slot as it
/// goes so a decoded record's bytes are released rather than kept alive
/// alongside it.
#[derive(Debug)]
pub(crate) struct DictionaryBuffer {
    records: Vec<Option<BufferedDictionaryRecord>>,
    cursor: usize,
    declared_encoding_label: Option<String>,
    character_code: Option<i32>,
}

impl DictionaryBuffer {
    /// Reads the dictionary section, from the record following the
    /// header through the `999` terminator.
    ///
    /// # Errors
    ///
    /// Returns [`SavError::Io`] on read failures and
    /// [`SavError::Format`] for any record whose structure does not
    /// parse.
    pub fn read<R: Read>(state: &mut ReaderState<R>, byte_order: ByteOrder) -> Result<Self> {
        let mut scan = Scan {
            state,
            byte_order,
            pending_continuations: 0,
            primaries: Vec::new(),
            physical_variable_count: 0,
            variable_count: 0,
            records: Vec::new(),
            declared_encoding_label: None,
            character_code: None,
        };
        scan.run()?;
        let buffer = Self {
            records: scan.records,
            cursor: 0,
            declared_encoding_label: scan.declared_encoding_label,
            character_code: scan.character_code,
        };
        Ok(buffer)
    }

    /// The label from the character encoding record (subtype 20), if the
    /// file carried one that could be read.
    pub fn declared_encoding_label(&self) -> Option<&str> {
        self.declared_encoding_label.as_deref()
    }

    /// The `character_code` from the machine integer info record
    /// (subtype 3), if the file carried one.
    pub fn character_code(&self) -> Option<i32> {
        self.character_code
    }

    /// Takes the next buffered record, emptying its slot. Returns `None`
    /// once every record has been handed out.
    pub fn next_record(&mut self) -> Option<BufferedDictionaryRecord> {
        let slot = self.records.get_mut(self.cursor)?;
        self.cursor += 1;
        slot.take()
    }
}

/// The in-progress walk over the dictionary section.
///
/// Owns the sequential bookkeeping that validation depends on — the
/// pending continuation run and the physical-to-logical variable index
/// map — none of which outlives the walk.
struct Scan<'a, R> {
    state: &'a mut ReaderState<R>,
    byte_order: ByteOrder,
    /// Continuation records still expected for the most recent
    /// string-variable primary. Must reach `0` before any other record
    /// kind, including the terminator.
    pending_continuations: u32,
    /// 0-based physical positions of each primary variable record, in
    /// declaration order, for translating a type-4 record's 1-based
    /// physical indices into 0-based logical ones.
    primaries: Vec<u32>,
    /// Count of all type-2 records seen, primaries and continuations
    /// alike.
    physical_variable_count: u32,
    /// Count of primaries only, for warning attribution.
    variable_count: usize,
    records: Vec<Option<BufferedDictionaryRecord>>,
    declared_encoding_label: Option<String>,
    character_code: Option<i32>,
}

impl<R: Read> Scan<'_, R> {
    fn run(&mut self) -> Result<()> {
        loop {
            let warnings_before = self.state.warnings().len();
            let position = self.state.position();
            let record_type = self.state.read_i32(self.byte_order, Section::Dictionary)?;

            // Non-variable record kinds are not allowed while the
            // previous string variable's continuation run is still
            // pending. Variable records must read their body before
            // primary-versus-continuation is known, so they check
            // themselves in `read_variable_record`.
            if record_type != RECORD_TYPE_VARIABLE && self.pending_continuations > 0 {
                return Err(SavError::format(
                    Section::Dictionary,
                    position,
                    FormatErrorKind::MissingContinuationRecord {
                        expected_remaining: self.pending_continuations,
                    },
                ));
            }

            let payload = match record_type {
                RECORD_TYPE_VARIABLE => match self.read_variable_record(position)? {
                    Some(record) => BufferedRecordPayload::Variable(record),
                    // A continuation, collapsed into its primary.
                    None => continue,
                },
                RECORD_TYPE_DICTIONARY_TERMINATOR => {
                    self.state
                        .skip(DICTIONARY_TERMINATOR_FILLER_LEN, Section::Dictionary)?;
                    return Ok(());
                }
                RECORD_TYPE_VALUE_LABEL => {
                    BufferedRecordPayload::ValueLabelSet(self.read_value_label_record()?)
                }
                RECORD_TYPE_VALUE_LABEL_VARIABLES => {
                    return Err(SavError::format(
                        Section::Dictionary,
                        position,
                        FormatErrorKind::UnpairedValueLabelRecord {
                            saw: RECORD_TYPE_VALUE_LABEL_VARIABLES,
                        },
                    ));
                }
                RECORD_TYPE_DOCUMENT => {
                    BufferedRecordPayload::Document(self.read_document_record()?)
                }
                RECORD_TYPE_EXTENSION => {
                    BufferedRecordPayload::Extension(self.read_extension_record()?)
                }
                value => {
                    return Err(SavError::format(
                        Section::Dictionary,
                        position,
                        FormatErrorKind::UnknownRecordType { value },
                    ));
                }
            };

            // Split off only the warnings this record raised, so they
            // can be replayed when it is handed to the caller.
            let warnings = self.state.warnings_mut().split_off(warnings_before);
            let record = BufferedDictionaryRecord { payload, warnings };
            self.records.push(Some(record));
        }
    }

    /// Reads a type-2 variable record's body plus any trailing label and
    /// missing-value blocks. Returns `Ok(None)` for a continuation
    /// record, which is collapsed into the preceding primary.
    fn read_variable_record(&mut self, position: u64) -> Result<Option<BufferedVariableRecord>> {
        let body: [u8; VARIABLE_RECORD_BODY_LEN] = self.state.read_array(Section::Dictionary)?;

        let type_value = self
            .byte_order
            .read_i32(four_bytes(&body, VARIABLE_TYPE_OFFSET));
        let type_code = parse_variable_type(type_value, position)?;

        if matches!(type_code, VariableTypeCode::Continuation) {
            if self.pending_continuations == 0 {
                return Err(SavError::format(
                    Section::Dictionary,
                    position,
                    FormatErrorKind::UnexpectedContinuationRecord,
                ));
            }
            self.pending_continuations -= 1;
            self.physical_variable_count += 1;
            return Ok(None);
        }

        // A primary arriving mid-run means the previous string
        // variable's continuations were short.
        if self.pending_continuations > 0 {
            return Err(SavError::format(
                Section::Dictionary,
                position,
                FormatErrorKind::MissingContinuationRecord {
                    expected_remaining: self.pending_continuations,
                },
            ));
        }

        let has_label = self
            .byte_order
            .read_i32(four_bytes(&body, VARIABLE_HAS_LABEL_OFFSET));
        let has_label = parse_has_label(has_label);

        let missing_count = self
            .byte_order
            .read_i32(four_bytes(&body, VARIABLE_MISSING_VALUE_COUNT_OFFSET));
        if missing_count == -1 {
            let variable_index = u32::try_from(self.variable_count).unwrap_or(u32::MAX);
            self.state
                .warnings_mut()
                .push(SavWarning::InvalidMissingValueCount {
                    variable_index,
                    value: missing_count,
                });
        }
        let missing_count = parse_missing_value_count(missing_count, position)?;

        let print_packed = self
            .byte_order
            .read_u32(four_bytes(&body, VARIABLE_PRINT_FORMAT_OFFSET));
        let print_format = parse_sav_format(print_packed);

        let write_packed = self
            .byte_order
            .read_u32(four_bytes(&body, VARIABLE_WRITE_FORMAT_OFFSET));
        let write_format = parse_sav_format(write_packed);

        let short_name: [u8; VARIABLE_SHORT_NAME_LEN] = body
            [VARIABLE_SHORT_NAME_OFFSET..VARIABLE_SHORT_NAME_OFFSET + VARIABLE_SHORT_NAME_LEN]
            .try_into()
            .expect("short-name slice is exactly 8 bytes");

        let label = if has_label {
            Some(self.read_variable_label()?)
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

        // A numeric primary owns no continuations; a string primary of
        // width W needs `ceil(W/8) - 1` continuations on disk.
        self.pending_continuations = match type_code {
            VariableTypeCode::Numeric => 0,
            VariableTypeCode::String(width) => u32::from(width).div_ceil(8) - 1,
            VariableTypeCode::Continuation => unreachable!("handled above"),
        };

        self.primaries.push(self.physical_variable_count);
        self.physical_variable_count += 1;
        self.variable_count += 1;

        let record = BufferedVariableRecord {
            short_name,
            label,
            variable_type,
            missing_values,
            print_format,
            write_format,
        };
        Ok(Some(record))
    }

    /// Reads the 4-byte `label_len` field followed by the padded label
    /// bytes, returning just the unpadded prefix.
    fn read_variable_label(&mut self) -> Result<Vec<u8>> {
        let label_len = self.state.read_u32_as_usize(
            self.byte_order,
            Section::Dictionary,
            Field::VariableLabel,
        )?;
        let padded_len = label_len.div_ceil(VARIABLE_LABEL_PADDING) * VARIABLE_LABEL_PADDING;
        let bytes = self.state.read_exact(padded_len, Section::Dictionary)?;
        let label_bytes = bytes[..label_len].to_vec();
        Ok(label_bytes)
    }

    /// Reads a type-6 document record: a `u32` line count followed by
    /// that many fixed-width lines.
    fn read_document_record(&mut self) -> Result<BufferedDocumentRecord> {
        let line_count = self.state.read_u32_as_usize(
            self.byte_order,
            Section::Dictionary,
            Field::DocumentLine,
        )?;

        let mut lines: Vec<[u8; DOCUMENT_LINE_LEN]> = Vec::with_capacity(line_count);
        for _ in 0..line_count {
            let line = self.state.read_array(Section::Dictionary)?;
            lines.push(line);
        }
        let record = BufferedDocumentRecord { lines };
        Ok(record)
    }

    /// Reads a type-3 value-label record and the type-4 record that must
    /// immediately follow it, as one unit.
    fn read_value_label_record(&mut self) -> Result<BufferedValueLabelSet> {
        let label_count = self.state.read_u32_as_usize(
            self.byte_order,
            Section::Dictionary,
            Field::ValueLabelEntry,
        )?;
        let entries = self.read_value_label_entries(label_count)?;

        // The very next record-type tag must be a type-4. Anything else
        // — including EOF, the dictionary terminator, or any other
        // record kind — is an unpaired-type-3 violation.
        let pair_position = self.state.position();
        let next_record_type = self.state.read_i32(self.byte_order, Section::Dictionary)?;
        if next_record_type != RECORD_TYPE_VALUE_LABEL_VARIABLES {
            return Err(SavError::format(
                Section::Dictionary,
                pair_position,
                FormatErrorKind::UnpairedValueLabelRecord {
                    saw: next_record_type,
                },
            ));
        }

        let variable_count = self.state.read_u32_as_usize(
            self.byte_order,
            Section::Dictionary,
            Field::VariableCount,
        )?;
        let indices_position = self.state.position();
        let raw_indices = self.read_variable_indexes(variable_count)?;

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

        let set = BufferedValueLabelSet {
            entries,
            variable_indices,
        };
        Ok(set)
    }

    fn read_value_label_entries(
        &mut self,
        label_count: usize,
    ) -> Result<Vec<BufferedValueLabelEntry>> {
        let mut entries: Vec<BufferedValueLabelEntry> = Vec::with_capacity(label_count);
        let mut seen_keys: HashSet<[u8; VALUE_LABEL_VALUE_LEN]> =
            HashSet::with_capacity(label_count);
        for _ in 0..label_count {
            let value: [u8; VALUE_LABEL_VALUE_LEN] = self.state.read_array(Section::Dictionary)?;
            let unpadded_len = self.state.read_u8(Section::Dictionary)?;
            let padded_len = value_label_entry_size(unpadded_len)
                - VALUE_LABEL_VALUE_LEN
                - VALUE_LABEL_LABEL_LEN_FIELD_LEN;
            let padded = self.state.read_exact(padded_len, Section::Dictionary)?;
            let label = padded[..usize::from(unpadded_len)].to_vec();

            if !seen_keys.insert(value) {
                self.state
                    .warnings_mut()
                    .push(SavWarning::DuplicateValueLabelKey { key: value });
            }
            let entry = BufferedValueLabelEntry { value, label };
            entries.push(entry);
        }
        Ok(entries)
    }

    fn read_variable_indexes(&mut self, variable_count: usize) -> Result<Vec<u32>> {
        let mut raw_indices: Vec<u32> = Vec::with_capacity(variable_count);
        for _ in 0..variable_count {
            let index = self.state.read_u32(self.byte_order, Section::Dictionary)?;
            raw_indices.push(index);
        }
        Ok(raw_indices)
    }

    /// Reads a type-7 extension record's envelope and payload.
    ///
    /// Also peeks at the two subtypes that declare the file's encoding,
    /// since resolution has to happen before any record is decoded.
    /// Neither peek validates: an unreadable declaration simply does not
    /// contribute to resolution, and the record's own `read` helper
    /// raises the complaint when it is handed to the caller.
    fn read_extension_record(&mut self) -> Result<ExtensionEnvelope> {
        let envelope = self.read_extension_envelope()?;
        match envelope.subtype {
            EXTENSION_SUBTYPE_CHARACTER_ENCODING => {
                self.declared_encoding_label = character_encoding::declared_label(&envelope).ok();
            }
            EXTENSION_SUBTYPE_MACHINE_INTEGER_INFO => {
                self.character_code = machine_integer_info::character_code(&envelope);
            }
            _ => {}
        }
        Ok(envelope)
    }

    /// Reads the envelope fields (`subtype`, `element_size`,
    /// `element_count`) and the declared payload.
    fn read_extension_envelope(&mut self) -> Result<ExtensionEnvelope> {
        let subtype = self.state.read_i32(self.byte_order, Section::Dictionary)?;
        let element_size_position = self.state.position();
        let element_size = self.state.read_u32(self.byte_order, Section::Dictionary)?;
        let element_count_position = self.state.position();
        let element_count = self.state.read_u32(self.byte_order, Section::Dictionary)?;
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
            byte_order: self.byte_order,
        };
        Ok(envelope)
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
        let payload_bytes = self.state.read_exact(payload_len, Section::Dictionary)?;
        let payload = payload_bytes.to_vec();
        Ok(payload)
    }
}

fn four_bytes(body: &[u8], offset: usize) -> [u8; 4] {
    body[offset..offset + 4]
        .try_into()
        .expect("four-byte slice has the requested length")
}
