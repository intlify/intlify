// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Fixed owner-fixture input binding. This covers preparation as well as the
//! actual immutable operation input; it is not a common Measurement Case ID or
//! shared-artifact digest. No acquisition, clock, run, or physical ID enters it.

use crate::benchmark::locale_icu4x::PreparedEngine;
use crate::benchmark::locale_resolution::Canonicalizers;
use crate::benchmark::observation::{document, Digest, Frame};
use crate::benchmark::operation::Prepared;
use crate::benchmark::workflow::{RawEntry, RawInvocation};
use crate::boundary::snapshot::{SnapshotReport, SubmittedNode};
use crate::construction::component::Submitted;
use crate::construction::flow::ConstructionInput;
use crate::input_limits::InputLimits;
use crate::resolver::capability::SnapshotLimits;
use crate::snapshot::SnapshotBound;
use crate::structural::StructuralLimits;

use super::prepare::{selector_input, Candidate};
use super::Recipe;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(in crate::benchmark) enum ContextFailure {
    Encoding,
    OperationMismatch,
    SelectorMismatch,
    PreparationLimitsMismatch,
    LocaleInputMismatch,
    PolicyInputMismatch,
    CollectionInputMismatch,
    BindingInputMismatch,
    TargetInputMismatch,
    HydrationInputMismatch,
    ConstructionInputMismatch,
    EvidenceInputMismatch,
    WorkflowInputMismatch,
    ReplayInputMismatch,
    SnapshotInputMismatch,
}

pub(super) fn observe(candidate: &Candidate) -> Result<Digest, ContextFailure> {
    if candidate.prepared.operation() != candidate.declaration.operation {
        return Err(ContextFailure::OperationMismatch);
    }
    let mut frame = Frame::new("fixture-input-context");
    frame
        .json(&serde_json::to_value(&candidate.declaration).map_err(|_| ContextFailure::Encoding)?);
    // Pin the limits used before the measured operation too. These describe
    // fixture preparation, not additional work inside a component interval.
    match (&candidate.prepared, candidate.input_limits) {
        (
            Prepared::Locale { .. }
            | Prepared::Icu4xLocale { .. }
            | Prepared::ArtifactCollection(_)
            | Prepared::Construction(_)
            | Prepared::ConstructionIdentity(_)
            | Prepared::Workflow(_)
            | Prepared::Replay(_),
            None,
        ) => {
            frame.text("no-file-preparation");
        }
        (
            Prepared::Locale { .. }
            | Prepared::Icu4xLocale { .. }
            | Prepared::ArtifactCollection(_)
            | Prepared::Construction(_)
            | Prepared::ConstructionIdentity(_)
            | Prepared::Workflow(_)
            | Prepared::Replay(_),
            Some(_),
        )
        | (_, None) => return Err(ContextFailure::PreparationLimitsMismatch),
        (_, Some(limits)) => input_limits(&mut frame, limits),
    }
    frame.text(candidate.prepared.operation().boundary());
    match &candidate.prepared {
        Prepared::Entry { source, limits } => {
            frame.bytes(source);
            input_limits(&mut frame, *limits);
        }
        Prepared::Structural {
            schema,
            doc,
            limits,
        } => {
            frame.digest(document(doc).identity());
            frame.json(schema.schema_body());
            structural_limits(&mut frame, *limits);
        }
        Prepared::Authoring(analysis) => {
            // Complete prepared input, including source, schema, bounds, and
            // admitted fragments; not just a claim that preparation succeeded.
            frame.digest(analysis.benchmark_observation().identity());
        }
        Prepared::Select { analysis, selector } => {
            let expected = selector_input(
                candidate.declaration.selector,
                analysis.benchmark_profile_id_bound(),
            );
            if !selector.benchmark_matches_fixture(&expected) {
                return Err(ContextFailure::SelectorMismatch);
            }
            // The declaration already names the fixed non-secret selector
            // recipe. Never hash or disclose an arbitrary submitted string.
            frame.digest(analysis.benchmark_observation().identity());
        }
        Prepared::Locale { core, input } => {
            let Recipe::Locale(recipe) = candidate.declaration.fixture else {
                return Err(ContextFailure::LocaleInputMismatch);
            };
            if input.as_ref() != recipe.spelling() {
                return Err(ContextFailure::LocaleInputMismatch);
            }
            // Only the known, non-secret fixture spelling is observed. Full
            // actual table content and every binding/limit/reuse fact are pinned.
            frame.text(recipe.spelling());
            frame.json(
                &serde_json::to_value(crate::benchmark::locale::InputFacts::observe(core))
                    .map_err(|_| ContextFailure::Encoding)?,
            );
        }
        Prepared::Icu4xLocale { engine, input } => {
            let Recipe::Icu4xLocale {
                input: recipe,
                engine: declared,
            } = candidate.declaration.fixture
            else {
                return Err(ContextFailure::LocaleInputMismatch);
            };
            if input.as_ref() != recipe.spelling() || engine.engine() != declared {
                return Err(ContextFailure::LocaleInputMismatch);
            }
            // The admitted binding, both limits, and the engine state are
            // pinned with the fixture spelling.
            frame.text(recipe.spelling());
            frame.json(
                &serde_json::to_value(crate::benchmark::locale_icu4x::AdmittedInputFacts::observe(
                    engine,
                ))
                .map_err(|_| ContextFailure::Encoding)?,
            );
        }
        Prepared::LocalePolicyResolution(prepared) => {
            // The declared provider and engine state are the prepared ones.
            let declared = match candidate.declaration.fixture {
                Recipe::LocalePolicyResolution(_) => None,
                Recipe::Icu4xLocalePolicyResolution { engine, .. } => Some(engine),
                _ => return Err(ContextFailure::LocaleInputMismatch),
            };
            if declared != prepared.icu4x().map(PreparedEngine::engine)
                || candidate.declaration.selector != super::Selector::App
                || prepared.selected.as_str() != "app"
                || prepared.fields().is_none()
            {
                return Err(ContextFailure::LocaleInputMismatch);
            }
            frame.digest(prepared.analysis.benchmark_observation().identity());
            frame.json(
                &serde_json::to_value(&prepared.config).map_err(|_| ContextFailure::Encoding)?,
            );
            let canonicalizer = match &prepared.canonicalizer {
                Canonicalizers::Finite(core) => {
                    serde_json::to_value(crate::benchmark::locale::InputFacts::observe(core))
                }
                Canonicalizers::Icu4x(engine) => serde_json::to_value(
                    crate::benchmark::locale_icu4x::AdmittedInputFacts::observe(engine),
                ),
            };
            frame.json(&canonicalizer.map_err(|_| ContextFailure::Encoding)?);
            frame.json(
                &serde_json::to_value(crate::benchmark::locale_resolution::InputFacts::observe(
                    prepared,
                ))
                .map_err(|_| ContextFailure::Encoding)?,
            );
        }
        Prepared::ResourcePolicy(prepared) => {
            let expected = selector_input(
                candidate.declaration.selector,
                prepared.analysis.benchmark_profile_id_bound(),
            );
            if !matches!(candidate.declaration.fixture, Recipe::ResourcePolicy(_))
                || candidate.declaration.selector != super::Selector::App
                || !prepared.selector.benchmark_matches_fixture(&expected)
                || prepared.selected.as_str() != "app"
                || prepared.inputs().is_none()
            {
                return Err(ContextFailure::PolicyInputMismatch);
            }
            // The complete configuration, whose reference names the policy;
            // every member in submission order; and every capability bound.
            frame.digest(prepared.analysis.benchmark_observation().identity());
            frame.json(
                &serde_json::to_value(&prepared.config).map_err(|_| ContextFailure::Encoding)?,
            );
            let members = prepared.collection.members();
            frame.uint(u64::try_from(members.len()).map_err(|_| ContextFailure::Encoding)?);
            for member in members {
                frame.bytes(member.canonical_body());
            }
            for (id, bound) in prepared.capability.policy_comparable().bounds() {
                frame.text(id.group.path());
                frame.text(id.name);
                frame.uint(bound.get());
            }
            frame.json(
                &serde_json::to_value(crate::benchmark::resource_policy::InputFacts::observe(
                    prepared,
                ))
                .map_err(|_| ContextFailure::Encoding)?,
            );
        }
        Prepared::ArtifactCollection(prepared) => {
            if !matches!(candidate.declaration.fixture, Recipe::ArtifactCollection(_))
                || candidate.declaration.selector != super::Selector::Absent
            {
                return Err(ContextFailure::CollectionInputMismatch);
            }
            // Every member in submission order, the limits the collection was
            // read under, and every bound of the confirmed policy.
            let members = prepared.collection.members();
            frame.uint(u64::try_from(members.len()).map_err(|_| ContextFailure::Encoding)?);
            for member in members {
                frame.bytes(member.canonical_body());
            }
            let read = super::prepare::COLLECTION_LIMITS;
            for bound in [read.member_bytes, read.total_bytes, read.depth] {
                frame.uint(bound);
            }
            for (id, bound) in prepared.policy.limits().bounds() {
                frame.text(id.group.path());
                frame.text(id.name);
                frame.uint(bound.get());
            }
            frame.json(
                &serde_json::to_value(crate::benchmark::artifact_collection::InputFacts::observe(
                    prepared,
                ))
                .map_err(|_| ContextFailure::Encoding)?,
            );
        }
        Prepared::ResolvedBinding(prepared) => {
            // The inputs exist only for a complete root whose configuration
            // declares the selected profile.
            if !matches!(candidate.declaration.fixture, Recipe::ResolvedBinding(_))
                || candidate.declaration.selector != super::Selector::App
                || prepared.inputs().is_none()
            {
                return Err(ContextFailure::BindingInputMismatch);
            }
            // The complete configuration, whose selected declaration the
            // references come from; every member in submission order; the
            // limits the collection was read under; and every bound of the
            // confirmed policy.
            frame.digest(prepared.analysis.benchmark_observation().identity());
            frame.json(
                &serde_json::to_value(&prepared.config).map_err(|_| ContextFailure::Encoding)?,
            );
            let members = prepared.collection.members();
            frame.uint(u64::try_from(members.len()).map_err(|_| ContextFailure::Encoding)?);
            for member in members {
                frame.bytes(member.canonical_body());
            }
            let read = super::prepare::COLLECTION_LIMITS;
            for bound in [read.member_bytes, read.total_bytes, read.depth] {
                frame.uint(bound);
            }
            for (id, bound) in prepared.policy.limits().bounds() {
                frame.text(id.group.path());
                frame.text(id.name);
                frame.uint(bound.get());
            }
            frame.json(
                &serde_json::to_value(crate::benchmark::resolved_binding::InputFacts::observe(
                    prepared,
                ))
                .map_err(|_| ContextFailure::Encoding)?,
            );
        }
        Prepared::TargetAndGroup(prepared) => {
            // The inputs exist only for a complete root whose configuration
            // declares the selected profile.
            if !matches!(candidate.declaration.fixture, Recipe::TargetAndGroup(_))
                || candidate.declaration.selector != super::Selector::App
                || prepared
                    .set()
                    .is_none_or(|set| prepared.inputs(&set).is_none())
            {
                return Err(ContextFailure::TargetInputMismatch);
            }
            // The complete configuration, whose selected declaration the
            // targets and groups come from; the finite provider's complete
            // table and binding; every member in submission order; the
            // limits the collection was read under; and every bound of the
            // confirmed policy.
            frame.digest(prepared.analysis.benchmark_observation().identity());
            frame.json(
                &serde_json::to_value(&prepared.config).map_err(|_| ContextFailure::Encoding)?,
            );
            frame.json(
                &serde_json::to_value(crate::benchmark::locale::InputFacts::observe(
                    &prepared.canonicalizer,
                ))
                .map_err(|_| ContextFailure::Encoding)?,
            );
            let members = prepared.collection.members();
            frame.uint(u64::try_from(members.len()).map_err(|_| ContextFailure::Encoding)?);
            for member in members {
                frame.bytes(member.canonical_body());
            }
            let read = super::prepare::COLLECTION_LIMITS;
            for bound in [read.member_bytes, read.total_bytes, read.depth] {
                frame.uint(bound);
            }
            for (id, bound) in prepared.policy.limits().bounds() {
                frame.text(id.group.path());
                frame.text(id.name);
                frame.uint(bound.get());
            }
            frame.json(
                &serde_json::to_value(crate::benchmark::target_group::InputFacts::observe(
                    prepared,
                ))
                .map_err(|_| ContextFailure::Encoding)?,
            );
        }
        Prepared::Hydration(prepared) => {
            // The inputs exist only for a complete root whose configuration
            // declares the selected profile.
            if !matches!(candidate.declaration.fixture, Recipe::Hydration(_))
                || candidate.declaration.selector != super::Selector::App
                || prepared
                    .set()
                    .is_none_or(|set| prepared.inputs(&set).is_none())
            {
                return Err(ContextFailure::HydrationInputMismatch);
            }
            // As for Phases 11 and 12. Each Target Profile's roles are in
            // its member's body.
            frame.digest(prepared.analysis.benchmark_observation().identity());
            frame.json(
                &serde_json::to_value(&prepared.config).map_err(|_| ContextFailure::Encoding)?,
            );
            frame.json(
                &serde_json::to_value(crate::benchmark::locale::InputFacts::observe(
                    &prepared.canonicalizer,
                ))
                .map_err(|_| ContextFailure::Encoding)?,
            );
            let members = prepared.collection.members();
            frame.uint(u64::try_from(members.len()).map_err(|_| ContextFailure::Encoding)?);
            for member in members {
                frame.bytes(member.canonical_body());
            }
            let read = super::prepare::COLLECTION_LIMITS;
            for bound in [read.member_bytes, read.total_bytes, read.depth] {
                frame.uint(bound);
            }
            for (id, bound) in prepared.policy.limits().bounds() {
                frame.text(id.group.path());
                frame.text(id.name);
                frame.uint(bound.get());
            }
            frame.json(
                &serde_json::to_value(crate::benchmark::hydration_relation::InputFacts::observe(
                    prepared,
                ))
                .map_err(|_| ContextFailure::Encoding)?,
            );
        }
        Prepared::Construction(prepared) => {
            construction_input(&mut frame, candidate, &prepared.input)?;
        }
        Prepared::ConstructionIdentity(prepared) => {
            construction_input(&mut frame, candidate, prepared.resolver.input())?;
        }
        Prepared::SharedEvidence(prepared) => evidence_input(
            &mut frame,
            candidate,
            prepared,
            &crate::benchmark::evidence::InputFacts::observe(prepared),
        )?,
        Prepared::EntryEvidence(prepared) => evidence_input(
            &mut frame,
            candidate,
            prepared,
            &crate::benchmark::evidence::InputFacts::entry(prepared),
        )?,
        // Finalization also takes the committed resolver's identity.
        Prepared::OutcomeFinalization(prepared) => {
            evidence_input(
                &mut frame,
                candidate,
                &prepared.evidence,
                &crate::benchmark::evidence::InputFacts::outcome(&prepared.evidence),
            )?;
            frame.text(&prepared.identity.text());
        }
        Prepared::Workflow(prepared) => workflow_input(&mut frame, candidate, prepared)?,
        Prepared::Snapshot(prepared) => {
            snapshot_input(&mut frame, &prepared.report, prepared.limits)?;
        }
        Prepared::Replay(prepared) => replay_input(&mut frame, candidate, prepared)?,
    }
    Ok(frame.finish())
}

/// The replay of the Materialized Resolution Input Set of one file entry's
/// invocation on the resolver its preparation constructed from the committed
/// input: every raw value of that invocation's submission, as the recipe
/// rebuilds it on that resolver, which binds the set, the one that
/// invocation materializes there (D15:2657, 2659).
fn replay_input(
    frame: &mut Frame,
    candidate: &Candidate,
    prepared: &crate::benchmark::reresolution::PreparedReplay,
) -> Result<(), ContextFailure> {
    use crate::benchmark::workflow::materialized;
    use crate::resolver::invocation::Resolver;

    let Recipe::Replay(recipe) = candidate.declaration.fixture else {
        return Err(ContextFailure::ReplayInputMismatch);
    };
    let (constructed, root) = (&prepared.constructed, prepared.root);
    let entry = super::EntryKind::File;
    let rebuilt = super::workflow_inputs::replayed(recipe, entry, constructed, root);
    let set = || materialized(&Resolver::new(constructed, root), &prepared.raw)?.1;
    if candidate.declaration.selector != super::Selector::App
        || rebuilt.as_ref() != Some(&prepared.raw)
        || set().as_ref() != Some(&prepared.set)
    {
        return Err(ContextFailure::ReplayInputMismatch);
    }
    submitted_input(frame, &prepared.raw)
}

/// A Snapshot's admission: the formal Snapshot as submitted, its supplied
/// digest, then its tree node by node in submission preorder, an object's
/// keys before its values and a number by its bits, so member order and
/// negative zero are bound as submitted; then each bound it is admitted
/// under. Read without recursion (plan 025 decision #298).
fn snapshot_input(
    frame: &mut Frame,
    report: &SnapshotReport,
    limits: SnapshotLimits,
) -> Result<(), ContextFailure> {
    let SnapshotReport::Formal { root, digest } = report else {
        return Err(ContextFailure::SnapshotInputMismatch);
    };
    frame.text(digest);
    submitted_tree(frame, root)?;
    for bound in SnapshotBound::ALL {
        frame.uint(bound.limit(&limits).get());
    }
    Ok(())
}

/// A submitted tree node by node in submission preorder, an object's keys
/// before its values and a number by its bits, read without recursion (plan
/// 025 decision #298).
fn submitted_tree(frame: &mut Frame, root: &SubmittedNode) -> Result<(), ContextFailure> {
    let count = |len: usize| u64::try_from(len).map_err(|_| ContextFailure::Encoding);
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        match node {
            SubmittedNode::Null => frame.uint(0),
            SubmittedNode::Boolean(value) => {
                frame.uint(1);
                frame.flag(*value);
            }
            SubmittedNode::Number(value) => {
                frame.uint(2);
                frame.uint(value.to_bits());
            }
            SubmittedNode::String(bytes) => {
                frame.uint(3);
                frame.bytes(bytes);
            }
            SubmittedNode::Array(elements) => {
                frame.uint(4);
                frame.uint(count(elements.len())?);
                pending.extend(elements.iter().rev());
            }
            SubmittedNode::Object(members) => {
                frame.uint(5);
                frame.uint(count(members.len())?);
                for (key, _) in members {
                    frame.bytes(key);
                }
                pending.extend(members.iter().rev().map(|(_, value)| value));
            }
            SubmittedNode::Marker(marker) => {
                frame.uint(6);
                frame.text(&marker.reason);
                frame.flag(marker.bound.is_some());
                if let Some(bound) = &marker.bound {
                    frame.text(bound);
                }
                frame.flag(marker.witness.is_some());
                if let Some(witness) = marker.witness {
                    frame.uint(witness);
                }
            }
        }
    }
    Ok(())
}

/// One entry's complete invocation on a resolver its preparation
/// constructed from the committed input, in the declared state: the
/// resolver, by the identity the submission names, every raw value a binding
/// submits, and the case's facts, which state what each repetition runs
/// before its start mark; and, for a resolver reused after a blocked
/// outcome, every raw value of the Blocked recipe's submission it resolves
/// first. Each is as the recipe and the state rebuild it on that resolver.
fn workflow_input(
    frame: &mut Frame,
    candidate: &Candidate,
    prepared: &crate::benchmark::workflow::PreparedWorkflow,
) -> Result<(), ContextFailure> {
    use crate::benchmark::workflow::{Before, InputFacts};

    let (root, raw) = (prepared.root, &prepared.raw);
    let fixture = candidate.declaration.fixture;
    let rebuilt = super::workflow_inputs::rebuilt(fixture, &prepared.constructed, root);
    if candidate.declaration.selector != super::Selector::App
        || !rebuilt.is_some_and(|(rebuilt, before)| rebuilt == *raw && before == prepared.before)
    {
        return Err(ContextFailure::WorkflowInputMismatch);
    }
    submitted_input(frame, raw)?;
    let facts = InputFacts::observe(prepared);
    frame.json(&serde_json::to_value(facts).map_err(|_| ContextFailure::Encoding)?);
    if let Before::AfterBlocked(blocked) = &prepared.before {
        submitted_input(frame, blocked)?;
    }
    Ok(())
}

/// What a binding submits for one entry's invocation: the Specification
/// Input, the vocabulary, the canonicalization data's manifest and payloads,
/// every member in submission order, the Verification Input and the
/// assertion beside it, the capability, the file or the Snapshot as
/// submitted and its revision, and the selector, under the identity the set
/// names.
fn submitted_input(frame: &mut Frame, raw: &RawInvocation) -> Result<(), ContextFailure> {
    let count = |len: usize| u64::try_from(len).map_err(|_| ContextFailure::Encoding);
    frame.text(&raw.reference);
    frame.bytes(&raw.specification);
    frame.bytes(&raw.vocabulary);
    frame.bytes(raw.manifest);
    frame.uint(count(raw.payloads.len())?);
    for (address, payload) in &raw.payloads {
        frame.text(address);
        frame.bytes(payload);
    }
    frame.uint(count(raw.members.len())?);
    for member in &raw.members {
        frame.bytes(member);
    }
    frame.bytes(&raw.verification);
    frame.text(raw.assertion.identity.as_str());
    frame.text(raw.assertion.revision.as_str());
    frame.text(raw.assertion.semantic_digest.as_str());
    frame.json(&raw.capability);
    match &raw.entry {
        RawEntry::File { bytes, revision } => {
            frame.bytes(bytes);
            frame.text(revision);
        }
        RawEntry::Programmatic { snapshot, revision } => {
            let SnapshotReport::Formal { root, digest } = snapshot else {
                return Err(ContextFailure::WorkflowInputMismatch);
            };
            frame.text("programmatic");
            frame.text(digest);
            submitted_tree(frame, root)?;
            frame.text(revision);
        }
    }
    frame.text(&raw.selector);
    Ok(())
}

/// One file entry's complete invocation: the recipe's configuration file
/// under the App selector, the bounds it is read and analyzed under and the
/// schema, every member in submission order and the limits the collection
/// was read under, every bound of the capability, whose `policyComparable`
/// bounds also classify the file's keys, the Verification Input's pins,
/// every specification asserted and what the resolver supports in each slot
/// it compares, the fixture provider's pins, the vocabulary, and the stage's
/// `facts`.
fn evidence_input(
    frame: &mut Frame,
    candidate: &Candidate,
    prepared: &crate::benchmark::evidence::PreparedEvidence,
    facts: &crate::benchmark::evidence::InputFacts,
) -> Result<(), ContextFailure> {
    use crate::locale::fixtures::fixture_pins;
    use crate::locale::VersionedIdentity;

    let (recipe, programmatic) = match candidate.declaration.fixture {
        Recipe::Evidence(recipe) => (recipe, None),
        Recipe::ProgrammaticEvidence(recipe) => (recipe.file(), Some(recipe)),
        _ => return Err(ContextFailure::EvidenceInputMismatch),
    };
    let expected = selector_input(
        candidate.declaration.selector,
        prepared.structural.max_profile_id_bytes,
    );
    // The file under the recipe's own bounds, which recipes alike but for
    // their bounds tell apart, or, at an edge, under the bounds the
    // capability and the selected policy set, which the edge's bound changes;
    // the capability's bounds are framed below.
    let source = match candidate.declaration.limit {
        None => recipe.source(&recipe.limits()),
        Some(_) => recipe.source(prepared.capability.policy_comparable()),
    };
    if candidate.declaration.selector != super::Selector::App
        || !prepared.selector.benchmark_matches_fixture(&expected)
        || *prepared.source != *source
    {
        return Err(ContextFailure::EvidenceInputMismatch);
    }
    // A programmatic entry's Snapshot is the one the file's value rebuilds.
    let rebuilt = programmatic
        .map(|recipe| {
            let source = std::sync::Arc::clone(&prepared.source);
            super::workflow_inputs::admitted(recipe, source)
                .ok_or(ContextFailure::EvidenceInputMismatch)
        })
        .transpose()?;
    if rebuilt != prepared.snapshot {
        return Err(ContextFailure::EvidenceInputMismatch);
    }
    frame.bytes(&prepared.source);
    input_limits(frame, prepared.input);
    if let Some(snapshot) = &prepared.snapshot {
        frame.text(snapshot.digest());
        frame.bytes(&snapshot.root().framed());
    }
    structural_limits(frame, prepared.structural);
    frame.json(prepared.schema.schema_body());
    let members = prepared.collection.members();
    frame.uint(u64::try_from(members.len()).map_err(|_| ContextFailure::Encoding)?);
    for member in members {
        frame.bytes(member.canonical_body());
    }
    let read = super::prepare::COLLECTION_LIMITS;
    for bound in [read.member_bytes, read.total_bytes, read.depth] {
        frame.uint(bound);
    }
    for (id, bound) in prepared.capability.policy_comparable().bounds() {
        frame.text(id.group.path());
        frame.text(id.name);
        frame.uint(bound.get());
    }
    for (id, bound) in prepared.capability.bootstrap_only().bounds() {
        frame.text(id.group.path());
        frame.text(id.name);
        frame.uint(bound.get());
    }
    frame.json(&prepared.verification.normalized());
    let mut versioned = |identity: Option<&VersionedIdentity<&str>>| {
        frame.flag(identity.is_some());
        if let Some(identity) = identity {
            frame.text(identity.identity);
            frame.text(identity.revision);
        }
    };
    let asserted = &prepared.specifications;
    versioned(asserted.project_profile.as_ref());
    versioned(asserted.locale_canonicalization.as_ref());
    versioned(asserted.locale_negotiation.as_ref());
    versioned(asserted.message_locale_fallback.as_ref());
    versioned(asserted.coverage.as_ref());
    versioned(asserted.delivery_graph.as_ref());
    versioned(asserted.delivery_placement.as_ref());
    let vocabulary = asserted
        .intent_surface_class_vocabulary
        .as_ref()
        .ok_or(ContextFailure::EvidenceInputMismatch)?;
    frame.text(vocabulary.identity);
    frame.text(vocabulary.revision);
    frame.text(vocabulary.semantic_digest);
    // What the resolver supports in each slot, which it compares the
    // assertion with; the vocabulary slot has no supported value.
    let supported = &prepared.supported;
    let active = supported.project_profile();
    frame.text(active.specification.identity);
    frame.text(active.specification.revision);
    frame.text(active.semantic_digest);
    let canonicalization = supported.locale_canonicalization().entries();
    frame.uint(u64::try_from(canonicalization.len()).map_err(|_| ContextFailure::Encoding)?);
    for pins in canonicalization {
        frame.text(pins.specification.identity);
        frame.text(pins.specification.revision);
        frame.text(pins.dataset.identity);
        frame.text(pins.dataset.semantic_digest);
    }
    for specification in [
        supported.locale_negotiation(),
        supported.message_locale_fallback(),
        supported.coverage(),
        supported.delivery_graph(),
        supported.delivery_placement(),
    ] {
        frame.text(specification.identity);
        frame.text(specification.revision);
    }
    let pins = fixture_pins();
    frame.text(pins.specification.identity);
    frame.text(pins.specification.revision);
    frame.text(pins.dataset.identity);
    frame.text(pins.dataset.semantic_digest);
    let intact = prepared
        .vocabulary
        .admit()
        .map_err(|_| ContextFailure::EvidenceInputMismatch)?;
    frame.text(intact.identity().as_str());
    frame.text(intact.revision().as_str());
    frame.uint(u64::try_from(intact.members().len()).map_err(|_| ContextFailure::Encoding)?);
    for member in intact.members() {
        frame.text(member.as_str());
    }
    frame.text(intact.semantic_digest().as_str());
    frame.json(&serde_json::to_value(facts).map_err(|_| ContextFailure::Encoding)?);
    Ok(())
}

/// Every byte of the input construction is given, and each component's
/// tuple: the case's input, and nothing else.
fn construction_input(
    frame: &mut Frame,
    candidate: &Candidate,
    input: &ConstructionInput,
) -> Result<(), ContextFailure> {
    let expected = super::construction_inputs::case_input(&candidate.declaration)
        .ok_or(ContextFailure::ConstructionInputMismatch)?;
    if candidate.declaration.selector != super::Selector::Absent || *input != expected.0 {
        return Err(ContextFailure::ConstructionInputMismatch);
    }
    let mut component = |submitted: Option<&Submitted>| -> Result<(), ContextFailure> {
        frame.flag(submitted.is_some());
        if let Some(submitted) = submitted {
            frame.json(&submitted.tuple);
            frame.bytes(submitted.artifact.bytes());
        }
        Ok(())
    };
    component(input.profile_specification_schema.as_ref())?;
    component(input.profile_specification.as_ref())?;
    let set = input.configuration_schema_authority_set.as_ref();
    component(set.map(|set| &set.component))?;
    component(input.resolver_capability_specification.as_ref())?;
    let package = input.registry_package.as_ref();
    component(package.map(|package| &package.component))?;
    // The schemas the set is given, mapped or not, by `schemaVersion`; the
    // package's members by role.
    let schemas = set.map(|set| &set.members);
    frame.uint(schemas.map_or(0, |schemas| {
        u64::try_from(schemas.len()).unwrap_or(u64::MAX)
    }));
    for (version, schema) in schemas.into_iter().flatten() {
        frame.text(version);
        frame.bytes(schema.bytes());
    }
    let members = package.map(|package| &package.members);
    frame.uint(members.map_or(0, |members| {
        u64::try_from(members.len()).unwrap_or(u64::MAX)
    }));
    for (role, member) in members.into_iter().flatten() {
        frame.text(role.name());
        frame.bytes(member.bytes());
    }
    Ok(())
}

fn input_limits(frame: &mut Frame, limits: InputLimits) {
    for bound in [
        limits.raw.max_file_bytes,
        limits.raw.max_parser_tokens,
        limits.value.max_nodes,
        limits.value.max_depth,
        limits.value.max_collection_entries,
        limits.value.max_total_string_bytes,
        limits.value.max_single_string_bytes,
    ] {
        frame.uint(bound.get());
    }
}

fn structural_limits(frame: &mut Frame, limits: StructuralLimits) {
    for bound in [
        limits.max_profiles,
        limits.max_profile_id_bytes,
        limits.max_structural_analysis_units,
    ] {
        frame.uint(bound.get());
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::benchmark::cases::{EvidenceRecipe, ResolverState, Selector};
    use crate::benchmark::workflow::tests::{candidate, candidate_in};
    use crate::benchmark::workflow::{Before, PreparedWorkflow};
    use crate::boundary::snapshot::SubmittedMarker;

    type Edit = (&'static str, fn(&mut RawInvocation));

    fn workflow(candidate: &mut Candidate) -> &mut PreparedWorkflow {
        let Prepared::Workflow(prepared) = &mut candidate.prepared else {
            unreachable!()
        };
        prepared
    }

    /// A file entry's bytes and their revision.
    fn file(raw: &mut RawInvocation) -> (&mut Arc<[u8]>, &mut String) {
        let RawEntry::File { bytes, revision } = &mut raw.entry else {
            unreachable!()
        };
        (bytes, revision)
    }

    fn raw(candidate: &mut Candidate) -> &mut RawInvocation {
        &mut workflow(candidate).raw
    }

    /// The Blocked recipe's submission a case reused after a blocked outcome
    /// resolves first.
    fn blocked(candidate: &mut Candidate) -> &mut RawInvocation {
        let Before::AfterBlocked(blocked) = &mut workflow(candidate).before else {
            unreachable!()
        };
        blocked
    }

    /// A complete invocation's context binds its recipe, the resolver's
    /// state, its selector, and every raw value a binding submits: a case any
    /// one of which is not what its recipe's submission rebuilds is refused.
    #[test]
    fn a_workflow_context_refuses_any_input_its_recipe_does_not_rebuild() {
        let mut candidate = candidate(EvidenceRecipe::Storefront);
        let context = observe(&candidate);
        let original = raw(&mut candidate).clone();
        let edits: [Edit; 12] = [
            ("reference", |raw| raw.reference.push('0')),
            ("specification", |raw| raw.specification.push(b' ')),
            ("vocabulary", |raw| raw.vocabulary.push(b' ')),
            ("manifest", |raw| raw.manifest = &b"{}"[..]),
            ("payload", |raw| _ = raw.payloads.pop_first()),
            ("members", |raw| raw.members.reverse()),
            ("verification", |raw| raw.verification.push(b' ')),
            ("assertion", |raw| {
                raw.assertion.revision = raw.assertion.identity.clone();
            }),
            ("capability", |raw| {
                raw.capability["bootstrapOnly"] = json!({});
            }),
            ("file", |raw| *file(raw).0 = Arc::from(&b"{}"[..])),
            ("revision", |raw| file(raw).1.push('0')),
            ("selector", |raw| raw.selector.push('0')),
        ];
        for (name, edit) in edits {
            edit(raw(&mut candidate));
            let refused = Err(ContextFailure::WorkflowInputMismatch);
            assert_eq!(observe(&candidate), refused, "{name}");
            *raw(&mut candidate) = original.clone();
        }
        assert!(context.is_ok() && observe(&candidate) == context);
        let declared = candidate.declaration.fixture;
        for fixture in [
            Recipe::Workflow {
                input: EvidenceRecipe::Storefront,
                resolver: ResolverState::Fresh,
            },
            Recipe::Workflow {
                input: EvidenceRecipe::Dense,
                resolver: ResolverState::Reused,
            },
        ] {
            candidate.declaration.fixture = fixture;
            let refused = Err(ContextFailure::WorkflowInputMismatch);
            assert_eq!(observe(&candidate), refused, "{fixture:?}");
        }
        candidate.declaration.fixture = declared;
        candidate.declaration.selector = Selector::Absent;
        let refused = Err(ContextFailure::WorkflowInputMismatch);
        assert_eq!(observe(&candidate), refused);
        candidate.declaration.selector = Selector::App;
        assert_eq!(observe(&candidate), context);
    }

    /// D15:2751, 2824: a case reused after a blocked outcome binds the
    /// Blocked recipe's submission it resolves first as well, and every case
    /// the state its preparation runs: a case whose first submission is not
    /// what the Blocked recipe's rebuilds, or whose declared state is not the
    /// prepared one, is refused.
    #[test]
    fn a_workflow_context_binds_the_state_and_what_it_resolves_first() {
        let state = ResolverState::ReusedAfterBlocked;
        let mut candidate = candidate_in(EvidenceRecipe::Storefront, state);
        let context = observe(&candidate);
        let original = blocked(&mut candidate).clone();
        let refused = Err(ContextFailure::WorkflowInputMismatch);
        let edits: [Edit; 3] = [
            ("file", |raw| *file(raw).0 = Arc::from(&b"{}"[..])),
            ("members", |raw| raw.members.reverse()),
            ("selector", |raw| raw.selector.push('0')),
        ];
        for (name, edit) in edits {
            edit(blocked(&mut candidate));
            assert_eq!(observe(&candidate), refused, "{name}");
            *blocked(&mut candidate) = original.clone();
        }
        assert!(context.is_ok() && observe(&candidate) == context);
        for declared in [ResolverState::Fresh, ResolverState::Reused] {
            candidate.declaration.fixture = Recipe::Workflow {
                input: EvidenceRecipe::Storefront,
                resolver: declared,
            };
            assert_eq!(observe(&candidate), refused, "{declared:?}");
        }
        candidate.declaration.fixture = Recipe::Workflow {
            input: EvidenceRecipe::Storefront,
            resolver: state,
        };
        let kept = std::mem::replace(&mut workflow(&mut candidate).before, Before::Fresh);
        assert_eq!(observe(&candidate), refused);
        workflow(&mut candidate).before = kept;
        assert_eq!(observe(&candidate), context);
    }

    /// D15:2788, 2824; plan 025 decision #316: a programmatic entry's
    /// invocation binds its Snapshot as submitted and its revision, and so
    /// does the Blocked recipe's it resolves first: either changed is
    /// refused, a file in its place is another operation's input, and the
    /// file case's recipe is refused.
    #[test]
    fn a_programmatic_context_binds_the_snapshot_as_submitted() {
        use crate::benchmark::cases::ProgrammaticRecipe;
        use crate::benchmark::workflow::tests::programmatic_in;

        let state = ResolverState::ReusedAfterBlocked;
        let mut candidate = programmatic_in(ProgrammaticRecipe::Storefront, state);
        let context = observe(&candidate);
        let refused = Err(ContextFailure::WorkflowInputMismatch);
        let edits: [Edit; 3] = [
            ("snapshot", |raw| {
                let RawEntry::Programmatic { snapshot, .. } = &mut raw.entry else {
                    unreachable!()
                };
                members(snapshot).reverse();
            }),
            ("revision", |raw| {
                let RawEntry::Programmatic { revision, .. } = &mut raw.entry else {
                    unreachable!()
                };
                revision.push('0');
            }),
            ("entry", |raw| {
                raw.entry = RawEntry::File {
                    bytes: Arc::from(&b"{}"[..]),
                    revision: String::new(),
                };
            }),
        ];
        for (name, edit) in edits {
            let original = raw(&mut candidate).clone();
            edit(raw(&mut candidate));
            let other = Err(ContextFailure::OperationMismatch);
            let expected = if name == "entry" { other } else { refused };
            assert_eq!(observe(&candidate), expected, "{name}");
            *raw(&mut candidate) = original;
            let original = blocked(&mut candidate).clone();
            edit(blocked(&mut candidate));
            assert_eq!(observe(&candidate), refused, "{name} first");
            *blocked(&mut candidate) = original;
        }
        assert!(context.is_ok() && observe(&candidate) == context);
        candidate.declaration.fixture = Recipe::Workflow {
            input: EvidenceRecipe::Storefront,
            resolver: state,
        };
        assert_eq!(observe(&candidate), refused);
    }

    /// D15:2657, 2659: a replay's context binds the submission whose
    /// invocation's set it replays, and that set: another recipe's
    /// submission or set in its place, a file spaced otherwise, whose set is
    /// the same, a declaration of another selector or recipe, and file
    /// preparation limits it has none of are refused.
    #[test]
    fn a_replay_context_binds_its_submission_and_its_set() {
        use crate::benchmark::cases::prepare::prepare;
        use crate::benchmark::cases::{declarations, ReplayRecipe};
        use crate::benchmark::reresolution::PreparedReplay;

        fn replay(candidate: &mut Candidate) -> &mut PreparedReplay {
            let Prepared::Replay(prepared) = &mut candidate.prepared else {
                unreachable!()
            };
            prepared
        }
        let find = |recipe| {
            let fixture = Recipe::Replay(recipe);
            let declaration = declarations()
                .into_iter()
                .find(|case| case.fixture == fixture);
            prepare(&declaration.unwrap()).unwrap()
        };
        let mut candidate = find(ReplayRecipe::Storefront);
        let mut other = find(ReplayRecipe::EntryWithheld);
        let context = observe(&candidate);
        let refused = Err(ContextFailure::ReplayInputMismatch);
        std::mem::swap(&mut replay(&mut candidate).raw, &mut replay(&mut other).raw);
        assert_eq!(observe(&candidate), refused);
        std::mem::swap(&mut replay(&mut candidate).raw, &mut replay(&mut other).raw);
        std::mem::swap(&mut replay(&mut candidate).set, &mut replay(&mut other).set);
        assert_eq!(observe(&candidate), refused);
        std::mem::swap(&mut replay(&mut candidate).set, &mut replay(&mut other).set);
        let kept = replay(&mut candidate).raw.clone();
        let mut spaced = kept.file().to_vec();
        spaced.push(b' ');
        let bytes: std::sync::Arc<[u8]> = spaced.into();
        let revision = crate::boundary::source::file_source_revision(&bytes);
        replay(&mut candidate).raw.entry = RawEntry::File { bytes, revision };
        assert_eq!(observe(&candidate), refused);
        replay(&mut candidate).raw = kept;
        let declared = candidate.declaration.clone();
        candidate.declaration.selector = crate::benchmark::cases::Selector::Absent;
        assert_eq!(observe(&candidate), refused);
        candidate.declaration = declared.clone();
        candidate.declaration.fixture = Recipe::Minimal;
        assert_eq!(observe(&candidate), refused);
        candidate.declaration = declared;
        candidate.input_limits = Some(crate::materialize_tests::limits());
        let limits = Err(ContextFailure::PreparationLimitsMismatch);
        assert_eq!(observe(&candidate), limits);
        candidate.input_limits = None;
        assert!(context.is_ok() && observe(&candidate) == context);
    }

    /// D15:2788, 2824: a programmatic Evidence stage's context binds the
    /// Snapshot its entry enters with: none, or another recipe's, in its
    /// place is refused.
    #[test]
    fn a_programmatic_evidence_context_binds_its_snapshot() {
        use crate::benchmark::cases::prepare::prepare;
        use crate::benchmark::cases::workflow_inputs::admitted;
        use crate::benchmark::cases::{declarations, ProgrammaticRecipe};
        use crate::boundary::snapshot::AdmittedSnapshot;

        fn snapshot(candidate: &mut Candidate) -> &mut Option<AdmittedSnapshot> {
            let Prepared::SharedEvidence(prepared) = &mut candidate.prepared else {
                unreachable!()
            };
            &mut prepared.snapshot
        }
        let recipe = Recipe::ProgrammaticEvidence(ProgrammaticRecipe::Storefront);
        let declaration = declarations()
            .into_iter()
            .find(|case| case.fixture == recipe);
        let mut candidate = prepare(&declaration.unwrap()).unwrap();
        let context = observe(&candidate);
        let refused = Err(ContextFailure::EvidenceInputMismatch);
        let kept = snapshot(&mut candidate).take();
        assert_eq!(observe(&candidate), refused);
        let Prepared::SharedEvidence(prepared) = &candidate.prepared else {
            unreachable!()
        };
        let other = admitted(
            ProgrammaticRecipe::RejectedMarker,
            Arc::clone(&prepared.source),
        );
        *snapshot(&mut candidate) = other;
        assert_eq!(observe(&candidate), refused);
        *snapshot(&mut candidate) = kept;
        assert!(context.is_ok() && observe(&candidate) == context);
    }

    /// The members of a formal Snapshot's root object.
    fn members(report: &mut SnapshotReport) -> &mut Vec<(Vec<u8>, SubmittedNode)> {
        let SnapshotReport::Formal {
            root: SubmittedNode::Object(members),
            ..
        } = report
        else {
            unreachable!()
        };
        members
    }

    /// The marker a marked recipe's last root member holds.
    fn marker(report: &mut SnapshotReport) -> &mut SubmittedMarker {
        let Some((_, SubmittedNode::Marker(marker))) = members(report).last_mut() else {
            unreachable!()
        };
        marker
    }

    /// A Snapshot admission's context binds the tree as submitted, its
    /// digest, and its bounds: reordered members, a renamed key, a number's
    /// sign, any field of a marker, the digest, or one bound each changes it,
    /// and a report with no Snapshot is refused.
    #[test]
    fn a_snapshot_context_binds_the_tree_as_submitted_and_its_bounds() {
        use crate::benchmark::cases::SnapshotRecipe as S;
        use crate::benchmark::snapshot_admission::tests::candidate as snapshot;
        use crate::input_limits::Bound;

        type Change = (S, fn(&mut SnapshotReport, &mut SnapshotLimits));
        let changes: [Change; 8] = [
            (S::Minimal, |report, _| members(report).reverse()),
            (S::Minimal, |report, _| members(report)[0].0.push(b'x')),
            (S::NegativeZero, |report, _| {
                let SnapshotReport::Formal { root, .. } = report else {
                    unreachable!()
                };
                *root = SubmittedNode::Number(0.0);
            }),
            (S::Cycle, |report, _| {
                marker(report).reason = "unsupported-type".to_owned();
            }),
            (S::Cycle, |report, _| {
                marker(report).bound = Some("bootstrapOnly.snapshot.maxNodes".to_owned());
            }),
            (S::Cycle, |report, _| marker(report).witness = Some(1)),
            (S::Minimal, |report, _| {
                let SnapshotReport::Formal { digest, .. } = report else {
                    unreachable!()
                };
                digest.push('0');
            }),
            (S::Minimal, |_, limits| {
                limits.max_traversal_units = Bound::new(1).unwrap();
            }),
        ];
        for (index, (recipe, change)) in changes.into_iter().enumerate() {
            let mut candidate = snapshot(recipe);
            let context = observe(&candidate).unwrap();
            let Prepared::Snapshot(prepared) = &mut candidate.prepared else {
                unreachable!()
            };
            change(&mut prepared.report, &mut prepared.limits);
            assert_ne!(observe(&candidate).unwrap(), context, "{index}");
        }
        let mut candidate = snapshot(S::Minimal);
        let Prepared::Snapshot(prepared) = &mut candidate.prepared else {
            unreachable!()
        };
        prepared.report = SnapshotReport::Unsafe;
        let refused = Err(ContextFailure::SnapshotInputMismatch);
        assert_eq!(observe(&candidate), refused);
    }
}
