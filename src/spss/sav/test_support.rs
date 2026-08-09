//! Shared helpers for building on-disk SAV byte streams and opening
//! them through the reader, used by the dictionary reader's tests and
//! by the per-extension test modules co-located with each extension
//! type.

use std::io::Cursor;

use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::dictionary_format::{
    DICTIONARY_TERMINATOR_FILLER_LEN, ENDIANNESS_BIG_ENDIAN, ENDIANNESS_LITTLE_ENDIAN,
    EXTENSION_SUBTYPE_MACHINE_INTEGER_INFO, FLOATING_POINT_REPRESENTATION_IEEE,
    MACHINE_INTEGER_INFO_ELEMENT_COUNT, MACHINE_INTEGER_INFO_ELEMENT_SIZE,
    RECORD_TYPE_DICTIONARY_TERMINATOR, RECORD_TYPE_EXTENSION, RECORD_TYPE_VARIABLE,
};
use crate::spss::sav::dictionary_reader::DictionaryReader;
use crate::spss::sav::encoding_strategy::EncodingStrategy;
use crate::spss::sav::header_format::{
    CANONICAL_BIAS, FILE_LABEL_LEN, HEADER_LEN, LAYOUT_CODE_VALUES, MAGIC_FL2, PRODUCT_NAME_LEN,
    TRAILING_PADDING_LEN,
};
use crate::spss::sav::sav_error::{Field, FormatErrorKind, SavError};
use crate::spss::sav::sav_reader::SavReader;
use crate::spss::sav::skippable_content::SkippableContent;

/// Asserts that `err` is a dictionary `UnexpectedValue` format error
/// tagged with `expected`.
pub(crate) fn assert_unexpected_value_error(err: &SavError, expected: Field) {
    match err {
        SavError::Format(e) => assert_eq!(
            e.kind(),
            FormatErrorKind::UnexpectedValue { field: expected }
        ),
        _ => panic!("expected Format error, got {err:?}"),
    }
}

/// Builds a minimal valid fixed header (uncompressed, little-endian,
/// IEEE 754, bias 100.0).
pub(crate) fn build_header(byte_order: ByteOrder) -> Vec<u8> {
    let i32_bytes = |v: i32| match byte_order {
        ByteOrder::LittleEndian => v.to_le_bytes(),
        ByteOrder::BigEndian => v.to_be_bytes(),
    };
    let f64_bytes = |v: f64| match byte_order {
        ByteOrder::LittleEndian => v.to_le_bytes(),
        ByteOrder::BigEndian => v.to_be_bytes(),
    };
    let mut buf = Vec::with_capacity(HEADER_LEN);
    buf.extend_from_slice(MAGIC_FL2);
    let mut prod = [b' '; PRODUCT_NAME_LEN];
    let product = b"@(#) SPSS DATA FIL";
    prod[..product.len()].copy_from_slice(product);
    buf.extend_from_slice(&prod);
    buf.extend_from_slice(&i32_bytes(LAYOUT_CODE_VALUES[0])); // layout_code
    buf.extend_from_slice(&i32_bytes(1)); // nominal_case_size
    buf.extend_from_slice(&i32_bytes(0)); // compression
    buf.extend_from_slice(&i32_bytes(0)); // weight_index
    buf.extend_from_slice(&i32_bytes(0)); // ncases
    buf.extend_from_slice(&f64_bytes(CANONICAL_BIAS)); // bias
    buf.extend_from_slice(b"01 Jan 24");
    buf.extend_from_slice(b"13:45:30");
    let mut label = [b' '; FILE_LABEL_LEN];
    let file_label = b"Test";
    label[..file_label.len()].copy_from_slice(file_label);
    buf.extend_from_slice(&label);
    buf.extend_from_slice(&[0u8; TRAILING_PADDING_LEN]);
    assert_eq!(buf.len(), HEADER_LEN);
    buf
}

/// Opens a byte stream through the reader and advances to the
/// dictionary phase.
pub(crate) fn open(bytes: Vec<u8>) -> DictionaryReader<Cursor<Vec<u8>>> {
    try_open(bytes).unwrap()
}

/// Like [`open`], but with an explicit encoding strategy.
pub(crate) fn open_with(
    bytes: Vec<u8>,
    strategy: EncodingStrategy,
) -> DictionaryReader<Cursor<Vec<u8>>> {
    SavReader::new()
        .encoding_strategy(strategy)
        .from_reader(Cursor::new(bytes))
        .read_header()
        .unwrap()
}

/// Like [`open`], but skipping the given dictionary content.
pub(crate) fn open_skipping(
    bytes: Vec<u8>,
    skipped: &[SkippableContent],
) -> DictionaryReader<Cursor<Vec<u8>>> {
    try_open_skipping(bytes, skipped).unwrap()
}

/// Like [`open_skipping`], but surfaces the error rather than panicking.
pub(crate) fn try_open_skipping(
    bytes: Vec<u8>,
    skipped: &[SkippableContent],
) -> Result<DictionaryReader<Cursor<Vec<u8>>>, SavError> {
    let mut reader = SavReader::new();
    for &content in skipped {
        reader = reader.skip_dictionary_content(content);
    }
    reader.from_reader(Cursor::new(bytes)).read_header()
}

/// Appends a subtype-3 machine integer info record declaring
/// `character_code`, with the endianness and float-format codes matching
/// what [`build_header`] writes so no cross-check warning fires.
pub(crate) fn write_character_code_record(
    buf: &mut Vec<u8>,
    byte_order: ByteOrder,
    character_code: i32,
) {
    let endianness = match byte_order {
        ByteOrder::LittleEndian => ENDIANNESS_LITTLE_ENDIAN,
        ByteOrder::BigEndian => ENDIANNESS_BIG_ENDIAN,
    };
    let fields = [
        1, // version_major
        0, // version_minor
        0, // version_revision
        0, // machine_code
        FLOATING_POINT_REPRESENTATION_IEEE,
        0, // compression_code
        endianness,
        character_code,
    ];
    write_rec_type(buf, byte_order, RECORD_TYPE_EXTENSION);
    write_rec_type(buf, byte_order, EXTENSION_SUBTYPE_MACHINE_INTEGER_INFO);
    write_u32(buf, byte_order, MACHINE_INTEGER_INFO_ELEMENT_SIZE);
    write_u32(buf, byte_order, MACHINE_INTEGER_INFO_ELEMENT_COUNT);
    for field in fields {
        write_rec_type(buf, byte_order, field);
    }
}

/// Like [`open`], but surfaces the error rather than panicking.
///
/// Structural errors anywhere in the dictionary surface from
/// `read_header`, which walks every record to find the file's declared
/// encoding, so tests asserting those errors open the file this way.
pub(crate) fn try_open(bytes: Vec<u8>) -> Result<DictionaryReader<Cursor<Vec<u8>>>, SavError> {
    SavReader::new()
        .from_reader(Cursor::new(bytes))
        .read_header()
}

/// Appends a 4-byte record-type tag in `byte_order`.
pub(crate) fn write_rec_type(buf: &mut Vec<u8>, byte_order: ByteOrder, value: i32) {
    match byte_order {
        ByteOrder::LittleEndian => buf.extend_from_slice(&value.to_le_bytes()),
        ByteOrder::BigEndian => buf.extend_from_slice(&value.to_be_bytes()),
    }
}

/// Appends a `u32` in `byte_order`.
pub(crate) fn write_u32(buf: &mut Vec<u8>, byte_order: ByteOrder, value: u32) {
    match byte_order {
        ByteOrder::LittleEndian => buf.extend_from_slice(&value.to_le_bytes()),
        ByteOrder::BigEndian => buf.extend_from_slice(&value.to_be_bytes()),
    }
}

/// Appends a complete type-7 extension record (tag, subtype,
/// `element_size`, `element_count`, then the payload).
pub(crate) fn write_extension_record(
    buf: &mut Vec<u8>,
    byte_order: ByteOrder,
    subtype: i32,
    element_size: u32,
    element_count: u32,
    payload: &[u8],
) {
    write_rec_type(buf, byte_order, RECORD_TYPE_EXTENSION);
    match byte_order {
        ByteOrder::LittleEndian => buf.extend_from_slice(&subtype.to_le_bytes()),
        ByteOrder::BigEndian => buf.extend_from_slice(&subtype.to_be_bytes()),
    }
    write_u32(buf, byte_order, element_size);
    write_u32(buf, byte_order, element_count);
    buf.extend_from_slice(payload);
}

/// Appends a type-999 dictionary terminator (tag plus 4-byte filler).
pub(crate) fn write_terminator(buf: &mut Vec<u8>, byte_order: ByteOrder) {
    write_rec_type(buf, byte_order, RECORD_TYPE_DICTIONARY_TERMINATOR);
    buf.extend_from_slice(&[0u8; DICTIONARY_TERMINATOR_FILLER_LEN]);
}

/// Packs a `(kind_byte, width, decimals)` triple into the on-disk
/// 4-byte format code (byte 0 = decimals, byte 1 = width, byte 2 =
/// kind, byte 3 = 0).
pub(crate) fn pack_format(kind: u8, width: u8, decimals: u8) -> u32 {
    u32::from_le_bytes([decimals, width, kind, 0])
}

/// Appends a complete type-2 record for one numeric variable: no label,
/// no missing values, an `F8.2` format on both sides.
pub(crate) fn write_numeric_variable(buf: &mut Vec<u8>, byte_order: ByteOrder, name: [u8; 8]) {
    write_rec_type(buf, byte_order, RECORD_TYPE_VARIABLE);
    for value in [0_i32, 0, 0] {
        write_rec_type(buf, byte_order, value);
    }
    write_u32(buf, byte_order, pack_format(5, 8, 2));
    write_u32(buf, byte_order, pack_format(5, 8, 2));
    buf.extend_from_slice(&name);
}

/// Appends a `u32`-length-prefixed byte string in `byte_order`.
pub(crate) fn push_prefixed(buf: &mut Vec<u8>, bytes: &[u8], byte_order: ByteOrder) {
    let len = u32::try_from(bytes.len()).unwrap();
    push_u32(buf, len, byte_order);
    buf.extend_from_slice(bytes);
}

/// Appends a `u32` in `byte_order`.
pub(crate) fn push_u32(buf: &mut Vec<u8>, value: u32, byte_order: ByteOrder) {
    match byte_order {
        ByteOrder::LittleEndian => buf.extend_from_slice(&value.to_le_bytes()),
        ByteOrder::BigEndian => buf.extend_from_slice(&value.to_be_bytes()),
    }
}
