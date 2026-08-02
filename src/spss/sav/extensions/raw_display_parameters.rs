//! Wire-level payload of an extension subtype-11 record.

use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::dictionary_format::DISPLAY_PARAMETERS_ELEMENT_SIZE;
use crate::spss::sav::dictionary_record::DictionaryRecord;
use crate::spss::sav::extension_envelope::ExtensionEnvelope;
use crate::spss::sav::extensions::extension_record::ExtensionRecord;
use crate::spss::sav::sav_error::{Field, FormatErrorKind, Result, SavError, Section};

/// Raw display-parameter values carried verbatim from an extension
/// subtype-11 record.
///
/// Subtype 11 stores either 2 or 3 unsigned 32-bit values per
/// variable: `(measure, alignment)` in the 2-tuple form, or
/// `(measure, display_width, alignment)` in the 3-tuple form. The
/// reader doesn't decide which form is in play at streaming time —
/// it preserves the values verbatim and defers per-variable slicing
/// to schema finalization, which knows the dictionary's variable
/// count and can drive the 2 vs. 3-tuple split.
///
/// The typed, per-variable [`VariableDisplay`](crate::spss::sav::extensions::variable_display::VariableDisplay)
/// is what finalization produces and attaches to each `SavVariable`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDisplayParameters {
    values: Vec<u32>,
}

impl RawDisplayParameters {
    /// Returns a fresh [`RawDisplayParametersBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> RawDisplayParametersBuilder {
        RawDisplayParametersBuilder::default()
    }

    /// Raw `u32` values in the order they appeared in the record's
    /// payload. Total length is `2 * variable_count` (no display
    /// width) or `3 * variable_count` (with display width).
    #[must_use]
    #[inline]
    pub fn values(&self) -> &[u32] {
        &self.values
    }
}

/// Builder for [`RawDisplayParameters`].
#[derive(Debug, Default, Clone)]
pub struct RawDisplayParametersBuilder {
    values: Vec<u32>,
}

impl RawDisplayParametersBuilder {
    /// Appends `values`.
    #[must_use]
    #[inline]
    pub fn add_values(mut self, values: Vec<u32>) -> Self {
        self.values.extend(values);
        self
    }

    /// Appends one value to the list.
    #[must_use]
    #[inline]
    pub fn add_value(mut self, value: u32) -> Self {
        self.values.push(value);
        self
    }

    /// Finalizes this builder into a [`RawDisplayParameters`].
    ///
    /// An empty list is permitted — it round-trips a subtype-11
    /// record with `element_count == 0`.
    #[must_use]
    #[inline]
    pub fn build(self) -> RawDisplayParameters {
        RawDisplayParameters {
            values: self.values,
        }
    }
}

/// Reads a subtype-11 record from `envelope`, yielding the wire-level
/// [`RawDisplayParameters`]. Forwards the envelope's fields to
/// [`parse`].
#[inline]
pub(crate) fn read(envelope: &ExtensionEnvelope) -> Result<DictionaryRecord> {
    let raw = parse(
        envelope.element_size,
        &envelope.payload,
        envelope.byte_order,
        envelope.element_size_position,
    )?;
    let record = ExtensionRecord::DisplayParameters(raw);
    let extension = DictionaryRecord::Extension(record);
    Ok(extension)
}

/// Parses a subtype-11 payload (per-variable display parameters) into
/// a wire-level [`RawDisplayParameters`].
///
/// The on-disk shape is `element_count` `u32` values, each carrying
/// either a measurement-level code, an optional display-width, or an
/// alignment code. The 2-tuple vs. 3-tuple choice is per-record (all
/// variables get a width or none do) and cannot be recovered without
/// the dictionary's variable count, so the streaming layer preserves
/// the raw values verbatim and defers per-variable slicing to schema
/// finalization.
///
/// # Errors
///
/// Returns [`FormatErrorKind::UnexpectedValue`] tagged
/// [`Field::ExtensionElementSize`] when `actual_size != 4`. The
/// `element_count` is *not* validated here.
///
/// # Panics
///
/// Panics in debug builds if `payload.len()` is not a multiple of 4;
/// the caller reads the payload to a size validated as `element_size *
/// element_count`, so this is a logic invariant.
fn parse(
    actual_size: u32,
    payload: &[u8],
    byte_order: ByteOrder,
    position: u64,
) -> Result<RawDisplayParameters> {
    if actual_size != DISPLAY_PARAMETERS_ELEMENT_SIZE {
        let error = SavError::format(
            Section::Dictionary,
            position,
            FormatErrorKind::UnexpectedValue {
                field: Field::ExtensionElementSize,
            },
        );
        return Err(error);
    }
    debug_assert_eq!(payload.len() % 4, 0);
    let values: Vec<u32> = payload
        .chunks_exact(4)
        .map(|chunk| {
            let bytes: [u8; 4] = chunk.try_into().expect("chunks_exact yields 4-byte slices");
            byte_order.read_u32(bytes)
        })
        .collect();
    let record = RawDisplayParameters::builder().add_values(values).build();
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spss::sav::test_support::{
        build_header, open, write_extension_record, write_terminator,
    };

    fn build_payload(byte_order: ByteOrder, values: &[u32]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(values.len() * 4);
        for &v in values {
            match byte_order {
                ByteOrder::LittleEndian => buf.extend_from_slice(&v.to_le_bytes()),
                ByteOrder::BigEndian => buf.extend_from_slice(&v.to_be_bytes()),
            }
        }
        buf
    }

    #[test]
    fn parse_collects_two_tuple_form_little_endian() {
        // Two variables in 2-tuple form: (measure, alignment) pairs.
        let mut payload = Vec::new();
        payload.extend_from_slice(&1u32.to_le_bytes()); // measure
        payload.extend_from_slice(&0u32.to_le_bytes()); // alignment
        payload.extend_from_slice(&3u32.to_le_bytes()); // measure
        payload.extend_from_slice(&1u32.to_le_bytes()); // alignment
        let raw = parse(4, &payload, ByteOrder::LittleEndian, 0).unwrap();
        assert_eq!(raw.values(), &[1, 0, 3, 1]);
    }

    #[test]
    fn parse_collects_three_tuple_form_little_endian() {
        // One variable in 3-tuple form: measure, width, alignment.
        let mut payload = Vec::new();
        payload.extend_from_slice(&2u32.to_le_bytes()); // measure
        payload.extend_from_slice(&12u32.to_le_bytes()); // width
        payload.extend_from_slice(&1u32.to_le_bytes()); // alignment
        let raw = parse(4, &payload, ByteOrder::LittleEndian, 0).unwrap();
        assert_eq!(raw.values(), &[2, 12, 1]);
    }

    #[test]
    fn parse_big_endian() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&100u32.to_be_bytes());
        payload.extend_from_slice(&200u32.to_be_bytes());
        let raw = parse(4, &payload, ByteOrder::BigEndian, 0).unwrap();
        assert_eq!(raw.values(), &[100, 200]);
    }

    #[test]
    fn parse_empty_payload_yields_empty_record() {
        let raw = parse(4, &[], ByteOrder::LittleEndian, 0).unwrap();
        assert!(raw.values().is_empty());
    }

    #[test]
    fn parse_preserves_unrecognized_codes() {
        // Codes 99 / 7 don't match any MeasurementLevel or Alignment
        // variant; the streaming layer carries them verbatim.
        let mut payload = Vec::new();
        payload.extend_from_slice(&99u32.to_le_bytes());
        payload.extend_from_slice(&7u32.to_le_bytes());
        let raw = parse(4, &payload, ByteOrder::LittleEndian, 0).unwrap();
        assert_eq!(raw.values(), &[99, 7]);
    }

    #[test]
    fn parse_rejects_wrong_element_size() {
        let err = parse(8, &[0; 16], ByteOrder::LittleEndian, 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::ExtensionElementSize
                }
            ),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn reader_two_tuple_form() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let values = [1u32, 0, 3, 1];
        let payload = build_payload(byte_order, &values);
        write_extension_record(
            &mut bytes,
            byte_order,
            11,
            4,
            u32::try_from(values.len()).unwrap(),
            &payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::DisplayParameters(raw)) = record else {
            panic!("expected DisplayParameters, got {record:?}");
        };
        assert_eq!(raw.values(), &values);
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn reader_three_tuple_form() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let values = [2u32, 12, 1];
        let payload = build_payload(byte_order, &values);
        write_extension_record(
            &mut bytes,
            byte_order,
            11,
            4,
            u32::try_from(values.len()).unwrap(),
            &payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::DisplayParameters(raw)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected DisplayParameters");
        };
        assert_eq!(raw.values(), &values);
    }

    #[test]
    fn reader_big_endian() {
        let byte_order = ByteOrder::BigEndian;
        let mut bytes = build_header(byte_order);
        let values = [1u32, 0, 3, 8, 1];
        let payload = build_payload(byte_order, &values);
        write_extension_record(
            &mut bytes,
            byte_order,
            11,
            4,
            u32::try_from(values.len()).unwrap(),
            &payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::DisplayParameters(raw)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected DisplayParameters");
        };
        assert_eq!(raw.values(), &values);
    }

    #[test]
    fn reader_empty_payload_yields_empty_values() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 11, 4, 0, &[]);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::DisplayParameters(raw)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected DisplayParameters");
        };
        assert!(raw.values().is_empty());
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn reader_preserves_unrecognized_codes() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let values = [99u32, 7];
        let payload = build_payload(byte_order, &values);
        write_extension_record(
            &mut bytes,
            byte_order,
            11,
            4,
            u32::try_from(values.len()).unwrap(),
            &payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::DisplayParameters(raw)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected DisplayParameters");
        };
        assert_eq!(raw.values(), &values);
    }

    #[test]
    fn reader_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 11, 8, 2, &[0; 16]);
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
