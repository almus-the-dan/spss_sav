//! Crate-internal bundle of the options set on a [`SavReader`].

use std::collections::HashSet;

use crate::spss::sav::dictionary_record_kind::DictionaryRecordKind;
use crate::spss::sav::encoding_strategy::EncodingStrategy;
use crate::spss::sav::skippable_content::SkippableContent;

/// Everything
/// [`SavReader`](crate::spss::sav::sav_reader::SavReader) accumulated,
/// threaded through the reader chain as one value.
///
/// Bundled rather than passed positionally so that adding the next
/// option does not change every constructor signature between the
/// entry point and the code that consults it. That only holds if the
/// bundle really is everything, so an option that happens to be
/// consumed earlier than the rest still lives here rather than
/// traveling alongside.
#[derive(Debug, Clone)]
pub(crate) struct ReaderOptions {
    /// How the text encoding is chosen.
    encoding_strategy: EncodingStrategy,
    /// Content the caller asked not to be retained. Exclusion rather
    /// than inclusion, so the empty set unambiguously means "yield
    /// everything" and [`Default`] is the do-nothing policy. A bitset
    /// is a non-breaking swap later — nothing here is public.
    skipped: HashSet<SkippableContent>,
}

impl ReaderOptions {
    /// Sets how the text encoding is chosen.
    pub fn set_encoding_strategy(&mut self, strategy: EncodingStrategy) {
        self.encoding_strategy = strategy;
    }

    /// How the text encoding is chosen.
    pub fn encoding_strategy(&self) -> EncodingStrategy {
        self.encoding_strategy
    }

    /// Records that `content` should not be retained.
    pub fn skip(&mut self, content: SkippableContent) {
        self.skipped.insert(content);
    }

    /// Whether a record of this kind should be skipped.
    ///
    /// Variable records are never skippable, and the terminator is not
    /// a record kind, so this is total over what the buffer can offer.
    pub fn skips(&self, kind: DictionaryRecordKind) -> bool {
        let content = match kind {
            DictionaryRecordKind::Variable => return false,
            DictionaryRecordKind::ValueLabelSet => SkippableContent::ValueLabels,
            DictionaryRecordKind::Document => SkippableContent::Documents,
            DictionaryRecordKind::Extension(subtype) => SkippableContent::Extension(subtype),
        };
        self.skipped.contains(&content)
    }
}

impl Default for ReaderOptions {
    /// Honor the file's own encoding declaration and retain
    /// everything — the policy a caller who set no options gets.
    fn default() -> Self {
        Self {
            encoding_strategy: EncodingStrategy::default(),
            skipped: HashSet::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spss::sav::extensions::extension_subtype::ExtensionSubtype;

    #[test]
    fn default_skips_nothing_and_builds_a_schema() {
        let options = ReaderOptions::default();
        assert_eq!(
            options.encoding_strategy(),
            crate::spss::sav::encoding_strategy::EncodingStrategy::default(),
        );
        assert!(!options.skips(DictionaryRecordKind::Document));
        assert!(!options.skips(DictionaryRecordKind::ValueLabelSet));
        assert!(!options.skips(DictionaryRecordKind::Extension(ExtensionSubtype::Uuid)));
    }

    #[test]
    fn variable_records_are_never_skipped() {
        let mut options = ReaderOptions::default();
        options.skip(SkippableContent::ValueLabels);
        options.skip(SkippableContent::Documents);
        options.skip(SkippableContent::Extension(ExtensionSubtype::Uuid));
        assert!(!options.skips(DictionaryRecordKind::Variable));
    }

    #[test]
    fn skipping_one_kind_leaves_the_others_alone() {
        let mut options = ReaderOptions::default();
        options.skip(SkippableContent::Documents);
        assert!(options.skips(DictionaryRecordKind::Document));
        assert!(!options.skips(DictionaryRecordKind::ValueLabelSet));
        assert!(!options.skips(DictionaryRecordKind::Extension(ExtensionSubtype::Uuid)));
    }

    #[test]
    fn extension_skipping_is_per_subtype() {
        let mut options = ReaderOptions::default();
        options.skip(SkippableContent::Extension(
            ExtensionSubtype::LongValueLabels,
        ));
        assert!(options.skips(DictionaryRecordKind::Extension(
            ExtensionSubtype::LongValueLabels
        )));
        assert!(!options.skips(DictionaryRecordKind::Extension(ExtensionSubtype::Uuid)));
    }

    /// `Unrecognized` stands for every unparsed subtype at once, so one
    /// entry covers all of them.
    #[test]
    fn unrecognized_covers_every_unparsed_subtype() {
        let mut options = ReaderOptions::default();
        options.skip(SkippableContent::Extension(ExtensionSubtype::Unrecognized));
        assert!(options.skips(DictionaryRecordKind::Extension(
            ExtensionSubtype::from_code(24)
        )));
        assert!(options.skips(DictionaryRecordKind::Extension(
            ExtensionSubtype::from_code(15)
        )));
    }
}
