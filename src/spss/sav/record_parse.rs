//! Pure parse helpers for the SAV data-record section.
//!
//! Everything here operates on a row buffer that has already been
//! filled — by a straight read for an uncompressed file, or by the
//! bytecode decoder otherwise. Splitting a filled row into cells is the
//! same job under all three compression schemes.

use std::borrow::Cow;

use crate::spss::missing_value::MissingValue;
use crate::spss::numeric::Numeric;
use crate::spss::sav::data_layout::DataLayout;
use crate::spss::sav::float_encoding::FloatEncoding;
use crate::spss::sav::missing_value_specification::MissingValueSpecification;
use crate::spss::sav::segment_layout::DATA_UNIT_LEN;
use crate::spss::sav::string_value::StringValue;
use crate::spss::sav::text::Text;
use crate::spss::sav::text_field::trim_trailing_padding;
use crate::spss::sav::value::Value;
use crate::spss::sav::variable_layout::VariableLayout;
use crate::spss::sav::variable_type::VariableType;

/// Splits the cell at `index` out of a filled row.
///
/// Takes the layout and an index rather than a
/// [`VariableLayout`](crate::spss::sav::variable_layout::VariableLayout)
/// alongside it: a variable only means anything relative to the layout
/// it came from, and a signature accepting both invites pairing one
/// file's variable with another file's layout — which would decode
/// something plausible and wrong.
///
/// `None` when `index` is past the last variable, or — unreachably —
/// when `row` is shorter than the layout says it is. Every row source
/// fills exactly [`DataLayout::row_len`] bytes, so the second case is a
/// library bug; returning an `Option` rather than indexing keeps it from
/// becoming a panic in a caller's read loop.
pub(crate) fn parse_cell<'a>(
    layout: &DataLayout,
    row: &'a [u8],
    index: usize,
) -> Option<Value<'a>> {
    let variable = layout.variables().get(index)?;
    match variable.variable_type() {
        VariableType::Numeric => {
            let offset = variable.segments().first()?.offset();
            let bytes = *row.get(offset..)?.first_chunk::<DATA_UNIT_LEN>()?;
            let numeric = parse_numeric_cell(
                bytes,
                layout.float_encoding(),
                layout.sentinels().system_missing(),
                variable.missing(),
            );
            let value = Value::Numeric(numeric);
            Some(value)
        }
        VariableType::String(_) => {
            let raw = parse_string_cell(row, variable)?;
            let string_value = StringValue::new(raw, layout.encoding());
            let text = Text::classify(string_value, variable.missing());
            let value = Value::String(text);
            Some(value)
        }
    }
}

/// Classifies one numeric cell's eight on-disk bytes.
///
/// **Only the system-missing pattern marks a cell missing.** The other
/// two sentinels in the subtype-4 triple, `LOWEST` and `HIGHEST`, are
/// the open endpoints of a missing-value *range declaration* — see
/// [`RangeBound::Unbounded`](crate::spss::sav::range_bound::RangeBound::Unbounded)
/// and `missing_value_reconcile`, which is where they are actually
/// consumed. In a data cell they are ordinary numbers.
///
/// Verified against PSPP by patching each pattern into a cell of
/// `compression_none.sav` and running `pspp-convert`: system-missing
/// came back blank, while `LOWEST`, `HIGHEST`, NaN and infinity all
/// came back as their values. `ReadStat`'s `sav_tag_missing_double`
/// reports all four as system-missing; we deliberately do not follow it
/// here, because three of those four would silently turn a legitimate
/// value into a missing one and there is no way for a caller to get it
/// back.
///
/// Comparison is **byte-exact**, not by decoded value. It has to be:
/// IBM HFP and VAX `D_float` carry more mantissa bits than an `f64`, so
/// their system-missing and `LOWEST` patterns decode to the same number
/// even though only one of them means missing.
///
/// Takes the one pattern rather than the whole
/// [`FloatSentinels`](crate::spss::sav::extensions::float_sentinels::FloatSentinels)
/// triple so that reaching for `LOWEST` or `HIGHEST` here is not
/// possible. That was a real bug, not a hypothetical one.
///
/// User-defined missing values are not detected here — see [`Value`]
/// for why they stay a schema-level question.
pub(crate) fn parse_numeric_cell(
    bytes: [u8; DATA_UNIT_LEN],
    encoding: FloatEncoding,
    system_missing: [u8; DATA_UNIT_LEN],
    missing: &MissingValueSpecification,
) -> Numeric {
    if bytes == system_missing {
        return Numeric::Missing(MissingValue::System);
    }
    let decoded = encoding.decode(bytes);
    if missing.matches_number(decoded) {
        let missing_value = MissingValue::UserDefined(decoded);
        return Numeric::Missing(missing_value);
    }
    Numeric::Present(decoded)
}

/// Extracts one string cell's bytes from a filled row buffer.
///
/// Borrows the row for a variable held in a single segment, which is
/// every string but a very long one. A very long string is reassembled
/// into an owned buffer, because its segments are not contiguous: each
/// is padded up to a unit boundary, and the run collectively
/// over-supplies by three bytes per earlier segment, so the joined
/// bytes are truncated to the variable's logical width.
///
/// Trailing spaces and NULs come off either way. SAV pads every string
/// cell out to its declared width and cannot distinguish that padding
/// from content, so nothing recoverable is lost — and both PSPP and
/// `ReadStat` trim.
pub(crate) fn parse_string_cell<'a>(
    row: &'a [u8],
    variable: &VariableLayout,
) -> Option<Cow<'a, [u8]>> {
    if let Some(range) = variable.contiguous_range() {
        let cell = row.get(range)?;
        let value = trim_trailing_padding(cell);
        let value = Cow::Borrowed(value);
        return Some(value);
    }
    let mut joined = Vec::with_capacity(variable.content_len());
    for segment in variable.segments() {
        let start = segment.offset();
        let end = start.checked_add(segment.content_len())?;
        joined.extend_from_slice(row.get(start..end)?);
    }
    // The declared segment widths over-supply by three bytes apiece, so
    // the logical width is the authority on where the value ends.
    joined.truncate(variable.content_len());
    joined.truncate(trim_trailing_padding(&joined).len());
    let value = Cow::Owned(joined);
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spss::sav::byte_order::ByteOrder;
    use crate::spss::sav::extensions::float_sentinels::FloatSentinels;
    use crate::spss::sav::float_format::FloatFormat;
    use crate::spss::sav::segment_layout::SegmentLayout;

    fn ieee() -> FloatEncoding {
        FloatEncoding::new(FloatFormat::Ieee754, ByteOrder::LittleEndian)
    }

    fn sentinels() -> FloatSentinels {
        FloatSentinels::spss_defaults(ieee())
    }

    fn numeric(bytes: [u8; 8]) -> Numeric {
        numeric_with(bytes, &MissingValueSpecification::None)
    }

    fn numeric_with(bytes: [u8; 8], missing: &MissingValueSpecification) -> Numeric {
        parse_numeric_cell(bytes, ieee(), sentinels().system_missing(), missing)
    }

    #[test]
    fn an_ordinary_double_is_present() {
        assert_eq!(numeric(1.5_f64.to_le_bytes()), Numeric::Present(1.5));
        assert_eq!(numeric(0.0_f64.to_le_bytes()), Numeric::Present(0.0));
        assert_eq!(
            numeric((-999_999.0_f64).to_le_bytes()),
            Numeric::Present(-999_999.0),
        );
    }

    #[test]
    fn the_system_missing_pattern_reads_as_missing() {
        let bytes = sentinels().system_missing();
        assert_eq!(numeric(bytes), Numeric::Missing(MissingValue::System));
    }

    /// `LOWEST` and `HIGHEST` mark the open ends of a missing-value
    /// *range declaration*; in a cell they are just numbers. Confirmed
    /// against PSPP, which renders both as their values — reporting
    /// them missing would destroy legitimate data.
    #[test]
    fn range_endpoint_sentinels_are_ordinary_data_in_a_cell() {
        let sentinels = sentinels();
        for bytes in [sentinels.lowest(), sentinels.highest()] {
            let expected = ieee().decode(bytes);
            assert_eq!(numeric(bytes), Numeric::Present(expected), "{bytes:02X?}");
        }
    }

    /// PSPP renders a NaN cell as `nan` and an infinity as `inf`, so
    /// both are values. `ReadStat` reports NaN as system-missing; we
    /// follow PSPP.
    #[test]
    fn nan_and_infinity_cells_are_data() {
        let Numeric::Present(nan) = numeric(f64::NAN.to_le_bytes()) else {
            panic!("a NaN cell must be present");
        };
        assert!(nan.is_nan());
        assert_eq!(
            numeric(f64::INFINITY.to_le_bytes()),
            Numeric::Present(f64::INFINITY),
        );
    }

    /// The test is on bytes, not on the decoded number, so the value
    /// one ulp from system-missing — which is exactly `LOWEST` — and
    /// the one beyond it are both data.
    #[test]
    fn values_neighbouring_system_missing_stay_present() {
        for step in 1..=2_u64 {
            let near = f64::from_bits(f64::MIN.to_bits() - step);
            assert_eq!(numeric(near.to_le_bytes()), Numeric::Present(near));
        }
    }

    fn string_variable(width: u16, offset: usize) -> VariableLayout {
        let variable_type = VariableType::String(width);
        VariableLayout::new(
            variable_type,
            vec![SegmentLayout::new(offset, variable_type)],
            MissingValueSpecification::None,
        )
    }

    #[test]
    fn a_short_string_borrows_the_row_and_trims_padding() {
        let row = b"aa      ".to_vec();
        let cell = parse_string_cell(&row, &string_variable(8, 0)).unwrap();
        assert_eq!(cell.as_ref(), b"aa");
        assert!(matches!(cell, Cow::Borrowed(_)), "must not allocate");
    }

    #[test]
    fn an_all_padding_string_is_empty() {
        let row = b"        ".to_vec();
        let cell = parse_string_cell(&row, &string_variable(8, 0)).unwrap();
        assert!(cell.is_empty());
    }

    #[test]
    fn interior_spaces_survive() {
        let row = b"a b     ".to_vec();
        let cell = parse_string_cell(&row, &string_variable(8, 0)).unwrap();
        assert_eq!(cell.as_ref(), b"a b");
    }

    fn very_long_string() -> VariableLayout {
        VariableLayout::new(
            VariableType::String(300),
            vec![
                SegmentLayout::new(0, VariableType::String(255)),
                SegmentLayout::new(256, VariableType::String(48)),
            ],
            MissingValueSpecification::None,
        )
    }

    /// The reassembly rule that matters: a 300-wide string is stored as
    /// a 255-wide segment occupying 256 bytes plus a 48-wide one, so the
    /// join has to skip the padding byte between them.
    #[test]
    fn a_very_long_string_reassembles_across_its_segments() {
        let mut row = vec![b' '; 304];
        row[..5].copy_from_slice(b"alpha");
        // A marker at the start of the second segment proves the join
        // reaches it, and lands it right after the first segment's 255
        // content bytes rather than after its 256-byte stride.
        row[256] = b'Z';

        let cell = parse_string_cell(&row, &very_long_string()).unwrap();
        assert!(matches!(cell, Cow::Owned(_)), "segmented values are owned");
        assert_eq!(cell.len(), 256, "255 content bytes, then the marker");
        assert_eq!(&cell[..5], b"alpha");
        assert_eq!(cell[255], b'Z');
    }

    /// The declared widths over-supply by three bytes per earlier
    /// segment, so without the truncate the tail would leak in.
    #[test]
    fn reassembly_stops_at_the_logical_width() {
        let row = vec![b'x'; 304];
        let cell = parse_string_cell(&row, &very_long_string()).unwrap();
        assert_eq!(cell.len(), 300, "303 bytes supplied, 300 kept");
    }

    #[test]
    fn a_row_shorter_than_the_layout_yields_none() {
        let row = vec![0_u8; 4];
        assert!(parse_string_cell(&row, &string_variable(8, 0)).is_none());
    }
}
