// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's replay of a Materialized Resolution Input Set: the Shared projection
//! and, when the shared result admits them, the profile view and Resolved
//! Profile Artifact Bindings, recomputed from a retained set under the
//! construction it names (D15:2658, 2660; Decision 015-205).
//!
//! [`Resolver::replay`] replays a set only under a constructed resolver whose
//! Resolver Construction Identity equals the set's reference. An unequal
//! reference is construction staleness, [`ConstructionStale`], which runs no
//! phase and leaves no result (D15:2586, 2660). The set's values were admitted
//! when it was created, so a replay admits nothing at a pre-invocation
//! boundary (D15:2776).
//!
//! The retained value, as the document of its canonical tree, takes the place
//! of the one `entry-admission` materialized, and the replay evaluates
//! `resolver-bootstrap-admission` and `structural-admission` through
//! `hydration-validation` under the other retained values, so every Derived
//! Admission State is recomputed rather than read (D15:2658). Phase 0's
//! Finding and status are Entry-owned, and the replay evaluates it only for the
//! status that the `profileSpecification` group's owner waits on (D15:2146). No
//! later check waits on an `entry-admission` check, and the value exists only
//! because that phase admitted it, so Phase 1 does not run (D15:2660). The
//! Shared projection is then built as the invocation built it and finalized
//! within the Shared domain ([`super::outcome::replayed`]).
//!
//! Phases 2 to 13 ask an entry for the Evidence of their candidates, and the
//! set holds no entry source. The replay's entry names an identity no boundary
//! admitted, never marked non-secret, without a locator or a span, under a
//! programmatic entry's Safe Origin Kinds, since the document is the one of a
//! marker-free Snapshot; its members are classified as the invocation
//! classified them (D15:777). That Evidence stays in the replay's evaluation:
//! the Shared domain counts each Finding candidate but retains its occurrences
//! and Evidence nowhere, since they are Entry records (D15:1943), and a replay
//! builds no Entry projection.
//!
//! The result is the identity, the Shared projection, and the profile view and
//! bindings only when the Shared domain is complete and the replayed phases
//! found no blocking Finding. It has no Entry projection, bundle completeness,
//! Entry-domain terminal state, Finding Occurrence Key, Coverage Decision
//! Evidence, or checked or blocked outcome, which need the complete Resolver
//! Invocation Input Set (D15:2660). What it borrows, the evaluation, the
//! Artifact Set Phase 9 admitted, and the semantic state, its caller holds in a
//! [`ReplayStorage`] of its own for each replay, which the borrow keeps from
//! being reset while the result lives (D15:2750).

use crate::boundary::source::ExactIdentity;
use crate::construction::identity::ConstructionIdentity;
use crate::locale::{FormalData, Provider};
use crate::materialize::{Locus, MaterializedDocument};

use super::artifact_resolution::ArtifactSet;
use super::entry::{Classify, ConfigurationEntry};
use super::evaluation::{Admitted, Evaluation};
use super::evidence::Source;
use super::evidence_path::KeyFacts;
use super::evidence_source::SourceSpan;
use super::invocation::Resolver;
use super::materialized::MaterializedInputs;
use super::origin::SafeOriginKind;
use super::outcome::{replayed, Checked};
use super::profile_resolution::{resolve_materialized, ResolverInvariant, SemanticState};
use super::resolution_evidence::Projection;
use super::specification::admit_project_profile;
use super::token::Sha256;

/// What one replay's result borrows: the evaluation the replayed phases
/// recorded, the Profile Resolution Artifact Set Phase 9 admitted, and the
/// semantic state. Its caller holds one for each replay (D15:2750).
#[derive(Default)]
pub(crate) struct ReplayStorage<'s> {
    evaluation: Evaluation,
    set: Option<Admitted<ArtifactSet<'s>>>,
    state: Option<SemanticState<'s, String>>,
}

/// A set whose Resolver Construction Identity reference is not the replaying
/// resolver's: construction staleness, which a resolver constructed again
/// answers rather than a replay (D15:2586, 2660).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ConstructionStale;

/// What one replay returns (D15:2660).
pub(crate) struct Replay<'e, 's> {
    /// The Resolver Construction Identity, the set's reference and the
    /// resolver's own.
    pub(crate) identity: ConstructionIdentity,
    /// The Shared projection, as the invocation produced it.
    pub(crate) shared: Projection<'e>,
    /// The profile view and its bindings, when the Shared domain is complete
    /// and the replayed phases found no blocking Finding.
    pub(crate) profile: Option<Checked<'e, 's>>,
}

/// The entry through which a replay's phases observe the retained value: the
/// identity [`ExactIdentity::replayed`], not marked non-secret and without a
/// locator or a span, and the invocation's classification.
struct ReplayEntry<'a> {
    identity: ExactIdentity,
    classify: &'a Classify<'a>,
}

/// The document is a Snapshot's, so the slots are a programmatic entry's
/// (D15:205).
impl ConfigurationEntry for ReplayEntry<'_> {
    fn origin(&self) -> SafeOriginKind {
        SafeOriginKind::ConfigurationProgrammatic
    }

    fn selector_origin(&self) -> SafeOriginKind {
        SafeOriginKind::ProfileSelectorProgrammatic
    }

    fn classify(&self) -> &Classify<'_> {
        self.classify
    }

    fn source_at(&self, _: Locus) -> (Source<'_>, Option<SourceSpan>) {
        let source = Source {
            identity: &self.identity,
            marked: false,
            locator: None,
        };
        (source, None)
    }
}

impl Resolver<'_> {
    /// Replays `materialized`, the result borrowing `storage`, or refuses a
    /// set of another construction (D15:2660).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug a replayed phase, the Shared projection, or
    /// profile projection found.
    pub(crate) fn replay<'s, D, P>(
        &'s self,
        materialized: &'s MaterializedInputs<D>,
        storage: &'s mut ReplayStorage<'s>,
    ) -> Result<Result<Replay<'s, 's>, ConstructionStale>, ResolverInvariant>
    where
        D: FormalData<Provider = P>,
        P: Provider<Identity = String>,
    {
        if materialized.construction_identity() != self.identity() {
            return Ok(Err(ConstructionStale));
        }
        let common = materialized.common();
        let invocation = self.invocation(common);
        let comparable = common.capability.policy_comparable();
        let schema = self.configuration_schema();
        let classify: &Classify<'_> = &|edges, locales| {
            KeyFacts {
                comparable,
                locales,
            }
            .steps(schema, edges)
        };
        let entry = ReplayEntry {
            identity: ExactIdentity::replayed(),
            classify,
        };
        let document = MaterializedDocument::from_snapshot(materialized.value())
            .ok_or(ResolverInvariant::RetainedMarker)?;
        let ReplayStorage {
            evaluation,
            set,
            state,
        } = storage;
        let project_profile =
            admit_project_profile(evaluation, invocation.supported, invocation.specifications)?;
        let structural = comparable.configuration_input.structural;
        let (resolved, _) = resolve_materialized(
            evaluation,
            (&project_profile, &entry, document),
            (schema, structural, materialized.selector()),
            &invocation,
            &common.data,
            set,
        )?;
        let state = &*state.insert(resolved);
        let (shared, profile) = replayed(evaluation, state, &invocation, &Sha256)?;
        Ok(Ok(Replay {
            identity: *self.identity(),
            shared,
            profile,
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{json, Value};

    use super::*;
    use crate::artifacts::envelope::{ExactTuple, FormalMember};
    use crate::boundary::invocation::InvocationInputs;
    use crate::construction::flow::construct;
    use crate::construction::identity::derive;
    use crate::construction::registry::check::RecordKind;
    use crate::construction::resolver::committed;
    use crate::construction::root::ConstructionRoot;
    use crate::resolver::evidence_source::hex;
    use crate::resolver::invocation::tests::{example, resolver, PinnedFixture, Raw};
    use crate::resolver::invocation::OutcomeStorage;
    use crate::resolver::limits::{BoundId, Group};
    use crate::resolver::outcome::tests::{
        blocked, profile, projected, reporting, shared, shown, Owned, Shown,
    };
    use crate::resolver::profile_resolution::tests::Inputs;
    use crate::resolver::profile_view::ProfileView;
    use crate::resolver::reporting::ReportingDomain;
    use crate::resolver::staleness::{compare, Retained, Staleness};

    /// What a Shared projection shows: its records, and its terminal Finding
    /// and state when a bound exhausted it.
    type SharedShown = (Owned, Option<(Value, Value)>);

    /// What a replay shows, outliving it: its identity, its Shared
    /// projection, and its profile view with its bindings' tuples and members.
    type Replayed = (
        ConstructionIdentity,
        SharedShown,
        Option<(ProfileView, Vec<(ExactTuple, FormalMember)>)>,
    );

    /// The file of `raw`, and Snapshots of its value without a sidecar and
    /// beside one mirroring the file, admitted on `resolver`.
    fn entries(resolver: &Resolver<'_>, raw: &Raw) -> [InvocationInputs<PinnedFixture>; 3] {
        [
            raw.file(resolver, true),
            raw.programmatic(resolver, true, false),
            raw.programmatic(resolver, true, true),
        ]
    }

    /// What the outcome of `inputs` on `resolver` shows, and the set beside
    /// it.
    fn resolved(
        resolver: &Resolver<'_>,
        inputs: &InvocationInputs<PinnedFixture>,
    ) -> (Shown, MaterializedInputs<PinnedFixture>) {
        let mut storage = OutcomeStorage::default();
        let resolution = resolver.resolve(inputs, &mut storage).unwrap();
        (shown(&resolution.outcome), resolution.materialized.unwrap())
    }

    /// What replaying `set` on `resolver` shows.
    fn replays(resolver: &Resolver<'_>, set: &MaterializedInputs<PinnedFixture>) -> Replayed {
        let mut storage = ReplayStorage::default();
        let replay = resolver.replay(set, &mut storage).unwrap().unwrap();
        let profile = replay.profile.as_ref().map(profile);
        (replay.identity, projected(&replay.shared), profile)
    }

    /// The Shared records of `shown` of `kind`.
    fn count(shown: &Shown, kind: RecordKind) -> usize {
        let (records, _) = shown.shared.as_ref().unwrap();
        records.iter().filter(|(_, of, _)| *of == kind).count()
    }

    /// How many records the first unit that reports into `shown`'s Shared
    /// domain retains, which a Shared `maxRecords` at that count lets the
    /// first slice commit and stops at the next unit's.
    fn first_slice(shown: &Shown) -> u64 {
        let (records, _) = shown.shared.as_ref().unwrap();
        let first = records.iter().map(|(unit, _, _)| unit).min().unwrap();
        let owned = records.iter().filter(|(unit, _, _)| unit == first).count();
        owned as u64
    }

    /// 015's example with the specification input asserting no
    /// `project-profile`, which Phase 0 then finds missing (D15:1126).
    fn unasserted() -> Raw {
        let mut raw = example(|inputs| inputs);
        let mut input: Value = serde_json::from_slice(&raw.specification).unwrap();
        input.as_object_mut().unwrap().remove("project-profile");
        raw.specification = serde_json::to_vec(&input).unwrap();
        raw
    }

    /// D15:2660, 3233 (Decision 015-205): replaying the set of a file, of a
    /// Snapshot of its value, or of one beside a mirroring sidecar gives the
    /// invocation's identity and Shared projection, every Finding Key in
    /// order, status, cause, Coverage Decision Basis, dependency, and
    /// terminal state included, whether the invocation was checked, blocked
    /// after materialization, by Phase 0, or past a capability's structural
    /// bound, or incomplete past a Shared bound at its first unit or at a
    /// later one, or past a bound of the Shared scope; and it gives the
    /// invocation's profile view and bindings exactly when the invocation was
    /// checked.
    #[test]
    fn a_replay_gives_the_shared_projection_of_the_invocation() {
        let resolver = resolver();
        let units = BoundId {
            group: Group::ConfigurationInput,
            name: "maxStructuralAnalysisUnits",
        };
        let exhausted = reporting(&Inputs::complete(), &[], &[(shared("maxRecords"), 1)]);
        let (checked, _) = resolved(&resolver, &example(|inputs| inputs).file(&resolver, true));
        let first = (shared("maxRecords"), first_slice(&checked));
        let later = reporting(&Inputs::complete(), &[], &[first]);
        let scope = example(|inputs| inputs).bounded("evidenceMetadata.maxSubjectOccurrences", 1);
        for (name, raw) in [
            ("checked", example(|inputs| inputs)),
            ("blocked", Raw::new(&blocked())),
            ("unasserted", unasserted()),
            (
                "structural",
                example(|inputs| inputs.bounding(&[(units, 10)], &[])),
            ),
            ("exhausted", Raw::new(&exhausted)),
            ("later", Raw::new(&later)),
            ("scope", scope),
        ] {
            for inputs in entries(&resolver, &raw) {
                let (shown, set) = resolved(&resolver, &inputs);
                assert_eq!(shown.checked.is_some(), name == "checked", "{name}");
                let (_, terminal) = shown.shared.as_ref().unwrap();
                let incomplete = ["exhausted", "later", "scope"].contains(&name);
                assert_eq!(terminal.is_some(), incomplete, "{name}");
                let (identity, shared, profile) = replays(&resolver, &set);
                assert_eq!(identity, shown.identity, "{name}");
                assert_eq!(Some(shared), shown.shared, "{name}");
                assert_eq!(profile, shown.checked, "{name}");
            }
        }
    }

    /// D15:2660: an invocation blocked only because its complete token inputs
    /// exceed their bound, which the Shared scope's are within, replays to
    /// its Shared projection and to the profile view and bindings of the
    /// checked invocation without that bound.
    #[test]
    fn a_replay_has_the_profile_an_entry_domain_bound_withheld() {
        let resolver = resolver();
        let raw = example(|inputs| inputs);
        let (checked, _) = resolved(&resolver, &raw.file(&resolver, true));
        let units = count(&checked, RecordKind::SharedEvaluationStatus);
        let raw = raw.bounded("evidenceMetadata.maxTokenInputs", units as u64);
        for inputs in entries(&resolver, &raw) {
            let (shown, set) = resolved(&resolver, &inputs);
            assert!(shown.checked.is_none());
            let (_, terminal) = shown.shared.as_ref().unwrap();
            assert!(terminal.is_none());
            assert_eq!(count(&shown, RecordKind::SharedEvaluationStatus), units);
            let (_, shared, profile) = replays(&resolver, &set);
            assert_eq!(Some(shared), shown.shared);
            assert!(profile.is_some());
            assert_eq!(profile, checked.checked);
        }
    }

    /// D15:2586, 2660: a set whose reference is another construction's is
    /// construction staleness, and replays to no result.
    #[test]
    fn a_set_of_another_construction_is_not_replayed() {
        let resolver = resolver();
        let inputs = example(|inputs| inputs).file(&resolver, true);
        let (_, set) = resolved(&resolver, &inputs);
        let root = ConstructionRoot::built_in().unwrap();
        let mut admitted = construct(root, committed().input()).unwrap();
        admitted.registry_package.declared.revision = "1".to_owned();
        let other = MaterializedInputs::new(
            derive(root, &admitted),
            set.value().clone(),
            set.selector().clone(),
            Arc::clone(&inputs.common),
        );
        let mut storage = ReplayStorage::default();
        let replay = resolver.replay(&other, &mut storage).unwrap();
        assert_eq!(replay.err(), Some(ConstructionStale));
    }

    /// The members of the raw Specification Input, in slot order.
    const SLOTS: [&str; 8] = [
        "project-profile",
        "locale-canonicalization",
        "locale-negotiation",
        "message-locale-fallback",
        "coverage",
        "intent-surface-class-vocabulary",
        "delivery-graph",
        "delivery-placement",
    ];

    /// 015's example with its Specification Input after `edit`, written
    /// spaced when `spaced`.
    fn specified(spaced: bool, edit: impl FnOnce(&mut Value)) -> Raw {
        let mut raw = example(|inputs| inputs);
        let mut input: Value = serde_json::from_slice(&raw.specification).unwrap();
        edit(&mut input);
        let written = if spaced {
            serde_json::to_vec_pretty(&input)
        } else {
            serde_json::to_vec(&input)
        };
        raw.specification = written.unwrap();
        raw
    }

    /// D15:1147, 2586, 2590, 2660, 3548: each Specification Input member's
    /// identity and revision is retained in the Materialized Resolution Input
    /// Set, reproduced, and compared. Another value of any one of the sixteen
    /// gives another set, an `invocation-stale` invocation, and a replay that
    /// reproduces that invocation's own Shared projection, unlike the
    /// example's. The input's members in another order and spacing give an
    /// equal set, a `fresh` invocation, and the example's replay.
    #[test]
    fn each_specification_member_identity_and_revision_is_retained_and_compared() {
        let resolver = resolver();
        let earlier = example(|inputs| inputs).file(&resolver, true);
        let (shown, set) = resolved(&resolver, &earlier);
        let context = committed().context();
        let staleness = |later: &InvocationInputs<PinnedFixture>| {
            let retained = |invocation| Retained {
                construction: context,
                invocation,
            };
            let root = ConstructionRoot::built_in().unwrap();
            compare(root, &retained(&earlier), &retained(later))
        };
        let reversed = specified(true, |input| {
            *input = input
                .as_object()
                .unwrap()
                .clone()
                .into_iter()
                .rev()
                .collect();
        });
        assert_ne!(
            reversed.specification,
            example(|inputs| inputs).specification
        );
        let later = reversed.file(&resolver, true);
        assert_eq!(staleness(&later), Staleness::Fresh);
        let (_, other) = resolved(&resolver, &later);
        assert!(other == set);
        let (identity, shared, profile) = replays(&resolver, &other);
        assert_eq!(
            (identity, Some(shared)),
            (shown.identity, shown.shared.clone())
        );
        assert_eq!(profile, shown.checked);
        let mut edits = 0;
        for slot in SLOTS {
            for (member, value) in [
                ("identity", "test-only-other-specification"),
                ("revision", "1"),
            ] {
                let raw = specified(false, |input| input[slot][member] = json!(value));
                let later = raw.file(&resolver, true);
                let name = format!("{slot} {member}");
                assert_eq!(staleness(&later), Staleness::InvocationStale, "{name}");
                let (other, edited) = resolved(&resolver, &later);
                assert!(edited != set, "{name}");
                assert!(other.checked.is_none(), "{name}");
                assert_ne!(other.shared, shown.shared, "{name}");
                let (identity, shared, profile) = replays(&resolver, &edited);
                assert_eq!(
                    (identity, Some(shared)),
                    (other.identity, other.shared),
                    "{name}"
                );
                assert_eq!(profile, None, "{name}");
                edits += 1;
            }
        }
        assert_eq!(edits, 16);
    }

    /// D15:2660: by type, a replay holds the identity, the Shared projection,
    /// and the profile, and so no Entry projection, completeness, Entry
    /// terminal state, occurrence, Coverage Decision Evidence, or outcome.
    /// Each pattern names every field, so a field added to either type fails
    /// to compile here. Every record and terminal record is the Shared
    /// domain's, its reserve holds no Shared-unavailability cause, and no
    /// view names the replay's entry.
    #[test]
    fn a_replay_holds_no_entry_data() {
        let resolver = resolver();
        let named = ExactIdentity::replayed();
        let hidden = [named.as_str().to_owned(), hex(named.as_str().as_bytes())];
        for raw in [
            example(|inputs| inputs),
            example(|inputs| inputs).bounded("evidenceMetadata.maxSubjectOccurrences", 1),
        ] {
            let (_, set) = resolved(&resolver, &raw.programmatic(&resolver, false, true));
            let mut storage = ReplayStorage::default();
            let replay = resolver.replay(&set, &mut storage).unwrap().unwrap();
            let Replay {
                identity,
                shared,
                profile,
            } = &replay;
            let _: (&ConstructionIdentity, &Option<Checked<'_, '_>>) = (identity, profile);
            let Projection {
                domain,
                records,
                terminal,
            } = shared;
            assert_eq!(*domain, ReportingDomain::Shared);
            let shared_kind = |kind| ReportingDomain::holding(kind) == ReportingDomain::Shared;
            assert!(records
                .iter()
                .all(|(_, retained)| shared_kind(retained.kind)));
            assert!(terminal.iter().all(|terminal| terminal.cause.is_none()));
            let views: Vec<&Value> = records
                .iter()
                .map(|(_, retained)| &retained.view)
                .chain(terminal.iter().flat_map(|end| [&end.finding, &end.state]))
                .collect();
            assert!(!views.is_empty());
            let text = serde_json::to_string(&views).unwrap();
            assert!(hidden.iter().all(|named| !text.contains(named.as_str())));
        }
    }
}
