// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::sync::Arc;

use serde_json::{json, Value};

use crate::benchmark::clock::tests::ScriptedClock;
use crate::benchmark::operation::{tests::operations, Schema};
use crate::input_limits::Bound;
use crate::materialize::materialize_file;
use crate::profile_fixtures::minimal_config;
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
        } else if prepared.operation() == Operation::LocalePolicyResolution {
            assert_eq!(work.facts.len(), 16);
            for item in &work.facts {
                let expected = if matches!(
                    item.kind,
                    WorkKind::LocalePolicyResolutionOccurrences
                        | WorkKind::LocalePolicyResolutionRawIdentifierBytes
                        | WorkKind::LocalePolicyResolutionNegotiationAliases
                        | WorkKind::LocalePolicyResolutionFallbackSources
                        | WorkKind::LocalePolicyResolutionMaximumFallbackCandidates
                        | WorkKind::LocalePolicyResolutionCoverageRules
                        | WorkKind::LocalePolicyResolutionCoverageLocaleSelectorItems
                        | WorkKind::LocalePolicyResolutionCoverageSurfaceSelectorItems
                ) {
                    WorkStage::PreparedInput
                } else {
                    WorkStage::OperationResult
                };
                assert_eq!(item.stage, expected);
            }
        } else if prepared.operation() == Operation::ResourcePolicyBootstrap {
            assert_eq!(work.facts.len(), 10);
            for (index, item) in work.facts.iter().enumerate() {
                let expected = if index < 7 {
                    WorkStage::PreparedInput
                } else {
                    WorkStage::OperationResult
                };
                assert_eq!(item.stage, expected, "{:?}", item.kind);
            }
        } else if prepared.operation() == Operation::ArtifactCollectionAdmission {
            assert_eq!(work.facts.len(), 8);
            for (index, item) in work.facts.iter().enumerate() {
                let expected = if index < 5 {
                    WorkStage::PreparedInput
                } else {
                    WorkStage::OperationResult
                };
                assert_eq!(item.stage, expected, "{:?}", item.kind);
            }
        } else if prepared.operation() == Operation::ResolvedBindingConstruction {
            assert_eq!(work.facts.len(), 8);
            for (index, item) in work.facts.iter().enumerate() {
                let expected = if index < 4 {
                    WorkStage::PreparedInput
                } else {
                    WorkStage::OperationResult
                };
                assert_eq!(item.stage, expected, "{:?}", item.kind);
            }
        } else if prepared.operation() == Operation::TargetAndGroupResolution {
            assert_eq!(work.facts.len(), 11);
            for (index, item) in work.facts.iter().enumerate() {
                let expected = if index < 6 {
                    WorkStage::PreparedInput
                } else {
                    WorkStage::OperationResult
                };
                assert_eq!(item.stage, expected, "{:?}", item.kind);
            }
        } else if prepared.operation() == Operation::HydrationRelationValidation {
            assert_eq!(work.facts.len(), 9);
            for (index, item) in work.facts.iter().enumerate() {
                let expected = if index < 4 {
                    WorkStage::PreparedInput
                } else {
                    WorkStage::OperationResult
                };
                assert_eq!(item.stage, expected, "{:?}", item.kind);
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
    use crate::locale::fixtures::{fixture_pins, FixtureProvider};
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
            &fixture_pins(),
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

/// Each recipe's vector: occurrences, their raw bytes, the spellings that
/// reached the provider, the canonical requested locales, then the units,
/// units not evaluated, blocking issues, and corrections of both phases; and
/// Phase 8's alias entries, fallback sources, longest fallback sequence,
/// coverage rules, locale and surface selector items, decision cells, and
/// rule-cell comparisons. Counted by hand from the recipes and the units
/// Phases 7 and 8 record for each subject: no policy declared adds seven
/// Phase 8 units and one cell per requested locale and vocabulary member.
#[test]
fn locale_resolution_work_counts_every_role_and_marks_unresolved_sets_unavailable() {
    use crate::benchmark::cases::{
        declarations, prepare::prepare, Engine, LimitEdge, LimitKind,
        LocalePolicyResolutionRecipe as R, Recipe,
    };
    let over = |kind| Some((kind, LimitEdge::FirstOver));
    let all = |values: [u64; 8]| values.map(Some);
    let join = |phase_7: [Option<u64>; 8], phase_8: [Option<u64>; 8]| {
        let mut joined = [None; 16];
        joined[..8].copy_from_slice(&phase_7);
        joined[8..].copy_from_slice(&phase_8);
        joined
    };
    // No policy declared, over the requested locales Phase 7 resolved: a
    // table of each by the vocabulary's three members, compared with no rule.
    let undeclared = |requested: Option<u64>| {
        let mut counts = [Some(0); 8];
        counts[6] = requested.map(|requested| requested * 3);
        counts[7] = requested.map(|_| 0);
        counts
    };
    // Two aliases, two fallback sources whose longest sequence holds two
    // candidates, and three rules with three locale and two surface items,
    // compared with each of the nine cells.
    let every_role = |resolved: bool| {
        [
            Some(2),
            Some(2),
            Some(2),
            Some(3),
            Some(3),
            Some(2),
            resolved.then_some(9),
            resolved.then_some(27),
        ]
    };
    for (recipe, limit, expected) in [
        (
            R::Minimal,
            None,
            join(all([3, 6, 3, 1, 18, 0, 0, 0]), undeclared(Some(1))),
        ),
        (
            R::Multi,
            None,
            join(all([7, 23, 7, 4, 34, 0, 0, 0]), undeclared(Some(4))),
        ),
        (
            R::Aliased,
            None,
            join(all([7, 23, 7, 4, 34, 0, 0, 5]), undeclared(Some(4))),
        ),
        (
            R::ExpandingAlias,
            None,
            join(all([3, 36, 3, 1, 18, 0, 0, 2]), undeclared(Some(1))),
        ),
        (
            R::InvalidSource,
            None,
            join(all([4, 11, 4, 1, 19, 0, 1, 0]), undeclared(Some(1))),
        ),
        // Without the requested set, the table and its comparisons wait.
        (
            R::InvalidAndDuplicate,
            None,
            join(
                [
                    Some(6),
                    Some(21),
                    Some(6),
                    None,
                    Some(18),
                    Some(5),
                    Some(3),
                    Some(1),
                ],
                undeclared(None),
            ),
        ),
        (
            R::DuplicateHeavy,
            None,
            join(all([34, 68, 34, 1, 18, 0, 1, 0]), undeclared(Some(1))),
        ),
        (
            R::ExactDuplicate,
            None,
            join(all([4, 8, 4, 1, 18, 0, 1, 0]), undeclared(Some(1))),
        ),
        // Over the occurrence bound, every spelling stands for itself and
        // waits on the bound; Phase 8 still counts what was declared, and its
        // table waits on the requested set.
        (
            R::Multi,
            over(LimitKind::ResolutionLocaleOccurrences),
            join(
                [
                    Some(7),
                    Some(23),
                    Some(0),
                    None,
                    Some(22),
                    Some(16),
                    Some(1),
                    Some(0),
                ],
                undeclared(None),
            ),
        ),
        (
            R::Multi,
            over(LimitKind::ResolutionRequestedLocales),
            join(all([7, 23, 7, 4, 34, 0, 1, 0]), undeclared(Some(4))),
        ),
        (
            R::Minimal,
            over(LimitKind::ResolutionRawIdentifierBytes),
            join(
                [
                    Some(3),
                    Some(6),
                    Some(3),
                    None,
                    Some(15),
                    Some(6),
                    Some(3),
                    Some(0),
                ],
                undeclared(None),
            ),
        ),
        (
            R::ExpandingAlias,
            over(LimitKind::ResolutionCanonicalIdentifierBytes),
            join(
                [
                    Some(3),
                    Some(36),
                    Some(3),
                    None,
                    Some(15),
                    Some(5),
                    Some(2),
                    Some(0),
                ],
                undeclared(None),
            ),
        ),
        // Three of the 21 spellings are coverage selectors, which Phase 8
        // canonicalizes: 47 Phase 8 units, nine of them cells.
        (
            R::EveryRole,
            None,
            join(all([21, 57, 18, 3, 75, 0, 0, 0]), every_role(true)),
        ),
        // Phase 8 still counts what was declared and checks each Intent
        // source candidate and surface item: 12 of its 35 units run, and
        // every unit needing a canonical locale waits on the bound.
        (
            R::EveryRole,
            over(LimitKind::ResolutionLocaleOccurrences),
            join(
                [
                    Some(21),
                    Some(57),
                    Some(0),
                    None,
                    Some(63),
                    Some(50),
                    Some(1),
                    Some(0),
                ],
                every_role(false),
            ),
        ),
        (
            R::EveryRoleAliased,
            None,
            join(all([21, 57, 18, 3, 75, 0, 0, 12]), every_role(true)),
        ),
        // The invalid alias key and destination, fallback source, and
        // literal leave six Phase 8 units unevaluated.
        (
            R::EveryRoleInvalid,
            None,
            join(all([21, 78, 18, 3, 75, 7, 6, 0]), every_role(true)),
        ),
        (
            R::TargetDuplicate,
            None,
            join(all([22, 62, 19, 3, 75, 0, 1, 1]), every_role(true)),
        ),
        // The rejected selector item leaves its rule without a domain, so
        // its two other units and the nine cells wait.
        (
            R::CoverageUncanonicalized,
            None,
            join(all([21, 57, 18, 3, 74, 11, 1, 0]), every_role(true)),
        ),
        (
            R::ManyTargets,
            None,
            join(all([34, 68, 34, 1, 80, 0, 0, 0]), undeclared(Some(1))),
        ),
        (
            R::EveryFixtureLocale,
            None,
            join(all([27, 204, 27, 13, 102, 0, 0, 0]), undeclared(Some(13))),
        ),
    ] {
        let declaration = declarations()
            .into_iter()
            .find(|case| {
                case.fixture == Recipe::LocalePolicyResolution(recipe) && case.limit == limit
            })
            .unwrap();
        let work = prepare(&declaration).unwrap().logical_work;
        assert_eq!(
            work.profile_identity,
            "intlify-config-locale-policy-resolution-work"
        );
        assert_eq!(work.facts.len(), expected.len());
        for (item, value) in work.facts.iter().zip(expected) {
            let expected = value.map_or(
                WorkValue::Unavailable {
                    reason: UnavailableWork::LocalePolicyResolutionRequestedNotResolved,
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
    // The ICU4X scales, through either engine: 256 targets of one locale,
    // two units each beyond the first; and 128 locales requested by the
    // project and its target, two units each in both sets and 384 cells.
    for (input, expected) in [
        (
            R::OccurrenceScale,
            join(all([258, 516, 258, 1, 528, 0, 0, 0]), undeclared(Some(1))),
        ),
        (
            R::CanonicalScale,
            join(
                all([257, 514, 257, 128, 907, 0, 0, 0]),
                undeclared(Some(128)),
            ),
        ),
    ] {
        for engine in [Engine::Reused, Engine::Fresh] {
            let declaration = declarations()
                .into_iter()
                .find(|case| case.fixture == Recipe::Icu4xLocalePolicyResolution { input, engine })
                .unwrap();
            let work = prepare(&declaration).unwrap().logical_work;
            assert_eq!(work.facts.len(), expected.len());
            for (item, value) in work.facts.iter().zip(expected) {
                assert_eq!(
                    item.observation,
                    WorkValue::exact(value.unwrap()),
                    "{input:?}, {engine:?}, {:?}",
                    item.kind
                );
            }
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
fn formal_configuration_work_agrees_with_an_independent_json_tree_count() {
    use crate::benchmark::cases::Recipe;

    // This oracle traverses serde_json's separately decoded tree, not the
    // materializer's nodes, its counts, or freshly printed golden checksums.
    // Tokens include punctuation and member names; logical nodes exclude keys.
    fn count(value: &Value, depth: u64, totals: &mut [u64; 6]) {
        totals[1] += 1;
        totals[2] = totals[2].max(depth);
        let string = |text: &str, totals: &mut [u64; 6]| {
            let bytes = u64::try_from(text.len()).unwrap();
            totals[4] += bytes;
            totals[5] = totals[5].max(bytes);
        };
        match value {
            Value::Object(members) => {
                let len = u64::try_from(members.len()).unwrap();
                totals[0] += 2 + len.saturating_sub(1) + 2 * len;
                totals[3] += len;
                for (key, child) in members {
                    string(key, totals);
                    count(child, depth + 1, totals);
                }
            }
            Value::Array(items) => {
                let len = u64::try_from(items.len()).unwrap();
                totals[0] += 2 + len.saturating_sub(1);
                totals[3] += len;
                for child in items {
                    count(child, depth + 1, totals);
                }
            }
            _ => {
                totals[0] += 1;
                if let Value::String(text) = value {
                    string(text, totals);
                }
            }
        }
    }

    for recipe in [
        Recipe::Minimal,
        Recipe::ReversedMembers,
        Recipe::PaddedBytes,
        Recipe::LongString,
        Recipe::TwoProfiles,
        Recipe::ManyProfiles,
        Recipe::ManyLocaleOccurrences,
    ] {
        let source = recipe.source();
        let value: Value = serde_json::from_slice(&source).unwrap();
        let mut expected = [0; 6];
        count(&value, 1, &mut expected);
        let work = observe(&Prepared::Entry {
            source: Arc::clone(&source),
            limits: crate::materialize_tests::limits(),
        });
        assert_eq!(
            fact(&work, WorkKind::RawFileBytes).observation,
            WorkValue::exact(u64::try_from(source.len()).unwrap())
        );
        for (kind, total) in [
            WorkKind::ParserTokensVisited,
            WorkKind::LogicalValueNodes,
            WorkKind::MaximumValueDepth,
            WorkKind::CollectionEntries,
            WorkKind::TotalDecodedStringBytes,
            WorkKind::MaximumDecodedStringBytes,
        ]
        .into_iter()
        .zip(expected)
        {
            assert_eq!(
                fact(&work, kind).observation,
                WorkValue::exact(total),
                "{recipe:?}, {kind:?}"
            );
            assert_eq!(fact(&work, kind).stage, WorkStage::OperationResult);
        }
    }
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
