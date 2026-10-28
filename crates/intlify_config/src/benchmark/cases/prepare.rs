// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Untimed preparation for declared finite fixtures. A candidate is not admitted
//! expected-output authority: the checked oracle registry must still compare its
//! complete output/work observations before any measurement is allowed.

use std::sync::Arc;

use crate::artifacts::collection::{read_collection, CollectionLimits};
use crate::input_limits::{Bound, InputLimits, RawInputLimits, ValueLimits};
use crate::locale::fixtures::{fixture_pins, FixtureProvider};
use crate::locale::Canonicalizer;
use crate::materialize::materialize_file;
use crate::resolver::limits::{ConfirmedPolicy, ImplementationCapability};
use crate::structural::selection::{InvalidSelectorType, Selection, SelectorInput};
use crate::structural::StructuralLimits;

use super::super::locale_icu4x::PreparedEngine;
use super::super::observation::Observation;
use super::super::operation::{Operation, Output, Prepared, Schema};
use super::super::work::LogicalWork;
use super::policy_inputs::capacity;
use super::{
    declarations, Declaration, Engine, ExpectedKind, Icu4xRecipe, LimitEdge, LimitKind,
    LocaleResolutionRecipe, Recipe, ResourcePolicyRecipe, Selector,
};

pub(in crate::benchmark) struct Candidate {
    pub(in crate::benchmark) declaration: Declaration,
    pub(super) input_limits: Option<InputLimits>,
    pub(in crate::benchmark) prepared: Prepared,
    pub(in crate::benchmark) output: Output,
    pub(in crate::benchmark) observation: Observation,
    pub(in crate::benchmark) logical_work: LogicalWork,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(in crate::benchmark) enum PreparationFailure {
    UndeclaredCase,
    PrerequisiteUnavailable,
    UnrepresentableCounterEdge,
    CoreInvariant,
    UnexpectedOutputKind,
    Observation,
}

/// The numbers are explicit owner fixture capacity, not resolver defaults or a
/// substitute for an admitted Implementation Capability / Resource Limit Policy.
pub(super) fn fixture_capacity() -> (InputLimits, StructuralLimits) {
    let bound = |value| Bound::new(value).expect("finite positive fixture capacity");
    (
        InputLimits {
            raw: RawInputLimits {
                max_file_bytes: bound(1_000_000),
                max_parser_tokens: bound(100_000),
            },
            value: ValueLimits {
                max_nodes: bound(100_000),
                max_depth: bound(50_000),
                max_collection_entries: bound(100_000),
                max_total_string_bytes: bound(1_000_000),
                max_single_string_bytes: bound(100_000),
            },
        },
        StructuralLimits {
            max_profiles: bound(64),
            max_profile_id_bytes: bound(256),
            max_structural_analysis_units: bound(1_000_000),
        },
    )
}

pub(in crate::benchmark) fn prepare(
    declaration: &Declaration,
) -> Result<Candidate, PreparationFailure> {
    if !declarations().contains(declaration) {
        return Err(PreparationFailure::UndeclaredCase);
    }
    if declaration.operation == Operation::LocaleCanonicalization {
        if let Recipe::Icu4xLocale { input, engine } = declaration.fixture {
            return prepare_icu4x(declaration, input, engine);
        }
        return prepare_locale(declaration);
    }
    if declaration.operation == Operation::LocaleResolution {
        return prepare_resolution(declaration);
    }
    if declaration.operation == Operation::ResourcePolicyBootstrap {
        return prepare_policy(declaration);
    }
    if declaration.operation == Operation::ArtifactCollectionAdmission {
        return prepare_collection(declaration);
    }
    let source = declaration.fixture.source();
    let (mut input_limits, mut structural_limits) = fixture_capacity();
    if let Some((kind, edge)) = declaration.limit {
        let baseline = materialize_file(Arc::clone(&source), input_limits)
            .map_err(|_| PreparationFailure::PrerequisiteUnavailable)?;
        let counts = baseline.counts();
        let actual = match kind {
            LimitKind::FileBytes => counts.file_bytes,
            LimitKind::ParserTokens => counts.parser_tokens,
            LimitKind::Nodes => counts.value.nodes,
            LimitKind::Depth => counts.value.depth,
            LimitKind::CollectionEntries => counts.value.collection_entries,
            LimitKind::TotalStringBytes => counts.value.total_string_bytes,
            LimitKind::SingleStringBytes => counts.value.single_string_bytes,
            LimitKind::Profiles | LimitKind::ProfileIdBytes | LimitKind::StructuralUnits => {
                let analysis = Schema::for_model()
                    .map_err(|_| PreparationFailure::CoreInvariant)?
                    .analyze(Arc::new(baseline), structural_limits)
                    .map_err(|_| PreparationFailure::CoreInvariant)?;
                if !analysis.is_complete() {
                    return Err(PreparationFailure::PrerequisiteUnavailable);
                }
                match kind {
                    LimitKind::Profiles => analysis.benchmark_profile_counts().map(|v| v.0),
                    LimitKind::ProfileIdBytes => analysis.benchmark_profile_counts().map(|v| v.1),
                    _ => analysis.structural_units(),
                }
                .ok_or(PreparationFailure::PrerequisiteUnavailable)?
            }
            LimitKind::LocaleRawIdentifierBytes
            | LimitKind::LocaleCanonicalIdentifierBytes
            | LimitKind::ResolutionLocaleOccurrences
            | LimitKind::ResolutionRequestedLocales
            | LimitKind::ResolutionRawIdentifierBytes
            | LimitKind::ResolutionCanonicalIdentifierBytes
            | LimitKind::BootstrapArtifacts
            | LimitKind::RecheckArtifacts
            | LimitKind::RecheckProfileIdBytes => {
                return Err(PreparationFailure::UndeclaredCase);
            }
        };
        let limit = match edge {
            LimitEdge::Exact => Some(actual),
            LimitEdge::FirstOver => actual.checked_sub(1),
        }
        .and_then(Bound::new)
        .ok_or(PreparationFailure::UnrepresentableCounterEdge)?;
        match kind {
            LimitKind::FileBytes => input_limits.raw.max_file_bytes = limit,
            LimitKind::ParserTokens => input_limits.raw.max_parser_tokens = limit,
            LimitKind::Nodes => input_limits.value.max_nodes = limit,
            LimitKind::Depth => input_limits.value.max_depth = limit,
            LimitKind::CollectionEntries => input_limits.value.max_collection_entries = limit,
            LimitKind::TotalStringBytes => input_limits.value.max_total_string_bytes = limit,
            LimitKind::SingleStringBytes => input_limits.value.max_single_string_bytes = limit,
            LimitKind::Profiles => structural_limits.max_profiles = limit,
            LimitKind::ProfileIdBytes => structural_limits.max_profile_id_bytes = limit,
            LimitKind::StructuralUnits => structural_limits.max_structural_analysis_units = limit,
            LimitKind::LocaleRawIdentifierBytes
            | LimitKind::LocaleCanonicalIdentifierBytes
            | LimitKind::ResolutionLocaleOccurrences
            | LimitKind::ResolutionRequestedLocales
            | LimitKind::ResolutionRawIdentifierBytes
            | LimitKind::ResolutionCanonicalIdentifierBytes
            | LimitKind::BootstrapArtifacts
            | LimitKind::RecheckArtifacts
            | LimitKind::RecheckProfileIdBytes => unreachable!(),
        }
    }
    let prepared = if declaration.operation == Operation::FileMaterialization {
        Prepared::Entry {
            source,
            limits: input_limits,
        }
    } else {
        let doc = Arc::new(
            materialize_file(source, input_limits)
                .map_err(|_| PreparationFailure::PrerequisiteUnavailable)?,
        );
        let schema = Schema::for_model().map_err(|_| PreparationFailure::CoreInvariant)?;
        match declaration.operation {
            Operation::StructuralAnalysis => Prepared::Structural {
                schema,
                doc,
                limits: structural_limits,
            },
            Operation::AuthoringConstruction => {
                let analysis = schema
                    .analyze(doc, structural_limits)
                    .map_err(|_| PreparationFailure::CoreInvariant)?;
                if !analysis.is_complete() {
                    return Err(PreparationFailure::PrerequisiteUnavailable);
                }
                Prepared::Authoring(analysis)
            }
            Operation::ProfileSelection => {
                let analysis = schema
                    .analyze(doc, structural_limits)
                    .map_err(|_| PreparationFailure::CoreInvariant)?;
                let bound = structural_limits.max_profile_id_bytes;
                let selector = selector_input(declaration.selector, bound);
                Prepared::Select { analysis, selector }
            }
            Operation::FileMaterialization
            | Operation::LocaleCanonicalization
            | Operation::LocaleResolution
            | Operation::ResourcePolicyBootstrap
            | Operation::ArtifactCollectionAdmission => unreachable!(),
        }
    };
    // Only dispatch is repeated here; both reference preparation and timed calls
    // invoke the same ordinary core functions, never a benchmark-only algorithm.
    let output = match &prepared {
        Prepared::Entry { source, limits } => {
            Output::Entry(materialize_file(Arc::clone(source), *limits))
        }
        Prepared::Structural {
            schema,
            doc,
            limits,
        } => Output::Structural(schema.analyze(Arc::clone(doc), *limits)),
        Prepared::Authoring(analysis) => Output::Authoring(analysis.construct()),
        Prepared::Select { analysis, selector } => Output::Select(analysis.select(selector)),
        Prepared::Locale { .. } | Prepared::Icu4xLocale { .. } => {
            unreachable!("locale preparation has no file stages")
        }
        Prepared::LocaleResolution(_)
        | Prepared::ResourcePolicy(_)
        | Prepared::ArtifactCollection(_) => {
            unreachable!("Phases 7, 4, and 9 have their own preparation")
        }
    };
    let kind = match &output {
        Output::Entry(Ok(_)) => ExpectedKind::Materialized,
        Output::Entry(Err(_)) => ExpectedKind::EntryFailure,
        Output::Structural(Ok(analysis)) => {
            if analysis.is_complete() {
                ExpectedKind::StructuralComplete
            } else {
                ExpectedKind::StructuralBlocked
            }
        }
        Output::Authoring(Ok(Some(_))) => ExpectedKind::AuthoringComplete,
        Output::Select(Ok(Selection::Selected(_))) => ExpectedKind::Selected,
        Output::Select(Ok(Selection::Rejected(_))) => ExpectedKind::SelectionRejected,
        Output::Select(Ok(Selection::Unavailable(_))) => ExpectedKind::SelectionUnavailable,
        _ => return Err(PreparationFailure::CoreInvariant),
    };
    if kind != declaration.expected_kind {
        return Err(PreparationFailure::UnexpectedOutputKind);
    }
    let observation = output
        .observe()
        .map_err(|_| PreparationFailure::Observation)?;
    let logical_work =
        LogicalWork::observe(&prepared, &output).map_err(|_| PreparationFailure::Observation)?;
    Ok(Candidate {
        declaration: declaration.clone(),
        input_limits: Some(input_limits),
        prepared,
        output,
        observation,
        logical_work,
    })
}

fn prepare_locale(declaration: &Declaration) -> Result<Candidate, PreparationFailure> {
    let Recipe::Locale(recipe) = declaration.fixture else {
        return Err(PreparationFailure::UndeclaredCase);
    };
    let limit = if let Some((kind, edge)) = declaration.limit {
        let count = match kind {
            LimitKind::LocaleRawIdentifierBytes => recipe.spelling().len(),
            LimitKind::LocaleCanonicalIdentifierBytes => recipe
                .canonical()
                .ok_or(PreparationFailure::PrerequisiteUnavailable)?
                .len(),
            _ => return Err(PreparationFailure::UndeclaredCase),
        };
        let count =
            u64::try_from(count).map_err(|_| PreparationFailure::UnrepresentableCounterEdge)?;
        let count = match edge {
            LimitEdge::Exact => Some(count),
            LimitEdge::FirstOver => count.checked_sub(1),
        };
        count
            .and_then(Bound::new)
            .ok_or(PreparationFailure::UnrepresentableCounterEdge)?
    } else {
        Bound::new(128).expect("explicit finite locale fixture capacity")
    };
    let core = Arc::new(
        Canonicalizer::bind(&fixture_pins(), Some(FixtureProvider::new()), limit)
            .map_err(|_| PreparationFailure::PrerequisiteUnavailable)?,
    );
    let input: Arc<str> = Arc::from(recipe.spelling());
    let output = Output::Locale(core.canonicalize(&input));
    let kind = match &output {
        Output::Locale(Ok(_)) => ExpectedKind::LocaleCanonicalized,
        Output::Locale(Err(_)) => ExpectedKind::LocaleRejected,
        _ => unreachable!(),
    };
    if declaration.expected_kind != kind {
        return Err(PreparationFailure::UnexpectedOutputKind);
    }
    let prepared = Prepared::Locale { core, input };
    // Unsupported coverage or provider malfunction cannot yield an expected
    // successful capture, even if a caller supplies a matching failure value.
    let observation = output
        .observe()
        .map_err(|_| PreparationFailure::Observation)?;
    let logical_work =
        LogicalWork::observe(&prepared, &output).map_err(|_| PreparationFailure::Observation)?;
    Ok(Candidate {
        declaration: declaration.clone(),
        input_limits: None,
        prepared,
        output,
        observation,
        logical_work,
    })
}

/// Canonicalization through the ICU4X provider. The reference output comes
/// from the engine the case measures: the resident one, or a fresh one.
fn prepare_icu4x(
    declaration: &Declaration,
    recipe: Icu4xRecipe,
    engine: Engine,
) -> Result<Candidate, PreparationFailure> {
    // The finite cases' capacity. No ICU4X case declares a limit edge.
    let limit = Bound::new(128).expect("explicit finite locale fixture capacity");
    let engine = PreparedEngine::prepare(engine, limit)
        .map_err(|_| PreparationFailure::PrerequisiteUnavailable)?;
    let input: Arc<str> = Arc::from(recipe.spelling());
    let output = Output::Locale(
        engine
            .with_engine(|core| core.canonicalize(&input))
            .map_err(|_| PreparationFailure::PrerequisiteUnavailable)?,
    );
    let kind = match &output {
        Output::Locale(Ok(_)) => ExpectedKind::LocaleCanonicalized,
        Output::Locale(Err(_)) => ExpectedKind::LocaleRejected,
        _ => unreachable!(),
    };
    if declaration.expected_kind != kind {
        return Err(PreparationFailure::UnexpectedOutputKind);
    }
    let prepared = Prepared::Icu4xLocale { engine, input };
    let observation = output
        .observe()
        .map_err(|_| PreparationFailure::Observation)?;
    let logical_work =
        LogicalWork::observe(&prepared, &output).map_err(|_| PreparationFailure::Observation)?;
    Ok(Candidate {
        declaration: declaration.clone(),
        input_limits: None,
        prepared,
        output,
        observation,
        logical_work,
    })
}

/// The independently specified count of a bound over its fixed recipe; never
/// derived from the resolver under test.
fn resolution_edge(
    kind: LimitKind,
    recipe: LocaleResolutionRecipe,
) -> Result<u64, PreparationFailure> {
    use LocaleResolutionRecipe as R;

    Ok(match (kind, recipe) {
        // Four requested locales, the default and the source, and the
        // browser target's requested locale.
        (LimitKind::ResolutionLocaleOccurrences, R::Multi) => 7,
        (LimitKind::ResolutionLocaleOccurrences, R::EveryRole) => 21,
        (LimitKind::ResolutionRequestedLocales, R::Multi) => 4,
        // `en`, and `und-u-ca-islamicc`'s canonical `und-u-ca-islamic-civil`.
        (LimitKind::ResolutionRawIdentifierBytes, R::Minimal) => 2,
        (LimitKind::ResolutionCanonicalIdentifierBytes, R::ExpandingAlias) => 22,
        _ => return Err(PreparationFailure::UndeclaredCase),
    })
}

fn prepare_resolution(declaration: &Declaration) -> Result<Candidate, PreparationFailure> {
    use crate::benchmark::locale_resolution::{resolve, Canonicalizers, PreparedLocales};
    use crate::resolver::locales::fixture_policy;

    let (recipe, engine) = match declaration.fixture {
        Recipe::LocaleResolution(recipe) => (recipe, None),
        Recipe::Icu4xLocaleResolution { input, engine } => (input, Some(engine)),
        _ => return Err(PreparationFailure::UndeclaredCase),
    };
    let (input_limits, structural_limits) = fixture_capacity();
    let doc = materialize_file(declaration.fixture.source(), input_limits)
        .map_err(|_| PreparationFailure::PrerequisiteUnavailable)?;
    let analysis = Schema::for_model()
        .map_err(|_| PreparationFailure::CoreInvariant)?
        .analyze(Arc::new(doc), structural_limits)
        .map_err(|_| PreparationFailure::CoreInvariant)?;
    let config = analysis
        .construct()
        .map_err(|_| PreparationFailure::CoreInvariant)?
        .ok_or(PreparationFailure::PrerequisiteUnavailable)?;
    let selection = analysis
        .select(&selector_input(
            declaration.selector,
            structural_limits.max_profile_id_bytes,
        ))
        .map_err(|_| PreparationFailure::CoreInvariant)?;
    let Selection::Selected(selected) = selection else {
        return Err(PreparationFailure::PrerequisiteUnavailable);
    };
    // Explicit finite capacity, not product defaults or an admitted policy.
    // The scales hold 258 spellings and 128 requested locales.
    let bound = |value| Bound::new(value).expect("explicit finite resolution capacity");
    let (mut occurrences, mut requested, mut identifier_bytes) = if recipe.scales() {
        (bound(512), bound(256), bound(128))
    } else {
        (bound(128), bound(64), bound(128))
    };
    if let Some((kind, edge)) = declaration.limit {
        let exact = resolution_edge(kind, recipe)?;
        let bound = Bound::new(match edge {
            LimitEdge::Exact => exact,
            LimitEdge::FirstOver => exact - 1,
        })
        .ok_or(PreparationFailure::UnrepresentableCounterEdge)?;
        match kind {
            LimitKind::ResolutionLocaleOccurrences => occurrences = bound,
            LimitKind::ResolutionRequestedLocales => requested = bound,
            LimitKind::ResolutionRawIdentifierBytes
            | LimitKind::ResolutionCanonicalIdentifierBytes => identifier_bytes = bound,
            _ => unreachable!(),
        }
    }
    // Phase 5 binds the canonicalizer under the policy's identifier bound.
    let policy = fixture_policy(identifier_bytes, occurrences, requested);
    let limit = policy
        .limits()
        .locale_resolution
        .max_locale_identifier_bytes;
    let canonicalizer = match engine {
        None => Canonicalizers::Finite(Arc::new(
            Canonicalizer::bind(&fixture_pins(), Some(FixtureProvider::new()), limit)
                .map_err(|_| PreparationFailure::PrerequisiteUnavailable)?,
        )),
        Some(engine) => Canonicalizers::Icu4x(
            PreparedEngine::prepare(engine, limit)
                .map_err(|_| PreparationFailure::PrerequisiteUnavailable)?,
        ),
    };
    let prepared = PreparedLocales {
        analysis,
        config,
        selected: selected.id().clone(),
        policy,
        canonicalizer,
    };
    let fields = prepared
        .fields()
        .ok_or(PreparationFailure::PrerequisiteUnavailable)?;
    // The reference output comes from the engine the case measures.
    let resolved = match &prepared.canonicalizer {
        Canonicalizers::Finite(core) => resolve(&prepared.policy, core.as_ref(), &fields),
        Canonicalizers::Icu4x(engine) => engine
            .with_engine(|core| resolve(&prepared.policy, core, &fields))
            .map_err(|_| PreparationFailure::PrerequisiteUnavailable)?,
    };
    let kind = match &resolved.result {
        Ok(_) if resolved.evaluation.issues().next().is_none() => {
            ExpectedKind::LocaleResolutionComplete
        }
        Ok(_) => ExpectedKind::LocaleResolutionBlocked,
        Err(_) => return Err(PreparationFailure::Observation),
    };
    if declaration.expected_kind != kind {
        return Err(PreparationFailure::UnexpectedOutputKind);
    }
    let output = Output::LocaleResolution(resolved);
    let prepared = Prepared::LocaleResolution(Box::new(prepared));
    let observation = output
        .observe()
        .map_err(|_| PreparationFailure::Observation)?;
    let logical_work =
        LogicalWork::observe(&prepared, &output).map_err(|_| PreparationFailure::Observation)?;
    Ok(Candidate {
        declaration: declaration.clone(),
        input_limits: Some(input_limits),
        prepared,
        output,
        observation,
        logical_work,
    })
}

/// The independently specified count of a bound over its fixed recipe; never
/// derived from the resolver under test.
fn policy_edge(kind: LimitKind, recipe: ResourcePolicyRecipe) -> Result<u64, PreparationFailure> {
    use ResourcePolicyRecipe as R;

    Ok(match (kind, recipe) {
        // The selected policy, a Target Profile, and an Approval Policy; and
        // the bytes of the selector and the only declared Profile ID, `app`.
        (LimitKind::BootstrapArtifacts | LimitKind::RecheckArtifacts, R::Mixed)
        | (LimitKind::RecheckProfileIdBytes, R::PolicyOnly) => 3,
        _ => return Err(PreparationFailure::UndeclaredCase),
    })
}

/// The caller's finite limits on the collection at 015's pre-invocation
/// boundary: explicit owner capacity, not the `artifactAdmission` bounds.
pub(super) const COLLECTION_LIMITS: CollectionLimits = CollectionLimits {
    members: 1_000,
    member_bytes: 1_000_000,
    total_bytes: 10_000_000,
    depth: 64,
};

fn prepare_policy(declaration: &Declaration) -> Result<Candidate, PreparationFailure> {
    use crate::benchmark::resource_policy::{resolve, PreparedPolicy};

    let Recipe::ResourcePolicy(recipe) = declaration.fixture else {
        return Err(PreparationFailure::UndeclaredCase);
    };
    let (input_limits, structural_limits) = fixture_capacity();
    let mut capability = capacity();
    let mut policy = capability.clone();
    if let Some((kind, edge)) = declaration.limit {
        let exact = policy_edge(kind, recipe)?;
        let bound = Bound::new(match edge {
            LimitEdge::Exact => exact,
            LimitEdge::FirstOver => exact - 1,
        })
        .ok_or(PreparationFailure::UnrepresentableCounterEdge)?;
        match kind {
            LimitKind::BootstrapArtifacts => {
                // A compatible policy needs no more than the capability.
                capability.artifact_admission.max_artifacts = bound;
                policy.artifact_admission.max_artifacts = bound;
            }
            LimitKind::RecheckArtifacts => policy.artifact_admission.max_artifacts = bound,
            LimitKind::RecheckProfileIdBytes => {
                policy.configuration_input.structural.max_profile_id_bytes = bound;
            }
            _ => unreachable!(),
        }
    }
    let fixture = recipe.fixture(&policy);
    let source =
        serde_json::to_vec(&fixture.config).map_err(|_| PreparationFailure::CoreInvariant)?;
    let doc = materialize_file(Arc::from(source), input_limits)
        .map_err(|_| PreparationFailure::PrerequisiteUnavailable)?;
    let analysis = Schema::for_model()
        .map_err(|_| PreparationFailure::CoreInvariant)?
        .analyze(Arc::new(doc), structural_limits)
        .map_err(|_| PreparationFailure::CoreInvariant)?;
    let config = analysis
        .construct()
        .map_err(|_| PreparationFailure::CoreInvariant)?
        .ok_or(PreparationFailure::PrerequisiteUnavailable)?;
    // The selector is observed under the bound the structural stage ran
    // under, the capability's `maxProfileIdBytes`.
    let selector = selector_input(declaration.selector, structural_limits.max_profile_id_bytes);
    let selection = analysis
        .select(&selector)
        .map_err(|_| PreparationFailure::CoreInvariant)?;
    let Selection::Selected(selected) = selection else {
        return Err(PreparationFailure::PrerequisiteUnavailable);
    };
    let selected = selected.id().clone();
    let members = fixture
        .members
        .iter()
        .map(serde_json::to_vec)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| PreparationFailure::CoreInvariant)?;
    let bytes = members.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let collection = read_collection(Some(&bytes), &COLLECTION_LIMITS)
        .map_err(|_| PreparationFailure::PrerequisiteUnavailable)?;
    let profile_ids = config
        .profiles()
        .iter()
        .map(|(id, _)| u64::try_from(id.as_str().len()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| PreparationFailure::CoreInvariant)?;
    let prepared = PreparedPolicy {
        analysis,
        config,
        selected,
        selector,
        capability: ImplementationCapability::fixture_owned(capability),
        collection,
        profile_ids,
    };
    let inputs = prepared
        .inputs()
        .ok_or(PreparationFailure::PrerequisiteUnavailable)?;
    let resolved = resolve(&prepared.capability, &prepared.collection, &inputs);
    let kind = match &resolved.result {
        Ok(outcome) if outcome.confirmed.is_ok() => ExpectedKind::ResourcePolicyConfirmed,
        Ok(_) => ExpectedKind::ResourcePolicyBlocked,
        Err(_) => return Err(PreparationFailure::Observation),
    };
    if declaration.expected_kind != kind {
        return Err(PreparationFailure::UnexpectedOutputKind);
    }
    let output = Output::ResourcePolicy(resolved);
    let prepared = Prepared::ResourcePolicy(Box::new(prepared));
    let observation = output
        .observe()
        .map_err(|_| PreparationFailure::Observation)?;
    let logical_work =
        LogicalWork::observe(&prepared, &output).map_err(|_| PreparationFailure::Observation)?;
    Ok(Candidate {
        declaration: declaration.clone(),
        input_limits: Some(input_limits),
        prepared,
        output,
        observation,
        logical_work,
    })
}

/// Phase 9 over a recipe's collection, read at 015's pre-invocation boundary,
/// under the policy whose bounds its Resource Limit Policy sets. No
/// configuration is materialized: the collection admission reads none.
fn prepare_collection(declaration: &Declaration) -> Result<Candidate, PreparationFailure> {
    use crate::benchmark::artifact_collection::PreparedCollection;

    let Recipe::ArtifactCollection(recipe) = declaration.fixture else {
        return Err(PreparationFailure::UndeclaredCase);
    };
    let members = recipe
        .members()
        .iter()
        .map(serde_json::to_vec)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| PreparationFailure::CoreInvariant)?;
    let bytes = members.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let collection = read_collection(Some(&bytes), &COLLECTION_LIMITS)
        .map_err(|_| PreparationFailure::PrerequisiteUnavailable)?;
    let prepared = PreparedCollection {
        policy: ConfirmedPolicy::fixture_owned(capacity()),
        collection,
    };
    let resolved = prepared.resolve();
    let kind = match &resolved.result {
        Ok(Ok(_)) => ExpectedKind::ArtifactCollectionAdmitted,
        Ok(Err(_)) => ExpectedKind::ArtifactCollectionBlocked,
        Err(_) => return Err(PreparationFailure::Observation),
    };
    if declaration.expected_kind != kind {
        return Err(PreparationFailure::UnexpectedOutputKind);
    }
    let output = Output::ArtifactCollection(resolved);
    let prepared = Prepared::ArtifactCollection(Box::new(prepared));
    let observation = output
        .observe()
        .map_err(|_| PreparationFailure::Observation)?;
    let logical_work =
        LogicalWork::observe(&prepared, &output).map_err(|_| PreparationFailure::Observation)?;
    Ok(Candidate {
        declaration: declaration.clone(),
        input_limits: None,
        prepared,
        output,
        observation,
        logical_work,
    })
}

/// Only finite, non-secret owner fixture literals enter this constructor.
pub(super) fn selector_input(selector: Selector, bound: Bound) -> SelectorInput {
    match selector {
        Selector::Absent => SelectorInput::absent(bound),
        Selector::App => SelectorInput::string("app", bound),
        Selector::Unknown => SelectorInput::string("unknown", bound),
        Selector::InvalidSyntax => SelectorInput::string("APP", bound),
        Selector::InvalidType => SelectorInput::invalid_type(InvalidSelectorType::Object, bound),
        Selector::ExactByteLimit => SelectorInput::string(&"z".repeat(256), bound),
        Selector::FirstOverByteLimit => SelectorInput::string(&"z".repeat(257), bound),
    }
}
