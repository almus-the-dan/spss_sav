//! Resolving a SAV file's declared encoding to a concrete encoding.
//!
//! A file can declare its encoding in two places, and this module
//! decides which one wins and what happens when neither yields a
//! usable answer. Resolution order is the character encoding record
//! (type 7, subtype 20), then the machine integer info record's
//! `character_code` (type 7, subtype 3), then the reader's
//! [`EncodingStrategy`] fallbacks.
//!
//! Every function here is pure: the caller supplies whatever the file
//! declared and receives the resolution plus any warnings.

use encoding_rs::Encoding;

use crate::spss::sav::encoding_strategy::EncodingStrategy;
use crate::spss::sav::file_encoding::FileEncoding;
use crate::spss::sav::sav_error::{Result, SavError};
use crate::spss::sav::sav_warning::SavWarning;

/// What a subtype-3 `character_code` value told us.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodepageResolution {
    /// The code maps to an encoding this library can apply.
    Supported(&'static Encoding),
    /// The code carries no usable information, so it is treated as
    /// though the file had not declared an encoding at all.
    NoInformation,
    /// The code names a real encoding this library cannot provide.
    Unsupported,
}

/// Resolves the encoding to apply from the reader's strategy and
/// whatever the file declared.
///
/// `label` is the subtype-20 payload and `character_code` the subtype-3
/// field; `None` means that record was absent. A `label` that is empty
/// or all whitespace is treated as absent — a subtype-20 record with no
/// payload declares nothing.
///
/// # Errors
///
/// Returns [`SavError::EncodingUnspecified`] when the file declared
/// nothing and the strategy has no `unspecified` fallback, and
/// [`SavError::EncodingUnrecognized`] when the file declared something
/// unresolvable and the strategy has no `unrecognized` fallback.
#[allow(dead_code)] // wired up when the header reader defers decoding.
pub(super) fn resolve(
    strategy: EncodingStrategy,
    label: Option<&str>,
    character_code: Option<i32>,
    warnings: &mut Vec<SavWarning>,
) -> Result<FileEncoding> {
    let (unspecified, unrecognized) = match strategy {
        // An override needs nothing from the file, so the declarations
        // are not consulted at all — not even to compare them against
        // each other, which would be noise when both are being ignored.
        // The override-mismatch warning is raised later, by the
        // dictionary reader, as each declaring record is handed to the
        // caller.
        EncodingStrategy::Override(encoding) => return Ok(FileEncoding::Overridden(encoding)),
        EncodingStrategy::Declared {
            unspecified,
            unrecognized,
        } => (unspecified, unrecognized),
    };

    let label = label.filter(|candidate| !candidate.trim().is_empty());
    let codepage = character_code.map(|code| (code, resolve_codepage(code)));

    // The declaration we could not honor, if any, for the error and
    // for choosing between the two fallbacks.
    let mut unresolvable: Option<String> = None;

    // Subtype 20 first: it names an encoding outright, and PSPP treats
    // it as authoritative.
    if let Some(label) = label {
        if let Some(encoding) = resolve_label(label) {
            if let Some((code, CodepageResolution::Supported(from_code))) = codepage
                && from_code != encoding
            {
                warnings.push(SavWarning::EncodingDeclarationMismatch {
                    label: label.to_owned(),
                    character_code: code,
                });
            }
            return Ok(FileEncoding::Declared(encoding));
        }
        warnings.push(SavWarning::EncodingDeclarationUnrecognized {
            declaration: label.to_owned(),
        });
        unresolvable = Some(label.to_owned());
    }

    // Subtype 3 next. PSPP stops at subtype 20; falling through to the
    // codepage is deliberately more useful, since a file with an exotic
    // label may still carry a perfectly ordinary codepage.
    match codepage {
        Some((_, CodepageResolution::Supported(encoding))) => {
            return Ok(FileEncoding::Codepage(encoding));
        }
        Some((code, CodepageResolution::Unsupported)) => {
            let declaration = format!("character_code {code}");
            warnings.push(SavWarning::EncodingDeclarationUnrecognized {
                declaration: declaration.clone(),
            });
            unresolvable = unresolvable.or(Some(declaration));
        }
        Some((_, CodepageResolution::NoInformation)) | None => {}
    }

    // Neither declaration produced an encoding, so fall back — to
    // `unrecognized` if the file did declare something we could not
    // honor, otherwise to `unspecified`.
    if let Some(declaration) = unresolvable {
        return unrecognized
            .map(FileEncoding::Unspecified)
            .ok_or(SavError::EncodingUnrecognized { declaration });
    }

    let encoding = unspecified.ok_or(SavError::EncodingUnspecified)?;
    warnings.push(SavWarning::EncodingUnspecified {
        used: encoding.name(),
    });
    Ok(FileEncoding::Unspecified(encoding))
}

/// Resolves a subtype-20 encoding label.
///
/// Delegates to WHATWG label matching, which is ASCII-case-insensitive
/// and whitespace-trimming, and which deliberately folds several legacy
/// labels onto supersets: both `"us-ascii"` and `"iso-8859-1"` resolve
/// to `windows-1252`. That agrees with how [`resolve_codepage`] treats
/// the equivalent numeric codes, so the two declaration sites cannot
/// disagree merely because they spell the same encoding differently.
///
/// Uses the no-replacement form on purpose: a handful of labels map to
/// `encoding_rs`'s `REPLACEMENT` encoding, which decodes everything to a
/// single error, and applying it to metadata would silently destroy
/// every string in the file.
fn resolve_label(label: &str) -> Option<&'static Encoding> {
    Encoding::for_label_no_replacement(label.as_bytes())
}

/// Resolves a subtype-3 `character_code` (a Windows codepage number).
///
/// Restricted to codes `encoding_rs` can actually serve. Codes it cannot
/// serve — EBCDIC, the DOS codepages, most Mac variants, DEC-KANJI, and
/// the UTF-16/UTF-32 codes (which cannot coexist with SAV's fixed-width
/// byte fields anyway) — resolve to [`CodepageResolution::Unsupported`]
/// rather than being approximated.
///
/// `0`, `2`, and `3` yield [`CodepageResolution::NoInformation`]. `2`
/// nominally means 7-bit ASCII, but old SPSS for Unix and Windows wrote
/// it unconditionally regardless of the encoding actually in use, so it
/// says nothing; `3` is undocumented and equally unreliable; `0` is
/// simply an unfilled field. Routing them to the strategy's
/// `unspecified` fallback reaches the same default outcome as
/// `ReadStat` (which maps them to `windows-1252`) while still letting a
/// caller's explicit fallback take effect.
///
/// Because `encoding_rs` folds some encodings onto supersets, several
/// codes share an arm; the groupings are noted inline.
fn resolve_codepage(code: i32) -> CodepageResolution {
    let encoding = match code {
        0 | 2 | 3 => return CodepageResolution::NoInformation,

        65001 => encoding_rs::UTF_8,

        // ASCII (20127) and Latin-1 (28591) fold onto windows-1252,
        // matching WHATWG label matching and `ReadStat`'s mapping.
        1252 | 20127 | 28591 => encoding_rs::WINDOWS_1252,
        // ISO-8859-9 (28599) folds onto windows-1254 the same way.
        1254 | 28599 => encoding_rs::WINDOWS_1254,

        1250 => encoding_rs::WINDOWS_1250,
        1251 => encoding_rs::WINDOWS_1251,
        1253 => encoding_rs::WINDOWS_1253,
        1255 => encoding_rs::WINDOWS_1255,
        1256 => encoding_rs::WINDOWS_1256,
        1257 => encoding_rs::WINDOWS_1257,
        1258 => encoding_rs::WINDOWS_1258,

        28592 => encoding_rs::ISO_8859_2,
        28593 => encoding_rs::ISO_8859_3,
        28594 => encoding_rs::ISO_8859_4,
        28595 => encoding_rs::ISO_8859_5,
        28596 => encoding_rs::ISO_8859_6,
        28597 => encoding_rs::ISO_8859_7,
        28598 => encoding_rs::ISO_8859_8,
        28603 => encoding_rs::ISO_8859_13,
        28605 => encoding_rs::ISO_8859_15,

        866 => encoding_rs::IBM866,
        874 => encoding_rs::WINDOWS_874,
        932 => encoding_rs::SHIFT_JIS,
        936 => encoding_rs::GBK,
        949 => encoding_rs::EUC_KR,
        950 => encoding_rs::BIG5,
        20866 => encoding_rs::KOI8_R,
        20932 => encoding_rs::EUC_JP,
        21866 => encoding_rs::KOI8_U,
        10000 => encoding_rs::MACINTOSH,
        10007 => encoding_rs::X_MAC_CYRILLIC,

        _ => return CodepageResolution::Unsupported,
    };
    CodepageResolution::Supported(encoding)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A strategy that honors declarations and errors on both fallback
    /// paths, so tests can assert which error a case reaches.
    const STRICT: EncodingStrategy = EncodingStrategy::Declared {
        unspecified: None,
        unrecognized: None,
    };

    fn lenient(
        unspecified: &'static Encoding,
        unrecognized: &'static Encoding,
    ) -> EncodingStrategy {
        EncodingStrategy::Declared {
            unspecified: Some(unspecified),
            unrecognized: Some(unrecognized),
        }
    }

    // -- resolve_label ------------------------------------------------------

    #[test]
    fn label_resolves_common_declarations() {
        assert_eq!(resolve_label("UTF-8"), Some(encoding_rs::UTF_8));
        assert_eq!(
            resolve_label("windows-1252"),
            Some(encoding_rs::WINDOWS_1252)
        );
    }

    #[test]
    fn label_matching_is_case_insensitive() {
        // PSPP writes the label uppercase; SPSS writes it lowercase.
        assert_eq!(
            resolve_label("WINDOWS-1252"),
            Some(encoding_rs::WINDOWS_1252)
        );
    }

    #[test]
    fn ascii_and_latin1_labels_fold_onto_windows_1252() {
        assert_eq!(resolve_label("us-ascii"), Some(encoding_rs::WINDOWS_1252));
        assert_eq!(resolve_label("iso-8859-1"), Some(encoding_rs::WINDOWS_1252));
    }

    #[test]
    fn unknown_label_does_not_resolve() {
        assert_eq!(resolve_label("UTF-9"), None);
        assert_eq!(resolve_label("EBCDIC-US"), None);
    }

    #[test]
    fn replacement_only_label_does_not_resolve() {
        // Would otherwise decode every string to a single error.
        assert_eq!(resolve_label("iso-2022-cn"), None);
    }

    // -- resolve_codepage ---------------------------------------------------

    #[test]
    fn codepage_resolves_supported_codes() {
        assert_eq!(
            resolve_codepage(65001),
            CodepageResolution::Supported(encoding_rs::UTF_8)
        );
        assert_eq!(
            resolve_codepage(1252),
            CodepageResolution::Supported(encoding_rs::WINDOWS_1252)
        );
        assert_eq!(
            resolve_codepage(932),
            CodepageResolution::Supported(encoding_rs::SHIFT_JIS)
        );
    }

    #[test]
    fn codepage_folds_ascii_and_latin1_onto_windows_1252() {
        assert_eq!(
            resolve_codepage(20127),
            CodepageResolution::Supported(encoding_rs::WINDOWS_1252)
        );
        assert_eq!(
            resolve_codepage(28591),
            CodepageResolution::Supported(encoding_rs::WINDOWS_1252)
        );
    }

    #[test]
    fn codepage_folds_iso_8859_9_onto_windows_1254() {
        assert_eq!(
            resolve_codepage(1254),
            CodepageResolution::Supported(encoding_rs::WINDOWS_1254)
        );
        assert_eq!(
            resolve_codepage(28599),
            CodepageResolution::Supported(encoding_rs::WINDOWS_1254)
        );
    }

    #[test]
    fn unreliable_codepages_carry_no_information() {
        // Old SPSS wrote 2 regardless of the encoding in use; 3 is
        // undocumented; 0 is an unfilled field.
        assert_eq!(resolve_codepage(0), CodepageResolution::NoInformation);
        assert_eq!(resolve_codepage(2), CodepageResolution::NoInformation);
        assert_eq!(resolve_codepage(3), CodepageResolution::NoInformation);
    }

    #[test]
    fn unserviceable_codepages_are_unsupported() {
        assert_eq!(resolve_codepage(1), CodepageResolution::Unsupported); // EBCDIC
        assert_eq!(resolve_codepage(437), CodepageResolution::Unsupported); // DOS
        assert_eq!(resolve_codepage(1200), CodepageResolution::Unsupported); // UTF-16LE
        assert_eq!(resolve_codepage(-7), CodepageResolution::Unsupported);
    }

    // -- resolve: override --------------------------------------------------

    #[test]
    fn override_ignores_every_declaration() {
        let mut warnings = Vec::new();
        let resolved = resolve(
            EncodingStrategy::Override(encoding_rs::UTF_8),
            Some("windows-1252"),
            Some(1252),
            &mut warnings,
        )
        .expect("override always resolves");
        assert_eq!(resolved, FileEncoding::Overridden(encoding_rs::UTF_8));
        assert!(warnings.is_empty(), "warnings = {warnings:?}");
    }

    // -- resolve: subtype 20 wins -------------------------------------------

    #[test]
    fn label_wins_over_codepage() {
        let mut warnings = Vec::new();
        let resolved =
            resolve(STRICT, Some("UTF-8"), Some(65001), &mut warnings).expect("resolves");
        assert_eq!(resolved, FileEncoding::Declared(encoding_rs::UTF_8));
        assert!(warnings.is_empty(), "warnings = {warnings:?}");
    }

    #[test]
    fn disagreeing_declarations_warn_and_the_label_wins() {
        let mut warnings = Vec::new();
        let resolved = resolve(STRICT, Some("UTF-8"), Some(1252), &mut warnings).expect("resolves");
        assert_eq!(resolved, FileEncoding::Declared(encoding_rs::UTF_8));
        assert!(
            matches!(
                warnings.as_slice(),
                [SavWarning::EncodingDeclarationMismatch {
                    character_code: 1252,
                    ..
                }]
            ),
            "warnings = {warnings:?}"
        );
    }

    #[test]
    fn equivalent_declarations_spelled_differently_do_not_warn() {
        // Label "us-ascii" and code 28591 both fold to windows-1252.
        let mut warnings = Vec::new();
        let resolved =
            resolve(STRICT, Some("us-ascii"), Some(28591), &mut warnings).expect("resolves");
        assert_eq!(resolved, FileEncoding::Declared(encoding_rs::WINDOWS_1252));
        assert!(warnings.is_empty(), "warnings = {warnings:?}");
    }

    // -- resolve: fall through to subtype 3 ---------------------------------

    #[test]
    fn codepage_is_used_when_no_label_is_present() {
        let mut warnings = Vec::new();
        let resolved = resolve(STRICT, None, Some(65001), &mut warnings).expect("resolves");
        assert_eq!(resolved, FileEncoding::Codepage(encoding_rs::UTF_8));
        assert!(warnings.is_empty(), "warnings = {warnings:?}");
    }

    #[test]
    fn unresolvable_label_falls_through_to_the_codepage() {
        let mut warnings = Vec::new();
        let resolved = resolve(STRICT, Some("UTF-9"), Some(1252), &mut warnings).expect("resolves");
        assert_eq!(resolved, FileEncoding::Codepage(encoding_rs::WINDOWS_1252));
        assert!(
            matches!(
                warnings.as_slice(),
                [SavWarning::EncodingDeclarationUnrecognized { .. }]
            ),
            "warnings = {warnings:?}"
        );
    }

    #[test]
    fn empty_label_is_treated_as_absent() {
        let mut warnings = Vec::new();
        let resolved = resolve(STRICT, Some("   "), Some(65001), &mut warnings).expect("resolves");
        assert_eq!(resolved, FileEncoding::Codepage(encoding_rs::UTF_8));
        assert!(warnings.is_empty(), "warnings = {warnings:?}");
    }

    // -- resolve: nothing declared -----------------------------------------

    #[test]
    fn no_declaration_uses_the_unspecified_fallback_and_warns() {
        let mut warnings = Vec::new();
        let strategy = lenient(encoding_rs::WINDOWS_1252, encoding_rs::UTF_8);
        let resolved = resolve(strategy, None, None, &mut warnings).expect("resolves");
        assert_eq!(
            resolved,
            FileEncoding::Unspecified(encoding_rs::WINDOWS_1252)
        );
        assert!(
            matches!(
                warnings.as_slice(),
                [SavWarning::EncodingUnspecified {
                    used: "windows-1252"
                }]
            ),
            "warnings = {warnings:?}"
        );
    }

    #[test]
    fn uninformative_codepage_is_treated_as_no_declaration() {
        let mut warnings = Vec::new();
        let strategy = lenient(encoding_rs::WINDOWS_1252, encoding_rs::UTF_8);
        let resolved = resolve(strategy, None, Some(2), &mut warnings).expect("resolves");
        assert_eq!(
            resolved,
            FileEncoding::Unspecified(encoding_rs::WINDOWS_1252)
        );
    }

    #[test]
    fn no_declaration_without_a_fallback_errors() {
        let mut warnings = Vec::new();
        let error = resolve(STRICT, None, None, &mut warnings).expect_err("no fallback");
        assert!(
            matches!(error, SavError::EncodingUnspecified),
            "error = {error:?}"
        );
    }

    // -- resolve: unresolvable declarations --------------------------------

    #[test]
    fn unresolvable_declarations_use_the_unrecognized_fallback() {
        let mut warnings = Vec::new();
        let strategy = lenient(encoding_rs::WINDOWS_1252, encoding_rs::UTF_8);
        let resolved = resolve(strategy, Some("UTF-9"), Some(1), &mut warnings).expect("resolves");
        // The `unrecognized` fallback, not the `unspecified` one.
        assert_eq!(resolved, FileEncoding::Unspecified(encoding_rs::UTF_8));
        assert_eq!(warnings.len(), 2, "warnings = {warnings:?}");
    }

    #[test]
    fn unresolvable_label_without_a_fallback_errors_naming_the_label() {
        let mut warnings = Vec::new();
        let error = resolve(STRICT, Some("UTF-9"), None, &mut warnings).expect_err("no fallback");
        match error {
            SavError::EncodingUnrecognized { declaration } => assert_eq!(declaration, "UTF-9"),
            other => panic!("expected EncodingUnrecognized, got {other:?}"),
        }
    }

    #[test]
    fn unsupported_codepage_without_a_fallback_errors_naming_the_code() {
        let mut warnings = Vec::new();
        let error = resolve(STRICT, None, Some(437), &mut warnings).expect_err("no fallback");
        match error {
            SavError::EncodingUnrecognized { declaration } => {
                assert_eq!(declaration, "character_code 437");
            }
            other => panic!("expected EncodingUnrecognized, got {other:?}"),
        }
    }

    #[test]
    fn the_preferred_declaration_names_the_error() {
        // Both declarations are unresolvable; the subtype-20 label is
        // the one reported, since it is the preferred site.
        let mut warnings = Vec::new();
        let error =
            resolve(STRICT, Some("UTF-9"), Some(437), &mut warnings).expect_err("no fallback");
        match error {
            SavError::EncodingUnrecognized { declaration } => assert_eq!(declaration, "UTF-9"),
            other => panic!("expected EncodingUnrecognized, got {other:?}"),
        }
    }
}
