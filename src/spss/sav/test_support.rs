//! Shared helpers for building on-disk SAV byte streams and opening
//! them through the reader, used by the dictionary reader's tests and
//! by the per-extension test modules co-located with each extension
//! type.

use std::io::Cursor;

use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::dictionary_reader::DictionaryReader;
use crate::spss::sav::sav_reader::SavReader;

/// Builds a minimal valid 176-byte SAV header (uncompressed,
/// little-endian, IEEE 754, bias 100.0).
pub(crate) fn build_header(byte_order: ByteOrder) -> Vec<u8> {
    let i32_bytes = |v: i32| match byte_order {
        ByteOrder::LittleEndian => v.to_le_bytes(),
        ByteOrder::BigEndian => v.to_be_bytes(),
    };
    let f64_bytes = |v: f64| match byte_order {
        ByteOrder::LittleEndian => v.to_le_bytes(),
        ByteOrder::BigEndian => v.to_be_bytes(),
    };
    let mut buf = Vec::with_capacity(176);
    buf.extend_from_slice(b"$FL2");
    let mut prod = [b' '; 60];
    prod[..18].copy_from_slice(b"@(#) SPSS DATA FIL");
    buf.extend_from_slice(&prod);
    buf.extend_from_slice(&i32_bytes(2)); // layout_code
    buf.extend_from_slice(&i32_bytes(1)); // nominal_case_size
    buf.extend_from_slice(&i32_bytes(0)); // compression
    buf.extend_from_slice(&i32_bytes(0)); // weight_index
    buf.extend_from_slice(&i32_bytes(0)); // ncases
    buf.extend_from_slice(&f64_bytes(100.0)); // bias
    buf.extend_from_slice(b"01 Jan 24");
    buf.extend_from_slice(b"13:45:30");
    let mut label = [b' '; 64];
    label[..4].copy_from_slice(b"Test");
    buf.extend_from_slice(&label);
    buf.extend_from_slice(&[0u8; 3]);
    assert_eq!(buf.len(), 176);
    buf
}

/// Opens a byte stream through the reader and advances to the
/// dictionary phase.
pub(crate) fn open(bytes: Vec<u8>) -> DictionaryReader<Cursor<Vec<u8>>> {
    SavReader::new()
        .from_reader(Cursor::new(bytes))
        .read_header()
        .unwrap()
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
    write_rec_type(buf, byte_order, 7);
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
    write_rec_type(buf, byte_order, 999);
    buf.extend_from_slice(&[0u8; 4]);
}
