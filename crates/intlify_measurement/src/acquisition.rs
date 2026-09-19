// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Acquiring one duration, the way 026 defines the method.
//!
//! Timestamps are subtracted in the full raw domain before the interval is
//! narrowed to nanoseconds, so a large absolute clock value cannot overflow a
//! small interval. The reported resolution comes from the operating system; it
//! is never inferred from a storage unit or from observed latency.
//!
//! One interval is one start read, one invocation, one end read. Caller
//! preparation, clock setup, observation encoding, validation, and output
//! destruction are outside it. The output black box and the preserved
//! invocation are inside it and are declared rather than subtracted.
//!
//! This module is compiled only under the non-default `acquisition` feature:
//! reading a clock is not part of reading a record.

use std::hint::black_box;
use std::panic::{catch_unwind, AssertUnwindSafe};

use intlify_shared_json::quantity::Quantity;

use crate::environment::ClockObservation;

/// Complete failure to read a clock or convert an interval.
///
/// None of these becomes a zero, saturated, or wrapped duration: an interval
/// that could not be measured is absent, not fast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClockFailure {
    UnsupportedPlatform,
    InvalidTimestamp,
    InvalidResolution,
    ReversedClock,
    DurationConversionOverflow,
}

/// One validated timestamp from a monotonic clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tick {
    seconds: u64,
    nanoseconds: u32,
}

impl Tick {
    /// Validate one raw timestamp before it can take part in a subtraction.
    pub fn new(seconds: i64, nanoseconds: i64) -> Result<Self, ClockFailure> {
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

    /// Subtract in the raw domain, then narrow the interval exactly.
    pub fn elapsed_until(self, end: Self) -> Result<Quantity, ClockFailure> {
        let difference = end
            .raw_nanoseconds()
            .checked_sub(self.raw_nanoseconds())
            .ok_or(ClockFailure::ReversedClock)?;
        u64::try_from(difference)
            .map(Quantity::new)
            .map_err(|_| ClockFailure::DurationConversionOverflow)
    }
}

/// What a clock provider reports about itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockDescription {
    pub provider: &'static str,
    pub provider_revision: &'static str,
    pub clock: &'static str,
    pub resolution_nanoseconds: Quantity,
    pub resolution_source: &'static str,
    pub conversion: &'static str,
}

impl ClockDescription {
    /// Present this description as the environment field it fills.
    #[must_use]
    pub fn observation(self) -> ClockObservation {
        ClockObservation {
            provider: self.provider.into(),
            provider_revision: self.provider_revision.into(),
            clock: self.clock.into(),
            resolution_nanoseconds: self.resolution_nanoseconds,
            resolution_source: self.resolution_source.into(),
            conversion: self.conversion.into(),
        }
    }
}

/// A clock a measured interval can be read from.
pub trait Clock {
    /// Read one timestamp.
    fn read(&self) -> Result<Tick, ClockFailure>;
}

/// The POSIX monotonic clock, acquired once per run.
pub struct MonotonicClock {
    description: ClockDescription,
}

impl MonotonicClock {
    /// Acquire the clock and its reported resolution.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn acquire() -> Result<Self, ClockFailure> {
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

    /// Acquire the clock and its reported resolution.
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub fn acquire() -> Result<Self, ClockFailure> {
        Err(ClockFailure::UnsupportedPlatform)
    }

    /// Borrow what this provider reports about itself.
    #[must_use]
    pub const fn description(&self) -> ClockDescription {
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

/// Complete failure of one measured interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(
    tag = "kind",
    content = "detail",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum MeasurementFailure {
    Clock(ClockFailure),
    InvocationPanicked,
    PrerequisiteUnavailable,
}

/// One measured interval and the output it produced.
pub struct Measured<Output> {
    pub duration: Quantity,
    pub output: Output,
}

/// Measure exactly one invocation.
///
/// The output is retained past the end read, so its construction is inside the
/// interval and its destruction is outside. A panic is contained here and
/// becomes a failed measurement rather than an unwinding harness.
pub fn measure<Input, Output>(
    clock: &impl Clock,
    input: Input,
    invoke: fn(Input) -> Output,
) -> Result<Measured<Output>, MeasurementFailure> {
    // Only finite owner-controlled operations call this helper. No live host
    // values, arbitrary callbacks, or panic payloads enter retained observations.
    catch_unwind(AssertUnwindSafe(|| {
        let input = black_box(input);
        let invoke = black_box(invoke);
        let start = clock.read().map_err(MeasurementFailure::Clock)?;
        let output = black_box(invoke(input));
        let end = clock.read().map_err(MeasurementFailure::Clock)?;
        let duration = start
            .elapsed_until(end)
            .map_err(MeasurementFailure::Clock)?;
        Ok(Measured { duration, output })
    }))
    .map_err(|_| MeasurementFailure::InvocationPanicked)?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::rc::Rc;

    struct ScriptedClock(RefCell<VecDeque<Result<Tick, ClockFailure>>>);

    impl ScriptedClock {
        fn new(values: impl IntoIterator<Item = Result<Tick, ClockFailure>>) -> Self {
            Self(RefCell::new(values.into_iter().collect()))
        }
        fn nanos(values: impl IntoIterator<Item = i64>) -> Self {
            Self::new(values.into_iter().map(|nanos| Tick::new(0, nanos)))
        }
        fn remaining(&self) -> usize {
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
    fn measures_exactly_one_invocation_and_retains_output_past_the_end_marker() {
        struct Output(Rc<Cell<u8>>);
        impl Drop for Output {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let dropped = Rc::new(Cell::new(0));
        let clock = ScriptedClock::nanos([20, 27]);
        let result = measure(&clock, &dropped, |counter| Output(Rc::clone(counter))).unwrap();
        assert_eq!(result.duration, Quantity::new(7));
        assert_eq!(clock.remaining(), 0);
        assert_eq!(dropped.get(), 0);
        drop(result);
        assert_eq!(dropped.get(), 1);
    }

    #[test]
    fn a_failed_start_read_does_not_invoke_the_component() {
        let invoked = Cell::new(false);
        let clock = ScriptedClock::new([Err(ClockFailure::InvalidTimestamp)]);
        assert!(matches!(
            measure(&clock, &invoked, |called| called.set(true)),
            Err(MeasurementFailure::Clock(ClockFailure::InvalidTimestamp))
        ));
        assert!(!invoked.get());
    }

    #[test]
    fn a_failed_or_reversed_end_read_produces_no_successful_measurement() {
        // The invocation has already happened in both cases. What must not
        // happen is that it becomes a sample anyway.
        for clock in [
            ScriptedClock::nanos([2, 1]),
            ScriptedClock::new([Tick::new(0, 1), Err(ClockFailure::InvalidTimestamp)]),
        ] {
            let invoked = Cell::new(0);
            assert!(measure(&clock, &invoked, |count| count.set(count.get() + 1)).is_err());
            assert_eq!(invoked.get(), 1);
        }
    }

    #[test]
    fn a_panicking_invocation_is_not_a_zero_sample_or_a_partial_output() {
        let clock = ScriptedClock::nanos([1, 2]);
        assert!(matches!(
            measure(&clock, &(), |()| panic!("test-owned failure")),
            Err(MeasurementFailure::InvocationPanicked)
        ));
        // The end read never happened, so no interval was closed over a panic.
        assert_eq!(clock.remaining(), 1);
    }

    #[test]
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn the_provider_reports_its_resolution_without_inferring_it_from_latency() {
        let clock = MonotonicClock::acquire().unwrap();
        let descriptor = clock.description();
        assert_eq!(descriptor.clock, "posix-clock-monotonic");
        assert!(descriptor.resolution_nanoseconds.get() > 0);
        let start = clock.read().unwrap();
        let end = clock.read().unwrap();
        assert!(start.elapsed_until(end).is_ok());
    }
}
