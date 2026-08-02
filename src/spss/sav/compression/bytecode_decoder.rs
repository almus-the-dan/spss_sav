//! Turning a bytecode command stream into row bytes.
//!
//! Shared by `$FL2` compressed files and ZSAV — see
//! [`compression`](crate::spss::sav::compression) for why those are the
//! same decoder, and
//! [`record_format`](crate::spss::sav::record_format) for the command
//! codes and for why a row boundary is not a stream boundary.

// Shell module: the shapes land now for review, the bodies with
// Phase 6(b).
#![allow(dead_code)]

use std::io::Read;

use crate::spss::sav::compression::data_unit_source::DataUnitSource;
use crate::spss::sav::data_layout::DataLayout;
use crate::spss::sav::reader_state::ReaderState;
use crate::spss::sav::record_format::COMMAND_GROUP_LEN;
use crate::spss::sav::sav_error::Result;

/// Decodes the bytecode command stream into row bytes.
///
/// Holds the partially-consumed command group across calls, which is
/// what makes straddling row boundaries work: a group's commands can
/// finish one row and begin the next.
#[derive(Debug, Default)]
pub(crate) struct BytecodeDecoder {
    /// The command group currently being executed.
    group: [u8; COMMAND_GROUP_LEN],
    /// How many of `group`'s commands have run. Equal to
    /// [`COMMAND_GROUP_LEN`] when the next call must fetch a new group.
    consumed: usize,
    /// Set once the stream has ended, by marker or by exhaustion.
    finished: bool,
}

impl BytecodeDecoder {
    /// Fills `row` with exactly `layout.row_len()` decoded bytes.
    ///
    /// Returns `false` when the stream ended before a full row could be
    /// produced, which is the normal end of the data section. A row
    /// that ends *partway* through is a truncated file and errors.
    ///
    /// The whole layout is taken rather than the four fields actually
    /// consumed, because two commands synthesize values instead of
    /// copying them and between them need all four: an inline code
    /// writes `code - bias` and must lay that `f64` out in the file's
    /// own float format and byte order, and the system-missing command
    /// writes the file's sentinel bit pattern. Every inline value is a
    /// small integer, so the encoding cannot fail in practice even for
    /// the formats whose range is narrow.
    pub fn fill_row<R: Read, S: DataUnitSource<R>>(
        &mut self,
        _source: &mut S,
        _state: &mut ReaderState<R>,
        _layout: &DataLayout,
        _row: &mut Vec<u8>,
    ) -> Result<bool> {
        let _ = (&self.group, self.consumed, self.finished);
        todo!("body lands with Phase 6(b)")
    }
}
