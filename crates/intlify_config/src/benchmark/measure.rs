// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! One component interval, narrowed to the visibility used here.
//!
//! What is inside and outside the interval is 026's method and belongs to
//! `intlify_measurement`.

pub(super) use intlify_measurement::acquisition::{measure, Measured, MeasurementFailure};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::clock::tests::ScriptedClock;
    use crate::benchmark::quantity::Quantity;

    #[test]
    fn the_private_locale_boundary_runs_between_the_existing_measurement_markers() {
        use crate::input_limits::Bound;
        use crate::locale::fixtures::{fixture_binding, FixtureProvider};
        use crate::locale::{CanonicalizationFailure, Canonicalizer, ProviderFailure};

        // Provider construction and all expected answers are outside the interval.
        // This is an interval integration test, not a registered Measurement Case
        // or a substitute for the owner result / projection / report path.
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
        use crate::benchmark::clock::MonotonicClock;
        use crate::input_limits::Bound;
        use crate::locale::fixtures::{fixture_binding, FixtureProvider};
        use crate::locale::Canonicalizer;

        let clock = MonotonicClock::acquire().unwrap();
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
