//! The ZSAV block container.
//!
//! A ZSAV data section is an ordinary bytecode command stream — see
//! [`record_format`](crate::spss::sav::record_format) — cut into blocks,
//! each block deflated into its own self-terminating zlib stream. The
//! blocks are framed by a [`ZsavHeader`] at the start of the data
//! section and a table in a trailer at the end of the file.
//!
//! # Why the trailer is not read
//!
//! The block table lives at the *end* of the file, and the reader chain
//! is `Read`-only by design — there is no [`Seek`](std::io::Seek) bound
//! to reach backwards with. It is not needed: the header gives the
//! trailer's position, which is where the block region stops, the
//! blocks are laid out contiguously from the end of the header (there
//! is no padding between them — measured on a PSPP `/ZCOMPRESSED`
//! file), and a zlib stream reports its own end. So the blocks can be
//! inflated in order, and the table would only restate what inflating
//! them already establishes.
//!
//! If row skipping ever wants a `Seek`-bound fast path, the table is
//! what it should use.

// Shell module: the shapes land now for review, the bodies with
// Phase 6(c).
#![allow(dead_code)]

use std::io::Read;

use crate::spss::sav::compression::data_unit_source::DataUnitSource;
use crate::spss::sav::compression::zsav_header::ZsavHeader;
use crate::spss::sav::reader_state::ReaderState;
use crate::spss::sav::sav_error::Result;
use crate::spss::sav::segment_layout::DATA_UNIT_LEN;

/// Feeds the bytecode decoder from a ZSAV file's inflated blocks.
///
/// Inflates one block at a time into `block` and hands out its bytes
/// eight at a time. A command group never has to be split across a
/// block boundary by this type — it refills transparently — but note
/// that the *stream* genuinely does continue across blocks, so the
/// decoder's group state must survive a refill.
///
/// Starts out unopened: the data-section header is read on the first
/// refill, so building one costs nothing and cannot fail.
#[derive(Debug, Default)]
pub(crate) struct ZlibBlocks {
    /// The data-section header, once the first refill has read it.
    header: Option<ZsavHeader>,
    /// Inflated bytes of the block currently being handed out.
    block: Vec<u8>,
    /// How far into `block` the next unit starts.
    position: usize,
    /// Set once the final block has been consumed.
    finished: bool,
}

impl<R: Read> DataUnitSource<R> for ZlibBlocks {
    fn next_unit(&mut self, _state: &mut ReaderState<R>) -> Result<Option<[u8; DATA_UNIT_LEN]>> {
        let _ = (self.header, &self.block, self.position, self.finished);
        todo!("body lands with Phase 6(c)")
    }
}
