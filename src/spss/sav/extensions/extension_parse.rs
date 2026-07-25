//! Parsing helpers shared across extension subtypes.

use encoding_rs::Encoding;

use crate::spss::sav::dictionary_format::{
    ATTRIBUTE_NAME_TERMINATOR, ATTRIBUTE_VALUE_QUOTE, ATTRIBUTE_VALUE_TERMINATOR,
    ATTRIBUTE_VALUES_CLOSE, VARIABLE_ATTRIBUTES_SET_SEPARATOR,
};
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

/// Parses one attribute set (the grammar shared by subtypes 17 and
/// 18) from `cursor`, consuming attributes until the cursor is
/// exhausted or a [`VARIABLE_ATTRIBUTES_SET_SEPARATOR`] (`/`) is
/// reached — the separator is consumed. Returns each attribute as its
/// verbatim (still-`[n]`-suffixed) name paired with its list of
/// values, each value's single outer quote pair already stripped.
/// `field` tags any structural error.
pub(crate) fn parse_attribute_set(
    cursor: &mut &[u8],
    encoding: &'static Encoding,
    position: u64,
    field: Field,
) -> Result<Vec<(String, Vec<String>)>> {
    let mut attributes: Vec<(String, Vec<String>)> = Vec::new();
    while let Some(&first) = cursor.first() {
        if first == VARIABLE_ATTRIBUTES_SET_SEPARATOR {
            *cursor = &cursor[1..];
            break;
        }
        // The attribute name runs up to the `(` that opens its values.
        let name_end = cursor.iter().position(|&b| b == ATTRIBUTE_NAME_TERMINATOR);
        let Some(name_end) = name_end else {
            return Err(unexpected_value_error(position, field));
        };
        let name_bytes = &cursor[..name_end];
        if name_bytes.is_empty() {
            return Err(unexpected_value_error(position, field));
        }
        *cursor = &cursor[name_end + 1..];
        let values = parse_attribute_values(cursor, encoding, position, field)?;
        let (name, _, _) = encoding.decode(name_bytes);
        attributes.push((name.into_owned(), values));
    }
    Ok(attributes)
}

/// Parses the parenthesized value list of one attribute, starting with
/// `cursor` positioned just past the opening `(`. Each value is
/// terminated by a line feed; the list ends at the closing `)`, which
/// is consumed. `field` tags any structural error.
fn parse_attribute_values(
    cursor: &mut &[u8],
    encoding: &'static Encoding,
    position: u64,
    field: Field,
) -> Result<Vec<String>> {
    let mut values: Vec<String> = Vec::new();
    loop {
        let value_end = cursor.iter().position(|&b| b == ATTRIBUTE_VALUE_TERMINATOR);
        let Some(value_end) = value_end else {
            return Err(unexpected_value_error(position, field));
        };
        let value = decode_attribute_value(&cursor[..value_end], encoding);
        values.push(value);
        *cursor = &cursor[value_end + 1..];
        match cursor.first() {
            Some(&b) if b == ATTRIBUTE_VALUES_CLOSE => {
                *cursor = &cursor[1..];
                return Ok(values);
            }
            Some(_) => {}
            None => return Err(unexpected_value_error(position, field)),
        }
    }
}

/// Strips the single outer single-quote pair (if both present) from an
/// attribute value and decodes it through `encoding`. Interior bytes
/// are kept verbatim — doubled quotes are not un-doubled, matching
/// PSPP, since values are line-feed-delimited rather than
/// quote-delimited.
fn decode_attribute_value(bytes: &[u8], encoding: &'static Encoding) -> String {
    let inner = if bytes.len() >= 2
        && bytes[0] == ATTRIBUTE_VALUE_QUOTE
        && bytes[bytes.len() - 1] == ATTRIBUTE_VALUE_QUOTE
    {
        &bytes[1..bytes.len() - 1]
    } else {
        bytes
    };
    let (value, _, _) = encoding.decode(inner);
    value.into_owned()
}
