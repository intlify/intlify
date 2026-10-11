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

use super::cases::{admit, prepare, Fixture, Prepared, Reads, FIXTURES, SCOPE};
use super::descriptor::Descriptors;
use super::operation::{self, LogicalWork, Observed};
use super::projection::CaseProjection;

/// Every spelling this owner registers.
pub(super) const LABELS: Labels = Labels {
    owner: "intlify-authoring-js",
    framing: Framing::new("intlify-authoring-js-minimum-observation/0"),
    plan_codec: "intlify-authoring-js-owner-run-plan/0",
    result_codec: "intlify-authoring-js-owner-run-result/0",
    result_domain: "intlify-authoring-js-owner-result-v0",
    runner_domain: "intlify-authoring-js-local-runner-instance-v0",
    build_schema: "intlify-authoring-js-build-observation/0",
    subject: "intlify-authoring-js-phase2-discovery",
    profile: Versioned::new("intlify-authoring-js-minimum-smoke", "0"),
    harness: Versioned::new("intlify-authoring-js-owner-run-harness", "0"),
    projection: Versioned::new("intlify-authoring-js-minimum-to-026", "0"),
    native_rule: Versioned::new(
        "intlify-authoring-js-native-unmanaged-component-context",
        "0",
    ),
    memory_rule: Versioned::new("intlify-authoring-js-duration-only-no-memory-observer", "0"),
    instrumentation: Versioned::new("intlify-authoring-js-owner-run-instrumentation", "0"),
    concurrency: Versioned::new("intlify-authoring-js-owner-run-concurrency", "0"),
};

/// Source discovery and inventory assembly, measured as one owner.
///
/// The types it names below are `pub` inside private modules: a public trait's
/// associated types must be, and none of them is reachable from outside the
/// `benchmark` module.
pub struct AuthoringJsDiscovery;

impl Owner for AuthoringJsDiscovery {
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
        let context = &prepared.context;
        match &prepared.reads {
            Reads::Unit(case) => {
                // Admission checks the bytes against their snapshot. It is not
                // source discovery, so it runs here, before any interval opens.
                let units = admit(
                    context,
                    &case.profile,
                    &case.snapshot,
                    &case.bytes,
                    &case.limits,
                )
                .map_err(CaptureFailure::Preparation)?;
                capturing.capture(
                    expected,
                    (
                        context,
                        &case.profile,
                        &units[0],
                        &case.limits,
                        &case.workspace,
                    ),
                    operation::invoke_classify,
                    operation::observe_classify,
                )
            }
            Reads::Inventory(case) => capturing.capture(
                expected,
                (
                    context,
                    &case.profile,
                    SCOPE,
                    case.completeness,
                    case.membership.as_slice(),
                    case.analyses.as_slice(),
                    &case.limits,
                ),
                operation::invoke_assemble,
                operation::observe_assemble,
            ),
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
            let capture = AuthoringJsDiscovery::capture(&capturing(&clock, 2), &prepared).unwrap();
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
        for name in ["vertical-slice-module", "inventory-complete"] {
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
                AuthoringJsDiscovery::capture(&capturing(&clock, 2), &prepared),
                Err(CaptureFailure::Overflow),
                "{name}"
            );
        }
    }
}
