// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use serde_json::{json, Value};

use super::prepare::prepare;
use super::*;
use crate::benchmark::hydration_relation::Resolved;
use crate::benchmark::operation::{Output, Prepared};
use crate::benchmark::work::LogicalWork;
use crate::model::{DeploymentGroupId, TargetId};
use crate::resolver::evaluation::{Actual, Check, Code, Reason, Status, Subject, Unit};
use crate::resolver::hydration::{unit as hydration_unit, HydrationSubject};
use crate::resolver::limits::{BoundId, Group};

use HydrationRecipe as R;

fn case(recipe: HydrationRecipe) -> Declaration {
    declarations()
        .into_iter()
        .find(|case| case.fixture == Recipe::Hydration(recipe) && case.limit.is_none())
        .unwrap()
}

fn edge(limit: LimitKind, edge: LimitEdge) -> Declaration {
    declarations()
        .into_iter()
        .find(|case| case.limit == Some((limit, edge)))
        .unwrap()
}

fn resolved(declaration: &Declaration) -> Resolved {
    let Output::Hydration(resolved) = prepare(declaration).unwrap().output else {
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

fn target(id: &str) -> TargetId {
    serde_json::from_value(json!(id)).unwrap()
}

fn group(id: &str) -> DeploymentGroupId {
    serde_json::from_value(json!(id)).unwrap()
}

fn relation(server: &str, client: &str, check: Check) -> Unit {
    hydration_unit(
        HydrationSubject::Relation(group("web"), target(server), target(client)),
        check,
    )
}

fn roles(id: &str) -> Unit {
    hydration_unit(
        HydrationSubject::Roles(group("web"), target(id)),
        Check::SingleRole,
    )
}

/// The units waiting on something, and the subjects of what they wait on.
fn waiting(resolved: &Resolved) -> Vec<(Unit, Vec<Subject>)> {
    resolved
        .evaluation
        .statuses()
        .filter_map(|(recorded, status)| match status {
            Status::NotEvaluated(causes) => Some((
                recorded.clone(),
                causes
                    .keys()
                    .map(|key| key.unit().subject.clone())
                    .collect(),
            )),
            Status::Evaluated => None,
        })
        .collect()
}

#[test]
fn hydration_cases_keep_the_280_rows_and_append_after_them() {
    let cases = declarations();
    assert_eq!(cases.len(), 303);
    assert!(cases[..280]
        .iter()
        .all(|case| case.operation != Operation::HydrationRelationValidation));
    assert!(cases[280..].iter().all(|case| {
        case.operation == Operation::HydrationRelationValidation && case.selector == Selector::App
    }));
    // Every recipe once, in its order, and then the four edges.
    assert_eq!(
        cases[280..299]
            .iter()
            .map(|case| (case.fixture, case.limit))
            .collect::<Vec<_>>(),
        R::ALL
            .into_iter()
            .map(|recipe| (Recipe::Hydration(recipe), None))
            .collect::<Vec<_>>()
    );
    assert!(cases[299..].iter().all(|case| case.limit.is_some()));
    for declaration in &cases[280..] {
        // The case's source is the configuration, the members are what
        // preparation read, and the answers are the recipe's.
        let Recipe::Hydration(recipe) = declaration.fixture else {
            unreachable!()
        };
        let fixture = recipe.fixture();
        assert_eq!(
            *declaration.fixture.source(),
            *serde_json::to_vec(&fixture.config).unwrap(),
            "{declaration:?}"
        );
        let candidate = prepare(declaration).unwrap();
        let Prepared::Hydration(prepared) = &candidate.prepared else {
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
        assert_eq!(prepared.capabilities, recipe.capabilities());
        assert_eq!(prepared.selected.as_str(), "app");
        assert!(candidate.input_limits.is_some());
        assert!(candidate.prepared.prerequisites_admitted());
        let Output::Hydration(resolved) = candidate.output else {
            panic!("wrong owner boundary")
        };
        let retained = resolved.result.unwrap().unwrap();
        let complete = retained.negotiation.is_ok()
            && retained.groups.iter().all(|group| group.relations.is_ok());
        assert_eq!(
            complete,
            declaration.expected_kind == ExpectedKind::HydrationRelationValidationComplete,
            "{declaration:?}"
        );
    }
}

/// A complete case checks every unit and keeps each declared pair once.
#[test]
fn a_complete_case_keeps_every_declared_relation() {
    for recipe in R::ALL.into_iter().filter(|recipe| recipe.completes()) {
        let resolved = resolved(&case(recipe));
        assert!(resolved.evaluation.issues().next().is_none(), "{recipe:?}");
        assert!(
            resolved
                .evaluation
                .statuses()
                .all(|(_, status)| *status == Status::Evaluated),
            "{recipe:?}"
        );
        let retained = resolved.result.unwrap().unwrap();
        assert!(retained.negotiation.is_ok(), "{recipe:?}");
        let fixture = recipe.fixture();
        let declared =
            &fixture.config["profiles"]["app"]["deploymentGroups"]["web"]["hydrationRelations"];
        let mut pairs = declared
            .as_array()
            .map(|relations| {
                relations
                    .iter()
                    .map(|relation| {
                        (
                            relation["server"].as_str().unwrap().to_owned(),
                            relation["client"].as_str().unwrap().to_owned(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        pairs.sort();
        pairs.dedup();
        assert_eq!(retained.groups.len(), 1, "{recipe:?}");
        assert_eq!(retained.groups[0].relations, Ok(pairs), "{recipe:?}");
    }
}

/// Each change of the example reaches the one decision its name says.
#[test]
fn each_blocked_recipe_reports_its_one_decision() {
    use Code::{
        HydrationCapabilityIncompatible as Capability, HydrationLocalePolicyIncompatible as Locale,
        HydrationRelationInvalid as Invalid,
    };

    for (recipe, expected) in [
        (
            R::EqualEndpoint,
            (
                relation("ssr", "ssr", Check::DistinctEndpoints),
                Invalid,
                Some(Reason::EqualEndpoint),
            ),
        ),
        (
            R::UnknownEndpoint,
            (
                relation("ssr", "phone", Check::KnownEndpoints),
                Invalid,
                Some(Reason::UnknownEndpoint),
            ),
        ),
        (
            R::CrossGroup,
            (
                relation("ssr", "worker", Check::SameGroupEndpoints),
                Invalid,
                Some(Reason::CrossGroup),
            ),
        ),
        (
            R::DuplicateRelation,
            (
                relation("ssr", "browser", Check::UniqueRelation),
                Invalid,
                Some(Reason::DuplicateRelation),
            ),
        ),
        (
            R::ServerCapabilityMissing,
            (
                relation("ssr", "browser", Check::ServerRole),
                Invalid,
                Some(Reason::ServerCapabilityMissing),
            ),
        ),
        (
            R::ClientCapabilityMissing,
            (
                relation("ssr", "browser", Check::ClientRole),
                Invalid,
                Some(Reason::ClientCapabilityMissing),
            ),
        ),
        (
            R::DualRole,
            (roles("browser"), Invalid, Some(Reason::DualRole)),
        ),
        (
            R::RequestedLocales,
            (
                relation("ssr", "browser", Check::SameRequestedLocales),
                Locale,
                Some(Reason::RequestedLocales),
            ),
        ),
        (
            R::EffectiveDefault,
            (
                relation("ssr", "browser", Check::SameEffectiveDefault),
                Locale,
                Some(Reason::EffectiveDefault),
            ),
        ),
        (
            R::StaticIncompatibility,
            (
                relation("ssr", "browser", Check::StaticCompatibility),
                Capability,
                None,
            ),
        ),
    ] {
        let resolved = resolved(&case(recipe));
        assert_eq!(issues(&resolved), [expected], "{recipe:?}");
        // Only `web` declares relations, and it has none checked.
        let retained = resolved.result.unwrap().unwrap();
        for group in &retained.groups {
            assert_eq!(group.relations.is_err(), group.id == "web", "{recipe:?}");
        }
    }
}

/// What Phases 9, 11, or 12 found before the interval leaves only the checks
/// that compare it waiting, without a result of Phase 13.
#[test]
fn a_prerequisite_failure_leaves_only_what_waits_on_it() {
    let checks = |recorded: &[(Unit, Vec<Subject>)]| {
        recorded
            .iter()
            .map(|(unit, _)| unit.check)
            .collect::<Vec<_>>()
    };
    // Phase 11 finds `de` outside the project: only the requested locales
    // wait, on that target.
    let unresolved = resolved(&case(R::EndpointLocalesUnresolved));
    assert!(unresolved.evaluation.issues().next().is_none());
    let found = waiting(&unresolved);
    assert_eq!(checks(&found), [Check::SameRequestedLocales]);
    assert!(found[0]
        .1
        .iter()
        .all(|subject| matches!(subject, Subject::Target(_))));
    // Phase 12 finds `tablet` names no target: the group's members wait, and
    // every comparison between endpoints with them.
    let members = resolved(&case(R::GroupMembersUnresolved));
    assert!(members.evaluation.issues().next().is_none());
    let found = waiting(&members);
    assert_eq!(
        checks(&found),
        [
            Check::SameGroupEndpoints,
            Check::ServerRole,
            Check::ClientRole,
            Check::SameRequestedLocales,
            Check::SameEffectiveDefault,
            Check::StaticCompatibility,
        ]
    );
    assert!(found
        .iter()
        .flat_map(|(_, subjects)| subjects)
        .all(|subject| matches!(subject, Subject::Group(_))));
    // Phase 9 resolves no Target Profile for `ssr`: the roles and the
    // compatibility wait, and the locales are still compared.
    let missing = resolved(&case(R::TargetProfileMissing));
    assert!(missing.evaluation.issues().next().is_none());
    let found = waiting(&missing);
    assert_eq!(
        checks(&found),
        [
            Check::ServerRole,
            Check::ClientRole,
            Check::StaticCompatibility
        ]
    );
    assert!(found
        .iter()
        .flat_map(|(_, subjects)| subjects)
        .all(|subject| matches!(subject, Subject::ArtifactReference(_))));
    // Phase 8 admits no Locale Negotiation Profile: every relation is still
    // checked, and only the shared dependency is unavailable.
    let unshared = resolved(&case(R::NegotiationUnavailable));
    assert!(unshared.evaluation.issues().next().is_none());
    assert!(waiting(&unshared).is_empty());
    let retained = unshared.result.unwrap().unwrap();
    assert!(retained.groups.iter().all(|group| group.relations.is_ok()));
    let causes = retained.negotiation.unwrap_err();
    assert!(causes
        .keys()
        .all(|key| matches!(key.unit().subject, Subject::Locale(_))));
}

/// Each bound holds at its exact count, and one less blocks only its unit,
/// with the bound, its value, and the total.
#[test]
fn each_bound_holds_at_its_count_and_one_less_blocks_its_unit() {
    let bound = |name| BoundId {
        group: Group::TargetGrouping,
        name,
    };
    for (limit, recipe, name, count, unit) in [
        (
            LimitKind::HydrationRelations,
            R::FanOutFanIn,
            "maxHydrationRelations",
            3,
            hydration_unit(HydrationSubject::Relations, Check::WithinBound),
        ),
        (
            LimitKind::HydrationEndpointBytes,
            R::Storefront,
            "maxTargetIdBytes",
            7,
            relation("ssr", "browser", Check::WithinBound),
        ),
    ] {
        let exact = edge(limit, LimitEdge::Exact);
        assert_eq!(exact.fixture, Recipe::Hydration(recipe));
        let held = resolved(&exact);
        assert!(held.evaluation.issues().next().is_none(), "{limit:?}");
        let over = resolved(&edge(limit, LimitEdge::FirstOver));
        assert_eq!(
            issues(&over),
            [(unit.clone(), Code::ResourceLimitExceeded, None)],
            "{limit:?}"
        );
        let key = over.evaluation.issues().next().unwrap();
        let exceeded = over.evaluation.limit(key).unwrap();
        assert_eq!(exceeded.bound, bound(name), "{limit:?}");
        assert_eq!(exceeded.limit.get(), count - 1, "{limit:?}");
        assert_eq!(exceeded.actual, Actual::Exact(count), "{limit:?}");
        // Only the relations' bound and the blocked unit were checked, and
        // every other unit waits on the blocked one.
        let relations = hydration_unit(HydrationSubject::Relations, Check::WithinBound);
        for (recorded, status) in over.evaluation.statuses() {
            match status {
                Status::Evaluated => assert!(
                    *recorded == relations || *recorded == unit,
                    "{limit:?} {recorded:?}"
                ),
                Status::NotEvaluated(causes) => assert_eq!(
                    causes
                        .keys()
                        .map(|key| key.unit().clone())
                        .collect::<Vec<_>>(),
                    std::slice::from_ref(&unit),
                    "{limit:?} {recorded:?}"
                ),
            }
        }
    }
}

#[test]
fn reordering_changes_neither_domain_nor_the_work() {
    let fan = prepare(&case(R::FanOutFanIn)).unwrap();
    let reordered = prepare(&case(R::FanReordered)).unwrap();
    assert_eq!(fan.observation, reordered.observation);
    assert_eq!(fan.logical_work, reordered.logical_work);
    // The configuration and the collection are the case's input, so its
    // identity is not shared.
    assert_ne!(
        super::context::observe(&fan).unwrap(),
        super::context::observe(&reordered).unwrap()
    );
}

#[test]
fn every_other_case_has_its_own_shared_domain() {
    let mut seen = Vec::new();
    for declaration in declarations().into_iter().filter(|case| {
        case.operation == Operation::HydrationRelationValidation
            && case.fixture != Recipe::Hydration(R::FanReordered)
            // An exact edge validates as its recipe does.
            && !matches!(case.limit, Some((_, LimitEdge::Exact)))
    }) {
        let observation = prepare(&declaration).unwrap().observation;
        assert!(observation.entry.is_none(), "{declaration:?}");
        assert!(
            seen.iter().all(|shared| *shared != observation.shared),
            "{declaration:?}"
        );
        seen.push(observation.shared);
    }
    assert_eq!(seen.len(), 18 + 2);
}

#[test]
fn the_work_counts_relations_roles_units_and_what_was_checked() {
    let work = |declaration: &Declaration| {
        let candidate = prepare(declaration).unwrap();
        serde_json::to_value(LogicalWork::observe(&candidate.prepared, &candidate.output).unwrap())
            .unwrap()
    };
    let fact = |work: &Value, kind: &str| {
        work["facts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|fact| fact["kind"] == kind)
            .unwrap()["observation"]["value"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let kinds = [
        "hydration-relation-occurrences",
        "hydration-distinct-relations",
        "hydration-maximum-endpoint-id-bytes",
        "hydration-role-targets",
        "hydration-evaluation-units",
        "hydration-not-evaluated-units",
        "hydration-blocking-issues",
        "hydration-checked-relations",
        "hydration-checked-groups",
    ];
    for (declaration, expected) in [
        // The relations' unit alone.
        (
            case(R::NoRelations),
            ["0", "0", "0", "0", "1", "0", "0", "0", "1"],
        ),
        (
            case(R::Storefront),
            ["1", "1", "7", "2", "13", "0", "0", "1", "1"],
        ),
        (
            case(R::FanOutFanIn),
            ["3", "3", "7", "4", "35", "0", "0", "3", "1"],
        ),
        // Eight servers each related to each of eight clients.
        (
            case(R::ManyRelations),
            ["64", "64", "8", "16", "657", "0", "0", "64", "1"],
        ),
        // A repeated pair is one relation with its units.
        (
            case(R::DuplicateRelation),
            ["2", "1", "7", "2", "13", "0", "1", "0", "0"],
        ),
        // Between equal endpoints the five comparisons wait, and no role is
        // taken.
        (
            case(R::EqualEndpoint),
            ["2", "2", "7", "2", "23", "5", "1", "0", "0"],
        ),
        // Every unit but the bound waits on it.
        (
            edge(LimitKind::HydrationRelations, LimitEdge::FirstOver),
            ["3", "3", "7", "4", "35", "34", "1", "0", "0"],
        ),
    ] {
        let work = work(&declaration);
        for (kind, value) in kinds.iter().zip(expected) {
            assert_eq!(fact(&work, kind), value, "{declaration:?} {kind}");
        }
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
        let mut candidate = prepare(&case(R::Storefront)).unwrap();
        assert!(observe(&candidate).is_ok());
        let Prepared::Hydration(prepared) = &mut candidate.prepared else {
            unreachable!()
        };
        let expected = match change {
            0 => {
                candidate.declaration.fixture = Recipe::Minimal;
                ContextFailure::HydrationInputMismatch
            }
            1 => {
                candidate.declaration.selector = Selector::Absent;
                ContextFailure::HydrationInputMismatch
            }
            2 => {
                prepared.selected = serde_json::from_value(json!("other")).unwrap();
                ContextFailure::HydrationInputMismatch
            }
            3 => {
                prepared.analysis = analyzed(b"{}");
                ContextFailure::HydrationInputMismatch
            }
            4 => {
                let mut value = crate::profile_fixtures::minimal_config();
                value["profiles"]["broken"] = json!({});
                let analysis = analyzed(&serde_json::to_vec(&value).unwrap());
                assert!(!analysis.is_complete());
                prepared.analysis = analysis;
                ContextFailure::HydrationInputMismatch
            }
            _ => {
                candidate.input_limits = None;
                ContextFailure::PreparationLimitsMismatch
            }
        };
        assert_eq!(observe(&candidate), Err(expected), "change {change}");
        assert_eq!(
            candidate.prepared.prerequisites_admitted(),
            !(2..5).contains(&change),
            "change {change}"
        );
    }
    // The answers are the case's input too.
    let storefront = prepare(&case(R::Storefront)).unwrap();
    let mut answered = prepare(&case(R::Storefront)).unwrap();
    let Prepared::Hydration(prepared) = &mut answered.prepared else {
        unreachable!()
    };
    prepared
        .capabilities
        .renders
        .insert("storefront-browser".into());
    assert_ne!(observe(&storefront).unwrap(), observe(&answered).unwrap());
}
