// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 026 exact observations, independent of the smaller 015 `ResourceBoundValue`
//! domain. JSON uses shortest unsigned decimal strings until 017 owns the wire.
//! Clock resolution and physical accuracy belong to the method descriptor.

use std::fmt;
use std::num::NonZeroU64;
use std::time::{Duration, Instant};

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Quantity(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum QuantityError {
    InvalidDecimal,
    Overflow,
    ZeroRepetitions,
}

impl Quantity {
    pub(super) const fn new(value: u64) -> Self {
        Self(value)
    }

    pub(super) const fn get(self) -> u64 {
        self.0
    }

    pub(super) fn parse(value: &str) -> Result<Self, QuantityError> {
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

    pub(super) fn checked_add(self, other: Self) -> Result<Self, QuantityError> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Repetitions(NonZeroU64);

impl Repetitions {
    pub(super) fn new(value: u64) -> Result<Self, QuantityError> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(QuantityError::ZeroRepetitions)
    }
    pub(super) const fn get(self) -> u64 {
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

/// Observation failures must become unavailable failed attempts, never zero,
/// saturated, wrapped, or partially successful duration samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DurationFailure {
    ReversedClock,
    Overflow,
}

pub(super) fn canonical_nanoseconds(duration: Duration) -> Result<Quantity, DurationFailure> {
    u64::try_from(duration.as_nanos())
        .map(Quantity::new)
        .map_err(|_| DurationFailure::Overflow)
}

pub(super) fn elapsed(start: Instant, end: Instant) -> Result<Quantity, DurationFailure> {
    let duration = end
        .checked_duration_since(start)
        .ok_or(DurationFailure::ReversedClock)?;
    canonical_nanoseconds(duration)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn duration_conversion_checks_full_u128_before_narrowing() {
        assert_eq!(canonical_nanoseconds(Duration::ZERO), Ok(Quantity::new(0)));
        assert_eq!(
            canonical_nanoseconds(Duration::from_nanos(u64::MAX)),
            Ok(Quantity::new(u64::MAX))
        );
        let first_over = Duration::from_nanos(u64::MAX) + Duration::from_nanos(1);
        assert_eq!(
            canonical_nanoseconds(first_over),
            Err(DurationFailure::Overflow)
        );
        assert_eq!(
            canonical_nanoseconds(Duration::MAX),
            Err(DurationFailure::Overflow)
        );
    }

    #[test]
    fn reversed_clock_is_a_failure_but_zero_is_not_replaced_with_a_fake_minimum() {
        let start = Instant::now();
        let end = start.checked_add(Duration::from_nanos(7)).unwrap();
        assert_eq!(elapsed(start, end), Ok(Quantity::new(7)));
        assert_eq!(elapsed(end, start), Err(DurationFailure::ReversedClock));
        assert_eq!(elapsed(start, start), Ok(Quantity::new(0)));
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
