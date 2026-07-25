//! Parsing helpers shared across extension subtypes.

use crate::spss::sav::sav_error::{Field, FormatErrorKind, Result, SavError, Section};

/// Builds a dictionary-section [`FormatErrorKind::UnexpectedValue`]
/// error tagged with `field`, at `position`. Shared by the text and
/// binary extension parsers.
pub(crate) fn unexpected_value_error(position: u64, field: Field) -> SavError {
    SavError::format(
        Section::Dictionary,
        position,
        FormatErrorKind::UnexpectedValue { field },
    )
}

/// Validates a fixed-shape extension envelope: `actual_size` must
/// equal `expected_size` and `actual_count` must equal
/// `expected_count`, else a [`FormatErrorKind::UnexpectedValue`] error
/// tagged with the offending field is returned. Shared by the
/// fixed-layout binary subtypes (3, 4, 16).
pub(crate) fn validate_extension_shape(
    actual_size: u32,
    actual_count: u32,
    expected_size: u32,
    expected_count: u32,
    position: u64,
) -> Result<()> {
    if actual_size != expected_size {
        return Err(unexpected_value_error(
            position,
            Field::ExtensionElementSize,
        ));
    }
    if actual_count != expected_count {
        return Err(unexpected_value_error(
            position,
            Field::ExtensionElementCount,
        ));
    }
    Ok(())
}
