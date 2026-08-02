//! What producing a row's bytes depends on.

use crate::spss::sav::float_encoding::FloatEncoding;
use crate::spss::sav::segment_layout::DATA_UNIT_LEN;

/// Everything a [`RowSource`](crate::spss::sav::compression::row_source::RowSource)
/// needs to fill a row buffer — and deliberately nothing more.
///
/// Narrower than
/// [`DataLayout`](crate::spss::sav::data_layout::DataLayout), which is
/// where it comes from, because **the compressed formats are
/// variable-agnostic**. A bytecode command stream is a flat run of
/// 8-byte units with no notion of where one variable ends and the next
/// begins; a row ends once `row_len` bytes have been produced, which is
/// why a command group routinely straddles a row boundary. See
/// [`record_format`](crate::spss::sav::record_format).
///
/// Handing the decoder a whole `DataLayout` would give it
/// `variables()`, which it must never consult. That invariant is the
/// one thing making the decoder correct, and nothing would enforce it —
/// a straddling-row bug "fixed" by peeking at variable boundaries would
/// pass on any fixture whose groups happen to align. Passing this
/// instead makes the restriction structural.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RowCoding {
    row_len: usize,
    bias: f64,
    float_encoding: FloatEncoding,
    system_missing: [u8; DATA_UNIT_LEN],
}

impl RowCoding {
    pub fn new(
        row_len: usize,
        bias: f64,
        float_encoding: FloatEncoding,
        system_missing: [u8; DATA_UNIT_LEN],
    ) -> Self {
        Self {
            row_len,
            bias,
            float_encoding,
            system_missing,
        }
    }

    /// Bytes in one uncompressed row, and so where a row ends.
    #[inline]
    pub fn row_len(self) -> usize {
        self.row_len
    }

    /// What an inline command code is measured against: code `c` stands
    /// for `c - bias`.
    #[allow(dead_code)] // read by BytecodeDecoder::fill_row in Phase 6(b).
    #[inline]
    pub fn bias(self) -> f64 {
        self.bias
    }

    /// How to lay an `f64` out on disk, for the values an inline code
    /// synthesizes rather than copies.
    #[allow(dead_code)] // read by BytecodeDecoder::fill_row in Phase 6(b).
    #[inline]
    pub fn float_encoding(self) -> FloatEncoding {
        self.float_encoding
    }

    /// The bytes the system-missing command writes.
    ///
    /// Just the one pattern, not the whole sentinel triple: `LOWEST` and
    /// `HIGHEST` belong to missing-value *declarations* and no command
    /// emits them.
    #[allow(dead_code)] // read by BytecodeDecoder::fill_row in Phase 6(b).
    #[inline]
    pub fn system_missing(self) -> [u8; DATA_UNIT_LEN] {
        self.system_missing
    }
}
