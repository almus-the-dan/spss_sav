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

/// Returns the prefix of `bytes` with trailing spaces and NULs
/// stripped. Returns an empty slice when the input is entirely
/// padding.
pub(super) fn trim_trailing_padding(bytes: &[u8]) -> &[u8] {
    let end = bytes
        .iter()
        .rposition(|&b| b != b' ' && b != 0)
        .map_or(0, |p| p + 1);
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
