// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Fixed owner-fixture input binding. This covers preparation as well as the
//! actual immutable operation input; it is not a common Measurement Case ID or
//! shared-artifact digest. No acquisition, clock, run, or physical ID enters it.

use crate::benchmark::locale_icu4x::PreparedEngine;
use crate::benchmark::locale_resolution::Canonicalizers;
use crate::benchmark::observation::{document, Digest, Frame};
use crate::benchmark::operation::Prepared;
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
            | Prepared::ArtifactCollection(_),
            None,
        ) => {
            frame.text("no-file-preparation");
        }
        (
            Prepared::Locale { .. }
            | Prepared::Icu4xLocale { .. }
            | Prepared::ArtifactCollection(_),
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
            for bound in [
                read.members,
                read.member_bytes,
                read.total_bytes,
                read.depth,
            ] {
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
            for bound in [
                read.members,
                read.member_bytes,
                read.total_bytes,
                read.depth,
            ] {
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
    }
    Ok(frame.finish())
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
