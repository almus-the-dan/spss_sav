//! Shared helpers for decoding fixed-width text fields.
//!
//! SAV stores many text fields (file label, product name, variable
//! short name, ...) as fixed-width byte arrays right-padded with
//! spaces or NULs. The reader trims that padding and decodes the
//! remaining bytes through the file's active encoding to obtain a
//! `String`. Both the header parser and the dictionary parser need
//! the same machinery; it lives here so they can share it.

use encoding_rs::Encoding;

/// Decodes a fixed-width byte field through `encoding` and trims
/// trailing whitespace and NULs.
pub(super) fn decode_trimmed(bytes: &[u8], encoding: &'static Encoding) -> String {
    let trimmed = trim_trailing_padding(bytes);
    let (cow, _, _) = encoding.decode(trimmed);
    cow.into_owned()
}

/// The bits a padding byte may have set — which is to say, the space
/// character itself.
///
/// Space is `0x20`, a single set bit, and NUL is `0x00`, which sets
/// none; between them they are the only two bytes with nothing set
/// outside that one bit. So "is this padding?" reduces to a single or
/// against this mask, with no comparison and no branch — which is what
/// lets the bulk scan below vectorize.
const PADDING_BITS: u8 = b' ';

/// Bytes the bulk scan tests per step.
///
/// One SSE2 / NEON register. Writing the test as a reduction over the
/// chunk rather than an equality compare against a constant buys two
/// things: it recognizes a chunk mixing spaces and NULs, which
/// comparing against all-spaces would not, and it hands LLVM a shape it
/// can vectorize. On x86-64 the loop body compiles to `movdqu` /
/// `pand` against a `!PADDING_BITS` splat / `pcmpeqb` / `pmovmskb` —
/// four instructions per sixteen bytes, no branch per byte. The scalar
/// tail collapses to a single `test` against the same mask.
const TRIM_CHUNK_LEN: usize = 16;

/// Whether `byte` is trailing padding — a space or a NUL.
#[inline]
const fn is_padding(byte: u8) -> bool {
    byte | PADDING_BITS == PADDING_BITS
}

/// Whether every byte of `chunk` is padding.
///
/// Reducing the chunk with a bitwise or first is what makes this
/// branchless: the accumulated byte has a bit outside [`PADDING_BITS`]
/// exactly when at least one input byte did.
#[inline]
fn is_all_padding(chunk: &[u8]) -> bool {
    let accumulated = chunk
        .iter()
        .fold(0_u8, |accumulated, &byte| accumulated | byte);
    is_padding(accumulated)
}

/// Returns the prefix of `bytes` with trailing spaces and NULs
/// stripped. Returns an empty slice when the input is entirely
/// padding.
///
/// Scans backwards a [chunk](TRIM_CHUNK_LEN) at a time before falling
/// back to a byte-at-a-time walk over the final partial chunk. The bulk
/// path matters on the data path rather than for the fixed-width
/// metadata fields this module was written for: a declared string width
/// is routinely orders of magnitude longer than the average value in
/// the column, so most of a cell is padding and the backwards scan
/// dominates the cost of reading it.
pub(super) fn trim_trailing_padding(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    while end >= TRIM_CHUNK_LEN {
        let start = end - TRIM_CHUNK_LEN;
        if !is_all_padding(&bytes[start..end]) {
            break;
        }
        end = start;
    }
    // At most `TRIM_CHUNK_LEN` iterations: either the loop above
    // stopped on a chunk holding a non-padding byte, or fewer than a
    // chunk's worth of bytes remain.
    while end > 0 && is_padding(bytes[end - 1]) {
        end -= 1;
    }
    &bytes[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_removes_trailing_spaces() {
        assert_eq!(trim_trailing_padding(b"abc   "), b"abc");
    }

    #[test]
    fn trim_removes_trailing_nuls() {
        assert_eq!(trim_trailing_padding(b"abc\0\0\0"), b"abc");
    }

    #[test]
    fn trim_removes_mixed_trailing_padding() {
        assert_eq!(trim_trailing_padding(b"abc \0 \0"), b"abc");
    }

    #[test]
    fn trim_preserves_interior_padding() {
        assert_eq!(trim_trailing_padding(b"a b c   "), b"a b c");
    }

    #[test]
    fn trim_all_padding_returns_empty() {
        assert_eq!(trim_trailing_padding(b"   \0\0"), b"");
        assert!(trim_trailing_padding(b"").is_empty());
    }

    /// The whole basis for the branchless test: NUL and space are the
    /// only two bytes with no bits set outside [`PADDING_BITS`].
    #[test]
    fn only_nul_and_space_count_as_padding() {
        let padding: Vec<u8> = (0..=u8::MAX).filter(|&b| is_padding(b)).collect();
        assert_eq!(padding, vec![0, b' ']);
    }

    /// Chunk-aligned padding runs are what the bulk scan is for, and
    /// lengths either side of a chunk boundary are where an off-by-one
    /// would hide.
    #[test]
    fn trim_handles_runs_around_the_chunk_boundary() {
        for padding_len in 0..(TRIM_CHUNK_LEN * 3 + 2) {
            let mut bytes = b"abc".to_vec();
            bytes.resize(3 + padding_len, b' ');
            assert_eq!(
                trim_trailing_padding(&bytes),
                b"abc",
                "padding_len {padding_len}",
            );
        }
    }

    /// A padding run longer than a chunk, mixing spaces and NULs, is
    /// the case an equality compare against all-spaces would miss.
    #[test]
    fn trim_handles_mixed_padding_past_a_chunk() {
        let mut bytes = b"abc".to_vec();
        for index in 0..(TRIM_CHUNK_LEN * 2 + 5) {
            bytes.push(if index % 2 == 0 { b' ' } else { 0 });
        }
        assert_eq!(trim_trailing_padding(&bytes), b"abc");
    }

    /// An all-padding input longer than a chunk trims to nothing.
    #[test]
    fn trim_all_padding_past_a_chunk_returns_empty() {
        let bytes = vec![b' '; TRIM_CHUNK_LEN * 2 + 3];
        assert!(trim_trailing_padding(&bytes).is_empty());
    }

    /// Interior padding survives even when it sits a whole chunk deep.
    #[test]
    fn trim_preserves_interior_padding_past_a_chunk() {
        let mut bytes = b"a".to_vec();
        bytes.resize(1 + TRIM_CHUNK_LEN * 2, b' ');
        bytes.push(b'z');
        let expected = bytes.clone();
        bytes.extend(std::iter::repeat_n(b' ', TRIM_CHUNK_LEN * 2));
        assert_eq!(trim_trailing_padding(&bytes), expected.as_slice());
    }

    /// The chunked scan must agree with the obvious implementation on
    /// every input shape, including the ones no fixture produces.
    #[test]
    fn trim_agrees_with_a_naive_reverse_scan() {
        fn naive(bytes: &[u8]) -> &[u8] {
            let end = bytes
                .iter()
                .rposition(|&b| b != b' ' && b != 0)
                .map_or(0, |p| p + 1);
            &bytes[..end]
        }
        // A deterministic spread of shapes: content and padding
        // interleaved at every offset around two chunk boundaries.
        for len in 0..(TRIM_CHUNK_LEN * 2 + 3) {
            for pattern in 0..8_u32 {
                let bytes: Vec<u8> = (0..len)
                    .map(|index| match (index as u32 + pattern) % 4 {
                        0 => b' ',
                        1 => 0,
                        2 => b'x',
                        _ => b'\xFF',
                    })
                    .collect();
                assert_eq!(
                    trim_trailing_padding(&bytes),
                    naive(&bytes),
                    "len {len} pattern {pattern}",
                );
            }
        }
    }

    #[test]
    fn decode_trimmed_round_trip_ascii() {
        let result = decode_trimmed(b"hello   ", encoding_rs::WINDOWS_1252);
        assert_eq!(result, "hello");
    }

    #[test]
    fn decode_trimmed_handles_high_bytes() {
        // Windows-1252 0xE9 = é
        let result = decode_trimmed(b"caf\xE9    ", encoding_rs::WINDOWS_1252);
        assert_eq!(result, "café");
    }
}
