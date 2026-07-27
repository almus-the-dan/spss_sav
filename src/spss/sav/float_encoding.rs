//! How a SAV file encodes an `f64` on disk.

use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::float_format::FloatFormat;
use crate::spss::sav::sav_error::{Result, SavError};

/// How a SAV file encodes an `f64` on disk — the file's declared
/// [`FloatFormat`] paired with its [`ByteOrder`].
///
/// A double's on-disk bytes are a function of three things: the value,
/// the file's float format, and its byte order. The latter two are
/// file-wide facts fixed by the header, so they travel together as one
/// value rather than as two loose parameters that a caller could pair
/// up wrongly.
///
/// There is no public constructor. The only way to obtain a
/// `FloatEncoding` is
/// [`SavHeader::float_encoding`](crate::spss::sav::sav_header::SavHeader::float_encoding),
/// which is the point: a bit pattern only means something relative to
/// a particular file, so a caller has to name that file's byte order
/// and float format — by reading a header, or by building one — before
/// it can convert anything.
///
/// There is deliberately no `Default` impl. A silent
/// IEEE-754/little-endian encoding is precisely the mistake this type
/// exists to prevent.
///
/// # Byte order applies to IEEE 754 only
///
/// The other three formats carry their byte order in the format
/// itself: IBM HFP is always big-endian, and both VAX encodings always
/// use VAX word order. `decode` and `encode` therefore ignore
/// [`byte_order`](Self::byte_order) for those.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FloatEncoding {
    format: FloatFormat,
    byte_order: ByteOrder,
}

impl FloatEncoding {
    /// Pairs a float format with a byte order.
    ///
    /// Crate-internal by design: users reach a `FloatEncoding` through
    /// [`SavHeader::float_encoding`](crate::spss::sav::sav_header::SavHeader::float_encoding)
    /// so that the encoding always describes a real file's header
    /// rather than an assumption.
    #[must_use]
    #[inline]
    pub(crate) fn new(format: FloatFormat, byte_order: ByteOrder) -> Self {
        Self { format, byte_order }
    }

    /// On-disk floating-point representation.
    #[must_use]
    #[inline]
    pub fn format(self) -> FloatFormat {
        self.format
    }

    /// Byte order of multibyte values.
    ///
    /// Governs the on-disk layout for IEEE 754 only; see the type-level
    /// docs.
    #[must_use]
    #[inline]
    pub fn byte_order(self) -> ByteOrder {
        self.byte_order
    }

    /// Decodes eight on-disk bytes into an `f64`.
    ///
    /// Infallible: every value the three legacy formats can represent
    /// fits in an `f64`, since all of them top out well below
    /// [`f64::MAX`] (IBM HFP around 7.2e75, VAX `D_float`/`G_float`
    /// around 1.7e38 and 8.9e307). A VAX reserved operand — the one
    /// bit pattern with no numeric meaning — decodes to NaN.
    #[must_use]
    pub fn decode(self, bytes: [u8; 8]) -> f64 {
        match self.format {
            FloatFormat::Ieee754 => self.byte_order.read_f64(bytes),
            FloatFormat::IbmHfp => f64::from(ibm_hfp::IbmFloat64::from_be_bytes(bytes)),
            FloatFormat::VaxDFloat => vax_floating::DFloating::from_le_bytes(bytes).to_f64(),
            FloatFormat::VaxGFloat => vax_floating::GFloating::from_le_bytes(bytes).to_f64(),
        }
    }

    /// Encodes an `f64` into its eight on-disk bytes.
    ///
    /// # Errors
    ///
    /// Returns [`SavError::FloatNotRepresentable`] when `value` has no
    /// encoding in this file's float format. IEEE 754 accepts every
    /// `f64`. The other three have no NaN or infinity encoding and a
    /// far narrower exponent range, so ordinary `f64` values —
    /// including [`f64::MAX`] — fail there.
    ///
    /// VAX has no negative zero — that bit pattern is the reserved
    /// operand — so `-0.0` is normalized to `+0.0` rather than
    /// rejected. It is the one value that survives encoding without
    /// round-tripping.
    pub fn encode(self, value: f64) -> Result<[u8; 8]> {
        let not_representable = || SavError::FloatNotRepresentable {
            value,
            format: self.format,
        };
        match self.format {
            FloatFormat::Ieee754 => Ok(self.byte_order.write_f64(value)),
            FloatFormat::IbmHfp => ibm_hfp::IbmFloat64::try_from(value)
                .map(ibm_hfp::IbmFloat64::to_be_bytes)
                .map_err(|_| not_representable()),
            FloatFormat::VaxDFloat => {
                let vax = vax_floating::DFloating::from_f64(value);
                if vax.is_reserved() {
                    return Err(not_representable());
                }
                Ok(vax.to_le_bytes())
            }
            FloatFormat::VaxGFloat => {
                let vax = vax_floating::GFloating::from_f64(value);
                if vax.is_reserved() {
                    return Err(not_representable());
                }
                Ok(vax.to_le_bytes())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `100.0` in IBM HFP, big-endian: characteristic `0x42`
    /// (exponent 2, excess-64) and mantissa `0.640000₁₆`, i.e.
    /// `0.390625 × 16² == 100.0`.
    const IBM_HUNDRED_BE: [u8; 8] = [0x42, 0x64, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];

    fn ieee(byte_order: ByteOrder) -> FloatEncoding {
        FloatEncoding::new(FloatFormat::Ieee754, byte_order)
    }

    fn ibm(byte_order: ByteOrder) -> FloatEncoding {
        FloatEncoding::new(FloatFormat::IbmHfp, byte_order)
    }

    #[test]
    fn accessors_return_the_pair() {
        let encoding = ibm(ByteOrder::BigEndian);
        assert_eq!(encoding.format(), FloatFormat::IbmHfp);
        assert_eq!(encoding.byte_order(), ByteOrder::BigEndian);
    }

    #[test]
    fn ieee_encodes_in_the_declared_byte_order() {
        let value = -f64::MAX;
        assert_eq!(
            ieee(ByteOrder::LittleEndian).encode(value).unwrap(),
            value.to_le_bytes(),
        );
        assert_eq!(
            ieee(ByteOrder::BigEndian).encode(value).unwrap(),
            value.to_be_bytes(),
        );
    }

    #[test]
    fn ieee_byte_order_is_not_cosmetic() {
        // -DBL_MAX is asymmetric, so a wrong-endian writer cannot hide
        // behind a palindromic bit pattern.
        let little = ieee(ByteOrder::LittleEndian).encode(-f64::MAX).unwrap();
        let big = ieee(ByteOrder::BigEndian).encode(-f64::MAX).unwrap();
        assert_ne!(little, big);
        assert_eq!(little, [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xEF, 0xFF]);
        assert_eq!(big, [0xFF, 0xEF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
    }

    #[test]
    fn ieee_round_trips_both_byte_orders() {
        // Compared bit-for-bit rather than by value: a sentinel is a
        // bit pattern, so `-0.0` must not pass for `0.0`.
        for byte_order in [ByteOrder::LittleEndian, ByteOrder::BigEndian] {
            let encoding = ieee(byte_order);
            for value in [0.0_f64, -0.0, 1.0, -100.0, f64::MAX, -f64::MAX] {
                let bytes = encoding.encode(value).unwrap();
                assert_eq!(
                    encoding.decode(bytes).to_bits(),
                    value.to_bits(),
                    "{value} {byte_order:?}",
                );
            }
        }
    }

    #[test]
    fn ieee_carries_nan_bit_patterns_through() {
        let encoding = ieee(ByteOrder::LittleEndian);
        let bytes = encoding.encode(f64::NAN).unwrap();
        assert!(encoding.decode(bytes).is_nan());
    }

    #[test]
    fn ibm_decodes_a_known_pattern() {
        // The same big-endian bytes under either declared byte order:
        // IBM HFP's layout is fixed by the format.
        for byte_order in [ByteOrder::BigEndian, ByteOrder::LittleEndian] {
            let decoded = ibm(byte_order).decode(IBM_HUNDRED_BE);
            assert_eq!(decoded.to_bits(), 100.0_f64.to_bits(), "{byte_order:?}");
        }
    }

    #[test]
    fn ibm_encodes_a_known_pattern() {
        assert_eq!(
            ibm(ByteOrder::BigEndian).encode(100.0).unwrap(),
            IBM_HUNDRED_BE,
        );
    }

    #[test]
    fn ibm_round_trips() {
        for byte_order in [ByteOrder::LittleEndian, ByteOrder::BigEndian] {
            let encoding = ibm(byte_order);
            for value in [0.0_f64, 1.0, -100.0, 1234.5] {
                let bytes = encoding.encode(value).unwrap();
                assert_eq!(
                    encoding.decode(bytes).to_bits(),
                    value.to_bits(),
                    "{value} {byte_order:?}",
                );
            }
        }
    }

    #[test]
    fn ibm_rejects_values_outside_its_range() {
        let encoding = ibm(ByteOrder::BigEndian);
        // The canonical IEEE system-missing sentinel is one of these:
        // -DBL_MAX has no IBM HFP encoding, so the sentinel triple is
        // format-specific rather than universal.
        for value in [f64::MAX, -f64::MAX, f64::NAN, f64::INFINITY] {
            let err = encoding.encode(value).unwrap_err();
            assert!(
                matches!(
                    err,
                    SavError::FloatNotRepresentable {
                        format: FloatFormat::IbmHfp,
                        ..
                    }
                ),
                "{value} gave {err:?}",
            );
        }
    }

    #[test]
    fn ibm_ignores_the_declared_byte_order() {
        // PSPP's `FLOAT_Z_LONG` has no endianness variants; the format
        // fixes the layout, so both encodings must agree.
        assert_eq!(
            ibm(ByteOrder::LittleEndian).encode(100.0).unwrap(),
            ibm(ByteOrder::BigEndian).encode(100.0).unwrap(),
        );
    }

    fn vax_d(byte_order: ByteOrder) -> FloatEncoding {
        FloatEncoding::new(FloatFormat::VaxDFloat, byte_order)
    }

    fn vax_g(byte_order: ByteOrder) -> FloatEncoding {
        FloatEncoding::new(FloatFormat::VaxGFloat, byte_order)
    }

    #[test]
    fn vax_encodes_known_patterns() {
        // Hand-derived from the VAX float layouts, independent of the
        // library. D_float: 100.0 == 0.78125 × 2^7, so the biased
        // exponent is 7 + 128 == 0x87 and the leading fraction bits are
        // 1001000, giving a first word of 0100_0011_1100_1000 == 0x43C8
        // — stored low byte first. G_float: exponent 7 + 1024 == 0x407
        // over a 4-bit leading fraction 1001, giving 0x4079.
        assert_eq!(
            vax_d(ByteOrder::LittleEndian).encode(100.0).unwrap(),
            [0xC8, 0x43, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        );
        assert_eq!(
            vax_g(ByteOrder::LittleEndian).encode(100.0).unwrap(),
            [0x79, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        );
    }

    #[test]
    fn vax_round_trips() {
        for encoding in [
            vax_d(ByteOrder::LittleEndian),
            vax_g(ByteOrder::LittleEndian),
        ] {
            for value in [0.0_f64, 1.0, 12.5, -100.0, 1234.5] {
                let bytes = encoding.encode(value).unwrap();
                assert_eq!(
                    encoding.decode(bytes).to_bits(),
                    value.to_bits(),
                    "{value} {:?}",
                    encoding.format(),
                );
            }
        }
    }

    #[test]
    fn vax_ignores_the_declared_byte_order() {
        // VAX word order is intrinsic to the format, so a big-endian
        // header must not flip it.
        assert_eq!(
            vax_d(ByteOrder::LittleEndian).encode(100.0).unwrap(),
            vax_d(ByteOrder::BigEndian).encode(100.0).unwrap(),
        );
    }

    #[test]
    fn vax_d_and_g_disagree_on_the_same_bytes() {
        // The reason the two need separate variants: one byte string,
        // two different numbers.
        let bytes = [0xC8, 0x43, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let as_d = vax_d(ByteOrder::LittleEndian).decode(bytes);
        let as_g = vax_g(ByteOrder::LittleEndian).decode(bytes);
        assert_eq!(as_d.to_bits(), 100.0_f64.to_bits());
        assert_ne!(as_g.to_bits(), 100.0_f64.to_bits());
    }

    #[test]
    fn vax_normalizes_negative_zero() {
        // VAX has no -0.0 (that pattern is the reserved operand), so it
        // encodes as +0.0 — the one value that does not round-trip.
        let encoding = vax_d(ByteOrder::LittleEndian);
        let bytes = encoding.encode(-0.0).unwrap();
        assert_eq!(bytes, [0; 8]);
        assert_eq!(encoding.decode(bytes).to_bits(), 0.0_f64.to_bits());
    }

    #[test]
    fn vax_rejects_values_outside_its_range() {
        // Without the reserved-operand guard these would be written as
        // the library's error-encoded bit patterns.
        let encoding = vax_d(ByteOrder::LittleEndian);
        for value in [f64::MAX, -f64::MAX, f64::NAN, f64::INFINITY] {
            let err = encoding.encode(value).unwrap_err();
            assert!(
                matches!(
                    err,
                    SavError::FloatNotRepresentable {
                        format: FloatFormat::VaxDFloat,
                        ..
                    }
                ),
                "{value} gave {err:?}",
            );
        }
    }

    #[test]
    fn every_format_encodes_the_canonical_bias_differently() {
        // What makes `parse_bias`'s probe unambiguous: no two formats
        // encode 100.0 the same way, so at most one candidate matches.
        let encodings = [
            ieee(ByteOrder::LittleEndian),
            ieee(ByteOrder::BigEndian),
            ibm(ByteOrder::BigEndian),
            vax_d(ByteOrder::LittleEndian),
            vax_g(ByteOrder::LittleEndian),
        ];
        let mut seen: Vec<[u8; 8]> = Vec::new();
        for encoding in encodings {
            let bytes = encoding.encode(100.0).unwrap();
            assert!(
                !seen.contains(&bytes),
                "{:?} collides on {bytes:02X?}",
                encoding.format(),
            );
            seen.push(bytes);
        }
    }
}
