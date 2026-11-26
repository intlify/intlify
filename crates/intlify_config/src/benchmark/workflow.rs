// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's complete in-memory invocation, which the `profile_resolve_e2e`
//! pair measures from an already acquired entry through the final outcome
//! (D15:2773).
//!
//! Before any interval, a case holds what a binding submits as the raw values
//! the pre-invocation boundaries read: the Resolver Construction Identity
//! reference, the Common Invocation Input Set with the committed ICU4X Locale
//! Canonicalization Data Artifact, and one file Entry Admission Input Set
//! (D15:2829), or one programmatic entry's, whose formal Snapshot is the
//! value the file's bytes spell (D15:2791). [`complete`] is the production
//! path an end-to-end interval runs on a constructed resolver: the
//! pre-invocation admission of that set, and Phases 0 to 15 over what it
//! admitted, through the outcome (D15:2776).
//! A checked or a blocked outcome is an expected result; a refused set or a
//! resolver defect is a benchmark failure (D15:2831).
//!
//! `file_entry_complete_workflow` and `programmatic_entry_complete_workflow`
//! measure that path on a resolver in one of three states (D15:2792; plan 025 decision #33): constructed before each
//! start mark from the input its preparation constructed from, the one its
//! preparation constructed and every interval reuses, or that one right after
//! it resolved the Blocked recipe's same entry before each start mark, which
//! shows the invocation reset (D15:2752, 3814). Its identity is derived
//! before the start mark. The submission, the slot for the admitted set, and
//! the storage the outcome borrows are made before it; the outcome and the
//! Materialized Resolution Input Set beside it are copied out after the end
//! mark, and what the interval admitted is counted then (D15:2829).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::artifacts::collection::CollectionReport;
use crate::artifacts::envelope::ExactTuple;
use crate::boundary::common::Submitted;
use crate::boundary::entry::{
    FileValue, ProgrammaticValue, SubmittedFileEntry, SubmittedProgrammaticEntry,
};
use crate::boundary::invocation::{
    EntryInputs, InvocationFailures, InvocationInputs, SubmittedEntry, SubmittedInvocation,
};
use crate::boundary::locator::{LocatorCandidate, LocatorVariant};
use crate::boundary::selector::SelectorReport;
use crate::boundary::snapshot::SnapshotReport;
use crate::boundary::step::Component;
use crate::boundary::verification::VerificationAssertion;
use crate::construction::identity::ConstructionIdentity;
use crate::construction::resolver::{build, ConstructedResolver};
use crate::construction::root::ConstructionRoot;
use crate::input_limits::{Bound, InputLimits, ValueCounts};
use crate::locale::icu4x::Icu4xData;
use crate::locale::{EnvelopeReason, VersionedIdentity};
use crate::materialize::materialize_file;
use crate::resolver::artifact_resolution::AdmittedArtifact;
use crate::resolver::entry::fixtures::FILE;
use crate::resolver::exact_resolution::ArtifactBindings;
use crate::resolver::finalization::reserved_bytes;
use crate::resolver::invocation::{OutcomeStorage, Resolution, Resolver};
use crate::resolver::materialized::MaterializedInputs;
use crate::resolver::origin::SafeOriginKind as Kind;
use crate::resolver::outcome::Checked;
use crate::resolver::profile_resolution::ResolverInvariant;
use crate::resolver::profile_view::ProfileView;
use crate::resolver::token::Specification;
use crate::snapshot::SnapshotCounts;

use super::cases::ResolverState;
use super::evidence::outcome::{project, RetainedProjection};
use super::instrument::{Instrument, Interval};
use super::observation::{Frame, Observation};
use super::operation::{Operation, OutputFailure};
use super::quantity::Quantity;

/// The Programmatic Entry identity every programmatic case submits, which its
/// frontend marks non-secret, and its locator, a safe label (D15:205, 729, 744).
const ENTRY: &str = "intlify-programmatic-entry.v0:storefront";
const LOCATOR: &str = "storefront-app";

/// The entry a binding submits. The file is named by [`FILE`], which its
/// owner marks non-secret, with the revision of its bytes and no locator, as
/// the Evidence stages' file is; the programmatic entry by [`ENTRY`] at
/// [`LOCATOR`], its revision the Snapshot's digest, with no call-site
/// sidecar (D15:744, 890; plan 025 decision #316).
#[derive(Clone, PartialEq)]
pub(super) enum RawEntry {
    File {
        bytes: Arc<[u8]>,
        /// The Source-content revision of the bytes.
        revision: String,
    },
    Programmatic {
        snapshot: SnapshotReport,
        revision: String,
    },
}

/// What a binding submits for one entry's invocation, as the raw values the
/// pre-invocation boundaries read, each in its typed slot.
#[derive(Clone, PartialEq)]
pub(super) struct RawInvocation {
    /// The text of the Resolver Construction Identity the set names.
    pub(super) reference: String,
    pub(super) specification: Vec<u8>,
    pub(super) vocabulary: Vec<u8>,
    /// The canonicalization data's manifest, and its payloads by the addresses
    /// it names.
    pub(super) manifest: &'static [u8],
    pub(super) payloads: BTreeMap<String, Arc<[u8]>>,
    /// The collection's members, in submission order.
    pub(super) members: Vec<Vec<u8>>,
    pub(super) verification: Vec<u8>,
    pub(super) capability: Value,
    /// What trusted setup asserts about the Verification Input, beside the
    /// set.
    pub(super) assertion: VerificationAssertion,
    pub(super) entry: RawEntry,
    /// The Profile ID the selector names.
    pub(super) selector: String,
}

impl RawInvocation {
    /// The collection's members as the slot borrows them.
    pub(super) fn members(&self) -> Vec<&[u8]> {
        self.members.iter().map(Vec::as_slice).collect()
    }

    /// The pair whose interval admits this entry.
    pub(super) const fn operation(&self) -> Operation {
        match self.entry {
            RawEntry::File { .. } => Operation::FileEntryCompleteWorkflow,
            RawEntry::Programmatic { .. } => Operation::ProgrammaticEntryCompleteWorkflow,
        }
    }

    /// The set as submitted, each value beside its slot's own Safe Origin Kind,
    /// borrowing these values and `members`, which [`Self::members`] gives. The
    /// data slot's reader is the ICU4X adapter's envelope reader over the
    /// manifest and the payloads.
    pub(super) fn submitted<'a>(
        &'a self,
        members: &'a [&'a [u8]],
    ) -> SubmittedInvocation<'a, impl FnOnce(Bound, Bound) -> Result<Icu4xData, EnvelopeReason> + 'a>
    {
        let own = |slot: Kind| Some(slot.id());
        let (manifest, payloads) = (self.manifest, &self.payloads);
        let common = Submitted {
            specification: Component {
                value: Some(self.specification.as_slice()),
                origin: own(Kind::ProfileResolutionSpecificationInput),
            },
            vocabulary: Component {
                value: Some(self.vocabulary.as_slice()),
                origin: own(Kind::IntentSurfaceClassVocabulary),
            },
            data: Component {
                value: move |bytes, depth| Icu4xData::read(Some(manifest), payloads, bytes, depth),
                origin: own(Kind::LocaleCanonicalizationData),
            },
            collection: Component {
                value: Some(CollectionReport::Formal(members)),
                origin: own(Kind::ProfileResolutionArtifactCollection),
            },
            verification: Component {
                value: Some(self.verification.as_slice()),
                origin: own(Kind::ResourceLimitPolicyVerification),
            },
            capability: Component {
                value: Some(&self.capability),
                origin: own(Kind::ImplementationCapability),
            },
        };
        let selector = |slot| Component {
            value: SelectorReport::String(&self.selector),
            origin: own(slot),
        };
        let entry = match &self.entry {
            RawEntry::File { bytes, revision } => {
                let file = FileValue {
                    bytes: Arc::clone(bytes),
                    identity: Some(FILE),
                    marked: true,
                    revision: Some(revision),
                    locator: None,
                };
                SubmittedEntry::File(SubmittedFileEntry {
                    file: Component {
                        value: file,
                        origin: own(Kind::ConfigurationFile),
                    },
                    selector: selector(Kind::ProfileSelectorCli),
                })
            }
            RawEntry::Programmatic { snapshot, revision } => {
                let programmatic = ProgrammaticValue {
                    snapshot: Some(snapshot),
                    identity: Some(ENTRY),
                    marked: true,
                    revision: Some(revision),
                    locator: Some(LocatorCandidate {
                        variant: LocatorVariant::SafeLabel,
                        payload: LOCATOR,
                        non_secret: true,
                    }),
                    call_site: None,
                };
                SubmittedEntry::Programmatic(SubmittedProgrammaticEntry {
                    programmatic: Component {
                        value: programmatic,
                        origin: own(Kind::ConfigurationProgrammatic),
                    },
                    selector: selector(Kind::ProfileSelectorProgrammatic),
                })
            }
        };
        SubmittedInvocation {
            construction_identity: Some(&self.reference),
            common,
            entry,
        }
    }
}

/// Why a complete invocation yields no outcome. Neither is an expected
/// result: the case's submission is one the resolver admits, and no input
/// causes a resolver defect (D15:2831). The defect is boxed, so that the
/// failure stays small.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum WorkflowFailure {
    Refused(InvocationFailures),
    Invariant(Box<ResolverInvariant>),
}

/// The complete invocation: `resolver` admits `submitted` with trusted setup's
/// `assertion`, keeps the admitted set in `slot`, and resolves it, the outcome
/// borrowing `storage`. Both are the caller's, prepared before the interval,
/// and the outcome and its Materialized Resolution Input Set are returned as
/// the resolver returns them.
pub(super) fn complete<'s, R>(
    (resolver, submitted, assertion, slot, storage): (
        &'s Resolver<'_>,
        SubmittedInvocation<'_, R>,
        &VerificationAssertion,
        &'s mut Option<InvocationInputs<Icu4xData>>,
        &'s mut OutcomeStorage<'s>,
    ),
) -> Result<Resolution<'s, Icu4xData>, WorkflowFailure>
where
    R: FnOnce(Bound, Bound) -> Result<Icu4xData, EnvelopeReason>,
{
    let inputs = resolver
        .admit(submitted, assertion)
        .map_err(WorkflowFailure::Refused)?;
    resolver
        .resolve(slot.insert(inputs), storage)
        .map_err(|invariant| WorkflowFailure::Invariant(Box::new(invariant)))
}

/// What each repetition of a complete invocation case runs before its start
/// mark, by the state of the resolver it invokes (D15:2792, 2829; plan 025
/// decision #33).
#[derive(PartialEq)]
pub(in crate::benchmark) enum Before {
    /// Nothing: the resolver preparation constructed is invoked again.
    Reused,
    /// A resolver is constructed from the input preparation constructed
    /// from, and invoked once.
    Fresh,
    /// The resolver preparation constructed resolves this submission, of the
    /// same entry's Blocked recipe, whose outcome must be blocked (plan 025
    /// decision #316).
    AfterBlocked(Box<RawInvocation>),
}

impl Before {
    pub(in crate::benchmark) const fn state(&self) -> ResolverState {
        match self {
            Self::Reused => ResolverState::Reused,
            Self::Fresh => ResolverState::Fresh,
            Self::AfterBlocked(_) => ResolverState::ReusedAfterBlocked,
        }
    }
}

/// How often the repetitions ran what [`Before`] states: the resolvers they
/// constructed, the times they invoked the one preparation constructed, and
/// the blocked outcomes they resolved first. Each is counted outside the
/// interval, so that a test can hold each state to its own work once per
/// repetition.
#[derive(Debug, Default)]
struct Counts {
    constructed: AtomicU64,
    prepared: AtomicU64,
    blocked: AtomicU64,
}

/// A complete invocation case's inputs: the resolver its preparation
/// constructed once from the committed input under `root`, what a binding
/// submits for the case's recipe on that resolver, and what each repetition
/// runs before its start mark.
pub(in crate::benchmark) struct PreparedWorkflow {
    pub(in crate::benchmark) root: &'static ConstructionRoot,
    pub(in crate::benchmark) constructed: ConstructedResolver,
    pub(in crate::benchmark) raw: RawInvocation,
    pub(in crate::benchmark) before: Before,
    counts: Counts,
}

impl PreparedWorkflow {
    pub(in crate::benchmark) fn new(
        root: &'static ConstructionRoot,
        constructed: ConstructedResolver,
        raw: RawInvocation,
        before: Before,
    ) -> Self {
        Self {
            root,
            constructed,
            raw,
            before,
            counts: Counts::default(),
        }
    }

    pub(in crate::benchmark) const fn state(&self) -> ResolverState {
        self.before.state()
    }

    pub(in crate::benchmark) const fn operation(&self) -> Operation {
        self.raw.operation()
    }

    /// One invocation measured by `instrument`. What the case's state runs
    /// first, the resolver's identity, the submission, the slot, and the
    /// storage are made before the start mark; the outcome is copied out
    /// after the end mark, before the storage it borrows is released, and
    /// then what the slot holds is counted.
    pub(in crate::benchmark) fn interval<I: Instrument>(
        &self,
        instrument: &I,
    ) -> Result<Interval<Resolved>, I::Failure> {
        self.with_resolver(|resolver| {
            let members = self.raw.members();
            let mut slot = None;
            let (quantity, result) = {
                let mut storage = OutcomeStorage::default();
                let submitted = self.raw.submitted(&members);
                let input = (
                    resolver,
                    submitted,
                    &self.raw.assertion,
                    &mut slot,
                    &mut storage,
                );
                let measured = instrument.interval(input, complete)?;
                let retained = measured
                    .output
                    .map(|resolution| RetainedWorkflow::of(&resolution));
                (measured.quantity, retained)
            };
            Ok(Interval {
                quantity,
                output: self.retain(result, slot.as_ref()),
            })
        })
        .ok_or_else(I::unavailable)?
    }

    /// One untimed invocation in the case's state, retained as a timed one
    /// is; `None` when what the state runs first fails.
    pub(in crate::benchmark) fn resolve(&self) -> Option<Resolved> {
        self.with_resolver(|resolver| {
            let members = self.raw.members();
            let mut slot = None;
            let result = {
                let mut storage = OutcomeStorage::default();
                let submitted = self.raw.submitted(&members);
                let assertion = &self.raw.assertion;
                complete((resolver, submitted, assertion, &mut slot, &mut storage))
                    .map(|resolution| RetainedWorkflow::of(&resolution))
            };
            self.retain(result, slot.as_ref())
        })
    }

    /// Runs `invoke` on the resolver this repetition invokes, once what the
    /// case's state runs first has run: a fresh case constructs a resolver
    /// from the input preparation constructed from, and a case reused after
    /// a blocked outcome resolves the Blocked recipe's submission on the
    /// prepared resolver and releases that outcome (D15:2752, 2829). `None`
    /// when that construction is refused, or that outcome is not blocked.
    fn with_resolver<T>(&self, invoke: impl FnOnce(&Resolver<'_>) -> T) -> Option<T> {
        match &self.before {
            Before::Reused => Some(invoke(&self.prepared())),
            Before::Fresh => {
                let constructed = build(self.root, self.constructed.input().clone()).ok()?;
                self.counts.constructed.fetch_add(1, Ordering::Relaxed);
                Some(invoke(&Resolver::new(&constructed, self.root)))
            }
            Before::AfterBlocked(blocked) => {
                let resolver = self.prepared();
                if !blocks(&resolver, blocked) {
                    return None;
                }
                self.counts.blocked.fetch_add(1, Ordering::Relaxed);
                Some(invoke(&resolver))
            }
        }
    }

    /// The resolver preparation constructed, counted as invoked once more.
    fn prepared(&self) -> Resolver<'_> {
        self.counts.prepared.fetch_add(1, Ordering::Relaxed);
        Resolver::new(&self.constructed, self.root)
    }

    fn retain(
        &self,
        result: Result<RetainedWorkflow, WorkflowFailure>,
        admitted: Option<&InvocationInputs<Icu4xData>>,
    ) -> Resolved {
        let supported = self.constructed.supported();
        Resolved {
            result,
            admitted: admitted.and_then(AdmittedCounts::of),
            specification: supported.project_profile().specification.clone(),
        }
    }
}

/// `raw`'s complete invocation on `resolver`, untimed: its outcome as the
/// output retains it, and the Materialized Resolution Input Set beside it;
/// `None` when the set is refused or a resolver defect is found.
pub(in crate::benchmark) fn materialized(
    resolver: &Resolver<'_>,
    raw: &RawInvocation,
) -> Option<(RetainedWorkflow, Option<MaterializedInputs<Icu4xData>>)> {
    let members = raw.members();
    let mut slot = None;
    let mut storage = OutcomeStorage::default();
    let submitted = raw.submitted(&members);
    let resolution =
        complete((resolver, submitted, &raw.assertion, &mut slot, &mut storage)).ok()?;
    Some((RetainedWorkflow::of(&resolution), resolution.materialized))
}

/// Whether `raw`'s complete invocation on `resolver` ends in a blocked
/// outcome. The outcome, the set it admitted, and the storage it borrowed are
/// released before this returns.
fn blocks(resolver: &Resolver<'_>, raw: &RawInvocation) -> bool {
    let members = raw.members();
    let mut slot = None;
    let mut storage = OutcomeStorage::default();
    let submitted = raw.submitted(&members);
    complete((resolver, submitted, &raw.assertion, &mut slot, &mut storage))
        .is_ok_and(|resolution| resolution.outcome.checked.is_none())
}

/// The entry's two counts in its bounds' units: a file's bytes and the
/// tokens its parser visits, or a Snapshot's canonical envelope bytes and the
/// nodes and edges a traversal visits (D15:1795, 1796, 1803, 1805).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::benchmark) enum EntryCounts {
    File { bytes: u64, tokens: u64 },
    Snapshot { envelope: u64, traversal: u64 },
}

/// What the interval admitted, counted after its end mark in the units of
/// the bounds that count it: the entry's counts and its value's once one is
/// complete, as strict file entry counts them under the admitted
/// capability's bounds, or as the admitted Snapshot's marker-free tree
/// counts them, and the collection's members and their `C(member)` bytes
/// (D15:1890, 2130, 2801).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::benchmark) struct AdmittedCounts {
    pub(in crate::benchmark) entry: EntryCounts,
    pub(in crate::benchmark) value: Option<ValueCounts>,
    pub(in crate::benchmark) members: u64,
    pub(in crate::benchmark) member_bytes: u64,
}

impl AdmittedCounts {
    fn of(inputs: &InvocationInputs<Icu4xData>) -> Option<Self> {
        let (entry, value) = match &inputs.entry {
            EntryInputs::File(file) => {
                let capability = &inputs.common.capability;
                let limits = InputLimits {
                    raw: capability.bootstrap_only().raw_input,
                    value: capability.policy_comparable().configuration_input.value,
                };
                let (bytes, tokens, value) = match materialize_file(Arc::clone(&file.bytes), limits)
                {
                    Ok(document) => {
                        let counts = document.counts()?;
                        (counts.file_bytes, counts.parser_tokens, Some(counts.value))
                    }
                    Err(error) => {
                        let progress = error.progress;
                        let value = progress.complete_value.as_deref().copied();
                        (progress.file_bytes, progress.parser_tokens_visited, value)
                    }
                };
                (EntryCounts::File { bytes, tokens }, value)
            }
            EntryInputs::Programmatic(programmatic) => {
                let counts = SnapshotCounts::of(programmatic.snapshot.root());
                let entry = EntryCounts::Snapshot {
                    envelope: counts.envelope_canonical_bytes,
                    traversal: counts.traversal_units,
                };
                (
                    entry,
                    (counts.rejected_markers == 0).then(|| counts.value()),
                )
            }
        };
        let members = inputs.common.collection.members();
        let member_bytes = members.iter().try_fold(0_u64, |total, member| {
            total.checked_add(u64::try_from(member.canonical_body().len()).ok()?)
        })?;
        Some(Self {
            entry,
            value,
            members: u64::try_from(members.len()).ok()?,
            member_bytes,
        })
    }
}

/// A binding as the output retains it: its tuple, and `C(member)` of its
/// member.
pub(in crate::benchmark) type Binding = (ExactTuple, Vec<u8>);

/// The outcome and the set beside it as the output retains them: the
/// outcome's Resolver Construction Identity, both projections, a checked
/// outcome's profile view and its bindings, each tuple with `C(member)` of
/// its member, and the mandatory reserve; and the snapshot-content digest of
/// the Materialized Resolution Input Set's value, when Phase 1 materialized
/// one (D15:1151-1167, 1848, 2658).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::benchmark) struct RetainedWorkflow {
    pub(in crate::benchmark) identity: ConstructionIdentity,
    pub(in crate::benchmark) entry: RetainedProjection,
    pub(in crate::benchmark) shared: Option<RetainedProjection>,
    pub(in crate::benchmark) profile: Option<(ProfileView, Vec<Binding>)>,
    pub(in crate::benchmark) reserve: u64,
    pub(in crate::benchmark) materialized: Option<String>,
}

/// A profile view and its bindings as the output retains them.
pub(in crate::benchmark) fn retained_profile(
    checked: &Checked<'_, '_>,
) -> (ProfileView, Vec<Binding>) {
    let body = |artifact: &AdmittedArtifact<'_>| artifact.member.canonical_body().to_vec();
    let bindings: &ArtifactBindings<'_> = checked.bindings;
    let bindings = bindings
        .iter()
        .map(|(tuple, artifact)| (tuple.clone(), body(artifact)));
    (checked.profile.clone(), bindings.collect())
}

impl RetainedWorkflow {
    fn of(resolution: &Resolution<'_, Icu4xData>) -> Self {
        let outcome = &resolution.outcome;
        Self {
            identity: outcome.identity,
            entry: RetainedProjection::of(&outcome.evidence.entry),
            shared: outcome.evidence.shared.as_ref().map(RetainedProjection::of),
            profile: outcome.checked.as_ref().map(retained_profile),
            reserve: reserved_bytes(outcome),
            materialized: resolution
                .materialized
                .as_ref()
                .map(|set| set.value().digest()),
        }
    }
}

/// A complete invocation's output, retained past the end mark: the outcome
/// and the set beside it, or the benchmark failure; what the interval
/// admitted; and the Profile Specification the subjects' records belong to.
pub(in crate::benchmark) struct Resolved {
    pub(in crate::benchmark) result: Result<RetainedWorkflow, WorkflowFailure>,
    pub(in crate::benchmark) admitted: Option<AdmittedCounts>,
    pub(in crate::benchmark) specification: VersionedIdentity<String>,
}

/// A refused set or a resolver defect is no outcome: the case fails rather
/// than completing as an expected result (D15:2831). Logical work uses this
/// screen.
pub(in crate::benchmark) fn require_observable(
    resolved: &Resolved,
) -> Result<&RetainedWorkflow, OutputFailure> {
    resolved
        .result
        .as_ref()
        .map_err(|_| OutputFailure::CompleteInvocationFailure)
}

/// The outcome's Shared semantic output and its entry-specific Evidence in
/// separate checksum domains (plan 025 decision #316): the Resolver
/// Construction Identity, the Shared projection or its absence, a checked
/// outcome's profile view and bindings with `C(member)` of each, and whether
/// a Materialized Resolution Input Set came beside the outcome with the digest
/// of its value in the shared domain, and the Entry projection in the entry
/// domain. Neither domain names the entry kind, so either entry of one value
/// can share the shared one (D15:2660, 2776, 2791, 2799).
pub(in crate::benchmark) fn observe(resolved: &Resolved) -> Result<Observation, OutputFailure> {
    let retained = require_observable(resolved)?;
    let specification = Specification {
        identity: &resolved.specification.identity,
        revision: &resolved.specification.revision,
    };
    let mut shared = Frame::new("complete-workflow");
    shared.text(&retained.identity.text());
    shared.flag(retained.shared.is_some());
    if let Some(projection) = &retained.shared {
        project(&mut shared, specification, projection)?;
    }
    frame_profile(&mut shared, retained.profile.as_ref())?;
    shared.flag(retained.materialized.is_some());
    if let Some(digest) = &retained.materialized {
        shared.text(digest);
    }
    let mut entry = Frame::new("complete-workflow-entry");
    project(&mut entry, specification, &retained.entry)?;
    Ok(Observation {
        shared: shared.finish(),
        entry: Some(entry.finish()),
    })
}

/// Whether `profile` is present, and its view and each binding's tuple and
/// `C(member)` of its member, as an outcome's and a replay's shared domains
/// frame them (plan 025 decision #316).
pub(in crate::benchmark) fn frame_profile(
    frame: &mut Frame,
    profile: Option<&(ProfileView, Vec<Binding>)>,
) -> Result<(), OutputFailure> {
    frame.flag(profile.is_some());
    if let Some((view, bindings)) = profile {
        frame.json(&serde_json::to_value(view).map_err(|_| OutputFailure::ObservationEncoding)?);
        frame.uint(u64::try_from(bindings.len()).unwrap_or(u64::MAX));
        for (tuple, body) in bindings {
            frame.text(tuple.kind.spelling());
            frame.text(tuple.identity.as_str());
            frame.text(tuple.revision.as_str());
            frame.text(tuple.specification_revision.as_str());
            frame.text(tuple.semantic_digest.as_str());
            frame.bytes(body);
        }
    }
    Ok(())
}

/// What a complete invocation case runs on and submits, which its
/// descriptor states.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(in crate::benchmark) struct InputFacts {
    scope: String,
    entry_kind: String,
    resolver: String,
    specifications: String,
    capability: String,
    verification: String,
    canonicalization: String,
    submitted_members: Quantity,
    prerequisites: String,
    input_storage: String,
    structural_preparation: String,
    ordinary_output: String,
}

impl InputFacts {
    pub(in crate::benchmark) fn observe(prepared: &PreparedWorkflow) -> Self {
        let members = u64::try_from(prepared.raw.members.len()).unwrap_or(u64::MAX);
        let programmatic = matches!(prepared.raw.entry, RawEntry::Programmatic { .. });
        let entry = if programmatic { "programmatic" } else { "file" };
        Self {
            scope: format!(
                "one-{entry}-entry-invocation-from-pre-invocation-admission-through-the-outcome"
            ),
            entry_kind: entry.into(),
            resolver: match prepared.state() {
                ResolverState::Fresh => "committed-input-constructed-before-each-start-mark",
                ResolverState::Reused => {
                    "committed-input-constructed-once-before-the-first-interval-and-reused"
                }
                ResolverState::ReusedAfterBlocked => {
                    "committed-input-constructed-once-and-reused-right-after-it-resolves-the-blocked-recipe-before-each-start-mark"
                }
            }
            .into(),
            specifications: "every-slot-asserting-what-the-constructed-resolver-supports".into(),
            capability:
                "owner-capability-for-the-recipe-bounds-with-the-bootstrap-groups-read-widened"
                    .into(),
            verification: "sealed-verification-input-pinning-the-selected-policy-with-assertion"
                .into(),
            canonicalization: "committed-icu4x-artifact-submitted-raw-and-admitted-in-the-interval"
                .into(),
            submitted_members: Quantity::new(members),
            prerequisites: "none-every-input-raw-as-a-binding-submits-it".into(),
            input_storage: "raw-values-borrowed-from-resident-preparation".into(),
            structural_preparation:
                "resolver-identity-submission-slot-and-outcome-storage-before-start-mark".into(),
            ordinary_output:
                "outcome-and-materialized-set-copied-after-end-mark-or-benchmark-failure".into(),
        }
    }
}

#[cfg(test)]
impl RawInvocation {
    /// The bytes a file entry's submission holds.
    pub(super) fn file(&self) -> &Arc<[u8]> {
        let RawEntry::File { bytes, .. } = &self.entry else {
            panic!("a file entry's submission");
        };
        bytes
    }
}

#[cfg(test)]
pub(in crate::benchmark) mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeSet;

    use intlify_measurement::acquisition::{ClockDescription, PROVIDER_REVISION};
    use intlify_measurement::measurement::Aggregation;

    use super::*;
    use crate::artifacts::envelope::FormalMember;
    use crate::benchmark::cases::prepare::{prepare, Candidate};
    use crate::benchmark::cases::workflow_inputs::ProgrammaticRecipe as P;
    use crate::benchmark::cases::workflow_inputs::{before, submission};
    use crate::benchmark::cases::{declarations, EntryKind, EvidenceRecipe as R, Recipe};
    use crate::benchmark::clock::tests::ScriptedClock;
    use crate::benchmark::descriptor::{prepared_core, DescriptorIssue, Descriptors};
    use crate::benchmark::instrument::Timed;
    use crate::benchmark::measure::MeasurementFailure;
    use crate::benchmark::memory_domain::{agrees, invocation};
    use crate::benchmark::operation::Operation;
    use crate::benchmark::operation::Output;
    use crate::construction::{committed, flow, identity};
    use crate::snapshot::{from_json, Marker, MarkerReason, Node};

    const STATES: [ResolverState; 3] = [
        ResolverState::Fresh,
        ResolverState::Reused,
        ResolverState::ReusedAfterBlocked,
    ];

    fn root() -> &'static ConstructionRoot {
        ConstructionRoot::built_in().unwrap()
    }

    /// `input`'s declared case on a reused resolver, as preparation makes it.
    pub(in crate::benchmark) fn candidate(input: R) -> Candidate {
        candidate_in(input, ResolverState::Reused)
    }

    /// `input`'s declared case on a resolver in `state`, as preparation makes
    /// it.
    pub(in crate::benchmark) fn candidate_in(input: R, state: ResolverState) -> Candidate {
        let recipe = Recipe::Workflow {
            input,
            resolver: state,
        };
        let declaration = declarations()
            .into_iter()
            .find(|case| case.fixture == recipe);
        prepare(&declaration.unwrap()).unwrap()
    }

    /// `input`'s declared programmatic case on a resolver in `state`, as
    /// preparation makes it.
    pub(in crate::benchmark) fn programmatic_in(input: P, state: ResolverState) -> Candidate {
        let recipe = Recipe::ProgrammaticWorkflow {
            input,
            resolver: state,
        };
        let declaration = declarations()
            .into_iter()
            .find(|case| case.fixture == recipe);
        prepare(&declaration.unwrap()).unwrap()
    }

    /// The outcome a case retained.
    fn retained(candidate: &Candidate) -> &RetainedWorkflow {
        let Output::Workflow(resolved) = &candidate.output else {
            unreachable!()
        };
        require_observable(resolved).unwrap()
    }

    /// D15:2791, 2799; plan 025 decision #316: a programmatic entry's
    /// invocation of the value a file spells frames the Shared domain that
    /// file's case in the same state does, the Materialized Resolution Input
    /// Set of that value with it, and an Entry domain of its own. The
    /// example's value holding a marker has no value: no Shared projection,
    /// no set, and a blocked outcome.
    #[test]
    fn a_programmatic_entry_shares_the_shared_domain_of_its_files_case() {
        for state in STATES {
            for recipe in P::ALL {
                let programmatic = programmatic_in(recipe, state);
                let file = candidate_in(recipe.file(), state);
                let source = |candidate: &Candidate| candidate.declaration.fixture.source();
                assert_eq!(source(&programmatic), source(&file), "{recipe:?}");
                assert_ne!(programmatic.observation.entry, file.observation.entry);
                let (retained, paired) = (retained(&programmatic), retained(&file));
                if recipe == P::RejectedMarker {
                    assert!(retained.shared.is_none() && retained.profile.is_none());
                    assert!(retained.materialized.is_none());
                    continue;
                }
                let shared = (&programmatic.observation.shared, &retained.materialized);
                assert_eq!(shared, (&file.observation.shared, &paired.materialized));
                assert!(retained.materialized.is_some(), "{recipe:?} {state:?}");
            }
        }
    }

    /// D15:205, 729, 744, 890: a programmatic entry submits its Snapshot in
    /// the programmatic slots, under the Programmatic Entry identity its
    /// frontend marks non-secret, at a non-secret safe-label locator, its
    /// revision the Snapshot's digest, with no call-site sidecar.
    #[test]
    fn a_programmatic_entry_is_submitted_in_its_own_slots() {
        use crate::benchmark::cases::workflow_inputs::programmatic;

        let constructed = build(root(), committed::input()).unwrap();
        let raw = programmatic(P::Storefront, &constructed, root()).unwrap();
        let members = raw.members();
        let SubmittedEntry::Programmatic(entry) = raw.submitted(&members).entry else {
            panic!("a programmatic entry");
        };
        let RawEntry::Programmatic { snapshot, revision } = &raw.entry else {
            unreachable!()
        };
        let value = &entry.programmatic.value;
        assert!(value
            .snapshot
            .is_some_and(|submitted| std::ptr::eq(submitted, snapshot)));
        let identity = (value.identity, value.marked, value.revision);
        let expected = Some("intlify-programmatic-entry.v0:storefront");
        assert_eq!(identity, (expected, true, Some(revision.as_str())));
        let locator = value.locator.as_ref().unwrap();
        let locator = (locator.variant, locator.payload, locator.non_secret);
        assert_eq!(locator, (LocatorVariant::SafeLabel, "storefront-app", true));
        assert!(value.call_site.is_none());
        let origins = (entry.programmatic.origin, entry.selector.origin);
        let own = |kind: Kind| Some(kind.id());
        let slots = (
            Kind::ConfigurationProgrammatic,
            Kind::ProfileSelectorProgrammatic,
        );
        assert_eq!(origins, (own(slots.0), own(slots.1)));
    }

    /// D15:890, 1890, 2801: a programmatic entry submits its file case's
    /// set but for the entry, the Snapshot of the value the file spells, its
    /// revision the digest, and the interval counts the admitted Snapshot's
    /// envelope bytes and traversal units, and its value as the file's, but
    /// none for a Snapshot holding a marker.
    #[test]
    fn a_programmatic_entry_submits_and_counts_the_snapshot_of_its_files_value() {
        use crate::benchmark::cases::workflow_inputs::programmatic;

        let constructed = build(root(), committed::input()).unwrap();
        for recipe in P::ALL {
            let raw = programmatic(recipe, &constructed, root()).unwrap();
            let file = submission(recipe.file(), &constructed, root()).unwrap();
            let RawEntry::Programmatic { snapshot, revision } = &raw.entry else {
                panic!("{recipe:?} is programmatic");
            };
            let SnapshotReport::Formal { digest, .. } = snapshot else {
                panic!("{recipe:?} is a formal Snapshot");
            };
            assert_eq!(revision, digest);
            let mut same = raw.clone();
            same.entry = file.entry.clone();
            assert!(same == file, "{recipe:?}");
            let resolved = programmatic_in(recipe, ResolverState::Reused);
            let Output::Workflow(resolved) = &resolved.output else {
                unreachable!()
            };
            let admitted = resolved.admitted.unwrap();
            let config = recipe.file().fixture(&recipe.file().limits()).config;
            let mut tree = from_json(&config);
            if recipe == P::RejectedMarker {
                let Node::Object(members) = &mut tree else {
                    unreachable!()
                };
                let marker = Marker::Rejected(MarkerReason::UnsupportedType);
                members.insert("onLoad".to_owned(), Node::Marker(marker));
            }
            let counts = SnapshotCounts::of(&tree);
            let entry = EntryCounts::Snapshot {
                envelope: counts.envelope_canonical_bytes,
                traversal: counts.traversal_units,
            };
            assert_eq!(admitted.entry, entry, "{recipe:?}");
            let value = (counts.rejected_markers == 0).then(|| counts.value());
            assert_eq!(admitted.value, value, "{recipe:?}");
            let paired = candidate(recipe.file());
            let Output::Workflow(paired) = &paired.output else {
                unreachable!()
            };
            if recipe != P::RejectedMarker {
                assert_eq!(admitted.value, paired.admitted.unwrap().value);
            }
        }
    }

    /// The inputs of `recipe`'s invocation on a reused resolver.
    fn prepared(recipe: R) -> PreparedWorkflow {
        prepared_in(recipe, ResolverState::Reused)
    }

    /// The inputs of `recipe`'s invocation on a resolver in `state`: one
    /// constructed once from the committed input, what a binding submits for
    /// the recipe on it, and what the state runs before each start mark.
    fn prepared_in(recipe: R, state: ResolverState) -> PreparedWorkflow {
        let constructed = build(root(), committed::input()).unwrap();
        let raw = submission(recipe, &constructed, root()).unwrap();
        let before = before(state, EntryKind::File, &constructed, root()).unwrap();
        PreparedWorkflow::new(root(), constructed, raw, before)
    }

    /// How often `prepared`'s repetitions constructed a resolver, invoked the
    /// prepared one, and resolved a blocked outcome first.
    fn counts(prepared: &PreparedWorkflow) -> [u64; 3] {
        let counts = &prepared.counts;
        [&counts.constructed, &counts.prepared, &counts.blocked]
            .map(|count| count.load(Ordering::Relaxed))
    }

    fn specification(resolved: &Resolved) -> Specification<'_> {
        Specification {
            identity: &resolved.specification.identity,
            revision: &resolved.specification.revision,
        }
    }

    /// D15:2658, 2660, 2776, 2831: each recipe's timed invocation retains
    /// what an untimed one does. Only the example's outcome is checked, and a
    /// Shared projection and a Materialized Resolution Input Set come with an
    /// outcome exactly when Phase 1 materialized a value, the set with the
    /// digest of the canonical tree a Snapshot of the recipe's configuration
    /// has.
    #[test]
    fn each_recipe_completes_and_its_interval_retains_what_an_untimed_run_does() {
        for recipe in R::ALL {
            let prepared = prepared(recipe);
            let untimed = prepared.resolve().unwrap();
            let clock = ScriptedClock::nanos([0, 1]);
            let timed = prepared.interval(&Timed(&clock)).unwrap().output;
            assert_eq!(observe(&timed), observe(&untimed), "{recipe:?}");
            assert_eq!(timed.admitted, untimed.admitted, "{recipe:?}");
            let retained = require_observable(&untimed).unwrap();
            assert_eq!(retained.profile.is_some(), recipe.checked(), "{recipe:?}");
            let valued = !matches!(recipe, R::Malformed | R::Inadmissible);
            assert_eq!(retained.shared.is_some(), valued, "{recipe:?}");
            let config = recipe.fixture(&recipe.limits()).config;
            let tree = valued.then(|| from_json(&config).digest());
            assert_eq!(retained.materialized, tree, "{recipe:?}");
        }
    }

    /// D15:1848, 2130, 2801: what the interval admitted is counted as strict
    /// file entry counts it, a value's counts only when the raw parse
    /// completed, with each submitted member's `C(member)` bytes; a checked
    /// outcome's bindings keep those bodies, the mandatory reserve grows only
    /// when a bound exhausted a domain, and the records are read under the
    /// Active Project Profile Specification the submission asserts.
    #[test]
    fn the_admitted_set_is_counted_and_the_bindings_keep_their_bodies() {
        let count = |len: usize| u64::try_from(len).unwrap();
        let mut reserves = Vec::new();
        for recipe in R::ALL {
            let prepared = prepared(recipe);
            let (raw, resolved) = (&prepared.raw, prepared.resolve().unwrap());
            let mut bodies: Vec<Vec<u8>> = raw
                .members
                .iter()
                .map(|member| serde_json::from_slice(member).unwrap())
                .map(|value| FormalMember::new(value).unwrap().canonical_body().to_vec())
                .collect();
            let bytes = bodies.iter().map(Vec::len).sum();
            let admitted = resolved.admitted.unwrap();
            let EntryCounts::File {
                bytes: read,
                tokens,
            } = admitted.entry
            else {
                panic!("{recipe:?} counts a file");
            };
            assert_eq!(read, count(raw.file().len()), "{recipe:?}");
            let members = (admitted.members, admitted.member_bytes);
            assert_eq!(members, (count(raw.members.len()), count(bytes)));
            let limits = crate::materialize_tests::limits();
            let parsed = match materialize_file(Arc::clone(raw.file()), limits) {
                Ok(document) => {
                    let counts = document.counts().unwrap();
                    (counts.parser_tokens, Some(counts.value))
                }
                Err(error) => {
                    let value = error.progress.complete_value.as_deref().copied();
                    (error.progress.parser_tokens_visited, value)
                }
            };
            assert_eq!((tokens, admitted.value), parsed);
            assert_eq!(admitted.value.is_some(), recipe != R::Malformed);
            let asserted: Value = serde_json::from_slice(&raw.specification).unwrap();
            let profile = &asserted["project-profile"];
            assert_eq!(
                profile["identity"],
                resolved.specification.identity.as_str()
            );
            assert_eq!(
                profile["revision"],
                resolved.specification.revision.as_str()
            );
            let retained = require_observable(&resolved).unwrap();
            if let Some((_, bindings)) = &retained.profile {
                let mut kept: Vec<_> = bindings.iter().map(|(_, body)| body.clone()).collect();
                kept.sort();
                bodies.sort();
                assert_eq!(kept, bodies, "{recipe:?}");
            }
            reserves.push((recipe, retained.reserve));
        }
        let whole = reserves[0].1;
        for (recipe, reserve) in reserves {
            assert!(reserve > 0, "{recipe:?}");
            assert_eq!(reserve > whole, recipe == R::Truncated, "{recipe:?}");
        }
    }

    /// D15:2780, 2791, 2799: the codec frames, in order, the shared domain
    /// under a name that does not say which entry the invocation had: the
    /// identity, the Shared projection behind its flag, the profile view and
    /// each binding's tuple and body behind theirs, and the set's value
    /// digest behind its own; and the Entry projection under its own name.
    #[test]
    fn a_checked_outcome_is_framed_as_the_codec_spells_it() {
        let resolved = prepared(R::Storefront).resolve().unwrap();
        let retained = require_observable(&resolved).unwrap();
        let (Some(projection), Some((view, bindings)), Some(digest)) =
            (&retained.shared, &retained.profile, &retained.materialized)
        else {
            panic!("the example's outcome is checked beside a set")
        };
        let specification = specification(&resolved);
        let mut shared = Frame::new("complete-workflow");
        shared.text(&retained.identity.text());
        shared.flag(true);
        project(&mut shared, specification, projection).unwrap();
        shared.flag(true);
        shared.json(&serde_json::to_value(view).unwrap());
        shared.uint(u64::try_from(bindings.len()).unwrap());
        for (tuple, body) in bindings {
            shared.text(tuple.kind.spelling());
            shared.text(tuple.identity.as_str());
            shared.text(tuple.revision.as_str());
            shared.text(tuple.specification_revision.as_str());
            shared.text(tuple.semantic_digest.as_str());
            shared.bytes(body);
        }
        shared.flag(true);
        shared.text(digest);
        let mut entry = Frame::new("complete-workflow-entry");
        project(&mut entry, specification, &retained.entry).unwrap();
        let framed = Observation {
            shared: shared.finish(),
            entry: Some(entry.finish()),
        };
        assert_eq!(observe(&resolved).unwrap(), framed);
    }

    /// Plan 025 decision #316: the shared domain tells apart the identity,
    /// the Shared projection and its terminal state, the profile, each
    /// binding and its body, and the set and its value, and the entry domain
    /// the Entry projection; neither tells apart what the other does.
    #[test]
    fn every_part_of_the_outcome_and_the_set_is_observed_in_its_domain() {
        let resolved = prepared(R::Storefront).resolve().unwrap();
        let observed = |edit: &dyn Fn(&mut RetainedWorkflow)| {
            let mut retained = resolved.result.as_ref().unwrap().clone();
            edit(&mut retained);
            observe(&Resolved {
                result: Ok(retained),
                admitted: resolved.admitted,
                specification: resolved.specification.clone(),
            })
            .unwrap()
        };
        let whole = observed(&|_| {});
        let truncated = prepared(R::Truncated).resolve().unwrap();
        let shared = require_observable(&truncated).unwrap().shared.clone();
        let terminal = shared.unwrap().terminal;
        let mut admitted = flow::construct(root(), &committed::input()).unwrap();
        admitted.registry_package.declared.revision = "1".to_owned();
        let other = identity::derive(root(), &admitted);
        let shared: [&dyn Fn(&mut RetainedWorkflow); 8] = [
            &|retained| retained.identity = other,
            &|retained| retained.shared = None,
            &|retained| retained.shared.as_mut().unwrap().terminal = terminal.clone(),
            &|retained| retained.profile = None,
            &|retained| _ = retained.profile.as_mut().unwrap().1.pop(),
            &|retained| retained.profile.as_mut().unwrap().1[0].1.push(0),
            &|retained| retained.materialized = None,
            &|retained| retained.materialized.as_mut().unwrap().push('0'),
        ];
        let entry: [&dyn Fn(&mut RetainedWorkflow); 2] =
            [&|retained| _ = retained.entry.records.pop(), &|retained| {
                retained.entry.terminal = terminal.clone();
            }];
        for (index, edit) in shared.into_iter().enumerate() {
            let changed = observed(edit);
            assert_ne!(changed.shared, whole.shared, "shared {index}");
            assert_eq!(changed.entry, whole.entry, "shared {index}");
        }
        for (index, edit) in entry.into_iter().enumerate() {
            let changed = observed(edit);
            assert_eq!(changed.shared, whole.shared, "entry {index}");
            assert_ne!(changed.entry, whole.entry, "entry {index}");
        }
    }

    /// D15:2792, 2829, 3814: before every start mark, and before an untimed
    /// run too, a fresh case constructs a resolver of its own and never
    /// invokes the prepared one, a reused case invokes the prepared one, and
    /// a case reused after a blocked outcome invokes it once it resolved the
    /// Blocked recipe on it. Every repetition retains what a reused
    /// resolver's does.
    #[test]
    fn each_state_runs_its_own_work_once_before_every_start_mark() {
        let reused = observe(&prepared(R::Storefront).resolve().unwrap());
        for state in STATES {
            let prepared = prepared_in(R::Storefront, state);
            let clock = ScriptedClock::nanos([0, 1, 2, 3, 4, 5]);
            for _ in 0..3 {
                let measured = prepared.interval(&Timed(&clock)).unwrap();
                assert_eq!(observe(&measured.output), reused, "{state:?}");
            }
            assert_eq!(observe(&prepared.resolve().unwrap()), reused, "{state:?}");
            let expected = match state {
                ResolverState::Fresh => [4, 0, 0],
                ResolverState::Reused => [0, 4, 0],
                ResolverState::ReusedAfterBlocked => [0, 4, 4],
            };
            assert_eq!(counts(&prepared), expected, "{state:?}");
        }
    }

    /// D15:2752, 2831: a case reused after a blocked outcome measures nothing
    /// unless what it resolves first is admitted and blocked. A checked
    /// outcome or a refused set first leaves a repetition, timed or not,
    /// unavailable before its start mark.
    #[test]
    fn a_case_reused_after_a_blocked_outcome_needs_that_outcome_first() {
        let mut prepared = prepared_in(R::Storefront, ResolverState::ReusedAfterBlocked);
        let checked = prepared.raw.clone();
        let mut refused = checked.clone();
        refused.reference = crate::base64url::construction_identity(&[0; 32]);
        for first in [checked, refused] {
            prepared.before = Before::AfterBlocked(Box::new(first));
            let clock = ScriptedClock::nanos([]);
            let measured = prepared.interval(&Timed(&clock));
            let unavailable = MeasurementFailure::PrerequisiteUnavailable;
            assert!(matches!(measured, Err(failure) if failure == unavailable));
            assert!(prepared.resolve().is_none());
        }
        assert_eq!(counts(&prepared), [0, 4, 0]);
    }

    /// D15:2752, 2792, 3814; D26:1739, 1815: the expectations pin each
    /// recipe's three cases to one result and one logical work, so a fresh
    /// resolver and one reused right after a blocked outcome resolve it as
    /// the reused one does, while each case's input context, which binds
    /// what runs before its start marks, is its own.
    #[test]
    fn every_state_is_expected_to_resolve_each_recipe_as_the_reused_resolver_does() {
        let expectations: Value =
            serde_json::from_str(include_str!("cases/expectations-v0.json")).unwrap();
        let mut cases: BTreeMap<&str, Vec<(&str, &Value)>> = BTreeMap::new();
        for row in expectations["rows"].as_array().unwrap() {
            let fixture = &row["declaration"]["fixture"]["workflow"];
            let (input, state) = (fixture["input"].as_str(), fixture["resolver"].as_str());
            if let (Some(input), Some(state)) = (input, state) {
                cases.entry(input).or_default().push((state, row));
            }
        }
        assert_eq!(cases.len(), R::ALL.len());
        for (input, rows) in &cases {
            let states: Vec<&str> = rows.iter().map(|(state, _)| *state).collect();
            assert_eq!(
                states,
                ["reused", "fresh", "reused-after-blocked"],
                "{input}"
            );
            let contexts: BTreeSet<_> = rows
                .iter()
                .map(|(_, row)| row["inputContext"].as_str())
                .collect();
            assert_eq!(contexts.len(), STATES.len(), "{input}");
            let (_, reused) = rows[0];
            for (state, row) in &rows[1..] {
                for key in ["result", "logicalWork", "work"] {
                    assert_eq!(row[key], reused[key], "{input} {state} {key}");
                }
            }
        }
    }

    /// D15:2810; D26:1667: each state's case is described as running in the
    /// engine state its resolver is in, fresh only for a fresh resolver,
    /// which the invocation Memory Observation Domain of that state agrees
    /// with and the other engine state's does not. Its facts say what runs
    /// before each start mark, and a descriptor stating the other engine
    /// state is refused.
    #[test]
    fn each_state_is_described_in_the_engine_state_its_memory_domain_agrees_with() {
        let clock = ClockDescription {
            provider: "rustix",
            provider_revision: PROVIDER_REVISION,
            clock: "posix-clock-monotonic",
            resolution_nanoseconds: Quantity::new(100),
            resolution_source: "clock-getres-reported-granularity",
            conversion: "exact-integer-nanoseconds",
        };
        let mut resolvers = BTreeSet::new();
        for state in STATES {
            let candidate = candidate_in(R::Storefront, state);
            let prepared = &candidate.prepared;
            assert_eq!(prepared.resolver_state(), Some(state));
            let descriptors = Descriptors::for_acquisition(prepared, clock);
            assert!(
                descriptors.validate(prepared, clock).is_empty(),
                "{state:?}"
            );
            let execution = &descriptors.execution;
            let fresh = state == ResolverState::Fresh;
            let engine = if fresh { "fresh" } else { "reused" };
            assert_eq!(execution.engine_state, engine, "{state:?}");
            assert!(agrees(&invocation(state), execution), "{state:?}");
            let other = if fresh {
                ResolverState::Reused
            } else {
                ResolverState::Fresh
            };
            assert!(!agrees(&invocation(other), execution), "{state:?}");
            let operation = Operation::FileEntryCompleteWorkflow;
            let mut stated = descriptors.clone();
            stated.execution = prepared_core(operation, None, Some(other));
            let issues = stated.validate(prepared, clock);
            assert_eq!(issues, [DescriptorIssue::ExecutionMismatch], "{state:?}");
            resolvers.insert(descriptors.workflow_input.unwrap().resolver);
        }
        assert_eq!(resolvers.len(), STATES.len());
    }

    /// D15:2776, 2829: the interval begins with the pre-invocation admission
    /// of a resolver every state made beforehand, so what it is handed is
    /// that resolver and the submission beside an empty slot for the set it
    /// admits and the storage the outcome borrows, never an admitted set,
    /// and a repetition runs no other interval.
    #[test]
    fn the_interval_is_handed_the_submission_not_an_admitted_set() {
        struct Recording(RefCell<Vec<&'static str>>);
        impl Instrument for Recording {
            type Failure = ();
            const AGGREGATION: Aggregation = Aggregation::BatchTotal;
            fn interval<Input, Output>(
                &self,
                input: Input,
                invoke: fn(Input) -> Output,
            ) -> Result<Interval<Output>, ()> {
                self.0.borrow_mut().push(std::any::type_name::<Input>());
                let output = invoke(input);
                Ok(Interval {
                    quantity: Quantity::new(0),
                    output,
                })
            }
            fn unavailable() {}
        }
        for state in STATES {
            let prepared = prepared_in(R::Storefront, state);
            let recording = Recording(RefCell::new(Vec::new()));
            let measured = prepared.interval(&recording).unwrap();
            assert_eq!(
                observe(&measured.output),
                observe(&prepared.resolve().unwrap())
            );
            let handed = recording.0.into_inner();
            let [input] = handed.as_slice() else {
                panic!("one interval: {handed:?}")
            };
            for part in [
                "(&intlify_config::resolver::invocation::Resolver<",
                "boundary::invocation::SubmittedInvocation<",
                "&mut core::option::Option<intlify_config::boundary::invocation::InvocationInputs<",
                "&mut intlify_config::resolver::invocation::OutcomeStorage",
            ] {
                assert!(input.contains(part), "{state:?} {input}");
            }
        }
    }
}
