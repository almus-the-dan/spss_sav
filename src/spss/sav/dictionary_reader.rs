//! Streaming reader for the SAV dictionary section.
//!
//! Sits between [`HeaderReader`](crate::spss::sav::header_reader::HeaderReader)
//! and the (future) record reader. Yields one
//! [`DictionaryRecord`](crate::spss::sav::dictionary_record::DictionaryRecord)
//! at a time — variable records, value-label sets,
//! document records, and extension records freely interleaved between
//! the header and the `999` end-of-dictionary marker.
//!
//! The records were already read off the wire by
//! [`HeaderReader::read_header`](crate::spss::sav::header_reader::HeaderReader::read_header),
//! which had to walk the whole dictionary to find the file's declared
//! encoding (see the crate-internal `DictionaryBuffer`).
//! This phase therefore decodes rather than reads: it turns each
//! buffered record into a
//! [`DictionaryRecord`](crate::spss::sav::dictionary_record::DictionaryRecord)
//! using the resolved encoding. Structural errors have already surfaced from
//! `read_header`; what can still fail here is per-subtype extension
//! payload validation, which never had to run to find record
//! boundaries.

use std::io::Read;

use encoding_rs::Encoding;

use crate::spss::sav::buffered_document_record::BufferedDocumentRecord;
use crate::spss::sav::buffered_record_payload::BufferedRecordPayload;
use crate::spss::sav::buffered_value_label_set::BufferedValueLabelSet;
use crate::spss::sav::buffered_variable_record::BufferedVariableRecord;
use crate::spss::sav::data_layout::{DataLayout, DataLayoutBuilder};
use crate::spss::sav::dictionary_buffer::DictionaryBuffer;
use crate::spss::sav::dictionary_parse::{parse_short_name, parse_value_label_entry};
use crate::spss::sav::dictionary_record::DictionaryRecord;
use crate::spss::sav::dictionary_record_kind::DictionaryRecordKind;
use crate::spss::sav::document_record::DocumentRecord;
use crate::spss::sav::encoding_provenance::EncodingProvenance;
use crate::spss::sav::extension_envelope::ExtensionEnvelope;
use crate::spss::sav::extensions::character_encoding;
use crate::spss::sav::extensions::extended_number_of_cases;
use crate::spss::sav::extensions::extension_record::ExtensionRecord;
use crate::spss::sav::extensions::extension_subtype::ExtensionSubtype;
use crate::spss::sav::extensions::extra_product_info;
use crate::spss::sav::extensions::file_attributes;
use crate::spss::sav::extensions::float_sentinels;
use crate::spss::sav::extensions::long_missing_values;
use crate::spss::sav::extensions::long_value_labels;
use crate::spss::sav::extensions::long_variable_names;
use crate::spss::sav::extensions::machine_integer_info;
use crate::spss::sav::extensions::multiple_response_sets;
use crate::spss::sav::extensions::raw_display_parameters;
use crate::spss::sav::extensions::unknown_extension::UnknownExtension;
use crate::spss::sav::extensions::uuid;
use crate::spss::sav::extensions::variable_attributes;
use crate::spss::sav::extensions::variable_sets;
use crate::spss::sav::extensions::very_long_strings;
use crate::spss::sav::raw_value_label_set::RawValueLabelSet;
use crate::spss::sav::reader_options::ReaderOptions;
use crate::spss::sav::reader_state::ReaderState;
use crate::spss::sav::record_reader::RecordReader;
use crate::spss::sav::sav_error::Result;
use crate::spss::sav::sav_header::SavHeader;
use crate::spss::sav::sav_schema::SavSchemaBuilder;
use crate::spss::sav::sav_variable_header::SavVariableHeader;
use crate::spss::sav::sav_warning::SavWarning;

/// Streaming reader for the SAV dictionary section.
///
/// Created by
/// [`HeaderReader::read_header`](crate::spss::sav::header_reader::HeaderReader::read_header).
/// Pull individual records via [`read_record`](Self::read_record)
/// until it returns `Ok(None)`, or skip straight to record reading via
/// [`into_record_reader`](Self::into_record_reader) which auto-consumes
/// any remaining dictionary records.
#[derive(Debug)]
pub struct DictionaryReader<R> {
    state: ReaderState<R>,
    header: SavHeader,
    /// Options set on the upstream `SavReader`. Consulted at
    /// finalization to decide whether a schema is assembled.
    #[allow(dead_code)] // exercised once schema accumulation lands.
    options: ReaderOptions,
    encoding_provenance: EncodingProvenance,
    /// What the file's own declarations named, regardless of what the
    /// reader applied. Only consulted to report an override.
    declared_encoding: Option<EncodingProvenance>,
    buffer: DictionaryBuffer,
    /// The in-progress schema, folded together at
    /// [`into_record_reader`](Self::into_record_reader). Left empty when
    /// the caller turned schema building off.
    schema: SavSchemaBuilder,
    /// 0-based offset of the weight variable within a data row, in
    /// 8-byte units — the form the header declares it in.
    ///
    /// Also the physical type-2 record position, continuations
    /// included: a numeric variable and each string segment occupy one
    /// unit and one record apiece, so the two counts never diverge.
    weight_offset: Option<usize>,
}

impl<R> DictionaryReader<R> {
    pub(crate) fn new(
        state: ReaderState<R>,
        header: SavHeader,
        options: ReaderOptions,
        encoding_provenance: EncodingProvenance,
        declared_encoding: Option<EncodingProvenance>,
        buffer: DictionaryBuffer,
        weight_offset: Option<usize>,
    ) -> Self {
        Self {
            state,
            header,
            options,
            encoding_provenance,
            declared_encoding,
            buffer,
            schema: SavSchemaBuilder::default(),
            weight_offset,
        }
    }

    /// The raw weight offset the header declared, 0-based, or `None`
    /// when the file declared no weight.
    ///
    /// Test-only. A row offset is the least useful of the three index
    /// spaces and the easiest to misread, so nothing outside the header
    /// reader's own tests has a reason to see one — the resolved
    /// variable is reported by
    /// [`SavSchema::weight_variable`](crate::spss::sav::sav_schema::SavSchema::weight_variable)
    /// once the dictionary phase finalizes.
    #[cfg(test)]
    #[must_use]
    #[inline]
    pub(crate) fn weight_offset(&self) -> Option<usize> {
        self.weight_offset
    }

    /// The file header parsed by the upstream
    /// [`HeaderReader`](crate::spss::sav::header_reader::HeaderReader).
    ///
    /// Complete as it stands — every field comes from the 176-byte
    /// preamble, so nothing here changes as the dictionary is read.
    #[must_use]
    #[inline]
    pub fn header(&self) -> &SavHeader {
        &self.header
    }

    /// The encoding the reader applied, and where it came from.
    ///
    /// Known from the moment this reader exists: resolving it is what
    /// [`HeaderReader::read_header`](crate::spss::sav::header_reader::HeaderReader::read_header)
    /// walked the dictionary for.
    #[must_use]
    #[inline]
    pub fn encoding_provenance(&self) -> EncodingProvenance {
        self.encoding_provenance
    }

    /// Warnings accumulated by the most recent
    /// [`read_record`](Self::read_record) call (or by
    /// [`HeaderReader::read_header`](crate::spss::sav::header_reader::HeaderReader::read_header)
    /// for the first call). Cleared at the start of each `read_record`
    /// invocation.
    #[must_use]
    #[inline]
    pub fn warnings(&self) -> &[SavWarning] {
        self.state.warnings()
    }

    /// The kind of the record [`read_record`](Self::read_record) would
    /// return next, or `None` once every record has been handed out.
    ///
    /// Free and infallible: the bytes are already resident, and
    /// classifying a record needs nothing beyond fields the buffering
    /// pass has already read. Use it to decide whether the next record
    /// is worth decoding before paying to decode it.
    ///
    /// Records excluded up front via
    /// [`SavReader::skip_dictionary_content`](crate::spss::sav::sav_reader::SavReader::skip_dictionary_content)
    /// were never buffered, so this never sees them — the two
    /// mechanisms compose without overlap.
    #[must_use]
    #[inline]
    pub fn peek_kind(&self) -> Option<DictionaryRecordKind> {
        self.buffer.peek_kind()
    }

    /// Passes over the next record without handing it out, returning the
    /// kind passed over, or `None` once every record has been consumed.
    ///
    /// This says "I do not want this record", not "do not read this
    /// record". The bytes were read during
    /// [`read_header`](crate::spss::sav::header_reader::HeaderReader::read_header)
    /// regardless — the up-front
    /// [`SavReader::skip_dictionary_content`](crate::spss::sav::sav_reader::SavReader::skip_dictionary_content)
    /// is where the memory win lives. What this saves is the decode and
    /// the allocations behind it, for a record the caller has looked at
    /// with [`peek_kind`](Self::peek_kind) and decided against.
    ///
    /// So it saves that work only where nobody else needs it. A record
    /// the schema draws on is decoded and folded in anyway and simply
    /// not returned; the data layout never depended on this path at all,
    /// since it comes from a skeleton the buffering pass kept aside. No
    /// sequence of calls here can leave the schema with a hole in it or
    /// a data read misaligned.
    ///
    /// Clears [`warnings`](Self::warnings) the way
    /// [`read_record`](Self::read_record) does. A record that was
    /// processed still reports its warnings — they are about the file,
    /// and the reader acted on it. One passed over undecoded reports
    /// nothing.
    ///
    /// # Errors
    ///
    /// Returns whatever [`read_record`](Self::read_record) would for
    /// this record, when it is one the schema needs and its payload does
    /// not match its subtype's declared shape. A record nothing needs is
    /// never decoded and so can never fail.
    pub fn skip_record(&mut self) -> Result<Option<DictionaryRecordKind>> {
        self.state.warnings_mut().clear();
        let Some(buffered) = self.buffer.next_record() else {
            return Ok(None);
        };
        let kind = buffered.payload.kind();
        if !self.wants(kind) {
            return Ok(Some(kind));
        }
        self.state.warnings_mut().extend(buffered.warnings);
        let record = self.decode(buffered.payload)?;
        self.accumulate(&record);
        Ok(Some(kind))
    }

    /// Whether the library itself draws on a record of this kind, and so
    /// must decode it even when the caller has passed over it.
    ///
    /// Mirrors [`accumulate`](Self::accumulate) — every kind that method
    /// does something with has to be listed here. Nothing consults the
    /// data layout: that is built from the buffer's skeleton, not from
    /// these records.
    fn wants(&self, kind: DictionaryRecordKind) -> bool {
        if !self.options.build_schema() {
            return false;
        }
        match kind {
            DictionaryRecordKind::Variable | DictionaryRecordKind::ValueLabelSet => true,
            DictionaryRecordKind::Document => false,
            DictionaryRecordKind::Extension(subtype) => matches!(
                subtype,
                ExtensionSubtype::LongVariableNames
                    | ExtensionSubtype::DisplayParameters
                    | ExtensionSubtype::VariableAttributes
                    | ExtensionSubtype::LongValueLabels
                    | ExtensionSubtype::LongMissingValues
            ),
        }
    }
    /// Decodes one buffered payload into the record a caller sees.
    fn decode(&mut self, payload: BufferedRecordPayload) -> Result<DictionaryRecord> {
        let encoding = self.encoding_provenance.encoding();
        let record = match payload {
            BufferedRecordPayload::Variable(variable) => {
                let record = decode_variable_record(variable, encoding);
                DictionaryRecord::Variable(record)
            }
            BufferedRecordPayload::ValueLabelSet(set) => {
                let set = decode_value_label_set(set, encoding);
                DictionaryRecord::ValueLabelSet(set)
            }
            BufferedRecordPayload::Document(document) => {
                let record = decode_document_record(document, encoding);
                DictionaryRecord::Document(record)
            }
            BufferedRecordPayload::Extension(envelope) => {
                self.decode_extension_record(envelope, encoding)?
            }
        };
        Ok(record)
    }

    /// Folds a decoded record into the in-progress schema.
    ///
    /// A no-op when the caller turned schema building off. The data
    /// layout is deliberately not fed here — it comes from the buffer's
    /// own skeleton at finalization, so it cannot depend on which
    /// records were pulled.
    fn accumulate(&mut self, record: &DictionaryRecord) {
        if !self.options.build_schema() {
            return;
        }
        match record {
            DictionaryRecord::Variable(header) => self.schema.push_variable(header),
            DictionaryRecord::ValueLabelSet(set) => self.schema.push_value_labels(set),
            DictionaryRecord::Document(_) => {}
            DictionaryRecord::Extension(extension) => match extension {
                ExtensionRecord::LongVariableNames(names) => self.schema.set_long_names(names),
                ExtensionRecord::DisplayParameters(parameters) => {
                    self.schema.set_display_parameters(parameters);
                }
                ExtensionRecord::VariableAttributes(attributes) => {
                    self.schema.set_variable_attributes(attributes);
                }
                ExtensionRecord::LongValueLabels(labels) => {
                    self.schema.set_long_value_labels(labels);
                }
                ExtensionRecord::LongMissingValues(values) => {
                    self.schema.set_long_missing_values(values);
                }
                _ => {}
            },
        }
    }

    /// Dispatches an extension envelope to its per-subtype reader.
    /// Unrecognized subtypes fall through to
    /// [`decode_unknown_extension`](Self::decode_unknown_extension),
    /// which preserves the payload verbatim and pushes a
    /// [`SavWarning::UnknownExtensionSubtype`].
    fn decode_extension_record(
        &mut self,
        envelope: ExtensionEnvelope,
        encoding: &'static Encoding,
    ) -> Result<DictionaryRecord> {
        self.warn_if_override_disagrees(envelope.subtype);
        match ExtensionSubtype::from_code(envelope.subtype) {
            ExtensionSubtype::MachineIntegerInfo => {
                machine_integer_info::read(&envelope, &self.header, self.state.warnings_mut())
            }
            ExtensionSubtype::FloatInfo => float_sentinels::read(&envelope),
            ExtensionSubtype::ExtendedNumberOfCases => extended_number_of_cases::read(&envelope),
            ExtensionSubtype::CharacterEncoding => character_encoding::read(&envelope),
            ExtensionSubtype::LongVariableNames => long_variable_names::read(&envelope, encoding),
            ExtensionSubtype::VeryLongStrings => very_long_strings::read(&envelope, encoding),
            ExtensionSubtype::DisplayParameters => raw_display_parameters::read(&envelope),
            ExtensionSubtype::VariableSets => variable_sets::read(&envelope, encoding),
            ExtensionSubtype::MultipleResponseSets
            | ExtensionSubtype::MultipleResponseSetsExtended => {
                multiple_response_sets::read(&envelope, encoding)
            }
            ExtensionSubtype::ExtraProductInfo => extra_product_info::read(&envelope, encoding),
            ExtensionSubtype::Uuid => uuid::read(&envelope, encoding),
            ExtensionSubtype::FileAttributes => file_attributes::read(&envelope, encoding),
            ExtensionSubtype::VariableAttributes => variable_attributes::read(&envelope, encoding),
            ExtensionSubtype::LongValueLabels => long_value_labels::read(&envelope, encoding),
            ExtensionSubtype::LongMissingValues => long_missing_values::read(&envelope, encoding),
            ExtensionSubtype::Unrecognized => {
                let unknown = self.decode_unknown_extension(envelope);
                Ok(unknown)
            }
        }
    }

    /// Raises [`SavWarning::EncodingOverridden`] when the reader is
    /// applying an override and the file's own declaration named a
    /// different encoding.
    ///
    /// Deliberately late: the warning fires as the declaring record
    /// reaches the caller, not during resolution, so it appears
    /// alongside the record it is about.
    ///
    /// Fires from whichever record *would have* determined the encoding
    /// had the override not been in play — the character encoding record
    /// (subtype 20) when it declared something resolvable, otherwise the
    /// machine integer info record (subtype 3). That is the resolution
    /// precedence, so a file declaring through both sites warns once
    /// rather than twice, and a file declaring only through subtype 3
    /// still warns. A declaration that resolves nowhere raises nothing:
    /// there is no encoding to report as having been overridden.
    fn warn_if_override_disagrees(&mut self, subtype: i32) {
        let EncodingProvenance::Overridden(used) = self.encoding_provenance else {
            return;
        };
        // Bound together so the encoding comes from the same match that
        // identified the declaring record, rather than being re-derived
        // from a value only this match proves is present.
        let (declaring_subtype, declared) = match self.declared_encoding {
            Some(EncodingProvenance::Label(declared)) => {
                (ExtensionSubtype::CharacterEncoding, declared)
            }
            Some(EncodingProvenance::Codepage(declared)) => {
                (ExtensionSubtype::MachineIntegerInfo, declared)
            }
            _ => return,
        };
        if ExtensionSubtype::from_code(subtype) != declaring_subtype {
            return;
        }
        if declared == used {
            return;
        }
        self.state
            .warnings_mut()
            .push(SavWarning::EncodingOverridden {
                declared: declared.name(),
                used: used.name(),
            });
    }

    /// Fallback for unrecognized subtypes. Owns the envelope so it can
    /// move the payload into [`UnknownExtension`] without an extra
    /// allocation, and pushes a
    /// [`SavWarning::UnknownExtensionSubtype`] so callers can
    /// distinguish "carried verbatim" from "interpreted".
    fn decode_unknown_extension(&mut self, envelope: ExtensionEnvelope) -> DictionaryRecord {
        let subtype_u32 = envelope.subtype.cast_unsigned();
        self.state
            .warnings_mut()
            .push(SavWarning::UnknownExtensionSubtype {
                subtype: subtype_u32,
            });
        let unknown = UnknownExtension::builder()
            .subtype(subtype_u32)
            .element_size(envelope.element_size_usize)
            .element_count(envelope.element_count_usize)
            .payload(envelope.payload)
            .build();
        let record = ExtensionRecord::Unknown(unknown);
        DictionaryRecord::Extension(record)
    }
}

impl<R: Read> DictionaryReader<R> {
    /// Decodes and returns the next dictionary record. Returns
    /// `Ok(None)` once every record has been handed out.
    ///
    /// String-variable continuation records were collapsed into their
    /// primaries while buffering; the caller never sees them.
    ///
    /// # Errors
    ///
    /// Returns [`SavError::Format`](crate::spss::sav::sav_error::SavError::Format)
    /// when an extension record's payload does not match its subtype's
    /// declared shape. Structural errors cannot occur here — they
    /// surfaced from `read_header`.
    pub fn read_record(&mut self) -> Result<Option<DictionaryRecord>> {
        self.state.warnings_mut().clear();
        let Some(buffered) = self.buffer.next_record() else {
            return Ok(None);
        };
        // Replay the warnings this record raised while it was read, so
        // `warnings()` still reports per-call results.
        self.state.warnings_mut().extend(buffered.warnings);

        let record = self.decode(buffered.payload)?;
        self.accumulate(&record);
        Ok(Some(record))
    }

    /// Auto-consumes any remaining dictionary records, finalizes
    /// the schema, and transitions to record reading.
    ///
    /// Never complains that the caller left records unread — consuming
    /// them is this method's job.
    ///
    /// # Errors
    ///
    /// Returns whatever [`read_record`](Self::read_record) would
    /// return for any record consumed during finalization, plus any
    /// error from decoding the layout-bearing extension records.
    ///
    /// # Implementation note
    ///
    /// The schema is accumulated in
    /// [`read_record`](Self::read_record), as each record is handed
    /// out, and cannot be rebuilt here: draining *moves* each record out
    /// of the buffer, so by the time this runs everything the caller
    /// already pulled is gone. Only the records they never pulled remain,
    /// which is why finalization has to drive them through the same path
    /// rather than making a second pass.
    ///
    /// The data layout is the exception, and deliberately so. It is
    /// derived from a compact skeleton the buffering pass kept aside —
    /// each variable's short name and declared type, plus copies of the
    /// three extension records the layout depends on. That is what makes
    /// the layout independent of every filtering choice: pull the
    /// records, skip them, exclude them up front, or never touch the
    /// reader, and the rows still read the same.
    pub fn into_record_reader(mut self) -> Result<RecordReader<R>> {
        self.state.warnings_mut().clear();
        while self.read_record()?.is_some() {
            self.state.warnings_mut().clear();
        }
        self.state.warnings_mut().clear();

        let encoding = self.encoding_provenance.encoding();
        let layout = self.build_layout(encoding)?;

        let weight = self.weight_variable_index(&layout);
        let schema = if self.options.build_schema() {
            let mut warnings = Vec::new();
            let accumulated = std::mem::take(&mut self.schema);
            let schema = accumulated.build(
                &layout,
                self.header.float_encoding(),
                layout.sentinels(),
                weight,
                &mut warnings,
            );
            self.state.warnings_mut().append(&mut warnings);
            Some(schema)
        } else {
            None
        };

        let reader = RecordReader::new(
            self.state,
            self.header,
            self.encoding_provenance,
            layout,
            schema,
        );
        Ok(reader)
    }

    /// Rebuilds the data layout from the buffer's skeleton.
    fn build_layout(&mut self, encoding: &'static Encoding) -> Result<DataLayout> {
        let mut builder = DataLayoutBuilder::default();
        for (short_name, variable_type) in self.buffer.skeleton().variables() {
            let short_name = parse_short_name(*short_name, encoding);
            builder.push_variable(short_name, *variable_type);
        }
        // Cloned so the borrow of the buffer ends before the builder is
        // fed; there are at most three of these, and they are tiny.
        let envelopes: Vec<_> = self.buffer.skeleton().envelopes().to_vec();
        for envelope in &envelopes {
            match ExtensionSubtype::from_code(envelope.subtype) {
                ExtensionSubtype::FloatInfo => {
                    if let DictionaryRecord::Extension(ExtensionRecord::FloatInfo(sentinels)) =
                        float_sentinels::read(envelope)?
                    {
                        builder.set_sentinels(sentinels);
                    }
                }
                ExtensionSubtype::VeryLongStrings => {
                    if let DictionaryRecord::Extension(ExtensionRecord::VeryLongStrings(strings)) =
                        very_long_strings::read(envelope, encoding)?
                    {
                        builder.set_very_long_strings(&strings);
                    }
                }
                ExtensionSubtype::ExtendedNumberOfCases => {
                    if let DictionaryRecord::Extension(ExtensionRecord::ExtendedNumberOfCases(
                        cases,
                    )) = extended_number_of_cases::read(envelope)?
                    {
                        builder.set_extended_case_count(cases.count());
                    }
                }
                _ => {}
            }
        }
        let mut warnings = Vec::new();
        let layout = builder.build(&self.header, encoding, &mut warnings);
        self.state.warnings_mut().append(&mut warnings);
        Ok(layout)
    }

    /// The weight variable's position in the finalized schema, resolved
    /// from the row offset the header declared.
    ///
    /// Three index spaces have to be crossed. The header stores an
    /// offset into the data row, which is also the *physical* record
    /// position since a numeric variable and each string segment occupy
    /// one 8-byte unit apiece; the skeleton turns that into a *segment*;
    /// and the layout's segment grouping turns that into the *variable*
    /// the schema indexes by.
    ///
    /// `None` when the file declared no weight, or when the offset names
    /// a continuation record or a position past the end.
    fn weight_variable_index(&self, layout: &DataLayout) -> Option<usize> {
        let physical = u32::try_from(self.weight_offset?).ok()?;
        let segment = self.buffer.skeleton().segment_of_physical(physical)?;
        variable_of_segment(layout, segment)
    }
}

/// The logical variable that owns `segment`, or `None` when the segment
/// index lies past the end.
///
/// A very long string owns several consecutive segments, so the mapping
/// is a walk over the layout's grouping rather than an identity.
fn variable_of_segment(layout: &DataLayout, segment: usize) -> Option<usize> {
    let mut seen = 0;
    for (index, variable) in layout.variables().iter().enumerate() {
        seen += variable.segments().len();
        if segment < seen {
            return Some(index);
        }
    }
    None
}

/// Decodes a buffered variable record's two text fields into a
/// [`SavVariableHeader`]. Everything else was parsed and validated while
/// the record was buffered.
fn decode_variable_record(
    variable: BufferedVariableRecord,
    encoding: &'static Encoding,
) -> SavVariableHeader {
    let mut builder = SavVariableHeader::builder()
        .short_name(parse_short_name(variable.short_name, encoding))
        .variable_type(variable.variable_type)
        .missing_values(variable.missing_values)
        .print_format(variable.print_format)
        .write_format(variable.write_format);
    if let Some(label) = variable.label {
        let (decoded, _, _) = encoding.decode(&label);
        builder = builder.label(decoded.into_owned());
    }
    builder.build()
}

/// Decodes the entry labels of a buffered value-label set. The 8-byte
/// values and the already-normalized variable indices pass through
/// unchanged.
fn decode_value_label_set(
    set: BufferedValueLabelSet,
    encoding: &'static Encoding,
) -> RawValueLabelSet {
    let entries = set
        .entries
        .into_iter()
        .map(|entry| {
            let unpadded_len = u8::try_from(entry.label.len()).unwrap_or(u8::MAX);
            parse_value_label_entry(entry.value, unpadded_len, &entry.label, encoding)
        })
        .collect();
    RawValueLabelSet::builder()
        .entries(entries)
        .segment_indices(set.segment_indices)
        .build()
}

/// Decodes a buffered document record's fixed-width lines.
fn decode_document_record(
    document: BufferedDocumentRecord,
    encoding: &'static Encoding,
) -> DocumentRecord {
    let lines = document
        .lines
        .into_iter()
        .map(|line| {
            let (decoded, _, _) = encoding.decode(&line);
            decoded.into_owned()
        })
        .collect();
    DocumentRecord::builder().lines(lines).build()
}

#[cfg(test)]
mod tests {

    use crate::spss::sav::byte_order::ByteOrder;
    use crate::spss::sav::dictionary_format::{DOCUMENT_LINE_LEN, VARIABLE_RECORD_BODY_LEN};
    use crate::spss::sav::dictionary_record::DictionaryRecord;
    use crate::spss::sav::encoding_provenance::EncodingProvenance;
    use crate::spss::sav::encoding_strategy::EncodingStrategy;
    use crate::spss::sav::extensions::extension_record::ExtensionRecord;
    use crate::spss::sav::raw_missing_values::RawMissingValues;
    use crate::spss::sav::sav_error::{Field, FormatErrorKind, SavError};
    use crate::spss::sav::sav_format_kind::SavFormatKind;
    use crate::spss::sav::sav_warning::SavWarning;
    use crate::spss::sav::test_support::{
        assert_unexpected_value_error, build_header, open, open_with, try_open,
        write_character_code_record, write_extension_record, write_rec_type, write_terminator,
        write_u32,
    };
    use crate::spss::sav::variable_type::VariableType;

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
        let mut buf = Vec::with_capacity(VARIABLE_RECORD_BODY_LEN);
        buf.extend_from_slice(&i32_bytes(type_value));
        buf.extend_from_slice(&i32_bytes(has_label));
        buf.extend_from_slice(&i32_bytes(n_missing));
        buf.extend_from_slice(&u32_bytes(print));
        buf.extend_from_slice(&u32_bytes(write));
        buf.extend_from_slice(&name);
        buf
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

        let err = try_open(bytes).unwrap_err();
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

        let err = try_open(bytes).unwrap_err();
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

        let err = try_open(bytes).unwrap_err();
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

        let err = try_open(bytes).unwrap_err();
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

        let err = try_open(bytes).unwrap_err();
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

        let err = try_open(bytes).unwrap_err();
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

        let err = try_open(bytes).unwrap_err();
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

        let err = try_open(bytes).unwrap_err();
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
        assert_eq!(set.segment_indices(), &[0]);
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
        assert_eq!(set.segment_indices(), &[0]);
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
        assert_eq!(set.segment_indices(), &[0, 1]);
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
        assert_eq!(set.segment_indices(), &[1]);
    }

    #[test]
    fn stray_type_4_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_rec_type(&mut bytes, byte_order, 4);
        write_u32(&mut bytes, byte_order, 1);
        write_u32(&mut bytes, byte_order, 1);

        let err = try_open(bytes).unwrap_err();
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

        let err = try_open(bytes).unwrap_err();
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
        let err = try_open(bytes).unwrap_err();
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
        let err = try_open(bytes).unwrap_err();
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

        let err = try_open(bytes).unwrap_err();
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
        assert_eq!(set.segment_indices(), &[] as &[u32]);
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
        assert_eq!(set.segment_indices(), &[0, 2, 0]);
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
        assert!(
            text.len() <= DOCUMENT_LINE_LEN,
            "test line exceeds one document line"
        );
        buf.extend_from_slice(text);
        buf.extend_from_slice(&vec![b' '; DOCUMENT_LINE_LEN - text.len()]);
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
        assert_eq!(doc.lines()[0].len(), DOCUMENT_LINE_LEN);
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

    /// A subtype-3 record whose declared `element_size` is not 4 must not
    /// steer encoding resolution.
    ///
    /// `character_code` sits at payload offset 28 only when the elements
    /// are four bytes wide. With `element_size = 8` the field is at offset
    /// 56, so reading offset 28 yields the middle of an unrelated field.
    /// Before the shape check, that garbage resolved the encoding and the
    /// whole file was decoded with it, and the record was only rejected
    /// later when it reached the caller.
    #[test]
    fn misshapen_codepage_record_does_not_steer_resolution() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let mut payload = vec![0u8; 64];
        // A plausible codepage where a 4-byte-field layout would put
        // character_code, and a different one where it actually is.
        payload[28..32].copy_from_slice(&65001_i32.to_le_bytes());
        payload[56..60].copy_from_slice(&1252_i32.to_le_bytes());
        write_extension_record(&mut bytes, byte_order, 3, 8, 8, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        // Neither of the two candidate codepages: the record declared a
        // shape that makes its payload uninterpretable, so it contributes
        // nothing and the strategy's fallback applies.
        assert_eq!(
            dict.encoding_provenance(),
            EncodingProvenance::Unspecified(encoding_rs::WINDOWS_1252)
        );
        // The record itself is still rejected when handed to the caller.
        let err = dict.read_record().unwrap_err();
        assert_unexpected_value_error(&err, Field::ExtensionElementSize);
    }

    /// A file that declares its encoding only through subtype 3 must
    /// still report that an override displaced it. Subtype 20 is the
    /// usual site, so this is the path that closes the gap.
    #[test]
    fn override_warns_from_the_codepage_record_when_no_label_is_declared() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_character_code_record(&mut bytes, byte_order, 65001);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open_with(bytes, EncodingStrategy::Override(encoding_rs::WINDOWS_1252));
        let mut overridden = Vec::new();
        while dict.read_record().unwrap().is_some() {
            overridden.extend(
                dict.warnings()
                    .iter()
                    .filter(|w| matches!(w, SavWarning::EncodingOverridden { .. }))
                    .cloned(),
            );
        }
        assert!(
            matches!(
                overridden.as_slice(),
                [SavWarning::EncodingOverridden {
                    declared: "UTF-8",
                    used: "windows-1252"
                }]
            ),
            "warnings = {overridden:?}"
        );
    }

    /// Declaring through both sites must warn once, from subtype 20 —
    /// the record that actually determines the encoding.
    #[test]
    fn override_warns_once_when_both_declaring_records_are_present() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_character_code_record(&mut bytes, byte_order, 65001);
        write_extension_record(&mut bytes, byte_order, 20, 1, 5, b"UTF-8");
        write_terminator(&mut bytes, byte_order);

        let mut dict = open_with(bytes, EncodingStrategy::Override(encoding_rs::WINDOWS_1252));
        let mut count = 0;
        while dict.read_record().unwrap().is_some() {
            count += dict
                .warnings()
                .iter()
                .filter(|w| matches!(w, SavWarning::EncodingOverridden { .. }))
                .count();
        }
        assert_eq!(count, 1);
    }

    /// An override matching what the file declared is not a mismatch.
    #[test]
    fn override_matching_the_declaration_does_not_warn() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_character_code_record(&mut bytes, byte_order, 65001);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open_with(bytes, EncodingStrategy::Override(encoding_rs::UTF_8));
        while dict.read_record().unwrap().is_some() {
            assert!(
                !dict
                    .warnings()
                    .iter()
                    .any(|w| matches!(w, SavWarning::EncodingOverridden { .. })),
                "warnings = {:?}",
                dict.warnings()
            );
        }
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
}
