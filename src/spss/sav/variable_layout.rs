//! On-disk placement of one logical variable's bytes within a data row.

use std::ops::Range;

use crate::spss::sav::missing_value_specification::MissingValueSpecification;
use crate::spss::sav::segment_layout::SegmentLayout;
use crate::spss::sav::variable_type::VariableType;

/// Everything the record reader needs to pull one logical variable's
/// value out of a data row.
///
/// "Logical" means post-collapse: a very long string is one
/// `VariableLayout` spanning several
/// [`SegmentLayout`]s, matching how
/// [`SavSchema`](crate::spss::sav::sav_schema::SavSchema) presents it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct VariableLayout {
    /// The variable's logical storage type — for a very long string,
    /// the width the file declared in subtype 14, not any segment's.
    variable_type: VariableType,
    /// Which segments hold this variable's bytes, in order. Always at
    /// least one.
    segments: Vec<SegmentLayout>,
    /// The values this variable declares missing.
    ///
    /// Lives on the layout rather than only on
    /// [`SavVariable`](crate::spss::sav::sav_variable::SavVariable)
    /// because a decoded row must not report a declared-missing cell as
    /// present, which makes the declaration something a *correct read*
    /// depends on rather than presentation. Everything the rows need is
    /// reachable without touching the schema.
    missing: MissingValueSpecification,
}

impl VariableLayout {
    pub fn new(
        variable_type: VariableType,
        segments: Vec<SegmentLayout>,
        missing: MissingValueSpecification,
    ) -> Self {
        debug_assert!(!segments.is_empty(), "a variable owns at least one segment");
        Self {
            variable_type,
            segments,
            missing,
        }
    }

    /// The values this variable declares missing.
    #[inline]
    pub fn missing(&self) -> &MissingValueSpecification {
        &self.missing
    }

    /// The variable's logical storage type.
    #[inline]
    pub fn variable_type(&self) -> VariableType {
        self.variable_type
    }

    /// The segments holding this variable's bytes, in order.
    #[inline]
    pub fn segments(&self) -> &[SegmentLayout] {
        &self.segments
    }

    /// `true` when this variable's value is spread across more than one
    /// segment and must be reassembled.
    #[allow(dead_code)] // exercised once row decoding lands.
    #[inline]
    pub fn is_segmented(&self) -> bool {
        self.segments.len() > 1
    }

    /// Byte range of this variable's value within a row, for the common
    /// single-segment case.
    ///
    /// `None` for a very long string, whose bytes are not contiguous —
    /// each segment is padded up to a unit boundary and the last one
    /// over-supplies. Those must go through
    /// [`segments`](Self::segments).
    #[allow(dead_code)] // exercised once row decoding lands.
    #[inline]
    pub fn contiguous_range(&self) -> Option<Range<usize>> {
        let [segment] = self.segments[..] else {
            return None;
        };
        let start = segment.offset();
        Some(start..start + segment.content_len())
    }

    /// Number of bytes this variable's reassembled value holds before
    /// trailing padding is trimmed — its logical width, which is *less*
    /// than the sum of its segments' contributions when segmented.
    #[allow(dead_code)] // exercised once row decoding lands.
    #[inline]
    pub fn content_len(&self) -> usize {
        match self.variable_type {
            VariableType::Numeric => crate::spss::sav::segment_layout::DATA_UNIT_LEN,
            VariableType::String(width) => usize::from(width),
        }
    }
}
