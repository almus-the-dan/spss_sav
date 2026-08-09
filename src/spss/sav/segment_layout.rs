//! On-disk placement of one variable's bytes within a data row.

use crate::spss::sav::variable_type::VariableType;

/// Number of bytes one 8-byte data unit holds.
///
/// Every value in a SAV data row occupies a whole number of these: a
/// numeric variable one, a string variable of declared width `w`
/// exactly `ceil(w / 8)`.
pub(crate) const DATA_UNIT_LEN: usize = 8;

/// The largest width a single string segment may declare.
///
/// A very long string is split across several dictionary variables;
/// every segment but the last declares this width.
pub(crate) const SEGMENT_WIDTH_MAX: u16 = 255;

/// Bytes of a very long string that each *earlier* segment accounts for
/// when the segment count is derived.
///
/// Deliberately **not** the number of bytes a segment contributes to the
/// value — that is its full declared width, 255. SPSS derives the
/// segment count and the final segment's declared width with 252 but
/// packs data at 255, so a reassembled value over-supplies by three
/// bytes per earlier segment and has to be truncated to the logical
/// width. Verified against PSPP output at widths 256, 300, 510, 765 and
/// 1000.
pub(crate) const SEGMENT_WIDTH_STRIDE: u16 = 252;

/// Where one variable's bytes sit in a data row, and how wide they are.
///
/// One `SegmentLayout` per **segment** — that is, per type-2 primary
/// record — not per logical variable. A very long string owns several
/// consecutive segments and reassembles its value from all of them;
/// everything else owns exactly one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SegmentLayout {
    /// Byte offset of this segment from the start of the row.
    offset: usize,
    /// The segment's declared width: `None` for a numeric variable,
    /// `Some(w)` for a string one.
    variable_type: VariableType,
}

impl SegmentLayout {
    pub fn new(offset: usize, variable_type: VariableType) -> Self {
        Self {
            offset,
            variable_type,
        }
    }

    /// Byte offset of this segment from the start of the row.
    #[inline]
    pub fn offset(self) -> usize {
        self.offset
    }

    /// This segment's declared storage type.
    #[inline]
    pub fn variable_type(self) -> VariableType {
        self.variable_type
    }

    /// Bytes this segment *occupies* on disk — its declared width
    /// rounded up to a whole number of 8-byte units.
    ///
    /// Larger than [`content_len`](Self::content_len) whenever the
    /// declared width is not a multiple of 8. A 255-wide segment is the
    /// case that matters: it occupies 256 bytes and contributes 255.
    #[inline]
    pub fn stride(self) -> usize {
        match self.variable_type {
            VariableType::Numeric => DATA_UNIT_LEN,
            VariableType::String(width) => {
                usize::from(width).div_ceil(DATA_UNIT_LEN) * DATA_UNIT_LEN
            }
        }
    }

    /// Bytes this segment *contributes* to its variable's value — its
    /// declared width, ignoring the padding that rounds it up to a unit
    /// boundary.
    #[inline]
    pub fn content_len(self) -> usize {
        match self.variable_type {
            VariableType::Numeric => DATA_UNIT_LEN,
            VariableType::String(width) => usize::from(width),
        }
    }
}

/// How many segments a string of logical `width` is stored across.
///
/// `1` for anything that fits in a single segment. Uses the 252-byte
/// stride, which is what SPSS derives the count from — see
/// [`SEGMENT_WIDTH_STRIDE`] for why that is not the same as the number
/// of bytes a segment contributes.
///
/// Taking a `u16` is what makes the result safe to hand back as a
/// `usize` with no fallible conversion: the widest string SAV can
/// declare needs at most `ceil(65535 / 252)` segments, which fits any
/// pointer width. A caller holding a wider declared width has a value
/// that describes no readable file and must reject it before asking.
#[must_use]
pub(crate) fn segment_count(width: u16) -> usize {
    if width <= SEGMENT_WIDTH_MAX {
        return 1;
    }
    usize::from(width).div_ceil(usize::from(SEGMENT_WIDTH_STRIDE))
}

/// The declared width of segment `index` of a string of logical
/// `width`, as it should appear in that segment's type-2 record.
///
/// Every segment but the last declares [`SEGMENT_WIDTH_MAX`]; the last
/// declares whatever the 252-byte stride leaves over.
///
/// Returns `None` when `index` is past the last segment. The arithmetic
/// below cannot overflow for an in-range index — `252 * index` is under
/// the width it is subtracted from, by the definition of the count.
/// It is written to fall out as `None` rather than to assert that,
/// because the only caller already reads `None` as "these segments do
/// not describe that width".
#[must_use]
pub(crate) fn segment_width(width: u16, index: usize) -> Option<u16> {
    let count = segment_count(width);
    if index >= count {
        return None;
    }
    if index + 1 < count {
        return Some(SEGMENT_WIDTH_MAX);
    }
    let index = u16::try_from(index).ok()?;
    let consumed = SEGMENT_WIDTH_STRIDE.checked_mul(index)?;
    width.checked_sub(consumed)
}

/// Whether `actual` — a segment's declared width, as its type-2 record
/// spells it — is acceptable where [`segment_width`] computes
/// `expected`.
///
/// Exact for every segment but the last. **The last is allowed to be
/// wider**, up to the next multiple of eight, because PSPP documents
/// that "the last segment has width `W - (N - 1) * 252`; some versions
/// of SPSS make it slightly wider, but not wide enough to make the last
/// segment require another 8 bytes of data" — its worked example of a
/// 20 000-wide string gives a last segment of 92 bytes "or slightly
/// wider (up to 96 bytes, the next multiple of 8)".
///
/// Accepting the wider spelling costs nothing and changes no offset.
/// The extra width lies inside padding the segment already occupies, so
/// [`stride`](SegmentLayout::stride) is identical either way and the row
/// layout does not move; reassembly truncates to the variable's logical
/// width regardless of how much each segment contributes. Demanding
/// equality, by contrast, would reject such a file's very long string
/// and leave it split across N variables.
#[must_use]
pub(crate) fn segment_width_accepted(expected: u16, actual: u16, is_last: bool) -> bool {
    if !is_last {
        return actual == expected;
    }
    let expected = usize::from(expected);
    let actual = usize::from(actual);
    let padded = expected.div_ceil(DATA_UNIT_LEN) * DATA_UNIT_LEN;
    (expected..=padded).contains(&actual)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_widths_need_one_segment() {
        for width in [1_u16, 8, 9, 100, 254, 255] {
            assert_eq!(segment_count(width), 1, "width {width}");
            assert_eq!(segment_width(width, 0), Some(width), "width {width}");
            assert_eq!(segment_width(width, 1), None);
        }
    }

    /// The widths measured against PSPP output. Each entry is
    /// `(logical width, declared segment widths)`.
    const MEASURED: &[(u16, &[u16])] = &[
        (256, &[255, 4]),
        (300, &[255, 48]),
        (510, &[255, 255, 6]),
        (765, &[255, 255, 255, 9]),
        (1000, &[255, 255, 255, 244]),
    ];

    #[test]
    fn segment_widths_match_pspp_output() {
        for &(width, expected) in MEASURED {
            let count = segment_count(width);
            assert_eq!(count, expected.len(), "segment count for width {width}");
            let actual: Vec<Option<u16>> = (0..count).map(|i| segment_width(width, i)).collect();
            let expected: Vec<Option<u16>> = expected.iter().copied().map(Some).collect();
            assert_eq!(actual, expected, "segment widths for width {width}");
        }
    }

    /// The declared widths sum to *more* than the logical width — three
    /// bytes per earlier segment — which is exactly why reassembly has
    /// to truncate rather than trust the sum.
    #[test]
    fn declared_widths_over_supply_by_three_per_earlier_segment() {
        for &(width, expected) in MEASURED {
            let total: usize = expected.iter().copied().map(usize::from).sum();
            let earlier = expected.len() - 1;
            assert_eq!(total, usize::from(width) + 3 * earlier, "width {width}");
        }
    }

    #[test]
    fn a_255_wide_segment_occupies_256_bytes_and_contributes_255() {
        let segment = SegmentLayout::new(0, VariableType::String(255));
        assert_eq!(segment.stride(), 256);
        assert_eq!(segment.content_len(), 255);
    }

    #[test]
    fn numeric_segments_occupy_and_contribute_one_unit() {
        let segment = SegmentLayout::new(16, VariableType::Numeric);
        assert_eq!(segment.stride(), DATA_UNIT_LEN);
        assert_eq!(segment.content_len(), DATA_UNIT_LEN);
        assert_eq!(segment.offset(), 16);
    }

    #[test]
    fn string_strides_round_up_to_a_unit_boundary() {
        for (width, stride) in [
            (1_u16, 8_usize),
            (4, 8),
            (8, 8),
            (9, 16),
            (48, 48),
            (250, 256),
        ] {
            let segment = SegmentLayout::new(0, VariableType::String(width));
            assert_eq!(segment.stride(), stride, "width {width}");
            assert_eq!(segment.content_len(), usize::from(width));
        }
    }
}
