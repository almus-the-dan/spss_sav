//! Turning wire-level missing-value bytes into a typed specification.

use crate::spss::sav::extensions::float_sentinels::FloatSentinels;
use crate::spss::sav::float_encoding::FloatEncoding;
use crate::spss::sav::missing_value_specification::MissingValueSpecification;
use crate::spss::sav::range_bound::RangeBound;
use crate::spss::sav::raw_missing_values::RawMissingValues;
use crate::spss::sav::variable_type::VariableType;

/// Decodes the raw bytes a type-2 record carried into the typed
/// specification a [`SavVariable`](crate::spss::sav::sav_variable::SavVariable)
/// exposes.
///
/// The interpretation depends on the variable's type, which is why this
/// cannot happen at streaming time: the same eight bytes are an `f64`
/// for a numeric variable and a padded string key for a string one.
///
/// For numeric variables the range endpoints are matched against the
/// file's `lowest` and `highest` sentinels and become
/// [`RangeBound::Unbounded`] when they hit. An SPSS range written as
/// `LO THRU 0` stores the lowest representable value as its low end,
/// and reporting that number back as if the user had typed it would be
/// misleading.
#[must_use]
pub(crate) fn decode(
    raw: &RawMissingValues,
    variable_type: VariableType,
    encoding: FloatEncoding,
    sentinels: &FloatSentinels,
) -> MissingValueSpecification {
    match variable_type {
        VariableType::Numeric => decode_numeric(raw, encoding, sentinels),
        VariableType::String(_) => decode_string(raw),
    }
}

fn decode_numeric(
    raw: &RawMissingValues,
    encoding: FloatEncoding,
    sentinels: &FloatSentinels,
) -> MissingValueSpecification {
    match raw {
        RawMissingValues::None => MissingValueSpecification::None,
        RawMissingValues::Discrete(entries) => {
            let values = entries
                .iter()
                .map(|bytes| encoding.decode(*bytes))
                .collect();
            MissingValueSpecification::Discrete(values)
        }
        RawMissingValues::Range { low, high } => MissingValueSpecification::Range {
            low: low_bound(*low, encoding, sentinels),
            high: high_bound(*high, encoding, sentinels),
            extra: None,
        },
        RawMissingValues::RangeWithDiscrete {
            low,
            high,
            discrete,
        } => MissingValueSpecification::Range {
            low: low_bound(*low, encoding, sentinels),
            high: high_bound(*high, encoding, sentinels),
            extra: Some(encoding.decode(*discrete)),
        },
    }
}

/// String missing values are raw byte keys, compared byte-for-byte
/// against the cell's leading bytes.
///
/// SPSS only ever writes discrete values for a string variable, but the
/// wire format has no way to say so, and a file could carry a range
/// anyway. Rather than drop those bytes, the endpoints are kept as
/// discrete keys — the read stays lossless and nothing has to fail.
fn decode_string(raw: &RawMissingValues) -> MissingValueSpecification {
    let values: Vec<Box<[u8]>> = match raw {
        RawMissingValues::None => return MissingValueSpecification::None,
        RawMissingValues::Discrete(entries) => entries.iter().copied().map(boxed).collect(),
        RawMissingValues::Range { low, high } => vec![boxed(*low), boxed(*high)],
        RawMissingValues::RangeWithDiscrete {
            low,
            high,
            discrete,
        } => vec![boxed(*low), boxed(*high), boxed(*discrete)],
    };
    MissingValueSpecification::String(values)
}

fn boxed(bytes: [u8; 8]) -> Box<[u8]> {
    bytes.to_vec().into_boxed_slice()
}

fn low_bound(bytes: [u8; 8], encoding: FloatEncoding, sentinels: &FloatSentinels) -> RangeBound {
    if bytes == sentinels.lowest() {
        return RangeBound::Unbounded;
    }
    RangeBound::Inclusive(encoding.decode(bytes))
}

fn high_bound(bytes: [u8; 8], encoding: FloatEncoding, sentinels: &FloatSentinels) -> RangeBound {
    if bytes == sentinels.highest() {
        return RangeBound::Unbounded;
    }
    RangeBound::Inclusive(encoding.decode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spss::sav::byte_order::ByteOrder;
    use crate::spss::sav::float_format::FloatFormat;

    fn encoding() -> FloatEncoding {
        FloatEncoding::new(FloatFormat::Ieee754, ByteOrder::LittleEndian)
    }

    fn sentinels() -> FloatSentinels {
        FloatSentinels::spss_defaults(encoding())
    }

    fn numeric(raw: &RawMissingValues) -> MissingValueSpecification {
        decode(raw, VariableType::Numeric, encoding(), &sentinels())
    }

    #[test]
    fn none_decodes_to_none() {
        assert_eq!(
            numeric(&RawMissingValues::None),
            MissingValueSpecification::None,
        );
    }

    #[test]
    fn discrete_numeric_values_decode_as_f64() {
        let raw = RawMissingValues::Discrete(vec![
            9.0_f64.to_le_bytes(),
            99.0_f64.to_le_bytes(),
            999.0_f64.to_le_bytes(),
        ]);
        assert_eq!(
            numeric(&raw),
            MissingValueSpecification::Discrete(vec![9.0, 99.0, 999.0]),
        );
    }

    #[test]
    fn a_plain_range_keeps_both_endpoints() {
        let raw = RawMissingValues::Range {
            low: (-1.0_f64).to_le_bytes(),
            high: 1.0_f64.to_le_bytes(),
        };
        assert_eq!(
            numeric(&raw),
            MissingValueSpecification::Range {
                low: RangeBound::Inclusive(-1.0),
                high: RangeBound::Inclusive(1.0),
                extra: None,
            },
        );
    }

    /// `LO THRU 0` stores the lowest sentinel as its low endpoint; that
    /// must come back as unbounded, not as the sentinel's numeric value.
    #[test]
    fn the_lowest_sentinel_becomes_an_unbounded_low_end() {
        let raw = RawMissingValues::Range {
            low: sentinels().lowest(),
            high: 0.0_f64.to_le_bytes(),
        };
        assert_eq!(
            numeric(&raw),
            MissingValueSpecification::Range {
                low: RangeBound::Unbounded,
                high: RangeBound::Inclusive(0.0),
                extra: None,
            },
        );
    }

    #[test]
    fn the_highest_sentinel_becomes_an_unbounded_high_end() {
        let raw = RawMissingValues::Range {
            low: 0.0_f64.to_le_bytes(),
            high: sentinels().highest(),
        };
        assert_eq!(
            numeric(&raw),
            MissingValueSpecification::Range {
                low: RangeBound::Inclusive(0.0),
                high: RangeBound::Unbounded,
                extra: None,
            },
        );
    }

    #[test]
    fn a_range_with_a_discrete_value_keeps_the_extra() {
        let raw = RawMissingValues::RangeWithDiscrete {
            low: (-99.0_f64).to_le_bytes(),
            high: (-1.0_f64).to_le_bytes(),
            discrete: 9999.0_f64.to_le_bytes(),
        };
        assert_eq!(
            numeric(&raw),
            MissingValueSpecification::Range {
                low: RangeBound::Inclusive(-99.0),
                high: RangeBound::Inclusive(-1.0),
                extra: Some(9999.0),
            },
        );
    }

    #[test]
    fn string_variables_keep_their_bytes() {
        let raw = RawMissingValues::Discrete(vec![*b"alpha   "]);
        let spec = decode(&raw, VariableType::String(8), encoding(), &sentinels());
        assert_eq!(
            spec,
            MissingValueSpecification::String(vec![b"alpha   ".to_vec().into_boxed_slice()]),
        );
    }

    /// The same eight bytes mean different things depending on the
    /// variable's type — which is exactly why this decode waits until
    /// the type is known.
    #[test]
    fn the_same_bytes_decode_differently_per_type() {
        let raw = RawMissingValues::Discrete(vec![*b"alpha   "]);
        let as_string = decode(&raw, VariableType::String(8), encoding(), &sentinels());
        let as_numeric = decode(&raw, VariableType::Numeric, encoding(), &sentinels());
        assert!(matches!(as_string, MissingValueSpecification::String(_)));
        assert!(matches!(as_numeric, MissingValueSpecification::Discrete(_)));
    }

    /// A range on a string variable is not something SPSS writes, but
    /// the bytes are kept rather than dropped.
    #[test]
    fn a_range_on_a_string_variable_degrades_to_discrete_keys() {
        let raw = RawMissingValues::Range {
            low: *b"aaaaaaaa",
            high: *b"zzzzzzzz",
        };
        let spec = decode(&raw, VariableType::String(8), encoding(), &sentinels());
        assert_eq!(
            spec,
            MissingValueSpecification::String(vec![
                b"aaaaaaaa".to_vec().into_boxed_slice(),
                b"zzzzzzzz".to_vec().into_boxed_slice(),
            ]),
        );
    }
}
