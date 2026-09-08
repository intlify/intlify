// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::sync::Arc;

use serde_json::{json, Value};

use crate::benchmark::clock::tests::ScriptedClock;
use crate::benchmark::operation::{tests::operations, Schema};
use crate::fixtures::minimal_config;
use crate::input_limits::Bound;
use crate::materialize::materialize_file;
use crate::structural::selection::{InvalidSelectorType, SelectorInput};
use crate::structural::StructuralLimits;

use super::*;

fn limits() -> StructuralLimits {
    StructuralLimits {
        max_profiles: Bound::new(32).unwrap(),
        max_profile_id_bytes: Bound::new(3).unwrap(),
        max_structural_analysis_units: Bound::new(100_000).unwrap(),
    }
}

fn analysis(value: &Value, limits: StructuralLimits) -> Analysis {
    let doc = materialize_file(
        Arc::from(serde_json::to_vec(value).unwrap()),
        crate::materialize_tests::limits(),
    )
    .unwrap();
    Schema::for_model()
        .unwrap()
        .analyze(Arc::new(doc), limits)
        .unwrap()
}

fn observe(prepared: &Prepared) -> LogicalWork {
    let measured = prepared.once(&ScriptedClock::nanos([0, 1])).unwrap();
    LogicalWork::observe(prepared, &measured.output).unwrap()
}

fn fact(work: &LogicalWork, kind: WorkKind) -> &WorkFact {
    work.facts.iter().find(|fact| fact.kind == kind).unwrap()
}

#[test]
fn every_active_operation_retains_the_complete_ordered_vocabulary_and_its_stages() {
    for prepared in operations() {
        let work = observe(&prepared);
        assert_eq!(work.operation, prepared.operation());
        assert_eq!(
            work.facts.iter().map(|fact| fact.kind).collect::<Vec<_>>(),
            WorkKind::for_operation(prepared.operation())
        );
        for fact in &work.facts {
            assert_eq!(fact.unit, fact.kind.unit());
        }
        let decoded: LogicalWork =
            serde_json::from_slice(&serde_json::to_vec(&work).unwrap()).unwrap();
        assert!(decoded.matches_expected(&work));
        let input_stage = if prepared.operation() == Operation::FileMaterialization {
            WorkStage::OperationResult
        } else {
            WorkStage::PreparedInput
        };
        if prepared.operation() == Operation::LocaleCanonicalization {
            assert_eq!(work.facts.len(), 5);
            assert_eq!(
                fact(&work, WorkKind::LocaleOccurrences).stage,
                WorkStage::PreparedInput
            );
            assert_eq!(
                fact(&work, WorkKind::RawLocaleIdentifierBytes).stage,
                WorkStage::PreparedInput
            );
            for kind in [
                WorkKind::CanonicalLocaleIdentifierBytes,
                WorkKind::RetainedCanonicalLocaleValues,
                WorkKind::LocaleCorrectionSuggestions,
            ] {
                assert_eq!(fact(&work, kind).stage, WorkStage::OperationResult);
            }
        } else if prepared.operation() == Operation::LocaleCoreResolution {
            assert_eq!(work.facts.len(), 8);
            for item in &work.facts {
                let expected = if matches!(
                    item.kind,
                    WorkKind::LocaleCoreOccurrences | WorkKind::LocaleCoreRawIdentifierBytes
                ) {
                    WorkStage::PreparedInput
                } else {
                    WorkStage::OperationResult
                };
                assert_eq!(item.stage, expected);
            }
        } else {
            assert_eq!(work.facts.len(), 14);
            assert_eq!(fact(&work, WorkKind::LogicalValueNodes).stage, input_stage);
        }
        if prepared.operation() == Operation::StructuralAnalysis {
            assert_eq!(
                fact(&work, WorkKind::StructuralAnalysisUnits).stage,
                WorkStage::OperationResult
            );
        }
        if prepared.operation() == Operation::AuthoringConstruction {
            assert_eq!(
                fact(&work, WorkKind::StructuralAnalysisUnits).stage,
                WorkStage::PreparedInput
            );
        }
    }
}

#[test]
fn single_locale_work_counts_bytes_and_retention_without_inventing_unobserved_values() {
    use crate::locale::fixtures::{fixture_binding, FixtureProvider};
    use crate::locale::Canonicalizer;

    // Independent expected lengths include an alias whose canonical form grows.
    for (input, bound, canonical_bytes, retained, suggestions) in [
        ("en-US", 128, Some(5), 1, 0),
        ("EN-us", 128, Some(5), 1, 1),
        ("und-u-ca-islamicc", 128, Some(22), 1, 1),
        ("en-US", 4, None, 0, 0),
        ("und-u-ca-islamicc", 21, Some(22), 0, 0),
        ("en_US", 128, None, 0, 0),
    ] {
        let core = Canonicalizer::bind(
            &fixture_binding(),
            Some(FixtureProvider::new()),
            Bound::new(bound).unwrap(),
        )
        .unwrap();
        let prepared = Prepared::Locale {
            core: Arc::new(core),
            input: Arc::from(input),
        };
        let work = observe(&prepared);
        assert_eq!(
            work.profile_identity,
            "intlify-config-minimum-single-locale-work"
        );
        assert_eq!(
            fact(&work, WorkKind::LocaleOccurrences).observation,
            WorkValue::exact(1)
        );
        assert_eq!(
            fact(&work, WorkKind::RawLocaleIdentifierBytes).observation,
            WorkValue::exact(input.len() as u64)
        );
        assert_eq!(
            fact(&work, WorkKind::CanonicalLocaleIdentifierBytes).observation,
            canonical_bytes.map_or(
                WorkValue::Unavailable {
                    reason: UnavailableWork::LocaleNotCanonicalized
                },
                WorkValue::exact
            )
        );
        assert_eq!(
            fact(&work, WorkKind::RetainedCanonicalLocaleValues).observation,
            WorkValue::exact(retained)
        );
        assert_eq!(
            fact(&work, WorkKind::LocaleCorrectionSuggestions).observation,
            WorkValue::exact(suggestions)
        );
    }
}

#[test]
fn locale_core_work_preserves_each_role_count_and_marks_unresolved_sets_unavailable() {
    use crate::benchmark::cases::{
        declarations, prepare::prepare, LimitEdge, LimitKind, LocaleCoreRecipe as R, Recipe,
    };
    let over = |kind| Some((kind, LimitEdge::FirstOver));
    for (recipe, limit, expected) in [
        (
            R::Minimal,
            None,
            [
                Some(2),
                Some(4),
                Some(1),
                Some(2),
                Some(4),
                Some(0),
                Some(0),
                Some(0),
            ],
        ),
        (
            R::Multi,
            None,
            [
                Some(6),
                Some(21),
                Some(4),
                Some(6),
                Some(21),
                Some(0),
                Some(0),
                Some(0),
            ],
        ),
        (
            R::Aliased,
            None,
            [
                Some(6),
                Some(21),
                Some(4),
                Some(6),
                Some(21),
                Some(0),
                Some(0),
                Some(5),
            ],
        ),
        (
            R::ExpandingAlias,
            None,
            [
                Some(2),
                Some(34),
                Some(1),
                Some(2),
                Some(44),
                Some(0),
                Some(0),
                Some(2),
            ],
        ),
        (
            R::InvalidSource,
            None,
            [
                Some(3),
                Some(9),
                Some(1),
                Some(0),
                Some(0),
                Some(1),
                Some(0),
                Some(0),
            ],
        ),
        (
            R::InvalidAndDuplicate,
            None,
            [
                Some(5),
                Some(19),
                None,
                Some(0),
                Some(0),
                Some(3),
                Some(2),
                Some(1),
            ],
        ),
        (
            R::DuplicateHeavy,
            None,
            [
                Some(33),
                Some(66),
                Some(1),
                Some(0),
                Some(0),
                Some(1),
                Some(32),
                Some(0),
            ],
        ),
        (
            R::ExactDuplicate,
            None,
            [
                Some(3),
                Some(6),
                Some(1),
                Some(0),
                Some(0),
                Some(1),
                Some(2),
                Some(0),
            ],
        ),
        (
            R::Multi,
            over(LimitKind::CoreActiveOccurrences),
            [
                Some(6),
                Some(21),
                None,
                Some(0),
                Some(0),
                Some(1),
                Some(0),
                Some(0),
            ],
        ),
        (
            R::Multi,
            over(LimitKind::CoreRequestedCardinality),
            [
                Some(6),
                Some(21),
                Some(4),
                Some(0),
                Some(0),
                Some(1),
                Some(0),
                Some(0),
            ],
        ),
        (
            R::Minimal,
            over(LimitKind::CoreRawIdentifierBytes),
            [
                Some(2),
                Some(4),
                None,
                Some(0),
                Some(0),
                Some(2),
                Some(0),
                Some(0),
            ],
        ),
        (
            R::ExpandingAlias,
            over(LimitKind::CoreCanonicalIdentifierBytes),
            [
                Some(2),
                Some(34),
                None,
                Some(0),
                Some(0),
                Some(2),
                Some(0),
                Some(0),
            ],
        ),
    ] {
        let declaration = declarations()
            .into_iter()
            .find(|case| case.fixture == Recipe::LocaleCore(recipe) && case.limit == limit)
            .unwrap();
        let work = prepare(&declaration).unwrap().logical_work;
        assert_eq!(
            work.profile_identity,
            "intlify-config-minimum-project-locale-core-work"
        );
        assert_eq!(work.facts.len(), expected.len());
        for (item, value) in work.facts.iter().zip(expected) {
            let expected = value.map_or(
                WorkValue::Unavailable {
                    reason: UnavailableWork::LocaleCoreRequestedNotResolved,
                },
                WorkValue::exact,
            );
            assert_eq!(
                item.observation, expected,
                "{recipe:?}, {limit:?}, {:?}",
                item.kind
            );
        }
        // A counter is checked even when the canonical result is unchanged.
        for index in 0..work.facts.len() {
            let mut changed = work.clone();
            changed.facts[index].observation = WorkValue::exact(u64::MAX);
            assert!(!changed.matches_expected(&work));
        }
    }
}

#[test]
fn scalar_counts_use_complete_logical_units_not_serialized_byte_lengths() {
    let prepared = Prepared::Entry {
        source: Arc::from(&br#""\u00df""#[..]),
        limits: crate::materialize_tests::limits(),
    };
    let work = observe(&prepared);
    for (kind, expected) in [
        (WorkKind::RawFileBytes, 8),
        (WorkKind::ParserTokensVisited, 1),
        (WorkKind::LogicalValueNodes, 1),
        (WorkKind::MaximumValueDepth, 1),
        (WorkKind::CollectionEntries, 0),
        (WorkKind::TotalDecodedStringBytes, 2),
        (WorkKind::MaximumDecodedStringBytes, 2),
    ] {
        assert_eq!(fact(&work, kind).observation, WorkValue::exact(expected));
    }
    assert_eq!(
        fact(&work, WorkKind::ProfileDeclarations).observation,
        WorkValue::NotApplicable {}
    );
}

#[test]
fn incomplete_raw_parsing_does_not_fabricate_zero_complete_value_counts() {
    for source in [&b"\xff"[..], &b"[0,"[..]] {
        let prepared = Prepared::Entry {
            source: Arc::from(source),
            limits: crate::materialize_tests::limits(),
        };
        let work = observe(&prepared);
        assert_eq!(fact(&work, WorkKind::RawFileBytes).unit, WorkUnit::Octet);
        assert_eq!(
            fact(&work, WorkKind::RawFileBytes).observation,
            WorkValue::exact(u64::try_from(source.len()).unwrap())
        );
        assert_eq!(
            fact(&work, WorkKind::LogicalValueNodes).observation,
            WorkValue::Unavailable {
                reason: UnavailableWork::RawInputNotComplete
            }
        );
        assert_eq!(
            fact(&work, WorkKind::StructuralAnalysisUnits).observation,
            WorkValue::NotApplicable {}
        );
    }
}

#[test]
fn logical_limit_failure_still_retains_complete_counts_after_parsing() {
    let mut limits = crate::materialize_tests::limits();
    limits.value.max_single_string_bytes = Bound::new(2).unwrap();
    let work = observe(&Prepared::Entry {
        source: Arc::from(&br#""abc""#[..]),
        limits,
    });
    assert_eq!(
        fact(&work, WorkKind::LogicalValueNodes).observation,
        WorkValue::exact(1)
    );
    assert_eq!(
        fact(&work, WorkKind::MaximumDecodedStringBytes).observation,
        WorkValue::exact(3)
    );
}

#[test]
fn suppressed_analysis_is_unavailable_but_retained_record_counts_are_actual_zeroes() {
    let doc = materialize_file(Arc::from(&b"{}"[..]), crate::materialize_tests::limits()).unwrap();
    let work = observe(&Prepared::Structural {
        schema: Schema::for_model().unwrap(),
        doc: Arc::new(doc),
        limits: limits(),
    });
    assert_eq!(
        fact(&work, WorkKind::StructuralAnalysisUnits).observation,
        WorkValue::Unavailable {
            reason: UnavailableWork::SchemaPrerequisiteUnavailable
        }
    );
    assert_eq!(
        fact(&work, WorkKind::ProfileDeclarations).observation,
        WorkValue::Unavailable {
            reason: UnavailableWork::ProfileContainerNotAdmitted
        }
    );
    assert_eq!(
        fact(&work, WorkKind::RetainedAdmissionIssues).observation,
        WorkValue::exact(1)
    );
    assert_eq!(
        fact(&work, WorkKind::RetainedSchemaFragments).observation,
        WorkValue::exact(0)
    );
    assert_eq!(
        fact(&work, WorkKind::RetainedSchemaIssuesIncludingAlternatives).observation,
        WorkValue::exact(0)
    );
}

#[test]
fn structural_first_over_keeps_the_complete_preflight_total_without_a_fake_evaluation() {
    let value = minimal_config();
    let units = analysis(&value, limits()).structural_units().unwrap();
    for (bound, expected_issues) in [(units, 0), (units - 1, 1)] {
        let mut capacity = limits();
        capacity.max_structural_analysis_units = Bound::new(bound).unwrap();
        let doc = materialize_file(
            Arc::from(serde_json::to_vec(&value).unwrap()),
            crate::materialize_tests::limits(),
        )
        .unwrap();
        let work = observe(&Prepared::Structural {
            schema: Schema::for_model().unwrap(),
            doc: Arc::new(doc),
            limits: capacity,
        });
        assert_eq!(
            fact(&work, WorkKind::StructuralAnalysisUnits).observation,
            WorkValue::exact(units)
        );
        assert_eq!(
            fact(&work, WorkKind::RetainedAdmissionIssues).observation,
            WorkValue::exact(expected_issues)
        );
        if bound < units {
            assert_eq!(
                fact(&work, WorkKind::RetainedSchemaFragments).observation,
                WorkValue::exact(0)
            );
        }
    }
}

#[test]
fn selector_counts_keep_only_the_first_over_witness_and_no_rejected_contents() {
    let bound = limits().max_profile_id_bytes;
    for (selector, expected) in [
        (SelectorInput::absent(bound), WorkValue::NotApplicable {}),
        (
            SelectorInput::invalid_type(InvalidSelectorType::Object, bound),
            WorkValue::NotApplicable {},
        ),
        (SelectorInput::string("app", bound), WorkValue::exact(3)),
        (SelectorInput::string("APP", bound), WorkValue::exact(3)),
        (
            SelectorInput::string("sensitive-never-retained", bound),
            WorkValue::AtLeast {
                value: Quantity::new(4),
            },
        ),
    ] {
        let work = observe(&Prepared::Select {
            analysis: analysis(&minimal_config(), limits()),
            selector,
        });
        assert_eq!(fact(&work, WorkKind::SelectorIdBytes).observation, expected);
        assert!(!serde_json::to_string(&work).unwrap().contains("sensitive"));
    }
}

#[test]
fn work_from_another_operation_is_not_accepted_and_invalid_core_results_have_no_vector() {
    let mut operations = operations().into_iter();
    let entry = operations.next().unwrap();
    let structural = operations.next().unwrap();
    let entry_output = entry.once(&ScriptedClock::nanos([0, 1])).unwrap().output;
    assert_eq!(
        LogicalWork::observe(&structural, &entry_output),
        Err(WorkFailure::OperationMismatch)
    );
    let blocked = Prepared::Authoring(analysis(&json!({}), limits()));
    let output = blocked.once(&ScriptedClock::nanos([0, 1])).unwrap().output;
    assert_eq!(
        LogicalWork::observe(&blocked, &output),
        Err(WorkFailure::InvalidOrdinaryResult)
    );
}

#[test]
fn decoded_missing_reordered_duplicate_or_changed_facts_do_not_match_the_fixture() {
    let prepared = operations().into_iter().next().unwrap();
    let expected = observe(&prepared);
    for change in 0..5 {
        let mut changed = expected.clone();
        match change {
            0 => {
                changed.facts.pop();
            }
            1 => changed.facts.reverse(),
            2 => changed.facts.push(changed.facts[0]),
            3 => changed.facts[0].stage = WorkStage::PreparedInput,
            _ => changed.facts[0].observation = WorkValue::exact(u64::MAX),
        }
        let decoded: LogicalWork =
            serde_json::from_value(serde_json::to_value(changed).unwrap()).unwrap();
        assert!(!decoded.matches_expected(&expected));
    }
    let original = serde_json::to_value(expected).unwrap();
    for path in [
        "",
        "/facts/0",
        "/facts/0/observation",
        "/facts/7/observation",
    ] {
        let mut changed = original.clone();
        changed
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), json!(true));
        assert!(
            serde_json::from_value::<LogicalWork>(changed).is_err(),
            "open object {path}"
        );
    }
}

#[test]
fn sample_capture_rechecks_work_even_when_the_semantic_checksum_matches() {
    use crate::benchmark::observation::Frame;
    use crate::benchmark::quantity::Repetitions;
    use crate::benchmark::sample::{
        collect_with_work, CaptureBinding, CaptureCapacity, CaptureFailureCause, CaptureStage,
        Sampling,
    };
    let prepared = operations().into_iter().next().unwrap();
    let output = prepared.once(&ScriptedClock::nanos([0, 1])).unwrap().output;
    let expected = output.observe().unwrap();
    let mut work = LogicalWork::observe(&prepared, &output).unwrap();
    work.facts[0].observation = WorkValue::exact(0);
    let count = Repetitions::new(1).unwrap();
    let sampling = Sampling::admit(
        Quantity::new(0),
        count,
        count,
        CaptureCapacity {
            warmup_repetitions: Quantity::new(0),
            samples: count,
            repetitions_per_sample: count,
            total_invocations: count,
        },
    )
    .unwrap();
    let binding = CaptureBinding {
        run: Frame::new("test-work-run").finish(),
        case: Frame::new("test-work-case").finish(),
    };
    let error = collect_with_work(
        &ScriptedClock::nanos([2, 9]),
        &prepared,
        expected,
        &work,
        sampling,
        binding,
    )
    .unwrap_err();
    assert_eq!(error.stage, CaptureStage::Measured);
    assert!(error.complete_sample_prefix.is_empty());
    let CaptureFailureCause::LogicalWorkMismatch(mismatch) = &error.cause else {
        panic!("expected independent workload mismatch");
    };
    assert_eq!(mismatch.expected, work);
    assert_ne!(mismatch.actual, work);
}
