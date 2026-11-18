// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's resolver outcome: what profile projection (Phase 14) and
//! reporting finalization (Phase 15) make of one invocation, exactly one
//! checked or blocked outcome (D15:1150-1166, 2142-2143).
//!
//! Every outcome carries the Resolver Construction Identity that construction
//! and the pre-invocation boundary fixed, which no phase creates or changes
//! (D15:1150, 1155, 2147), and one Resolution Evidence bundle: the Entry
//! projection, and the Shared projection once the entry materialized a value,
//! or else `shared: null` with the canonical set of blocking Entry Finding
//! Keys that explains why (D15:1161-1166, 2369). The bundle is complete
//! unless a bound exhausted a domain (D15:1157, 1163, 2408).
//!
//! An outcome is checked exactly when its bundle is complete and the
//! invocation found no blocking Finding. Each result a check lacks waits on a
//! blocking Finding, so a value was then materialized, the whole root
//! admitted, every check evaluated, and every result profile projection needs
//! established (D15:1152, 1157, 1185, 2142, 2369). Only a checked outcome
//! carries the profile view and its Resolved Profile Artifact Bindings; a
//! blocked one carries neither, however many results were established
//! (D15:1153, 1159). Before it decides, reporting finalization verifies what
//! the earlier stages fixed, and the outcome's reserved records fit the
//! mandatory reserve ([`super::finalization`]).
//!
//! The outcome comes in three crate-private stages, which the
//! `profile_evidence_materialize` benchmark measures one at a time
//! (D15:2771, 2775): the Shared projection, which counts the Shared scope's
//! Evidence metadata work and plans the Shared domain's slices; the Entry
//! projection, which then admits the paths of every candidate's Evidence,
//! counts the complete invocation's work, and plans the Entry domain's; and
//! outcome finalization, which orders each domain's records, verifies them,
//! and projects the profile (D15:1851, 2143; plan 025 decision #253).

use crate::construction::identity::ConstructionIdentity;

use super::evaluation::{Evaluation, IssueKey};
use super::evidence_metadata::{self, SharedScope};
use super::exact_resolution::ArtifactBindings;
use super::finalization;
use super::limits::DiagnosticsLimits;
use super::profile_resolution::{Invocation, Invoked, ResolverInvariant, SemanticState};
use super::profile_view::{project, ProfileView};
use super::reporting::{Record, Reporting, ReportingDomain};
use super::reporting_plan::{report_domain, Committed, Reported};
use super::resolution_evidence::{Projection, Terminal};
use super::specification::SupportedSpecifications;
use super::token::{Digest, Specification};

/// One invocation's Resolution Evidence bundle (D15:1161-1166).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolutionEvidence<'e> {
    /// The Entry projection, which every bundle has.
    pub(crate) entry: Projection<'e>,
    /// The Shared projection once the entry materialized a value; without
    /// one, `shared: null` (D15:1165).
    pub(crate) shared: Option<Projection<'e>>,
}

impl<'e> ResolutionEvidence<'e> {
    /// The terminal state of each exhausted domain, Entry before Shared
    /// (D15:2408).
    pub(crate) fn terminal_states(&self) -> impl Iterator<Item = &Terminal<'e>> {
        std::iter::once(&self.entry)
            .chain(&self.shared)
            .filter_map(|projection| projection.terminal.as_ref())
    }

    /// Whether the bundle is complete: no bound exhausted a domain, the only
    /// way a revision-`"0"` bundle is incomplete (D15:1157, 1163, 2408).
    pub(crate) fn complete(&self) -> bool {
        self.terminal_states().next().is_none()
    }

    /// The canonical shared-unavailability cause set, in Finding order: the
    /// blocking Entry Finding Keys that the Entry projection's cause records
    /// name, and last, when the Entry reserve holds the cause that names it,
    /// the Entry terminal Finding Key (D15:1165, 2014, 2408). Only an
    /// invocation without a Shared projection has either, and it always has
    /// one or the other, so the set is empty exactly when the Shared
    /// projection is present.
    pub(crate) fn unavailable(&self) -> Vec<IssueKey> {
        let reserved = self
            .entry
            .terminal
            .as_ref()
            .and_then(|terminal| terminal.cause.as_ref())
            .map(|_| IssueKey::terminal(ReportingDomain::Entry.reason()));
        self.entry
            .records
            .iter()
            .filter_map(|(_, retained)| match retained.record {
                Record::UnavailabilityCause(key) => Some(key.clone()),
                _ => None,
            })
            .chain(reserved)
            .collect()
    }
}

/// A checked outcome's profile: its view, which profile equality compares
/// and the profile semantic digest covers, and its Resolved Profile Artifact
/// Bindings (D15:1152, 2142).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Checked<'e, 's> {
    pub(crate) profile: ProfileView,
    pub(crate) bindings: &'e ArtifactBindings<'s>,
}

/// One invocation's outcome: checked when it carries a profile, and
/// otherwise blocked (D15:1150-1153).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Outcome<'e, 's> {
    /// The Resolver Construction Identity both outcomes carry (D15:1150,
    /// 1155).
    pub(crate) identity: ConstructionIdentity,
    pub(crate) checked: Option<Checked<'e, 's>>,
    pub(crate) evidence: ResolutionEvidence<'e>,
}

/// What the Shared projection stage leaves (D15:2775): the reporting of the
/// Shared domain, the Evidence metadata work of the Shared scope, and the
/// slices the Shared domain committed.
#[derive(Debug)]
pub(crate) struct SharedStage<'e, 'a> {
    reporting: Reporting<'e, 'a>,
    scope: SharedScope,
    shared: Committed<'e>,
}

/// What the Entry projection stage leaves (D15:2775): the reporting of both
/// domains and the slices each committed.
#[derive(Debug)]
pub(crate) struct EntryStage<'e, 'a> {
    reporting: Reporting<'e, 'a>,
    reported: Reported<'e>,
}

/// The outcome of one file entry's invocation, which `evaluation` recorded
/// and `invoked` left ([`super::profile_resolution::invoke_file`]), under
/// `identity`: its three stages in turn, the Shared projection, the Entry
/// projection, and outcome finalization (D15:2771, 2775; plan 025 decision
/// #253). The subject and evidence tokens `digest` assigns once the
/// resolver's own Evidence metadata work is within the capability's
/// `evidenceMetadata` bounds, and each domain reports under the capability's
/// `policyComparable` bounds until Phase 4 admits a Resource Limit Policy,
/// and under the Policy's from then on (D15:1851, 2391).
///
/// # Errors
///
/// Returns the resolver bug that leaves a record without its view, that
/// reporting finalization finds in what the earlier stages fixed, or that
/// leaves the profile without a result no blocking Finding holds back.
pub(crate) fn outcome<'e, 's, Id: AsRef<str>>(
    evaluation: &'e Evaluation,
    invoked: &'e Invoked<'s, Id>,
    identity: ConstructionIdentity,
    invocation: &Invocation<'_, Id>,
    digest: &impl Digest,
) -> Result<Outcome<'e, 's>, ResolverInvariant> {
    let shared = shared_projection(evaluation, invoked, invocation, digest)?;
    let entry = entry_projection(shared, evaluation, invoked, invocation, digest)?;
    finalize(entry, evaluation, invoked, identity, invocation)
}

/// The Shared projection stage: the Evidence metadata work of the Shared
/// scope of the invocation `evaluation` recorded and `invoked` left, whose
/// subject tokens `digest` assigns once it is within the capability's
/// `evidenceMetadata` bounds, and then the slices of the Shared domain
/// (D15:1851, 2771, 2775; plan 025 decision #253). Without a value no unit
/// reports into the Shared domain, which then commits nothing (D15:2369).
///
/// # Errors
///
/// Returns the resolver bug that leaves a record without its view.
pub(crate) fn shared_projection<'e, 'a, Id: AsRef<str>>(
    evaluation: &'e Evaluation,
    invoked: &'e Invoked<'_, Id>,
    invocation: &Invocation<'a, Id>,
    digest: &impl Digest,
) -> Result<SharedStage<'e, 'a>, ResolverInvariant> {
    let specification = specification(invocation);
    let capability = invocation.capability;
    let scope = evidence_metadata::shared_scope(
        evaluation,
        specification,
        &capability.bootstrap_only().evidence_metadata,
        digest,
    );
    let reporting = Reporting::shared(
        evaluation,
        specification,
        (invoked.state.as_ref().err(), &invoked.entry),
        &scope,
        digest,
    );
    let shared = report_domain(
        &reporting,
        ReportingDomain::Shared,
        &capability.policy_comparable().diagnostics,
        policy(invoked),
    )?;
    Ok(SharedStage {
        reporting,
        scope,
        shared,
    })
}

/// The Entry projection stage, after the Shared projection stage `stage`:
/// path admission and the Evidence metadata work of the complete invocation,
/// whose remaining subject tokens and evidence tokens `digest` assigns once
/// it is within the capability's `evidenceMetadata` bounds, and then the
/// slices of the Entry domain (D15:1851, 2771, 2775; plan 025 decision
/// #253).
///
/// # Errors
///
/// Returns the resolver bug that leaves a record without its view.
pub(crate) fn entry_projection<'e, 'a, Id: AsRef<str>>(
    stage: SharedStage<'e, 'a>,
    evaluation: &'e Evaluation,
    invoked: &'e Invoked<'_, Id>,
    invocation: &Invocation<'a, Id>,
    digest: &impl Digest,
) -> Result<EntryStage<'e, 'a>, ResolverInvariant> {
    let SharedStage {
        reporting,
        scope,
        shared,
    } = stage;
    let capability = invocation.capability;
    let admission = evidence_metadata::invocation(
        evaluation,
        &invoked.entry,
        specification(invocation),
        &capability.bootstrap_only().evidence_metadata,
        digest,
        scope,
    );
    let reporting = reporting.complete(admission, digest);
    let entry = report_domain(
        &reporting,
        ReportingDomain::Entry,
        &capability.policy_comparable().diagnostics,
        policy(invoked),
    )?;
    Ok(EntryStage {
        reporting,
        reported: Reported { entry, shared },
    })
}

/// Outcome finalization, after the Entry projection stage `stage`, under
/// `identity` (D15:2771, 2775): each domain's projection in canonical order,
/// reporting finalization's verification of what the earlier stages fixed,
/// the profile that Phase 14 projects for a checked outcome, and the
/// mandatory reserve (D15:1150-1166, 2142-2143).
///
/// # Errors
///
/// Returns the resolver bug that leaves a record without its view, that
/// reporting finalization finds in what the earlier stages fixed, or that
/// leaves the profile without a result no blocking Finding holds back.
pub(crate) fn finalize<'e, 's, Id: AsRef<str>>(
    stage: EntryStage<'e, '_>,
    evaluation: &'e Evaluation,
    invoked: &'e Invoked<'s, Id>,
    identity: ConstructionIdentity,
    invocation: &Invocation<'_, Id>,
) -> Result<Outcome<'e, 's>, ResolverInvariant> {
    let EntryStage {
        reporting,
        reported,
    } = stage;
    let state = invoked.state.as_ref();
    let entry = Projection::finalized(&reporting, ReportingDomain::Entry, reported.entry)?;
    // Without a value no unit reports into the Shared domain, whose
    // projection is then absent (D15:2369).
    let shared = state
        .is_ok()
        .then(|| Projection::finalized(&reporting, ReportingDomain::Shared, reported.shared))
        .transpose()?;
    let evidence = ResolutionEvidence { entry, shared };
    finalization::verify(evaluation, &evidence)?;
    let checked = match state {
        Ok(state) if evidence.complete() && evaluation.issues().next().is_none() => {
            Some(checked(state, invocation.supported)?)
        }
        _ => None,
    };
    let outcome = Outcome {
        identity,
        checked,
        evidence,
    };
    finalization::reserve(
        &outcome,
        &invocation.capability.bootstrap_only().mandatory_reserve,
    )?;
    Ok(outcome)
}

/// The Profile Specification whose tokens `invocation`'s records carry: the
/// Active Project Profile Specification's.
fn specification<'a, Id: AsRef<str>>(invocation: &Invocation<'a, Id>) -> Specification<'a> {
    let profile = &invocation.supported.project_profile().specification;
    Specification {
        identity: profile.identity.as_ref(),
        revision: profile.revision.as_ref(),
    }
}

/// The diagnostic bounds of the Resource Limit Policy that Phase 4 admitted
/// in `invoked`, if it admitted one.
fn policy<'e, Id>(invoked: &'e Invoked<'_, Id>) -> Option<&'e DiagnosticsLimits> {
    invoked
        .state
        .as_ref()
        .ok()
        .and_then(|state| state.policy.diagnostics.as_ref())
}

/// The profile of `state` when no blocking Finding holds a result back: its
/// view and its bindings (D15:1152, 2142).
///
/// # Errors
///
/// Returns the resolver bug of a result missing although no blocking
/// Finding holds it back.
fn checked<'e, 's, Id: AsRef<str>>(
    state: &'e SemanticState<'s, Id>,
    supported: &SupportedSpecifications<Id>,
) -> Result<Checked<'e, 's>, ResolverInvariant> {
    let profile = project(state, supported).map_err(ResolverInvariant::Unprojected)?;
    let bindings = state
        .bindings
        .as_ref()
        .map_err(|causes| ResolverInvariant::Unprojected(causes.clone()))?;
    Ok(Checked { profile, bindings })
}

// `finalization`'s tests take the same inputs.
#[cfg(test)]
pub(crate) mod tests {
    use std::sync::{Arc, OnceLock};

    use serde_json::{json, Value};

    use crate::construction::flow::construct;
    use crate::construction::flow::testing::committed;
    use crate::construction::identity::{self, ConstructionIdentity};
    use crate::construction::registry::check::RecordKind;
    use crate::construction::root::ConstructionRoot;
    use crate::input_limits::Bound;
    use crate::resolver::dependency::{self, SemanticGroup};
    use crate::resolver::evaluation::{
        Code, ExhaustedBound, IssueKey, Phase, Reason, Status, Unit,
    };
    use crate::resolver::evidence_metadata::fixtures::with;
    use crate::resolver::evidence_metadata::MetadataBound;
    use crate::resolver::limits::{BoundId, Group, ResolutionLimits};
    use crate::resolver::profile_resolution::tests::{Inputs, ACTIVE};
    use crate::resolver::reporting::{Record, ReportingDomain};
    use crate::resolver::resolution_evidence::Projection;

    /// The Resolver Construction Identity of the committed construction input
    /// under the built-in root.
    pub(crate) fn identity() -> ConstructionIdentity {
        static IDENTITY: OnceLock<ConstructionIdentity> = OnceLock::new();
        *IDENTITY.get_or_init(|| {
            let root = ConstructionRoot::built_in().unwrap();
            identity::derive(root, &construct(root, &committed()).unwrap())
        })
    }

    pub(crate) fn entry(name: &'static str) -> BoundId {
        BoundId {
            group: Group::DiagnosticsEntry,
            name,
        }
    }

    pub(crate) fn shared(name: &'static str) -> BoundId {
        BoundId {
            group: Group::DiagnosticsShared,
            name,
        }
    }

    /// A value no diagnostic count here reaches.
    const ROOM: u64 = 1 << 40;

    /// Every diagnostic bound of both domains at `ROOM`, but each of `tight`
    /// at its limit.
    fn diagnostics(tight: &[(BoundId, u64)]) -> Vec<(BoundId, u64)> {
        ResolutionLimits::from_bounds(|_| Bound::new(1))
            .unwrap()
            .bounds()
            .into_iter()
            .map(|(id, _)| id)
            .filter(|id| matches!(id.group, Group::DiagnosticsEntry | Group::DiagnosticsShared))
            .map(|id| {
                tight
                    .iter()
                    .find(|(bound, _)| *bound == id)
                    .copied()
                    .unwrap_or((id, ROOM))
            })
            .collect()
    }

    /// `inputs` with `diagnostics` of `capability` in the capability and of
    /// `policy` in the policy: the policy's own values leave too little room
    /// for the records of 015's example.
    pub(crate) fn reporting(
        inputs: &Inputs,
        capability: &[(BoundId, u64)],
        policy: &[(BoundId, u64)],
    ) -> Inputs {
        inputs.bounding(&diagnostics(capability), &diagnostics(policy))
    }

    /// A file that is no JSON.
    pub(crate) fn no_json() -> Arc<[u8]> {
        Arc::from(&b"{"[..])
    }

    /// The views of `projection`'s records of `kind`, in its order.
    fn views<'p>(projection: &'p Projection<'_>, kind: RecordKind) -> Vec<&'p Value> {
        projection
            .records
            .iter()
            .filter(|(_, retained)| retained.kind == kind)
            .map(|(_, retained)| &retained.view)
            .collect()
    }

    /// The keys of `projection`'s Findings, in its order.
    fn findings<'e>(projection: &Projection<'e>) -> Vec<&'e IssueKey> {
        projection
            .records
            .iter()
            .filter_map(|(_, retained)| match retained.record {
                Record::Finding(finding) => Some(finding.key),
                _ => None,
            })
            .collect()
    }

    /// `complete`'s profile without its project default and with a target
    /// locale that is no string. Without the default, the project's locale
    /// occurrences, which count it, wait on its Finding, and so does every
    /// check of a locale spelling, a target's among them (D15:1907; plan 025
    /// interpretations #219 and #252): structural admission finds the target
    /// locale on its own.
    pub(crate) fn blocked() -> Inputs {
        let mut inputs = Inputs::complete();
        let declaration = inputs.declaration();
        declaration
            .as_object_mut()
            .unwrap()
            .remove("defaultRequestedLocale");
        declaration["targetProfiles"]["server"]["requestedLocales"] = json!(["en", 42]);
        reporting(&inputs, &[], &[])
    }

    /// 015's example with every input it needs is checked: the outcome
    /// carries the identity, a complete bundle with both projections and no
    /// Finding, and the profile view Phase 14 projects with the invocation's
    /// bindings (D15:1150, 1152, 3441).
    #[test]
    fn the_example_is_checked_with_its_identity_and_a_complete_bundle() {
        let inputs = reporting(&Inputs::complete(), &[], &[]);
        let view = inputs.view().unwrap();
        inputs.outcome(None, identity(), |evaluation, invoked, outcome| {
            assert_eq!(outcome.identity, identity());
            let evidence = &outcome.evidence;
            assert!(evidence.complete());
            assert!(evidence.unavailable().is_empty());
            assert!(evaluation.findings().is_empty());
            let shared = evidence.shared.as_ref().unwrap();
            assert!(findings(&evidence.entry).is_empty());
            assert!(findings(shared).is_empty());
            let checked = outcome.checked.unwrap();
            assert_eq!(checked.profile, view);
            let state = invoked.state.as_ref().unwrap();
            assert!(std::ptr::eq(
                checked.bindings,
                state.bindings.as_ref().unwrap()
            ));
        });
    }

    /// A non-canonical spelling is a warning that suggests its canonical
    /// replacement: the outcome stays checked, the Shared projection holding
    /// the Finding and the Entry projection its occurrence (D15:1157, 2308,
    /// 3441).
    #[test]
    fn a_non_canonical_spelling_leaves_the_outcome_checked_with_its_warning() {
        let mut inputs = reporting(&Inputs::complete(), &[], &[]);
        inputs.declaration()["requestedLocales"] = json!(["ja", "EN"]);
        inputs.outcome(None, identity(), |_, _, outcome| {
            assert!(outcome.checked.is_some());
            let evidence = &outcome.evidence;
            assert!(evidence.complete());
            let shared = evidence.shared.as_ref().unwrap();
            let warnings = views(shared, RecordKind::SharedFinding);
            assert_eq!(warnings.len(), 1);
            let warning = warnings[0];
            assert_eq!(
                warning["key"]["code"],
                "project-profile-locale-non-canonical"
            );
            // Its key names the Active Project Profile Specification.
            assert_eq!(
                warning["key"]["specification"],
                json!({ "identity": ACTIVE.identity, "revision": ACTIVE.revision })
            );
            assert_eq!(warning["blocking"], false);
            assert_eq!(
                warning["suggestion"],
                json!({ "canonical-locale-replacement": "en" })
            );
            let occurrences = views(&evidence.entry, RecordKind::FindingOccurrence);
            assert_eq!(occurrences.len(), 1);
            assert_eq!(occurrences[0]["finding"], warning["key"]);
        });
    }

    /// A file that is no JSON is blocked without a Shared projection: its
    /// complete bundle holds the blocking Entry Findings, and their keys are
    /// the shared-unavailability causes (D15:1153, 1165, 2369, 3440).
    #[test]
    fn a_file_that_is_no_json_is_blocked_without_a_shared_projection() {
        reporting(&Inputs::complete(), &[], &[]).outcome(
            Some(no_json()),
            identity(),
            |evaluation, invoked, outcome| {
                assert_eq!(outcome.identity, identity());
                assert!(outcome.checked.is_none());
                let evidence = &outcome.evidence;
                assert!(evidence.shared.is_none());
                assert!(evidence.complete());
                let causes: Vec<IssueKey> = invoked
                    .state
                    .as_ref()
                    .unwrap_err()
                    .keys()
                    .cloned()
                    .collect();
                assert!(!causes.is_empty());
                assert_eq!(evidence.unavailable(), causes);
                let blocking: Vec<&IssueKey> = evaluation.issues().collect();
                assert_eq!(findings(&evidence.entry), blocking);
                assert!(causes.iter().all(|cause| blocking.contains(&cause)));
            },
        );
    }

    /// A Phase 0 Finding blocks the outcome beside a complete Shared
    /// projection, in which the owner of the group it leaves unestablished
    /// is not evaluated, its status naming the Entry-owned Phase 0 Finding as
    /// its cause (D15:1125, 2145, 2369, 2516; plan 025 interpretation #257).
    #[test]
    fn the_shared_projection_names_a_phase_0_finding_as_a_cause() {
        let mut inputs = Inputs::complete();
        inputs.unasserted();
        reporting(&inputs, &[], &[]).outcome(None, identity(), |evaluation, _, outcome| {
            assert!(outcome.checked.is_none());
            let evidence = &outcome.evidence;
            assert!(evidence.complete());
            let blocking: Vec<&IssueKey> = evaluation.issues().collect();
            let [phase_0] = blocking[..] else {
                panic!("{blocking:?}");
            };
            assert_eq!(phase_0.unit().phase, Phase::ResolverBootstrapAdmission);
            assert_eq!(findings(&evidence.entry), blocking);
            let owner = dependency::unit(SemanticGroup::ProfileSpecification);
            let status =
                evidence
                    .shared
                    .as_ref()
                    .unwrap()
                    .records
                    .iter()
                    .find_map(|(unit, retained)| match retained.record {
                        Record::Status(status) if **unit == owner => Some(status),
                        _ => None,
                    });
            let Some(Status::NotEvaluated(causes)) = status else {
                panic!("{status:?}");
            };
            assert!(causes.keys().eq([phase_0]));
        });
    }

    /// The views of `projection`'s records with their owners and kinds, in
    /// its order, which outlive its evaluation.
    fn owned(projection: &Projection<'_>) -> Vec<(Unit, RecordKind, Value)> {
        projection
            .records
            .iter()
            .map(|(unit, retained)| ((*unit).clone(), retained.kind, retained.view.clone()))
            .collect()
    }

    /// Past a count of the Shared scope, the Shared projection stage plans no
    /// slice, and every domain is exhausted at its first unit, each terminal
    /// Finding naming the bound by its capability path. Past only a count of
    /// the complete invocation, only the Entry domain is, and the Shared
    /// projection is the one within every bound (D15:1851, 2408; plan 025
    /// decisions #229, #231, #247, and #253).
    #[test]
    fn an_evidence_metadata_bound_exhausts_the_domains_of_its_scope() {
        let shared = |unit: &Unit| ReportingDomain::of(unit.phase) == ReportingDomain::Shared;
        let (within, subjects) = reporting(&Inputs::complete(), &[], &[]).outcome(
            None,
            identity(),
            |evaluation, _, outcome| {
                let units = evaluation
                    .statuses()
                    .filter(|(unit, _)| shared(unit))
                    .count();
                (
                    owned(outcome.evidence.shared.as_ref().unwrap()),
                    u64::try_from(units).unwrap(),
                )
            },
        );
        for (bound, limit, every, path) in [
            (
                MetadataBound::SubjectOccurrences,
                1,
                true,
                "bootstrapOnly.evidenceMetadata.maxSubjectOccurrences",
            ),
            (
                MetadataBound::TokenInputs,
                subjects,
                false,
                "bootstrapOnly.evidenceMetadata.maxTokenInputs",
            ),
        ] {
            let mut inputs = reporting(&Inputs::complete(), &[], &[]);
            inputs.metadata(with(bound, limit));
            inputs.outcome(None, identity(), |evaluation, _, outcome| {
                assert!(outcome.checked.is_none(), "{bound:?}");
                let first = |domain: ReportingDomain| {
                    evaluation
                        .statuses()
                        .map(|(unit, _)| unit)
                        .find(|unit| ReportingDomain::of(unit.phase) == domain)
                        .unwrap()
                };
                let evidence = &outcome.evidence;
                let entry = evidence.entry.terminal.as_ref().unwrap();
                assert_eq!(
                    entry.exhausted.bound,
                    ExhaustedBound::Capability(bound.id())
                );
                assert_eq!(entry.exhausted.cursor, first(ReportingDomain::Entry));
                assert_eq!(entry.finding["details"]["bound"], path, "{bound:?}");
                assert!(evidence.entry.records.is_empty(), "{bound:?}");
                let projection = evidence.shared.as_ref().unwrap();
                if every {
                    let terminal = projection.terminal.as_ref().unwrap();
                    assert_eq!(
                        terminal.exhausted.bound,
                        ExhaustedBound::Capability(bound.id())
                    );
                    assert_eq!(terminal.exhausted.cursor, first(ReportingDomain::Shared));
                    assert_eq!(terminal.finding["details"]["bound"], path);
                    assert!(projection.records.is_empty());
                } else {
                    assert!(projection.terminal.is_none(), "{bound:?}");
                    assert_eq!(owned(projection), within, "{bound:?}");
                }
            });
        }
    }

    /// Blocking Findings after materialization block the outcome beside a
    /// complete Shared projection, which holds both Findings and the status of
    /// every unit after entry; those waiting on the missing default are not
    /// evaluated, with its key among their causes (D15:1153, 1185, 2369,
    /// 3442).
    #[test]
    fn findings_after_materialization_block_the_outcome_beside_a_shared_projection() {
        blocked().outcome(None, identity(), |evaluation, _, outcome| {
            assert!(outcome.checked.is_none());
            let evidence = &outcome.evidence;
            assert!(evidence.complete());
            assert!(evidence.unavailable().is_empty());
            let shared = evidence.shared.as_ref().unwrap();
            let blocking: Vec<&IssueKey> = evaluation.issues().collect();
            assert_eq!(findings(shared), blocking);
            let codes: Vec<Code> = blocking.iter().map(|key| key.code()).collect();
            assert!(codes.contains(&Code::RequiredFieldMissing));
            assert!(codes.contains(&Code::FieldTypeInvalid));
            let reported: Vec<&Unit> = shared
                .records
                .iter()
                .filter(|(_, retained)| retained.kind == RecordKind::SharedEvaluationStatus)
                .map(|(owner, _)| *owner)
                .collect();
            let recorded: Vec<&Unit> = evaluation
                .statuses()
                .map(|(unit, _)| unit)
                .filter(|unit| unit.phase > Phase::EntryAdmission)
                .collect();
            assert_eq!(reported, recorded);
            let missing = blocking
                .iter()
                .find(|key| key.code() == Code::RequiredFieldMissing)
                .unwrap();
            assert!(evaluation.statuses().any(|(_, status)| matches!(
                status,
                Status::NotEvaluated(causes) if causes.keys().any(|cause| cause == *missing)
            )));
        });
    }

    /// A domain past a bound of the admitted policy leaves the bundle
    /// incomplete, with one terminal state for each exhausted domain, Entry
    /// before Shared, and the outcome blocked, though no Finding blocks:
    /// the Entry domain past its `maxFindingOccurrences` by the second
    /// warning's occurrence, the Shared one past its `maxRecords` or, at the
    /// second warning, its `maxFindings`, or both. The complete Entry
    /// projection keeps the occurrences of the Shared Findings omitted with
    /// the Shared suffix (D15:1157, 1185, 2408, 3443, 3450).
    #[test]
    fn an_exhausted_domain_blocks_an_outcome_no_finding_blocks() {
        let occurrences = (entry("maxFindingOccurrences"), 1);
        let records = (shared("maxRecords"), 1);
        let findings = (shared("maxFindings"), 1);
        for (tight, domains) in [
            (vec![occurrences], vec!["entry"]),
            (vec![records], vec!["shared"]),
            (vec![findings], vec!["shared"]),
            (vec![occurrences, records], vec!["entry", "shared"]),
        ] {
            let mut inputs = reporting(&Inputs::complete(), &[], &tight);
            inputs.declaration()["requestedLocales"] = json!(["ja", "EN", "EN-us"]);
            inputs.outcome(None, identity(), |evaluation, _, outcome| {
                assert_eq!(evaluation.issues().count(), 0);
                assert_eq!(evaluation.notices().count(), 2);
                assert!(outcome.checked.is_none());
                let evidence = &outcome.evidence;
                assert!(!evidence.complete());
                // Each terminal state names its domain's terminal Finding,
                // whose reason is the domain.
                let states: Vec<(&str, &str)> = evidence
                    .terminal_states()
                    .map(|terminal| {
                        let state = &terminal.state;
                        let reason = &state["cause"]["reason"];
                        (state["domain"].as_str().unwrap(), reason.as_str().unwrap())
                    })
                    .collect();
                assert_eq!(
                    states,
                    domains
                        .iter()
                        .map(|domain| (*domain, *domain))
                        .collect::<Vec<_>>()
                );
                assert!(evidence.unavailable().is_empty());
                // The exhausted Entry domain keeps the first warning's
                // occurrence and omits the second's.
                let kept = if domains.contains(&"entry") { 1 } else { 2 };
                assert_eq!(
                    views(&evidence.entry, RecordKind::FindingOccurrence).len(),
                    kept
                );
            });
        }
    }

    /// An Entry overrun before materialization keeps the reserved terminal
    /// Finding and state and the reserved cause naming the terminal Finding,
    /// and commits no record of the omitted suffix, the cause records of the
    /// Phase 1 Findings among them (D15:2014, 2408, 3455).
    #[test]
    fn an_entry_overrun_without_a_value_reserves_the_cause_of_its_absence() {
        let inputs = reporting(&Inputs::complete(), &[(entry("maxRecords"), 1)], &[]);
        inputs.outcome(Some(no_json()), identity(), |_, _, outcome| {
            assert!(outcome.checked.is_none());
            let evidence = &outcome.evidence;
            assert!(evidence.shared.is_none());
            assert!(!evidence.complete());
            let terminal = evidence.entry.terminal.as_ref().unwrap();
            assert!(terminal.cause.is_some());
            assert_eq!(evidence.unavailable(), [IssueKey::terminal(Reason::Entry)]);
            assert_eq!(evidence.entry.records.len(), 1);
            assert!(evidence
                .entry
                .records
                .iter()
                .all(|(owner, _)| *owner < terminal.exhausted.cursor));
        });
    }

    /// Two declarations that differ only in their Profile IDs have equal
    /// profiles and profile semantic digests (D15:1208).
    #[test]
    fn declarations_differing_only_in_their_profile_ids_have_equal_profiles() {
        let profile = |inputs: &Inputs| {
            inputs.outcome(None, identity(), |_, _, outcome| {
                outcome.checked.unwrap().profile
            })
        };
        let inputs = reporting(&Inputs::complete(), &[], &[]);
        let web = profile(&inputs);
        let shop = profile(&inputs.selecting("shop"));
        assert_eq!(web, shop);
        assert_eq!(
            web.semantic_digest().unwrap(),
            shop.semantic_digest().unwrap()
        );
    }

    /// Reordering every set, object, and the collection changes no Finding
    /// Key, Finding order, status, or cause (D15:3449).
    #[test]
    fn reordering_the_inputs_keeps_the_findings_statuses_and_causes() {
        let reported = |inputs: &Inputs| {
            inputs.outcome(None, identity(), |_, _, outcome| {
                let evidence = &outcome.evidence;
                let entry = [RecordKind::EntryFinding, RecordKind::EntryEvaluationStatus]
                    .into_iter()
                    .flat_map(|kind| views(&evidence.entry, kind));
                let shared = evidence.shared.as_ref().unwrap();
                let shared = [
                    RecordKind::SharedFinding,
                    RecordKind::SharedEvaluationStatus,
                ]
                .into_iter()
                .flat_map(|kind| views(shared, kind));
                entry.chain(shared).cloned().collect::<Vec<Value>>()
            })
        };
        let inputs = blocked();
        let original = reported(&inputs);
        assert!(!original.is_empty());
        assert_eq!(original, reported(&inputs.reordered()));
    }
}
