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

use intlify_shared_json::quantity::{Quantity, Repetitions};
use serde::{Deserialize, Serialize};

use super::context::SamplingPolicy;
use super::observation::{Digest, Framing, Observation};
use super::run::PreparationFailure;
use crate::acquisition::{measure, Clock, MeasurementFailure};

/// What one measured invocation produced, observed after the interval closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observed<W> {
    /// The complete semantic observation.
    pub observation: Observation,
    /// The logical work the invocation performed, in the owner's vocabulary.
    pub work: W,
    /// Whether the operation produced its complete result.
    ///
    /// A fixture declares which path it expects, and this is what that
    /// declaration is checked against.
    pub complete: bool,
}

/// The binding one case's samples are local to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Binding {
    pub run: Digest,
    pub case: Digest,
}

impl Binding {
    /// Derive one identity local to this case, its domain, and an ordinal.
    #[must_use]
    pub fn local(self, framing: Framing, domain: &str, ordinal: u64) -> Digest {
        let mut frame = framing.frame(domain);
        frame.digest(self.run);
        frame.digest(self.case);
        frame.uint(ordinal);
        frame.finish()
    }
}

/// One captured sample of one case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapturedSample {
    pub local_identity: Digest,
    pub ordinal: Quantity,
    pub repetition_count: Repetitions,
    pub aggregate_nanoseconds: Quantity,
    pub semantic_observation: Observation,
    pub execution_identity: Digest,
}

/// The complete capture of one case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capture<W> {
    pub warmup_completed: Quantity,
    pub samples: Vec<CapturedSample>,
    pub work: W,
}

/// Why a case produced no complete capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "detail",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum CaptureFailure {
    /// The fixture could not be prepared.
    Preparation(PreparationFailure),
    /// A clock read or an invocation failed.
    Measurement(MeasurementFailure),
    /// A repetition sum exceeded the exact quantity domain.
    Overflow,
    /// A measured invocation produced a different result than the expectation.
    SemanticMismatch,
}

/// Where one case's samples are taken: its clock, its policy, and its binding.
pub struct Capturing<'c, C> {
    pub clock: &'c C,
    pub framing: Framing,
    pub sampling: SamplingPolicy,
    pub binding: Binding,
}

impl<C: Clock> Capturing<'_, C> {
    /// Capture one operation against its established expectation.
    ///
    /// `invoke` is the only thing inside an interval. `observe` runs after the
    /// end read, on the output the invocation returned and the same input.
    ///
    /// A prefix of samples taken before a failure is discarded rather than
    /// returned.
    pub fn capture<I: Copy, O, W: PartialEq + Clone>(
        &self,
        expected: &Observed<W>,
        input: I,
        invoke: fn(I) -> O,
        observe: fn(&O, I) -> Observed<W>,
    ) -> Result<Capture<W>, CaptureFailure> {
        let sampling = self.sampling;
        let mut warmup_completed = 0_u64;
        for _ in 0..sampling.warmup_repetitions.get() {
            let measured =
                measure(self.clock, input, invoke).map_err(CaptureFailure::Measurement)?;
            drop(measured);
            warmup_completed += 1;
        }

        let mut samples = Vec::new();
        for ordinal in 0..sampling.measured_samples.get() {
            let mut aggregate = 0_u64;
            for _ in 0..sampling.repetitions_per_sample.get() {
                let measured =
                    measure(self.clock, input, invoke).map_err(CaptureFailure::Measurement)?;
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
                local_identity: self.binding.local(self.framing, "sample-local", ordinal),
                ordinal: Quantity::new(ordinal),
                repetition_count: sampling.repetitions_per_sample,
                aggregate_nanoseconds: Quantity::new(aggregate),
                semantic_observation: expected.observation,
                execution_identity: self
                    .binding
                    .local(self.framing, "sample-execution", ordinal),
            });
        }
        Ok(Capture {
            warmup_completed: Quantity::new(warmup_completed),
            samples,
            work: expected.work.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acquisition::{ClockFailure, Tick};
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;

    const FRAMING: Framing = Framing::new("intlify-measurement-test-observation/0");

    fn digest(value: &str) -> Digest {
        let mut frame = FRAMING.frame("test-binding");
        frame.text(value);
        frame.finish()
    }

    fn binding() -> Binding {
        Binding {
            run: digest("run"),
            case: digest("case"),
        }
    }

    fn sampling(warmup: u64, samples: u64, repetitions: u64) -> SamplingPolicy {
        SamplingPolicy {
            warmup_repetitions: Quantity::new(warmup),
            measured_samples: Repetitions::new(samples).unwrap(),
            repetitions_per_sample: Repetitions::new(repetitions).unwrap(),
        }
    }

    /// A clock whose readings the test writes in advance.
    ///
    /// The real provider cannot be made to report an interval near the top of
    /// the quantity domain, and that is exactly the case that must not become
    /// a saturated or wrapped sample.
    struct ScriptedClock(RefCell<VecDeque<Tick>>);

    impl ScriptedClock {
        fn spans(nanoseconds: u64, reads: usize) -> Self {
            let seconds = i64::try_from(nanoseconds / 1_000_000_000).unwrap();
            let rest = i64::try_from(nanoseconds % 1_000_000_000).unwrap();
            let mut ticks = VecDeque::new();
            for _ in 0..reads {
                ticks.push_back(Tick::new(0, 0).unwrap());
                ticks.push_back(Tick::new(seconds, rest).unwrap());
            }
            Self(RefCell::new(ticks))
        }
    }

    impl Clock for ScriptedClock {
        fn read(&self) -> Result<Tick, ClockFailure> {
            Ok(self.0.borrow_mut().pop_front().expect("scripted read"))
        }
    }

    fn invoke(input: &str) -> usize {
        input.len()
    }

    #[allow(
        clippy::trivially_copy_pass_by_ref,
        reason = "the capture contract fixes this signature"
    )]
    fn observe(output: &usize, input: &str) -> Observed<u64> {
        let mut semantic = FRAMING.frame("length");
        semantic.text(input);
        semantic.uint(*output as u64);
        Observed {
            observation: Observation {
                semantic: semantic.finish(),
                positions: None,
            },
            work: *output as u64,
            complete: true,
        }
    }

    fn capturing(clock: &ScriptedClock, policy: SamplingPolicy) -> Capturing<'_, ScriptedClock> {
        Capturing {
            clock,
            framing: FRAMING,
            sampling: policy,
            binding: binding(),
        }
    }

    #[test]
    fn warmups_are_counted_and_every_sample_aggregates_its_repetitions() {
        let clock = ScriptedClock::spans(7, 2 + 3 * 2);
        let expected = observe(&invoke("abc"), "abc");
        let capture = capturing(&clock, sampling(2, 3, 2))
            .capture(&expected, "abc", invoke, observe)
            .unwrap();
        assert_eq!(capture.warmup_completed, Quantity::new(2));
        assert_eq!(capture.samples.len(), 3);
        for (ordinal, sample) in capture.samples.iter().enumerate() {
            assert_eq!(sample.ordinal, Quantity::new(ordinal as u64));
            assert_eq!(sample.repetition_count, Repetitions::new(2).unwrap());
            // Two repetitions of seven nanoseconds each; the warmups add none.
            assert_eq!(sample.aggregate_nanoseconds, Quantity::new(14));
            assert_eq!(sample.semantic_observation, expected.observation);
        }
        assert_eq!(capture.work, 3);
        // Every read the script supplied was used, and none was left over.
        assert!(clock.0.borrow().is_empty());
    }

    #[test]
    fn a_repetition_sum_past_the_quantity_domain_fails_the_case() {
        // Two repetitions, each just over half the domain. The sum does not
        // fit, and the case must fail rather than saturate or wrap.
        let expected = observe(&invoke("abc"), "abc");
        let clock = ScriptedClock::spans(u64::MAX / 2 + 2, 2);
        assert_eq!(
            capturing(&clock, sampling(0, 1, 2)).capture(&expected, "abc", invoke, observe),
            Err(CaptureFailure::Overflow)
        );

        // The same two repetitions inside the domain are captured exactly.
        let clock = ScriptedClock::spans(u64::MAX / 2 - 2, 2);
        let capture = capturing(&clock, sampling(0, 1, 2))
            .capture(&expected, "abc", invoke, observe)
            .unwrap();
        assert_eq!(
            capture.samples[0].aggregate_nanoseconds,
            Quantity::new((u64::MAX / 2 - 2) * 2)
        );
    }

    #[test]
    fn a_different_result_is_a_semantic_mismatch_rather_than_a_sample() {
        let clock = ScriptedClock::spans(1, 1);
        // The expectation was established for other input, so the measured
        // invocation disagrees with it.
        let expected = observe(&invoke("abcd"), "abcd");
        assert_eq!(
            capturing(&clock, sampling(0, 1, 1)).capture(&expected, "abc", invoke, observe),
            Err(CaptureFailure::SemanticMismatch)
        );

        // Agreeing on the observation but not on the work is a mismatch too.
        let clock = ScriptedClock::spans(1, 1);
        let mut expected = observe(&invoke("abc"), "abc");
        expected.work += 1;
        assert_eq!(
            capturing(&clock, sampling(0, 1, 1)).capture(&expected, "abc", invoke, observe),
            Err(CaptureFailure::SemanticMismatch)
        );
    }

    thread_local! {
        static CALLS: Cell<u32> = const { Cell::new(0) };
    }

    fn panics_on_second_call(input: &str) -> usize {
        let calls = CALLS.with(|calls| {
            calls.set(calls.get() + 1);
            calls.get()
        });
        assert!(calls < 2, "a failing invocation");
        input.len()
    }

    #[test]
    fn a_panic_inside_a_warmup_fails_the_case_without_a_sample() {
        CALLS.with(|calls| calls.set(0));
        let clock = ScriptedClock::spans(1, 3);
        let expected = observe(&invoke("abc"), "abc");
        // The first warmup succeeds and the second panics, so the case fails
        // before any sample is taken.
        assert_eq!(
            capturing(&clock, sampling(2, 1, 1)).capture(
                &expected,
                "abc",
                panics_on_second_call,
                observe,
            ),
            Err(CaptureFailure::Measurement(
                MeasurementFailure::InvocationPanicked
            ))
        );
    }

    #[test]
    fn a_samples_local_identity_is_distinct_per_case_and_ordinal() {
        let binding = binding();
        assert_ne!(
            binding.local(FRAMING, "sample-local", 0),
            binding.local(FRAMING, "sample-local", 1)
        );
        // The execution identity is a different domain than the local one, so
        // one is never mistaken for the other.
        assert_ne!(
            binding.local(FRAMING, "sample-local", 0),
            binding.local(FRAMING, "sample-execution", 0)
        );
        // Another case in the same run has its own identities.
        let other = Binding {
            case: digest("other case"),
            ..binding
        };
        assert_ne!(
            binding.local(FRAMING, "sample-local", 0),
            other.local(FRAMING, "sample-local", 0)
        );
    }
}
