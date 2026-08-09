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
//! The one thing the table would add is a count, and the header's
//! trailer *length* already gives that — one 24-byte entry per block —
//! so the block count is cross-checked without reading it.
//!
//! If row skipping ever wants a `Seek`-bound fast path, the table is
//! what it should use.
//!
//! # Why inflation is windowed
//!
//! Blocks are inflated into a fixed window rather than one block at a
//! time. A block's inflated size is chosen by whoever wrote the file —
//! PSPP writes 0x3ff000, but nothing in the format binds it — so
//! inflating a whole block into a buffer would let the file dictate the
//! allocation. The window caps it instead, and the decoder above cannot
//! tell the difference: it asks for eight bytes at a time either way.

use std::io::Read;

use flate2::{Decompress, FlushDecompress, Status};

use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::compression::data_unit_source::DataUnitSource;
use crate::spss::sav::compression::zsav_header::ZsavHeader;
use crate::spss::sav::reader_state::ReaderState;
use crate::spss::sav::record_format::ZHEADER_LEN;
use crate::spss::sav::sav_error::{FormatErrorKind, Result, SavError, Section};
use crate::spss::sav::segment_layout::DATA_UNIT_LEN;

/// Inflated bytes held at once.
///
/// A window, not a block: see the module docs for why the file does not
/// get to choose this number.
const WINDOW_LEN: usize = 64 * 1_024;

/// Compressed bytes read from the file per top-up.
const INPUT_CHUNK_LEN: usize = 16 * 1_024;

/// Feeds the bytecode decoder from a ZSAV file's inflated blocks.
///
/// Hands out the inflated command stream eight bytes at a time. A
/// command group never has to be split across a block boundary by this
/// type — it refills transparently — but note that the *stream*
/// genuinely does continue across blocks, so the decoder's group state
/// must survive a refill.
///
/// Starts out unopened: the data-section header is read on the first
/// refill, so building one costs nothing and cannot fail.
#[derive(Debug)]
pub(crate) struct ZlibBlocks {
    /// Byte order of the container's own fields.
    byte_order: ByteOrder,
    /// The data-section header, once the first refill has read it.
    header: Option<ZsavHeader>,
    /// Inflate state for the block currently being read. Reset at each
    /// block boundary, which is the only thing a boundary means here.
    decompress: Decompress,
    /// Compressed bytes read ahead of what the current block consumed.
    ///
    /// A block's compressed length is only recorded in the trailer, so
    /// the end of one is discovered by inflating into it. Whatever was
    /// read past that end is the start of the next block, and has to be
    /// kept rather than re-read.
    input: Vec<u8>,
    /// How far into [`input`](Self::input) inflation has consumed.
    input_position: usize,
    /// The inflated window currently being handed out.
    output: Vec<u8>,
    /// How far into [`output`](Self::output) the next unit starts.
    output_position: usize,
    /// Blocks whose zlib stream has ended, for the cross-check against
    /// the count the header's trailer length implies.
    blocks: u64,
    /// Set once the final block has been consumed.
    finished: bool,
}

impl ZlibBlocks {
    pub fn new(byte_order: ByteOrder) -> Self {
        Self {
            byte_order,
            header: None,
            decompress: Decompress::new(true),
            input: Vec::new(),
            input_position: 0,
            // Capacity is what bounds a single inflate call, so it has
            // to exist before the first one rather than grow into place.
            output: Vec::with_capacity(WINDOW_LEN),
            output_position: 0,
            blocks: 0,
            finished: false,
        }
    }

    /// Refills [`output`](Self::output), returning `false` once the
    /// block region is exhausted.
    fn advance<R: Read>(&mut self, state: &mut ReaderState<R>) -> Result<bool> {
        while !self.finished {
            let header = self.open(state)?;
            self.refill_input(state, header)?;

            self.output.clear();
            self.output_position = 0;
            let before = self.decompress.total_in();
            let status = self
                .decompress
                .decompress_vec(
                    &self.input[self.input_position..],
                    &mut self.output,
                    FlushDecompress::None,
                )
                .map_err(|_| corrupt(state))?;
            // Read before any reset, which zeroes the counter.
            let consumed = self.decompress.total_in() - before;
            self.input_position += as_usize(consumed);

            if status == Status::StreamEnd {
                self.end_block(state, header);
            } else if consumed == 0 && self.output.is_empty() {
                // The stream wants more input and there is none left in
                // the region: the file stops mid-block.
                return Err(corrupt(state));
            }
            if !self.output.is_empty() {
                return Ok(true);
            }
        }
        // Only now, with every byte that did inflate already handed out:
        // a block region that came up short is still a broken file, but
        // the rows it did contain read back fine, so the complaint
        // belongs at the end rather than in place of the last block.
        self.verify_block_count(state)?;
        Ok(false)
    }

    /// Reads the data-section header, once.
    fn open<R: Read>(&mut self, state: &mut ReaderState<R>) -> Result<ZsavHeader> {
        if let Some(header) = self.header {
            return Ok(header);
        }
        let position = state.position();
        let bytes = state.read_array::<ZHEADER_LEN>(Section::Records)?;
        let header = ZsavHeader::parse(bytes, position, self.byte_order)?;
        self.header = Some(header);
        Ok(header)
    }

    /// Tops up the compressed buffer when inflation has drained it.
    ///
    /// Never reads past the trailer: the block region ends there, and
    /// the trailer's own bytes are not a zlib stream.
    fn refill_input<R: Read>(
        &mut self,
        state: &mut ReaderState<R>,
        header: ZsavHeader,
    ) -> Result<()> {
        if self.input_position < self.input.len() {
            return Ok(());
        }
        // Clamping is the answer here rather than a masked error: this
        // asks how much of the region is left, and every read is already
        // bounded by that same figure, so the reader cannot get past the
        // trailer. Were it ever to, zero is what should be read next.
        let remaining = header.trailer_position().saturating_sub(state.position());
        let take = as_usize(remaining.min(INPUT_CHUNK_LEN as u64));
        self.input.clear();
        self.input_position = 0;
        if take == 0 {
            return Ok(());
        }
        self.input.resize(take, 0);
        if !state.read_into(&mut self.input, Section::Records)? {
            // The trailer position pointed past the end of the file.
            self.input.clear();
            return Err(corrupt(state));
        }
        Ok(())
    }

    /// Checks the blocks inflated against the count the header's trailer
    /// length accounts for, once the region has been consumed.
    fn verify_block_count<R>(&self, state: &ReaderState<R>) -> Result<()> {
        let Some(header) = self.header else {
            return Ok(());
        };
        if self.blocks == header.block_count() {
            return Ok(());
        }
        Err(corrupt(state))
    }

    /// Records a finished zlib stream and decides whether any block
    /// region is left.
    fn end_block<R: Read>(&mut self, state: &ReaderState<R>, header: ZsavHeader) {
        self.blocks += 1;
        self.decompress.reset(true);
        let region_left = header.trailer_position() > state.position();
        let buffered_left = self.input_position < self.input.len();
        self.finished = !region_left && !buffered_left;
    }
}

/// The error for a block region that did not hold the zlib streams the
/// header said it would.
fn corrupt<R>(state: &ReaderState<R>) -> SavError {
    SavError::format(
        Section::Records,
        state.position(),
        FormatErrorKind::InvalidCompressedBlock,
    )
}

impl<R: Read> DataUnitSource<R> for ZlibBlocks {
    /// The next eight bytes of the inflated stream.
    ///
    /// Assembled byte by byte rather than sliced out, because a unit can
    /// span two windows — and, in principle, two blocks. PSPP's own
    /// files never split one: its block size is a multiple of eight and
    /// every unit starts at a multiple of eight, so its boundaries land
    /// between units. Nothing in the format guarantees that of another
    /// writer, and the cost of not assuming it is this loop.
    fn next_unit(&mut self, state: &mut ReaderState<R>) -> Result<Option<[u8; DATA_UNIT_LEN]>> {
        let mut unit = [0_u8; DATA_UNIT_LEN];
        let filled = self.fill_unit(state, &mut unit)?;
        if filled == 0 {
            return Ok(None);
        }
        if filled < DATA_UNIT_LEN {
            let kind = FormatErrorKind::Truncated {
                expected: DATA_UNIT_LEN as u64,
                actual: filled as u64,
            };
            return Err(SavError::format(Section::Records, state.position(), kind));
        }
        Ok(Some(unit))
    }
}

impl ZlibBlocks {
    fn fill_unit<R: Read>(
        &mut self,
        state: &mut ReaderState<R>,
        unit: &mut [u8; DATA_UNIT_LEN],
    ) -> Result<usize> {
        let mut filled = 0;
        while filled < DATA_UNIT_LEN {
            if self.output_position == self.output.len() && !self.advance(state)? {
                break;
            }
            let available = &self.output[self.output_position..];
            let take = available.len().min(DATA_UNIT_LEN - filled);
            unit[filled..filled + take].copy_from_slice(&available[..take]);
            self.output_position += take;
            filled += take;
        }
        Ok(filled)
    }
}

/// Narrows a byte count that came from a buffer this type owns.
///
/// Saturates rather than failing: every value passed here is already
/// bounded by a buffer length, so the conversion cannot lose anything on
/// a platform able to hold that buffer.
fn as_usize(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use flate2::Compression;
    use flate2::write::ZlibEncoder;

    use super::*;
    use crate::spss::sav::record_format::{ZTRAILER_ENTRY_LEN, ZTRAILER_HEADER_LEN};
    use crate::spss::sav::sav_error::Field;

    fn deflate(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes).expect("deflate");
        encoder.finish().expect("finish the stream")
    }

    /// A whole ZSAV data section: the header, each block deflated into
    /// its own stream back to back, then a trailer of the length the
    /// header declares.
    ///
    /// `declared_blocks` is what the trailer's *length* accounts for,
    /// which is how the reader learns the count without seeking to the
    /// table. Passing something other than `blocks.len()` is how the
    /// disagreement is tested.
    fn container(blocks: &[&[u8]], declared_blocks: usize, self_position: u64) -> Vec<u8> {
        let compressed: Vec<Vec<u8>> = blocks.iter().map(|block| deflate(block)).collect();
        let region: usize = compressed.iter().map(Vec::len).sum();
        let trailer_len = ZTRAILER_HEADER_LEN + declared_blocks * ZTRAILER_ENTRY_LEN;

        let mut out = Vec::new();
        out.extend_from_slice(&self_position.to_le_bytes());
        out.extend_from_slice(&((ZHEADER_LEN + region) as u64).to_le_bytes());
        out.extend_from_slice(&(trailer_len as u64).to_le_bytes());
        for block in compressed {
            out.extend_from_slice(&block);
        }
        // The reader never reads the trailer, so its contents do not
        // matter — but its presence does: a reader that ran past the
        // position the header gave would find these bytes and fail to
        // inflate them.
        out.resize(out.len() + trailer_len, 0);
        out
    }

    fn data_section(blocks: &[&[u8]]) -> Vec<u8> {
        container(blocks, blocks.len(), 0)
    }

    struct Harness {
        state: ReaderState<Cursor<Vec<u8>>>,
        blocks: ZlibBlocks,
    }

    impl Harness {
        fn new(bytes: Vec<u8>) -> Self {
            Self {
                state: ReaderState::new(Cursor::new(bytes)),
                blocks: ZlibBlocks::new(ByteOrder::LittleEndian),
            }
        }

        fn next(&mut self) -> Result<Option<[u8; DATA_UNIT_LEN]>> {
            self.blocks.next_unit(&mut self.state)
        }

        /// Every unit the section yields, stopping at a clean end.
        fn all(&mut self) -> Vec<[u8; DATA_UNIT_LEN]> {
            let mut units = Vec::new();
            while let Some(unit) = self.next().expect("read a unit") {
                units.push(unit);
            }
            units
        }
    }

    /// Blocks are handed out in order and joined into one stream — the
    /// container is framing, not content.
    #[test]
    fn blocks_are_inflated_in_order() {
        let section = data_section(&[b"first   ", b"second  ", b"third   "]);
        let units = Harness::new(section).all();
        assert_eq!(units, [*b"first   ", *b"second  ", *b"third   "],);
    }

    /// A unit split across a block boundary is reassembled. PSPP never
    /// writes one — its block size is a multiple of eight — so this
    /// shape only exists in a hand-built file, which is exactly why the
    /// reader must not assume the alignment.
    #[test]
    fn a_unit_split_across_two_blocks_is_reassembled() {
        let section = data_section(&[b"ABCD", b"EFGHIJKLMNOP"]);
        let units = Harness::new(section).all();
        assert_eq!(units, [*b"ABCDEFGH", *b"IJKLMNOP"]);
    }

    /// Many small blocks, none of them unit-aligned, still reassemble —
    /// the boundary is not allowed to mean anything.
    #[test]
    fn a_unit_can_span_more_than_two_blocks() {
        let section = data_section(&[b"AB", b"CD", b"EF", b"GH", b"IJKLMNOP"]);
        let units = Harness::new(section).all();
        assert_eq!(units, [*b"ABCDEFGH", *b"IJKLMNOP"]);
    }

    /// The end of the block region is the end of the stream, and it
    /// stays ended.
    #[test]
    fn the_stream_ends_when_the_region_does() {
        let mut harness = Harness::new(data_section(&[b"one     "]));
        assert_eq!(harness.next().expect("a unit"), Some(*b"one     "));
        assert!(harness.next().expect("past the end").is_none());
        assert!(harness.next().expect("twice").is_none());
    }

    /// A region whose bytes do not divide into whole units is truncated
    /// — the command stream is units all the way down.
    #[test]
    fn a_trailing_partial_unit_is_truncated() {
        let mut harness = Harness::new(data_section(&[b"whole   ", b"half"]));
        assert_eq!(harness.next().expect("a unit"), Some(*b"whole   "));
        let error = harness.next().expect_err("a partial unit must error");
        match error {
            SavError::Format(format) => assert_eq!(
                format.kind(),
                FormatErrorKind::Truncated {
                    expected: 8,
                    actual: 4,
                },
            ),
            other => panic!("expected a format error, got {other:?}"),
        }
    }

    /// The self-referential position field is the guard against having
    /// misread the dictionary's length, so a wrong one is refused
    /// rather than ignored.
    #[test]
    fn a_header_recording_the_wrong_position_is_rejected() {
        let section = container(&[b"whatever"], 1, 8);
        let error = Harness::new(section).next().expect_err("must reject");
        assert_unexpected(&error, Field::ZsavHeaderPosition);
    }

    /// The trailer length states one 24-byte entry per block, so a
    /// region holding fewer streams than that means blocks are missing.
    #[test]
    fn a_region_holding_fewer_blocks_than_the_trailer_accounts_for_is_rejected() {
        let section = container(&[b"first   ", b"second  "], 3, 0);
        let mut harness = Harness::new(section);
        assert_eq!(harness.next().expect("a unit"), Some(*b"first   "));
        assert_eq!(harness.next().expect("a unit"), Some(*b"second  "));
        let error = harness.next().expect_err("the missing block must error");
        assert_invalid_block(&error);
    }

    /// A trailer length that is not a whole number of entries cannot
    /// state a block count at all.
    #[test]
    fn a_trailer_length_that_is_not_whole_entries_is_rejected() {
        let mut section = data_section(&[b"whatever"]);
        // Bump the declared trailer length off an entry boundary.
        let bumped = u64::from_le_bytes(section[16..24].try_into().expect("eight bytes")) + 1;
        section[16..24].copy_from_slice(&bumped.to_le_bytes());
        let error = Harness::new(section).next().expect_err("must reject");
        assert_unexpected(&error, Field::ZsavTrailerLength);
    }

    /// A block region cut off partway through a zlib stream is corrupt,
    /// not a clean end: the stream never reported its own end.
    #[test]
    fn a_region_that_stops_mid_block_is_rejected() {
        let mut section = data_section(&[b"first   ", b"second  "]);
        // Drop the trailer and the tail of the last block, then restate
        // the region's end so the reader believes the file.
        section.truncate(section.len() - ZTRAILER_HEADER_LEN - ZTRAILER_ENTRY_LEN * 2 - 4);
        let end = section.len() as u64;
        section[8..16].copy_from_slice(&end.to_le_bytes());
        let mut harness = Harness::new(section);
        assert_eq!(harness.next().expect("a unit"), Some(*b"first   "));
        // Whatever of the cut block did inflate is still handed out —
        // deflate stores the data before the checksum that ends the
        // stream. What must not happen is the cut reading as a clean
        // end, since the stream never reported one.
        let error = loop {
            match harness.next() {
                Ok(Some(_)) => {}
                Ok(None) => panic!("a cut block must not read as the end of the data"),
                Err(error) => break error,
            }
        };
        assert_invalid_block(&error);
    }

    fn assert_unexpected(error: &SavError, field: Field) {
        match error {
            SavError::Format(format) => {
                assert_eq!(format.kind(), FormatErrorKind::UnexpectedValue { field });
            }
            other => panic!("expected a format error, got {other:?}"),
        }
    }

    fn assert_invalid_block(error: &SavError) {
        match error {
            SavError::Format(format) => {
                assert_eq!(format.section(), Section::Records);
                assert_eq!(format.kind(), FormatErrorKind::InvalidCompressedBlock);
            }
            other => panic!("expected a format error, got {other:?}"),
        }
    }
}
