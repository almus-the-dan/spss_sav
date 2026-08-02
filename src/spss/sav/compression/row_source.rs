//! Producing raw row bytes, whichever way the file is compressed.
//!
//! The three compression schemes differ only in how a row's bytes are
//! obtained; once a row buffer is full, splitting it into cells is
//! identical (see [`record_parse`](crate::spss::sav::record_parse)).
//! This is where that difference is confined.

// Shell module: the shapes land now for review, the bodies with
// Phase 6(a) through 6(c).
#![allow(dead_code)]

use std::io::Read;

use crate::spss::sav::compression::bytecode_decoder::BytecodeDecoder;
use crate::spss::sav::compression::compression_kind::CompressionKind;
use crate::spss::sav::compression::file_units::FileUnits;
use crate::spss::sav::compression::zlib_blocks::ZlibBlocks;
use crate::spss::sav::data_layout::DataLayout;
use crate::spss::sav::reader_state::ReaderState;
use crate::spss::sav::sav_error::Result;

/// Fills row buffers from the data section.
#[derive(Debug)]
pub(crate) enum RowSource {
    /// Rows are written back to back at full width.
    Uncompressed,
    /// Rows come from the bytecode command stream.
    Bytecode {
        /// Straight-from-file command units.
        units: FileUnits,
        /// The shared command-stream decoder.
        decoder: BytecodeDecoder,
    },
    /// Rows come from the same command stream, carried in zlib blocks.
    ///
    /// The decoder is the *same* type the `$FL2` path uses — ZSAV adds
    /// framing, not a second encoding.
    Zlib {
        /// The block container feeding command units.
        blocks: ZlibBlocks,
        /// The shared command-stream decoder.
        decoder: BytecodeDecoder,
    },
}

impl RowSource {
    /// Picks the scheme the header declared.
    ///
    /// Deliberately pure: no I/O happens until the first row is asked
    /// for. A caller that builds a [`RecordReader`](crate::spss::sav::record_reader::RecordReader)
    /// and never reads a row — to inspect the schema and stop — should
    /// not pay for touching the data section, and constructing the
    /// reader should not be able to fail on a file nobody is going to
    /// read. The ZSAV data-section header is consequently read by
    /// [`ZlibBlocks`] on its first refill rather than here.
    pub fn new(compression: CompressionKind) -> Self {
        match compression {
            CompressionKind::None => Self::Uncompressed,
            CompressionKind::Bytecode => Self::Bytecode {
                units: FileUnits,
                decoder: BytecodeDecoder::default(),
            },
            CompressionKind::Zlib => Self::Zlib {
                blocks: ZlibBlocks::default(),
                decoder: BytecodeDecoder::default(),
            },
        }
    }

    /// Fills `row` with the next row's `layout.row_len()` bytes.
    ///
    /// Returns `false` at a clean end of the data section. A file that
    /// stops partway through a row is truncated and errors.
    ///
    /// Note that a clean end is *not* signaled by the declared case
    /// count: PSPP writes no end-of-data marker, and the count in the
    /// header may be absent (`-1`) or disagree with what is actually
    /// there. The count is a cross-check to warn against, not the thing
    /// that ends the read.
    #[allow(dead_code)] // wired up in Phase 6(a).
    pub fn next_row<R: Read>(
        &mut self,
        _state: &mut ReaderState<R>,
        _layout: &DataLayout,
        _row: &mut Vec<u8>,
    ) -> Result<bool> {
        todo!("body lands with Phase 6(a), gaining arms in 6(b) and 6(c)")
    }
}
