//! The header opening a ZSAV data section.

use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::record_format::{
    ZHEADER_LEN, ZHEADER_SELF_OFFSET, ZHEADER_TRAILER_LEN_OFFSET, ZHEADER_TRAILER_POSITION_OFFSET,
    ZTRAILER_ENTRY_LEN, ZTRAILER_HEADER_LEN,
};
use crate::spss::sav::sav_error::{Field, FormatErrorKind, Result, SavError, Section};

/// The 24-byte header opening a ZSAV data section.
///
/// Frames the block region: the blocks run from the end of this header
/// up to [`trailer_position`](Self::trailer_position).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ZsavHeader {
    /// File position of the trailer, and so the end of the blocks.
    trailer_position: u64,
    /// Byte length of the trailer, which is what states the block
    /// count. Read through [`block_count`](Self::block_count).
    trailer_len: u64,
}

impl ZsavHeader {
    /// Parses the header from its 24 on-disk bytes.
    ///
    /// `position` is the file offset the bytes were read from. The
    /// header's first field records that same offset, so comparing the
    /// two is a free check that the dictionary was walked to exactly
    /// the right place — and the reason the field is not kept
    /// afterward: once it has been shown equal to `position`, storing
    /// it would only be a second place for the value to be wrong.
    ///
    /// # Errors
    ///
    /// Every field is rejected outright rather than repaired, because
    /// each one is load-bearing: a wrong position means the dictionary
    /// was misread and the bytes here are not a header at all, and a
    /// wrong trailer position would have the reader inflate the trailer
    /// as though it were a block.
    pub fn parse(bytes: [u8; ZHEADER_LEN], position: u64, byte_order: ByteOrder) -> Result<Self> {
        let field = |offset, field| read_position(&bytes, offset, byte_order, position, field);

        let declared = field(ZHEADER_SELF_OFFSET, Field::ZsavHeaderPosition)?;
        if declared != position {
            return Err(invalid(position, Field::ZsavHeaderPosition));
        }

        let trailer_position = field(ZHEADER_TRAILER_POSITION_OFFSET, Field::ZsavTrailerPosition)?;
        // The blocks start where this header ends, so the trailer has to
        // sit at least a header's length past the header's own position.
        //
        // Phrased as a subtraction rather than comparing against
        // `position + ZHEADER_LEN`, so that no addition exists to
        // overflow. `checked_sub` folds in the other half of the same
        // check for free: a trailer *before* the header is rejected by
        // the same branch rather than wrapping into a huge span.
        let region = trailer_position
            .checked_sub(position)
            .filter(|span| *span >= header_len());
        if region.is_none() {
            return Err(invalid(position, Field::ZsavTrailerPosition));
        }

        let trailer_len = field(ZHEADER_TRAILER_LEN_OFFSET, Field::ZsavTrailerLength)?;
        // The trailer is a fixed prefix plus a whole number of block
        // entries. Checking that here is what lets `block_count` be
        // exact arithmetic rather than a guess.
        let entries = trailer_len
            .checked_sub(trailer_header_len())
            .filter(|bytes| bytes % entry_len() == 0);
        if entries.is_none() {
            return Err(invalid(position, Field::ZsavTrailerLength));
        }

        let header = Self {
            trailer_position,
            trailer_len,
        };
        Ok(header)
    }

    /// File position of the trailer, and so the end of the block
    /// region.
    pub fn trailer_position(self) -> u64 {
        self.trailer_position
    }

    /// How many blocks the trailer's length accounts for.
    ///
    /// The trailer holds one 24-byte entry per block after its fixed
    /// prefix, so its length states the block count without the
    /// [`Seek`](std::io::Seek) it would take to go and read the table
    /// itself. `ReadStat` derives the count the same way, and checks it
    /// against the count the trailer declares; we check it against the
    /// number of blocks actually inflated.
    pub fn block_count(self) -> u64 {
        (self.trailer_len - trailer_header_len()) / entry_len()
    }
}

/// Reads one 8-byte file position, rejecting a negative value.
///
/// The fields are signed on disk — `ReadStat` declares the trailer's as
/// `int64_t` — but every one of them is an offset or a length, so a
/// negative value is a corrupt file rather than a meaningful number.
fn read_position(
    bytes: &[u8; ZHEADER_LEN],
    offset: usize,
    byte_order: ByteOrder,
    position: u64,
    field: Field,
) -> Result<u64> {
    let mut eight = [0_u8; 8];
    eight.copy_from_slice(&bytes[offset..offset + 8]);
    u64::try_from(byte_order.read_i64(eight)).map_err(|_| invalid(position, field))
}

fn invalid(position: u64, field: Field) -> SavError {
    SavError::format(
        Section::Records,
        position,
        FormatErrorKind::UnexpectedValue { field },
    )
}

/// The layout constants as file offsets. They are `usize` because they
/// index byte buffers; positions are `u64`.
fn header_len() -> u64 {
    ZHEADER_LEN as u64
}

fn trailer_header_len() -> u64 {
    ZTRAILER_HEADER_LEN as u64
}

fn entry_len() -> u64 {
    ZTRAILER_ENTRY_LEN as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds the 24 header bytes, little-endian.
    fn header_bytes(
        self_position: u64,
        trailer_position: u64,
        trailer_len: u64,
    ) -> [u8; ZHEADER_LEN] {
        let mut bytes = [0_u8; ZHEADER_LEN];
        bytes[ZHEADER_SELF_OFFSET..ZHEADER_SELF_OFFSET + 8]
            .copy_from_slice(&self_position.to_le_bytes());
        bytes[ZHEADER_TRAILER_POSITION_OFFSET..ZHEADER_TRAILER_POSITION_OFFSET + 8]
            .copy_from_slice(&trailer_position.to_le_bytes());
        bytes[ZHEADER_TRAILER_LEN_OFFSET..ZHEADER_TRAILER_LEN_OFFSET + 8]
            .copy_from_slice(&trailer_len.to_le_bytes());
        bytes
    }

    /// A trailer length holding `blocks` entries after its prefix.
    fn trailer_len(blocks: u64) -> u64 {
        trailer_header_len() + blocks * entry_len()
    }

    fn parse(bytes: [u8; ZHEADER_LEN], position: u64) -> Result<ZsavHeader> {
        ZsavHeader::parse(bytes, position, ByteOrder::LittleEndian)
    }

    fn assert_rejects(bytes: [u8; ZHEADER_LEN], position: u64, field: Field) {
        let error = parse(bytes, position).expect_err("must reject");
        match error {
            SavError::Format(format) => {
                assert_eq!(format.kind(), FormatErrorKind::UnexpectedValue { field });
            }
            other => panic!("expected a format error, got {other:?}"),
        }
    }

    #[test]
    fn a_well_formed_header_parses() {
        let bytes = header_bytes(200, 5000, trailer_len(3));
        let header = parse(bytes, 200).expect("parse");
        assert_eq!(header.trailer_position(), 5000);
        assert_eq!(header.block_count(), 3);
    }

    /// A file whose data section holds no blocks at all: the trailer
    /// begins the moment the header ends. Degenerate, but consistent —
    /// zero blocks inflated against a zero block count.
    #[test]
    fn an_empty_block_region_is_accepted() {
        let bytes = header_bytes(200, 200 + header_len(), trailer_len(0));
        let header = parse(bytes, 200).expect("parse");
        assert_eq!(header.block_count(), 0);
    }

    /// The self-referential field is the check that the dictionary was
    /// walked to exactly the right place.
    #[test]
    fn a_position_disagreeing_with_where_it_was_read_is_rejected() {
        let bytes = header_bytes(200, 5000, trailer_len(1));
        assert_rejects(bytes, 208, Field::ZsavHeaderPosition);
    }

    /// A trailer inside the header leaves nowhere for the blocks.
    #[test]
    fn a_trailer_overlapping_the_header_is_rejected() {
        let bytes = header_bytes(200, 200 + header_len() - 1, trailer_len(1));
        assert_rejects(bytes, 200, Field::ZsavTrailerPosition);
    }

    /// A trailer *before* the header would make the block region a
    /// negative span. Caught by the same branch as the overlap, which is
    /// why the comparison is written as a subtraction.
    #[test]
    fn a_trailer_before_the_header_is_rejected() {
        let bytes = header_bytes(200, 8, trailer_len(1));
        assert_rejects(bytes, 200, Field::ZsavTrailerPosition);
    }

    /// The fields are signed on disk, so a high bit set reads as a
    /// negative offset — meaningless for a position or a length.
    #[test]
    fn a_negative_field_is_rejected() {
        let negative = (-1_i64).cast_unsigned();
        assert_rejects(
            header_bytes(negative, 5000, trailer_len(1)),
            200,
            Field::ZsavHeaderPosition,
        );
        assert_rejects(
            header_bytes(200, negative, trailer_len(1)),
            200,
            Field::ZsavTrailerPosition,
        );
        assert_rejects(
            header_bytes(200, 5000, negative),
            200,
            Field::ZsavTrailerLength,
        );
    }

    /// A trailer shorter than its own fixed prefix, or holding a partial
    /// entry, cannot state a block count.
    #[test]
    fn a_trailer_length_that_is_not_whole_entries_is_rejected() {
        assert_rejects(
            header_bytes(200, 5000, trailer_header_len() - 1),
            200,
            Field::ZsavTrailerLength,
        );
        assert_rejects(
            header_bytes(200, 5000, trailer_len(2) + 1),
            200,
            Field::ZsavTrailerLength,
        );
    }
}
