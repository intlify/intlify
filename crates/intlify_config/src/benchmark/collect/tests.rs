// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

#![cfg(any(target_os = "linux", target_os = "macos"))]

use serde_json::{json, Value};

use crate::benchmark::clock::tests::ScriptedClock;
use crate::benchmark::observation::{Digest, Frame};
use crate::benchmark::operation::tests::operations;
use crate::benchmark::operation::Operation;
use crate::benchmark::quantity::{Quantity, Repetitions};
use crate::benchmark::sample::{CaptureCapacity, CaptureFailureCause, CaptureStage};
use crate::profile_fixtures::{minimal_config, reference};
use crate::structural::selection::Selection;

use super::*;

fn sampling() -> Sampling {
    Sampling::admit(
        Quantity::new(1),
        Repetitions::new(2).unwrap(),
        Repetitions::new(2).unwrap(),
        CaptureCapacity {
            warmup_repetitions: Quantity::new(1),
            samples: Repetitions::new(2).unwrap(),
            repetitions_per_sample: Repetitions::new(2).unwrap(),
            total_invocations: Repetitions::new(5).unwrap(),
        },
    )
    .unwrap()
}

fn id(value: &str) -> Digest {
    let mut frame = Frame::new("collector-test-binding");
    frame.text(value);
    frame.finish()
}

fn binding(operation: Operation) -> CaptureBinding {
    CaptureBinding {
        run: id("one-test-run"),
        case: id(operation.boundary()),
    }
}

// Unit-test fixture preparation, not the production registry's eventual oracle.
// The synthetic interval has no physical value; check known fixture semantics
// before deriving its observation. No measured sample supplies its own expected
// checksum, and the fixed fake duration is never included in collected samples.
fn fixture_observation(prepared: &Prepared) -> (Observation, LogicalWork) {
    use crate::benchmark::operation::Output;
    let output = prepared.once(&ScriptedClock::nanos([0, 1])).unwrap().output;
    match &output {
        Output::Entry(Ok(doc)) => {
            assert_eq!(doc.decode::<Value>(doc.root()).unwrap(), minimal_config());
        }
        Output::Structural(Ok(analysis)) => assert!(analysis.is_complete()),
        Output::Authoring(Ok(Some(config))) => {
            assert_eq!(serde_json::to_value(config).unwrap(), minimal_config());
        }
        Output::Select(Ok(Selection::Selected(selected))) => {
            assert_eq!(selected.id().as_str(), "app");
            assert_eq!(
                serde_json::to_value(selected.resource_limits()).unwrap(),
                reference("resource-limit-policy")
            );
        }
        Output::Locale(Ok(result)) => {
            assert_eq!(result.locale().as_str(), "en-US");
            assert_eq!(result.suggested_replacement(), Some("en-US"));
        }
        Output::LocaleCore(result) => {
            let core = result.value().unwrap();
            assert_eq!(core.source_default(), None);
            assert_eq!(
                core.requested()
                    .iter()
                    .map(crate::locale::CanonicalLocale::as_str)
                    .collect::<Vec<_>>(),
                ["en"]
            );
            assert_eq!(core.requested_default().as_str(), "en");
            assert!(result.corrections().is_empty());
        }
        _ => panic!("fixture preparation did not produce its known result"),
    }
    (
        output.observe().unwrap(),
        LogicalWork::observe(prepared, &output).unwrap(),
    )
}

#[test]
fn all_active_real_pairs_produce_revalidated_serializable_owner_fragments() {
    let clock = MonotonicClock::acquire().unwrap();
    for prepared in operations() {
        let operation = prepared.operation();
        let (expected, work) = fixture_observation(&prepared);
        let collected = collect_prepared(
            &clock,
            &prepared,
            expected,
            &work,
            id("collector-unit-test-context"),
            sampling(),
            binding(operation),
        )
        .unwrap();
        assert_eq!(collected.capture.warmup_completed.get(), 1);
        assert_eq!(collected.capture.samples.len(), 2);
        for sample in &collected.capture.samples {
            assert_eq!(sample.repetition_count.get(), 2);
            assert_eq!(sample.semantic_observation, expected);
        }
        let wire = serde_json::to_vec(&collected).unwrap();
        let decoded: CollectedOperation = serde_json::from_slice(&wire).unwrap();
        assert_eq!(decoded, collected);
        assert!(decoded
            .validate_against(
                &prepared,
                clock.description(),
                expected,
                &work,
                sampling(),
                binding(operation)
            )
            .is_empty());
    }
}

#[test]
fn wrong_expected_semantics_returns_failure_not_a_serializable_success_prefix() {
    let clock = MonotonicClock::acquire().unwrap();
    let prepared = operations().into_iter().next().unwrap();
    let (mut expected, work) = fixture_observation(&prepared);
    expected.shared = id("deliberately-incorrect-fixture-observation");
    let error = collect_prepared(
        &clock,
        &prepared,
        expected,
        &work,
        id("collector-unit-test-context"),
        sampling(),
        binding(prepared.operation()),
    )
    .unwrap_err();
    let CollectionFailure::Capture(failure) = error else {
        panic!("expected a capture failure");
    };
    assert_eq!(failure.stage, CaptureStage::Warmup);
    assert!(matches!(
        failure.cause,
        CaptureFailureCause::SemanticObservationMismatch(_)
    ));
    assert!(failure.complete_sample_prefix.is_empty());
}

#[test]
fn unsupported_locale_input_cannot_be_recorded_as_a_successful_expected_failure() {
    use crate::benchmark::operation::OutputFailure;
    use std::sync::Arc;

    let clock = MonotonicClock::acquire().unwrap();
    let mut prepared = operations()
        .into_iter()
        .find(|prepared| prepared.operation() == Operation::LocaleCanonicalization)
        .unwrap();
    let (expected, work) = fixture_observation(&prepared);
    let Prepared::Locale { input, .. } = &mut prepared else {
        unreachable!()
    };
    *input = Arc::from("pt-BR"); // Valid locales outside the finite fixture are unsupported.
    let output = prepared.once(&ScriptedClock::nanos([0, 1])).unwrap().output;
    assert_eq!(
        output.observe(),
        Err(OutputFailure::LocaleProviderUnavailable)
    );
    assert!(LogicalWork::observe(&prepared, &output).is_err());
    let error = collect_prepared(
        &clock,
        &prepared,
        expected,
        &work,
        id("unsupported-locale-unit-test-context"),
        sampling(),
        binding(prepared.operation()),
    )
    .unwrap_err();
    let CollectionFailure::Capture(failure) = error else {
        panic!("expected capture failure")
    };
    assert_eq!(failure.stage, CaptureStage::Warmup);
    assert_eq!(
        failure.cause,
        CaptureFailureCause::Output(OutputFailure::LocaleProviderUnavailable)
    );
    assert!(failure.complete_sample_prefix.is_empty());
}

#[test]
fn unsupported_core_input_cannot_supply_samples_even_as_an_expected_failure() {
    use crate::benchmark::operation::{tests::core_operation, OutputFailure};
    let clock = MonotonicClock::acquire().unwrap();
    let (expected, work) = fixture_observation(&core_operation(&minimal_config()));
    let mut value = minimal_config();
    value["profiles"]["app"]["requestedLocales"] = json!(["pt-BR"]);
    let prepared = core_operation(&value);
    let output = prepared.once(&ScriptedClock::nanos([0, 1])).unwrap().output;
    assert_eq!(
        output.observe(),
        Err(OutputFailure::LocaleProviderUnavailable)
    );
    assert!(LogicalWork::observe(&prepared, &output).is_err());
    let error = collect_prepared(
        &clock,
        &prepared,
        expected,
        &work,
        id("unsupported-core-context"),
        sampling(),
        binding(prepared.operation()),
    )
    .unwrap_err();
    let CollectionFailure::Capture(failure) = error else {
        panic!("expected capture failure")
    };
    assert_eq!(
        failure.cause,
        CaptureFailureCause::Output(OutputFailure::LocaleProviderUnavailable)
    );
    assert!(failure.complete_sample_prefix.is_empty());
}

#[test]
fn decoded_fragments_cannot_rebind_themselves_to_a_different_case_or_run() {
    let clock = MonotonicClock::acquire().unwrap();
    let prepared = operations().into_iter().next().unwrap();
    let operation = prepared.operation();
    let (expected, work) = fixture_observation(&prepared);
    let collected = collect_prepared(
        &clock,
        &prepared,
        expected,
        &work,
        id("collector-unit-test-context"),
        sampling(),
        binding(operation),
    )
    .unwrap();
    let other_case = collected.validate_against(
        &operations()[2],
        clock.description(),
        expected,
        &work,
        sampling(),
        binding(Operation::AuthoringConstruction),
    );
    assert!(other_case
        .iter()
        .any(|issue| matches!(issue, CollectionIssue::Descriptor(_))));
    assert!(other_case
        .iter()
        .any(|issue| matches!(issue, CollectionIssue::Sample(_))));
    let mut other_run = binding(operation);
    other_run.run = id("another-run");
    assert!(!collected
        .validate_against(
            &prepared,
            clock.description(),
            expected,
            &work,
            sampling(),
            other_run
        )
        .is_empty());
}

#[test]
fn missing_unknown_or_tampered_record_parts_never_become_admitted_defaults() {
    let clock = MonotonicClock::acquire().unwrap();
    let prepared = operations().into_iter().next().unwrap();
    let operation = prepared.operation();
    let (expected, work) = fixture_observation(&prepared);
    let collected = collect_prepared(
        &clock,
        &prepared,
        expected,
        &work,
        id("collector-unit-test-context"),
        sampling(),
        binding(operation),
    )
    .unwrap();
    let original = serde_json::to_value(&collected).unwrap();
    for path in ["", "/capture", "/capture/samples/0"] {
        for key in original.pointer(path).unwrap().as_object().unwrap().keys() {
            let mut missing = original.clone();
            missing
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(serde_json::from_value::<CollectedOperation>(missing).is_err());
        }
        let mut extra = original.clone();
        extra
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), json!(true));
        assert!(serde_json::from_value::<CollectedOperation>(extra).is_err());
    }
    for (path, value) in [
        ("/capture/samples", json!([])),
        ("/capture/warmupCompleted", json!("0")),
        ("/descriptors/method/canonicalUnit", json!("millisecond")),
        ("/logicalWork/facts/0/observation/value", json!("0")),
    ] {
        let mut tampered = original.clone();
        *tampered.pointer_mut(path).unwrap() = value;
        let decoded: CollectedOperation = serde_json::from_value(tampered).unwrap();
        assert!(!decoded
            .validate_against(
                &prepared,
                clock.description(),
                expected,
                &work,
                sampling(),
                binding(operation)
            )
            .is_empty());
    }
}

#[test]
fn all_pinned_cases_collect_and_revalidate_through_the_admitted_fixture_path() {
    let registry = crate::benchmark::cases::registry::Registry::load().unwrap();
    let clock = MonotonicClock::acquire().unwrap();
    for declaration in crate::benchmark::cases::declarations() {
        let fixture = registry.prepare(&declaration).unwrap();
        // Test-only binding; the fixture context is not a common Case ID codec.
        let binding = CaptureBinding {
            run: id("pinned-fixture-test-run"),
            case: fixture.input_context(),
        };
        let collected = collect_operation(&clock, &fixture, sampling(), binding).unwrap();
        assert_eq!(collected.fixture_input_context, fixture.input_context());
        let decoded: CollectedOperation =
            serde_json::from_slice(&serde_json::to_vec(&collected).unwrap()).unwrap();
        assert!(decoded
            .validate(&fixture, clock.description(), sampling(), binding)
            .is_empty());
        let mut altered = decoded;
        altered.fixture_input_context = id("another-fixture-input-context");
        assert_eq!(
            altered.validate(&fixture, clock.description(), sampling(), binding),
            vec![CollectionIssue::FixtureInputContext]
        );
    }
}

#[test]
fn equal_outputs_and_work_cannot_rebind_a_record_to_different_input_bounds() {
    use crate::benchmark::cases::{declarations, LimitEdge, LimitKind, Recipe};
    let registry = crate::benchmark::cases::registry::Registry::load().unwrap();
    let cases = declarations();
    let base = registry
        .prepare(
            cases
                .iter()
                .find(|case| {
                    case.operation == Operation::FileMaterialization
                        && case.fixture == Recipe::Minimal
                        && case.limit.is_none()
                })
                .unwrap(),
        )
        .unwrap();
    let exact = registry
        .prepare(
            cases
                .iter()
                .find(|case| case.limit == Some((LimitKind::FileBytes, LimitEdge::Exact)))
                .unwrap(),
        )
        .unwrap();
    assert_eq!(base.expected(), exact.expected());
    assert!(base.expected_work().matches_expected(exact.expected_work()));
    assert_ne!(base.input_context(), exact.input_context());
    let clock = MonotonicClock::acquire().unwrap();
    let binding = binding(Operation::FileMaterialization);
    let collected = collect_operation(&clock, &base, sampling(), binding).unwrap();
    // Even retaining the same run/case labels cannot conceal a changed context.
    assert_eq!(
        collected.validate(&exact, clock.description(), sampling(), binding),
        vec![CollectionIssue::FixtureInputContext]
    );
}

#[test]
fn equal_locale_results_cannot_conceal_a_changed_provider_input_byte_bound() {
    use crate::benchmark::cases::{declarations, LimitEdge, LimitKind, LocaleRecipe, Recipe};
    let registry = crate::benchmark::cases::registry::Registry::load().unwrap();
    let cases = declarations();
    let base = registry
        .prepare(
            cases
                .iter()
                .find(|case| {
                    case.fixture == Recipe::Locale(LocaleRecipe::Region) && case.limit.is_none()
                })
                .unwrap(),
        )
        .unwrap();
    let exact = registry
        .prepare(
            cases
                .iter()
                .find(|case| {
                    case.limit == Some((LimitKind::LocaleRawIdentifierBytes, LimitEdge::Exact))
                })
                .unwrap(),
        )
        .unwrap();
    assert_eq!(base.expected(), exact.expected());
    assert!(base.expected_work().matches_expected(exact.expected_work()));
    assert_ne!(base.input_context(), exact.input_context());
    let clock = MonotonicClock::acquire().unwrap();
    let binding = binding(Operation::LocaleCanonicalization);
    let collected = collect_operation(&clock, &base, sampling(), binding).unwrap();
    assert_eq!(
        collected.validate(&exact, clock.description(), sampling(), binding),
        vec![
            CollectionIssue::FixtureInputContext,
            CollectionIssue::Descriptor(DescriptorIssue::LocaleInputBindingMismatch),
        ]
    );
}
