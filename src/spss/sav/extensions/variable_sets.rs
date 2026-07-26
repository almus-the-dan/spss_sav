//! Subtype 5 — named variable groupings.

use encoding_rs::Encoding;

use crate::spss::sav::dictionary_format::{
    VARIABLE_SETS_ELEMENT_SIZE, VARIABLE_SETS_LINE_CARRIAGE_RETURN, VARIABLE_SETS_LINE_SEPARATOR,
    VARIABLE_SETS_MEMBER_SEPARATOR, VARIABLE_SETS_NAME_TERMINATOR,
};
use crate::spss::sav::dictionary_record::DictionaryRecord;
use crate::spss::sav::extension_envelope::ExtensionEnvelope;
use crate::spss::sav::extensions::extension_parse::unexpected_value_error;
use crate::spss::sav::extensions::extension_record::ExtensionRecord;
use crate::spss::sav::extensions::variable_set::VariableSet;
use crate::spss::sav::sav_error::{Field, Result};

/// Reads a subtype-5 record from `envelope`, yielding the
/// [`VariableSets`]. Forwards the envelope's fields and `encoding` to [`parse`].
#[inline]
pub(crate) fn read(
    envelope: &ExtensionEnvelope,
    encoding: &'static Encoding,
) -> Result<DictionaryRecord> {
    let sets = parse(
        envelope.element_size,
        &envelope.payload,
        encoding,
        envelope.element_size_position,
    )?;
    let record = ExtensionRecord::VariableSets(sets);
    let extension = DictionaryRecord::Extension(record);
    Ok(extension)
}

/// Named variable groupings declared by extension record subtype 5.
///
/// SPSS uses these to organize variables into thematic sets in the
/// dataset editor. The on-disk format is a single text payload with
/// one set per line; the reader exposes the parsed structure rather
/// than the raw text. Wraps the parsed [`VariableSet`]s in declaration
/// order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableSets {
    sets: Vec<VariableSet>,
}

impl VariableSets {
    /// Returns a fresh [`VariableSetsBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> VariableSetsBuilder {
        VariableSetsBuilder::default()
    }

    /// The parsed variable sets, in on-disk (declaration) order.
    #[must_use]
    #[inline]
    pub fn sets(&self) -> &[VariableSet] {
        &self.sets
    }
}

/// Builder for [`VariableSets`].
#[derive(Debug, Default, Clone)]
pub struct VariableSetsBuilder {
    sets: Vec<VariableSet>,
}

impl VariableSetsBuilder {
    /// Appends one variable set.
    #[must_use]
    #[inline]
    pub fn set(mut self, value: VariableSet) -> Self {
        self.sets.push(value);
        self
    }

    /// Replaces the collection with `sets`.
    #[must_use]
    #[inline]
    pub fn sets(mut self, sets: Vec<VariableSet>) -> Self {
        self.sets = sets;
        self
    }

    /// Finalizes this builder into a [`VariableSets`].
    ///
    /// Unset sets default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> VariableSets {
        VariableSets { sets: self.sets }
    }
}

/// Parses an extension subtype-5 payload (named variable groupings)
/// into its [`VariableSets`].
///
/// The payload is one set per line: a set name, `=`, a space, then the
/// members' long variable names separated by spaces; each line ends
/// with a line feed, optionally preceded by a carriage return. A set
/// may have no members. Blank lines (including the trailing line
/// feed's empty segment) are skipped. Names and members are decoded
/// through `encoding`; members are not validated against the schema.
///
/// # Errors
///
/// * [`Field::ExtensionElementSize`] when `actual_size != 1`.
/// * [`Field::VariableSet`] when a non-empty line lacks a `=` or has
///   an empty set name.
fn parse(
    actual_size: u32,
    payload: &[u8],
    encoding: &'static Encoding,
    position: u64,
) -> Result<VariableSets> {
    if actual_size != VARIABLE_SETS_ELEMENT_SIZE {
        return Err(unexpected_value_error(
            position,
            Field::ExtensionElementSize,
        ));
    }
    let mut builder = VariableSets::builder();
    for line in payload.split(|&b| b == VARIABLE_SETS_LINE_SEPARATOR) {
        // A carriage return may precede the line feed; a trailing line
        // feed leaves a final empty segment. Neither is a set.
        let line = match line.split_last() {
            Some((&VARIABLE_SETS_LINE_CARRIAGE_RETURN, head)) => head,
            _ => line,
        };
        if line.is_empty() {
            continue;
        }
        let set = parse_variable_set(line, encoding, position)?;
        builder = builder.set(set);
    }
    let sets = builder.build();
    Ok(sets)
}

/// Parses one `name= members...` line into a [`VariableSet`]. The name
/// runs up to the first `=`; a single space after it (the `= `
/// separator) is skipped, and the remaining space-separated tokens are
/// the members (empty tokens from repeated spaces are dropped).
fn parse_variable_set(
    line: &[u8],
    encoding: &'static Encoding,
    position: u64,
) -> Result<VariableSet> {
    let field = Field::VariableSet;
    let name_end = line
        .iter()
        .position(|&b| b == VARIABLE_SETS_NAME_TERMINATOR);
    let Some(name_end) = name_end else {
        return Err(unexpected_value_error(position, field));
    };
    let name_bytes = &line[..name_end];
    if name_bytes.is_empty() {
        return Err(unexpected_value_error(position, field));
    }
    let members = &line[name_end + 1..];
    // Skip the single space that follows the `=`; any further empty
    // tokens (from repeated spaces) are dropped below.
    let members = match members.split_first() {
        Some((&VARIABLE_SETS_MEMBER_SEPARATOR, rest)) => rest,
        _ => members,
    };
    let mut builder = VariableSet::builder();
    let (name, _, _) = encoding.decode(name_bytes);
    builder = builder.name(name.into_owned());
    let members = members.split(|&b| b == VARIABLE_SETS_MEMBER_SEPARATOR);
    for member in members {
        if member.is_empty() {
            continue;
        }
        let (member, _, _) = encoding.decode(member);
        builder = builder.variable(member.into_owned());
    }
    let set = builder.build();
    Ok(set)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spss::sav::byte_order::ByteOrder;
    use crate::spss::sav::sav_error::{FormatErrorKind, SavError};
    use crate::spss::sav::test_support::{
        assert_unexpected_value_error, build_header, open, write_extension_record, write_terminator,
    };

    #[test]
    fn parse_single_set_with_members() {
        let payload = b"demographics= age sex region\n";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.sets().len(), 1);
        assert_eq!(result.sets()[0].name(), "demographics");
        assert_eq!(
            result.sets()[0].variables(),
            &["age".to_string(), "sex".to_string(), "region".to_string()]
        );
    }

    #[test]
    fn parse_multiple_sets() {
        let payload = b"grp1= a b\ngrp2= c\n";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.sets().len(), 2);
        assert_eq!(result.sets()[0].name(), "grp1");
        assert_eq!(result.sets()[1].name(), "grp2");
        assert_eq!(result.sets()[1].variables(), &["c".to_string()]);
    }

    #[test]
    fn parse_empty_set_has_no_members() {
        let payload = b"empty= \n";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.sets().len(), 1);
        assert_eq!(result.sets()[0].name(), "empty");
        assert!(result.sets()[0].variables().is_empty());
    }

    #[test]
    fn parse_strips_carriage_return() {
        let payload = b"grp= a b\r\n";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(
            result.sets()[0].variables(),
            &["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn parse_ignores_repeated_and_trailing_spaces() {
        let payload = b"grp=  a   b \n";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(
            result.sets()[0].variables(),
            &["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn parse_decodes_through_supplied_encoding() {
        // 0xE9 = é in Windows-1252, invalid in standalone UTF-8.
        let payload = b"caf\xE9= r\xE9gion\n";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.sets()[0].name(), "café");
        assert_eq!(result.sets()[0].variables(), &["région".to_string()]);
    }

    #[test]
    fn parse_empty_payload_yields_no_sets() {
        let result = parse(1, &[], encoding_rs::WINDOWS_1252, 0).unwrap();
        assert!(result.sets().is_empty());
    }

    #[test]
    fn parse_accepts_final_line_without_line_feed() {
        let payload = b"grp= a b";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.sets().len(), 1);
        assert_eq!(
            result.sets()[0].variables(),
            &["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn parse_rejects_wrong_element_size() {
        let err = parse(4, b"grp= a\n", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::ExtensionElementSize);
    }

    #[test]
    fn parse_rejects_line_without_equals() {
        let err = parse(1, b"noequals\n", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::VariableSet);
    }

    #[test]
    fn parse_rejects_empty_set_name() {
        let err = parse(1, b"= a b\n", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::VariableSet);
    }

    #[test]
    fn reader_variable_sets() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"demographics= age sex\nmetrics= height\n";
        write_extension_record(
            &mut bytes,
            byte_order,
            5,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::VariableSets(sets)) = record else {
            panic!("expected VariableSets, got {record:?}");
        };
        assert_eq!(sets.sets().len(), 2);
        assert_eq!(sets.sets()[0].name(), "demographics");
        assert_eq!(
            sets.sets()[0].variables(),
            &["age".to_string(), "sex".to_string()]
        );
        assert_eq!(sets.sets()[1].name(), "metrics");
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn reader_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 5, 4, 2, &[0; 8]);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::ExtensionElementSize,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }
}
