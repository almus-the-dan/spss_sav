//! A string cell, classified against its variable's missing values.

use crate::spss::sav::missing_value_specification::MissingValueSpecification;
use crate::spss::sav::string_value::StringValue;

/// A string cell's contents, and whether the dictionary calls them
/// missing.
///
/// The string counterpart to [`Numeric`](crate::spss::numeric::Numeric),
/// with one difference that shapes the type: **there is no
/// system-missing value for a string.** A string cell always holds
/// bytes, so both variants carry a [`StringValue`] and
/// [`value`](Self::value) is total. What
/// [`Missing`](Self::Missing) records is that the variable declared
/// those bytes missing — `MISSING VALUES name ('XXX')` — not that
/// anything is absent.
///
/// Reading the bytes regardless of classification is therefore always
/// available; use [`present`](Self::present) when a declared-missing
/// value should be treated as no data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Text<'a> {
    /// A cell holding ordinary data.
    Present(StringValue<'a>),
    /// A cell whose value the variable declares missing.
    Missing(StringValue<'a>),
}

impl<'a> Text<'a> {
    /// Classifies `value` against the missing values a variable
    /// declares.
    ///
    /// **The way to build a `Text`.** Which arm a cell belongs in is not
    /// a property of its bytes — it is a fact about the variable the
    /// cell came from, so only a declaration can decide it. Naming an
    /// arm directly asserts an answer this constructor derives, and an
    /// assertion made without a
    /// [`MissingValueSpecification`] in hand is a guess: build
    /// `Text::Present(v)` for a variable declaring `v` missing and the
    /// value now claims to be data when the file says it is not.
    ///
    /// The variants stay public so a cell can be matched on, and
    /// hand-building one remains possible for a caller who genuinely
    /// knows better. This is the path that cannot be wrong.
    #[must_use]
    pub fn classify(value: StringValue<'a>, missing: &MissingValueSpecification) -> Self {
        if missing.matches_bytes(value.raw()) {
            Self::Missing(value)
        } else {
            Self::Present(value)
        }
    }

    /// The cell's contents, whether they are declared missing.
    ///
    /// Total, because a string cell always has bytes — see the
    /// type-level docs.
    #[must_use]
    #[inline]
    pub fn value(&self) -> &StringValue<'a> {
        match self {
            Self::Present(value) | Self::Missing(value) => value,
        }
    }

    /// The cell's contents, or `None` when the variable declares them
    /// missing.
    #[must_use]
    #[inline]
    pub fn present(&self) -> Option<&StringValue<'a>> {
        match self {
            Self::Present(value) => Some(value),
            Self::Missing(_) => None,
        }
    }

    /// Whether the variable declares this cell's value missing.
    #[must_use]
    #[inline]
    pub fn is_missing(&self) -> bool {
        matches!(self, Self::Missing(_))
    }

    /// Takes ownership of the cell's bytes so the value outlives the
    /// row buffer it was read from.
    #[must_use]
    pub fn into_owned(self) -> Text<'static> {
        match self {
            Self::Present(value) => Text::Present(value.into_owned()),
            Self::Missing(value) => Text::Missing(value.into_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declares(values: &[&[u8]]) -> MissingValueSpecification {
        let boxed = values
            .iter()
            .map(|value| value.to_vec().into_boxed_slice())
            .collect();
        MissingValueSpecification::String(boxed)
    }

    /// The constructor derives the arm from the declaration, which is
    /// the whole reason to prefer it over naming a variant.
    #[test]
    fn classify_reads_the_declaration_rather_than_the_bytes() {
        let declared = declares(&[b"cc  \0\0\0\0"]);

        let missing = Text::classify(StringValue::from("cc"), &declared);
        assert!(missing.is_missing(), "the variable declares 'cc' missing");
        assert_eq!(missing.value().raw(), b"cc", "the bytes survive either way");

        let present = Text::classify(StringValue::from("aa"), &declared);
        assert!(!present.is_missing());
    }

    /// The same bytes classify differently for different variables —
    /// which is why no constructor taking only bytes can be right.
    #[test]
    fn the_same_bytes_classify_differently_per_variable() {
        let cell = || StringValue::from("cc");
        assert!(Text::classify(cell(), &declares(&[b"cc"])).is_missing());
        assert!(!Text::classify(cell(), &MissingValueSpecification::None).is_missing());
    }

    #[test]
    fn present_exposes_its_value_both_ways() {
        let text = Text::Present(StringValue::from("aa"));
        assert!(!text.is_missing());
        assert_eq!(text.value().raw(), b"aa");
        assert_eq!(text.present().map(StringValue::raw), Some(&b"aa"[..]));
    }

    /// The point of the type: a declared-missing string still has its
    /// bytes, and `value` hands them over.
    #[test]
    fn missing_keeps_its_bytes_but_is_not_present() {
        let text = Text::Missing(StringValue::from("cc"));
        assert!(text.is_missing());
        assert_eq!(text.value().raw(), b"cc");
        assert!(text.present().is_none());
    }

    #[test]
    fn into_owned_preserves_the_classification() {
        let owned = Text::Missing(StringValue::from("cc")).into_owned();
        assert!(owned.is_missing());
        assert_eq!(owned.value().raw(), b"cc");
    }
}
