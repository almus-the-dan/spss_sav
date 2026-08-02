//! A single SAV cell value.

use crate::spss::numeric::Numeric;
use crate::spss::sav::string_value::StringValue;

/// A single cell value from the data section of a SAV file.
///
/// The [`Numeric`](Self::Numeric) variant uses [`Numeric`] to
/// distinguish present `f64` data from the system-missing designation.
/// User-defined missing values are deliberately *not* tagged here: they
/// are a property of the variable's declared
/// [`MissingValueSpecification`](crate::spss::sav::missing_value_specification::MissingValueSpecification),
/// which lives on the schema, and the schema is optional. Tagging them
/// per cell would make a cell's meaning depend on whether schema
/// building was switched on.
///
/// The [`String`](Self::String) variant carries a [`StringValue`],
/// which keeps the cell's raw bytes alongside the encoding needed to
/// read them as text.
#[derive(Debug, Clone, PartialEq)]
pub enum Value<'a> {
    /// A numeric cell.
    Numeric(Numeric),
    /// A string cell.
    String(StringValue<'a>),
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
    pub fn string(&self) -> Option<&StringValue<'a>> {
        match self {
            Self::Numeric(_) => None,
            Self::String(value) => Some(value),
        }
    }
}

impl<'a> From<&'a str> for Value<'a> {
    #[inline]
    fn from(text: &'a str) -> Self {
        Self::String(StringValue::from(text))
    }
}

impl<'a> From<StringValue<'a>> for Value<'a> {
    #[inline]
    fn from(value: StringValue<'a>) -> Self {
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

    #[test]
    fn from_str_borrows_input() {
        let value = Value::from("hello");
        let string = value.string().expect("a string cell");
        assert_eq!(string.raw(), b"hello");
        assert_eq!(string.text(), "hello");
    }

    #[test]
    fn from_str_equality() {
        assert_eq!(Value::from("hi"), Value::String(StringValue::from("hi")));
    }

    #[test]
    fn into_works_with_type_inference() {
        let value: Value<'_> = "x".into();
        assert_eq!(value, Value::String(StringValue::from("x")));
    }

    #[test]
    fn numeric_accessor_discriminates() {
        let numeric = Value::from(Numeric::Present(1.5));
        assert_eq!(numeric.numeric(), Some(Numeric::Present(1.5)));
        assert!(numeric.string().is_none());

        let string = Value::from("x");
        assert!(string.numeric().is_none());
        assert!(string.string().is_some());
    }
}
