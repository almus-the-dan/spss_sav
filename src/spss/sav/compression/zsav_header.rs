//! The header opening a ZSAV data section.

// Shell module: the shapes land now for review, the bodies with
// Phase 6(c).
#![allow(dead_code)]

use crate::spss::sav::sav_error::Result;

/// The 24-byte header opening a ZSAV data section.
///
/// Frames the block region: the blocks run from the end of this header
/// up to [`trailer_position`](Self::trailer_position).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ZsavHeader {
    /// The header's own file position, as it records it.
    ///
    /// Self-referential, so it is checked against the position the
    /// header was actually read from — a cheap guard against a file
    /// whose dictionary length was misread.
    position: u64,
    /// File position of the trailer, and so the end of the blocks.
    trailer_position: u64,
    /// Byte length of the trailer.
    trailer_len: u64,
}

impl ZsavHeader {
    /// Parses the header from its 24 on-disk bytes.
    ///
    /// `position` is the file offset the bytes were read from, used to
    /// validate the self-referential field.
    pub fn parse(_bytes: &[u8], _position: u64) -> Result<Self> {
        todo!("body lands with Phase 6(c)")
    }

    /// The header's own recorded position.
    pub fn position(self) -> u64 {
        self.position
    }

    /// File position of the trailer, and so the end of the block
    /// region.
    pub fn trailer_position(self) -> u64 {
        self.trailer_position
    }

    /// Byte length of the trailer.
    pub fn trailer_len(self) -> u64 {
        self.trailer_len
    }
}
