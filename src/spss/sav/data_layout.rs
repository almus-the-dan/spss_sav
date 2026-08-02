//! Everything the record reader needs to decode a data row.

use encoding_rs::Encoding;

use crate::spss::sav::compression::Compression;
use crate::spss::sav::extensions::extension_subtype::ExtensionSubtype;
use crate::spss::sav::extensions::float_sentinels::FloatSentinels;
use crate::spss::sav::extensions::very_long_strings::VeryLongStrings;
use crate::spss::sav::float_encoding::FloatEncoding;
use crate::spss::sav::sav_header::SavHeader;
use crate::spss::sav::sav_warning::SavWarning;
use crate::spss::sav::segment_layout::{SegmentLayout, segment_count, segment_width};
use crate::spss::sav::variable_layout::VariableLayout;
use crate::spss::sav::variable_type::VariableType;

/// The complete, self-contained description of a SAV file's data
/// section.
///
/// Deliberately separate from
/// [`SavSchema`](crate::spss::sav::sav_schema::SavSchema), which is
/// presentation — names, labels, value labels, display parameters, the
/// things a data read never consults. Splitting them turns an invariant
/// that would otherwise be a doc comment into a structural guarantee:
/// `DataLayout` is derived from a skeleton the buffering pass retains
/// rather than from the records as they stream, so no combination of
/// [`skip_dictionary_content`](crate::spss::sav::sav_reader::SavReader::skip_dictionary_content),
/// [`build_schema(false)`](crate::spss::sav::sav_reader::SavReader::build_schema)
/// or
/// [`skip_record`](crate::spss::sav::dictionary_reader::DictionaryReader::skip_record)
/// can leave the record reader unable to do its job.
///
/// It is small — a few words per variable — so it is copied into the
/// record reader rather than borrowed.
#[derive(Debug, Clone)]
pub(crate) struct DataLayout {
    variables: Vec<VariableLayout>,
    row_len: usize,
    compression: Compression,
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
    #[allow(dead_code)] // exercised once row decoding lands.
    #[inline]
    pub fn row_len(&self) -> usize {
        self.row_len
    }

    /// How the data section is compressed.
    #[allow(dead_code)] // exercised once row decoding lands.
    #[inline]
    pub fn compression(&self) -> Compression {
        self.compression
    }

    /// Bytecode-compression bias, from the header.
    #[allow(dead_code)] // exercised once row decoding lands.
    #[inline]
    pub fn bias(&self) -> f64 {
        self.bias
    }

    /// How this file encodes an `f64` on disk.
    #[inline]
    pub fn float_encoding(&self) -> FloatEncoding {
        self.float_encoding
    }

    /// The sentinel values marking system-missing, highest and lowest,
    /// from subtype 4 when the file carried one and the format's
    /// canonical triple otherwise.
    #[inline]
    pub fn sentinels(&self) -> &FloatSentinels {
        &self.sentinels
    }

    /// The resolved text encoding for string cells.
    #[allow(dead_code)] // exercised once row decoding lands.
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
}

impl DataLayoutBuilder {
    /// Records one type-2 primary record. Continuation records are
    /// already collapsed away by the time they reach here, so every
    /// call is a new segment.
    pub fn add_variable(&mut self, short_name: String, variable_type: VariableType) {
        let layout = SegmentLayout::new(self.row_len, variable_type);
        self.row_len += layout.stride();
        self.segments.push(Segment { short_name, layout });
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
        let variables = self.resolve_variables(&spans);
        let sentinels = self.resolve_sentinels(header);
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
    /// mis-slicing the row.
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
            if segment.layout.variable_type() != VariableType::String(expected) {
                return false;
            }
        }
        true
    }

    fn variable_layout(&self, span: &Span) -> VariableLayout {
        let segments = self.segments[span.start..span.start + span.len]
            .iter()
            .map(|segment| segment.layout)
            .collect();
        VariableLayout::new(span.variable_type, segments)
    }

    fn resolve_variables(&self, spans: &[Span]) -> Vec<VariableLayout> {
        spans
            .iter()
            .map(|span| self.variable_layout(span))
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
        let strings = entries
            .iter()
            .map(|(name, width)| {
                VeryLongString::builder()
                    .short_name(*name)
                    .width(*width)
                    .build()
            })
            .collect();
        VeryLongStrings::builder().add_strings(strings).build()
    }

    /// Builds the layout for `segments`, returning it with any warnings.
    fn layout_of(
        segments: &[(&str, VariableType)],
        strings: Option<VeryLongStrings>,
    ) -> (DataLayout, Vec<SavWarning>) {
        let mut builder = DataLayoutBuilder::default();
        for (name, variable_type) in segments {
            builder.add_variable((*name).to_owned(), *variable_type);
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
        assert!(longstr.is_segmented());
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
        assert!(!layout.variables()[1].is_segmented());
        assert_eq!(layout.variables()[1].contiguous_range(), Some(8..263));
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
