//! What the SAV file declared about its text encoding.

use encoding_rs::Encoding;

/// What the file declared about its text encoding.
///
/// SAV files can declare their encoding in two places:
///
/// - The character encoding extension record (subtype 20) — preferred.
/// - The legacy `character_code` field of the integer info record
///   (subtype 7B) — used as a fallback when subtype 20 is absent.
///
/// `FileEncoding` records what the file *said*; the encoding the
/// reader actually applied is governed by the reader's
/// [`EncodingStrategy`](crate::spss::sav::encoding_strategy::EncodingStrategy).
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum FileEncoding {
    /// Declared via a character encoding extension record (subtype
    /// 20).
    Declared(&'static Encoding),
    /// Inferred from the integer info record's `character_code` field
    /// when subtype 20 was absent.
    Heuristic(&'static Encoding),
    /// Neither subtype 20 nor a recognizable `character_code` was
    /// present.
    Unknown,
}
