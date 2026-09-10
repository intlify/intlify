// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! One component interval. Caller preparation, clock setup, observation encoding,
//! validation, and output destruction are outside it. The output black box and
//! preserved invocation are inside and must be declared, never subtracted.

use std::hint::black_box;
use std::panic::{catch_unwind, AssertUnwindSafe};

use super::clock::{Clock, ClockFailure};
use super::quantity::Quantity;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(
    tag = "kind",
    content = "detail",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub(super) enum MeasurementFailure {
    Clock(ClockFailure),
    InvocationPanicked,
    PrerequisiteUnavailable,
}

pub(super) struct Measured<Output> {
    pub(super) duration: Quantity,
    pub(super) output: Output,
}

pub(super) fn measure<Input, Output>(
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
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;
    use crate::benchmark::clock::tests::ScriptedClock;

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
    fn failed_start_does_not_invoke_the_component() {
        let invoked = Cell::new(false);
        let clock = ScriptedClock::new([Err(ClockFailure::InvalidTimestamp)]);
        assert!(matches!(
            measure(&clock, &invoked, |called| called.set(true)),
            Err(MeasurementFailure::Clock(ClockFailure::InvalidTimestamp))
        ));
        assert!(!invoked.get());
    }

    #[test]
    fn failed_or_reversed_end_produces_no_successful_measurement() {
        for clock in [
            ScriptedClock::nanos([2, 1]),
            ScriptedClock::new([
                super::super::clock::Tick::new(0, 1),
                Err(ClockFailure::InvalidTimestamp),
            ]),
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
        assert_eq!(clock.remaining(), 1);
    }

    #[test]
    fn the_private_locale_boundary_runs_between_the_existing_measurement_markers() {
        use crate::input_limits::Bound;
        use crate::locale::fixtures::{fixture_binding, FixtureProvider};
        use crate::locale::{CanonicalizationFailure, Canonicalizer, ProviderFailure};

        // Provider construction and all expected answers are outside the interval.
        // This is an interval integration test, not a registered Measurement Case
        // or a substitute for the pending owner result / projection / report path.
        let core = Canonicalizer::bind(
            &fixture_binding(),
            Some(FixtureProvider::new()),
            Bound::new(128).unwrap(),
        )
        .unwrap();
        for input in ["EN-us", "en_US", "pt-BR"] {
            let clock = ScriptedClock::nanos([20, 31]);
            let measured = measure(&clock, (&core, input), |(core, input)| {
                core.canonicalize(input)
            })
            .unwrap();
            assert_eq!(measured.duration, Quantity::new(11));
            assert_eq!(clock.remaining(), 0);
            match input {
                "EN-us" => {
                    let output = measured.output.unwrap();
                    assert_eq!(output.locale().as_str(), "en-US");
                    assert_eq!(output.suggested_replacement(), Some("en-US"));
                }
                "en_US" => assert_eq!(
                    measured.output.unwrap_err(),
                    CanonicalizationFailure::Provider(ProviderFailure::InvalidIdentifier)
                ),
                "pt-BR" => assert_eq!(
                    measured.output.unwrap_err(),
                    CanonicalizationFailure::Provider(ProviderFailure::UnsupportedInput)
                ),
                _ => unreachable!(),
            }
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn native_clock_retains_the_locale_result_after_the_provider_is_released() {
        use crate::input_limits::Bound;
        use crate::locale::fixtures::{fixture_binding, FixtureProvider};
        use crate::locale::Canonicalizer;
        let clock = super::super::clock::MonotonicClock::acquire().unwrap();
        let core = Canonicalizer::bind(
            &fixture_binding(),
            Some(FixtureProvider::new()),
            Bound::new(128).unwrap(),
        )
        .unwrap();
        let measured = measure(&clock, (&core, "iw-IL"), |(core, input)| {
            core.canonicalize(input)
        })
        .unwrap();
        drop(core);
        let output = measured.output.unwrap();
        assert_eq!(output.locale().as_str(), "he-IL");
        assert_eq!(output.suggested_replacement(), Some("he-IL"));
        // No threshold or fake positive minimum is imposed on the observed time.
    }
}
