//! Shared per-reader state used by every SAV reader phase.
//!
//! `ReaderState<R>` owns the underlying reader, the active
//! encoding, the scratch buffer, the running byte position, the
//! detected byte order (filled in by the header reader), and the
//! warnings vec. Pure parsing functions in `*_parse.rs` operate on
//! byte slices; the I/O primitives that fill those slices live here.
//!
//! Compression-related state (bytecode codes block, ZLIB decoder
//! wrapper) is intentionally absent. It will be added when the
//! record reader lands; the dictionary section is always
//! uncompressed, so phases through the dictionary reader do not
//! need it.

use encoding_rs::Encoding;

use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::sav_warning::SavWarning;

/// Crate-internal state threaded through the reader typestate
/// chain. The same allocation is moved from one phase to the next
/// via the `into_*()` consuming transitions, so the warnings vec
/// and scratch buffer keep their capacity across phases.
#[derive(Debug)]
#[allow(dead_code)] // exercised once the header reader lands.
pub(crate) struct ReaderState<R> {
    reader: R,
    encoding: &'static Encoding,
    buffer: Vec<u8>,
    position: u64,
    byte_order: Option<ByteOrder>,
    warnings: Vec<SavWarning>,
}

impl<R> ReaderState<R> {
    #[allow(dead_code)] // exercised once the header reader lands.
    pub(crate) fn new(reader: R, encoding: &'static Encoding) -> Self {
        Self {
            reader,
            encoding,
            buffer: Vec::new(),
            position: 0,
            byte_order: None,
            warnings: Vec::new(),
        }
    }

    /// Returns a new state with the given encoding, preserving the
    /// reader, buffer allocation, position, byte order, and
    /// warnings vec.
    #[allow(dead_code)] // exercised once the header reader lands.
    pub(crate) fn with_encoding(self, encoding: &'static Encoding) -> Self {
        Self { encoding, ..self }
    }

    /// Byte offset in the file.
    #[allow(dead_code)] // exercised once the header reader lands.
    pub(crate) fn position(&self) -> u64 {
        self.position
    }

    /// The active character encoding.
    #[allow(dead_code)] // exercised once the header reader lands.
    pub(crate) fn encoding(&self) -> &'static Encoding {
        self.encoding
    }

    /// The detected byte order, or `None` before the header reader
    /// has determined it.
    #[allow(dead_code)] // exercised once the header reader lands.
    pub(crate) fn byte_order(&self) -> Option<ByteOrder> {
        self.byte_order
    }

    /// Records the byte order detected from the header's
    /// `layout_code` field.
    #[allow(dead_code)] // exercised once the header reader lands.
    pub(crate) fn set_byte_order(&mut self, byte_order: ByteOrder) {
        self.byte_order = Some(byte_order);
    }

    /// Warnings accumulated during the most recent advance. Each
    /// `read_*` / `write_*` operation clears this vec at the start
    /// of its logic, then appends fresh warnings while running.
    #[allow(dead_code)] // exercised once the header reader lands.
    pub(crate) fn warnings(&self) -> &[SavWarning] {
        &self.warnings
    }

    /// Mutable access to the warnings vec, for the parser to push
    /// onto during an advance.
    #[allow(dead_code)] // exercised once the header reader lands.
    pub(crate) fn warnings_mut(&mut self) -> &mut Vec<SavWarning> {
        &mut self.warnings
    }

    /// Internal scratch buffer, populated by the most recent
    /// `read_exact`-style call.
    #[allow(dead_code)] // exercised once the header reader lands.
    pub(crate) fn buffer(&self) -> &[u8] {
        &self.buffer
    }
}
