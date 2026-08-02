//! Where the bytecode decoder's command units come from.

// Shell module: the shape lands now for review, the implementations
// with Phase 6(b) and 6(c).
#![allow(dead_code)]

use crate::spss::sav::reader_state::ReaderState;
use crate::spss::sav::sav_error::Result;
use crate::spss::sav::segment_layout::DATA_UNIT_LEN;

/// A source of 8-byte units for
/// [`BytecodeDecoder`](crate::spss::sav::compression::bytecode_decoder::BytecodeDecoder).
///
/// Exists so the decoder is written once. A `$FL2` compressed file
/// feeds it straight from the reader
/// ([`FileUnits`](crate::spss::sav::compression::file_units::FileUnits));
/// a ZSAV file feeds it from inflated blocks
/// ([`ZlibBlocks`](crate::spss::sav::compression::zlib_blocks::ZlibBlocks)).
/// Both are driven by the same [`ReaderState`], which is why that is
/// threaded through each call rather than captured by the source.
pub(crate) trait DataUnitSource<R> {
    /// The next eight bytes of the command stream, or `None` once the
    /// stream is exhausted.
    ///
    /// Returning `None` is the ordinary way a data section ends: PSPP
    /// does not write a
    /// [`COMMAND_END_OF_DATA`](crate::spss::sav::record_format::COMMAND_END_OF_DATA)
    /// marker, so running out of stream is the only termination signal
    /// most real files give.
    fn next_unit(&mut self, state: &mut ReaderState<R>) -> Result<Option<[u8; DATA_UNIT_LEN]>>;
}
