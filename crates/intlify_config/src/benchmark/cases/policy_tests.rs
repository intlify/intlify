// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use super::policy_inputs::capacity;
use super::prepare::prepare;
use super::*;
use crate::benchmark::operation::{Output, Prepared};
use crate::benchmark::resource_policy::Resolved;
use crate::benchmark::work::LogicalWork;
use crate::input_limits::Bound;
use crate::resolver::evaluation::{Actual, Check, Code, Reason, Status, Unit};
use crate::resolver::limits::{BoundId, Group};
use crate::resolver::resource_policy::{unit, PolicyOutcome, PolicySubject};
use crate::structural::selection::StringLengthFact;

use ResourcePolicyRecipe as R;

fn case(recipe: ResourcePolicyRecipe) -> Declaration {
    declarations()
        .into_iter()
        .find(|case| case.fixture == Recipe::ResourcePolicy(recipe) && case.limit.is_none())
        .unwrap()
}

fn edge(kind: LimitKind, edge: LimitEdge) -> Declaration {
    declarations()
        .into_iter()
        .find(|case| case.limit == Some((kind, edge)))
        .unwrap()
}

fn resolved(declaration: &Declaration) -> Resolved {
    let Output::ResourcePolicy(resolved) = prepare(declaration).unwrap().output else {
        panic!("wrong owner boundary")
    };
    resolved
}

fn outcome(resolved: &Resolved) -> &PolicyOutcome {
    resolved.result.as_ref().unwrap()
}

fn issues(resolved: &Resolved) -> Vec<(Unit, Code, Option<Reason>)> {
    resolved
        .evaluation
        .issues()
        .map(|key| (key.unit().clone(), key.code(), key.reason()))
        .collect()
}

fn decision() -> Unit {
    unit(PolicySubject::ResourceLimits, Check::BootstrapCandidate)
}

fn artifact_bound(name: &'static str) -> BoundId {
    BoundId {
        group: Group::ArtifactAdmission,
        name,
    }
}

#[test]
fn policy_cases_keep_the_160_rows_and_append_after_them() {
    let cases = declarations();
    assert_eq!(cases.len(), 213);
    assert!(cases[..160]
        .iter()
        .all(|case| case.operation != Operation::ResourcePolicyBootstrap));
    assert!(cases[160..186]
        .iter()
        .all(|case| case.operation == Operation::ResourcePolicyBootstrap
            && case.selector == Selector::App));
    assert!(cases[160..180].iter().all(|case| case.limit.is_none()));
    assert!(cases[180..186].iter().all(|case| case.limit.is_some()));
    for declaration in &cases[160..186] {
        let candidate = prepare(declaration).unwrap();
        assert!(
            candidate.input_limits.is_some(),
            "complete config preparation is explicit"
        );
        let Output::ResourcePolicy(resolved) = candidate.output else {
            panic!("wrong owner boundary")
        };
        let outcome = outcome(&resolved);
        assert_eq!(
            outcome.confirmed.is_ok(),
            declaration.expected_kind == ExpectedKind::ResourcePolicyConfirmed,
            "{declaration:?}"
        );
        // A confirmed policy leaves no blocking result behind it.
        assert_eq!(
            resolved.evaluation.issues().next().is_none(),
            outcome.confirmed.is_ok(),
            "{declaration:?}"
        );
    }
}

#[test]
fn a_confirmed_case_confirms_the_policy_its_reference_names() {
    for recipe in ResourcePolicyRecipe::ALL
        .into_iter()
        .filter(|recipe| recipe.confirms())
    {
        let resolved = resolved(&case(recipe));
        let outcome = outcome(&resolved);
        let confirmed = outcome.confirmed.as_ref().unwrap();
        assert_eq!(
            confirmed.limits().bounds(),
            capacity().bounds(),
            "{recipe:?}"
        );
        // `app` within the policy's 256 bytes.
        assert_eq!(outcome.selector_length, Some(StringLengthFact::Exact(3)));
        assert!(resolved
            .evaluation
            .statuses()
            .all(|(_, status)| *status == Status::Evaluated));
    }
}

#[test]
fn each_decision_stage_reports_its_own_code_and_reason() {
    use Code::{
        ArtifactCollectionNotAdmitted, ArtifactReferenceNotAdmitted, ResourcePolicyNotAdmitted,
    };

    for (recipe, code, reason) in [
        (
            R::RoleKindMismatch,
            ArtifactReferenceNotAdmitted,
            Reason::RoleKindMismatch,
        ),
        (
            R::SpecificationRevisionUnsupported,
            ArtifactReferenceNotAdmitted,
            Reason::SpecificationRevisionUnsupported,
        ),
        (
            R::ConflictingContent,
            ArtifactCollectionNotAdmitted,
            Reason::ConflictingContent,
        ),
        (
            R::DuplicateEnvelope,
            ArtifactCollectionNotAdmitted,
            Reason::DuplicateEnvelope,
        ),
        (
            R::BodyIntegrityInvalid,
            ResourcePolicyNotAdmitted,
            Reason::BodyIntegrityInvalid,
        ),
        (
            R::BodySchemaInvalid,
            ResourcePolicyNotAdmitted,
            Reason::BodySchemaInvalid,
        ),
        (
            R::SectionMissing,
            ResourcePolicyNotAdmitted,
            Reason::ProjectProfileResolutionMissing,
        ),
        (
            R::SectionInvalid,
            ResourcePolicyNotAdmitted,
            Reason::ProjectProfileResolutionInvalid,
        ),
        (
            R::DigestMismatch,
            ArtifactReferenceNotAdmitted,
            Reason::DigestMismatch,
        ),
        (
            R::SpecificationRevisionMismatch,
            ArtifactReferenceNotAdmitted,
            Reason::SpecificationRevisionMismatch,
        ),
        (
            R::RevisionMismatch,
            ArtifactReferenceNotAdmitted,
            Reason::RevisionMismatch,
        ),
        (
            R::KindMismatch,
            ArtifactReferenceNotAdmitted,
            Reason::KindMismatch,
        ),
        (
            R::IdentityMismatch,
            ArtifactReferenceNotAdmitted,
            Reason::IdentityMismatch,
        ),
        (R::NotFound, ArtifactReferenceNotAdmitted, Reason::NotFound),
    ] {
        let resolved = resolved(&case(recipe));
        assert_eq!(
            issues(&resolved),
            vec![(decision(), code, Some(reason))],
            "{recipe:?}"
        );
        let outcome = outcome(&resolved);
        assert!(outcome.confirmed.is_err(), "{recipe:?}");
        // Without a compatible policy, the capability's 256 bytes stay the
        // projection authority.
        assert_eq!(outcome.selector_length, Some(StringLengthFact::Exact(3)));
        // Nothing past the decision runs: neither the comparison nor any
        // recheck under a policy.
        for (recorded, status) in resolved.evaluation.statuses() {
            if matches!(recorded.check, Check::CompatiblePolicy | Check::WithinBound) {
                assert!(
                    matches!(status, Status::NotEvaluated(_)),
                    "{recipe:?} {recorded:?}"
                );
            }
        }
    }
}

#[test]
fn a_policy_needing_more_than_the_capability_is_incompatible() {
    let resolved = resolved(&case(R::CapabilityIncompatible));
    let comparison = unit(PolicySubject::ResourceLimits, Check::CompatiblePolicy);
    let found = issues(&resolved);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, comparison);
    assert_eq!(found[0].1, Code::ResourcePolicyCapabilityIncompatible);
    let key = resolved.evaluation.issues().next().unwrap();
    let shortfall = resolved.evaluation.shortfall(key).unwrap();
    assert_eq!(shortfall.bound, artifact_bound("maxArtifacts"));
    assert_eq!(shortfall.required.get(), 1_001);
    assert_eq!(shortfall.available.get(), 1_000);
    assert_eq!(
        resolved.evaluation.status(&decision()),
        Some(&Status::Evaluated)
    );
    assert!(outcome(&resolved).confirmed.is_err());
}

#[test]
fn reordering_the_collection_changes_neither_domain_nor_the_work() {
    let mixed = prepare(&case(R::Mixed)).unwrap();
    let reordered = prepare(&case(R::MixedReordered)).unwrap();
    assert_eq!(mixed.observation, reordered.observation);
    assert_eq!(mixed.logical_work, reordered.logical_work);
    // The collection is the case's input, so its identity is not shared.
    assert_ne!(
        super::context::observe(&mixed).unwrap(),
        super::context::observe(&reordered).unwrap()
    );
}

#[test]
fn changed_decisions_change_the_shared_domain() {
    let mut seen = Vec::new();
    for recipe in ResourcePolicyRecipe::ALL
        .into_iter()
        .filter(|recipe| *recipe != R::MixedReordered)
    {
        let observation = prepare(&case(recipe)).unwrap().observation;
        // Equal shared results are allowed only between confirmed cases,
        // which decide the same policy over different collections.
        if !recipe.confirms() {
            assert!(
                seen.iter().all(|(_, shared)| *shared != observation.shared),
                "{recipe:?}"
            );
        }
        seen.push((recipe, observation.shared));
    }
}

#[test]
fn the_scales_count_occurrences_and_distinct_bodies_apart() {
    let work = |recipe| {
        let candidate = prepare(&case(recipe)).unwrap();
        serde_json::to_value(LogicalWork::observe(&candidate.prepared, &candidate.output).unwrap())
            .unwrap()
    };
    let fact = |work: &serde_json::Value, kind: &str| {
        work["facts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|fact| fact["kind"] == kind)
            .map(|fact| fact["observation"]["value"].as_str().unwrap().to_owned())
            .unwrap()
    };
    for (recipe, occurrences, policies, targets, bodies) in [
        (R::PolicyOnly, "1", "1", "0", "1"),
        (R::Mixed, "3", "2", "1", "3"),
        // The selected policy and 63 Target Profiles of their own.
        (R::ManyArtifacts, "64", "1", "63", "64"),
        // The selected policy and 32 copies of one Target Profile.
        (R::RepeatedBodies, "33", "1", "32", "2"),
        (R::DuplicateEnvelope, "2", "2", "0", "1"),
    ] {
        let work = work(recipe);
        assert_eq!(
            fact(&work, "resource-policy-artifact-occurrences"),
            occurrences,
            "{recipe:?}"
        );
        assert_eq!(
            fact(&work, "resource-policy-policy-artifacts"),
            policies,
            "{recipe:?}"
        );
        assert_eq!(
            fact(&work, "resource-policy-target-profile-artifacts"),
            targets,
            "{recipe:?}"
        );
        assert_eq!(
            fact(&work, "resource-policy-distinct-artifact-bodies"),
            bodies,
            "{recipe:?}"
        );
        // The five required `policies`, the explicit `null`s left out, and
        // the browser target's `profile`.
        assert_eq!(fact(&work, "resource-policy-references"), "6", "{recipe:?}");
    }
}

#[test]
fn each_bound_edge_holds_at_its_exact_value_and_blocks_one_below() {
    let artifacts = |check| unit(PolicySubject::Artifacts, check);
    for kind in [
        LimitKind::BootstrapArtifacts,
        LimitKind::RecheckArtifacts,
        LimitKind::RecheckProfileIdBytes,
    ] {
        let exact = resolved(&edge(kind, LimitEdge::Exact));
        assert!(outcome(&exact).confirmed.is_ok(), "{kind:?}");
        assert_eq!(
            outcome(&exact).selector_length,
            Some(StringLengthFact::Exact(3))
        );
        let over = resolved(&edge(kind, LimitEdge::FirstOver));
        let found = issues(&over)
            .into_iter()
            .map(|(unit, _, _)| unit)
            .collect::<Vec<_>>();
        let limit = |unit: &Unit| {
            let key = over
                .evaluation
                .issues()
                .find(|key| key.unit() == unit)
                .unwrap();
            let exceeded = over.evaluation.limit(key).unwrap();
            (exceeded.limit.get(), exceeded.actual)
        };
        match kind {
            // Under the capability, before any policy: the decision waits.
            LimitKind::BootstrapArtifacts => {
                let crossed = artifacts(Check::WithinCapability);
                assert_eq!(found, vec![crossed.clone()]);
                assert_eq!(limit(&crossed), (2, Actual::Exact(3)));
                assert!(matches!(
                    over.evaluation.status(&decision()),
                    Some(Status::NotEvaluated(_))
                ));
            }
            // Under the admitted policy, after the comparison held.
            LimitKind::RecheckArtifacts => {
                let crossed = artifacts(Check::WithinBound);
                assert_eq!(found, vec![crossed.clone()]);
                assert_eq!(limit(&crossed), (2, Actual::Exact(3)));
                assert_eq!(
                    over.evaluation.status(&unit(
                        PolicySubject::ResourceLimits,
                        Check::CompatiblePolicy
                    )),
                    Some(&Status::Evaluated)
                );
            }
            // `app` is both the only declared ID and the selector; the
            // selector keeps only the smallest first-over witness.
            _ => {
                let id = unit(PolicySubject::ProfileId(0), Check::WithinBound);
                let selector = unit(PolicySubject::Selector, Check::WithinBound);
                assert_eq!(found, vec![id.clone(), selector.clone()]);
                assert_eq!(limit(&id), (2, Actual::Exact(3)));
                assert_eq!(limit(&selector), (2, Actual::AtLeast(3)));
                assert_eq!(
                    outcome(&over).selector_length,
                    Some(StringLengthFact::OverLimit)
                );
            }
        }
        assert!(outcome(&over).confirmed.is_err(), "{kind:?}");
    }
}

#[test]
fn the_inputs_hand_phase_4_what_the_earlier_phases_measured() {
    use std::sync::Arc;

    use crate::materialize::materialize_file;

    let candidate = prepare(&case(R::Mixed)).unwrap();
    let Prepared::ResourcePolicy(prepared) = &candidate.prepared else {
        unreachable!()
    };
    let inputs = prepared.inputs().unwrap();
    // The configuration's counts, materialized again apart from preparation.
    let source = Recipe::ResourcePolicy(R::Mixed).source();
    let (limits, _) = super::prepare::fixture_capacity();
    let counts = materialize_file(Arc::clone(&source), limits)
        .unwrap()
        .counts();
    assert_eq!(inputs.measures.value, Ok(counts.value));
    assert!(counts.value.nodes > 1);
    // `app` is the only declared Profile ID.
    assert_eq!(inputs.measures.profile_ids, Ok(&[3_u64][..]));
    assert_eq!(
        inputs.measures.structural_units,
        Ok(prepared.analysis.structural_units().unwrap())
    );
    let declaration = prepared.config.profiles().get(&prepared.selected).unwrap();
    assert_eq!(
        inputs.resource_limits,
        &declaration.policies.resource_limits
    );
}

#[test]
fn the_input_context_refuses_a_candidate_bound_to_another_input() {
    use std::sync::Arc;

    use super::context::{observe, ContextFailure};
    use crate::benchmark::operation::Schema;
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
        let mut candidate = prepare(&case(R::PolicyOnly)).unwrap();
        let Prepared::ResourcePolicy(prepared) = &mut candidate.prepared else {
            unreachable!()
        };
        match change {
            0 => candidate.declaration.fixture = Recipe::Minimal,
            1 => candidate.declaration.selector = Selector::Absent,
            2 => prepared.selected = serde_json::from_value(serde_json::json!("other")).unwrap(),
            3 => {
                prepared.selector =
                    super::prepare::selector_input(Selector::Unknown, Bound::new(256).unwrap());
            }
            // The selected declaration's references cannot be borrowed from
            // an incomplete root: not from one without structural units, nor
            // from one whose analysis completed with an invalid sibling.
            4 => prepared.analysis = analyzed(b"{}"),
            _ => {
                let mut value = crate::profile_fixtures::minimal_config();
                value["profiles"]["broken"] = serde_json::json!({});
                let analysis = analyzed(&serde_json::to_vec(&value).unwrap());
                assert!(!analysis.is_complete());
                assert!(analysis.benchmark_structural_units().is_some());
                prepared.analysis = analysis;
            }
        }
        assert_eq!(
            observe(&candidate),
            Err(ContextFailure::PolicyInputMismatch),
            "change {change}"
        );
        // Phase 4 reads the selected declaration of a complete root only.
        assert_eq!(
            candidate.prepared.prerequisites_admitted(),
            change < 2 || change == 3,
            "change {change}"
        );
    }
}
