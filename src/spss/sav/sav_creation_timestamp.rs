//! Creation timestamp recorded in a SAV file header.

use crate::spss::sav::parsed_sav_timestamp::ParsedSavTimestamp;

/// Creation timestamp recorded in a SAV file header.
///
/// Parsing is all-or-nothing: if every component (date and time) was
/// recognized, the result is [`Parsed`](Self::Parsed); if any component
/// failed to parse, the original raw strings are preserved verbatim
/// in [`Raw`](Self::Unparsed).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SavCreationTimestamp {
    /// Both date and time parsed successfully.
    Parsed(ParsedSavTimestamp),
    /// At least one part was unparseable; the raw strings are
    /// preserved.
    #[non_exhaustive]
    Unparsed {
        /// Raw date string from the header.
        date: String,
        /// Raw time string from the header.
        time: String,
    },
}
