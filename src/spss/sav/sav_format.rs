//! SAV variable display format.

use crate::spss::sav::sav_format_kind::SavFormatKind;

/// On-disk display format for a SAV variable's print or write slot.
///
/// Each format pairs a [`SavFormatKind`] with a `width` (total
/// character count) and a `decimals` (post-radix character count).
/// All three components are stored as written; no validation is
/// performed at construction time. The writer surfaces invalid
/// `(kind, width, decimals)` triples via
/// [`SavWarning::InvalidFormatCombination`](crate::spss::sav::sav_warning::SavWarning::InvalidFormatCombination)
/// at finish time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SavFormat {
    kind: SavFormatKind,
    width: u8,
    decimals: u8,
}

impl SavFormat {
    /// Returns a fresh [`SavFormatBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> SavFormatBuilder {
        SavFormatBuilder::default()
    }

    /// The format kind.
    #[must_use]
    #[inline]
    pub fn kind(&self) -> SavFormatKind {
        self.kind
    }

    /// Total width of the formatted value, in characters.
    #[must_use]
    #[inline]
    pub fn width(&self) -> u8 {
        self.width
    }

    /// Number of characters after the decimal point.
    #[must_use]
    #[inline]
    pub fn decimals(&self) -> u8 {
        self.decimals
    }
}

/// Builder for [`SavFormat`].
#[derive(Debug, Default, Clone)]
pub struct SavFormatBuilder {
    kind: Option<SavFormatKind>,
    width: u8,
    decimals: u8,
}

impl SavFormatBuilder {
    /// Sets the format kind.
    #[must_use]
    #[inline]
    pub fn kind(mut self, kind: SavFormatKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Sets the total formatted-value width, in characters.
    #[must_use]
    #[inline]
    pub fn width(mut self, width: u8) -> Self {
        self.width = width;
        self
    }

    /// Sets the number of post-decimal-point characters.
    #[must_use]
    #[inline]
    pub fn decimals(mut self, decimals: u8) -> Self {
        self.decimals = decimals;
        self
    }

    /// Finalizes this builder into a [`SavFormat`].
    #[must_use]
    #[inline]
    pub fn build(self) -> SavFormat {
        todo!("body lands with the writer phase")
    }
}
