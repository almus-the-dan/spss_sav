//! Shared per-reader state used by every SAV reader phase.
//!
//! `ReaderState<R>` owns the underlying reader, the running byte
//! position, and the warnings vec — and deliberately little else. Pure
//! parsing functions in `*_parse.rs` operate on byte slices; the I/O
//! primitives that produce those slices live here.
//!
//! Three things it does *not* hold, each for the same reason — a copy
//! here would only be a second place for the value to be wrong:
//!
//! - **Byte order.** Every multi-byte read takes it as a parameter.
//! - **Encoding.** Not resolvable until the whole dictionary has been
//!   walked, so any state holding one would hold a stale guess.
//! - **Compression state.** The bytecode decoder and the ZSAV block
//!   container keep their own, in
//!   [`compression`](crate::spss::sav::compression), where the row
//!   source that owns them lives.

use std::io::{ErrorKind, Read};

use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::sav_error::{Field, FormatErrorKind, Result, SavError, Section};
use crate::spss::sav::sav_warning::SavWarning;

/// Bytes [`ReaderState::skip`] discards at a time.
///
/// The chunk lives on the stack, so this is a stack-frame budget rather
/// than a bound on anything retained — which is why it is kilobytes and
/// not the tens of kilobytes a heap window would justify. Chunk size
/// barely shows up in any case: `SavReader::from_path` and `from_file`
/// wrap the file in a [`BufReader`](std::io::BufReader), so a discard
/// read is a copy out of that buffer rather than a syscall.
const SKIP_CHUNK_LEN: usize = 1024;

/// Crate-internal state threaded through the reader typestate
/// chain, moved from one phase to the next via the `into_*()`
/// consuming transitions so the warnings vec keeps its capacity
/// across phases.
#[derive(Debug)]
pub(crate) struct ReaderState<R> {
    reader: R,
    position: u64,
    warnings: Vec<SavWarning>,
}

impl<R> ReaderState<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            position: 0,
            warnings: Vec::new(),
        }
    }

    /// Byte offset in the file.
    pub fn position(&self) -> u64 {
        self.position
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

    /// Advances the running byte offset by `len` bytes just read.
    ///
    /// Fallible only in the two ways that are unreachable on real
    /// input: a `usize` too wide for a `u64`, and a file long enough to
    /// overflow the offset itself. Both are errors rather than panics
    /// so that reading a SAV file has no documented panic path — see
    /// [`FormatErrorKind::PositionOverflow`].
    fn advance(&mut self, len: usize, section: Section) -> Result<()> {
        let advanced = u64::try_from(len)
            .ok()
            .and_then(|len| self.position.checked_add(len));
        let Some(position) = advanced else {
            let kind = FormatErrorKind::PositionOverflow;
            return Err(SavError::format(section, self.position, kind));
        };
        self.position = position;
        Ok(())
    }
}

impl<R: Read> ReaderState<R> {
    /// Reads exactly `len` bytes into a fresh [`Vec`] and returns it.
    ///
    /// Hands back ownership rather than a borrow of shared scratch
    /// because every caller needs owned bytes regardless: the file's
    /// encoding is not known until the dictionary ends, so records are
    /// retained undecoded and outlive any buffer the reader could hold.
    /// Reading straight into the destination is therefore one
    /// allocation and no copy, where a shared buffer was a copy on top
    /// of the same allocation.
    ///
    /// A caller wanting only a prefix — a padded field, say — should
    /// read the full on-disk length and then
    /// [`truncate`](Vec::truncate), which costs nothing.
    pub fn read_vec(&mut self, len: usize, section: Section) -> Result<Vec<u8>> {
        let mut out = vec![0_u8; len];
        self.reader
            .read_exact(&mut out)
            .map_err(|e| SavError::io(section, e))?;
        self.advance(len, section)?;
        Ok(out)
    }

    /// Fills `buffer` from the reader, telling a clean end of stream
    /// apart from a truncated one.
    ///
    /// Returns `Ok(false)` when the stream was already exhausted and
    /// not one byte was read. That is how a data section ordinarily
    /// ends: PSPP writes no end-of-data marker, and the declared case
    /// count may be absent or wrong, so running out of bytes on a row
    /// boundary is the only reliable signal. `Ok(true)` means `buffer`
    /// was filled. A stream that stops partway through is truncated and
    /// errors.
    ///
    /// Unlike [`read_vec`](Self::read_vec) this fills a caller-owned
    /// buffer, which is what lets the record reader reuse one row
    /// allocation for the whole file.
    pub fn read_into(&mut self, buffer: &mut [u8], section: Section) -> Result<bool> {
        let mut filled = 0;
        while filled < buffer.len() {
            match self.reader.read(&mut buffer[filled..]) {
                Ok(0) => break,
                Ok(read) => filled += read,
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return Err(SavError::io(section, e)),
            }
        }
        self.advance(filled, section)?;
        if filled == 0 {
            return Ok(false);
        }
        if filled < buffer.len() {
            let kind = FormatErrorKind::Truncated {
                expected: as_u64(buffer.len()),
                actual: as_u64(filled),
            };
            return Err(SavError::format(section, self.position, kind));
        }
        Ok(true)
    }

    /// Reads exactly `N` bytes into a stack-allocated array.
    pub fn read_array<const N: usize>(&mut self, section: Section) -> Result<[u8; N]> {
        let mut out = [0u8; N];
        self.reader
            .read_exact(&mut out)
            .map_err(|e| SavError::io(section, e))?;
        self.advance(N, section)?;
        Ok(out)
    }

    /// Reads `len` bytes and discards them.
    ///
    /// There is no [`Seek`](std::io::Seek) bound to jump past the data
    /// with, so the bytes have to be read either way. They go into a
    /// stack chunk rather than anything owned: discarding needs a place
    /// to *throw* bytes, not a place to keep them, so the destination
    /// should not outlive the call — and skipping a large record must
    /// not leave a large allocation behind, which is the opposite of
    /// what skipping it was for.
    pub fn skip(&mut self, len: usize, section: Section) -> Result<()> {
        let mut chunk = [0_u8; SKIP_CHUNK_LEN];
        let mut remaining = len;
        while remaining > 0 {
            let take = remaining.min(SKIP_CHUNK_LEN);
            self.reader
                .read_exact(&mut chunk[..take])
                .map_err(|e| SavError::io(section, e))?;
            self.advance(take, section)?;
            remaining -= take;
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
}

/// Widens a byte count for reporting inside an error.
///
/// Saturates rather than failing: this is only ever called on a length
/// that has already been read successfully, on a path that is already
/// returning an error, so a second error kind would say nothing useful.
/// Unreachable anyway unless `usize` is wider than `u64`.
fn as_u64(len: usize) -> u64 {
    u64::try_from(len).unwrap_or(u64::MAX)
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
