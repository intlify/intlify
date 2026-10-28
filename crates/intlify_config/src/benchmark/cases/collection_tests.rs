// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::collections::BTreeMap;

use super::prepare::prepare;
use super::*;
use crate::artifacts::envelope::{ExactTuple, FormalMember};
use crate::benchmark::artifact_collection::Resolved;
use crate::benchmark::operation::{Output, Prepared};
use crate::benchmark::work::LogicalWork;
use crate::resolver::artifact_resolution::{unit, CollectionSubject};
use crate::resolver::evaluation::{Check, Code, Reason, Status, Unit};

use ArtifactCollectionRecipe as R;

fn case(recipe: ArtifactCollectionRecipe) -> Declaration {
    declarations()
        .into_iter()
        .find(|case| case.fixture == Recipe::ArtifactCollection(recipe))
        .unwrap()
}

fn resolved(recipe: ArtifactCollectionRecipe) -> Resolved {
    let Output::ArtifactCollection(resolved) = prepare(&case(recipe)).unwrap().output else {
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

fn member(rank: usize, check: Check) -> Unit {
    unit(CollectionSubject::Member(rank), check)
}

fn group(rank: usize) -> Unit {
    unit(CollectionSubject::Group(rank), Check::ConsistentContent)
}

#[test]
fn collection_cases_keep_the_186_rows_and_append_after_them() {
    let cases = declarations();
    assert_eq!(cases.len(), 213);
    assert!(cases[..186]
        .iter()
        .all(|case| case.operation != Operation::ArtifactCollectionAdmission));
    assert!(cases[186..198].iter().all(|case| {
        case.operation == Operation::ArtifactCollectionAdmission
            && case.selector == Selector::Absent
            && case.limit.is_none()
    }));
    for declaration in &cases[186..198] {
        // The case's source is its members in submission order, and they
        // are what preparation read.
        let Recipe::ArtifactCollection(recipe) = declaration.fixture else {
            unreachable!()
        };
        let source: Vec<serde_json::Value> =
            serde_json::from_slice(&declaration.fixture.source()).unwrap();
        assert_eq!(source, recipe.members(), "{declaration:?}");
        let candidate = prepare(declaration).unwrap();
        let Prepared::ArtifactCollection(prepared) = &candidate.prepared else {
            panic!("wrong owner boundary")
        };
        assert_eq!(
            prepared
                .collection
                .members()
                .iter()
                .map(|member| member.value().clone())
                .collect::<Vec<_>>(),
            source,
            "{declaration:?}"
        );
        assert!(
            candidate.input_limits.is_none(),
            "no configuration is materialized"
        );
        assert!(candidate.prepared.prerequisites_admitted());
        let Output::ArtifactCollection(resolved) = candidate.output else {
            panic!("wrong owner boundary")
        };
        let admitted = resolved.result.as_ref().unwrap().is_ok();
        assert_eq!(
            admitted,
            declaration.expected_kind == ExpectedKind::ArtifactCollectionAdmitted,
            "{declaration:?}"
        );
        // A Set exists exactly when nothing blocks the collection, and every
        // unit was evaluated either way.
        assert_eq!(
            resolved.evaluation.issues().next().is_none(),
            admitted,
            "{declaration:?}"
        );
        assert!(resolved
            .evaluation
            .statuses()
            .all(|(_, status)| *status == Status::Evaluated));
    }
}

/// The Set holds each distinct tuple once, in the tuples' order, with the
/// body of the member that establishes it.
#[test]
fn an_admitted_collection_is_the_set_of_its_distinct_tuples() {
    for recipe in R::ALL.into_iter().filter(|recipe| recipe.admits()) {
        let mut expected = BTreeMap::<ExactTuple, Vec<u8>>::new();
        for value in recipe.members() {
            let member = FormalMember::new(value).unwrap();
            let tuple = member.envelope().unwrap().tuple().clone();
            expected.insert(tuple, member.canonical_body().to_vec());
        }
        let resolved = resolved(recipe);
        let entries = resolved.result.unwrap().unwrap();
        assert_eq!(
            entries
                .into_iter()
                .map(|entry| (entry.tuple, entry.body))
                .collect::<Vec<_>>(),
            expected.into_iter().collect::<Vec<_>>(),
            "{recipe:?}"
        );
    }
}

/// Each blocked recipe adds one failure, found at its own unit whether or
/// not a reference reaches the member. Distinct bodies order by their tuple
/// before the members that establish none, and groups by their tuple.
#[test]
fn each_blocked_recipe_reports_its_one_kind_of_failure() {
    use Code::ArtifactCollectionNotAdmitted as Collection;

    let conflicts = (0..32)
        .map(|rank| (group(rank), Collection, Some(Reason::ConflictingContent)))
        .collect::<Vec<_>>();
    for (recipe, expected) in [
        // The approval policy's kind sorts first.
        (
            R::UnreferencedDuplicate,
            vec![(
                member(0, Check::SingleOccurrence),
                Collection,
                Some(Reason::DuplicateEnvelope),
            )],
        ),
        // The policy, then the Target Profile submitted 32 times.
        (
            R::RepeatedBodies,
            vec![(
                member(1, Check::SingleOccurrence),
                Collection,
                Some(Reason::DuplicateEnvelope),
            )],
        ),
        (
            R::ArtifactConflict,
            vec![(group(0), Collection, Some(Reason::ConflictingContent))],
        ),
        // Every approval group, but not the policy's.
        (R::ManyConflicts, conflicts),
        // The approval policy, the policy, and then `storefront-server`
        // before `storefront-web`.
        (
            R::EnvelopeInvalid,
            vec![(
                member(2, Check::AdmissibleMember),
                Collection,
                Some(Reason::EnvelopeInvalid),
            )],
        ),
        // A member establishing no tuple follows the three that do.
        (
            R::NoTuple,
            vec![(
                member(3, Check::AdmissibleMember),
                Collection,
                Some(Reason::EnvelopeInvalid),
            )],
        ),
        (
            R::KindUnsupported,
            vec![(
                member(3, Check::AdmissibleMember),
                Collection,
                Some(Reason::KindUnsupported),
            )],
        ),
    ] {
        let resolved = resolved(recipe);
        assert_eq!(issues(&resolved), expected, "{recipe:?}");
        // No Set, and exactly those failures are why.
        let causes = resolved.result.unwrap().unwrap_err();
        assert_eq!(
            causes
                .keys()
                .map(|key| (key.unit().clone(), key.code(), key.reason()))
                .collect::<Vec<_>>(),
            expected,
            "{recipe:?}"
        );
    }
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
fn every_other_collection_has_its_own_shared_domain() {
    let mut seen = Vec::new();
    for recipe in R::ALL
        .into_iter()
        .filter(|recipe| *recipe != R::MixedReordered)
    {
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
fn the_scales_count_occurrences_bodies_and_units_apart() {
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
    for (recipe, [occurrences, policies, targets, bodies, units, issues, set]) in [
        (R::PolicyOnly, ["1", "1", "0", "1", "3", "0", "1"]),
        (R::Mixed, ["3", "2", "1", "3", "9", "0", "3"]),
        // A Glossary Set counts with the policies, as `maxPolicyArtifacts`
        // counts it.
        (R::EveryKind, ["8", "7", "1", "8", "24", "0", "8"]),
        (R::ManyArtifacts, ["64", "1", "63", "64", "192", "0", "64"]),
        // Two occurrences of one body: one unit of each check.
        (
            R::UnreferencedDuplicate,
            ["4", "3", "1", "3", "9", "1", unavailable],
        ),
        (
            R::RepeatedBodies,
            ["33", "1", "32", "2", "6", "1", unavailable],
        ),
        // Two bodies in one group.
        (
            R::ArtifactConflict,
            ["4", "3", "1", "4", "11", "1", unavailable],
        ),
        (
            R::ManyConflicts,
            ["65", "65", "0", "65", "163", "32", unavailable],
        ),
        (
            R::EnvelopeInvalid,
            ["4", "2", "2", "4", "12", "1", unavailable],
        ),
        // A member without a tuple has no occurrence or group unit, but its
        // declared kind still counts.
        (R::NoTuple, ["4", "2", "2", "4", "10", "1", unavailable]),
        // An unadmitted kind counts as neither.
        (
            R::KindUnsupported,
            ["4", "2", "1", "4", "10", "1", unavailable],
        ),
    ] {
        let work = work(recipe);
        for (kind, expected) in [
            ("artifact-collection-artifact-occurrences", occurrences),
            ("artifact-collection-policy-artifacts", policies),
            ("artifact-collection-target-profile-artifacts", targets),
            ("artifact-collection-distinct-artifact-bodies", bodies),
            ("artifact-collection-evaluation-units", units),
            ("artifact-collection-blocking-issues", issues),
            ("artifact-collection-set-artifacts", set),
        ] {
            assert_eq!(fact(&work, kind), expected, "{recipe:?} {kind}");
        }
    }
}

/// Phase 4 reaches only the policy the configuration selects, so it confirms
/// every collection here: Phase 9 alone finds what the other members break.
#[test]
fn phase_4_confirms_every_collection_phase_9_checks() {
    use crate::benchmark::resource_policy::resolve;

    let declaration = declarations()
        .into_iter()
        .find(|case| {
            case.fixture == Recipe::ResourcePolicy(ResourcePolicyRecipe::PolicyOnly)
                && case.limit.is_none()
        })
        .unwrap();
    let Prepared::ResourcePolicy(mut policy) = prepare(&declaration).unwrap().prepared else {
        unreachable!()
    };
    for recipe in R::ALL {
        let Prepared::ArtifactCollection(collection) = prepare(&case(recipe)).unwrap().prepared
        else {
            unreachable!()
        };
        policy.collection = collection.collection.clone();
        let inputs = policy.inputs().unwrap();
        let phase_4 = resolve(&policy.capability, &policy.collection, &inputs);
        let confirmed = phase_4.result.unwrap().confirmed.unwrap();
        // The policy Phase 9 runs under is the one Phase 4 confirms.
        assert_eq!(
            confirmed.limits().bounds(),
            collection.policy.limits().bounds(),
            "{recipe:?}"
        );
        assert!(phase_4.evaluation.issues().next().is_none(), "{recipe:?}");
    }
}

#[test]
fn the_input_context_refuses_a_candidate_bound_to_another_input() {
    use super::context::{observe, ContextFailure};

    for change in 0..3 {
        let mut candidate = prepare(&case(R::PolicyOnly)).unwrap();
        assert!(observe(&candidate).is_ok());
        let expected = match change {
            0 => {
                candidate.declaration.fixture = Recipe::Minimal;
                ContextFailure::CollectionInputMismatch
            }
            1 => {
                candidate.declaration.selector = Selector::App;
                ContextFailure::CollectionInputMismatch
            }
            // No configuration is read, so no file limits are either.
            _ => {
                candidate.input_limits = Some(super::prepare::fixture_capacity().0);
                ContextFailure::PreparationLimitsMismatch
            }
        };
        assert_eq!(observe(&candidate), Err(expected), "change {change}");
    }
}
