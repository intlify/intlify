// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Fixed owner-fixture input binding. This covers preparation as well as the
//! actual immutable operation input; it is not a common Measurement Case ID or
//! shared-artifact digest. No acquisition, clock, run, or physical ID enters it.

use crate::benchmark::locale_icu4x::PreparedEngine;
use crate::benchmark::locale_resolution::Canonicalizers;
use crate::benchmark::observation::{document, Digest, Frame};
use crate::benchmark::operation::Prepared;
use crate::construction::component::Submitted;
use crate::construction::flow::ConstructionInput;
use crate::input_limits::InputLimits;
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
            | Prepared::ConstructionIdentity(_),
            None,
        ) => {
            frame.text("no-file-preparation");
        }
        (
            Prepared::Locale { .. }
            | Prepared::Icu4xLocale { .. }
            | Prepared::ArtifactCollection(_)
            | Prepared::Construction(_)
            | Prepared::ConstructionIdentity(_),
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
    }
    Ok(frame.finish())
}

/// One file entry's complete invocation: the recipe's configuration file
/// under the App selector, the bounds it is read and analyzed under and the
/// schema, every member in submission order and the limits the collection
/// was read under, every bound of the capability, the Verification Input's
/// pins, every specification asserted and the active one supported with the
/// fixture provider's pins, the vocabulary, and the stage's `facts`.
fn evidence_input(
    frame: &mut Frame,
    candidate: &Candidate,
    prepared: &crate::benchmark::evidence::PreparedEvidence,
    facts: &crate::benchmark::evidence::InputFacts,
) -> Result<(), ContextFailure> {
    use crate::locale::fixtures::fixture_pins;
    use crate::locale::VersionedIdentity;

    let Recipe::Evidence(recipe) = candidate.declaration.fixture else {
        return Err(ContextFailure::EvidenceInputMismatch);
    };
    let expected = selector_input(
        candidate.declaration.selector,
        prepared.structural.max_profile_id_bytes,
    );
    // The file under the bounds the capability and the selected policy set,
    // which an edge's bound changes; the capability's bounds are framed below.
    let source = recipe.source(prepared.capability.policy_comparable());
    if candidate.declaration.selector != super::Selector::App
        || !prepared.selector.benchmark_matches_fixture(&expected)
        || *prepared.source != *source
    {
        return Err(ContextFailure::EvidenceInputMismatch);
    }
    frame.bytes(&prepared.source);
    input_limits(frame, prepared.input);
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
    let active = prepared.supported.project_profile();
    frame.text(active.specification.identity);
    frame.text(active.specification.revision);
    frame.text(active.semantic_digest);
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
