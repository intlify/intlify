// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 026 duration acquisition over 017's exact quantity domain.
//!
//! The `UInt64` and `PositiveUInt64` representations belong to
//! `intlify_shared_json`. Clock resolution and physical accuracy belong to the
//! method descriptor; this module only converts an acquired interval without
//! losing or inventing a value.

use std::time::{Duration, Instant};

// The enclosing module already carries `allow(dead_code)` because parts of the
// harness compile only under the non-default benchmark feature. Re-exported
// names need the matching allowance for the same reason.
#[allow(unused_imports)]
pub(super) use intlify_shared_json::quantity::{Quantity, Repetitions};

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
    // Windows may smooth a small reversed interval to zero during subtraction.
    // Check ordering first so an invalid interval cannot become a valid sample.
    if end < start {
        return Err(DurationFailure::ReversedClock);
    }
    let duration = end
        .checked_duration_since(start)
        .ok_or(DurationFailure::ReversedClock)?;
    canonical_nanoseconds(duration)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Include sub-tick intervals: Windows subtraction may otherwise accept
        // their reversal as zero even though Instant ordering is unambiguous.
        for nanoseconds in [1, 7, 1_000_000_000] {
            let end = start
                .checked_add(Duration::from_nanos(nanoseconds))
                .unwrap();
            assert!(end > start);
            assert_eq!(elapsed(start, end), Ok(Quantity::new(nanoseconds)));
            assert_eq!(elapsed(end, start), Err(DurationFailure::ReversedClock));
        }
        assert_eq!(elapsed(start, start), Ok(Quantity::new(0)));
    }
}
