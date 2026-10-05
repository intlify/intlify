// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! This owner, as the shared owner run drives it.
//!
//! The run itself — issuance, capture, the sealed owner record, admission, and
//! the adapter onto the common pipeline — belongs to `intlify_measurement`.
//! What is here is what only this owner knows: its registered spellings, and
//! which of its four operations each fixture invokes on what input.

use intlify_measurement::acquisition::Clock;
use intlify_measurement::environment::ClockObservation;
use intlify_measurement::owner_run::capture::{Capture, CaptureFailure, Capturing};
use intlify_measurement::owner_run::observation::Framing;
use intlify_measurement::owner_run::run::PreparationFailure;
use intlify_measurement::owner_run::{Labels, Owner, Package, Versioned};

use super::cases::{prepare, Fixture, Input, Prepared, FIXTURES};
use super::descriptor::Descriptors;
use super::operation::{self, LogicalWork, Observed, Operation};
use super::projection::CaseProjection;

/// Every spelling this owner registers.
pub(super) const LABELS: Labels = Labels {
    owner: "intlify-authoring",
    framing: Framing::new("intlify-authoring-minimum-observation/0"),
    plan_codec: "intlify-authoring-owner-run-plan/1",
    // Revision 2 records the host and toolchain views in the run context.
    result_codec: "intlify-authoring-owner-run-result/2",
    result_domain: "intlify-authoring-owner-result-v1",
    runner_domain: "intlify-authoring-local-runner-instance-v0",
    build_schema: "intlify-authoring-build-observation/0",
    subject: "intlify-authoring-phase1-semantics",
    profile: Versioned::new("intlify-authoring-minimum-smoke", "0"),
    harness: Versioned::new("intlify-authoring-owner-run-harness", "2"),
    projection: Versioned::new("intlify-authoring-minimum-to-026", "1"),
    native_rule: Versioned::new("intlify-authoring-native-unmanaged-component-context", "0"),
    memory_rule: Versioned::new("intlify-authoring-duration-only-no-memory-observer", "0"),
    instrumentation: Versioned::new("intlify-authoring-owner-run-instrumentation", "0"),
    concurrency: Versioned::new("intlify-authoring-owner-run-concurrency", "0"),
};

/// The four Phase 1 authoring operations, measured as one owner.
///
/// The types it names below are `pub` inside private modules: a public trait's
/// associated types must be, and none of them is reachable from outside the
/// `benchmark` module.
pub struct AuthoringSemantics;

impl Owner for AuthoringSemantics {
    const LABELS: Labels = LABELS;
    const PACKAGE: Package = Package {
        name: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
        assertions: cfg!(debug_assertions),
    };

    type Fixture = Fixture;
    type Prepared = Prepared;
    type Projection = CaseProjection;
    type Descriptors = Descriptors;
    type Work = LogicalWork;

    fn fixtures() -> &'static [Fixture] {
        &FIXTURES
    }

    fn prepare(fixture: Fixture) -> Result<Prepared, PreparationFailure> {
        prepare(fixture)
    }

    fn expected(prepared: &Prepared) -> &Observed {
        prepared.expected()
    }

    fn project(prepared: &Prepared) -> CaseProjection {
        CaseProjection::of(prepared)
    }

    fn descriptors(prepared: &Prepared, clock: ClockObservation) -> Descriptors {
        Descriptors::for_acquisition(prepared.fixture.operation, clock)
    }

    fn capture<C: Clock>(
        capturing: &Capturing<'_, C>,
        prepared: &Prepared,
    ) -> Result<Capture<LogicalWork>, CaptureFailure> {
        let expected = prepared.expected();
        match prepared.fixture.operation {
            Operation::LiteralEncode => {
                let Input::Literal(text) = prepared.fixture.input else {
                    return Err(CaptureFailure::Preparation(PreparationFailure::Fixture));
                };
                capturing.capture(
                    expected,
                    (text, &prepared.limits, &prepared.segments),
                    operation::invoke_encode,
                    operation::observe_encode,
                )
            }
            Operation::Mf2ParseAndSemanticFacts => {
                let Input::Mf2(source) = prepared.fixture.input else {
                    return Err(CaptureFailure::Preparation(PreparationFailure::Fixture));
                };
                capturing.capture(
                    expected,
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
                capturing.capture(
                    expected,
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
                capturing.capture(
                    expected,
                    (&declaration, analysis, facts, &prepared.limits),
                    operation::invoke_facts,
                    operation::observe_facts,
                )
            }
        }
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use super::*;
    use intlify_measurement::acquisition::MonotonicClock;
    use intlify_measurement::owner_run::capture::Binding;
    use intlify_measurement::owner_run::context::SamplingPolicy;
    use intlify_shared_json::quantity::{Quantity, Repetitions};

    fn capturing(clock: &MonotonicClock) -> Capturing<'_, MonotonicClock> {
        let digest = |value: &str| {
            let mut frame = LABELS.framing.frame("test-binding");
            frame.text(value);
            frame.finish()
        };
        Capturing {
            clock,
            framing: LABELS.framing,
            sampling: SamplingPolicy {
                warmup_repetitions: Quantity::new(2),
                measured_samples: Repetitions::new(1).unwrap(),
                repetitions_per_sample: Repetitions::new(2).unwrap(),
            },
            binding: Binding {
                run: digest("run"),
                case: digest("case"),
            },
        }
    }

    #[test]
    fn every_fixture_captures_and_repeats_the_same_observation() {
        let clock = MonotonicClock::acquire().unwrap();
        for fixture in FIXTURES {
            let prepared = prepare(fixture).unwrap();
            let capture = AuthoringSemantics::capture(&capturing(&clock), &prepared).unwrap();
            assert_eq!(capture.warmup_completed, Quantity::new(2));
            assert_eq!(capture.samples.len(), 1);
            assert_eq!(
                capture.samples[0].repetition_count,
                Repetitions::new(2).unwrap()
            );
            // Two runs of the same fixture agree on the result, which is what
            // makes a later disagreement meaningful.
            let again = AuthoringSemantics::capture(&capturing(&clock), &prepared).unwrap();
            assert_eq!(
                again.samples[0].semantic_observation, capture.samples[0].semantic_observation,
                "{} disagreed with itself",
                fixture.name
            );
            assert_eq!(again.work, capture.work);
        }
    }

    #[test]
    fn a_fixture_whose_input_does_not_fit_its_operation_is_not_captured() {
        // The fixed inventory never pairs an operation with the other kind of
        // input, but if it did, the case would fail preparation rather than
        // measure something else.
        let clock = MonotonicClock::acquire().unwrap();
        let mut prepared = prepare(FIXTURES[0]).unwrap();
        prepared.fixture.input = Input::Mf2("Hello");
        assert_eq!(
            AuthoringSemantics::capture(&capturing(&clock), &prepared),
            Err(CaptureFailure::Preparation(PreparationFailure::Fixture))
        );
        let mut prepared = prepare(FIXTURES[3]).unwrap();
        prepared.fixture.input = Input::Literal("Hello");
        assert_eq!(
            AuthoringSemantics::capture(&capturing(&clock), &prepared),
            Err(CaptureFailure::Preparation(PreparationFailure::Fixture))
        );
    }
}
