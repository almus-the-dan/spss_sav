//! Reader policy for choosing a text encoding.

/// Strategy for choosing the text encoding when decoding strings in
/// a SAV file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum EncodingStrategy {
    /// Use the encoding declared by the file when one is present;
    /// fall back to a hard-coded last resort (`windows-1252`)
    /// otherwise. Default.
    #[default]
    Fallback,
    /// Use the user-supplied encoding regardless of what the file
    /// declared. A mismatch surfaces as
    /// [`SavWarning::EncodingOverridden`](crate::spss::sav::sav_warning::SavWarning::EncodingOverridden).
    Override,
}
