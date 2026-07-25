//! Subtype 3 — integer-typed environment metadata.

use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::dictionary_format::{
    ENDIANNESS_BIG_ENDIAN, ENDIANNESS_LITTLE_ENDIAN, FLOATING_POINT_REPRESENTATION_IBM_HFP,
    FLOATING_POINT_REPRESENTATION_IEEE, FLOATING_POINT_REPRESENTATION_VAX,
    MACHINE_INTEGER_INFO_ELEMENT_COUNT, MACHINE_INTEGER_INFO_ELEMENT_SIZE,
};
use crate::spss::sav::dictionary_record::DictionaryRecord;
use crate::spss::sav::extension_envelope::ExtensionEnvelope;
use crate::spss::sav::extensions::extension_parse::validate_extension_shape;
use crate::spss::sav::extensions::extension_record::ExtensionRecord;
use crate::spss::sav::float_format::FloatFormat;
use crate::spss::sav::sav_error::Result;
use crate::spss::sav::sav_header::SavHeader;
use crate::spss::sav::sav_warning::SavWarning;

/// Integer-typed environment metadata from extension record subtype
/// 3: version numbers, machine code, floating-point representation,
/// compression code, endianness, and character encoding code.
///
/// Eight `i32` fields, carried verbatim as read from disk. Several
/// of them (notably [`floating_point_representation`] and
/// [`endianness`]) duplicate information the dictionary reader
/// already derived from the file header; the reader exposes both
/// and emits a
/// [`SavWarning`](crate::spss::sav::sav_warning::SavWarning) when
/// the two disagree, leaving final reconciliation to consumers.
///
/// Convenience methods like [`floating_point_representation_kind`]
/// and [`endianness_kind`] map the well-known tagged codes onto the
/// crate's existing [`FloatFormat`] / [`ByteOrder`] enums; they
/// return `None` for codes not in the recognized set.
///
/// [`floating_point_representation`]: Self::floating_point_representation
/// [`endianness`]: Self::endianness
/// [`floating_point_representation_kind`]: Self::floating_point_representation_kind
/// [`endianness_kind`]: Self::endianness_kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineIntegerInfo {
    version_major: i32,
    version_minor: i32,
    version_revision: i32,
    machine_code: i32,
    floating_point_representation: i32,
    compression_code: i32,
    endianness: i32,
    character_code: i32,
}

impl MachineIntegerInfo {
    /// Returns a fresh [`MachineIntegerInfoBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> MachineIntegerInfoBuilder {
        MachineIntegerInfoBuilder::default()
    }

    /// Major version of the SPSS release that wrote the file.
    #[must_use]
    #[inline]
    pub fn version_major(&self) -> i32 {
        self.version_major
    }

    /// Minor version of the SPSS release that wrote the file.
    #[must_use]
    #[inline]
    pub fn version_minor(&self) -> i32 {
        self.version_minor
    }

    /// Revision number of the SPSS release that wrote the file.
    #[must_use]
    #[inline]
    pub fn version_revision(&self) -> i32 {
        self.version_revision
    }

    /// Opaque machine identifier as written by the producing
    /// platform. The value space is not documented to a useful
    /// degree; surface it verbatim.
    #[must_use]
    #[inline]
    pub fn machine_code(&self) -> i32 {
        self.machine_code
    }

    /// Tagged code identifying the file's floating-point
    /// representation (`1` = IEEE 754, `2` = IBM HFP, `3` = VAX).
    /// Use [`floating_point_representation_kind`] for the typed
    /// form.
    ///
    /// [`floating_point_representation_kind`]: Self::floating_point_representation_kind
    #[must_use]
    #[inline]
    pub fn floating_point_representation(&self) -> i32 {
        self.floating_point_representation
    }

    /// Maps [`floating_point_representation`] onto the typed
    /// [`FloatFormat`] enum. Returns `None` for codes outside the
    /// documented set; the raw value is still available via
    /// [`floating_point_representation`].
    ///
    /// [`floating_point_representation`]: Self::floating_point_representation
    #[must_use]
    pub fn floating_point_representation_kind(&self) -> Option<FloatFormat> {
        match self.floating_point_representation {
            FLOATING_POINT_REPRESENTATION_IEEE => Some(FloatFormat::Ieee754),
            FLOATING_POINT_REPRESENTATION_IBM_HFP => Some(FloatFormat::IbmHfp),
            FLOATING_POINT_REPRESENTATION_VAX => Some(FloatFormat::Vax),
            _ => None,
        }
    }

    /// Tagged compression code. SPSS writes `1` for bytecode
    /// compression; other values appear only in malformed or
    /// non-SPSS files.
    #[must_use]
    #[inline]
    pub fn compression_code(&self) -> i32 {
        self.compression_code
    }

    /// Tagged endianness code (`1` = big-endian, `2` = little-
    /// endian). Use [`endianness_kind`] for the typed form.
    ///
    /// [`endianness_kind`]: Self::endianness_kind
    #[must_use]
    #[inline]
    pub fn endianness(&self) -> i32 {
        self.endianness
    }

    /// Maps [`endianness`] onto the typed [`ByteOrder`] enum.
    /// Returns `None` for codes outside the documented set; the
    /// raw value is still available via [`endianness`].
    ///
    /// [`endianness`]: Self::endianness
    #[must_use]
    pub fn endianness_kind(&self) -> Option<ByteOrder> {
        match self.endianness {
            ENDIANNESS_BIG_ENDIAN => Some(ByteOrder::BigEndian),
            ENDIANNESS_LITTLE_ENDIAN => Some(ByteOrder::LittleEndian),
            _ => None,
        }
    }

    /// Opaque character-set code identifying the file's text
    /// encoding. SPSS uses several conventions here (legacy
    /// numeric codes, Windows code pages, locale numbers); the
    /// reader surfaces it verbatim and defers interpretation.
    #[must_use]
    #[inline]
    pub fn character_code(&self) -> i32 {
        self.character_code
    }
}

/// Builder for [`MachineIntegerInfo`].
#[derive(Debug, Default, Clone, Copy)]
pub struct MachineIntegerInfoBuilder {
    version_major: Option<i32>,
    version_minor: Option<i32>,
    version_revision: Option<i32>,
    machine_code: Option<i32>,
    floating_point_representation: Option<i32>,
    compression_code: Option<i32>,
    endianness: Option<i32>,
    character_code: Option<i32>,
}

impl MachineIntegerInfoBuilder {
    /// Sets the major version.
    #[must_use]
    #[inline]
    pub fn version_major(mut self, value: i32) -> Self {
        self.version_major = Some(value);
        self
    }

    /// Sets the minor version.
    #[must_use]
    #[inline]
    pub fn version_minor(mut self, value: i32) -> Self {
        self.version_minor = Some(value);
        self
    }

    /// Sets the revision number.
    #[must_use]
    #[inline]
    pub fn version_revision(mut self, value: i32) -> Self {
        self.version_revision = Some(value);
        self
    }

    /// Sets the machine code.
    #[must_use]
    #[inline]
    pub fn machine_code(mut self, value: i32) -> Self {
        self.machine_code = Some(value);
        self
    }

    /// Sets the floating-point representation code.
    #[must_use]
    #[inline]
    pub fn floating_point_representation(mut self, value: i32) -> Self {
        self.floating_point_representation = Some(value);
        self
    }

    /// Sets the compression code.
    #[must_use]
    #[inline]
    pub fn compression_code(mut self, value: i32) -> Self {
        self.compression_code = Some(value);
        self
    }

    /// Sets the endianness code.
    #[must_use]
    #[inline]
    pub fn endianness(mut self, value: i32) -> Self {
        self.endianness = Some(value);
        self
    }

    /// Sets the character-set code.
    #[must_use]
    #[inline]
    pub fn character_code(mut self, value: i32) -> Self {
        self.character_code = Some(value);
        self
    }

    /// Finalizes this builder into a [`MachineIntegerInfo`].
    ///
    /// Unset fields default to `0`.
    #[must_use]
    #[inline]
    pub fn build(self) -> MachineIntegerInfo {
        let version_major = self.version_major.unwrap_or(0);
        let version_minor = self.version_minor.unwrap_or(0);
        let version_revision = self.version_revision.unwrap_or(0);
        let machine_code = self.machine_code.unwrap_or(0);
        let floating_point_representation = self.floating_point_representation.unwrap_or(0);
        let compression_code = self.compression_code.unwrap_or(0);
        let endianness = self.endianness.unwrap_or(0);
        let character_code = self.character_code.unwrap_or(0);
        MachineIntegerInfo {
            version_major,
            version_minor,
            version_revision,
            machine_code,
            floating_point_representation,
            compression_code,
            endianness,
            character_code,
        }
    }
}

/// Reads a subtype-3 record from `envelope`, cross-checks it against
/// the header, and yields the typed [`MachineIntegerInfo`].
///
/// Stateful relative to the header: it compares the record's
/// byte-order and floating-point codes against the header-derived
/// values and pushes a [`SavWarning`] onto `warnings` for each
/// disagreement (the header stays authoritative).
#[inline]
pub(crate) fn read(
    envelope: &ExtensionEnvelope,
    header: &SavHeader,
    warnings: &mut Vec<SavWarning>,
) -> Result<DictionaryRecord> {
    let info = parse(
        envelope.element_size,
        envelope.element_count,
        &envelope.payload,
        envelope.byte_order,
        envelope.element_size_position,
    )?;
    cross_check(&info, envelope.byte_order, header.float_format(), warnings);
    let record = ExtensionRecord::MachineIntegerInfo(info);
    let extension = DictionaryRecord::Extension(record);
    Ok(extension)
}

/// Parses a subtype-3 payload (machine integer info: 8 `i32` fields
/// holding version numbers, machine code, floating-point
/// representation, compression code, endianness, and character-set
/// code).
///
/// Validates the envelope against the subtype's spec shape
/// (`element_size == 4`, `element_count == 8`) and decodes each 4-byte
/// slot as an `i32` in the file's byte order. No tagged-code
/// validation happens here — the typed accessors on the resulting
/// [`MachineIntegerInfo`] return `None` for unrecognized values, and
/// header cross-checks live in [`read`].
///
/// # Errors
///
/// Returns [`FormatErrorKind::UnexpectedValue`](crate::spss::sav::sav_error::FormatErrorKind::UnexpectedValue)
/// when the envelope shape disagrees with the spec.
///
/// # Panics
///
/// Panics in debug builds if `payload.len()` does not equal `32`; the
/// caller reads the payload from the validated dimensions, so this is
/// a logic invariant.
fn parse(
    actual_size: u32,
    actual_count: u32,
    payload: &[u8],
    byte_order: ByteOrder,
    position: u64,
) -> Result<MachineIntegerInfo> {
    validate_extension_shape(
        actual_size,
        actual_count,
        MACHINE_INTEGER_INFO_ELEMENT_SIZE,
        MACHINE_INTEGER_INFO_ELEMENT_COUNT,
        position,
    )?;
    debug_assert_eq!(payload.len(), 32);
    let i32_at = |offset: usize| -> i32 {
        let bytes: [u8; 4] = payload[offset..offset + 4]
            .try_into()
            .expect("envelope validation guarantees a 32-byte payload");
        byte_order.read_i32(bytes)
    };
    let info = MachineIntegerInfo::builder()
        .version_major(i32_at(0))
        .version_minor(i32_at(4))
        .version_revision(i32_at(8))
        .machine_code(i32_at(12))
        .floating_point_representation(i32_at(16))
        .compression_code(i32_at(20))
        .endianness(i32_at(24))
        .character_code(i32_at(28))
        .build();
    Ok(info)
}

/// Compares the byte-order and floating-point codes carried by a
/// subtype-3 record against the header-derived values, pushing a
/// [`SavWarning::HeaderByteOrderMismatch`] or
/// [`SavWarning::HeaderFloatFormatMismatch`] onto `warnings` for each
/// disagreement. Unknown codes (those for which the typed accessors
/// return `None`) are tolerated silently — the record's raw code
/// remains available on [`MachineIntegerInfo`].
fn cross_check(
    info: &MachineIntegerInfo,
    header_byte_order: ByteOrder,
    header_float_format: FloatFormat,
    warnings: &mut Vec<SavWarning>,
) {
    if let Some(record_byte_order) = info.endianness_kind()
        && record_byte_order != header_byte_order
    {
        warnings.push(SavWarning::HeaderByteOrderMismatch {
            record_value: info.endianness(),
        });
    }
    if let Some(record_format) = info.floating_point_representation_kind()
        && record_format != header_float_format
    {
        warnings.push(SavWarning::HeaderFloatFormatMismatch {
            record_value: info.floating_point_representation(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spss::sav::sav_error::{Field, FormatErrorKind, SavError};
    use crate::spss::sav::test_support::{
        build_header, open, write_extension_record, write_terminator,
    };

    /// Builds a 32-byte subtype-3 payload from 8 i32 fields in the
    /// given byte order.
    fn build_payload(byte_order: ByteOrder, fields: [i32; 8]) -> Vec<u8> {
        let to_bytes = |v: i32| match byte_order {
            ByteOrder::LittleEndian => v.to_le_bytes(),
            ByteOrder::BigEndian => v.to_be_bytes(),
        };
        let mut buf = Vec::with_capacity(32);
        for value in fields {
            buf.extend_from_slice(&to_bytes(value));
        }
        buf
    }

    #[test]
    fn reader_machine_integer_info() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // version 25.0.0, machine code 720, IEEE/standard compression,
        // little-endian, character code 1252 (Windows-1252).
        let fields = [25, 0, 0, 720, 1, 1, 2, 1252];
        let payload = build_payload(byte_order, fields);
        write_extension_record(&mut bytes, byte_order, 3, 4, 8, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::MachineIntegerInfo(info)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected MachineIntegerInfo");
        };
        assert_eq!(info.version_major(), 25);
        assert_eq!(info.version_minor(), 0);
        assert_eq!(info.version_revision(), 0);
        assert_eq!(info.machine_code(), 720);
        assert_eq!(info.floating_point_representation(), 1);
        assert_eq!(info.compression_code(), 1);
        assert_eq!(info.endianness(), 2);
        assert_eq!(info.character_code(), 1252);
        assert_eq!(
            info.floating_point_representation_kind(),
            Some(FloatFormat::Ieee754),
        );
        assert_eq!(info.endianness_kind(), Some(ByteOrder::LittleEndian));
        // Header byte order (LE) matches record (2 → LE), float format
        // matches (IEEE), so no cross-check warnings.
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn reader_big_endian_payload() {
        let byte_order = ByteOrder::BigEndian;
        let mut bytes = build_header(byte_order);
        let fields = [25, 0, 0, 720, 1, 1, 1, 1252];
        let payload = build_payload(byte_order, fields);
        write_extension_record(&mut bytes, byte_order, 3, 4, 8, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::MachineIntegerInfo(info)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected MachineIntegerInfo");
        };
        assert_eq!(info.machine_code(), 720);
        assert_eq!(info.endianness_kind(), Some(ByteOrder::BigEndian));
    }

    #[test]
    fn reader_unknown_codes_return_none_from_typed_accessors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // 99 for both floating-point representation and endianness —
        // neither maps onto a recognized enum variant.
        let fields = [25, 0, 0, 720, 99, 1, 99, 1252];
        let payload = build_payload(byte_order, fields);
        write_extension_record(&mut bytes, byte_order, 3, 4, 8, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::MachineIntegerInfo(info)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected MachineIntegerInfo");
        };
        assert_eq!(info.floating_point_representation(), 99);
        assert_eq!(info.endianness(), 99);
        assert!(info.floating_point_representation_kind().is_none());
        assert!(info.endianness_kind().is_none());
        // Unknown codes don't trigger the cross-check warnings — there's
        // nothing to compare against.
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn reader_byte_order_mismatch_warns() {
        // Header is little-endian, but the record claims big-endian
        // (code 1).
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let fields = [25, 0, 0, 720, 1, 1, 1, 1252];
        let payload = build_payload(byte_order, fields);
        write_extension_record(&mut bytes, byte_order, 3, 4, 8, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        dict.read_record().unwrap().unwrap();
        assert!(matches!(
            dict.warnings(),
            &[SavWarning::HeaderByteOrderMismatch { record_value: 1 }]
        ));
    }

    #[test]
    fn reader_float_format_mismatch_warns() {
        // Header is IEEE 754 (the default test fixture uses IEEE bias
        // 100.0); the record claims IBM HFP (code 2).
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let fields = [25, 0, 0, 720, 2, 1, 2, 1252];
        let payload = build_payload(byte_order, fields);
        write_extension_record(&mut bytes, byte_order, 3, 4, 8, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        dict.read_record().unwrap().unwrap();
        assert!(matches!(
            dict.warnings(),
            &[SavWarning::HeaderFloatFormatMismatch { record_value: 2 }]
        ));
    }

    #[test]
    fn reader_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 3, 8, 8, &[0; 64]);

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

    #[test]
    fn reader_wrong_element_count_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 3, 4, 4, &[0; 16]);

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::ExtensionElementCount,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }
}
