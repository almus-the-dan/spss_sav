//! A single SAV cell value.

use crate::spss::numeric::Numeric;
use crate::spss::sav::text::Text;

/// A single cell value from the data section of a SAV file.
///
/// Both variants report whether the cell counts as data or as missing,
/// and both keep the underlying value either way:
///
/// - [`Numeric`] separates a present `f64` from the system-missing
///   marker and from a value the variable declares missing, the last of
///   which keeps its number in
///   [`MissingValue::UserDefined`](crate::spss::missing_value::MissingValue::UserDefined).
/// - [`Text`] separates a present string from one the variable declares
///   missing. There is no system-missing for strings, so the bytes are
///   always reachable.
///
/// Classification happens while the row is decoded, against the
/// variable's declared missing values. A reader must never hand back a
/// cell claiming to be present when the dictionary says otherwise.
#[derive(Debug, Clone, PartialEq)]
pub enum Value<'a> {
    /// A numeric cell.
    Numeric(Numeric),
    /// A string cell.
    String(Text<'a>),
}

impl<'a> Value<'a> {
    /// The numeric payload, or `None` for a string cell.
    #[must_use]
    #[inline]
    pub fn numeric(&self) -> Option<Numeric> {
        match self {
            Self::Numeric(value) => Some(*value),
            Self::String(_) => None,
        }
    }

    /// The string payload, or `None` for a numeric cell.
    #[must_use]
    #[inline]
    pub fn text(&self) -> Option<&Text<'a>> {
        match self {
            Self::Numeric(_) => None,
            Self::String(value) => Some(value),
        }
    }

    /// Whether this cell is missing — system-missing, or a value the
    /// variable declares missing.
    #[must_use]
    #[inline]
    pub fn is_missing(&self) -> bool {
        match self {
            Self::Numeric(value) => matches!(value, Numeric::Missing(_)),
            Self::String(value) => value.is_missing(),
        }
    }
}

impl<'a> From<Text<'a>> for Value<'a> {
    #[inline]
    fn from(value: Text<'a>) -> Self {
        Self::String(value)
    }
}

impl From<Numeric> for Value<'_> {
    #[inline]
    fn from(value: Numeric) -> Self {
        Self::Numeric(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spss::missing_value::MissingValue;
    use crate::spss::sav::string_value::StringValue;

    fn text(value: &str) -> Value<'_> {
        Value::from(Text::Present(StringValue::from(value)))
    }

    #[test]
    fn a_string_cell_reads_back_its_contents() {
        let value = text("hello");
        let cell = value.text().expect("a string cell");
        assert_eq!(cell.value().raw(), b"hello");
        assert_eq!(cell.value().text(), "hello");
    }

    #[test]
    fn numeric_accessor_discriminates() {
        let numeric = Value::from(Numeric::Present(1.5));
        assert_eq!(numeric.numeric(), Some(Numeric::Present(1.5)));
        assert!(numeric.text().is_none());

        let string = text("x");
        assert!(string.numeric().is_none());
        assert!(string.text().is_some());
    }

    #[test]
    fn is_missing_covers_both_sides() {
        assert!(!Value::from(Numeric::Present(1.0)).is_missing());
        assert!(Value::from(Numeric::Missing(MissingValue::System)).is_missing());
        assert!(Value::from(Numeric::Missing(MissingValue::UserDefined(99.0))).is_missing());
        assert!(!text("aa").is_missing());
        assert!(Value::from(Text::Missing(StringValue::from("cc"))).is_missing());
    }
}
