//! Reader policy for choosing a text encoding.

use encoding_rs::Encoding;

/// Last-resort encoding for [`EncodingStrategy::default`].
const DEFAULT_UNSPECIFIED_ENCODING: &Encoding = encoding_rs::WINDOWS_1252;

/// Strategy for choosing the text encoding when decoding strings in a
/// SAV file.
///
/// A SAV file can declare its own encoding in two places, both of which
/// sit near the end of the dictionary — see
/// [`EncodingProvenance`](crate::spss::sav::encoding_provenance::EncodingProvenance). This
/// type decides whether those declarations are honored, and what
/// happens when they are absent or cannot be resolved.
///
/// The encoding to apply and the policy for applying it are one value
/// rather than two: an encoding without a policy (or a policy without
/// the encoding it needs) is not a meaningful configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EncodingStrategy {
    /// Honor the encoding the file declares.
    Declared {
        /// Encoding to use when the file declares none at all. `None`
        /// makes an undeclared encoding a
        /// [`SavError::EncodingUnspecified`](crate::spss::sav::sav_error::SavError::EncodingUnspecified)
        /// rather than a guess.
        unspecified: Option<&'static Encoding>,
        /// Encoding to use when the file declares one that cannot be
        /// resolved to a supported encoding. `None` makes an
        /// unresolvable declaration a
        /// [`SavError::EncodingUnrecognized`](crate::spss::sav::sav_error::SavError::EncodingUnrecognized).
        unrecognized: Option<&'static Encoding>,
    },
    /// Use this encoding regardless of what the file declares. A
    /// mismatch surfaces as
    /// [`SavWarning::EncodingOverridden`](crate::spss::sav::sav_warning::SavWarning::EncodingOverridden).
    Override(&'static Encoding),
}

impl Default for EncodingStrategy {
    /// Honors the file's declaration, guessing `windows-1252` when the
    /// file declares nothing, and failing the read when it declares
    /// something that cannot be resolved.
    ///
    /// The asymmetry is deliberate: guessing is reasonable when the
    /// file is silent, but silently substituting an encoding other than
    /// the one the file explicitly asked for is not.
    #[inline]
    fn default() -> Self {
        Self::Declared {
            unspecified: Some(DEFAULT_UNSPECIFIED_ENCODING),
            unrecognized: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_honors_declaration_and_guesses_windows_1252() {
        assert_eq!(
            EncodingStrategy::default(),
            EncodingStrategy::Declared {
                unspecified: Some(encoding_rs::WINDOWS_1252),
                unrecognized: None,
            }
        );
    }

    #[test]
    fn override_carries_its_encoding() {
        let strategy = EncodingStrategy::Override(encoding_rs::UTF_8);
        assert_eq!(strategy, EncodingStrategy::Override(encoding_rs::UTF_8));
        assert_ne!(
            strategy,
            EncodingStrategy::Override(encoding_rs::WINDOWS_1252)
        );
    }

    #[test]
    fn declared_distinguishes_its_two_fallbacks() {
        let strict = EncodingStrategy::Declared {
            unspecified: None,
            unrecognized: None,
        };
        let lenient = EncodingStrategy::Declared {
            unspecified: Some(encoding_rs::UTF_8),
            unrecognized: Some(encoding_rs::UTF_8),
        };
        assert_ne!(strict, lenient);
    }
}
