//! Shared per-reader state used by every SAV reader phase.
//!
//! `ReaderState<R>` owns the underlying reader, the scratch buffer,
//! the running byte position, the detected byte order (filled in by
//! the header reader), and the warnings vec. It deliberately does not
//! own an encoding: the file's encoding is not resolvable until the
//! whole dictionary has been walked, so state that held one would only
//! ever hold a stale guess. Pure parsing functions in `*_parse.rs` operate on
//! byte slices; the I/O primitives that fill those slices live here.
//!
//! Compression-related state (bytecode codes block, ZLIB decoder
//! wrapper) is intentionally absent. It will be added when the
//! record reader lands; the dictionary section is always
//! uncompressed, so phases through the dictionary reader do not
//! need it.

use std::io::Read;

use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::sav_error::{Field, FormatErrorKind, Result, SavError, Section};
use crate::spss::sav::sav_warning::SavWarning;

/// Largest chunk [`ReaderState::skip`] reads at once. Bounds how far
/// the shared scratch buffer can grow on behalf of bytes nobody
/// wanted, without making the discard loop chatty for the small skips
/// (record filler, padding) that dominate.
const SKIP_WINDOW_LEN: usize = 64 * 1024;

/// Crate-internal state threaded through the reader typestate
/// chain. The same allocation is moved from one phase to the next
/// via the `into_*()` consuming transitions, so the warnings vec
/// and scratch buffer keep their capacity across phases.
#[derive(Debug)]
pub(crate) struct ReaderState<R> {
    reader: R,
    buffer: Vec<u8>,
    position: u64,
    byte_order: Option<ByteOrder>,
    warnings: Vec<SavWarning>,
}

impl<R> ReaderState<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: Vec::new(),
            position: 0,
            byte_order: None,
            warnings: Vec::new(),
        }
    }

    /// Byte offset in the file.
    pub fn position(&self) -> u64 {
        self.position
    }

    /// The detected byte order, or `None` before the header reader
    /// has determined it.
    #[allow(dead_code)] // exercised once the record reader phase lands.
    pub fn byte_order(&self) -> Option<ByteOrder> {
        self.byte_order
    }

    /// Records the byte order detected from the header's
    /// `layout_code` field.
    pub fn set_byte_order(&mut self, byte_order: ByteOrder) {
        self.byte_order = Some(byte_order);
    }

    /// Warnings accumulated during the most recent advance. Each
    /// `read_*` / `write_*` operation clears this vec at the start
    /// of its logic, then appends fresh warnings while running.
    pub fn warnings(&self) -> &[SavWarning] {
        &self.warnings
    }

    /// Mutable access to the warnings vec, for the parser to push
    /// onto during an advance.
    pub fn warnings_mut(&mut self) -> &mut Vec<SavWarning> {
        &mut self.warnings
    }

    /// Internal scratch buffer, populated by the most recent
    /// `read_exact`-style call.
    #[allow(dead_code)] // exercised once the record reader phase lands.
    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }
}

impl<R: Read> ReaderState<R> {
    /// Resizes the internal buffer to `len`, reads exactly `len`
    /// bytes into it, and returns the filled slice. The same
    /// allocation is reused across calls.
    pub fn read_exact(&mut self, len: usize, section: Section) -> Result<&[u8]> {
        self.buffer.resize(len, 0);
        self.reader
            .read_exact(&mut self.buffer)
            .map_err(|e| SavError::io(section, e))?;
        self.position += u64::try_from(len).expect("buffer length exceeds u64");
        Ok(&self.buffer)
    }

    /// Reads exactly `N` bytes into a stack-allocated array.
    pub fn read_array<const N: usize>(&mut self, section: Section) -> Result<[u8; N]> {
        let mut out = [0u8; N];
        self.reader
            .read_exact(&mut out)
            .map_err(|e| SavError::io(section, e))?;
        self.position += u64::try_from(N).expect("array length exceeds u64");
        Ok(out)
    }

    /// Reads `len` bytes and discards them.
    ///
    /// Discards through a bounded window rather than one
    /// [`read_exact`](Self::read_exact) of `len` bytes. There is no
    /// [`Seek`](std::io::Seek) bound to seek past the data with, so the
    /// bytes have to be read either way — but reading them into the
    /// shared scratch buffer would size that buffer to the largest
    /// record ever skipped and keep it there for the rest of the read,
    /// which is the opposite of what skipping a large record is for.
    pub fn skip(&mut self, len: usize, section: Section) -> Result<()> {
        let mut remaining = len;
        while remaining > 0 {
            let chunk = remaining.min(SKIP_WINDOW_LEN);
            self.read_exact(chunk, section)?;
            remaining -= chunk;
        }
        Ok(())
    }

    /// Reads a single byte.
    pub fn read_u8(&mut self, section: Section) -> Result<u8> {
        let [byte] = self.read_array::<1>(section)?;
        Ok(byte)
    }

    /// Reads a 4-byte unsigned integer in the file's byte order.
    pub fn read_u32(&mut self, byte_order: ByteOrder, section: Section) -> Result<u32> {
        let bytes = self.read_array::<4>(section)?;
        let value = byte_order.read_u32(bytes);
        Ok(value)
    }

    /// Reads a 4-byte unsigned integer in the file's byte order and
    /// returns it as a [`usize`]. The only failure mode beyond
    /// [`read_u32`](Self::read_u32)'s own is the cast itself, on
    /// platforms where `usize` is narrower than `u32`; that surfaces
    /// as [`FormatErrorKind::FieldTooLarge`] tagged with `field` and
    /// the value's byte offset.
    pub fn read_u32_as_usize(
        &mut self,
        byte_order: ByteOrder,
        section: Section,
        field: Field,
    ) -> Result<usize> {
        let position = self.position();
        let value = self.read_u32(byte_order, section)?;
        u32_as_usize(value, position, section, field)
    }

    /// Reads a 4-byte signed integer in the file's byte order.
    pub fn read_i32(&mut self, byte_order: ByteOrder, section: Section) -> Result<i32> {
        let bytes = self.read_array::<4>(section)?;
        let value = byte_order.read_i32(bytes);
        Ok(value)
    }

    /// Reads an 8-byte IEEE 754 double in the file's byte order.
    #[allow(dead_code)] // exercised once the record reader phase lands.
    pub fn read_f64(&mut self, byte_order: ByteOrder, section: Section) -> Result<f64> {
        let bytes = self.read_array::<8>(section)?;
        let value = byte_order.read_f64(bytes);
        Ok(value)
    }
}

/// Casts a `u32` to a `usize`, mapping any failure to a
/// position-tagged [`FormatErrorKind::FieldTooLarge`] error.
///
/// On platforms where `usize` is at least 32 bits (every supported
/// target — 32-bit and 64-bit), the cast is always lossless and
/// this never errors. It exists for symmetry with the reading-side
/// helper and to centralize the error-shaping pattern so call sites
/// stay one line.
pub(crate) fn u32_as_usize(
    value: u32,
    position: u64,
    section: Section,
    field: Field,
) -> Result<usize> {
    usize::try_from(value)
        .map_err(|_| SavError::format(section, position, FormatErrorKind::FieldTooLarge { field }))
}
