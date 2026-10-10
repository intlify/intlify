// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! This owner, as the shared owner run drives it.
//!
//! The run itself — issuance, capture, the sealed owner record, admission, and
//! the adapter onto the common pipeline — belongs to `intlify_measurement`.
//! What is here is what only this owner knows: its registered spellings, and
//! which of its two operations each fixture invokes on what input.

use intlify_measurement::acquisition::Clock;
use intlify_measurement::environment::ClockObservation;
use intlify_measurement::owner_run::capture::{Capture, CaptureFailure, Capturing};
use intlify_measurement::owner_run::observation::Framing;
use intlify_measurement::owner_run::run::PreparationFailure;
use intlify_measurement::owner_run::{Labels, Owner, Package, Versioned};

use super::cases::{prepare, Fixture, Prepared, Reads, FIXTURES};
use super::descriptor::Descriptors;
use super::operation::{self, LogicalWork, Observed};
use super::projection::CaseProjection;

/// Every spelling this owner registers.
pub(super) const LABELS: Labels = Labels {
    owner: "intlify-authoring-identity",
    framing: Framing::new("intlify-authoring-identity-minimum-observation/0"),
    plan_codec: "intlify-authoring-identity-owner-run-plan/1",
    result_codec: "intlify-authoring-identity-owner-run-result/1",
    result_domain: "intlify-authoring-identity-owner-result-v1",
    runner_domain: "intlify-authoring-identity-local-runner-instance-v0",
    build_schema: "intlify-authoring-identity-build-observation/0",
    subject: "intlify-authoring-identity-phase3-reconciliation",
    profile: Versioned::new("intlify-authoring-identity-minimum-smoke", "0"),
    harness: Versioned::new("intlify-authoring-identity-owner-run-harness", "1"),
    projection: Versioned::new("intlify-authoring-identity-minimum-to-026", "0"),
    native_rule: Versioned::new(
        "intlify-authoring-identity-native-unmanaged-component-context",
        "0",
    ),
    memory_rule: Versioned::new(
        "intlify-authoring-identity-duration-only-no-memory-observer",
        "0",
    ),
    instrumentation: Versioned::new("intlify-authoring-identity-owner-run-instrumentation", "0"),
    concurrency: Versioned::new("intlify-authoring-identity-owner-run-concurrency", "0"),
};

/// Reconciliation and registry replay, measured as one owner.
///
/// The types it names below are `pub` inside private modules: a public trait's
/// associated types must be, and none of them is reachable from outside the
/// `benchmark` module.
pub struct AuthoringIdentityReconciliation;

impl Owner for AuthoringIdentityReconciliation {
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
        match &prepared.reads {
            Reads::Plan(case) => case
                .evidence
                // Checking the retained bytes against their snapshots is input
                // preparation, so it happens here, before any interval opens.
                .inputs(|inputs| {
                    capturing.capture(
                        expected,
                        (
                            &case.base,
                            &case.inventory,
                            inputs,
                            &case.limits,
                            &case.workspace,
                        ),
                        operation::invoke_plan,
                        operation::observe_plan,
                    )
                })
                .map_err(CaptureFailure::Preparation)?,
            Reads::Replay(case) => {
                // Indexing the retained history and accepting the anchor are
                // the host's preparation, not replay, so they happen here.
                let retained = case.history().map_err(CaptureFailure::Preparation)?;
                let anchor = case.anchor().map_err(CaptureFailure::Preparation)?;
                capturing.capture(
                    expected,
                    (
                        &case.head,
                        &anchor,
                        &retained,
                        &case.limits,
                        case.retained(),
                    ),
                    operation::invoke_replay,
                    operation::observe_replay,
                )
            }
        }
    }
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use intlify_measurement::acquisition::{ClockFailure, MonotonicClock, Tick};
    use intlify_measurement::owner_run::capture::Binding;
    use intlify_measurement::owner_run::context::SamplingPolicy;
    use intlify_shared_json::quantity::{Quantity, Repetitions};

    use super::super::cases::testing;
    use super::*;

    fn capturing<C>(clock: &C, repetitions: u64) -> Capturing<'_, C> {
        let digest = |value: &str| {
            let mut frame = LABELS.framing.frame("test-binding");
            frame.text(value);
            frame.finish()
        };
        Capturing {
            clock,
            framing: LABELS.framing,
            sampling: SamplingPolicy {
                warmup_repetitions: Quantity::new(0),
                measured_samples: Repetitions::new(1).unwrap(),
                repetitions_per_sample: Repetitions::new(repetitions).unwrap(),
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
            let capture =
                AuthoringIdentityReconciliation::capture(&capturing(&clock, 2), &prepared).unwrap();
            assert_eq!(capture.samples.len(), 1);
            // Every repetition was compared with the expectation, which a
            // fresh workspace established, while this case's own workspace
            // was reused between them.
            assert_eq!(
                capture.samples[0].semantic_observation,
                prepared.expected().observation,
                "{}",
                fixture.name
            );
            assert_eq!(&capture.work, &prepared.expected().work);
        }
    }

    #[test]
    fn a_planning_capture_reconciles_in_the_workspace_the_case_lends() {
        let clock = MonotonicClock::acquire().unwrap();
        let prepared = testing::prepared("catalog-allocation");
        let case = testing::plan_case(&prepared);
        assert_eq!(case.workspace.borrow().capacities().classes, 0);
        AuthoringIdentityReconciliation::capture(&capturing(&clock, 1), &prepared).unwrap();
        assert_ne!(case.workspace.borrow().capacities().classes, 0);
    }

    /// A clock whose readings the test writes in advance.
    struct ScriptedClock(RefCell<VecDeque<Tick>>);

    impl Clock for ScriptedClock {
        fn read(&self) -> Result<Tick, ClockFailure> {
            Ok(self.0.borrow_mut().pop_front().expect("scripted read"))
        }
    }

    #[test]
    fn a_repetition_sum_past_the_quantity_domain_fails_a_case() {
        // Two repetitions, each just over half the domain, do not fit. The
        // case fails rather than saturating or wrapping, for either operation.
        let span = u64::MAX / 2 + 2;
        let seconds = i64::try_from(span / 1_000_000_000).unwrap();
        let rest = i64::try_from(span % 1_000_000_000).unwrap();
        for name in ["first-allocation", "small-chain"] {
            let fixture = FIXTURES
                .into_iter()
                .find(|fixture| fixture.name == name)
                .unwrap();
            let prepared = prepare(fixture).unwrap();
            let clock = ScriptedClock(RefCell::new(
                (0..2)
                    .flat_map(|_| [Tick::new(0, 0).unwrap(), Tick::new(seconds, rest).unwrap()])
                    .collect(),
            ));
            assert_eq!(
                AuthoringIdentityReconciliation::capture(&capturing(&clock, 2), &prepared),
                Err(CaptureFailure::Overflow),
                "{name}"
            );
        }
    }
}
