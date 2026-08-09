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
    DICTIONARY_TERMINATOR_FILLER_LEN, DOCUMENT_LINE_LEN, MISSING_VALUE_ENTRY_LEN,
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
use crate::spss::sav::dictionary_record_kind::DictionaryRecordKind;
use crate::spss::sav::extension_envelope::ExtensionEnvelope;
use crate::spss::sav::extensions::extension_subtype::ExtensionSubtype;
use crate::spss::sav::extensions::{character_encoding, machine_integer_info};
use crate::spss::sav::raw_missing_values::RawMissingValues;
use crate::spss::sav::reader_options::ReaderOptions;
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
    skeleton: LayoutSkeleton,
}

/// The bare minimum needed to reconstruct the data layout, kept aside
/// from the records themselves.
///
/// Retained unconditionally, before any skip decision, so the layout can
/// be derived at finalization no matter what the caller did with the
/// records — pulled them, skipped them, filtered them out up front, or
/// never touched the reader at all. That turns "filtering can never
/// break a data read" from a rule someone has to remember into
/// something the types make true.
///
/// It costs a short name, a type and a raw missing-value slot per
/// variable, plus a copy of at most five tiny extension records.
#[derive(Debug, Default)]
pub(crate) struct LayoutSkeleton {
    /// One entry per type-2 primary, in order.
    variables: Vec<SkeletonVariable>,
    /// Copies of the extension records the layout depends on —
    /// subtypes 4, 13, 14, 16 and 22. Subtypes 3 and 20 are absorbed
    /// during the scan itself and need no copy.
    envelopes: Vec<ExtensionEnvelope>,
    /// Physical position of each segment's type-2 record, continuations
    /// included. The header's weight index is a physical one, so
    /// resolving it needs this map.
    primaries: Vec<u32>,
}

/// What one type-2 primary record contributes to the data layout.
///
/// The short name stays undecoded because the encoding is not resolved
/// until the dictionary ends. The missing values stay raw because
/// interpreting them needs the variable's logical type, which very long
/// strings only settle once subtype 14 is in hand.
#[derive(Debug, Clone)]
pub(crate) struct SkeletonVariable {
    pub short_name: [u8; VARIABLE_SHORT_NAME_LEN],
    pub variable_type: VariableType,
    pub missing_values: RawMissingValues,
}

impl LayoutSkeleton {
    /// One entry per type-2 primary, in order.
    pub fn variables(&self) -> &[SkeletonVariable] {
        &self.variables
    }

    /// The layout-bearing extension records, in the order they appeared.
    pub fn envelopes(&self) -> &[ExtensionEnvelope] {
        &self.envelopes
    }

    /// The segment index of the variable record at 0-based physical
    /// position `physical`, or `None` when that position holds a
    /// continuation record or lies past the end.
    pub fn segment_of_physical(&self, physical: u32) -> Option<usize> {
        self.primaries.binary_search(&physical).ok()
    }
}

/// Whether the layout depends on this subtype, and so whether the
/// skeleton needs its own copy.
///
/// Subtypes 3 and 20 are load-bearing too, but for the *encoding*, and
/// the scan absorbs those directly rather than deferring them.
///
/// Subtypes 13 and 22 are here because a row must not report a
/// declared-missing cell as present. Subtype 22 carries the missing
/// values of every very long string, and keys them by **long** name —
/// so resolving it needs subtype 13's short-to-long map as well. (14
/// keys by short name; the two extensions disagree, which is measurable
/// on any PSPP file carrying both.)
fn is_layout_bearing(subtype: ExtensionSubtype) -> bool {
    matches!(
        subtype,
        ExtensionSubtype::FloatInfo
            | ExtensionSubtype::LongVariableNames
            | ExtensionSubtype::VeryLongStrings
            | ExtensionSubtype::ExtendedNumberOfCases
            | ExtensionSubtype::LongMissingValues
    )
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
    pub fn read<R: Read>(
        state: &mut ReaderState<R>,
        byte_order: ByteOrder,
        options: &ReaderOptions,
    ) -> Result<Self> {
        let mut scan = Scan {
            state,
            byte_order,
            options,
            pending_continuations: 0,
            primaries: Vec::new(),
            physical_variable_count: 0,
            variable_count: 0,
            records: Vec::new(),
            declared_encoding_label: None,
            character_code: None,
            skeleton: LayoutSkeleton::default(),
        };
        scan.run()?;
        let buffer = Self {
            records: scan.records,
            cursor: 0,
            declared_encoding_label: scan.declared_encoding_label,
            character_code: scan.character_code,
            skeleton: LayoutSkeleton {
                primaries: scan.primaries,
                ..scan.skeleton
            },
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

    /// Everything the data layout is derived from, retained
    /// independently of the records themselves.
    pub fn skeleton(&self) -> &LayoutSkeleton {
        &self.skeleton
    }

    /// The kind of the record [`next_record`](Self::next_record) would
    /// return, without taking it.
    pub fn peek_kind(&self) -> Option<DictionaryRecordKind> {
        let slot = self.records.get(self.cursor)?;
        let record = slot.as_ref()?;
        let kind = record.payload.kind();
        Some(kind)
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
    /// Which content the caller asked not to be retained. Consulted
    /// before each record's bulk is read, so a skipped record's bytes
    /// are discarded through a bounded window rather than allocated.
    options: &'a ReaderOptions,
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
    skeleton: LayoutSkeleton,
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
                    Some(record) => Some(BufferedRecordPayload::Variable(record)),
                    // A continuation, collapsed into its primary.
                    None => continue,
                },
                RECORD_TYPE_DICTIONARY_TERMINATOR => {
                    self.state
                        .skip(DICTIONARY_TERMINATOR_FILLER_LEN, Section::Dictionary)?;
                    return Ok(());
                }
                RECORD_TYPE_VALUE_LABEL => {
                    let skipped = self.options.skips(DictionaryRecordKind::ValueLabelSet);
                    let record = self.read_value_label_record(skipped)?;
                    record.map(BufferedRecordPayload::ValueLabelSet)
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
                    let skipped = self.options.skips(DictionaryRecordKind::Document);
                    let record = self.read_document_record(skipped)?;
                    record.map(BufferedRecordPayload::Document)
                }
                RECORD_TYPE_EXTENSION => {
                    let record = self.read_extension_record()?;
                    record.map(BufferedRecordPayload::Extension)
                }
                value => {
                    return Err(SavError::format(
                        Section::Dictionary,
                        position,
                        FormatErrorKind::UnknownRecordType { value },
                    ));
                }
            };

            let Some(payload) = payload else {
                // Skipped: nothing will be handed out, so nothing should
                // report warnings about it either.
                self.state.warnings_mut().truncate(warnings_before);
                continue;
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
        let variable = SkeletonVariable {
            short_name,
            variable_type,
            missing_values: missing_values.clone(),
        };
        self.skeleton.variables.push(variable);

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
        let mut label_bytes = self.state.read_vec(padded_len, Section::Dictionary)?;
        label_bytes.truncate(label_len);
        Ok(label_bytes)
    }

    /// Reads a type-6 document record: a `u32` line count followed by
    /// that many fixed-width lines.
    ///
    /// Fully skippable once the line count is known — the lines are
    /// fixed-width, so nothing after this record depends on reading
    /// them. Returns `Ok(None)` when skipped.
    fn read_document_record(&mut self, skipped: bool) -> Result<Option<BufferedDocumentRecord>> {
        let line_count = self.state.read_u32_as_usize(
            self.byte_order,
            Section::Dictionary,
            Field::DocumentLine,
        )?;

        if skipped {
            let len = line_count.checked_mul(DOCUMENT_LINE_LEN).ok_or_else(|| {
                SavError::format(
                    Section::Dictionary,
                    self.state.position(),
                    FormatErrorKind::FieldTooLarge {
                        field: Field::DocumentLine,
                    },
                )
            })?;
            self.state.skip(len, Section::Dictionary)?;
            return Ok(None);
        }

        let mut lines: Vec<[u8; DOCUMENT_LINE_LEN]> = Vec::with_capacity(line_count);
        for _ in 0..line_count {
            let line = self.state.read_array(Section::Dictionary)?;
            lines.push(line);
        }
        let record = BufferedDocumentRecord { lines };
        Ok(Some(record))
    }

    /// Reads a type-3 value-label record and the type-4 record that must
    /// immediately follow it, as one unit.
    ///
    /// Only partly skippable: the entries are length-prefixed, so they
    /// have to be walked to find the type-4 record that follows, and the
    /// type-4 pairing and index normalization run either way. What
    /// skipping drops is retaining the labels. Returns `Ok(None)` when
    /// skipped.
    fn read_value_label_record(&mut self, skipped: bool) -> Result<Option<BufferedValueLabelSet>> {
        let label_count = self.state.read_u32_as_usize(
            self.byte_order,
            Section::Dictionary,
            Field::ValueLabelEntry,
        )?;
        let entries = self.read_value_label_entries(label_count, skipped)?;

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

        let segment_indices = normalize_value_label_variable_indices(
            &raw_indices,
            &self.primaries,
            indices_position,
        )?;

        if skipped {
            return Ok(None);
        }

        let set = BufferedValueLabelSet {
            entries,
            segment_indices,
        };
        Ok(Some(set))
    }

    /// Walks `label_count` on-disk entries. When `skipped`, the walk
    /// still happens. The entries are length-prefixed, so it is the
    /// only way to reach the record that follows. Nothing is
    /// retained and the duplicate-key check is left off, since its
    /// warning would be suppressed anyway.
    fn read_value_label_entries(
        &mut self,
        label_count: usize,
        skipped: bool,
    ) -> Result<Vec<BufferedValueLabelEntry>> {
        let mut entries: Vec<BufferedValueLabelEntry> = if skipped {
            Vec::new()
        } else {
            Vec::with_capacity(label_count)
        };
        let mut seen_keys: HashSet<[u8; VALUE_LABEL_VALUE_LEN]> =
            HashSet::with_capacity(if skipped { 0 } else { label_count });
        for _ in 0..label_count {
            let value: [u8; VALUE_LABEL_VALUE_LEN] = self.state.read_array(Section::Dictionary)?;
            let unpadded_len = self.state.read_u8(Section::Dictionary)?;
            let padded_len = value_label_entry_size(unpadded_len)
                - VALUE_LABEL_VALUE_LEN
                - VALUE_LABEL_LABEL_LEN_FIELD_LEN;
            if skipped {
                self.state.skip(padded_len, Section::Dictionary)?;
                continue;
            }
            let mut label = self.state.read_vec(padded_len, Section::Dictionary)?;
            label.truncate(usize::from(unpadded_len));

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
    /// Returns `Ok(None)` when the caller asked for this subtype to be
    /// skipped, in which case the payload bytes are discarded rather
    /// than allocated.
    ///
    /// The two encoding-declaring subtypes are absorbed first regardless.
    /// They are read here rather than merely yielded, so honoring a skip
    /// before the peek would change which encoding the whole file
    /// decodes with — exactly what skipping must never do. Both are a
    /// few dozen bytes, so reading them costs nothing.
    fn read_extension_record(&mut self) -> Result<Option<ExtensionEnvelope>> {
        let header = self.read_extension_header()?;
        let subtype = ExtensionSubtype::from_code(header.subtype);
        let declares_encoding = matches!(
            subtype,
            ExtensionSubtype::CharacterEncoding | ExtensionSubtype::MachineIntegerInfo
        );
        let must_absorb = declares_encoding || is_layout_bearing(subtype);

        if !must_absorb && self.options.skips(DictionaryRecordKind::Extension(subtype)) {
            self.state.skip(header.payload_len, Section::Dictionary)?;
            return Ok(None);
        }

        let envelope = self.read_extension_payload(header)?;
        if is_layout_bearing(subtype) {
            self.skeleton.envelopes.push(envelope.clone());
        }
        match subtype {
            ExtensionSubtype::CharacterEncoding => {
                self.declared_encoding_label = character_encoding::declared_label(&envelope).ok();
            }
            ExtensionSubtype::MachineIntegerInfo => {
                self.character_code = machine_integer_info::character_code(&envelope);
            }
            _ => {}
        }
        if must_absorb && self.options.skips(DictionaryRecordKind::Extension(subtype)) {
            return Ok(None);
        }
        Ok(Some(envelope))
    }

    /// Reads the envelope fields (`subtype`, `element_size`,
    /// `element_count`) that precede a type-7 payload, and derives the
    /// payload's byte length from them.
    ///
    /// Separate from reading the payload so the subtype is known — and
    /// a skip decision reachable — before any payload bytes are
    /// allocated.
    fn read_extension_header(&mut self) -> Result<ExtensionHeader> {
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
        let header = ExtensionHeader {
            subtype,
            element_size,
            element_count,
            element_size_usize,
            element_count_usize,
            element_size_position,
            payload_len,
        };
        Ok(header)
    }

    /// Reads the payload `header` declared and completes the envelope.
    fn read_extension_payload(&mut self, header: ExtensionHeader) -> Result<ExtensionEnvelope> {
        let payload = self
            .state
            .read_vec(header.payload_len, Section::Dictionary)?;
        let envelope = ExtensionEnvelope {
            subtype: header.subtype,
            element_size: header.element_size,
            element_count: header.element_count,
            element_size_usize: header.element_size_usize,
            element_count_usize: header.element_count_usize,
            element_size_position: header.element_size_position,
            payload,
            byte_order: self.byte_order,
        };
        Ok(envelope)
    }
}

/// A type-7 record's envelope fields, read before its payload.
///
/// Exists so the subtype is in hand while the payload is still on the
/// wire — the point at which a skip can avoid an allocation rather than
/// merely discard one.
#[derive(Clone, Copy)]
struct ExtensionHeader {
    subtype: i32,
    element_size: u32,
    element_count: u32,
    element_size_usize: usize,
    element_count_usize: usize,
    element_size_position: u64,
    /// `element_size * element_count`, already overflow-checked.
    payload_len: usize,
}

fn four_bytes(body: &[u8], offset: usize) -> [u8; 4] {
    body[offset..offset + 4]
        .try_into()
        .expect("four-byte slice has the requested length")
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::DOCUMENT_LINE_LEN;
    use crate::spss::sav::byte_order::ByteOrder;
    use crate::spss::sav::dictionary_reader::DictionaryReader;
    use crate::spss::sav::dictionary_record_kind::DictionaryRecordKind;
    use crate::spss::sav::encoding_provenance::EncodingProvenance;
    use crate::spss::sav::extensions::extension_subtype::ExtensionSubtype;
    use crate::spss::sav::sav_error::{FormatErrorKind, SavError};
    use crate::spss::sav::sav_warning::SavWarning;
    use crate::spss::sav::skippable_content::SkippableContent;
    use crate::spss::sav::test_support::{
        build_header, open, open_skipping, try_open_skipping, write_character_code_record,
        write_extension_record, write_numeric_variable, write_rec_type, write_terminator,
        write_u32,
    };
    use crate::spss::sav::value_label_set::ValueLabelSet;

    const LE: ByteOrder = ByteOrder::LittleEndian;

    /// Packs a `(kind_byte, width, decimals)` triple into the on-disk
    /// 4-byte format code.
    fn write_value_label_pair(buf: &mut Vec<u8>, labels: &[(f64, &[u8])], targets: &[u32]) {
        write_rec_type(buf, LE, 3);
        write_u32(buf, LE, u32::try_from(labels.len()).unwrap());
        for (value, label) in labels {
            buf.extend_from_slice(&value.to_le_bytes());
            buf.push(u8::try_from(label.len()).unwrap());
            buf.extend_from_slice(label);
            let pad = (8 - ((1 + label.len()) % 8)) % 8;
            buf.extend_from_slice(&vec![0u8; pad]);
        }
        write_rec_type(buf, LE, 4);
        write_u32(buf, LE, u32::try_from(targets.len()).unwrap());
        for &target in targets {
            write_u32(buf, LE, target);
        }
    }

    fn write_document(buf: &mut Vec<u8>, lines: &[&str]) {
        write_rec_type(buf, LE, 6);
        write_u32(buf, LE, u32::try_from(lines.len()).unwrap());
        for line in lines {
            let mut padded = [b' '; DOCUMENT_LINE_LEN];
            padded[..line.len()].copy_from_slice(line.as_bytes());
            buf.extend_from_slice(&padded);
        }
    }

    /// A file with one numeric variable, a value-label pair, a document
    /// record, and a UUID extension — one of every skippable kind.
    fn one_of_each() -> Vec<u8> {
        let mut bytes = build_header(LE);
        write_numeric_variable(&mut bytes, LE, *b"V1      ");
        write_value_label_pair(&mut bytes, &[(1.0, b"one")], &[1]);
        write_document(&mut bytes, &["a documentary line"]);
        let uuid = b"3f2504e0-4f89-11d3-9a0c-0305e82c3301";
        write_extension_record(
            &mut bytes,
            LE,
            12,
            1,
            u32::try_from(uuid.len()).unwrap(),
            uuid,
        );
        write_terminator(&mut bytes, LE);
        bytes
    }

    fn kinds(reader: &mut DictionaryReader<Cursor<Vec<u8>>>) -> Vec<DictionaryRecordKind> {
        let mut out = Vec::new();
        while let Some(record) = reader.read_record().unwrap() {
            out.push(record.kind());
        }
        out
    }

    #[test]
    fn nothing_is_skipped_by_default() {
        let mut reader = open(one_of_each());
        assert_eq!(
            kinds(&mut reader),
            vec![
                DictionaryRecordKind::Variable,
                DictionaryRecordKind::ValueLabelSet,
                DictionaryRecordKind::Document,
                DictionaryRecordKind::Extension(ExtensionSubtype::Uuid),
            ],
        );
    }

    #[test]
    fn skipping_documents_drops_only_documents() {
        let mut reader = open_skipping(one_of_each(), &[SkippableContent::Documents]);
        assert_eq!(
            kinds(&mut reader),
            vec![
                DictionaryRecordKind::Variable,
                DictionaryRecordKind::ValueLabelSet,
                DictionaryRecordKind::Extension(ExtensionSubtype::Uuid),
            ],
        );
    }

    #[test]
    fn skipping_value_labels_drops_only_value_labels() {
        let mut reader = open_skipping(one_of_each(), &[SkippableContent::ValueLabels]);
        assert_eq!(
            kinds(&mut reader),
            vec![
                DictionaryRecordKind::Variable,
                DictionaryRecordKind::Document,
                DictionaryRecordKind::Extension(ExtensionSubtype::Uuid),
            ],
        );
    }

    #[test]
    fn skipping_one_extension_subtype_leaves_the_others() {
        let mut reader = open_skipping(
            one_of_each(),
            &[SkippableContent::Extension(ExtensionSubtype::Uuid)],
        );
        assert_eq!(
            kinds(&mut reader),
            vec![
                DictionaryRecordKind::Variable,
                DictionaryRecordKind::ValueLabelSet,
                DictionaryRecordKind::Document,
            ],
        );
    }

    /// Variable records have no `SkippableContent` spelling, so the only
    /// thing to assert is that skipping everything else leaves them.
    #[test]
    fn variables_survive_skipping_everything_else() {
        let mut reader = open_skipping(
            one_of_each(),
            &[
                SkippableContent::ValueLabels,
                SkippableContent::Documents,
                SkippableContent::Extension(ExtensionSubtype::Uuid),
            ],
        );
        assert_eq!(kinds(&mut reader), vec![DictionaryRecordKind::Variable]);
    }

    /// Invariant 1: structural validation still runs for skipped
    /// records. A type-4 index past the end of the dictionary errors
    /// whether or not value labels are being retained.
    #[test]
    fn skipped_value_labels_still_validate_their_indices() {
        let mut bytes = build_header(LE);
        write_numeric_variable(&mut bytes, LE, *b"V1      ");
        // Index 2 is past the only variable.
        write_value_label_pair(&mut bytes, &[(1.0, b"one")], &[2]);
        write_terminator(&mut bytes, LE);

        let err = try_open_skipping(bytes, &[SkippableContent::ValueLabels])
            .expect_err("dangling index must still error");
        match err {
            SavError::Format(e) => assert_eq!(e.kind(), FormatErrorKind::DanglingValueLabel),
            other => panic!("expected Format error, got {other:?}"),
        }
    }

    /// Invariant 2: the encoding-declaring subtypes are absorbed even
    /// when the caller asked to skip them, so the file still decodes
    /// with the encoding it declared rather than the fallback.
    #[test]
    fn skipping_the_character_code_record_still_resolves_the_encoding() {
        let mut bytes = build_header(LE);
        write_numeric_variable(&mut bytes, LE, *b"V1      ");
        write_character_code_record(&mut bytes, LE, 65001); // UTF-8
        write_terminator(&mut bytes, LE);

        let mut reader = open_skipping(
            bytes,
            &[SkippableContent::Extension(
                ExtensionSubtype::MachineIntegerInfo,
            )],
        );
        assert_eq!(
            reader.encoding_provenance(),
            EncodingProvenance::Codepage(encoding_rs::UTF_8),
        );
        // Absorbed, but not handed out.
        assert_eq!(kinds(&mut reader), vec![DictionaryRecordKind::Variable]);
    }

    /// Skipping the subtype-20 record likewise still lets it govern the
    /// encoding.
    #[test]
    fn skipping_the_character_encoding_record_still_resolves_the_encoding() {
        let mut bytes = build_header(LE);
        write_numeric_variable(&mut bytes, LE, *b"V1      ");
        let label = b"UTF-8";
        write_extension_record(
            &mut bytes,
            LE,
            20,
            1,
            u32::try_from(label.len()).unwrap(),
            label,
        );
        write_terminator(&mut bytes, LE);

        let mut reader = open_skipping(
            bytes,
            &[SkippableContent::Extension(
                ExtensionSubtype::CharacterEncoding,
            )],
        );
        assert_eq!(
            reader.encoding_provenance(),
            EncodingProvenance::Label(encoding_rs::UTF_8),
        );
        assert_eq!(kinds(&mut reader), vec![DictionaryRecordKind::Variable]);
    }

    /// Warnings a skipped record would have raised are suppressed along
    /// with it — nothing will be handed out to attribute them to.
    #[test]
    fn skipped_records_raise_no_warnings() {
        let mut bytes = build_header(LE);
        write_numeric_variable(&mut bytes, LE, *b"V1      ");
        // Duplicate value-label keys warn when retained.
        write_value_label_pair(&mut bytes, &[(1.0, b"one"), (1.0, b"uno")], &[1]);
        write_terminator(&mut bytes, LE);

        let mut retained = open(bytes.clone());
        let mut saw_duplicate = false;
        while retained.read_record().unwrap().is_some() {
            saw_duplicate |= retained
                .warnings()
                .iter()
                .any(|w| matches!(w, SavWarning::DuplicateValueLabelKey { .. }));
        }
        assert!(saw_duplicate, "baseline should warn");

        let mut skipped = open_skipping(bytes, &[SkippableContent::ValueLabels]);
        while let Some(record) = skipped.read_record().unwrap() {
            let _ = record;
            assert!(skipped.warnings().is_empty(), "{:?}", skipped.warnings());
        }
    }

    #[test]
    fn peek_kind_matches_the_next_record() {
        let mut reader = open(one_of_each());
        while let Some(expected) = reader.peek_kind() {
            let record = reader.read_record().unwrap().expect("peek promised one");
            assert_eq!(record.kind(), expected);
        }
        assert!(reader.read_record().unwrap().is_none());
    }

    #[test]
    fn skip_record_reports_what_it_passed_over() {
        let mut reader = open(one_of_each());
        assert_eq!(
            reader.skip_record().unwrap(),
            Some(DictionaryRecordKind::Variable),
        );
        assert_eq!(
            reader.skip_record().unwrap(),
            Some(DictionaryRecordKind::ValueLabelSet),
        );
        // The rest still stream normally.
        assert_eq!(
            kinds(&mut reader),
            vec![
                DictionaryRecordKind::Document,
                DictionaryRecordKind::Extension(ExtensionSubtype::Uuid),
            ],
        );
        assert_eq!(reader.skip_record().unwrap(), None);
    }

    /// `skip_record` says "I do not want this record", not "do not read
    /// it". A variable the caller passed over is still folded into the
    /// schema — otherwise the schema would come out with a hole in it
    /// and disagree with the layout the rows are read through.
    #[test]
    fn skipping_every_record_still_yields_a_complete_schema() {
        let mut reader = open(one_of_each());
        while reader.skip_record().unwrap().is_some() {}
        let finalized = reader.into_record_reader().expect("finalize");
        let schema = finalized.schema();
        assert_eq!(schema.variable_count(), 1);
        assert_eq!(schema.variables()[0].short_name(), "V1");
        // The value-label pair the caller passed over is attached too.
        assert!(schema.variables()[0].value_labels().is_some());
    }

    /// Reading some records and passing over others must come out the
    /// same as reading them all — the caller's choices are about what
    /// they see, not about what the library builds.
    #[test]
    fn mixing_reads_and_skips_matches_reading_everything() {
        let mut read_all = open(one_of_each());
        while read_all.read_record().unwrap().is_some() {}
        let expected = read_all.into_record_reader().expect("finalize");
        let expected = expected.schema();

        let mut mixed = open(one_of_each());
        mixed.skip_record().unwrap(); // the variable
        mixed.read_record().unwrap(); // the value labels
        mixed.skip_record().unwrap(); // the document
        let mixed = mixed.into_record_reader().expect("finalize");
        let mixed = mixed.schema();

        assert_eq!(mixed.variable_count(), expected.variable_count());
        assert_eq!(
            mixed.variables()[0].short_name(),
            expected.variables()[0].short_name(),
        );
        assert_eq!(
            mixed.variables()[0].value_labels().map(ValueLabelSet::len),
            expected.variables()[0]
                .value_labels()
                .map(ValueLabelSet::len),
        );
    }

    /// A record nothing draws on is passed over undecoded — that is the
    /// work `skip_record` exists to save.
    #[test]
    fn a_record_nothing_needs_is_never_decoded() {
        let mut reader = open(one_of_each());
        // Documents contribute nothing to a schema, so the reader has no
        // reason to decode one the caller declined.
        assert_eq!(
            reader.peek_kind(),
            Some(DictionaryRecordKind::Variable),
            "fixture order changed",
        );
        reader.skip_record().unwrap();
        reader.skip_record().unwrap();
        assert_eq!(reader.peek_kind(), Some(DictionaryRecordKind::Document));
        assert_eq!(
            reader.skip_record().unwrap(),
            Some(DictionaryRecordKind::Document),
        );
        assert!(reader.warnings().is_empty());
    }

    /// The two mechanisms compose without overlap: a record skipped up
    /// front was never buffered, so `peek_kind` never offers it.
    #[test]
    fn option_skipped_records_are_invisible_to_peek() {
        let mut reader = open_skipping(one_of_each(), &[SkippableContent::Documents]);
        let mut seen = Vec::new();
        while let Some(kind) = reader.peek_kind() {
            seen.push(kind);
            reader.read_record().unwrap();
        }
        assert!(!seen.contains(&DictionaryRecordKind::Document));
    }
}
