// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::collections::BTreeMap;

use super::prepare::prepare;
use super::*;
use crate::artifacts::envelope::{ExactTuple, FormalMember};
use crate::benchmark::operation::{Output, Prepared};
use crate::benchmark::resolved_binding::Resolved;
use crate::benchmark::work::LogicalWork;
use crate::resolver::artifact_resolution::{unit as collection_unit, CollectionSubject};
use crate::resolver::evaluation::{Check, Code, Reason, Status, Unit};
use crate::resolver::exact_resolution::unit;
use crate::resolver::resource_policy::{PolicySlot, ReferenceField};

use ResolvedBindingRecipe as R;

fn case(recipe: ResolvedBindingRecipe) -> Declaration {
    declarations()
        .into_iter()
        .find(|case| case.fixture == Recipe::ResolvedBinding(recipe))
        .unwrap()
}

fn resolved(recipe: ResolvedBindingRecipe) -> Resolved {
    let Output::ResolvedBinding(resolved) = prepare(&case(recipe)).unwrap().output else {
        panic!("wrong owner boundary")
    };
    resolved
}

fn issues(resolved: &Resolved) -> Vec<(Unit, Code, Option<Reason>)> {
    resolved
        .evaluation
        .issues()
        .map(|key| (key.unit().clone(), key.code(), key.reason()))
        .collect()
}

fn trust() -> Unit {
    unit(ReferenceField::Policy(PolicySlot::Trust))
}

fn tuple_of(value: &serde_json::Value) -> ExactTuple {
    FormalMember::new(value.clone())
        .unwrap()
        .envelope()
        .unwrap()
        .tuple()
        .clone()
}

/// Each present reference of a fixture's declaration as the tuple it names.
fn named(recipe: ResolvedBindingRecipe) -> Vec<ExactTuple> {
    let fixture = recipe.fixture();
    let declaration = &fixture.config["profiles"]["app"];
    declaration["policies"]
        .as_object()
        .unwrap()
        .values()
        .filter(|reference| !reference.is_null())
        .chain(
            declaration["targetProfiles"]
                .as_object()
                .unwrap()
                .values()
                .map(|target| &target["profile"]),
        )
        .map(|reference| {
            let wanted = |key: &str| reference[key].as_str().unwrap().to_owned();
            fixture
                .members
                .iter()
                .map(tuple_of)
                .find(|tuple| {
                    tuple.kind.spelling() == wanted("kind")
                        && tuple.identity.as_str() == wanted("identity")
                        && tuple.revision.as_str() == wanted("revision")
                        && tuple.semantic_digest.as_str() == wanted("semanticDigest")
                })
                .unwrap()
        })
        .collect()
}

#[test]
fn binding_cases_keep_the_198_rows_and_append_after_them() {
    let cases = declarations();
    assert_eq!(cases.len(), 252);
    assert!(cases[..198]
        .iter()
        .all(|case| case.operation != Operation::ResolvedBindingConstruction));
    assert!(cases[198..213].iter().all(|case| {
        case.operation == Operation::ResolvedBindingConstruction
            && case.selector == Selector::App
            && case.limit.is_none()
    }));
    for declaration in &cases[198..213] {
        // The case's source is the configuration, and the members are what
        // preparation read.
        let Recipe::ResolvedBinding(recipe) = declaration.fixture else {
            unreachable!()
        };
        let fixture = recipe.fixture();
        assert_eq!(
            *declaration.fixture.source(),
            *serde_json::to_vec(&fixture.config).unwrap(),
            "{declaration:?}"
        );
        let candidate = prepare(declaration).unwrap();
        let Prepared::ResolvedBinding(prepared) = &candidate.prepared else {
            panic!("wrong owner boundary")
        };
        assert_eq!(
            prepared
                .collection
                .members()
                .iter()
                .map(|member| member.value().clone())
                .collect::<Vec<_>>(),
            fixture.members,
            "{declaration:?}"
        );
        assert_eq!(prepared.selected.as_str(), "app");
        // The configuration is materialized under the file limits.
        assert!(candidate.input_limits.is_some());
        assert!(candidate.prepared.prerequisites_admitted());
        let Output::ResolvedBinding(resolved) = candidate.output else {
            panic!("wrong owner boundary")
        };
        let bound = resolved.result.as_ref().unwrap().is_ok();
        assert_eq!(
            bound,
            declaration.expected_kind == ExpectedKind::ResolvedBindingConstructed,
            "{declaration:?}"
        );
    }
}

/// Each present reference names the artifact the Set holds under its tuple,
/// and each distinct tuple binds the body of the member that establishes it.
#[test]
fn a_complete_resolution_binds_each_distinct_reference_once() {
    for recipe in R::ALL.into_iter().filter(|recipe| recipe.binds()) {
        let named = named(recipe);
        let bodies = recipe
            .fixture()
            .members
            .iter()
            .map(|value| {
                let member = FormalMember::new(value.clone()).unwrap();
                (tuple_of(value), member.canonical_body().to_vec())
            })
            .collect::<BTreeMap<_, _>>();
        let expected = named
            .iter()
            .map(|tuple| (tuple.clone(), bodies[tuple].clone()))
            .collect::<BTreeMap<_, _>>();
        let resolved = resolved(recipe);
        assert!(resolved.evaluation.issues().next().is_none(), "{recipe:?}");
        let retained = resolved.result.unwrap().unwrap();
        let mut references = retained
            .references
            .iter()
            .map(|(_, tuple)| tuple.clone())
            .collect::<Vec<_>>();
        let mut declared = named.clone();
        references.sort();
        declared.sort();
        assert_eq!(references, declared, "{recipe:?}");
        assert_eq!(
            retained.bindings,
            expected.into_iter().collect::<Vec<_>>(),
            "{recipe:?}"
        );
    }
}

/// Each blocked recipe reaches one decision at the `trust` reference while
/// every other reference resolves, or blocks each target's reference, or
/// leaves every reference waiting on the collection.
#[test]
fn each_blocked_recipe_reports_its_one_decision() {
    use Code::ArtifactReferenceNotAdmitted as Reference;

    for (recipe, reason) in [
        (R::RoleKindMismatch, Reason::RoleKindMismatch),
        (
            R::SpecificationRevisionUnsupported,
            Reason::SpecificationRevisionUnsupported,
        ),
        (R::DigestMismatch, Reason::DigestMismatch),
        (R::RevisionMismatch, Reason::RevisionMismatch),
        (R::KindMismatch, Reason::KindMismatch),
        (R::IdentityMismatch, Reason::IdentityMismatch),
        (R::NotFound, Reason::NotFound),
    ] {
        let resolved = resolved(recipe);
        assert_eq!(
            issues(&resolved),
            [(trust(), Reference, Some(reason))],
            "{recipe:?}"
        );
        assert_eq!(resolved.evaluation.statuses().count(), 6, "{recipe:?}");
        assert!(
            resolved
                .evaluation
                .statuses()
                .all(|(_, status)| *status == Status::Evaluated),
            "{recipe:?}"
        );
    }
    // No Target Profile is submitted, so each target's reference finds
    // nothing of its kind or identity.
    let many = resolved(R::ManyNotFound);
    let found = issues(&many);
    assert_eq!(found.len(), 63);
    assert!(found.iter().all(|(recorded, code, reason)| {
        matches!(
            recorded.subject,
            crate::resolver::evaluation::Subject::ArtifactReference(ReferenceField::TargetProfile(
                _
            ))
        ) && *code == Reference
            && *reason == Some(Reason::NotFound)
    }));
    // The second trust policy blocks the collection: every reference waits
    // on that one duplicate, and none is checked.
    let unset = resolved(R::CollectionNotAdmitted);
    assert!(unset.evaluation.issues().next().is_none());
    let causes = unset.result.unwrap().unwrap_err();
    assert_eq!(
        causes
            .keys()
            .map(|key| (key.unit().clone(), key.code(), key.reason()))
            .collect::<Vec<_>>(),
        // The trust policy's kind sorts last, after the Target Profile's.
        [(
            collection_unit(CollectionSubject::Member(5), Check::SingleOccurrence),
            Code::ArtifactCollectionNotAdmitted,
            Some(Reason::DuplicateEnvelope)
        )]
    );
    assert_eq!(unset.evaluation.statuses().count(), 6);
    assert!(unset
        .evaluation
        .statuses()
        .all(|(_, status)| *status == Status::NotEvaluated(causes.clone())));
}

#[test]
fn reordering_changes_neither_domain_nor_the_work() {
    let every = prepare(&case(R::EveryReference)).unwrap();
    let reordered = prepare(&case(R::EveryReferenceReordered)).unwrap();
    assert_eq!(every.observation, reordered.observation);
    assert_eq!(every.logical_work, reordered.logical_work);
    // The configuration and the collection are the case's input, so its
    // identity is not shared.
    assert_ne!(
        super::context::observe(&every).unwrap(),
        super::context::observe(&reordered).unwrap()
    );
}

/// The bindings leave out what no reference names, so artifacts only the Set
/// holds change the work but not the result.
#[test]
fn unreferenced_artifacts_change_the_work_but_not_the_result() {
    let required = prepare(&case(R::RequiredOnly)).unwrap();
    let unreferenced = prepare(&case(R::UnreferencedArtifacts)).unwrap();
    assert_eq!(required.observation, unreferenced.observation);
    assert_ne!(required.logical_work, unreferenced.logical_work);
}

#[test]
fn every_other_case_has_its_own_shared_domain() {
    let mut seen = Vec::new();
    for recipe in R::ALL.into_iter().filter(|recipe| {
        !matches!(
            recipe,
            R::EveryReferenceReordered | R::UnreferencedArtifacts
        )
    }) {
        let observation = prepare(&case(recipe)).unwrap().observation;
        assert!(observation.entry.is_none(), "{recipe:?}");
        assert!(
            seen.iter().all(|shared| *shared != observation.shared),
            "{recipe:?}"
        );
        seen.push(observation.shared);
    }
}

#[test]
fn the_scales_count_references_bindings_and_the_set_apart() {
    let work = |recipe| {
        let candidate = prepare(&case(recipe)).unwrap();
        serde_json::to_value(LogicalWork::observe(&candidate.prepared, &candidate.output).unwrap())
            .unwrap()
    };
    let fact = |work: &serde_json::Value, kind: &str| {
        let found = work["facts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|fact| fact["kind"] == kind)
            .unwrap();
        found["observation"]["value"]
            .as_str()
            .map_or_else(|| found["observation"]["state"].to_string(), str::to_owned)
    };
    let unavailable = "\"unavailable\"";
    for (recipe, [references, distinct, set, units, waiting, issues, bindings]) in [
        (R::RequiredOnly, ["6", "6", "6", "6", "0", "0", "6"]),
        (R::EveryReference, ["9", "9", "9", "9", "0", "0", "9"]),
        (R::ManyTargets, ["68", "68", "68", "68", "0", "0", "68"]),
        // Every target names one Target Profile: one binding for them all.
        (
            R::SharedTargetProfile,
            ["68", "6", "6", "68", "0", "0", "6"],
        ),
        // The Set holds what no reference names, and nothing binds it.
        (
            R::UnreferencedArtifacts,
            ["6", "6", "69", "6", "0", "0", "6"],
        ),
        // Another role's artifact is another distinct reference.
        (
            R::RoleKindMismatch,
            ["6", "5", "6", "6", "0", "1", unavailable],
        ),
        (R::NotFound, ["6", "6", "5", "6", "0", "1", unavailable]),
        (
            R::ManyNotFound,
            ["68", "68", "5", "68", "0", "63", unavailable],
        ),
        (
            R::CollectionNotAdmitted,
            ["6", "6", unavailable, "6", "6", "0", unavailable],
        ),
    ] {
        let work = work(recipe);
        for (kind, expected) in [
            ("resolved-binding-references", references),
            ("resolved-binding-distinct-references", distinct),
            ("resolved-binding-set-artifacts", set),
            ("resolved-binding-evaluation-units", units),
            ("resolved-binding-not-evaluated-units", waiting),
            ("resolved-binding-blocking-issues", issues),
            ("resolved-binding-bindings", bindings),
        ] {
            assert_eq!(fact(&work, kind), expected, "{recipe:?} {kind}");
        }
    }
}

/// Phase 4 reaches only the policy the configuration selects and counts the
/// references under its bounds, so it confirms every case here: Phase 9 alone
/// finds what the other references and members break.
#[test]
fn phase_4_confirms_every_declaration_phase_9_resolves() {
    use crate::benchmark::resource_policy::resolve;

    let declaration = declarations()
        .into_iter()
        .find(|case| {
            case.fixture == Recipe::ResourcePolicy(ResourcePolicyRecipe::PolicyOnly)
                && case.limit.is_none()
        })
        .unwrap();
    for recipe in R::ALL {
        let Prepared::ResourcePolicy(mut policy) = prepare(&declaration).unwrap().prepared else {
            unreachable!()
        };
        let Prepared::ResolvedBinding(bindings) = prepare(&case(recipe)).unwrap().prepared else {
            unreachable!()
        };
        let bindings = *bindings;
        policy.analysis = bindings.analysis;
        policy.config = bindings.config;
        policy.collection = bindings.collection;
        let inputs = policy.inputs().unwrap();
        let phase_4 = resolve(&policy.capability, &policy.collection, &inputs);
        let confirmed = phase_4.result.unwrap().confirmed.unwrap();
        // The policy Phase 9 runs under is the one Phase 4 confirms.
        assert_eq!(
            confirmed.limits().bounds(),
            bindings.policy.limits().bounds(),
            "{recipe:?}"
        );
        assert!(phase_4.evaluation.issues().next().is_none(), "{recipe:?}");
    }
}

#[test]
fn the_input_context_refuses_a_candidate_bound_to_another_input() {
    use std::sync::Arc;

    use super::context::{observe, ContextFailure};
    use crate::benchmark::operation::Schema;
    use crate::input_limits::Bound;
    use crate::materialize::materialize_file;
    use crate::structural::StructuralLimits;

    let analyzed = |source: &[u8]| {
        let doc = materialize_file(Arc::from(source), crate::materialize_tests::limits()).unwrap();
        let limits = StructuralLimits {
            max_profiles: Bound::new(64).unwrap(),
            max_profile_id_bytes: Bound::new(256).unwrap(),
            max_structural_analysis_units: Bound::new(1_000_000).unwrap(),
        };
        Schema::for_model()
            .unwrap()
            .analyze(Arc::new(doc), limits)
            .unwrap()
    };
    for change in 0..6 {
        let mut candidate = prepare(&case(R::RequiredOnly)).unwrap();
        assert!(observe(&candidate).is_ok());
        let Prepared::ResolvedBinding(prepared) = &mut candidate.prepared else {
            unreachable!()
        };
        let expected = match change {
            0 => {
                candidate.declaration.fixture = Recipe::Minimal;
                ContextFailure::BindingInputMismatch
            }
            1 => {
                candidate.declaration.selector = Selector::Absent;
                ContextFailure::BindingInputMismatch
            }
            2 => {
                prepared.selected = serde_json::from_value(serde_json::json!("other")).unwrap();
                ContextFailure::BindingInputMismatch
            }
            // The selected declaration's references cannot be borrowed from
            // an incomplete root: not from one without structural units, nor
            // from one whose analysis completed with an invalid sibling.
            3 => {
                prepared.analysis = analyzed(b"{}");
                ContextFailure::BindingInputMismatch
            }
            4 => {
                let mut value = crate::profile_fixtures::minimal_config();
                value["profiles"]["broken"] = serde_json::json!({});
                let analysis = analyzed(&serde_json::to_vec(&value).unwrap());
                assert!(!analysis.is_complete());
                assert!(analysis.benchmark_structural_units().is_some());
                prepared.analysis = analysis;
                ContextFailure::BindingInputMismatch
            }
            // The configuration is materialized under the file limits.
            _ => {
                candidate.input_limits = None;
                ContextFailure::PreparationLimitsMismatch
            }
        };
        assert_eq!(observe(&candidate), Err(expected), "change {change}");
        // Phase 9 resolves the selected declaration of a complete root only.
        assert_eq!(
            candidate.prepared.prerequisites_admitted(),
            !(2..5).contains(&change),
            "change {change}"
        );
    }
}
