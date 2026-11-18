// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Evidence materialization stages over one file entry's complete
//! invocation (D15:2771, 2775). Before the interval, the invocation's
//! evaluation phases run on an evaluation of their own over 015's example and
//! every artifact it names, under fixture-owned inputs: the finite fixture
//! provider and the specifications the resolver supports for it, a capability
//! whose `evidenceMetadata` bounds the resolver's own work fits, the
//! Verification Input pinning the selected policy, and a sealed vocabulary.
//! The interval runs one stage: the Shared projection stage counts the Shared
//! scope's Evidence metadata work and plans the Shared domain's slices (plan
//! 025 decision #253). The slices borrow the evaluation, so after the end
//! read they are copied out. Neither the result nor its observation is a
//! Profile or a shared artifact.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::artifacts::collection::FormalCollection;
use crate::artifacts::vocabulary::{
    self, FormalVocabulary, IntentSurfaceClassVocabularyArtifact, VocabularyLimits,
};
use crate::boundary::capability::ImplementationCapability;
use crate::boundary::verification::VerificationInput;
use crate::construction::registry::check::{check_id, RecordKind};
use crate::input_limits::InputLimits;
use crate::locale::fixtures::{fixture_pins, FixtureProvider};
use crate::locale::{Supported, VersionedIdentity};
use crate::resolver::artifact_resolution::ArtifactSet;
use crate::resolver::entry::fixtures::FileFixture;
use crate::resolver::evaluation::{Admitted, Evaluation, ExhaustedBound, Status, Unit};
use crate::resolver::outcome::{shared_projection, SharedStage};
use crate::resolver::profile_resolution::{invoke_file, Invocation, Invoked, ResolverInvariant};
use crate::resolver::reporting::{Candidates, Record, ReportingDomain};
use crate::resolver::reporting_plan::Committed;
use crate::resolver::specification::{
    ActiveProfileSpecification, SpecificationInput, SupportedSpecifications, VocabularyAssertion,
};
use crate::resolver::token::{self, Sha256, Specification, Tokens};
use crate::structural::selection::SelectorInput;
use crate::structural::StructuralLimits;

use super::observation::{Frame, Observation};
use super::operation::{OutputFailure, Schema};
use super::quantity::Quantity;

/// The fixture provider's identity type, which every specification here is
/// spelled in.
pub(super) type Id = &'static str;

/// The Active Project Profile Specification the benchmark's invocations run
/// under, a fixture's.
pub(super) const ACTIVE: VersionedIdentity<Id> = VersionedIdentity {
    identity: "intlify-benchmark-profile-specification",
    revision: "0",
};

/// The vocabulary the benchmark's invocations submit, by identity and
/// revision, and its members: the surface classes 015's example names.
const VOCABULARY: (Id, Id) = ("intlify-benchmark-vocabulary", "0");
const SURFACES: [&str; 3] = ["checkout", "internal-tool", "nav"];

/// The semantic digest of that vocabulary, which its slot asserts. It covers
/// the members alone, so it is the resolver tests' fixture vocabulary's too.
const VOCABULARY_DIGEST: Id =
    "sha256:ca5f503b95aad972565514d3b68b3e8be4a811bb2fc983be16163b88c1fffbb2";

/// Limits no fixture vocabulary comes near.
const ROOM: VocabularyLimits = VocabularyLimits {
    artifact_bytes: u64::MAX,
    depth: u64::MAX,
    members: u64::MAX,
    member_bytes: u64::MAX,
    total_member_bytes: u64::MAX,
};

/// The sealed vocabulary past 015's pre-invocation boundary.
pub(super) fn vocabulary() -> Option<FormalVocabulary> {
    let artifact =
        IntentSurfaceClassVocabularyArtifact::sealed(VOCABULARY.0, VOCABULARY.1, &SURFACES).ok()?;
    vocabulary::read(Some(&serde_json::to_vec(&artifact).ok()?), &ROOM).ok()
}

/// All eight slots, each asserting what the resolver supports for the
/// fixture provider.
pub(super) fn specifications() -> SpecificationInput<Id> {
    let identity = |identity| VersionedIdentity {
        identity,
        revision: "0",
    };
    SpecificationInput {
        project_profile: Some(ACTIVE),
        locale_canonicalization: Some(fixture_pins().specification),
        locale_negotiation: Some(identity("intlify-locale-negotiation")),
        message_locale_fallback: Some(identity("intlify-message-locale-fallback")),
        coverage: Some(identity("intlify-localization-coverage")),
        intent_surface_class_vocabulary: Some(VocabularyAssertion {
            identity: VOCABULARY.0,
            revision: VOCABULARY.1,
            semantic_digest: VOCABULARY_DIGEST,
        }),
        delivery_graph: Some(identity("intlify-delivery-graph")),
        delivery_placement: Some(identity("intlify-delivery-placement")),
    }
}

/// What the resolver supports for the fixture provider: revision "0" around
/// the fixture's active profile specification and the provider's pins.
pub(super) fn supported() -> Option<SupportedSpecifications<Id>> {
    Some(SupportedSpecifications::revision_zero(
        ActiveProfileSpecification::fixture(ACTIVE),
        Supported::new(vec![fixture_pins()]).ok()?,
    ))
}

/// A case's configuration file and the bounds strict file entry reads it
/// under, the structural stage's bounds and schema, the selector, and the
/// invocation's other inputs past 015's pre-invocation boundary.
pub(super) struct PreparedEvidence {
    pub(super) source: Arc<[u8]>,
    pub(super) input: InputLimits,
    pub(super) structural: StructuralLimits,
    pub(super) file: FileFixture,
    pub(super) schema: Schema,
    pub(super) selector: SelectorInput,
    pub(super) supported: SupportedSpecifications<Id>,
    pub(super) specifications: SpecificationInput<Id>,
    pub(super) capability: ImplementationCapability,
    pub(super) verification: VerificationInput,
    pub(super) collection: FormalCollection,
    pub(super) vocabulary: FormalVocabulary,
}

impl PreparedEvidence {
    /// The invocation's inputs besides the configuration.
    pub(super) const fn invocation(&self) -> Invocation<'_, Id> {
        Invocation {
            supported: &self.supported,
            specifications: &self.specifications,
            capability: &self.capability,
            verification: &self.verification,
            collection: &self.collection,
            vocabulary: &self.vocabulary,
        }
    }

    /// The invocation's evaluation phases over the configuration file,
    /// recording into `evaluation`: what they leave for the Evidence stages,
    /// or `None` on a resolver defect. The Set Phase 9 admits is kept in
    /// `set`.
    pub(super) fn invoke<'s>(
        &self,
        evaluation: &mut Evaluation,
        invocation: &Invocation<'s, Id>,
        set: &'s mut Option<Admitted<ArtifactSet<'s>>>,
    ) -> Option<Invoked<'s, Id>> {
        let file = self.file.file(Arc::clone(&self.source));
        invoke_file(
            evaluation,
            (&file, self.input),
            (&self.schema, self.structural, &self.selector),
            invocation,
            FixtureProvider::new(),
            set,
        )
        .ok()
    }

    /// One untimed invocation of the Shared projection stage, retained as a
    /// timed one is.
    pub(super) fn resolve_shared(&self) -> Option<Resolved> {
        let invocation = self.invocation();
        let mut evaluation = Evaluation::default();
        let mut set = None;
        let invoked = self.invoke(&mut evaluation, &invocation, &mut set)?;
        let stage = shared((&evaluation, &invoked, &invocation));
        Some(Resolved::retain(&evaluation, &invocation, stage))
    }
}

/// The Shared projection stage: what every timed invocation of
/// `shared_evidence` runs.
pub(super) fn shared<'e, 'a>(
    (evaluation, invoked, invocation): (
        &'e Evaluation,
        &'e Invoked<'_, Id>,
        &'e Invocation<'a, Id>,
    ),
) -> Result<SharedStage<'e, 'a>, ResolverInvariant> {
    shared_projection(evaluation, invoked, invocation, &Sha256)
}

/// One committed record as the output retains it: its record kind, its view,
/// the length of `C` of its accounting view (D15:2518), and, for a
/// not-evaluated status, its distinct causal Finding Keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RetainedRecord {
    pub(super) kind: RecordKind,
    pub(super) view: Value,
    pub(super) bytes: u64,
    pub(super) causes: u64,
}

/// One committed slice as the output retains it: its owner, what its
/// candidate stage counted, and its records in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RetainedSlice {
    pub(super) unit: Unit,
    pub(super) candidates: Candidates,
    pub(super) records: Vec<RetainedRecord>,
}

/// What a domain committed, copied out of the evaluation's borrow: its
/// slices in canonical unit order, and, when a slice it refused exhausted
/// it, that slice's unit and the bound its terminal Finding names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Retained {
    pub(super) slices: Vec<RetainedSlice>,
    pub(super) exhausted: Option<(Unit, ExhaustedBound)>,
}

impl Retained {
    fn of(committed: &Committed<'_>) -> Self {
        Self {
            slices: committed
                .slices
                .iter()
                .map(|slice| RetainedSlice {
                    unit: slice.unit.clone(),
                    candidates: slice.candidates.clone(),
                    records: slice
                        .records
                        .iter()
                        .map(|retained| RetainedRecord {
                            kind: retained.kind,
                            view: retained.view.clone(),
                            bytes: retained.bytes,
                            causes: match retained.record {
                                Record::Status(Status::NotEvaluated(causes)) => {
                                    count(causes.keys().count())
                                }
                                _ => 0,
                            },
                        })
                        .collect(),
                })
                .collect(),
            exhausted: committed
                .exhausted
                .map(|exhausted| (exhausted.cursor.clone(), exhausted.bound)),
        }
    }
}

/// What the Shared scope counts under the four `evidenceMetadata` counts,
/// in their units (D15:1836-1839, 1851): the raw candidates and subjects of
/// the units that report into the Shared domain, and the comparisons and
/// records their tokens take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SharedScopeWork {
    pub(super) subject_occurrences: u64,
    pub(super) token_inputs: u64,
    pub(super) collision_comparisons: u64,
    pub(super) metadata_work_units: u64,
}

impl SharedScopeWork {
    /// Counted over `evaluation` under `specification`'s tokens, outside the
    /// interval.
    fn of(evaluation: &Evaluation, specification: Specification<'_>) -> Self {
        let shared = |unit: &Unit| ReportingDomain::of(unit.phase) == ReportingDomain::Shared;
        let subjects: Vec<_> = evaluation
            .statuses()
            .filter(|(unit, _)| shared(unit))
            .map(|(unit, _)| &unit.subject)
            .collect();
        let tokens = Tokens::assign(specification, subjects.iter().copied(), &Sha256);
        Self {
            subject_occurrences: count(
                evaluation
                    .candidates()
                    .filter(|(unit, _)| shared(unit))
                    .count(),
            ),
            token_inputs: count(subjects.len()),
            collision_comparisons: tokens.comparisons(),
            metadata_work_units: tokens.records(),
        }
    }
}

/// The Shared projection stage's complete ordinary output, retained past the
/// end read, with the Shared scope's work and the Profile Specification its
/// subjects' records belong to.
pub(super) struct Resolved {
    pub(super) result: Result<Retained, ResolverInvariant>,
    pub(super) scope: SharedScopeWork,
    pub(super) specification: Specification<'static>,
}

impl Resolved {
    /// The slices borrow the evaluation, so after the end read they are
    /// copied out, and the Shared scope's work counted. Both are observation,
    /// not work of the interval.
    pub(super) fn retain(
        evaluation: &Evaluation,
        invocation: &Invocation<'_, Id>,
        stage: Result<SharedStage<'_, '_>, ResolverInvariant>,
    ) -> Self {
        let profile = &invocation.supported.project_profile().specification;
        let specification = Specification {
            identity: profile.identity,
            revision: profile.revision,
        };
        Self {
            result: stage.map(|stage| Retained::of(stage.committed())),
            scope: SharedScopeWork::of(evaluation, specification),
            specification,
        }
    }
}

fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct InputFacts {
    scope: String,
    entry_kind: String,
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
    pub(super) fn observe(prepared: &PreparedEvidence) -> Self {
        Self {
            scope: "the-shared-domain-of-one-file-entry-invocation".into(),
            entry_kind: "file".into(),
            specifications:
                "fixture-active-profile-specification-and-every-slot-asserting-what-it-supports"
                    .into(),
            capability:
                "fixture-owned-capability-with-room-for-every-diagnostic-and-evidence-metadata-count"
                    .into(),
            verification: "fixture-owned-verification-input-pinning-the-selected-policy".into(),
            canonicalization: "finite-fixture-provider-under-its-own-pins".into(),
            submitted_members: Quantity::new(count(prepared.collection.members().len())),
            prerequisites: "every-evaluation-phase-of-the-invocation".into(),
            input_storage: "borrowed-evaluation-and-invocation-state-over-resident-inputs".into(),
            structural_preparation:
                "complete-invocation-on-an-evaluation-of-its-own-before-interval".into(),
            ordinary_output: "shared-slices-copied-after-end-read-or-resolver-defect".into(),
        }
    }
}

/// A resolver defect is no blocked outcome: the case fails rather than
/// completing as an expected failure. Logical work uses this screen.
pub(super) fn require_observable(resolved: &Resolved) -> Result<&Retained, OutputFailure> {
    resolved
        .result
        .as_ref()
        .map_err(|_| OutputFailure::EvidenceMaterializationInvariant)
}

/// `unit` without its subject's token: its phase, its subject's record under
/// `specification`, the token generation's input, and its check.
fn unit(
    frame: &mut Frame,
    specification: Specification<'_>,
    unit: &Unit,
) -> Result<(), OutputFailure> {
    frame.text(unit.phase.id());
    frame.bytes(&token::record(specification, &unit.subject));
    frame.text(
        check_id(unit.subject.kind(), unit.check)
            .ok_or(OutputFailure::EvidenceMaterializationInvariant)?,
    );
    Ok(())
}

/// The Shared semantic output, framed in its own checksum domain; the stage
/// has no entry-specific Evidence (D15:2794): each committed slice's owner,
/// candidate counts, and records in order, and how the domain was exhausted,
/// if it was.
pub(super) fn observe(resolved: &Resolved) -> Result<Observation, OutputFailure> {
    let retained = require_observable(resolved)?;
    let mut frame = Frame::new("shared-evidence");
    frame.uint(count(retained.slices.len()));
    for slice in &retained.slices {
        unit(&mut frame, resolved.specification, &slice.unit)?;
        let candidates = &slice.candidates;
        frame.uint(candidates.findings);
        frame.uint(candidates.related);
        frame.uint(count(candidates.evidence.len()));
        for bytes in &candidates.evidence {
            frame.uint(*bytes);
        }
        frame.uint(candidates.causes);
        frame.uint(count(slice.records.len()));
        for record in &slice.records {
            frame.text(record.kind.id());
            frame.json(&record.view);
            frame.uint(record.bytes);
        }
    }
    frame.flag(retained.exhausted.is_some());
    if let Some((cursor, bound)) = &retained.exhausted {
        unit(&mut frame, resolved.specification, cursor)?;
        match bound {
            ExhaustedBound::Domain(id) => {
                frame.uint(0);
                frame.text(id.group.path());
                frame.text(id.name);
            }
            ExhaustedBound::Capability(id) => {
                frame.uint(1);
                frame.text(id.group.path());
                frame.text(id.name);
            }
        }
    }
    Ok(Observation {
        shared: frame.finish(),
        entry: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::cases::evidence_limits;
    use crate::benchmark::cases::prepare::prepare;
    use crate::benchmark::cases::{declarations, EvidenceRecipe as R, Recipe};
    use crate::benchmark::operation::Prepared;
    use crate::input_limits::Bound;
    use crate::resolver::capability::EvidenceMetadataLimits;
    use crate::resolver::evidence_metadata::fixtures::{unbounded, with};
    use crate::resolver::evidence_metadata::MetadataBound;
    use crate::resolver::limits::{BoundId, Group, ResolutionLimits};
    use crate::resolver::outcome::outcome;
    use crate::resolver::outcome::tests::identity;
    use intlify_shared_json::token::Token;

    fn prepared(recipe: R) -> PreparedEvidence {
        let declaration = declarations()
            .into_iter()
            .find(|case| case.fixture == Recipe::Evidence(recipe) && case.limit.is_none())
            .unwrap();
        let Prepared::SharedEvidence(prepared) = prepare(&declaration).unwrap().prepared else {
            unreachable!()
        };
        *prepared
    }

    fn retained_of(recipe: R) -> Retained {
        prepared(recipe).resolve_shared().unwrap().result.unwrap()
    }

    /// The record kinds and views of `retained`'s records, sorted.
    fn sorted(records: impl Iterator<Item = (RecordKind, Value)>) -> Vec<(&'static str, String)> {
        let mut records: Vec<_> = records
            .map(|(kind, view)| (kind.id(), view.to_string()))
            .collect();
        records.sort();
        records
    }

    /// The example is a checked outcome, so the stages run over an
    /// invocation that holds every input it needs (D15:1152).
    #[test]
    fn the_storefront_invocation_is_checked() {
        let prepared = prepared(R::Storefront);
        let invocation = prepared.invocation();
        let mut evaluation = Evaluation::default();
        let mut set = None;
        let invoked = prepared
            .invoke(&mut evaluation, &invocation, &mut set)
            .unwrap();
        let outcome = outcome(&evaluation, &invoked, identity(), &invocation, &Sha256).unwrap();
        assert!(outcome.checked.is_some());
        assert!(outcome.evidence.complete());
    }

    /// Each recipe's Shared domain commits whole: the example's with no
    /// Finding, the blocked one's with the Findings after materialization,
    /// and the dense one's with a Finding for each rule at least.
    #[test]
    fn each_recipe_commits_the_shared_domain_whole() {
        let findings = |recipe| {
            let retained = retained_of(recipe);
            assert!(retained.exhausted.is_none(), "{recipe:?}");
            assert!(!retained.slices.is_empty(), "{recipe:?}");
            retained
                .slices
                .iter()
                .flat_map(|slice| &slice.records)
                .filter(|record| record.kind == RecordKind::SharedFinding)
                .count()
        };
        assert_eq!(findings(R::Storefront), 0);
        assert!(findings(R::Blocked) > 0);
        assert!(findings(R::Dense) >= crate::benchmark::cases::DENSE);
    }

    /// The stage commits exactly the records of the outcome's Shared
    /// projection, which finalization only orders (D15:2143).
    #[test]
    fn the_stage_commits_the_records_of_the_shared_projection() {
        for recipe in R::ALL {
            let prepared = prepared(recipe);
            let invocation = prepared.invocation();
            let mut evaluation = Evaluation::default();
            let mut set = None;
            let invoked = prepared
                .invoke(&mut evaluation, &invocation, &mut set)
                .unwrap();
            let retained = Retained::of(
                shared((&evaluation, &invoked, &invocation))
                    .unwrap()
                    .committed(),
            );
            let outcome = outcome(&evaluation, &invoked, identity(), &invocation, &Sha256).unwrap();
            let projection = outcome.evidence.shared.unwrap();
            assert_eq!(
                sorted(
                    retained
                        .slices
                        .into_iter()
                        .flat_map(|slice| slice.records)
                        .map(|record| (record.kind, record.view))
                ),
                sorted(
                    projection
                        .records
                        .into_iter()
                        .map(|(_, retained)| (retained.kind, retained.view))
                ),
                "{recipe:?}"
            );
        }
    }

    /// The capability over `evidence_limits`, but `tight` at its limit, with
    /// `metadata` for its `evidenceMetadata` bounds.
    fn capability(
        tight: Option<(BoundId, u64)>,
        metadata: EvidenceMetadataLimits,
    ) -> ImplementationCapability {
        let current = evidence_limits().bounds();
        let limits = ResolutionLimits::from_bounds(|id| match tight {
            Some((bound, limit)) if bound == id => Bound::new(limit),
            _ => current
                .iter()
                .find(|(bound, _)| *bound == id)
                .map(|&(_, value)| value),
        })
        .unwrap();
        ImplementationCapability::fixture_owned(limits).with_evidence_metadata(metadata)
    }

    /// A Shared domain a bound exhausts is retained with its terminal cursor
    /// and the bound its terminal Finding names, and observed apart from the
    /// one committed whole: a diagnostic bound of the domain, and a count of
    /// the Shared scope past its `evidenceMetadata` bound (D15:1851, 2408).
    #[test]
    fn an_exhausted_shared_domain_is_retained_and_observed_with_its_bound() {
        let records = BoundId {
            group: Group::DiagnosticsShared,
            name: "maxRecords",
        };
        let mut seen = vec![
            observe(&prepared(R::Storefront).resolve_shared().unwrap())
                .unwrap()
                .shared,
        ];
        for (capability, named) in [
            (
                capability(Some((records, 1)), unbounded()),
                ExhaustedBound::Domain(records),
            ),
            (
                capability(None, with(MetadataBound::SubjectOccurrences, 1)),
                ExhaustedBound::Capability(MetadataBound::SubjectOccurrences.id()),
            ),
        ] {
            let mut prepared = prepared(R::Storefront);
            prepared.capability = capability;
            let resolved = prepared.resolve_shared().unwrap();
            let (_, bound) = resolved.result.as_ref().unwrap().exhausted.clone().unwrap();
            assert_eq!(bound, named);
            let observation = observe(&resolved).unwrap();
            assert!(!seen.contains(&observation.shared), "{named:?}");
            seen.push(observation.shared);
        }
    }

    /// How a domain was exhausted is observed whole, apart from a domain
    /// committed whole: the cursor, the bound's group and name, and whether
    /// it is a bound of the domain or of the capability.
    #[test]
    fn how_a_shared_domain_was_exhausted_is_observed_whole() {
        let dense = prepared(R::Dense).resolve_shared().unwrap();
        let slices = &dense.result.as_ref().unwrap().slices;
        let (first, second) = (&slices[0].unit, &slices[1].unit);
        assert_ne!(first, second);
        let bound = |group, name| ExhaustedBound::Domain(BoundId { group, name });
        let mut seen = Vec::new();
        for exhausted in [
            None,
            Some((first, bound(Group::DiagnosticsShared, "maxRecords"))),
            Some((second, bound(Group::DiagnosticsShared, "maxRecords"))),
            Some((first, bound(Group::DiagnosticsShared, "maxFindings"))),
            Some((first, bound(Group::DiagnosticsEntry, "maxRecords"))),
            Some((
                first,
                ExhaustedBound::Capability(MetadataBound::SubjectOccurrences.id()),
            )),
            Some((
                first,
                ExhaustedBound::Capability(MetadataBound::TokenInputs.id()),
            )),
        ] {
            let resolved = Resolved {
                result: Ok(Retained {
                    slices: slices.clone(),
                    exhausted: exhausted.map(|(unit, bound)| (unit.clone(), bound)),
                }),
                scope: dense.scope,
                specification: dense.specification,
            };
            let observation = observe(&resolved).unwrap().shared;
            assert!(!seen.contains(&observation), "{exhausted:?}");
            seen.push(observation);
        }
    }

    /// Each recipe's Shared output has its own checksum, the same each time
    /// it is observed, and no entry-specific Evidence (D15:2794).
    #[test]
    fn every_recipe_has_its_own_shared_observation_and_no_entry_domain() {
        let mut seen = Vec::new();
        for recipe in R::ALL {
            let resolved = prepared(recipe).resolve_shared().unwrap();
            let observation = observe(&resolved).unwrap();
            assert_eq!(observation, observe(&resolved).unwrap());
            assert_eq!(observation.entry, None);
            assert!(!seen.contains(&observation.shared), "{recipe:?}");
            seen.push(observation.shared);
        }
    }

    /// The digest the vocabulary slot asserts is the sealed vocabulary's.
    #[test]
    fn the_vocabulary_slot_asserts_the_sealed_vocabularys_digest() {
        let vocabulary = vocabulary().unwrap().admit().unwrap();
        assert_eq!(vocabulary.semantic_digest().as_str(), VOCABULARY_DIGEST);
        assert_eq!(vocabulary.identity().as_str(), VOCABULARY.0);
        let members: Vec<&str> = vocabulary.members().iter().map(Token::as_str).collect();
        assert_eq!(members, SURFACES);
    }
}
