// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use serde_json::{json, Value};

use super::prepare::prepare;
use super::*;
use crate::benchmark::operation::{Output, Prepared};
use crate::benchmark::target_group::Resolved;
use crate::benchmark::work::LogicalWork;
use crate::model::{DeploymentGroupId, TargetId};
use crate::resolver::evaluation::{Actual, Check, Code, IssueKey, Reason, Status, Subject, Unit};
use crate::resolver::groups::{unit as group_unit, GroupSubject};
use crate::resolver::limits::{BoundId, Group};
use crate::resolver::targets::{unit as target_unit, TargetSubject};

use TargetAndGroupRecipe as R;

fn case(recipe: TargetAndGroupRecipe) -> Declaration {
    declarations()
        .into_iter()
        .find(|case| case.fixture == Recipe::TargetAndGroup(recipe) && case.limit.is_none())
        .unwrap()
}

fn edge(limit: LimitKind, edge: LimitEdge) -> Declaration {
    declarations()
        .into_iter()
        .find(|case| case.limit == Some((limit, edge)))
        .unwrap()
}

fn resolved(declaration: &Declaration) -> Resolved {
    let Output::TargetAndGroup(resolved) = prepare(declaration).unwrap().output else {
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

fn of_target(id: &str, check: Check) -> Unit {
    target_unit(TargetSubject::Target(target(id)), check)
}

#[test]
fn target_group_cases_keep_the_252_rows_and_append_after_them() {
    let cases = declarations();
    assert_eq!(cases.len(), 304);
    assert!(cases[..252]
        .iter()
        .all(|case| case.operation != Operation::TargetAndGroupResolution));
    assert!(cases[252..281].iter().all(|case| {
        case.operation == Operation::TargetAndGroupResolution && case.selector == Selector::App
    }));
    // Every recipe once, in its order, and then the twelve edges.
    assert_eq!(
        cases[252..269]
            .iter()
            .map(|case| (case.fixture, case.limit))
            .collect::<Vec<_>>(),
        R::ALL
            .into_iter()
            .map(|recipe| (Recipe::TargetAndGroup(recipe), None))
            .collect::<Vec<_>>()
    );
    assert!(cases[269..281].iter().all(|case| case.limit.is_some()));
    for declaration in &cases[252..281] {
        // The case's source is the configuration, and the members are what
        // preparation read.
        let Recipe::TargetAndGroup(recipe) = declaration.fixture else {
            unreachable!()
        };
        let fixture = recipe.fixture();
        assert_eq!(
            *declaration.fixture.source(),
            *serde_json::to_vec(&fixture.config).unwrap(),
            "{declaration:?}"
        );
        let candidate = prepare(declaration).unwrap();
        let Prepared::TargetAndGroup(prepared) = &candidate.prepared else {
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
        let Output::TargetAndGroup(resolved) = candidate.output else {
            panic!("wrong owner boundary")
        };
        let retained = resolved.result.unwrap().unwrap();
        let complete = retained.partition.is_ok()
            && retained
                .targets
                .iter()
                .all(|target| target.resolved.is_ok());
        assert_eq!(
            complete,
            declaration.expected_kind == ExpectedKind::TargetAndGroupResolutionComplete,
            "{declaration:?}"
        );
    }
}

/// A complete case resolves every target with the project's locales it
/// requests and its effective default, and partitions the targets.
#[test]
fn a_complete_case_resolves_every_target_and_partitions_them() {
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
        assert!(retained.partition.is_ok(), "{recipe:?}");
        let fixture = recipe.fixture();
        let declared = &fixture.config["profiles"]["app"]["targetProfiles"];
        let mut ids = declared
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        ids.sort();
        assert_eq!(
            retained
                .targets
                .iter()
                .map(|target| target.id.clone())
                .collect::<Vec<_>>(),
            ids,
            "{recipe:?}"
        );
        for target in &retained.targets {
            let declared = &declared[&target.id];
            let mut requested = declared["requestedLocales"]
                .as_array()
                .unwrap()
                .iter()
                .map(|locale| locale.as_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            requested.sort();
            assert_eq!(target.requested, Ok(requested), "{recipe:?}");
            assert_eq!(target.effective_default, Ok("en".into()), "{recipe:?}");
            let named = target.profile.as_ref().unwrap();
            assert_eq!(
                named.identity.as_str(),
                declared["profile"]["identity"],
                "{recipe:?}"
            );
            assert!(target.resolved.is_ok(), "{recipe:?}");
        }
        let groups = &fixture.config["profiles"]["app"]["deploymentGroups"];
        for group in &retained.groups {
            let mut members = groups[&group.id]["members"]
                .as_array()
                .unwrap()
                .iter()
                .map(|member| member.as_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            members.sort();
            assert_eq!(group.members, Ok(members), "{recipe:?}");
        }
        assert_eq!(
            retained.groups.len(),
            groups.as_object().unwrap().len(),
            "{recipe:?}"
        );
    }
}

/// Each change of the example reaches the one decision its name says, and
/// nothing else is blocked.
#[test]
fn each_blocked_recipe_reports_its_one_decision() {
    let member = |id: &str| GroupSubject::Member(group("web"), target(id));
    for (recipe, expected) in [
        (
            R::LocalesNotSubset,
            (
                of_target("ssr", Check::ProjectSubset),
                Code::TargetLocalesNotSubset,
                None,
            ),
        ),
        (
            R::ExplicitDefaultNotRequested,
            (
                of_target("browser", Check::SupportedDefault),
                Code::TargetDefaultLocaleNotSupported,
                Some(Reason::ExplicitOverride),
            ),
        ),
        (
            R::InheritedDefaultNotRequested,
            (
                of_target("ssr", Check::SupportedDefault),
                Code::TargetDefaultLocaleNotSupported,
                Some(Reason::InheritedProjectDefault),
            ),
        ),
        (
            R::TargetLocaleDirectionMissing,
            (
                of_target("ssr", Check::DirectedLocales),
                Code::TargetLocaleDirectionMissing,
                None,
            ),
        ),
        (
            R::MemberUnknown,
            (
                group_unit(member("tablet"), Check::KnownMember),
                Code::GroupMemberUnknown,
                None,
            ),
        ),
        (
            R::MemberDuplicate,
            (
                group_unit(member("browser"), Check::UniqueMember),
                Code::GroupMemberDuplicate,
                None,
            ),
        ),
        (
            R::TargetUnassigned,
            (
                group_unit(GroupSubject::Assignment(target("ssr")), Check::SingleGroup),
                Code::TargetUnassigned,
                None,
            ),
        ),
        (
            R::TargetInTwoGroups,
            (
                group_unit(
                    GroupSubject::Assignment(target("browser")),
                    Check::SingleGroup,
                ),
                Code::TargetAssignedToMultipleGroups,
                None,
            ),
        ),
    ] {
        let resolved = resolved(&case(recipe));
        assert_eq!(issues(&resolved), [expected], "{recipe:?}");
        // Every unit was checked: a blocking result blocks only what waits
        // on it.
        assert!(
            resolved
                .evaluation
                .statuses()
                .all(|(_, status)| *status == Status::Evaluated),
            "{recipe:?}"
        );
    }
}

/// What Phase 7, the collection admission, or Phase 9 found before the
/// interval leaves a target's facts unavailable without a result of Phases
/// 11 and 12.
#[test]
fn a_prerequisite_failure_leaves_only_what_waits_on_it() {
    let ssr = |recipe| {
        let resolved = resolved(&case(recipe));
        assert!(resolved.evaluation.issues().next().is_none(), "{recipe:?}");
        let retained = resolved.result.unwrap().unwrap();
        assert!(retained.partition.is_ok(), "{recipe:?}");
        let browser = retained.targets[0].clone();
        assert!(browser.resolved.is_ok(), "{recipe:?}");
        (resolved.evaluation, retained.targets[1].clone())
    };
    let subjects = |causes: &crate::resolver::evaluation::Causes| {
        causes
            .keys()
            .map(|key| key.unit().subject.clone())
            .collect::<Vec<_>>()
    };
    // Phase 7 rejects one spelling, so none of the target's three locale
    // checks runs.
    let (evaluation, invalid) = ssr(R::TargetLocaleInvalid);
    let waiting = evaluation
        .statuses()
        .filter_map(|(unit, status)| match status {
            Status::NotEvaluated(causes) => Some((unit.check, subjects(causes))),
            Status::Evaluated => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(waiting.len(), 3);
    assert!(waiting.iter().all(|(check, subjects)| {
        matches!(
            check,
            Check::ProjectSubset | Check::SupportedDefault | Check::DirectedLocales
        ) && subjects
            .iter()
            .all(|subject| matches!(subject, Subject::Locale(_)))
    }));
    assert!(invalid.requested.is_err() && invalid.resolved.is_err());
    // Phase 7 records the repeat, which only the resolved target needs.
    let (evaluation, repeated) = ssr(R::TargetLocaleRepeated);
    assert!(evaluation
        .statuses()
        .all(|(_, status)| *status == Status::Evaluated));
    assert_eq!(
        repeated.requested,
        Ok(vec!["en".into(), "fr-FR".into(), "ja".into()])
    );
    let causes = repeated.resolved.unwrap_err();
    assert_eq!(
        causes
            .keys()
            .map(|key| key.unit().check)
            .collect::<Vec<_>>(),
        [Check::UniqueLocale]
    );
    // Phase 9 finds no Target Profile for `ssr`: the Set holds only
    // `browser`'s, another identity of the kind. Only its directions wait.
    let (evaluation, missing) = ssr(R::TargetProfileMissing);
    let directed = of_target("ssr", Check::DirectedLocales);
    assert!(evaluation
        .statuses()
        .all(|(unit, status)| (*status == Status::Evaluated) == (*unit != directed)));
    let causes = missing.profile.unwrap_err();
    assert_eq!(
        evaluation
            .statuses()
            .find(|(unit, _)| **unit == directed)
            .unwrap()
            .1,
        &Status::NotEvaluated(causes.clone())
    );
    assert!(subjects(&causes)
        .iter()
        .all(|subject| matches!(subject, Subject::ArtifactReference(_))));
    assert_eq!(
        causes.keys().map(IssueKey::reason).collect::<Vec<_>>(),
        [Some(Reason::IdentityMismatch)]
    );
    assert_eq!(missing.resolved, Err(causes));
    // Without a Set, no target's Target Profile is resolved, and no target's
    // directions are checked.
    let resolved = resolved(&case(R::CollectionNotAdmitted));
    assert!(resolved.evaluation.issues().next().is_none());
    assert!(resolved.evaluation.statuses().all(|(unit, status)| {
        (*status == Status::Evaluated) == (unit.check != Check::DirectedLocales)
    }));
    let retained = resolved.result.unwrap().unwrap();
    for target in &retained.targets {
        let causes = target.profile.clone().unwrap_err();
        assert!(subjects(&causes)
            .iter()
            .all(|subject| matches!(subject, Subject::ArtifactCollection(_))));
        assert!(target.requested.is_ok() && target.effective_default.is_ok());
    }
    assert!(retained.partition.is_ok());
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
            LimitKind::TargetProfiles,
            R::Storefront,
            "maxTargetProfiles",
            2,
            target_unit(TargetSubject::Targets, Check::WithinBound),
        ),
        (
            LimitKind::TargetIdBytes,
            R::Storefront,
            "maxTargetIdBytes",
            7,
            of_target("browser", Check::WithinBound),
        ),
        (
            LimitKind::DeploymentGroups,
            R::ManyGroups,
            "maxDeploymentGroups",
            64,
            group_unit(GroupSubject::Groups, Check::WithinBound),
        ),
        (
            LimitKind::GroupIdBytes,
            R::Storefront,
            "maxGroupIdBytes",
            3,
            group_unit(GroupSubject::Group(group("web")), Check::WithinBound),
        ),
        (
            LimitKind::MembersPerGroup,
            R::Storefront,
            "maxMembersPerGroup",
            2,
            group_unit(GroupSubject::Members(group("web")), Check::WithinBound),
        ),
        (
            LimitKind::MembershipOccurrences,
            R::Storefront,
            "maxMembershipOccurrences",
            2,
            group_unit(GroupSubject::Memberships, Check::WithinBound),
        ),
    ] {
        let exact = edge(limit, LimitEdge::Exact);
        assert_eq!(exact.fixture, Recipe::TargetAndGroup(recipe));
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
        // The same units as the exact case, and its result with the bound's
        // unit blocked.
        assert_eq!(
            over.evaluation
                .statuses()
                .map(|(recorded, _)| recorded.clone())
                .collect::<Vec<_>>(),
            held.evaluation
                .statuses()
                .map(|(recorded, _)| recorded.clone())
                .collect::<Vec<_>>(),
            "{limit:?}"
        );
    }
}

#[test]
fn reordering_changes_neither_domain_nor_the_work() {
    let storefront = prepare(&case(R::Storefront)).unwrap();
    let reordered = prepare(&case(R::StorefrontReordered)).unwrap();
    assert_eq!(storefront.observation, reordered.observation);
    assert_eq!(storefront.logical_work, reordered.logical_work);
    // The configuration and the collection are the case's input, so its
    // identity is not shared.
    assert_ne!(
        super::context::observe(&storefront).unwrap(),
        super::context::observe(&reordered).unwrap()
    );
}

#[test]
fn every_other_case_has_its_own_shared_domain() {
    let mut seen = Vec::new();
    for declaration in declarations().into_iter().filter(|case| {
        case.operation == Operation::TargetAndGroupResolution
            && case.fixture != Recipe::TargetAndGroup(R::StorefrontReordered)
            // An exact edge resolves as its recipe does.
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
    assert_eq!(seen.len(), 16 + 6);
}

#[test]
fn the_work_counts_the_bounds_units_and_what_resolved() {
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
        "target-and-group-targets",
        "target-and-group-maximum-target-id-bytes",
        "target-and-group-groups",
        "target-and-group-maximum-group-id-bytes",
        "target-and-group-maximum-members-per-group",
        "target-and-group-membership-occurrences",
        "target-and-group-evaluation-units",
        "target-and-group-not-evaluated-units",
        "target-and-group-blocking-issues",
        "target-and-group-resolved-targets",
        "target-and-group-resolved-groups",
    ];
    for (declaration, expected) in [
        (
            case(R::Minimal),
            ["1", "7", "1", "3", "1", "1", "12", "0", "0", "1", "1"],
        ),
        (
            case(R::Storefront),
            ["2", "7", "1", "3", "2", "2", "19", "0", "0", "2", "1"],
        ),
        // 64 targets in one group, and 64 groups of one.
        (
            case(R::ManyTargets),
            ["64", "9", "1", "3", "64", "64", "453", "0", "0", "64", "1"],
        ),
        (
            case(R::ManyGroups),
            ["64", "9", "64", "8", "1", "64", "579", "0", "0", "64", "64"],
        ),
        // `tablet` is one more distinct member, which blocks the group.
        (
            case(R::MemberUnknown),
            ["2", "7", "1", "3", "3", "3", "21", "0", "1", "2", "0"],
        ),
        // A repeated member is one more occurrence, but no more units.
        (
            case(R::MemberDuplicate),
            ["2", "7", "1", "3", "3", "3", "19", "0", "1", "2", "0"],
        ),
        // Each target's four checks wait on the map's bound, and so do the
        // group's two member checks and each target's place.
        (
            edge(LimitKind::TargetProfiles, LimitEdge::FirstOver),
            ["2", "7", "1", "3", "2", "2", "19", "12", "1", "0", "0"],
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
        let mut candidate = prepare(&case(R::Minimal)).unwrap();
        assert!(super::context::observe(&candidate).is_ok());
        let Prepared::TargetAndGroup(prepared) = &mut candidate.prepared else {
            unreachable!()
        };
        let expected = match change {
            0 => {
                candidate.declaration.fixture = Recipe::Minimal;
                ContextFailure::TargetInputMismatch
            }
            1 => {
                candidate.declaration.selector = Selector::Absent;
                ContextFailure::TargetInputMismatch
            }
            2 => {
                prepared.selected = serde_json::from_value(json!("other")).unwrap();
                ContextFailure::TargetInputMismatch
            }
            // The selected declaration's targets and groups cannot be
            // borrowed from an incomplete root: not from one without
            // structural units, nor from one whose analysis completed with an
            // invalid sibling.
            3 => {
                prepared.analysis = analyzed(b"{}");
                ContextFailure::TargetInputMismatch
            }
            4 => {
                let mut value = crate::profile_fixtures::minimal_config();
                value["profiles"]["broken"] = json!({});
                let analysis = analyzed(&serde_json::to_vec(&value).unwrap());
                assert!(!analysis.is_complete());
                assert!(analysis.benchmark_structural_units().is_some());
                prepared.analysis = analysis;
                ContextFailure::TargetInputMismatch
            }
            // The configuration is materialized under the file limits.
            _ => {
                candidate.input_limits = None;
                ContextFailure::PreparationLimitsMismatch
            }
        };
        assert_eq!(observe(&candidate), Err(expected), "change {change}");
        // Phases 11 and 12 resolve the selected declaration of a complete
        // root only.
        assert_eq!(
            candidate.prepared.prerequisites_admitted(),
            !(2..5).contains(&change),
            "change {change}"
        );
    }
}
