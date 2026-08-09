//! Turning a bytecode command stream into row bytes.
//!
//! Shared by `$FL2` compressed files and ZSAV — see
//! [`compression`](crate::spss::sav::compression) for why those are the
//! same decoder, and
//! [`record_format`](crate::spss::sav::record_format) for the command
//! codes and for why a row boundary is not a stream boundary.

use std::io::Read;

use crate::spss::sav::compression::data_unit_source::DataUnitSource;
use crate::spss::sav::compression::row_coding::RowCoding;
use crate::spss::sav::reader_state::ReaderState;
use crate::spss::sav::record_format::{
    COMMAND_ALL_SPACES, COMMAND_END_OF_DATA, COMMAND_GROUP_LEN, COMMAND_PADDING,
    COMMAND_SYSTEM_MISSING, COMMAND_VERBATIM, EIGHT_SPACES,
};
use crate::spss::sav::sav_error::{FormatErrorKind, Result, SavError, Section};
use crate::spss::sav::sav_warning::SavWarning;
use crate::spss::sav::segment_layout::DATA_UNIT_LEN;

/// Decodes the bytecode command stream into row bytes.
///
/// Holds the partially-consumed command group across calls, which is
/// what makes straddling row boundaries work: a group's commands can
/// finish one row and begin the next.
#[derive(Debug)]
pub(crate) struct BytecodeDecoder {
    /// The command group currently being executed.
    group: [u8; COMMAND_GROUP_LEN],
    /// How many of `group`'s commands have run. Equal to
    /// [`COMMAND_GROUP_LEN`] when the next call must fetch a new group.
    consumed: usize,
    /// Set once the stream has ended, by marker or by exhaustion.
    finished: bool,
}

impl Default for BytecodeDecoder {
    /// Starts with no group in hand, which `consumed` records by being
    /// already exhausted.
    ///
    /// Written out rather than derived because a zeroed `consumed` would
    /// claim the zeroed `group` is a real group with all eight commands
    /// still to run. Those commands are [`COMMAND_PADDING`], so it would
    /// decode the same bytes and the mistake would never show — which is
    /// the reason to not leave it to chance.
    fn default() -> Self {
        Self {
            group: [COMMAND_PADDING; COMMAND_GROUP_LEN],
            consumed: COMMAND_GROUP_LEN,
            finished: false,
        }
    }
}

impl BytecodeDecoder {
    /// Fills `row` with exactly `coding.row_len()` decoded bytes.
    ///
    /// Returns `false` when the stream ended before a full row could be
    /// produced, which is the normal end of the data section. A row
    /// that ends *partway* through is a truncated file and errors.
    ///
    /// Two commands synthesize values rather than copying them, which
    /// is why [`RowCoding`] carries more than a length: an inline code
    /// writes `code - bias` and must lay that `f64` out in the file's
    /// own float format and byte order, and the system-missing command
    /// writes the file's sentinel bit pattern. Every inline value is a
    /// small integer, so the encoding cannot fail in practice even for
    /// the formats whose range is narrow.
    ///
    /// Note what is *not* reachable from here: the command stream has no
    /// variable boundaries, so a row ends purely on the output byte
    /// count and a group carries across it.
    pub fn fill_row<R: Read, S: DataUnitSource<R>>(
        &mut self,
        source: &mut S,
        state: &mut ReaderState<R>,
        coding: RowCoding,
        row: &mut Vec<u8>,
    ) -> Result<bool> {
        debug_assert_eq!(
            coding.row_len() % DATA_UNIT_LEN,
            0,
            "every command emits a whole data unit, so a row must be a whole number of them",
        );
        // Clearing rather than resizing: a command appends a unit at a
        // time, and the capacity is what carries across rows.
        row.clear();
        if self.finished {
            return Ok(false);
        }
        while row.len() < coding.row_len() {
            let Some(command) = self.next_command(source, state)? else {
                self.finished = true;
                // Nothing produced means the stream ended on a row
                // boundary, which is how a data section ordinarily ends.
                // Anything else is a file that stops mid-row.
                if row.is_empty() {
                    return Ok(false);
                }
                return Err(truncated_row(state, coding, row));
            };
            match command {
                COMMAND_PADDING => {}
                COMMAND_END_OF_DATA => {
                    self.finished = true;
                    if !row.is_empty() {
                        state
                            .warnings_mut()
                            .push(end_of_data_inside_row(coding, row));
                    }
                    return Ok(false);
                }
                COMMAND_VERBATIM => {
                    // The payload comes from the *stream*, not the file:
                    // under ZSAV the two are different things, and taking
                    // it from the file would read compressed bytes.
                    let Some(unit) = source.next_unit(state)? else {
                        self.finished = true;
                        return Err(truncated_row(state, coding, row));
                    };
                    row.extend_from_slice(&unit);
                }
                COMMAND_ALL_SPACES => row.extend_from_slice(&EIGHT_SPACES),
                COMMAND_SYSTEM_MISSING => row.extend_from_slice(&coding.system_missing()),
                code => {
                    let value = f64::from(code) - coding.bias();
                    row.extend_from_slice(&coding.float_encoding().encode(value)?);
                }
            }
        }
        Ok(true)
    }

    /// The next command byte, fetching a fresh group when the current
    /// one has run out.
    ///
    /// `None` once the stream is exhausted. Note that a group is fetched
    /// only when its predecessor is spent, never at the start of a row:
    /// that is the whole of what makes a group carry across a row
    /// boundary.
    fn next_command<R: Read, S: DataUnitSource<R>>(
        &mut self,
        source: &mut S,
        state: &mut ReaderState<R>,
    ) -> Result<Option<u8>> {
        if self.consumed == COMMAND_GROUP_LEN {
            let Some(group) = source.next_unit(state)? else {
                return Ok(None);
            };
            self.group = group;
            self.consumed = 0;
        }
        let command = self.group[self.consumed];
        self.consumed += 1;
        Ok(Some(command))
    }
}

/// The error for a command stream that ran out partway through a row.
fn truncated_row<R>(state: &ReaderState<R>, coding: RowCoding, row: &[u8]) -> SavError {
    let kind = FormatErrorKind::Truncated {
        expected: as_u64(coding.row_len()),
        actual: as_u64(row.len()),
    };
    SavError::format(Section::Records, state.position(), kind)
}

/// The warning for a `252` that arrived with a row half-produced.
fn end_of_data_inside_row(coding: RowCoding, row: &[u8]) -> SavWarning {
    SavWarning::EndOfDataInsideRow {
        bytes_produced: as_u64(row.len()),
        row_len: as_u64(coding.row_len()),
    }
}

/// Widens a byte count for reporting. Saturates rather than failing:
/// this only ever describes a length already held in memory, on a path
/// that is already reporting something else.
fn as_u64(len: usize) -> u64 {
    u64::try_from(len).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::spss::sav::byte_order::ByteOrder;
    use crate::spss::sav::compression::file_units::FileUnits;
    use crate::spss::sav::float_encoding::FloatEncoding;
    use crate::spss::sav::float_format::FloatFormat;
    use crate::spss::sav::record_format::{COMMAND_INLINE_MAX, COMMAND_INLINE_MIN};

    /// The canonical bias, so a command code reads as `code - 100`.
    const BIAS: f64 = 100.0;

    /// The system-missing pattern of an IEEE little-endian file.
    fn system_missing() -> [u8; DATA_UNIT_LEN] {
        (-f64::MAX).to_le_bytes()
    }

    /// A row of `units` eight-byte values, decoded as an IEEE
    /// little-endian file with the canonical bias.
    fn coding(units: usize) -> RowCoding {
        coding_with_bias(units, BIAS)
    }

    fn coding_with_bias(units: usize, bias: f64) -> RowCoding {
        RowCoding::new(
            units * DATA_UNIT_LEN,
            bias,
            FloatEncoding::new(FloatFormat::Ieee754, ByteOrder::LittleEndian),
            system_missing(),
        )
    }

    /// Reads the numeric units of a row back as numbers, so assertions
    /// read as values rather than as bit patterns.
    fn numbers(row: &[u8]) -> Vec<f64> {
        row.chunks_exact(DATA_UNIT_LEN)
            .map(|unit| f64::from_le_bytes(unit.try_into().expect("eight bytes")))
            .collect()
    }

    /// Drives the decoder over an in-memory command stream.
    struct Harness {
        state: ReaderState<Cursor<Vec<u8>>>,
        units: FileUnits,
        decoder: BytecodeDecoder,
        row: Vec<u8>,
    }

    impl Harness {
        fn new(stream: Vec<u8>) -> Self {
            Self {
                state: ReaderState::new(Cursor::new(stream)),
                units: FileUnits,
                decoder: BytecodeDecoder::default(),
                row: Vec::new(),
            }
        }

        /// The next row, or `None` at the end of the stream.
        fn next_row(&mut self, coding: RowCoding) -> Result<Option<Vec<u8>>> {
            let filled =
                self.decoder
                    .fill_row(&mut self.units, &mut self.state, coding, &mut self.row)?;
            Ok(filled.then(|| self.row.clone()))
        }

        /// Every row the stream yields, stopping at a clean end.
        fn rows(&mut self, coding: RowCoding) -> Vec<Vec<u8>> {
            let mut rows = Vec::new();
            while let Some(row) = self.next_row(coding).expect("decode a row") {
                rows.push(row);
            }
            rows
        }

        fn warnings(&self) -> &[SavWarning] {
            self.state.warnings()
        }
    }

    /// An inline code stands for `code - bias`, and the four special
    /// codes are excluded from that range.
    #[test]
    fn inline_codes_decode_to_their_biased_value() {
        let stream = vec![101, COMMAND_INLINE_MIN, COMMAND_INLINE_MAX, 100, 0, 0, 0, 0];
        let rows = Harness::new(stream).rows(coding(1));
        let values: Vec<f64> = rows.iter().flat_map(|row| numbers(row)).collect();
        assert_eq!(values, [1.0, -99.0, 151.0, 0.0]);
    }

    /// The bias comes from the file, not from the canonical constant.
    #[test]
    fn the_bias_is_the_files_own() {
        let stream = vec![101, 0, 0, 0, 0, 0, 0, 0];
        let rows = Harness::new(stream).rows(coding_with_bias(1, 0.0));
        assert_eq!(numbers(&rows[0]), [101.0]);
    }

    /// The invariant the whole decoder exists to hold: a row ends on the
    /// output byte count, so a group runs on into the next row. Rows are
    /// three units wide against eight-command groups, so row 3 takes its
    /// first two commands from group 1 and its last from group 2.
    #[test]
    fn a_command_group_carries_across_a_row_boundary() {
        let mut stream: Vec<u8> = (101..=108).collect();
        stream.extend(109..=115);
        stream.push(COMMAND_PADDING);

        let rows = Harness::new(stream).rows(coding(3));
        let decoded: Vec<Vec<f64>> = rows.iter().map(|row| numbers(row)).collect();
        assert_eq!(
            decoded,
            [
                vec![1.0, 2.0, 3.0],
                vec![4.0, 5.0, 6.0],
                // Straddles: 7.0 and 8.0 close group 1, 9.0 opens group 2.
                vec![7.0, 8.0, 9.0],
                vec![10.0, 11.0, 12.0],
                vec![13.0, 14.0, 15.0],
            ],
        );
    }

    /// A row that happens to end where a group does is the case a naive
    /// decoder gets right by accident; it must still work.
    #[test]
    fn a_row_ending_exactly_on_a_group_boundary_reads() {
        let mut stream: Vec<u8> = vec![101; COMMAND_GROUP_LEN];
        stream.extend([102; COMMAND_GROUP_LEN]);

        let rows = Harness::new(stream).rows(coding(COMMAND_GROUP_LEN));
        assert_eq!(rows.len(), 2);
        assert_eq!(numbers(&rows[0]), [1.0; COMMAND_GROUP_LEN]);
        assert_eq!(numbers(&rows[1]), [2.0; COMMAND_GROUP_LEN]);
    }

    /// Verbatim payloads sit after the *whole* group, in command order —
    /// so the group's last command takes a payload that precedes the
    /// next group. Getting this backwards reads a command byte as data.
    #[test]
    fn verbatim_payloads_follow_the_whole_group_in_command_order() {
        let mut stream = vec![
            COMMAND_VERBATIM,
            COMMAND_ALL_SPACES,
            COMMAND_SYSTEM_MISSING,
            101,
            COMMAND_PADDING,
            COMMAND_PADDING,
            COMMAND_PADDING,
            COMMAND_VERBATIM,
        ];
        stream.extend_from_slice(b"first   ");
        stream.extend_from_slice(b"second  ");
        stream.extend([102, 0, 0, 0, 0, 0, 0, 0]);

        let rows = Harness::new(stream).rows(coding(1));
        let units: Vec<&[u8]> = rows.iter().map(Vec::as_slice).collect();
        assert_eq!(
            units,
            [
                b"first   ".as_slice(),
                &EIGHT_SPACES,
                &system_missing(),
                &1.0_f64.to_le_bytes(),
                b"second  ".as_slice(),
                &2.0_f64.to_le_bytes(),
            ],
        );
    }

    /// Padding emits nothing, and a stream that ends on it ends cleanly:
    /// PSPP pads out the final group, so this is the ordinary way a real
    /// file finishes.
    #[test]
    fn trailing_padding_ends_the_stream_cleanly() {
        let stream = vec![101, COMMAND_PADDING, 0, 0, 0, 0, 0, 0];
        let mut harness = Harness::new(stream);
        let rows = harness.rows(coding(1));
        assert_eq!(rows.len(), 1);
        assert_eq!(numbers(&rows[0]), [1.0]);
        assert!(harness.warnings().is_empty(), "{:?}", harness.warnings());
    }

    /// A `252` on a row boundary is a plain early stop — nothing is lost,
    /// so nothing is said. Bytes after it are never read.
    #[test]
    fn an_end_of_data_command_on_a_row_boundary_stops_silently() {
        let mut stream = vec![101, COMMAND_END_OF_DATA, 0, 0, 0, 0, 0, 0];
        stream.extend([102; COMMAND_GROUP_LEN]);

        let mut harness = Harness::new(stream);
        let rows = harness.rows(coding(1));
        assert_eq!(rows.len(), 1, "the group after the marker is not decoded");
        assert!(harness.warnings().is_empty(), "{:?}", harness.warnings());
    }

    /// A `252` partway through a row discards it. `ReadStat` stops at the
    /// same point and says nothing; the warning is the one thing we add.
    #[test]
    fn an_end_of_data_command_inside_a_row_warns_and_discards_it() {
        let stream = vec![101, 102, COMMAND_END_OF_DATA, 0, 0, 0, 0, 0];
        let mut harness = Harness::new(stream);
        assert!(harness.rows(coding(3)).is_empty(), "the row was incomplete");
        assert!(
            matches!(
                harness.warnings(),
                [SavWarning::EndOfDataInsideRow {
                    bytes_produced: 16,
                    row_len: 24,
                }],
            ),
            "{:?}",
            harness.warnings(),
        );
    }

    /// Once the stream has ended it stays ended, however often it is
    /// asked — the record reader's loop depends on that.
    #[test]
    fn reading_past_the_end_keeps_reporting_the_end() {
        let mut harness = Harness::new(vec![101, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(harness.rows(coding(1)).len(), 1);
        assert!(harness.next_row(coding(1)).expect("past the end").is_none());
        assert!(harness.next_row(coding(1)).expect("twice").is_none());
    }

    /// Ending mid-row is a truncated file, not a clean end. The
    /// distinction is the whole reason the exhausted case checks whether
    /// anything was produced.
    #[test]
    fn a_stream_that_stops_mid_row_is_truncated() {
        let stream = vec![101, 102, COMMAND_PADDING, 0, 0, 0, 0, 0];
        let error = Harness::new(stream)
            .next_row(coding(3))
            .expect_err("a partial row must error");
        assert_truncated(&error, 24, 16);
    }

    /// A `253` whose payload never arrives is truncated even though no
    /// bytes of the row were produced — a command group was in hand, so
    /// the stream did not end on a boundary.
    #[test]
    fn a_verbatim_payload_that_never_arrives_is_truncated() {
        let stream = vec![COMMAND_VERBATIM, 0, 0, 0, 0, 0, 0, 0];
        let error = Harness::new(stream)
            .next_row(coding(1))
            .expect_err("a missing payload must error");
        assert_truncated(&error, 8, 0);
    }

    /// A payload cut off partway is caught by the unit source, which
    /// reports the unit it could not fill rather than the row.
    #[test]
    fn a_verbatim_payload_cut_short_is_truncated() {
        let mut stream = vec![COMMAND_VERBATIM, 0, 0, 0, 0, 0, 0, 0];
        stream.extend_from_slice(b"half");
        let error = Harness::new(stream)
            .next_row(coding(1))
            .expect_err("a short payload must error");
        assert_truncated(&error, 8, 4);
    }

    /// An empty data section is a clean end, not a truncation.
    #[test]
    fn an_empty_stream_ends_immediately() {
        let mut harness = Harness::new(Vec::new());
        assert!(harness.next_row(coding(1)).expect("empty").is_none());
        assert!(harness.warnings().is_empty(), "{:?}", harness.warnings());
    }

    fn assert_truncated(error: &SavError, expected: u64, actual: u64) {
        match error {
            SavError::Format(format) => {
                assert_eq!(format.section(), Section::Records);
                assert_eq!(
                    format.kind(),
                    FormatErrorKind::Truncated { expected, actual },
                );
            }
            other => panic!("expected a format error, got {other:?}"),
        }
    }
}
