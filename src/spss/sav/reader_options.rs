//! Crate-internal bundle of the options set on a [`SavReaderBuilder`].

use crate::spss::sav::encoding_strategy::EncodingStrategy;

/// Everything
/// [`SavReaderBuilder`](crate::spss::sav::sav_reader_builder::SavReaderBuilder)
/// accumulated, threaded through the reader chain as one value.
///
/// Bundled rather than passed positionally so that adding the next
/// option does not change every constructor signature between the
/// entry point and the code that consults it. That only holds if the
/// bundle really is everything, so an option that happens to be
/// consumed earlier than the rest still lives here rather than
/// traveling alongside.
///
/// How much of the dictionary is retained is deliberately *not* here. It
/// is settled by which terminal method the caller reaches for on
/// [`SavReader`](crate::spss::sav::sav_reader::SavReader) — see
/// [`DictionaryRetention`](crate::spss::sav::dictionary_retention::DictionaryRetention).
#[derive(Debug, Clone, Default)]
pub(crate) struct ReaderOptions {
    /// How the text encoding is chosen.
    encoding_strategy: EncodingStrategy,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The do-nothing policy: honor the file's own encoding declaration.
    #[test]
    fn default_honors_the_declared_encoding() {
        let options = ReaderOptions::default();
        assert_eq!(options.encoding_strategy(), EncodingStrategy::default());
    }

    #[test]
    fn the_encoding_strategy_round_trips() {
        let mut options = ReaderOptions::default();
        let strategy = EncodingStrategy::Override(encoding_rs::WINDOWS_1252);
        options.set_encoding_strategy(strategy);
        assert_eq!(options.encoding_strategy(), strategy);
    }
}
