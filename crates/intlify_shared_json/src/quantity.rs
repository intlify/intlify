// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The 017 `UInt64` and `PositiveUInt64` domains.
//!
//! Every integer quantity, count, repetition, ordinal, and byte offset in the
//! shared JSON subset is the shortest ASCII decimal string for its value,
//! including small values. JSON numbers, signs, leading zeroes, exponent
//! notation, and fractional spellings are invalid.
//!
//! Formatting and parsing are reporting operations. Owners keep them outside
//! any measured component interval.

use std::fmt;
use std::num::NonZeroU64;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

/// Complete failure of an exact quantity operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantityError {
    /// The spelling is outside the shortest-decimal grammar.
    InvalidDecimal,
    /// The value is outside the `u64` domain, or an accumulation would wrap.
    Overflow,
    /// A positive count was required and zero was supplied.
    ZeroRepetitions,
}

/// The JSON Schema proves the entire `u64` domain, not merely decimal syntax.
/// Twenty-digit alternatives partition all values below the exact maximum.
fn decimal_schema(positive: bool) -> schemars::Schema {
    let maximum = u64::MAX.to_string();
    let mut branches = vec!["[1-9][0-9]{0,18}".to_owned()];
    if !positive {
        branches.insert(0, "0".into());
    }
    for (index, byte) in maximum.bytes().enumerate() {
        let digit = byte - b'0';
        let lower = u8::from(index == 0);
        if digit > lower {
            let range = if digit == lower + 1 {
                lower.to_string()
            } else {
                format!("[{lower}-{}]", digit - 1)
            };
            let remaining = maximum.len() - index - 1;
            let suffix = if remaining == 0 {
                String::new()
            } else {
                format!("[0-9]{{{remaining}}}")
            };
            branches.push(format!("{}{range}{suffix}", &maximum[..index]));
        }
    }
    branches.push(maximum);
    schemars::json_schema!({"type": "string", "pattern": format!("^(?:{})$", branches.join("|"))})
}

/// One exact `UInt64` value in `0..=18446744073709551615`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Quantity(u64);

impl schemars::JsonSchema for Quantity {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "UInt64".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        decimal_schema(false)
    }
}

impl Quantity {
    /// Retain one exact value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Return the exact value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Parse one shortest unsigned decimal string.
    pub fn parse(value: &str) -> Result<Self, QuantityError> {
        let bytes = value.as_bytes();
        if bytes.is_empty()
            || bytes.len() > 20
            || !bytes.iter().all(u8::is_ascii_digit)
            || (bytes[0] == b'0' && bytes.len() != 1)
        {
            return Err(QuantityError::InvalidDecimal);
        }
        value.parse().map(Self).map_err(|_| QuantityError::Overflow)
    }

    /// Add two exact quantities, reporting overflow instead of wrapping.
    pub fn checked_add(self, other: Self) -> Result<Self, QuantityError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(QuantityError::Overflow)
    }
}

impl Serialize for Quantity {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Formatting is a reporting operation, never inside a component interval.
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Quantity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct DecimalVisitor;
        impl de::Visitor<'_> for DecimalVisitor {
            type Value = Quantity;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a shortest unsigned decimal string in the u64 domain")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Quantity::parse(value).map_err(|_| E::custom("invalid exact quantity"))
            }
        }
        deserializer.deserialize_str(DecimalVisitor)
    }
}

/// One exact `PositiveUInt64` value; zero is outside the domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Repetitions(NonZeroU64);

impl schemars::JsonSchema for Repetitions {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "PositiveUInt64".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        decimal_schema(true)
    }
}

impl Repetitions {
    /// Retain one positive count.
    pub fn new(value: u64) -> Result<Self, QuantityError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(QuantityError::ZeroRepetitions)
    }

    /// Return the exact positive value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl Serialize for Repetitions {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Quantity::new(self.get()).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Repetitions {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let quantity = Quantity::deserialize(deserializer)?;
        Self::new(quantity.get()).map_err(|_| de::Error::custom("repetitions must be positive"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_decimal_schemas_match_the_full_rust_domains() {
        for positive in [false, true] {
            let schema = serde_json::to_value(decimal_schema(positive)).unwrap();
            let validator = jsonschema::draft7::new(&schema).unwrap();
            let mut candidates = vec![
                String::new(),
                "00".into(),
                "01".into(),
                "-1".into(),
                "+1".into(),
                "1.0".into(),
                "1e0".into(),
                " 1".into(),
                "１".into(),
            ];
            for length in 1..=22 {
                for digit in ['0', '1', '8', '9'] {
                    candidates.push(std::iter::repeat_n(digit, length).collect());
                }
            }
            for offset in 0..=32_u128 {
                candidates.push((u128::from(u64::MAX) - offset).to_string());
                candidates.push((u128::from(u64::MAX) + offset).to_string());
            }
            let maximum = u64::MAX.to_string();
            for index in 0..maximum.len() {
                for digit in b'0'..=b'9' {
                    let mut digits = maximum.as_bytes().to_vec();
                    digits[index] = digit;
                    candidates.push(String::from_utf8(digits).unwrap());
                }
            }
            for text in candidates {
                let expected =
                    Quantity::parse(&text).is_ok_and(|quantity| !positive || quantity.get() > 0);
                assert_eq!(
                    validator.is_valid(&serde_json::json!(text)),
                    expected,
                    "{positive}, {text}"
                );
            }
            for non_string in [
                serde_json::json!(0),
                serde_json::json!(u64::MAX),
                serde_json::json!(null),
                serde_json::json!(true),
            ] {
                assert!(!validator.is_valid(&non_string));
            }
        }
    }

    #[test]
    fn exact_domain_includes_zero_and_u64_max_beyond_javascript_safe_integers() {
        for value in [0, 1, 9_007_199_254_740_991, 9_007_199_254_740_992, u64::MAX] {
            let quantity = Quantity::new(value);
            let json = serde_json::to_string(&quantity).unwrap();
            assert_eq!(json, format!("\"{value}\""));
            assert_eq!(serde_json::from_str::<Quantity>(&json).unwrap(), quantity);
            assert_eq!(Quantity::parse(&value.to_string()).unwrap().get(), value);
        }
    }

    #[test]
    fn noncanonical_negative_fractional_and_out_of_domain_strings_are_rejected() {
        for value in [
            "", "00", "01", "+1", "-0", "-1", "1.0", "1e3", " 1", "1\n", "１", "0x10",
        ] {
            assert_eq!(Quantity::parse(value), Err(QuantityError::InvalidDecimal));
        }
        assert_eq!(
            Quantity::parse("18446744073709551616"),
            Err(QuantityError::Overflow)
        );
        assert!(Quantity::parse("100000000000000000000").is_err());
    }

    #[test]
    fn json_numbers_and_other_types_cannot_bypass_lossless_string_encoding() {
        for value in [
            "0",
            "1",
            "9007199254740992",
            "1.0",
            "null",
            "false",
            "[]",
            "{}",
        ] {
            assert!(serde_json::from_str::<Quantity>(value).is_err());
            assert!(serde_json::from_str::<Repetitions>(value).is_err());
        }
    }

    #[test]
    fn repetition_counts_are_positive_and_lossless() {
        assert_eq!(Repetitions::new(0), Err(QuantityError::ZeroRepetitions));
        assert!(serde_json::from_str::<Repetitions>("\"0\"").is_err());
        for value in [1, 9_007_199_254_740_992, u64::MAX] {
            let count = Repetitions::new(value).unwrap();
            let json = serde_json::to_string(&count).unwrap();
            assert_eq!(serde_json::from_str::<Repetitions>(&json).unwrap(), count);
            assert_eq!(count.get(), value);
        }
    }

    #[test]
    fn sample_accumulation_never_wraps_or_saturates() {
        assert_eq!(
            Quantity::new(u64::MAX - 1).checked_add(Quantity::new(1)),
            Ok(Quantity::new(u64::MAX))
        );
        assert_eq!(
            Quantity::new(u64::MAX).checked_add(Quantity::new(1)),
            Err(QuantityError::Overflow)
        );
    }

    #[test]
    fn malformed_strings_do_not_enter_decoder_error_text() {
        let error = serde_json::from_str::<Quantity>("\"secret-not-a-quantity\"").unwrap_err();
        assert!(!error.to_string().contains("secret-not-a-quantity"));
    }
}
