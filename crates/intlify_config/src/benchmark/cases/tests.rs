// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::benchmark::operation::Schema;
use crate::input_limits::Bound;
use crate::materialize::materialize_file;
use crate::structural::selection::{InvalidSelectorType, Selection, SelectorInput};
use crate::structural::StructuralLimits;

use super::prepare::{prepare, PreparationFailure};
use super::*;

fn limits() -> StructuralLimits {
    StructuralLimits {
        max_profiles: Bound::new(64).unwrap(),
        max_profile_id_bytes: Bound::new(256).unwrap(),
        max_structural_analysis_units: Bound::new(1_000_000).unwrap(),
    }
}

#[test]
fn declared_matrix_is_finite_unique_ordered_and_covers_every_active_boundary() {
    let cases = declarations();
    assert_eq!(cases.len(), 127);
    assert_eq!(cases, declarations());
    let mut identities = BTreeSet::new();
    for case in &cases {
        let encoded = serde_json::to_vec(case).unwrap();
        assert!(identities.insert(encoded.clone()));
        let decoded: Declaration = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, *case);
        assert_eq!(case.fixture_revision, "1");
        assert_eq!(case.fixture.source(), case.fixture.source());
        assert!(case.fixture.source().len() < 100_000);
        if !matches!(
            case.operation,
            Operation::ProfileSelection | Operation::LocaleCoreResolution
        ) {
            assert_eq!(case.selector, Selector::Absent);
        }
    }
    for operation in Operation::ALL {
        assert!(cases.iter().any(|case| case.operation == operation));
    }
    for limit in LimitKind::ENTRY
        .into_iter()
        .chain(LimitKind::STRUCTURAL)
        .chain([
            LimitKind::LocaleRawIdentifierBytes,
            LimitKind::LocaleCanonicalIdentifierBytes,
            LimitKind::CoreActiveOccurrences,
            LimitKind::CoreRequestedCardinality,
            LimitKind::CoreRawIdentifierBytes,
            LimitKind::CoreCanonicalIdentifierBytes,
        ])
    {
        for edge in [LimitEdge::Exact, LimitEdge::FirstOver] {
            assert_eq!(
                cases
                    .iter()
                    .filter(|case| case.limit == Some((limit, edge)))
                    .count(),
                1
            );
        }
    }
}

#[test]
fn declared_unbounded_case_kinds_agree_with_independently_checked_fixture_semantics() {
    let schema = Schema::for_model().unwrap();
    let published: serde_json::Value = serde_json::from_str(include_str!(
        "../../../schema/project-profile-config-v0.schema.json"
    ))
    .unwrap();
    assert_eq!(schema.schema_body(), &published);
    let oracle = jsonschema::draft7::new(&published).unwrap();
    for case in declarations().into_iter().filter(|case| {
        case.limit.is_none()
            && !matches!(
                case.operation,
                Operation::LocaleCanonicalization | Operation::LocaleCoreResolution
            )
    }) {
        let source = case.fixture.source();
        let input = materialize_file(Arc::clone(&source), crate::materialize_tests::limits());
        if case.operation == Operation::FileMaterialization {
            assert_eq!(
                input.is_ok(),
                case.expected_kind == ExpectedKind::Materialized,
                "{case:?}"
            );
            continue;
        }
        let doc = Arc::new(input.unwrap_or_else(|_| panic!("case prerequisite {case:?}")));
        let value: serde_json::Value = serde_json::from_slice(&source).unwrap();
        let analysis = schema.analyze(doc, limits()).unwrap();
        if matches!(
            case.operation,
            Operation::StructuralAnalysis | Operation::AuthoringConstruction
        ) {
            assert_eq!(
                analysis.is_complete(),
                oracle.is_valid(&value),
                "independent schema oracle: {case:?}"
            );
        }
        match case.operation {
            Operation::StructuralAnalysis => assert_eq!(
                analysis.is_complete(),
                case.expected_kind == ExpectedKind::StructuralComplete,
                "{case:?}"
            ),
            Operation::AuthoringConstruction => assert_eq!(
                serde_json::to_value(analysis.construct().unwrap().unwrap()).unwrap(),
                value,
                "{case:?}"
            ),
            Operation::ProfileSelection => {
                let bound = limits().max_profile_id_bytes;
                let selector = match case.selector {
                    Selector::Absent => SelectorInput::absent(bound),
                    Selector::App => SelectorInput::string("app", bound),
                    Selector::Unknown => SelectorInput::string("unknown", bound),
                    Selector::InvalidSyntax => SelectorInput::string("APP", bound),
                    Selector::InvalidType => {
                        SelectorInput::invalid_type(InvalidSelectorType::Object, bound)
                    }
                    Selector::ExactByteLimit => SelectorInput::string(&"z".repeat(256), bound),
                    Selector::FirstOverByteLimit => SelectorInput::string(&"z".repeat(257), bound),
                };
                let actual = match analysis.select(&selector).unwrap() {
                    Selection::Selected(selected) => {
                        assert_eq!(selected.id().as_str(), "app");
                        ExpectedKind::Selected
                    }
                    Selection::Rejected(_) => ExpectedKind::SelectionRejected,
                    Selection::Unavailable(_) => ExpectedKind::SelectionUnavailable,
                };
                assert_eq!(actual, case.expected_kind, "{case:?}");
            }
            Operation::FileMaterialization
            | Operation::LocaleCanonicalization
            | Operation::LocaleCoreResolution => unreachable!(),
        }
    }
}

#[test]
fn raw_scaling_and_member_permutation_preserve_logical_input_but_not_source_identity() {
    let parse =
        |recipe: Recipe| serde_json::from_slice::<serde_json::Value>(&recipe.source()).unwrap();
    assert_eq!(parse(Recipe::Minimal), parse(Recipe::ReversedMembers));
    assert_eq!(parse(Recipe::Minimal), parse(Recipe::PaddedBytes));
    assert_ne!(Recipe::Minimal.source(), Recipe::ReversedMembers.source());
    assert_eq!(
        Recipe::PaddedBytes.source().len(),
        Recipe::Minimal.source().len() + 4096
    );
    assert_eq!(
        parse(Recipe::ManyProfiles)["profiles"]
            .as_object()
            .unwrap()
            .len(),
        16
    );
    assert_eq!(
        parse(Recipe::ManyLocaleOccurrences)["profiles"]["app"]["requestedLocales"]
            .as_array()
            .unwrap()
            .len(),
        32
    );
}

#[test]
fn every_finite_case_prepares_its_exact_result_and_work_including_all_limit_edges() {
    use crate::benchmark::clock::tests::ScriptedClock;
    use crate::benchmark::operation::Output;
    use crate::benchmark::work::LogicalWork;
    use crate::materialize::InputFailure;
    use crate::structural::StructuralFailure;
    let mut edges = 0;
    for declaration in declarations() {
        let candidate =
            prepare(&declaration).unwrap_or_else(|error| panic!("{declaration:?}: {error:?}"));
        assert_eq!(candidate.declaration, declaration);
        let repeated = candidate
            .prepared
            .once(&ScriptedClock::nanos([0, 1]))
            .unwrap()
            .output;
        assert_eq!(repeated.observe().unwrap(), candidate.observation);
        assert!(LogicalWork::observe(&candidate.prepared, &repeated)
            .unwrap()
            .matches_expected(&candidate.logical_work));
        if let Some((_, edge)) = declaration.limit {
            edges += 1;
            if edge == LimitEdge::FirstOver {
                match &candidate.output {
                    Output::Entry(Err(error)) => {
                        let InputFailure::ResourceLimits(violations) = &error.reason else {
                            panic!("wrong rejection {declaration:?}");
                        };
                        assert_eq!(violations.len(), 1);
                        assert_eq!(violations[0].actual, violations[0].limit.get() + 1);
                    }
                    Output::Structural(Ok(analysis)) => {
                        assert_eq!(analysis.issues().len(), 1);
                        let (StructuralFailure::ProfilesLimit { limit, actual }
                        | StructuralFailure::ProfileIdLimit { limit, actual }
                        | StructuralFailure::StructuralWorkLimit { limit, actual }) =
                            analysis.issues()[0].reason
                        else {
                            panic!("wrong structural rejection");
                        };
                        assert_eq!(actual, limit.get() + 1);
                    }
                    Output::Locale(Err(crate::locale::CanonicalizationFailure::ByteLimit {
                        limit,
                        actual,
                        ..
                    })) => assert_eq!(*actual, limit.get() + 1),
                    Output::LocaleCore(result) => {
                        use crate::locale::core::{Failure, Issue};
                        match result.value().unwrap_err() {
                            Failure::OccurrenceLimit { limit, actual } => {
                                assert_eq!(*actual, limit.get() + 1);
                            }
                            Failure::Issues(issues) => {
                                assert!(!issues.is_empty());
                                for issue in issues {
                                    match issue {
                                        Issue::RequestedLimit { limit, actual }
                                        | Issue::Canonicalization {
                                            reason:
                                                crate::locale::CanonicalizationFailure::ByteLimit {
                                                    limit,
                                                    actual,
                                                    ..
                                                },
                                            ..
                                        } => assert_eq!(*actual, limit.get() + 1),
                                        _ => panic!("wrong locale-core rejection"),
                                    }
                                }
                            }
                            Failure::AccountingOverflow => panic!("wrong locale-core rejection"),
                        }
                    }
                    _ => panic!("first-over must fail its owning stage"),
                }
            }
        }
    }
    assert_eq!(edges, 32);
}

#[test]
fn invalid_entry_recipes_have_exact_declared_failure_kinds_not_any_generic_failure() {
    use crate::benchmark::operation::Output;
    use crate::materialize::InputFailure;
    for declaration in declarations()
        .into_iter()
        .filter(|case| case.limit.is_none() && case.expected_kind == ExpectedKind::EntryFailure)
    {
        let candidate = prepare(&declaration).unwrap();
        let Output::Entry(Err(error)) = candidate.output else {
            panic!("expected raw-input failure");
        };
        let expected = match declaration.fixture {
            Recipe::InvalidUtf8 => InputFailure::InvalidUtf8,
            Recipe::InvalidJson | Recipe::TrailingToken => InputFailure::Syntax,
            Recipe::DuplicateKey | Recipe::EscapedDuplicateKey => InputFailure::DuplicateMember,
            Recipe::InvalidSurrogate => InputFailure::NonScalarString,
            Recipe::NonPortableNumber => InputFailure::NonPortableNumber,
            _ => panic!("unexpected failing recipe"),
        };
        assert_eq!(error.reason, expected);
    }
}

#[test]
fn arbitrary_or_mutated_declarations_cannot_acquire_fixture_preparation() {
    let original = declarations().remove(0);
    let mut revision = original.clone();
    revision.fixture_revision = "unknown".into();
    let mut selector = original.clone();
    selector.selector = Selector::App;
    let mut limit = original;
    limit.limit = Some((LimitKind::Profiles, LimitEdge::Exact));
    for declaration in [revision, selector, limit] {
        assert!(matches!(
            prepare(&declaration),
            Err(PreparationFailure::UndeclaredCase)
        ));
    }
    let mut changed = declarations().remove(0);
    changed.expected_kind = ExpectedKind::Selected;
    assert!(matches!(
        prepare(&changed),
        Err(PreparationFailure::UndeclaredCase)
    ));
}

#[test]
fn every_declared_case_is_admitted_against_fixed_input_result_and_work_expectations() {
    let registry = super::registry::Registry::load().unwrap();
    let mut contexts = BTreeSet::new();
    for declaration in declarations() {
        let fixture = registry
            .prepare(&declaration)
            .unwrap_or_else(|error| panic!("{declaration:?}: {error:?}"));
        assert_eq!(fixture.declaration(), &declaration);
        assert!(contexts.insert(fixture.input_context()));
        let repeated = fixture
            .prepared()
            .once(&crate::benchmark::clock::tests::ScriptedClock::nanos([
                0, 1,
            ]))
            .unwrap()
            .output;
        assert_eq!(repeated.observe().unwrap(), fixture.expected());
        assert!(
            crate::benchmark::work::LogicalWork::observe(fixture.prepared(), &repeated)
                .unwrap()
                .matches_expected(fixture.expected_work())
        );
    }
    assert_eq!(contexts.len(), 127);
}
