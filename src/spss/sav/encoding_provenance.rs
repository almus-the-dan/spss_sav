//! Where the text encoding the reader applied came from.

use encoding_rs::Encoding;

/// Provenance of the text encoding the reader applied.
///
/// SAV files can declare their encoding in two places, both of which
/// sit near the end of the dictionary — after every string they govern:
///
/// - The character encoding record (record type 7, subtype 20) — an
///   IANA-style label such as `"UTF-8"`. Preferred.
/// - The `character_code` field of the machine integer info record
///   (record type 7, subtype 3) — a numeric Windows codepage. Used when
///   subtype 20 is absent or cannot be resolved.
///
/// Every variant carries the encoding actually applied; use
/// [`encoding`](Self::encoding) when the provenance does not matter.
///
/// A declaration that cannot be resolved either falls back per the
/// reader's
/// [`EncodingStrategy`](crate::spss::sav::encoding_strategy::EncodingStrategy)
/// — reported as [`Unrecognized`](Self::Unrecognized) — or fails the read
/// outright, so an `EncodingProvenance` never describes a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EncodingProvenance {
    /// Resolved from the character encoding record (record type 7,
    /// subtype 20).
    Declared(&'static Encoding),
    /// Resolved from the `character_code` field of the machine integer
    /// info record (record type 7, subtype 3).
    Codepage(&'static Encoding),
    /// The file declared no encoding at all, so the reader applied the
    /// [`EncodingStrategy`](crate::spss::sav::encoding_strategy::EncodingStrategy)'s
    /// `unspecified` fallback.
    Unspecified(&'static Encoding),
    /// The file declared an encoding that could not be resolved, so the
    /// reader applied the
    /// [`EncodingStrategy`](crate::spss::sav::encoding_strategy::EncodingStrategy)'s
    /// `unrecognized` fallback.
    ///
    /// Distinct from [`Unspecified`](Self::Unspecified): the file did
    /// name an encoding, it just named one this library cannot provide.
    /// Which declaration failed is reported by
    /// [`SavWarning::EncodingDeclarationUnrecognized`](crate::spss::sav::sav_warning::SavWarning::EncodingDeclarationUnrecognized).
    Unrecognized(&'static Encoding),
    /// The reader applied
    /// [`EncodingStrategy::Override`](crate::spss::sav::encoding_strategy::EncodingStrategy::Override),
    /// ignoring whatever the file declared.
    Overridden(&'static Encoding),
}

impl EncodingProvenance {
    /// The encoding the reader applied, whatever its provenance.
    #[must_use]
    #[inline]
    pub fn encoding(self) -> &'static Encoding {
        match self {
            Self::Declared(encoding)
            | Self::Codepage(encoding)
            | Self::Unspecified(encoding)
            | Self::Unrecognized(encoding)
            | Self::Overridden(encoding) => encoding,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_yields_its_encoding() {
        let utf8 = encoding_rs::UTF_8;
        assert_eq!(EncodingProvenance::Declared(utf8).encoding(), utf8);
        assert_eq!(EncodingProvenance::Codepage(utf8).encoding(), utf8);
        assert_eq!(EncodingProvenance::Unspecified(utf8).encoding(), utf8);
        assert_eq!(EncodingProvenance::Unrecognized(utf8).encoding(), utf8);
        assert_eq!(EncodingProvenance::Overridden(utf8).encoding(), utf8);
    }

    #[test]
    fn provenance_distinguishes_equal_encodings() {
        let utf8 = encoding_rs::UTF_8;
        assert_ne!(
            EncodingProvenance::Declared(utf8),
            EncodingProvenance::Codepage(utf8)
        );
        assert_ne!(
            EncodingProvenance::Unspecified(utf8),
            EncodingProvenance::Overridden(utf8)
        );
    }
}
