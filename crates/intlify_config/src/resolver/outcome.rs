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
//! and projects the profile (D15:1851, 2143; plan 025 decision #253). The
//! Shared projection stage reads no entry input, so a replay of a
//! Materialized Resolution Input Set runs it as an invocation does and then
//! finalizes the Shared domain alone ([`replayed`], D15:2659).

use crate::construction::identity::ConstructionIdentity;

use super::evaluation::{Causes, Evaluation, IssueKey};
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
/// Bindings (D15:1152, 2142). A replay carries one too when the shared
/// result admits it, though a replay is no outcome (D15:2659).
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

#[cfg(feature = "benchmark")]
impl<'e> SharedStage<'e, '_> {
    /// The slices the Shared domain committed, and how a slice it refused
    /// exhausted it, if one did.
    pub(crate) const fn committed(&self) -> &Committed<'e> {
        &self.shared
    }
}

/// What the Entry projection stage leaves (D15:2775): the reporting of both
/// domains and the slices each committed.
#[derive(Debug)]
pub(crate) struct EntryStage<'e, 'a> {
    reporting: Reporting<'e, 'a>,
    reported: Reported<'e>,
}

#[cfg(feature = "benchmark")]
impl<'e> EntryStage<'e, '_> {
    /// The slices the Entry domain committed, and how a slice it refused
    /// exhausted it, if one did.
    pub(crate) const fn committed(&self) -> &Committed<'e> {
        &self.reported.entry
    }
}

/// The outcome of one entry's invocation, which `evaluation` recorded and
/// `invoked` left ([`super::profile_resolution::invoke_file`] or
/// [`super::profile_resolution::invoke_programmatic`]), under
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
    shared_stage(evaluation, invoked.state.as_ref(), invocation, digest)
}

/// The Shared projection stage over `state`, the semantic state or the
/// causes of its absence, which reads no entry input (D15:1851, 2659).
fn shared_stage<'e, 'a, Id: AsRef<str>>(
    evaluation: &'e Evaluation,
    state: Result<&'e SemanticState<'_, Id>, &'e Causes>,
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
    let reporting = Reporting::shared(evaluation, specification, state.err(), &scope, digest);
    let shared = report_domain(
        &reporting,
        ReportingDomain::Shared,
        &capability.policy_comparable().diagnostics,
        policy(state.ok()),
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
    let reporting = reporting.complete(&invoked.entry, admission, digest);
    let entry = report_domain(
        &reporting,
        ReportingDomain::Entry,
        &capability.policy_comparable().diagnostics,
        policy(invoked.state.as_ref().ok()),
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

/// What a replay of a Materialized Resolution Input Set reports of the
/// semantic state `state` that `evaluation` recorded under `invocation`
/// (D15:2659): the Shared projection stage as an invocation's, then the
/// Shared projection in canonical order, verified by reporting finalization
/// within the Shared domain, and the profile Phase 14 projects only when the
/// Shared domain is complete and the replayed phases found no blocking
/// Finding. Every check is then evaluated, since each one held back waits on
/// a blocking Finding (D15:2142, 2369). Without an entry there is no Entry
/// projection and no outcome, so neither the bundle's checks nor the
/// mandatory reserve, which holds the outcome envelope, apply (D15:2143,
/// 2518).
///
/// # Errors
///
/// Returns the resolver bug that leaves a record without its view, that
/// reporting finalization finds in the Shared projection, or that leaves the
/// profile without a result no blocking Finding holds back.
pub(crate) fn replayed<'e, 's, Id: AsRef<str>>(
    evaluation: &'e Evaluation,
    state: &'e SemanticState<'s, Id>,
    invocation: &Invocation<'_, Id>,
    digest: &impl Digest,
) -> Result<(Projection<'e>, Option<Checked<'e, 's>>), ResolverInvariant> {
    let stage = shared_stage(evaluation, Ok(state), invocation, digest)?;
    let reporting = stage.reporting.alone(digest);
    let shared = Projection::finalized(&reporting, ReportingDomain::Shared, stage.shared)?;
    finalization::verify_shared(evaluation, &shared)?;
    let admitted = shared.terminal.is_none() && evaluation.issues().next().is_none();
    let profile = admitted
        .then(|| checked(state, invocation.supported))
        .transpose()?;
    Ok((shared, profile))
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
/// in `state`, if it admitted one.
fn policy<'e, Id>(state: Option<&'e SemanticState<'_, Id>>) -> Option<&'e DiagnosticsLimits> {
    state.and_then(|state| state.policy.diagnostics.as_ref())
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
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::{Arc, OnceLock};

    use serde_json::{json, Value};

    use crate::artifacts::envelope::{ExactTuple, FormalMember};
    use crate::construction::flow::construct;
    use crate::construction::flow::testing::committed;
    use crate::construction::identity::{self, ConstructionIdentity};
    use crate::construction::registry::check::RecordKind;
    use crate::construction::root::ConstructionRoot;
    use crate::input_limits::Bound;
    use crate::resolver::call_site_span::fixtures::CALL_SITE;
    use crate::resolver::coverage::{self, CoverageSubject};
    use crate::resolver::dependency::{self, MaterializedSlot, SemanticGroup};
    use crate::resolver::entry::fixtures::FILE;
    use crate::resolver::evaluation::{
        Check, Code, ExhaustedBound, IssueKey, Phase, Reason, Status, Unit,
    };
    use crate::resolver::evidence_metadata::fixtures::with;
    use crate::resolver::evidence_metadata::MetadataBound;
    use crate::resolver::evidence_source::hex;
    use crate::resolver::limits::{BoundId, Group, ResolutionLimits};
    use crate::resolver::profile_resolution::tests::{Inputs, ACTIVE};
    use crate::resolver::profile_view::ProfileView;
    use crate::resolver::reporting::{Record, ReportingDomain};
    use crate::resolver::reporting_plan::Exhausted;
    use crate::resolver::resolution_evidence::Projection;
    use crate::resolver::snapshot_entry::fixtures::ENTRY;
    use crate::resolver::structural_admission::OWNER as MATERIALIZED_OWNER;
    use crate::resolver::subject::{EntryPath, SubjectKind};

    use super::{Checked, Outcome};

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
    /// locale that is no locale. The two are independent: the default's
    /// Finding holds back what inherits it, and Phase 7 still finds the
    /// target's locale (D15:2367, 3446; plan 025 interpretation #252).
    pub(crate) fn blocked() -> Inputs {
        let mut inputs = Inputs::complete();
        let declaration = inputs.declaration();
        declaration
            .as_object_mut()
            .unwrap()
            .remove("defaultRequestedLocale");
        declaration["targetProfiles"]["server"]["requestedLocales"] = json!(["en", "zz"]);
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
    /// evaluated, with its key among their causes. The projection retains only
    /// the dependencies and decision records admission established (D15:1153,
    /// 1185, 2369, 3442, 3444).
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
            assert_eq!(codes, [Code::RequiredFieldMissing, Code::LocaleInvalid]);
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
            // Only what admission established is retained: the materialized
            // dependency of every slot, each established group's semantic
            // dependency while the owners of the others wait, and the
            // decision table's records once the table is complete (D15:1185,
            // 1609, 2516, 3444).
            let retained = |kind| -> Vec<(Unit, Value)> {
                let records = shared.records.iter();
                records
                    .filter(|(_, retained)| retained.kind == kind)
                    .map(|(owner, retained)| ((*owner).clone(), retained.view.clone()))
                    .collect()
            };
            assert_eq!(
                retained(RecordKind::MaterializedDependency),
                MaterializedSlot::ALL.map(|slot| (MATERIALIZED_OWNER, slot.view()))
            );
            let mut groups = SemanticGroup::ALL.to_vec();
            groups.sort_by_key(|group| dependency::unit(*group));
            let (established, waiting): (Vec<_>, Vec<_>) = groups.into_iter().partition(|group| {
                evaluation.status(&dependency::unit(*group)) == Some(&Status::Evaluated)
            });
            assert!(!waiting.is_empty());
            for group in waiting {
                let status = evaluation.status(&dependency::unit(group));
                assert!(matches!(status, Some(Status::NotEvaluated(_))), "{group:?}");
            }
            let semantic: Vec<(Unit, Value)> = established
                .into_iter()
                .map(|group| (dependency::unit(group), group.view()))
                .collect();
            assert_eq!(retained(RecordKind::SemanticDependency), semantic);
            let table = coverage::unit(CoverageSubject::Table, Check::CompleteTable);
            assert_eq!(evaluation.status(&table), Some(&Status::Evaluated));
            let basis = retained(RecordKind::CoverageDecisionBasis);
            assert_eq!(basis.len(), evaluation.decision_basis().len());
            assert!(basis.iter().all(|(owner, _)| *owner == table));
            let mapped = views(&evidence.entry, RecordKind::CoverageDecisionEvidence);
            assert_eq!(mapped.len(), evaluation.decision_evidence().len());
        });
    }

    /// A structural failure of one reference or target no longer holds the
    /// selection back, and the phases after it run, but a profile needs the
    /// whole root, so the outcome stays blocked (D15:2142, 2367; plan 025
    /// decision #342).
    #[test]
    fn a_confirmed_selection_beside_a_structural_failure_stays_blocked() {
        let edits: [fn(&mut Value); 3] = [
            |app| app["policies"]["trust"] = json!(42),
            |app| app["targetProfiles"]["server"]["profile"] = json!(42),
            |app| app["targetProfiles"]["server"]["requestedLocales"] = json!(["en", 42]),
        ];
        for edit in edits {
            let mut inputs = Inputs::complete();
            edit(inputs.declaration());
            reporting(&inputs, &[], &[]).outcome(None, identity(), |_, invoked, outcome| {
                let state = invoked.state.as_ref().unwrap();
                assert!(state.policy.confirmed.is_ok());
                assert!(state.structure.is_err());
                assert!(outcome.checked.is_none());
                assert!(outcome.evidence.complete());
            });
        }
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

    /// Each domain counts every Finding candidate of a unit against its own
    /// `maxFindingOccurrences` before equal Finding Keys merge, whatever the
    /// other domain's count gives. Three spellings no locale has, two of them
    /// equal, are one Shared Finding with three Shared candidates and three
    /// Entry occurrences: either domain holds at three, a Shared `maxFindings`
    /// of one holding the merged Finding, and refuses the Finding's slice at
    /// two, keeping only what the units before it committed, while the other
    /// domain keeps its whole projection (D15:1946, 1962, 1970, 2211, 3450,
    /// 3452).
    #[test]
    fn each_domain_counts_every_finding_candidate_before_equal_keys_merge() {
        let mut inputs = Inputs::complete();
        inputs.declaration()["requestedLocales"] = json!(["ja", "en", "zz", "en-x-brand", "zz"]);
        let (within, key) =
            reporting(&inputs, &[], &[]).outcome(None, identity(), |evaluation, _, outcome| {
                let blocking: Vec<&IssueKey> = evaluation.issues().collect();
                let [key] = blocking[..] else {
                    panic!("{blocking:?}");
                };
                assert_eq!(key.code(), Code::LocaleInvalid);
                let evidence = &outcome.evidence;
                assert!(evidence.complete());
                assert_eq!(findings(evidence.shared.as_ref().unwrap()), [key]);
                let occurrences = views(&evidence.entry, RecordKind::FindingOccurrence);
                assert_eq!(occurrences.len(), 3);
                (shown(&outcome), key.clone())
            });
        let (shared_candidates, entry_candidates) = (
            shared("maxFindingOccurrences"),
            entry("maxFindingOccurrences"),
        );
        let merged = (shared("maxFindings"), 1);
        for (tight, ended) in [
            (
                vec![(shared_candidates, 3), merged, (entry_candidates, 3)],
                vec![],
            ),
            (
                vec![(shared_candidates, 2), (entry_candidates, 3)],
                vec![shared_candidates],
            ),
            (
                vec![(shared_candidates, 3), (entry_candidates, 2)],
                vec![entry_candidates],
            ),
            (
                vec![(shared_candidates, 2), (entry_candidates, 2)],
                vec![entry_candidates, shared_candidates],
            ),
        ] {
            reporting(&inputs, &[], &tight).outcome(None, identity(), |_, _, outcome| {
                assert!(outcome.checked.is_none());
                let exhausted: Vec<Exhausted<'_>> = outcome
                    .evidence
                    .terminal_states()
                    .map(|terminal| terminal.exhausted)
                    .collect();
                let expected: Vec<Exhausted<'_>> = ended
                    .iter()
                    .map(|bound| Exhausted {
                        cursor: key.unit(),
                        bound: ExhaustedBound::Domain(*bound),
                    })
                    .collect();
                assert_eq!(exhausted, expected, "{tight:?}");
                // An exhausted domain keeps what the units before the
                // Finding's committed, and the other its whole projection.
                let kept = |records: &Owned, group| -> Owned {
                    let before = |(owner, ..): &&(Unit, RecordKind, Value)| owner < key.unit();
                    let cut = ended.iter().any(|bound| bound.group == group);
                    let records = records.iter();
                    records
                        .filter(|record| !cut || before(record))
                        .cloned()
                        .collect()
                };
                let bounded = shown(&outcome);
                let entry = kept(&within.entry, Group::DiagnosticsEntry);
                assert_eq!(unrevised(&bounded.entry), unrevised(&entry), "{tight:?}");
                let (records, _) = bounded.shared.unwrap();
                let (all, _) = within.shared.as_ref().unwrap();
                assert_eq!(records, kept(all, Group::DiagnosticsShared), "{tight:?}");
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

    /// What no order of an unordered collection may change: the Shared
    /// projection whole, whose Findings and statuses carry the Redacted
    /// subject tokens and suppression causes, the Entry Findings, statuses,
    /// and unavailability causes, and how many Entry records of each kind
    /// each unit owns. An Entry locator, an occurrence key, and the source's
    /// revision may change, so no other part of an Entry record is compared
    /// (D15:2389, 3353).
    #[derive(Debug, PartialEq)]
    struct Unmoved {
        shared: Option<(Owned, Option<(Value, Value)>)>,
        entry: Owned,
        owners: Vec<(Unit, RecordKind)>,
    }

    fn unmoved(outcome: &Outcome<'_, '_>) -> Unmoved {
        let shown = shown(outcome);
        // Index records order by their Evidence, which a reordering moves,
        // so the owners are compared as a multiset.
        let mut owners: Vec<(Unit, RecordKind)> = shown
            .entry
            .iter()
            .map(|(unit, kind, _)| (unit.clone(), *kind))
            .collect();
        owners.sort_by(|(left, one), (right, other)| (left, one.id()).cmp(&(right, other.id())));
        let kept = [
            RecordKind::EntryFinding,
            RecordKind::EntryEvaluationStatus,
            RecordKind::SharedUnavailabilityCause,
        ];
        let entry = shown
            .entry
            .into_iter()
            .filter(|(_, kind, _)| kept.contains(kind))
            .collect();
        Unmoved {
            shared: shown.shared,
            entry,
            owners,
        }
    }

    /// Reversing every schema-declared unordered collection, whatever its
    /// members, keeps every Logical subject in order, every Redacted subject
    /// token, the Shared Findings and statuses in their order, the suppression
    /// causes, and the owner of every Evidence record: the requested locales,
    /// a target's, the Profile, Target, and Group ID maps, a group's members
    /// and relations, the coverage rules and their selectors, the alias and
    /// fallback maps, and the artifact collection, each holding invalid or
    /// not yet identifiable members (D15:2184, 2389, 3353).
    #[test]
    fn permuting_any_unordered_collection_keeps_subjects_tokens_causes_and_owners() {
        type Edit = fn(&mut Inputs);
        let edits: [Edit; 12] = [
            |_| {},
            |inputs| *inputs = inputs.selecting("shop"),
            |inputs| {
                inputs.declaration()["requestedLocales"] = json!(["ja", "zz", 7, "en", "EN"]);
            },
            |inputs| {
                inputs.declaration()["targetProfiles"]["browser"]["requestedLocales"] =
                    json!(["en", "zz", 7, "ja", "ja"]);
            },
            |inputs| {
                let targets = &mut inputs.declaration()["targetProfiles"];
                targets["Bad Target"] = json!({ "profile": 1 });
                targets["Other Target"] = json!(7);
            },
            |inputs| {
                let groups = &mut inputs.declaration()["deploymentGroups"];
                groups["Bad Group"] = json!({ "members": ["browser"] });
                groups["Other Group"] = json!(7);
            },
            |inputs| {
                inputs.declaration()["deploymentGroups"]["web"]["members"] =
                    json!(["browser", "phantom", 7, "server", "browser"]);
            },
            |inputs| {
                inputs.declaration()["deploymentGroups"]["web"]["hydrationRelations"] = json!([
                    { "server": "server", "client": "browser" },
                    { "server": "absent", "client": "browser" },
                    9,
                    { "server": "server", "client": "browser" },
                ]);
            },
            |inputs| {
                let fr = json!({ "requestedLocales": ["ja"], "mode": "fallback-allowed" });
                inputs.declaration()["coverage"]["rules"] = json!([
                    fr,
                    7,
                    {
                        "requestedLocales": ["ja", "ja", "zz"],
                        "intentSurfaceClasses": ["nav", "nav", "help"],
                        "mode": "fallback-allowed",
                    },
                    fr,
                    { "requestedLocales": [7], "mode": "direct-required" },
                ]);
            },
            |inputs| {
                inputs.declaration()["localeNegotiation"]["aliases"] =
                    json!({ "de": "en", "fr": "ja", "zz": "ja", "iw-IL": 7 });
            },
            |inputs| {
                inputs.declaration()["messageFallback"] =
                    json!({ "ja": ["en"], "zz": ["en"], "fr": 7 });
            },
            |inputs| {
                let first = inputs.submitted().2[0].clone();
                inputs.submit(json!("no envelope"));
                inputs.submit(json!({ "kind": "unsupported" }));
                inputs.submit(first);
            },
        ];
        for (index, edit) in edits.into_iter().enumerate() {
            let mut edited = Inputs::complete();
            edit(&mut edited);
            let inputs = reporting(&edited, &[], &[]);
            let permuted = inputs.permuted();
            assert_ne!(
                inputs.submitted().0.to_string(),
                permuted.submitted().0.to_string()
            );
            let held = inputs.outcome(None, identity(), |_, _, outcome| unmoved(&outcome));
            assert!(held.shared.is_some(), "{index}");
            assert_eq!(
                permuted.outcome(None, identity(), |_, _, outcome| unmoved(&outcome)),
                held,
                "{index}"
            );
        }
    }

    /// Every Finding of the outcome of `inputs` as its view, the Entry domain
    /// first, in 015's order.
    fn ordered_findings(inputs: &Inputs) -> Vec<Value> {
        inputs.outcome(None, identity(), |_, _, outcome| {
            let shown = shown(&outcome);
            let mut found: Vec<Value> = entry_views(&shown, RecordKind::EntryFinding)
                .into_iter()
                .cloned()
                .collect();
            if let Some((records, _)) = &shown.shared {
                found.extend(
                    records
                        .iter()
                        .filter(|(_, kind, _)| *kind == RecordKind::SharedFinding)
                        .map(|(_, _, view)| view.clone()),
                );
            }
            found
        })
    }

    /// Several independently provable resource violations give the same
    /// ordered Findings, each with its bound, limit, and actual, whatever
    /// order the input is written in: past two root-value bounds of the
    /// capability in Phase 1, past three of the collection's bounds in Phase
    /// 4 whatever order its members were submitted in, and past bounds of the
    /// locales, coverage rules, and group members in the phases after it
    /// (D15:1999, 2006, 2008, 3351).
    #[test]
    fn several_resource_violations_give_the_same_findings_in_any_order() {
        let bound = |group, name| BoundId { group, name };
        let input = |name| bound(Group::ConfigurationInput, name);
        let artifact = |name| bound(Group::ArtifactAdmission, name);
        let locale = |name| bound(Group::LocaleResolution, name);
        let grouping = |name| bound(Group::TargetGrouping, name);
        let complete = Inputs::complete();
        let policy = [
            diagnostics(&[]),
            vec![
                (locale("maxRequestedLocales"), 1),
                (locale("maxCoverageRules"), 1),
                (grouping("maxMembersPerGroup"), 1),
                (grouping("maxMembershipOccurrences"), 1),
            ],
        ]
        .concat();
        let cases: [(Inputs, &[&str]); 3] = [
            (
                reporting(
                    &complete.bounding(
                        &[(input("maxNodes"), 10), (input("maxSingleStringBytes"), 2)],
                        &[],
                    ),
                    &[],
                    &[],
                ),
                &["maxNodes", "maxSingleStringBytes"],
            ),
            (
                reporting(
                    &complete.bounding(
                        &[
                            (artifact("maxArtifacts"), 2),
                            (artifact("maxPolicyArtifacts"), 1),
                            (artifact("maxSingleArtifactCanonicalBytes"), 2),
                        ],
                        &[],
                    ),
                    &[],
                    &[],
                ),
                // The third bound is checked of each of the nine artifacts.
                &[
                    "maxArtifacts",
                    "maxPolicyArtifacts",
                    "maxSingleArtifactCanonicalBytes",
                    "maxSingleArtifactCanonicalBytes",
                    "maxSingleArtifactCanonicalBytes",
                    "maxSingleArtifactCanonicalBytes",
                    "maxSingleArtifactCanonicalBytes",
                    "maxSingleArtifactCanonicalBytes",
                    "maxSingleArtifactCanonicalBytes",
                    "maxSingleArtifactCanonicalBytes",
                    "maxSingleArtifactCanonicalBytes",
                ],
            ),
            (
                complete.bounding(&diagnostics(&[]), &policy),
                &[
                    "maxRequestedLocales",
                    "maxCoverageRules",
                    "maxMembershipOccurrences",
                    "maxMembersPerGroup",
                ],
            ),
        ];
        for (inputs, bounds) in cases {
            let findings = ordered_findings(&inputs);
            let named: Vec<&str> = findings
                .iter()
                .map(|finding| finding["details"]["bound"].as_str().unwrap())
                .collect();
            assert_eq!(named, bounds);
            for finding in &findings {
                assert_eq!(
                    finding["key"]["code"],
                    "project-profile-resource-limit-exceeded"
                );
                assert!(finding["details"]["limit"].is_string(), "{finding}");
            }
            assert_eq!(ordered_findings(&inputs.permuted()), findings);
        }
    }

    /// A projection's records with their owners and kinds, as `owned` gives
    /// them.
    pub(crate) type Owned = Vec<(Unit, RecordKind, Value)>;

    /// What an outcome shows of its invocation, outliving it: its identity,
    /// its checked profile view with its bindings' tuples and members, each
    /// projection's records, the Shared one's terminal Finding and state when
    /// a bound exhausted it, and the shared-unavailability causes.
    #[derive(Debug, PartialEq)]
    pub(crate) struct Shown {
        pub(crate) identity: ConstructionIdentity,
        pub(crate) checked: Option<(ProfileView, Vec<(ExactTuple, FormalMember)>)>,
        pub(crate) shared: Option<(Owned, Option<(Value, Value)>)>,
        pub(crate) entry: Owned,
        pub(crate) unavailable: Vec<IssueKey>,
    }

    pub(crate) fn shown(outcome: &Outcome<'_, '_>) -> Shown {
        let evidence = &outcome.evidence;
        Shown {
            identity: outcome.identity,
            checked: outcome.checked.as_ref().map(profile),
            shared: evidence.shared.as_ref().map(projected),
            entry: owned(&evidence.entry),
            unavailable: evidence.unavailable(),
        }
    }

    /// What `checked` shows, outliving it: its view, and its bindings'
    /// tuples and members.
    pub(crate) fn profile(
        checked: &Checked<'_, '_>,
    ) -> (ProfileView, Vec<(ExactTuple, FormalMember)>) {
        let bindings = checked
            .bindings
            .iter()
            .map(|(tuple, artifact)| (tuple.clone(), artifact.member.clone()))
            .collect();
        (checked.profile.clone(), bindings)
    }

    /// What `projection` shows, outliving it: its records, and its terminal
    /// Finding and state when a bound exhausted it.
    pub(crate) fn projected(projection: &Projection<'_>) -> (Owned, Option<(Value, Value)>) {
        let terminal = projection.terminal.as_ref();
        let terminal = terminal.map(|end| (end.finding.clone(), end.state.clone()));
        (owned(projection), terminal)
    }

    /// The views of `shown`'s Entry records of `kind`, in order.
    pub(crate) fn entry_views(shown: &Shown, kind: RecordKind) -> Vec<&Value> {
        shown
            .entry
            .iter()
            .filter(|(_, recorded, _)| *recorded == kind)
            .map(|(_, _, view)| view)
            .collect()
    }

    /// The status view of each unit whose status `shown`'s Entry projection
    /// holds: Phase 0's and Phase 1's.
    fn entry_statuses(shown: &Shown) -> BTreeMap<&Unit, &Value> {
        shown
            .entry
            .iter()
            .filter(|(_, kind, _)| *kind == RecordKind::EntryEvaluationStatus)
            .map(|(unit, _, view)| (unit, view))
            .collect()
    }

    /// The checks of the units `one` holds a status of and `other` does not.
    fn unshared(one: &Shown, other: &Shown) -> Vec<Check> {
        let other = entry_statuses(other);
        entry_statuses(one)
            .into_keys()
            .filter(|unit| !other.contains_key(unit))
            .map(|unit| unit.check)
            .collect()
    }

    /// The complete input's checks that only an entry through `path`
    /// records (Decision 015-202).
    fn own_checks(path: EntryPath) -> Vec<Check> {
        let other = match path {
            EntryPath::File => EntryPath::Programmatic,
            EntryPath::Programmatic => EntryPath::File,
        };
        let checks = SubjectKind::CompleteInput.checks().iter().copied();
        checks.filter(|check| !check.recorded_for(other)).collect()
    }

    /// D15:2129, 3228 (Decision 015-202): two entries of one value record the
    /// same status for each unit both record, and each records its own
    /// Phase 1 checks besides.
    fn assert_entry_statuses_agree(file: &Shown, snapshot: &Shown) {
        let statuses = entry_statuses(snapshot);
        for (unit, view) in entry_statuses(file) {
            if let Some(&other) = statuses.get(unit) {
                assert_eq!(other, view, "{unit:?}");
            }
        }
        assert_eq!(unshared(file, snapshot), own_checks(EntryPath::File));
        assert_eq!(
            unshared(snapshot, file),
            own_checks(EntryPath::Programmatic)
        );
    }

    /// D15:204, 743, 2507, 2516: a Snapshot's entry dependencies name the
    /// file's selector, but for the token of a redacted one, which its origin
    /// keys; under `configuration-programmatic` each source slot and under
    /// `profile-selector-programmatic` the selector's; and beside a sidecar
    /// the `call-site-source` record in its slot's place.
    fn assert_programmatic_dependencies(file: &Shown, snapshot: &Shown, beside: bool) {
        let filed = entry_views(file, RecordKind::EntryDependency);
        let named = entry_views(snapshot, RecordKind::EntryDependency);
        let slots: Vec<&Value> = named.iter().map(|view| &view["slot"]).collect();
        let mut expected = vec!["entry-source", "profile-selector", "origins"];
        if beside {
            expected.insert(1, "call-site-source");
        }
        assert_eq!(slots, expected);
        assert_ne!(named[0]["source"], filed[0]["source"]);
        assert_ne!(named[0]["revision"], filed[0]["revision"]);
        let [.., selector, origins] = named[..] else {
            panic!("{named:?}");
        };
        let (mut selector, mut expected) = (selector.clone(), filed[1].clone());
        entry_free(&mut selector, true);
        entry_free(&mut expected, true);
        assert_eq!(selector, expected);
        let mut programmatic = filed[2].clone();
        let entry = &mut programmatic["origins"];
        entry["entry-source"] = json!("configuration-programmatic");
        entry["profile-selector"] = json!("profile-selector-programmatic");
        if beside {
            entry["call-site-source"] = json!("configuration-programmatic");
        }
        assert_eq!(origins, &programmatic);
    }

    /// D15:2145, 3228: a file and a Programmatic Entry Snapshot of its value,
    /// without a sidecar and beside one mirroring the file, have the same
    /// construction identity, checked profile and bindings, and Shared
    /// projection, every Finding Key in order, status, cause, Coverage
    /// Decision Basis, semantic and materialized dependency, and terminal
    /// state included, whether the outcome is checked, blocked after
    /// materialization or by Phase 0, or incomplete past a Shared bound;
    /// their Entry projections differ only in Phase 1's own checks and in the
    /// entry dependencies.
    #[test]
    fn a_file_and_a_snapshot_of_its_value_share_their_resolution() {
        let mut unasserted = Inputs::complete();
        unasserted.unasserted();
        for (name, inputs) in [
            ("checked", reporting(&Inputs::complete(), &[], &[])),
            ("blocked", blocked()),
            ("unasserted", reporting(&unasserted, &[], &[])),
            (
                "exhausted",
                reporting(&Inputs::complete(), &[], &[(shared("maxRecords"), 1)]),
            ),
        ] {
            let file = inputs.outcome(None, identity(), |_, _, outcome| shown(&outcome));
            assert_eq!(file.checked.is_some(), name == "checked");
            let (_, terminal) = file.shared.as_ref().unwrap();
            assert_eq!(terminal.is_some(), name == "exhausted");
            let mirror = inputs.mirror();
            for call_site in [None, Some(&mirror)] {
                let snapshot =
                    inputs.programmatic_outcome(call_site, identity(), |_, _, outcome| {
                        shown(&outcome)
                    });
                assert_eq!(snapshot.identity, file.identity, "{name}");
                assert_eq!(snapshot.checked, file.checked, "{name}");
                assert_eq!(snapshot.shared, file.shared, "{name}");
                assert_eq!(snapshot.unavailable, file.unavailable, "{name}");
                assert_eq!(
                    entry_views(&snapshot, RecordKind::EntryFinding),
                    entry_views(&file, RecordKind::EntryFinding),
                    "{name}"
                );
                assert_entry_statuses_agree(&file, &snapshot);
                assert_programmatic_dependencies(&file, &snapshot, call_site.is_some());
            }
        }
    }

    /// `records` without the revision of each entry dependency: a bound set
    /// in the selected Policy changes its digest, which the configuration's
    /// reference names, and so the entry's revision.
    fn unrevised(records: &Owned) -> Owned {
        let mut records = records.clone();
        for (_, kind, view) in &mut records {
            if *kind == RecordKind::EntryDependency {
                view.as_object_mut().unwrap().remove("revision");
            }
        }
        records
    }

    /// What an outcome shows, the length of `C` of its Entry records
    /// together, and whether a bound exhausted its Entry domain.
    type Sized = (Shown, u64, bool);

    /// What `outcome` shows, its Entry records' bytes, and its Entry end.
    fn sized(outcome: &Outcome<'_, '_>) -> Sized {
        let entry = &outcome.evidence.entry;
        let charged = entry.records.iter().map(|(_, retained)| retained.bytes);
        (shown(outcome), charged.sum(), entry.terminal.is_some())
    }

    /// D15:2211, 3228, 3456: a file and a Snapshot of its value charge their
    /// Entry projections differently, so an Entry `maxTotalCanonicalBytes` at
    /// the smaller total holds that entry's whole Entry projection and its
    /// profile, and refuses a slice of the other's, while each keeps the
    /// Shared projection both have within every bound. A Shared `maxRecords`
    /// one under the Shared projection's records refuses its last slice under
    /// either entry, and each keeps its whole Entry projection. The outcome
    /// is checked or blocked after materialization.
    #[test]
    fn an_entry_bound_between_a_file_and_its_snapshot_keeps_their_shared_projection() {
        let file =
            |inputs: &Inputs| inputs.outcome(None, identity(), |_, _, outcome| sized(&outcome));
        let snapshot = |inputs: &Inputs| {
            inputs.programmatic_outcome(None, identity(), |_, _, outcome| sized(&outcome))
        };
        for (name, inputs) in [("checked", Inputs::complete()), ("blocked", blocked())] {
            let within = reporting(&inputs, &[], &[]);
            let whole = [file(&within), snapshot(&within)];
            let common = whole[0].0.shared.clone();
            assert_eq!(whole[1].0.shared, common, "{name}");
            assert_ne!(whole[0].1, whole[1].1, "{name}");
            let smaller = whole[0].1.min(whole[1].1);
            let bytes = (entry("maxTotalCanonicalBytes"), smaller);
            let bounded = reporting(&inputs, &[bytes], &[bytes]);
            for ((shown, _, ended), (unbounded, charged, _)) in
                [file(&bounded), snapshot(&bounded)].into_iter().zip(&whole)
            {
                assert_eq!(shown.shared, common, "{name}");
                assert_eq!(ended, *charged > smaller, "{name}");
                if ended {
                    assert!(shown.checked.is_none());
                    assert!(shown.entry.len() < unbounded.entry.len());
                } else {
                    let checked = shown.checked.is_some();
                    assert_eq!(checked, unbounded.checked.is_some(), "{name}");
                    assert_eq!(unrevised(&shown.entry), unrevised(&unbounded.entry));
                }
            }
            let (records, _) = common.as_ref().unwrap();
            let last = (
                shared("maxRecords"),
                crate::artifacts::count(records.len()) - 1,
            );
            let bounded = reporting(&inputs, &[last], &[last]);
            for ((shown, _, ended), (unbounded, ..)) in
                [file(&bounded), snapshot(&bounded)].into_iter().zip(&whole)
            {
                assert!(shown.checked.is_none());
                assert!(shown.shared.unwrap().1.is_some(), "{name}");
                assert!(!ended, "{name}");
                assert_eq!(unrevised(&shown.entry), unrevised(&unbounded.entry));
            }
        }
    }

    /// `view` as either entry names it: each configuration or selector
    /// origin as its slot, without the source it names, every token blanked,
    /// since a redacted member's token is its origin's, and, unless
    /// `spanned`, every span dropped.
    fn entry_free(view: &mut Value, spanned: bool) {
        match view {
            Value::Object(members) => {
                let slot = match members.get("origin").and_then(Value::as_str) {
                    Some("configuration-file" | "configuration-programmatic") => "configuration",
                    Some("profile-selector-cli" | "profile-selector-programmatic") => {
                        "profile-selector"
                    }
                    _ => "",
                };
                if !slot.is_empty() {
                    members.insert("origin".to_owned(), json!(slot));
                    members.remove("source");
                }
                if !spanned {
                    members.remove("span");
                }
                for (name, member) in members.iter_mut() {
                    if name == "token" {
                        *member = Value::Null;
                    } else {
                        entry_free(member, spanned);
                    }
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|item| entry_free(item, spanned)),
            _ => {}
        }
    }

    /// `shown`'s Entry records that retain Evidence, in order, each as its
    /// kind and its view as either entry names it.
    fn located(shown: &Shown, spanned: bool) -> Vec<String> {
        let kinds = [
            RecordKind::FindingOccurrence,
            RecordKind::CoverageDecisionEvidence,
            RecordKind::EvidenceIndex,
        ];
        shown
            .entry
            .iter()
            .filter(|(_, kind, _)| kinds.contains(kind))
            .map(|(_, kind, view)| {
                let mut view = view.clone();
                entry_free(&mut view, spanned);
                format!("{} {view}", kind.id())
            })
            .collect()
    }

    /// The source of each configuration Evidence in `shown`'s evidence index,
    /// which holds every Evidence its records retain, and whether it has a
    /// span.
    fn sourced(shown: &Shown) -> BTreeSet<(String, bool)> {
        entry_views(shown, RecordKind::EvidenceIndex)
            .into_iter()
            .filter(|view| {
                let origin = view["origin"].as_str();
                origin.is_some_and(|origin| origin.starts_with("configuration-"))
            })
            .map(|view| (view["source"].to_string(), view.get("span").is_some()))
            .collect()
    }

    /// D15:747, 790, 796, 800, 3228 (Decision 015-203): beside a sidecar that
    /// mirrors the file, a Snapshot's Entry projection observes every value
    /// and key at the file's span in the call-site source, so its
    /// occurrences, Coverage Decision Evidence, and evidence index are the
    /// file's, in the file's order, but for their origins and sources.
    /// Without a sidecar it names the Programmatic Entry with no span, and
    /// holds the same records once spans are set aside, in an order spans no
    /// longer decide.
    #[test]
    fn a_mirroring_sidecar_gives_a_snapshot_the_files_spans() {
        let named = |identity: &str, spanned| {
            let source = json!({ "safe-opaque": hex(identity.as_bytes()) });
            BTreeSet::from([(source.to_string(), spanned)])
        };
        for inputs in [reporting(&Inputs::complete(), &[], &[]), blocked()] {
            let file = inputs.outcome(None, identity(), |_, _, outcome| shown(&outcome));
            let mirror = inputs.mirror();
            let mirrored =
                inputs.programmatic_outcome(Some(&mirror), identity(), |_, _, outcome| {
                    shown(&outcome)
                });
            let bare =
                inputs.programmatic_outcome(None, identity(), |_, _, outcome| shown(&outcome));
            assert!(!located(&file, true).is_empty());
            assert_eq!(located(&mirrored, true), located(&file, true));
            assert_eq!(sourced(&file), named(FILE, true));
            assert_eq!(sourced(&mirrored), named(CALL_SITE, true));
            assert_eq!(sourced(&bare), named(ENTRY, false));
            let unspanned = |shown: &Shown| {
                let mut views = located(shown, false);
                views.sort();
                views
            };
            assert_eq!(unspanned(&bare), unspanned(&file));
        }
    }

    /// D15:2369, 3228, 3418: a root-value bound that stops either entry
    /// before materialization blocks both without a Shared projection, with
    /// the same Entry Finding Keys and shared-unavailability causes and the
    /// same status for each unit both record; only each entry's own checks,
    /// origin, and location differ, a Snapshot's total observed at the root's
    /// span beside a mirroring sidecar, where a file names none.
    #[test]
    fn a_bound_that_stops_either_entry_gives_the_same_finding_keys() {
        let nodes = BoundId {
            group: Group::ConfigurationInput,
            name: "maxNodes",
        };
        let inputs = reporting(&Inputs::complete().bounding(&[(nodes, 10)], &[]), &[], &[]);
        let primary = |shown: &Shown| {
            let occurrences = entry_views(shown, RecordKind::FindingOccurrence);
            let [occurrence] = occurrences[..] else {
                panic!("{occurrences:?}");
            };
            occurrence["primary"].clone()
        };
        let file = inputs.outcome(None, identity(), |_, _, outcome| shown(&outcome));
        assert_eq!(file.shared, None);
        assert_eq!(
            primary(&file),
            json!({
                "kind": "entry-source",
                "origin": "configuration-file",
                "source": { "safe-opaque": hex(FILE.as_bytes()) },
                "path": [],
            })
        );
        let mirror = inputs.mirror();
        for (call_site, source, span) in [
            (None, ENTRY, Value::Null),
            (
                Some(&mirror),
                CALL_SITE,
                json!({ "start": "0", "end": mirror.source().len().to_string() }),
            ),
        ] {
            let snapshot =
                inputs.programmatic_outcome(call_site, identity(), |evaluation, _, outcome| {
                    let phases: BTreeSet<Phase> =
                        evaluation.statuses().map(|(unit, _)| unit.phase).collect();
                    assert_eq!(
                        phases,
                        BTreeSet::from([Phase::ResolverBootstrapAdmission, Phase::EntryAdmission])
                    );
                    shown(&outcome)
                });
            assert_eq!(snapshot.identity, file.identity);
            assert_eq!(snapshot.shared, None);
            assert_eq!(snapshot.unavailable, file.unavailable);
            let [cause] = &file.unavailable[..] else {
                panic!("{:?}", file.unavailable);
            };
            assert_eq!(cause.unit().check, Check::WithinNodes);
            for kind in [
                RecordKind::EntryFinding,
                RecordKind::SharedUnavailabilityCause,
            ] {
                assert_eq!(entry_views(&snapshot, kind), entry_views(&file, kind));
            }
            assert_entry_statuses_agree(&file, &snapshot);
            assert_programmatic_dependencies(&file, &snapshot, call_site.is_some());
            let mut expected = json!({
                "kind": "entry-source",
                "origin": "configuration-programmatic",
                "source": { "safe-opaque": hex(source.as_bytes()) },
                "path": [],
            });
            if !span.is_null() {
                expected["span"] = span;
            }
            assert_eq!(primary(&snapshot), expected);
        }
    }
}
