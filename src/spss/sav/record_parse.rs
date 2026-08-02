//! Pure parse helpers for the SAV data-record section.
//!
//! Everything here operates on a row buffer that has already been
//! filled — by a straight read for an uncompressed file, or by the
//! bytecode decoder otherwise. Splitting a filled row into cells is the
//! same job under all three compression schemes.

use std::borrow::Cow;

use crate::spss::numeric::Numeric;
use crate::spss::sav::extensions::float_sentinels::FloatSentinels;
use crate::spss::sav::float_encoding::FloatEncoding;
use crate::spss::sav::segment_layout::DATA_UNIT_LEN;
use crate::spss::sav::text_field::trim_trailing_padding;
use crate::spss::sav::variable_layout::VariableLayout;

/// Classifies one numeric cell's eight on-disk bytes.
///
/// Comparison against the sentinels is **byte-exact**, not by decoded
/// value. It has to be: IBM HFP and VAX `D_float` carry more mantissa
/// bits than an `f64`, so their system-missing and `LOWEST` patterns
/// decode to the same number even though only one of them means
/// missing.
///
/// System-missing, `HIGHEST` and `LOWEST` all report as
/// [`MissingValue::System`](crate::spss::missing_value::MissingValue::System),
/// as does any NaN. `HIGHEST` and `LOWEST` are open-range markers that
/// belong in a missing-value declaration rather than a data cell, so a
/// cell holding one is already anomalous.
///
/// User-defined missing values are not detected here — see
/// [`Value`](crate::spss::sav::value::Value) for why they stay a
/// schema-level question.
#[allow(dead_code)] // wired up in Phase 6(a).
pub(crate) fn parse_numeric_cell(
    _bytes: [u8; DATA_UNIT_LEN],
    _encoding: FloatEncoding,
    _sentinels: &FloatSentinels,
) -> Numeric {
    todo!("body lands with Phase 6(a)")
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
#[allow(dead_code)] // wired up in Phase 6(a).
pub(crate) fn parse_string_cell<'a>(_row: &'a [u8], _layout: &VariableLayout) -> Cow<'a, [u8]> {
    let _ = trim_trailing_padding;
    todo!("body lands with Phase 6(a)")
}
