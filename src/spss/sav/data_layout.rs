//! Everything the record reader needs to decode a data row.

use encoding_rs::Encoding;

use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::compression::compression_kind::CompressionKind;
use crate::spss::sav::compression::row_coding::RowCoding;
use crate::spss::sav::extensions::extension_subtype::ExtensionSubtype;
use crate::spss::sav::extensions::float_sentinels::FloatSentinels;
use crate::spss::sav::extensions::long_missing_values::LongMissingValues;
use crate::spss::sav::extensions::long_variable_names::LongVariableNames;
use crate::spss::sav::extensions::very_long_strings::VeryLongStrings;
use crate::spss::sav::float_encoding::FloatEncoding;
use crate::spss::sav::missing_value_reconcile;
use crate::spss::sav::missing_value_specification::MissingValueSpecification;
use crate::spss::sav::raw_missing_values::RawMissingValues;
use crate::spss::sav::sav_header::SavHeader;
use crate::spss::sav::sav_warning::SavWarning;
use crate::spss::sav::segment_layout::{
    SegmentLayout, segment_count, segment_width, segment_width_accepted,
};
use crate::spss::sav::variable_layout::VariableLayout;
use crate::spss::sav::variable_type::VariableType;

/// The complete, self-contained description of a SAV file's data
/// section.
///
/// Kept separate from
/// [`SavSchema`](crate::spss::sav::sav_schema::SavSchema) because the
/// two are assembled from different sources. This is derived from a
/// skeleton the buffering pass sets aside *before any skip decision*,
/// where the schema is accumulated from records as they are handed out.
/// That difference is the guarantee: neither
/// [`skip_dictionary_content`](crate::spss::sav::sav_reader::SavReader::skip_dictionary_content)
/// nor
/// [`skip_record`](crate::spss::sav::dictionary_reader::DictionaryReader::skip_record)
/// can leave the record reader unable to do its job, and nothing has to
/// be kept in sync for that to hold.
///
/// The schema is skip-safe too, but only partly by the same structure.
/// Everything the skeleton already carries — the variable names from
/// subtype 13, a very long string's missing values from subtype 22 — is
/// handed to the schema from here, so no filtering choice can lose it.
/// The rest is skip-safe by discipline: `schema_draws_on` has to name
/// every record kind that `accumulate` still consumes, and missing one
/// loses data silently. Content whose payload an up-front skip declines
/// to retain is gone either way, which is what that option is for.
///
/// The two overlap on the declared missing values, which a correct read
/// needs and the schema also presents. The rest of the schema — labels,
/// value labels, display parameters, attributes — a data read never
/// consults.
///
/// It is copied into the record reader rather than borrowed because
/// [`into_record_reader`](crate::spss::sav::dictionary_reader::DictionaryReader::into_record_reader)
/// consumes the dictionary buffer the skeleton lives in, so by the time
/// the record reader exists there is nothing left to borrow from.
#[derive(Debug, Clone)]
pub(crate) struct DataLayout {
    variables: Vec<VariableLayout>,
    row_len: usize,
    compression: CompressionKind,
    bias: f64,
    float_encoding: FloatEncoding,
    sentinels: FloatSentinels,
    encoding: &'static Encoding,
    case_count: Option<u64>,
}

impl DataLayout {
    /// Per-variable placement, in logical (very-long-string-collapsed)
    /// order — the same order
    /// [`SavSchema::variables`](crate::spss::sav::sav_schema::SavSchema::variables)
    /// uses, so an index into one indexes the other.
    #[inline]
    pub fn variables(&self) -> &[VariableLayout] {
        &self.variables
    }

    /// Bytes in one uncompressed data row.
    ///
    /// The sum of every segment's stride, which is also what the
    /// header's `nominal_case_size` counts in 8-byte units.
    #[inline]
    pub fn row_len(&self) -> usize {
        self.row_len
    }

    /// What the row source needs, and nothing else.
    ///
    /// Deliberately narrow — see [`RowCoding`] for why the compressed
    /// decoders must not be able to reach [`variables`](Self::variables).
    pub fn row_coding(&self) -> RowCoding {
        RowCoding::new(
            self.row_len,
            self.bias,
            self.float_encoding,
            self.sentinels.system_missing(),
        )
    }

    /// How the data section is compressed.
    #[inline]
    pub fn compression(&self) -> CompressionKind {
        self.compression
    }

    /// How this file encodes an `f64` on disk.
    #[inline]
    pub fn float_encoding(&self) -> FloatEncoding {
        self.float_encoding
    }

    /// Byte order of the file's multibyte fields.
    ///
    /// Carried by [`float_encoding`](Self::float_encoding), since a
    /// double's on-disk layout depends on it, but asked for in its own
    /// right by the ZSAV block container — whose header fields are
    /// multibyte and follow the file's order, while the command stream
    /// they frame has no multibyte fields at all.
    #[inline]
    pub fn byte_order(&self) -> ByteOrder {
        self.float_encoding.byte_order()
    }

    /// The sentinel values marking system-missing, highest and lowest,
    /// from subtype 4 when the file carried one and the format's
    /// canonical triple otherwise.
    #[inline]
    pub fn sentinels(&self) -> &FloatSentinels {
        &self.sentinels
    }

    /// The resolved text encoding for string cells.
    #[inline]
    pub fn encoding(&self) -> &'static Encoding {
        self.encoding
    }

    /// How many rows the file claims to hold, or `None` when it did not
    /// say.
    ///
    /// The subtype-16 extended count wins over the header's 32-bit
    /// field when both are present and disagree.
    #[inline]
    pub fn case_count(&self) -> Option<u64> {
        self.case_count
    }
}

/// One type-2 primary record's contribution, held until the very
/// long strings are known and segments can be grouped into variables.
#[derive(Debug, Clone)]
struct Segment {
    short_name: String,
    layout: SegmentLayout,
    missing: RawMissingValues,
}

/// Builds a [`DataLayout`] from the buffer's layout skeleton.
///
/// Fed from what the buffering pass set aside, not from the records the
/// caller pulls — which is what makes a data read independent of every
/// filtering choice. Nothing a caller does to the record stream reaches
/// this builder.
#[derive(Debug, Default)]
pub(crate) struct DataLayoutBuilder {
    segments: Vec<Segment>,
    /// Running byte offset of the next segment within a row.
    row_len: usize,
    sentinels: Option<FloatSentinels>,
    very_long_strings: Vec<(String, u32)>,
    extended_case_count: Option<i64>,
    /// Subtype 13's short-to-long name map, needed only to resolve
    /// subtype 22, which keys by long name where subtype 14 keys by
    /// short name.
    long_names: Vec<(String, String)>,
    /// Subtype 22's per-variable missing values, keyed by long name.
    long_missing_values: Vec<(String, Vec<Box<[u8]>>)>,
}

impl DataLayoutBuilder {
    /// Records one type-2 primary record. Continuation records are
    /// already collapsed away by the time they reach here, so every
    /// call is a new segment.
    pub fn add_variable(
        &mut self,
        short_name: String,
        variable_type: VariableType,
        missing: RawMissingValues,
    ) {
        let layout = SegmentLayout::new(self.row_len, variable_type);
        self.row_len += layout.stride();
        let segment = Segment {
            short_name,
            layout,
            missing,
        };
        self.segments.push(segment);
    }

    /// Records the subtype-13 long variable names.
    pub fn set_long_variable_names(&mut self, names: &LongVariableNames) {
        self.long_names = names
            .mappings()
            .iter()
            .map(|entry| (entry.short_name().to_owned(), entry.long_name().to_owned()))
            .collect();
    }

    /// Records the subtype-22 very-long-string missing values.
    pub fn set_long_missing_values(&mut self, values: &LongMissingValues) {
        self.long_missing_values = values
            .records()
            .iter()
            .map(|record| {
                let entries = record
                    .values()
                    .iter()
                    .map(|value| value.clone().into_boxed_slice())
                    .collect();
                (record.variable_name().to_owned(), entries)
            })
            .collect();
    }

    /// Records the subtype-4 sentinel triple.
    pub fn set_sentinels(&mut self, sentinels: FloatSentinels) {
        self.sentinels = Some(sentinels);
    }

    /// Records the subtype-14 very-long-string widths.
    pub fn set_very_long_strings(&mut self, strings: &VeryLongStrings) {
        self.very_long_strings = strings
            .strings()
            .iter()
            .map(|entry| (entry.short_name().to_owned(), entry.width()))
            .collect();
    }

    /// Records the subtype-16 extended case count.
    pub fn set_extended_case_count(&mut self, count: i64) {
        self.extended_case_count = Some(count);
    }

    /// Groups segments into logical variables and finalizes the layout.
    ///
    /// Pushes a warning for every subtype-14 entry that cannot be
    /// reconciled; the affected variable is left uncollapsed rather than
    /// failing the read, since the type-2 records already define the row
    /// layout correctly either way.
    pub fn build(
        self,
        header: &SavHeader,
        encoding: &'static Encoding,
        warnings: &mut Vec<SavWarning>,
    ) -> DataLayout {
        let spans = self.resolve_spans(warnings);
        let sentinels = self.resolve_sentinels(header);
        let variables = self.resolve_variables(&spans, header.float_encoding(), &sentinels);
        let case_count = self.resolve_case_count(header, warnings);

        DataLayout {
            variables,
            row_len: self.row_len,
            compression: header.compression(),
            bias: header.bias(),
            float_encoding: header.float_encoding(),
            sentinels,
            encoding,
            case_count,
        }
    }

    /// Maps each logical variable to the run of segments it owns.
    ///
    /// Segments not claimed by a very-long-string entry stand alone, so
    /// a file with no subtype 14 comes out one span per segment.
    fn resolve_spans(&self, warnings: &mut Vec<SavWarning>) -> Vec<Span> {
        // Claimed[i] is the logical width of the variable starting at
        // segment i, for the very long strings that reconcile.
        let claimed = self.claim_segments(warnings);

        let mut spans = Vec::with_capacity(self.segments.len());
        let mut index = 0;
        while index < self.segments.len() {
            let (len, variable_type) = match claimed[index] {
                Some(width) => (segment_count(width), VariableType::String(width)),
                None => (1, self.segments[index].layout.variable_type()),
            };
            let span = Span {
                start: index,
                len,
                variable_type,
            };
            spans.push(span);
            index += len;
        }
        spans
    }

    /// The logical width of the variable starting at each segment, for
    /// the very long strings that reconcile against the records on disk.
    ///
    /// Subtype 14 declares its widths as unbounded ASCII decimals, so a
    /// width arrives here having survived nothing but its own parse. One
    /// wider than `u16::MAX` describes no string SAV can hold, and is
    /// rejected on the same terms as any other disagreement — narrowing
    /// it here is also what lets every downstream conversion be
    /// infallible.
    fn claim_segments(&self, warnings: &mut Vec<SavWarning>) -> Vec<Option<u16>> {
        let mut claimed: Vec<Option<u16>> = vec![None; self.segments.len()];
        for (short_name, declared_width) in &self.very_long_strings {
            let Some(start) = self.segment_index_of(short_name) else {
                let warning = SavWarning::UnknownVariableInExtension {
                    subtype: ExtensionSubtype::VeryLongStrings,
                    name: short_name.clone(),
                };
                warnings.push(warning);
                continue;
            };
            let width = u16::try_from(*declared_width)
                .ok()
                .filter(|width| self.segments_agree(start, *width));
            let Some(width) = width else {
                let warning = SavWarning::VeryLongStringSegmentMismatch {
                    short_name: short_name.clone(),
                    declared_width: *declared_width,
                };
                warnings.push(warning);
                continue;
            };
            claimed[start] = Some(width);
        }
        claimed
    }

    /// The segment declaring `short_name`, matched case-insensitively.
    /// Subtype 14 spells short names in upper case while the type-2 records may not.
    fn segment_index_of(&self, short_name: &str) -> Option<usize> {
        self.segments
            .iter()
            .position(|segment| segment.short_name.eq_ignore_ascii_case(short_name))
    }

    /// Whether the segments starting at `start` are exactly what a
    /// very long string of `width` should look like on disk.
    ///
    /// Checks the count and every declared width, so a subtype-14 record
    /// that disagrees with the dictionary is caught rather than silently
    /// mis-slicing the row. The last segment's width is checked with the
    /// tolerance [`segment_width_accepted`] documents.
    fn segments_agree(&self, start: usize, width: u16) -> bool {
        let count = segment_count(width);
        for offset in 0..count {
            let Some(expected) = segment_width(width, offset) else {
                return false;
            };
            let Some(index) = start.checked_add(offset) else {
                return false;
            };
            let Some(segment) = self.segments.get(index) else {
                return false;
            };
            let VariableType::String(actual) = segment.layout.variable_type() else {
                return false;
            };
            if !segment_width_accepted(expected, actual, offset + 1 == count) {
                return false;
            }
        }
        true
    }

    fn variable_layout(
        &self,
        span: &Span,
        encoding: FloatEncoding,
        sentinels: &FloatSentinels,
    ) -> VariableLayout {
        let owned = &self.segments[span.start..span.start + span.len];
        let segments = owned.iter().map(|segment| segment.layout).collect();
        let missing = self.resolve_missing(owned, span.variable_type, encoding, sentinels);
        VariableLayout::new(span.variable_type, segments, missing)
    }

    /// The missing values of the variable owning `segments`.
    ///
    /// A very long string declares its own in subtype 22 rather than in
    /// its type-2 record, because an eight-byte record slot cannot hold
    /// a key for a 300-wide variable — so that record wins where it
    /// exists. Everything else takes what its primary segment carried.
    ///
    /// A subtype-22 entry naming no variable in this file is simply not
    /// found here; the schema builder warns about it, and warning twice
    /// for one record would be noise.
    fn resolve_missing(
        &self,
        segments: &[Segment],
        variable_type: VariableType,
        encoding: FloatEncoding,
        sentinels: &FloatSentinels,
    ) -> MissingValueSpecification {
        let Some(primary) = segments.first() else {
            return MissingValueSpecification::None;
        };
        if let Some(values) = self.long_missing_values_for(&primary.short_name) {
            return MissingValueSpecification::String(values);
        }
        missing_value_reconcile::decode(&primary.missing, variable_type, encoding, sentinels)
    }

    /// Subtype 22's values for the variable whose *short* name is
    /// `short_name`, translating through subtype 13 because the two
    /// records key by different names.
    fn long_missing_values_for(&self, short_name: &str) -> Option<Vec<Box<[u8]>>> {
        let long_name = self
            .long_names
            .iter()
            .find(|(short, _)| short.eq_ignore_ascii_case(short_name))
            .map_or(short_name, |(_, long)| long.as_str());
        self.long_missing_values
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(long_name))
            .map(|(_, values)| values.clone())
    }

    fn resolve_variables(
        &self,
        spans: &[Span],
        encoding: FloatEncoding,
        sentinels: &FloatSentinels,
    ) -> Vec<VariableLayout> {
        spans
            .iter()
            .map(|span| self.variable_layout(span, encoding, sentinels))
            .collect()
    }

    fn resolve_sentinels(&self, header: &SavHeader) -> FloatSentinels {
        self.sentinels
            .unwrap_or_else(|| FloatSentinels::spss_defaults(header.float_encoding()))
    }

    /// Reconciles the header's 32-bit case count with the subtype-16
    /// extended one. The extended count wins, which is the whole point
    /// of the record; a disagreement warns.
    fn resolve_case_count(
        &self,
        header: &SavHeader,
        warnings: &mut Vec<SavWarning>,
    ) -> Option<u64> {
        let declared = header.case_count();
        let Some(extended) = self.extended_case_count else {
            return declared.map(u64::from);
        };
        let extended_count = u64::try_from(extended).ok();
        if let Some(declared) = declared
            && extended_count != Some(u64::from(declared))
        {
            let warning = SavWarning::CaseCountMismatch {
                header: declared,
                extended,
            };
            warnings.push(warning);
        }
        extended_count.or_else(|| declared.map(u64::from))
    }
}

/// A run of consecutive segments making up one logical variable.
struct Span {
    start: usize,
    len: usize,
    variable_type: VariableType,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spss::sav::byte_order::ByteOrder;
    use crate::spss::sav::extensions::very_long_string::VeryLongString;
    use crate::spss::sav::float_format::FloatFormat;

    fn header() -> SavHeader {
        SavHeader::builder()
            .byte_order(ByteOrder::LittleEndian)
            .float_format(FloatFormat::Ieee754)
            .build()
    }

    fn very_long_strings(entries: &[(&str, u32)]) -> VeryLongStrings {
        let strings = entries.iter().map(|(name, width)| {
            VeryLongString::builder()
                .short_name(*name)
                .width(*width)
                .build()
        });
        VeryLongStrings::builder().add_strings(strings).build()
    }

    /// Builds the layout for `segments`, returning it with any warnings.
    fn layout_of(
        segments: &[(&str, VariableType)],
        strings: Option<VeryLongStrings>,
    ) -> (DataLayout, Vec<SavWarning>) {
        let mut builder = DataLayoutBuilder::default();
        for (name, variable_type) in segments {
            builder.add_variable((*name).to_owned(), *variable_type, RawMissingValues::None);
        }
        if let Some(strings) = strings {
            builder.set_very_long_strings(&strings);
        }
        let mut warnings = Vec::new();
        let layout = builder.build(&header(), encoding_rs::UTF_8, &mut warnings);
        (layout, warnings)
    }

    #[test]
    fn offsets_and_row_length_follow_the_segment_strides() {
        let (layout, warnings) = layout_of(
            &[
                ("ID", VariableType::Numeric),
                ("NAME", VariableType::String(4)),
                ("WIDE", VariableType::String(9)),
            ],
            None,
        );
        assert!(warnings.is_empty());
        let offsets: Vec<usize> = layout
            .variables()
            .iter()
            .map(|v| v.segments()[0].offset())
            .collect();
        // 8 for the numeric, 8 for the width-4 string (padded), 16 for
        // the width-9 one.
        assert_eq!(offsets, [0, 8, 16]);
        assert_eq!(layout.row_len(), 32);
    }

    /// The comprehensive fixture's shape: `A300` stored as a 255-wide
    /// and a 48-wide segment. The two collapse into one variable whose
    /// declared width is the logical 300.
    #[test]
    fn a_declared_very_long_string_collapses_its_segments() {
        let (layout, warnings) = layout_of(
            &[
                ("LONGSTR", VariableType::String(255)),
                ("LONGST_A", VariableType::String(48)),
                ("SHORTSTR", VariableType::String(4)),
            ],
            Some(very_long_strings(&[("LONGSTR", 300)])),
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(layout.variables().len(), 2);

        let longstr = &layout.variables()[0];
        assert_eq!(longstr.variable_type(), VariableType::String(300));
        assert_eq!(longstr.segments().len(), 2);
        // Not contiguous: the 255-wide segment sits in 256 bytes.
        assert!(longstr.contiguous_range().is_none());
        assert_eq!(longstr.segments()[0].offset(), 0);
        assert_eq!(longstr.segments()[1].offset(), 256);
        // The logical width is less than what the segments supply.
        let supplied: usize = longstr.segments().iter().map(|s| s.content_len()).sum();
        assert_eq!(supplied, 303);
        assert_eq!(longstr.content_len(), 300);

        assert_eq!(layout.row_len(), 256 + 48 + 8);
    }

    /// PSPP's own worked example: a 20 000-wide string is 80 segments,
    /// 79 of them 255 wide and the last `20000 - 79 * 252 == 92` — but
    /// "some versions of SPSS make it slightly wider", up to 96, the
    /// next multiple of 8. Every spelling in that range has to collapse,
    /// and the row layout must come out identical, since the extra width
    /// lies inside padding the segment already occupies.
    #[test]
    fn a_last_segment_padded_to_the_next_unit_still_collapses() {
        for last in [92, 93, 96] {
            let mut segments: Vec<(String, VariableType)> = (0..79)
                .map(|index| (format!("WIDE_{index:02}"), VariableType::String(255)))
                .collect();
            segments.push(("WIDE_79".to_owned(), VariableType::String(last)));
            let borrowed: Vec<(&str, VariableType)> = segments
                .iter()
                .map(|(name, variable_type)| (name.as_str(), *variable_type))
                .collect();

            let (layout, warnings) =
                layout_of(&borrowed, Some(very_long_strings(&[("WIDE_00", 20_000)])));
            assert!(warnings.is_empty(), "last {last}: {warnings:?}");
            assert_eq!(layout.variables().len(), 1, "last {last}");
            let wide = &layout.variables()[0];
            assert_eq!(wide.variable_type(), VariableType::String(20_000));
            assert_eq!(wide.segments().len(), 80, "last {last}");
            assert_eq!(wide.content_len(), 20_000);
            // 79 segments of 256 bytes apiece, then the last one's 96 —
            // the same row whichever width the writer spelled.
            assert_eq!(layout.row_len(), 79 * 256 + 96, "last {last}");
        }
    }

    /// The tolerance stops at the unit boundary. A last segment one byte
    /// past it would need another 8 bytes of row, which is exactly what
    /// PSPP says cannot happen.
    #[test]
    fn a_last_segment_past_the_next_unit_is_still_a_mismatch() {
        let mut segments: Vec<(String, VariableType)> = (0..79)
            .map(|index| (format!("WIDE_{index:02}"), VariableType::String(255)))
            .collect();
        segments.push(("WIDE_79".to_owned(), VariableType::String(97)));
        let borrowed: Vec<(&str, VariableType)> = segments
            .iter()
            .map(|(name, variable_type)| (name.as_str(), *variable_type))
            .collect();

        let (layout, warnings) =
            layout_of(&borrowed, Some(very_long_strings(&[("WIDE_00", 20_000)])));
        assert_eq!(layout.variables().len(), 80, "left uncollapsed");
        assert!(
            matches!(
                warnings.as_slice(),
                [SavWarning::VeryLongStringSegmentMismatch { .. }],
            ),
            "{warnings:?}",
        );
    }

    /// A subtype-14 entry naming a variable the dictionary does not have
    /// warns and is dropped; nothing collapses.
    #[test]
    fn an_unknown_short_name_warns_and_collapses_nothing() {
        let (layout, warnings) = layout_of(
            &[("ID", VariableType::Numeric)],
            Some(very_long_strings(&[("GHOST", 300)])),
        );
        assert_eq!(layout.variables().len(), 1);
        assert!(
            matches!(
                warnings.as_slice(),
                [SavWarning::UnknownVariableInExtension {
                    subtype: ExtensionSubtype::VeryLongStrings,
                    name,
                }] if name == "GHOST",
            ),
            "{warnings:?}",
        );
    }

    /// The decision that keeps a disagreeing file readable: when the
    /// segments do not match the declared width, the variable is left
    /// uncollapsed rather than the read failing. The type-2 records
    /// already define the row layout, so the offsets stay correct.
    #[test]
    fn a_segment_mismatch_warns_and_leaves_the_variable_uncollapsed() {
        let (layout, warnings) = layout_of(
            &[
                // A 300-wide string needs segments of 255 and 48; this
                // file's second segment is the wrong width.
                ("LONGSTR", VariableType::String(255)),
                ("LONGST_A", VariableType::String(16)),
            ],
            Some(very_long_strings(&[("LONGSTR", 300)])),
        );
        assert_eq!(layout.variables().len(), 2, "must stay uncollapsed");
        assert_eq!(
            layout.variables()[0].variable_type(),
            VariableType::String(255),
        );
        assert_eq!(
            layout.variables()[1].variable_type(),
            VariableType::String(16),
        );
        // Offsets still describe the file as written.
        assert_eq!(layout.row_len(), 256 + 16);
        assert!(
            matches!(
                warnings.as_slice(),
                [SavWarning::VeryLongStringSegmentMismatch {
                    short_name,
                    declared_width: 300,
                }] if short_name == "LONGSTR",
            ),
            "{warnings:?}",
        );
    }

    /// Subtype 14 parses its widths from unbounded ASCII decimals, so a
    /// crafted file can declare one wider than any string SAV can hold —
    /// and can supply matching segments so the disagreement is not caught
    /// by shape alone. `99999` needs 397 segments of 255 plus a final
    /// 207, all of which this builds. The width still has to be rejected.
    #[test]
    fn a_width_too_wide_for_a_string_warns_rather_than_panicking() {
        let declared_width: u32 = 99_999;
        let mut segments: Vec<(&str, VariableType)> = vec![("LONGSTR", VariableType::String(255))];
        // 252 * 396 = 99_792, leaving 207 for the final segment.
        for _ in 1..396 {
            segments.push(("", VariableType::String(255)));
        }
        segments.push(("", VariableType::String(207)));

        let (layout, warnings) = layout_of(
            &segments,
            Some(very_long_strings(&[("LONGSTR", declared_width)])),
        );
        assert_eq!(
            layout.variables().len(),
            segments.len(),
            "stays uncollapsed"
        );
        assert!(
            matches!(
                warnings.as_slice(),
                [SavWarning::VeryLongStringSegmentMismatch {
                    short_name,
                    declared_width: 99_999,
                }] if short_name == "LONGSTR",
            ),
            "{warnings:?}",
        );
    }

    /// A declared width whose trailing segments are missing entirely is
    /// the same story — warn, do not collapse.
    #[test]
    fn a_truncated_segment_run_warns_and_leaves_the_variable_uncollapsed() {
        let (layout, warnings) = layout_of(
            &[("LONGSTR", VariableType::String(255))],
            Some(very_long_strings(&[("LONGSTR", 300)])),
        );
        assert_eq!(layout.variables().len(), 1);
        assert_eq!(
            layout.variables()[0].variable_type(),
            VariableType::String(255),
        );
        assert!(
            matches!(
                warnings.as_slice(),
                [SavWarning::VeryLongStringSegmentMismatch { .. }],
            ),
            "{warnings:?}",
        );
    }

    /// Short names are matched case-insensitively: subtype 14 spells
    /// them in upper case, and SPSS treats names that way anyway.
    #[test]
    fn short_names_match_case_insensitively() {
        let (layout, warnings) = layout_of(
            &[
                ("longstr", VariableType::String(255)),
                ("longst_a", VariableType::String(48)),
            ],
            Some(very_long_strings(&[("LONGSTR", 300)])),
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(layout.variables().len(), 1);
    }

    #[test]
    fn a_file_with_no_very_long_strings_is_one_variable_per_segment() {
        let (layout, warnings) = layout_of(
            &[
                ("A", VariableType::Numeric),
                ("B", VariableType::String(255)),
            ],
            None,
        );
        assert!(warnings.is_empty());
        assert_eq!(layout.variables().len(), 2);
        assert_eq!(layout.variables()[1].segments().len(), 1);
        assert_eq!(layout.variables()[1].contiguous_range(), Some(8..263));
    }

    /// The row source gets a strict subset of the layout: enough to
    /// fill a row, with no way to reach the variable table.
    #[test]
    fn row_coding_carries_what_a_row_source_needs() {
        let (layout, _) = layout_of(
            &[
                ("ID", VariableType::Numeric),
                ("NAME", VariableType::String(4)),
            ],
            None,
        );
        let coding = layout.row_coding();
        assert_eq!(coding.row_len(), layout.row_len());
        assert_eq!(coding.float_encoding(), layout.float_encoding());
        assert_eq!(
            coding.system_missing(),
            layout.sentinels().system_missing(),
            "command 255 writes exactly this pattern",
        );
        // The other two sentinels are declaration-side and no command
        // emits them, so they are deliberately absent.
        assert_ne!(coding.system_missing(), layout.sentinels().highest());
    }

    // ---- case count -------------------------------------------------

    fn case_count_of(
        header_count: Option<u32>,
        extended: Option<i64>,
    ) -> (Option<u64>, Vec<SavWarning>) {
        let mut builder = DataLayoutBuilder::default();
        if let Some(extended) = extended {
            builder.set_extended_case_count(extended);
        }
        let header = SavHeader::builder().case_count(header_count).build();
        let mut warnings = Vec::new();
        let layout = builder.build(&header, encoding_rs::UTF_8, &mut warnings);
        (layout.case_count(), warnings)
    }

    #[test]
    fn the_header_count_stands_when_there_is_no_extended_record() {
        let (count, warnings) = case_count_of(Some(2), None);
        assert_eq!(count, Some(2));
        assert!(warnings.is_empty());
    }

    #[test]
    fn an_unknown_header_count_stays_unknown() {
        let (count, warnings) = case_count_of(None, None);
        assert_eq!(count, None);
        assert!(warnings.is_empty());
    }

    /// The extended record exists precisely to carry a count too large
    /// for the header's 32-bit field, so it wins.
    #[test]
    fn the_extended_count_supersedes_the_header() {
        let (count, warnings) = case_count_of(None, Some(5_000_000_000));
        assert_eq!(count, Some(5_000_000_000));
        assert!(warnings.is_empty());
    }

    #[test]
    fn a_disagreement_warns_and_the_extended_count_still_wins() {
        let (count, warnings) = case_count_of(Some(2), Some(99));
        assert_eq!(count, Some(99));
        assert!(
            matches!(
                warnings.as_slice(),
                [SavWarning::CaseCountMismatch {
                    header: 2,
                    extended: 99,
                }],
            ),
            "{warnings:?}",
        );
    }

    #[test]
    fn agreement_between_the_two_counts_is_silent() {
        let (count, warnings) = case_count_of(Some(2), Some(2));
        assert_eq!(count, Some(2));
        assert!(warnings.is_empty());
    }
}
