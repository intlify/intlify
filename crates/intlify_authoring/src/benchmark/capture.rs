// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Capturing the samples of one case.
//!
//! The expectation is established before any interval opens, by invoking the
//! operation once outside measurement. Every measured invocation is then
//! compared against it, so a run that produced a different result reports a
//! semantic mismatch instead of a fast sample.
//!
//! Warmup invocations are executed and counted, never sampled. An arithmetic
//! overflow, a clock failure, or a panic fails the case; none of them becomes
//! a zero or a saturated duration.

use intlify_measurement::acquisition::{measure, Clock, MeasurementFailure};
use intlify_shared_json::quantity::{Quantity, Repetitions};
use serde::{Deserialize, Serialize};

use super::cases::{PreparationFailure, Prepared};
use super::observation::{Digest, Frame, Observation};
use super::operation::{self, LogicalWork, Observed, Operation};

/// The sampling this profile performs, fixed before capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Sampling {
    pub(super) warmup: Quantity,
    pub(super) samples: Repetitions,
    pub(super) repetitions: Repetitions,
}

/// The binding one case's samples are local to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Binding {
    pub(super) run: Digest,
    pub(super) case: Digest,
}

impl Binding {
    fn local(self, domain: &str, ordinal: u64) -> Digest {
        let mut frame = Frame::new(domain);
        frame.digest(self.run);
        frame.digest(self.case);
        frame.uint(ordinal);
        frame.finish()
    }
}

/// One captured sample of one case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CapturedSample {
    pub(super) local_identity: Digest,
    pub(super) ordinal: Quantity,
    pub(super) repetition_count: Repetitions,
    pub(super) aggregate_nanoseconds: Quantity,
    pub(super) semantic_observation: Observation,
    pub(super) execution_identity: Digest,
}

/// The complete capture of one case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Capture {
    pub(super) warmup_completed: Quantity,
    pub(super) samples: Vec<CapturedSample>,
    pub(super) work: LogicalWork,
}

/// Why a case produced no complete capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "detail",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub(super) enum CaptureFailure {
    /// The fixture could not be prepared.
    Preparation(PreparationFailure),
    /// A clock read or an invocation failed.
    Measurement(MeasurementFailure),
    /// A repetition sum exceeded the exact quantity domain.
    Overflow,
    /// A measured invocation produced a different result than the expectation.
    SemanticMismatch,
}

fn capture_with<I: Copy, O>(
    clock: &impl Clock,
    sampling: Sampling,
    binding: Binding,
    expected: &Observed,
    input: I,
    invoke: fn(I) -> O,
    observe: fn(&O, I) -> Observed,
) -> Result<Capture, CaptureFailure> {
    let mut warmup_completed = 0_u64;
    for _ in 0..sampling.warmup.get() {
        let measured = measure(clock, input, invoke).map_err(CaptureFailure::Measurement)?;
        drop(measured);
        warmup_completed += 1;
    }

    let mut samples = Vec::new();
    for ordinal in 0..sampling.samples.get() {
        let mut aggregate = 0_u64;
        for _ in 0..sampling.repetitions.get() {
            let measured = measure(clock, input, invoke).map_err(CaptureFailure::Measurement)?;
            aggregate = aggregate
                .checked_add(measured.duration.get())
                .ok_or(CaptureFailure::Overflow)?;
            // Observation and comparison happen after the end read.
            let actual = observe(&measured.output, input);
            if actual.observation != expected.observation || actual.work != expected.work {
                return Err(CaptureFailure::SemanticMismatch);
            }
        }
        samples.push(CapturedSample {
            local_identity: binding.local("sample-local", ordinal),
            ordinal: Quantity::new(ordinal),
            repetition_count: sampling.repetitions,
            aggregate_nanoseconds: Quantity::new(aggregate),
            semantic_observation: expected.observation,
            execution_identity: binding.local("sample-execution", ordinal),
        });
    }
    Ok(Capture {
        warmup_completed: Quantity::new(warmup_completed),
        samples,
        work: expected.work.clone(),
    })
}

/// Capture one prepared case under the fixed sampling policy.
pub(super) fn capture(
    clock: &impl Clock,
    prepared: &Prepared,
    sampling: Sampling,
    binding: Binding,
) -> Result<Capture, CaptureFailure> {
    match prepared.fixture.operation {
        Operation::LiteralEncode => {
            let super::cases::Input::Literal(text) = prepared.fixture.input else {
                return Err(CaptureFailure::Preparation(PreparationFailure::Fixture));
            };
            capture_with(
                clock,
                sampling,
                binding,
                prepared.expected(),
                (text, &prepared.limits, &prepared.segments),
                operation::invoke_encode,
                operation::observe_encode,
            )
        }
        Operation::Mf2ParseAndSemanticFacts => {
            let super::cases::Input::Mf2(source) = prepared.fixture.input else {
                return Err(CaptureFailure::Preparation(PreparationFailure::Fixture));
            };
            capture_with(
                clock,
                sampling,
                binding,
                prepared.expected(),
                (
                    source,
                    &prepared.occurrence,
                    &prepared.limits,
                    &prepared.workspace,
                ),
                operation::invoke_mf2,
                operation::observe_mf2,
            )
        }
        Operation::SourceLocaleAndSurfaceClass => {
            let declaration = prepared.declaration();
            capture_with(
                clock,
                sampling,
                binding,
                prepared.expected(),
                (&prepared.context, &declaration, &prepared.limits),
                operation::invoke_context,
                operation::observe_context,
            )
        }
        Operation::DeclarationFacts => {
            let (Some(analysis), Some(facts)) = (&prepared.analysis, &prepared.context_facts)
            else {
                return Err(CaptureFailure::Preparation(PreparationFailure::PriorStage));
            };
            let declaration = prepared.declaration();
            capture_with(
                clock,
                sampling,
                binding,
                prepared.expected(),
                (&declaration, analysis, facts, &prepared.limits),
                operation::invoke_facts,
                operation::observe_facts,
            )
        }
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use super::capture as capture_case;
    use super::*;
    use crate::benchmark::cases::{prepare, FIXTURES};
    use intlify_measurement::acquisition::{MonotonicClock, Tick};

    fn binding() -> Binding {
        let mut frame = Frame::new("test-binding");
        frame.text("run");
        let run = frame.finish();
        let mut frame = Frame::new("test-binding");
        frame.text("case");
        Binding {
            run,
            case: frame.finish(),
        }
    }

    fn sampling() -> Sampling {
        Sampling {
            warmup: Quantity::new(2),
            samples: Repetitions::new(1).unwrap(),
            repetitions: Repetitions::new(2).unwrap(),
        }
    }

    #[test]
    fn every_fixture_captures_and_repeats_the_same_observation() {
        let clock = MonotonicClock::acquire().unwrap();
        for fixture in FIXTURES {
            let prepared = prepare(fixture).unwrap();
            let capture = capture_case(&clock, &prepared, sampling(), binding()).unwrap();
            assert_eq!(capture.warmup_completed, Quantity::new(2));
            assert_eq!(capture.samples.len(), 1);
            assert_eq!(
                capture.samples[0].repetition_count,
                Repetitions::new(2).unwrap()
            );
            // Two runs of the same fixture agree on the result, which is what
            // makes a later disagreement meaningful.
            let again = capture_case(&clock, &prepared, sampling(), binding()).unwrap();
            assert_eq!(
                again.samples[0].semantic_observation, capture.samples[0].semantic_observation,
                "{} disagreed with itself",
                fixture.name
            );
            assert_eq!(again.work, capture.work);
        }
    }

    /// A clock whose readings the test writes in advance.
    ///
    /// The real provider cannot be made to report an interval near the top of
    /// the quantity domain, and that is exactly the case that must not become
    /// a saturated or wrapped sample.
    struct ScriptedClock(std::cell::RefCell<std::collections::VecDeque<Tick>>);

    impl ScriptedClock {
        fn spans(nanoseconds: u64, reads: usize) -> Self {
            let seconds = i64::try_from(nanoseconds / 1_000_000_000).unwrap();
            let rest = i64::try_from(nanoseconds % 1_000_000_000).unwrap();
            let mut ticks = std::collections::VecDeque::new();
            for _ in 0..reads {
                ticks.push_back(Tick::new(0, 0).unwrap());
                ticks.push_back(Tick::new(seconds, rest).unwrap());
            }
            Self(std::cell::RefCell::new(ticks))
        }
    }

    impl Clock for ScriptedClock {
        fn read(&self) -> Result<Tick, intlify_measurement::acquisition::ClockFailure> {
            Ok(self.0.borrow_mut().pop_front().expect("scripted read"))
        }
    }

    #[test]
    fn a_repetition_sum_past_the_quantity_domain_fails_the_case() {
        // Two repetitions, each just over half the domain. The sum does not
        // fit, and the case must fail rather than saturate or wrap.
        let prepared = prepare(FIXTURES[0]).unwrap();
        let sampling = Sampling {
            warmup: Quantity::new(0),
            samples: Repetitions::new(1).unwrap(),
            repetitions: Repetitions::new(2).unwrap(),
        };
        let clock = ScriptedClock::spans(u64::MAX / 2 + 2, 2);
        assert_eq!(
            capture_case(&clock, &prepared, sampling, binding()),
            Err(CaptureFailure::Overflow)
        );

        // The same two repetitions inside the domain are captured exactly.
        let clock = ScriptedClock::spans(u64::MAX / 2 - 2, 2);
        let capture = capture_case(&clock, &prepared, sampling, binding()).unwrap();
        assert_eq!(
            capture.samples[0].aggregate_nanoseconds,
            Quantity::new((u64::MAX / 2 - 2) * 2)
        );
    }

    #[test]
    fn a_samples_local_identity_is_distinct_per_case_and_ordinal() {
        let binding = binding();
        assert_ne!(
            binding.local("sample-local", 0),
            binding.local("sample-local", 1)
        );
        // The execution identity is a different domain than the local one, so
        // one is never mistaken for the other.
        assert_ne!(
            binding.local("sample-local", 0),
            binding.local("sample-execution", 0)
        );
    }
}
