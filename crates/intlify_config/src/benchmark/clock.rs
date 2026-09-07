// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Measurement-only clock provider. Subtract in the full raw timestamp domain
//! before converting the interval to 026's u64 nanoseconds. OS-reported clock
//! resolution is not inferred from `Duration`'s storage unit or observed latency.

use super::quantity::Quantity;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ClockFailure {
    UnsupportedPlatform,
    InvalidTimestamp,
    InvalidResolution,
    ReversedClock,
    DurationConversionOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Tick {
    seconds: u64,
    nanoseconds: u32,
}

impl Tick {
    pub(super) fn new(seconds: i64, nanoseconds: i64) -> Result<Self, ClockFailure> {
        let seconds = u64::try_from(seconds).map_err(|_| ClockFailure::InvalidTimestamp)?;
        let nanoseconds = u32::try_from(nanoseconds)
            .ok()
            .filter(|&nanos| nanos < 1_000_000_000)
            .ok_or(ClockFailure::InvalidTimestamp)?;
        Ok(Self {
            seconds,
            nanoseconds,
        })
    }

    fn raw_nanoseconds(self) -> u128 {
        // Every u64 second value and subsecond value fits this wider domain.
        u128::from(self.seconds) * 1_000_000_000 + u128::from(self.nanoseconds)
    }

    pub(super) fn elapsed_until(self, end: Self) -> Result<Quantity, ClockFailure> {
        let difference = end
            .raw_nanoseconds()
            .checked_sub(self.raw_nanoseconds())
            .ok_or(ClockFailure::ReversedClock)?;
        u64::try_from(difference)
            .map(Quantity::new)
            .map_err(|_| ClockFailure::DurationConversionOverflow)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ClockDescription {
    pub(super) provider: &'static str,
    pub(super) provider_revision: &'static str,
    pub(super) clock: &'static str,
    pub(super) resolution_nanoseconds: Quantity,
    pub(super) resolution_source: &'static str,
    pub(super) conversion: &'static str,
}

pub(super) trait Clock {
    fn read(&self) -> Result<Tick, ClockFailure>;
}

pub(super) struct MonotonicClock {
    description: ClockDescription,
}

impl MonotonicClock {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(super) fn acquire() -> Result<Self, ClockFailure> {
        use rustix::time::{clock_getres, ClockId};
        let resolution = clock_getres(ClockId::Monotonic);
        let resolution = Tick::new(resolution.tv_sec, resolution.tv_nsec)
            .map_err(|_| ClockFailure::InvalidResolution)?;
        let resolution = u64::try_from(resolution.raw_nanoseconds())
            .ok()
            .filter(|&value| value > 0)
            .ok_or(ClockFailure::InvalidResolution)?;
        Ok(Self {
            description: ClockDescription {
                provider: "rustix",
                provider_revision: "1.1.4",
                clock: "posix-clock-monotonic",
                resolution_nanoseconds: Quantity::new(resolution),
                resolution_source: "clock-getres-reported-granularity",
                conversion: "exact-integer-nanoseconds",
            },
        })
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub(super) fn acquire() -> Result<Self, ClockFailure> {
        Err(ClockFailure::UnsupportedPlatform)
    }

    pub(super) const fn description(&self) -> ClockDescription {
        self.description
    }
}

impl Clock for MonotonicClock {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn read(&self) -> Result<Tick, ClockFailure> {
        use rustix::time::{clock_gettime, ClockId};
        let value = clock_gettime(ClockId::Monotonic);
        Tick::new(value.tv_sec, value.tv_nsec)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    fn read(&self) -> Result<Tick, ClockFailure> {
        Err(ClockFailure::UnsupportedPlatform)
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    pub(crate) struct ScriptedClock(RefCell<VecDeque<Result<Tick, ClockFailure>>>);

    impl ScriptedClock {
        pub(crate) fn new(values: impl IntoIterator<Item = Result<Tick, ClockFailure>>) -> Self {
            Self(RefCell::new(values.into_iter().collect()))
        }
        pub(crate) fn nanos(values: impl IntoIterator<Item = i64>) -> Self {
            Self::new(values.into_iter().map(|nanos| Tick::new(0, nanos)))
        }
        pub(crate) fn remaining(&self) -> usize {
            self.0.borrow().len()
        }
    }
    impl Clock for ScriptedClock {
        fn read(&self) -> Result<Tick, ClockFailure> {
            self.0
                .borrow_mut()
                .pop_front()
                .expect("unexpected clock read")
        }
    }

    #[test]
    fn timestamp_domain_is_validated_before_subtraction() {
        for (seconds, nanos) in [(-1, 0), (0, -1), (0, 1_000_000_000)] {
            assert_eq!(
                Tick::new(seconds, nanos),
                Err(ClockFailure::InvalidTimestamp)
            );
        }
        let first = Tick::new(10, 999_999_999).unwrap();
        let next = Tick::new(11, 0).unwrap();
        assert_eq!(first.elapsed_until(next), Ok(Quantity::new(1)));
        assert_eq!(next.elapsed_until(first), Err(ClockFailure::ReversedClock));
        assert_eq!(first.elapsed_until(first), Ok(Quantity::new(0)));
    }

    #[test]
    fn large_absolute_timestamps_do_not_overflow_small_intervals() {
        let start = Tick::new(i64::MAX, 10).unwrap();
        let end = Tick::new(i64::MAX, 11).unwrap();
        assert_eq!(start.elapsed_until(end), Ok(Quantity::new(1)));
    }

    #[test]
    fn duration_accepts_u64_max_but_rejects_the_first_over_witness() {
        let start = Tick::new(0, 0).unwrap();
        let seconds = i64::try_from(u64::MAX / 1_000_000_000).unwrap();
        let nanos = i64::try_from(u64::MAX % 1_000_000_000).unwrap();
        assert_eq!(
            start.elapsed_until(Tick::new(seconds, nanos).unwrap()),
            Ok(Quantity::new(u64::MAX))
        );
        assert_eq!(
            start.elapsed_until(Tick::new(seconds, nanos + 1).unwrap()),
            Err(ClockFailure::DurationConversionOverflow)
        );
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn provider_reports_resolution_and_monotonic_values_without_latency_thresholds() {
        let clock = MonotonicClock::acquire().unwrap();
        let descriptor = clock.description();
        assert_eq!(descriptor.clock, "posix-clock-monotonic");
        assert!(descriptor.resolution_nanoseconds.get() > 0);
        let start = clock.read().unwrap();
        let end = clock.read().unwrap();
        assert!(start.elapsed_until(end).is_ok());
    }
}
