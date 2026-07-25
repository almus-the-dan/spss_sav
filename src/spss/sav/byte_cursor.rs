//! Shared low-level reads over an in-memory byte slice.
//!
//! Several extension-record parsers — and, later, record parsing —
//! consume a fixed payload slice as a sequence of length-prefixed
//! binary fields. [`ByteCursor`] centralizes those reads so each
//! parser doesn't re-implement bounds-checked `u32` and byte-string
//! extraction. Truncation is reported as a
//! [`FormatErrorKind::UnexpectedValue`] tagged with the
//! caller-supplied [`Field`], at the cursor's base `position`, in its
//! `section`.
//!
//! This is the binary counterpart to
//! [`text_field`](crate::spss::sav::text_field), which shares
//! fixed-width text decoding.

use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::reader_state::u32_as_usize;
use crate::spss::sav::sav_error::{Field, FormatErrorKind, Result, SavError, Section};

/// A forward-only reader over an in-memory byte slice.
///
/// Each `take_*` method consumes from the front and advances the
/// cursor; returned byte slices borrow from the original data (not the
/// cursor), so they remain valid after later reads.
#[derive(Debug)]
pub(super) struct ByteCursor<'a> {
    data: &'a [u8],
    section: Section,
    position: u64,
}

impl<'a> ByteCursor<'a> {
    /// Creates a cursor over `data`. `section` and `position` tag any
    /// truncation error the cursor raises.
    pub fn new(data: &'a [u8], section: Section, position: u64) -> Self {
        Self {
            data,
            section,
            position,
        }
    }

    /// Returns `true` when the cursor has no bytes left.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Reads one byte, advancing the cursor. Errors (tagged `field`)
    /// when the cursor is empty.
    pub fn take_u8(&mut self, field: Field) -> Result<u8> {
        let (&first, rest) = self
            .data
            .split_first()
            .ok_or_else(|| self.unexpected_value(field))?;
        self.data = rest;
        Ok(first)
    }

    /// Reads a `u32` in `byte_order`, advancing the cursor. Errors
    /// (tagged `field`) when fewer than four bytes remain.
    pub fn take_u32(&mut self, byte_order: ByteOrder, field: Field) -> Result<u32> {
        let bytes: [u8; 4] = self
            .take_bytes(4, field)?
            .try_into()
            .expect("take_bytes(4) yields four bytes");
        Ok(byte_order.read_u32(bytes))
    }

    /// Splits `len` bytes off the front, advancing the cursor. Errors
    /// (tagged `field`) when fewer than `len` bytes remain.
    pub fn take_bytes(&mut self, len: usize, field: Field) -> Result<&'a [u8]> {
        if self.data.len() < len {
            return Err(self.unexpected_value(field));
        }
        let (head, rest) = self.data.split_at(len);
        self.data = rest;
        Ok(head)
    }

    /// Reads a `u32` in `byte_order` and converts it to `usize`,
    /// advancing the cursor. Errors (tagged `field`) on truncation or
    /// when the value exceeds `usize`.
    pub fn take_u32_as_usize(&mut self, byte_order: ByteOrder, field: Field) -> Result<usize> {
        let value = self.take_u32(byte_order, field)?;
        u32_as_usize(value, self.position, self.section, field)
    }

    /// Reads a `u32`-length-prefixed byte string (the length honoring
    /// `byte_order`), advancing past both the prefix and the bytes.
    /// Errors (tagged `field`) on truncation.
    pub fn take_length_prefixed(
        &mut self,
        byte_order: ByteOrder,
        field: Field,
    ) -> Result<&'a [u8]> {
        let len = self.take_u32_as_usize(byte_order, field)?;
        self.take_bytes(len, field)
    }

    /// Builds an `UnexpectedValue` error tagged with `field` at the
    /// cursor's base position and section. Used internally for
    /// truncation, and by callers that reject an in-range-but-invalid
    /// value they just read (e.g., an out-of-bounds count).
    pub fn unexpected_value(&self, field: Field) -> SavError {
        SavError::format(
            self.section,
            self.position,
            FormatErrorKind::UnexpectedValue { field },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_u8_reads_and_advances() {
        let data = [0x01, 0x02];
        let mut cursor = ByteCursor::new(&data, Section::Dictionary, 0);
        assert_eq!(cursor.take_u8(Field::CellValue).unwrap(), 0x01);
        assert_eq!(cursor.take_u8(Field::CellValue).unwrap(), 0x02);
        assert!(cursor.is_empty());
    }

    #[test]
    fn take_u8_errors_when_empty() {
        let mut cursor = ByteCursor::new(&[], Section::Dictionary, 7);
        let err = cursor.take_u8(Field::CellValue).unwrap_err();
        assert_error(&err, Field::CellValue);
    }

    #[test]
    fn take_u32_honors_byte_order() {
        let data = [0x01, 0x00, 0x00, 0x00];
        let mut le = ByteCursor::new(&data, Section::Dictionary, 0);
        assert_eq!(
            le.take_u32(ByteOrder::LittleEndian, Field::CellValue)
                .unwrap(),
            1
        );
        let mut be = ByteCursor::new(&data, Section::Dictionary, 0);
        assert_eq!(
            be.take_u32(ByteOrder::BigEndian, Field::CellValue).unwrap(),
            0x0100_0000
        );
    }

    #[test]
    fn take_u32_errors_when_truncated() {
        let data = [0x01, 0x02, 0x03];
        let mut cursor = ByteCursor::new(&data, Section::Dictionary, 0);
        let err = cursor
            .take_u32(ByteOrder::LittleEndian, Field::CellValue)
            .unwrap_err();
        assert_error(&err, Field::CellValue);
    }

    #[test]
    fn take_bytes_returns_slice_with_data_lifetime() {
        let data = *b"abcd";
        let mut cursor = ByteCursor::new(&data, Section::Dictionary, 0);
        let first = cursor.take_bytes(2, Field::CellValue).unwrap();
        // A later read must not invalidate the earlier borrow.
        let second = cursor.take_bytes(2, Field::CellValue).unwrap();
        assert_eq!(first, b"ab");
        assert_eq!(second, b"cd");
    }

    #[test]
    fn take_bytes_errors_when_too_short() {
        let data = [0x01, 0x02];
        let mut cursor = ByteCursor::new(&data, Section::Dictionary, 0);
        let err = cursor.take_bytes(3, Field::CellValue).unwrap_err();
        assert_error(&err, Field::CellValue);
    }

    #[test]
    fn take_length_prefixed_reads_prefix_then_bytes() {
        let mut data = Vec::new();
        data.extend_from_slice(&3u32.to_le_bytes());
        data.extend_from_slice(b"xyz");
        let mut cursor = ByteCursor::new(&data, Section::Dictionary, 0);
        let bytes = cursor
            .take_length_prefixed(ByteOrder::LittleEndian, Field::CellValue)
            .unwrap();
        assert_eq!(bytes, b"xyz");
        assert!(cursor.is_empty());
    }

    #[test]
    fn take_length_prefixed_errors_when_bytes_missing() {
        let mut data = Vec::new();
        data.extend_from_slice(&5u32.to_le_bytes());
        data.extend_from_slice(b"ab"); // fewer than the declared 5
        let mut cursor = ByteCursor::new(&data, Section::Dictionary, 0);
        let err = cursor
            .take_length_prefixed(ByteOrder::LittleEndian, Field::CellValue)
            .unwrap_err();
        assert_error(&err, Field::CellValue);
    }

    fn assert_error(err: &SavError, expected: Field) {
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue { field: expected }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }
}
