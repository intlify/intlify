// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Untimed preparation for declared finite fixtures. A candidate is not admitted
//! expected-output authority: the checked oracle registry must still compare its
//! complete output/work observations before any measurement is allowed.

use std::sync::Arc;

use serde_json::Value;

use crate::artifacts::collection::{read_collection, CollectionLimits, FormalCollection};
use crate::input_limits::{Bound, InputLimits, RawInputLimits, ValueLimits};
use crate::locale::fixtures::{fixture_pins, FixtureProvider};
use crate::locale::Canonicalizer;
use crate::materialize::materialize_file;
use crate::model::ProfileId;
use crate::resolver::limits::{ConfirmedPolicy, ImplementationCapability};
use crate::structural::selection::{InvalidSelectorType, Selection, SelectorInput};
use crate::structural::StructuralLimits;

use super::super::locale_icu4x::PreparedEngine;
use super::super::observation::Observation;
use super::super::operation::{Analysis, Config, Operation, Output, Prepared, Schema};
use super::super::work::LogicalWork;
use super::policy_inputs::capacity;
use super::{
    declarations, Declaration, Engine, ExpectedKind, HydrationRecipe, Icu4xRecipe, LimitEdge,
    LimitKind, LocalePolicyResolutionRecipe, Recipe, ResourcePolicyRecipe, Selector,
    TargetAndGroupRecipe,
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
    if declaration.operation == Operation::LocalePolicyResolution {
        return prepare_resolution(declaration);
    }
    if declaration.operation == Operation::ResourcePolicyBootstrap {
        return prepare_policy(declaration);
    }
    if declaration.operation == Operation::ArtifactCollectionAdmission {
        return prepare_collection(declaration);
    }
    if declaration.operation == Operation::ResolvedBindingConstruction {
        return prepare_bindings(declaration);
    }
    if declaration.operation == Operation::TargetAndGroupResolution {
        return prepare_targets(declaration);
    }
    if declaration.operation == Operation::HydrationRelationValidation {
        return prepare_hydration(declaration);
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
            | LimitKind::ResolutionNegotiationAliases
            | LimitKind::ResolutionFallbackSources
            | LimitKind::ResolutionFallbackCandidatesPerSource
            | LimitKind::ResolutionCoverageRules
            | LimitKind::ResolutionCoverageLocaleSelectorOccurrences
            | LimitKind::ResolutionCoverageSurfaceSelectorOccurrences
            | LimitKind::ResolutionCoverageDecisionCells
            | LimitKind::ResolutionCoverageRuleCellComparisons
            | LimitKind::BootstrapArtifacts
            | LimitKind::RecheckArtifacts
            | LimitKind::RecheckProfileIdBytes
            | LimitKind::TargetProfiles
            | LimitKind::TargetIdBytes
            | LimitKind::DeploymentGroups
            | LimitKind::GroupIdBytes
            | LimitKind::MembersPerGroup
            | LimitKind::MembershipOccurrences
            | LimitKind::HydrationRelations
            | LimitKind::HydrationEndpointBytes => {
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
            | LimitKind::ResolutionNegotiationAliases
            | LimitKind::ResolutionFallbackSources
            | LimitKind::ResolutionFallbackCandidatesPerSource
            | LimitKind::ResolutionCoverageRules
            | LimitKind::ResolutionCoverageLocaleSelectorOccurrences
            | LimitKind::ResolutionCoverageSurfaceSelectorOccurrences
            | LimitKind::ResolutionCoverageDecisionCells
            | LimitKind::ResolutionCoverageRuleCellComparisons
            | LimitKind::BootstrapArtifacts
            | LimitKind::RecheckArtifacts
            | LimitKind::RecheckProfileIdBytes
            | LimitKind::TargetProfiles
            | LimitKind::TargetIdBytes
            | LimitKind::DeploymentGroups
            | LimitKind::GroupIdBytes
            | LimitKind::MembersPerGroup
            | LimitKind::MembershipOccurrences
            | LimitKind::HydrationRelations
            | LimitKind::HydrationEndpointBytes => unreachable!(),
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
            | Operation::LocalePolicyResolution
            | Operation::ResourcePolicyBootstrap
            | Operation::ArtifactCollectionAdmission
            | Operation::ResolvedBindingConstruction
            | Operation::TargetAndGroupResolution
            | Operation::HydrationRelationValidation => unreachable!(),
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
        Prepared::LocalePolicyResolution(_)
        | Prepared::ResourcePolicy(_)
        | Prepared::ArtifactCollection(_)
        | Prepared::ResolvedBinding(_)
        | Prepared::TargetAndGroup(_)
        | Prepared::Hydration(_) => {
            unreachable!("Phases 7, 4, 9, 11, 12, and 13 have their own preparation")
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
    recipe: LocalePolicyResolutionRecipe,
) -> Result<u64, PreparationFailure> {
    use LocalePolicyResolutionRecipe as R;

    if recipe == R::EveryRole {
        if let Some(count) = every_role_policy_count(kind) {
            return Ok(count);
        }
    }
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

/// Phase 8's counts over every role: two aliases, two fallback sources whose
/// longest sequence is `ja`'s two candidates, and two surface items; three
/// rules with three locale items; and three requested locales by three
/// vocabulary members, each cell compared with the three rules.
const fn every_role_policy_count(kind: LimitKind) -> Option<u64> {
    Some(match kind {
        LimitKind::ResolutionNegotiationAliases
        | LimitKind::ResolutionFallbackSources
        | LimitKind::ResolutionFallbackCandidatesPerSource
        | LimitKind::ResolutionCoverageSurfaceSelectorOccurrences => 2,
        LimitKind::ResolutionCoverageRules
        | LimitKind::ResolutionCoverageLocaleSelectorOccurrences => 3,
        LimitKind::ResolutionCoverageDecisionCells => 9,
        LimitKind::ResolutionCoverageRuleCellComparisons => 27,
        _ => return None,
    })
}

/// Phase 8's eight bounds at distinct capacities that every case passing
/// Phase 7's requested bound and the rule bound fits: a decision table of 256
/// requested locales by the vocabulary's three members, compared with 64
/// rules.
fn policy_capacity(limits: &mut crate::resolver::limits::LocaleResolutionLimits) {
    let bound = |value| Bound::new(value).expect("explicit finite policy capacity");
    limits.max_negotiation_aliases = bound(32);
    limits.max_fallback_sources = bound(48);
    limits.max_fallback_candidates_per_source = bound(56);
    limits.max_coverage_rules = bound(64);
    limits.max_coverage_locale_selector_occurrences = bound(96);
    limits.max_coverage_surface_selector_occurrences = bound(112);
    limits.max_coverage_decision_cells = bound(1024);
    limits.max_coverage_rule_cell_comparisons = bound(65_536);
}

/// The Phase 8 bound a case's edge replaces.
fn policy_bound(
    limits: &mut crate::resolver::limits::LocaleResolutionLimits,
    kind: LimitKind,
    bound: Bound,
) {
    match kind {
        LimitKind::ResolutionNegotiationAliases => limits.max_negotiation_aliases = bound,
        LimitKind::ResolutionFallbackSources => limits.max_fallback_sources = bound,
        LimitKind::ResolutionFallbackCandidatesPerSource => {
            limits.max_fallback_candidates_per_source = bound;
        }
        LimitKind::ResolutionCoverageRules => limits.max_coverage_rules = bound,
        LimitKind::ResolutionCoverageLocaleSelectorOccurrences => {
            limits.max_coverage_locale_selector_occurrences = bound;
        }
        LimitKind::ResolutionCoverageSurfaceSelectorOccurrences => {
            limits.max_coverage_surface_selector_occurrences = bound;
        }
        LimitKind::ResolutionCoverageDecisionCells => limits.max_coverage_decision_cells = bound,
        LimitKind::ResolutionCoverageRuleCellComparisons => {
            limits.max_coverage_rule_cell_comparisons = bound;
        }
        _ => unreachable!("only a Phase 8 bound reaches here"),
    }
}

fn prepare_resolution(declaration: &Declaration) -> Result<Candidate, PreparationFailure> {
    use crate::benchmark::locale_resolution::{
        resolve, Canonicalizers, PolicyInputs, PreparedLocales,
    };
    use crate::resolver::limits::ConfirmedPolicy;
    use crate::resolver::locales::fixture_policy;

    let (recipe, engine) = match declaration.fixture {
        Recipe::LocalePolicyResolution(recipe) => (recipe, None),
        Recipe::Icu4xLocalePolicyResolution { input, engine } => (input, Some(engine)),
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
    let mut policy_edge = None;
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
            _ => policy_edge = Some((kind, bound)),
        }
    }
    // Phase 7's three bounds as the case gives them, and Phase 8's eight at
    // a capacity no recipe reaches. Phase 5 binds the canonicalizer under the
    // policy's identifier bound.
    let mut limits = fixture_policy(identifier_bytes, occurrences, requested)
        .limits()
        .clone();
    policy_capacity(&mut limits.locale_resolution);
    if let Some((kind, bound)) = policy_edge {
        policy_bound(&mut limits.locale_resolution, kind, bound);
    }
    let policy = ConfirmedPolicy::fixture_owned(limits);
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
        inputs: PolicyInputs::fixture_owned(),
        canonicalizer,
    };
    let fields = prepared
        .fields()
        .ok_or(PreparationFailure::PrerequisiteUnavailable)?;
    // The reference output comes from the engine the case measures.
    let resolved = match &prepared.canonicalizer {
        Canonicalizers::Finite(core) => {
            resolve(&prepared.policy, &prepared.inputs, core.as_ref(), &fields)
        }
        Canonicalizers::Icu4x(engine) => engine
            .with_engine(|core| resolve(&prepared.policy, &prepared.inputs, core, &fields))
            .map_err(|_| PreparationFailure::PrerequisiteUnavailable)?,
    };
    let kind = match &resolved.result {
        Ok(_) if resolved.evaluation.issues().next().is_none() => {
            ExpectedKind::LocalePolicyResolutionComplete
        }
        Ok(_) => ExpectedKind::LocalePolicyResolutionBlocked,
        Err(_) => return Err(PreparationFailure::Observation),
    };
    if declaration.expected_kind != kind {
        return Err(PreparationFailure::UnexpectedOutputKind);
    }
    let output = Output::LocalePolicyResolution(resolved);
    let prepared = Prepared::LocalePolicyResolution(Box::new(prepared));
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
    let (input_limits, _) = fixture_capacity();
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
    let SelectedConfig {
        analysis,
        config,
        selector,
        selected,
    } = select_config(&fixture.config, declaration.selector)?;
    let collection = submitted(&fixture.members)?;
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
    let collection = submitted(&recipe.members())?;
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

/// Phase 9's exact resolution over a recipe's configuration, selected with
/// `app`, and its collection, read at 015's pre-invocation boundary and
/// admitted into the Set under the policy whose bounds its Resource Limit
/// Policy sets.
fn prepare_bindings(declaration: &Declaration) -> Result<Candidate, PreparationFailure> {
    use crate::benchmark::resolved_binding::PreparedBindings;

    let Recipe::ResolvedBinding(recipe) = declaration.fixture else {
        return Err(PreparationFailure::UndeclaredCase);
    };
    let fixture = recipe.fixture();
    let SelectedConfig {
        analysis,
        config,
        selected,
        ..
    } = select_config(&fixture.config, declaration.selector)?;
    let prepared = PreparedBindings {
        analysis,
        config,
        selected,
        policy: ConfirmedPolicy::fixture_owned(capacity()),
        collection: submitted(&fixture.members)?,
    };
    let resolved = prepared
        .resolve()
        .ok_or(PreparationFailure::PrerequisiteUnavailable)?;
    let kind = match &resolved.result {
        Ok(Ok(_)) => ExpectedKind::ResolvedBindingConstructed,
        Ok(Err(_)) => ExpectedKind::ResolvedBindingBlocked,
        Err(_) => return Err(PreparationFailure::Observation),
    };
    if declaration.expected_kind != kind {
        return Err(PreparationFailure::UnexpectedOutputKind);
    }
    let output = Output::ResolvedBinding(resolved);
    let prepared = Prepared::ResolvedBinding(Box::new(prepared));
    let observation = output
        .observe()
        .map_err(|_| PreparationFailure::Observation)?;
    let logical_work =
        LogicalWork::observe(&prepared, &output).map_err(|_| PreparationFailure::Observation)?;
    Ok(Candidate {
        declaration: declaration.clone(),
        input_limits: Some(fixture_capacity().0),
        prepared,
        output,
        observation,
        logical_work,
    })
}

/// The independently specified count of a bound over its fixed recipe; never
/// derived from the resolver under test.
fn target_group_edge(
    kind: LimitKind,
    recipe: TargetAndGroupRecipe,
) -> Result<u64, PreparationFailure> {
    use TargetAndGroupRecipe as R;

    Ok(match (kind, recipe) {
        // `browser` and `ssr`, both members of `web`, the one group.
        (
            LimitKind::TargetProfiles
            | LimitKind::MembersPerGroup
            | LimitKind::MembershipOccurrences,
            R::Storefront,
        ) => 2,
        // `browser`, the longer Target ID, and `web`.
        (LimitKind::TargetIdBytes, R::Storefront) => 7,
        (LimitKind::GroupIdBytes, R::Storefront) => 3,
        // One group for each of the scale's 64 targets.
        (LimitKind::DeploymentGroups, R::ManyGroups) => 64,
        _ => return Err(PreparationFailure::UndeclaredCase),
    })
}

/// Phases 11 and 12 over a recipe's configuration, selected with `app`, under
/// the policy whose bound a case's edge replaces. Phase 7 canonicalizes
/// through the finite provider bound under the policy's identifier bound, as
/// Phase 5 binds it, and the collection, read at 015's pre-invocation
/// boundary, is admitted into the Set that Phase 9 resolves against, all
/// before the interval.
fn prepare_targets(declaration: &Declaration) -> Result<Candidate, PreparationFailure> {
    use crate::benchmark::target_group::PreparedTargets;

    let Recipe::TargetAndGroup(recipe) = declaration.fixture else {
        return Err(PreparationFailure::UndeclaredCase);
    };
    let mut limits = capacity();
    if let Some((kind, edge)) = declaration.limit {
        let exact = target_group_edge(kind, recipe)?;
        let bound = Bound::new(match edge {
            LimitEdge::Exact => exact,
            LimitEdge::FirstOver => exact - 1,
        })
        .ok_or(PreparationFailure::UnrepresentableCounterEdge)?;
        let grouping = &mut limits.target_grouping;
        match kind {
            LimitKind::TargetProfiles => grouping.max_target_profiles = bound,
            LimitKind::TargetIdBytes => grouping.max_target_id_bytes = bound,
            LimitKind::DeploymentGroups => grouping.max_deployment_groups = bound,
            LimitKind::GroupIdBytes => grouping.max_group_id_bytes = bound,
            LimitKind::MembersPerGroup => grouping.max_members_per_group = bound,
            LimitKind::MembershipOccurrences => grouping.max_membership_occurrences = bound,
            _ => unreachable!("only a Phase 11 or 12 bound reaches here"),
        }
    }
    let policy = ConfirmedPolicy::fixture_owned(limits);
    let canonicalizer = finite_canonicalizer(&policy)?;
    let fixture = recipe.fixture();
    let SelectedConfig {
        analysis,
        config,
        selected,
        ..
    } = select_config(&fixture.config, declaration.selector)?;
    let prepared = PreparedTargets {
        analysis,
        config,
        selected,
        policy,
        canonicalizer,
        collection: submitted(&fixture.members)?,
    };
    let resolved = prepared
        .resolve()
        .ok_or(PreparationFailure::PrerequisiteUnavailable)?;
    let kind = match &resolved.result {
        Ok(Ok(retained))
            if retained.partition.is_ok()
                && retained
                    .targets
                    .iter()
                    .all(|target| target.resolved.is_ok()) =>
        {
            ExpectedKind::TargetAndGroupResolutionComplete
        }
        Ok(Ok(_)) => ExpectedKind::TargetAndGroupResolutionBlocked,
        Ok(Err(_)) | Err(_) => return Err(PreparationFailure::Observation),
    };
    if declaration.expected_kind != kind {
        return Err(PreparationFailure::UnexpectedOutputKind);
    }
    let output = Output::TargetAndGroup(resolved);
    let prepared = Prepared::TargetAndGroup(Box::new(prepared));
    let observation = output
        .observe()
        .map_err(|_| PreparationFailure::Observation)?;
    let logical_work =
        LogicalWork::observe(&prepared, &output).map_err(|_| PreparationFailure::Observation)?;
    Ok(Candidate {
        declaration: declaration.clone(),
        input_limits: Some(fixture_capacity().0),
        prepared,
        output,
        observation,
        logical_work,
    })
}

/// The finite provider Phase 7 canonicalizes through, bound under the
/// policy's identifier bound, as Phase 5 binds it.
fn finite_canonicalizer(
    policy: &ConfirmedPolicy,
) -> Result<Arc<crate::benchmark::locale::Core>, PreparationFailure> {
    Canonicalizer::bind(
        &fixture_pins(),
        Some(FixtureProvider::new()),
        policy
            .limits()
            .locale_resolution
            .max_locale_identifier_bytes,
    )
    .map(Arc::new)
    .map_err(|_| PreparationFailure::PrerequisiteUnavailable)
}

/// The independently specified count of a bound over its fixed recipe; never
/// derived from the resolver under test.
fn hydration_edge(kind: LimitKind, recipe: HydrationRecipe) -> Result<u64, PreparationFailure> {
    use HydrationRecipe as R;

    Ok(match (kind, recipe) {
        // `ssr -> browser`, `ssr -> app`, and `edge -> browser`.
        (LimitKind::HydrationRelations, R::FanOutFanIn) => 3,
        // `browser`, the longer endpoint of `ssr -> browser`.
        (LimitKind::HydrationEndpointBytes, R::Storefront) => 7,
        _ => return Err(PreparationFailure::UndeclaredCase),
    })
}

/// Phase 13 over a recipe's configuration, selected with `app`, under the
/// policy whose bound a case's edge replaces, and with the recipe's answers
/// to what 015 asks 024. Every earlier phase runs in preparation, as for
/// Phases 11 and 12, with Phase 8's negotiation besides.
fn prepare_hydration(declaration: &Declaration) -> Result<Candidate, PreparationFailure> {
    use crate::benchmark::hydration_relation::PreparedHydration;

    let Recipe::Hydration(recipe) = declaration.fixture else {
        return Err(PreparationFailure::UndeclaredCase);
    };
    let mut limits = capacity();
    if let Some((kind, edge)) = declaration.limit {
        let exact = hydration_edge(kind, recipe)?;
        let bound = Bound::new(match edge {
            LimitEdge::Exact => exact,
            LimitEdge::FirstOver => exact - 1,
        })
        .ok_or(PreparationFailure::UnrepresentableCounterEdge)?;
        let grouping = &mut limits.target_grouping;
        match kind {
            LimitKind::HydrationRelations => grouping.max_hydration_relations = bound,
            // An endpoint's bound is the Target ID bound Phase 11 applies too.
            LimitKind::HydrationEndpointBytes => grouping.max_target_id_bytes = bound,
            _ => unreachable!("only a Phase 13 bound reaches here"),
        }
    }
    let policy = ConfirmedPolicy::fixture_owned(limits);
    let canonicalizer = finite_canonicalizer(&policy)?;
    let fixture = recipe.fixture();
    let SelectedConfig {
        analysis,
        config,
        selected,
        ..
    } = select_config(&fixture.config, declaration.selector)?;
    let prepared = PreparedHydration {
        analysis,
        config,
        selected,
        policy,
        canonicalizer,
        collection: submitted(&fixture.members)?,
        capabilities: recipe.capabilities(),
    };
    let resolved = prepared
        .resolve()
        .ok_or(PreparationFailure::PrerequisiteUnavailable)?;
    let kind = match &resolved.result {
        Ok(Ok(retained))
            if retained.negotiation.is_ok()
                && retained.groups.iter().all(|group| group.relations.is_ok()) =>
        {
            ExpectedKind::HydrationRelationValidationComplete
        }
        Ok(Ok(_)) => ExpectedKind::HydrationRelationValidationBlocked,
        Ok(Err(_)) | Err(_) => return Err(PreparationFailure::Observation),
    };
    if declaration.expected_kind != kind {
        return Err(PreparationFailure::UnexpectedOutputKind);
    }
    let output = Output::Hydration(resolved);
    let prepared = Prepared::Hydration(Box::new(prepared));
    let observation = output
        .observe()
        .map_err(|_| PreparationFailure::Observation)?;
    let logical_work =
        LogicalWork::observe(&prepared, &output).map_err(|_| PreparationFailure::Observation)?;
    Ok(Candidate {
        declaration: declaration.clone(),
        input_limits: Some(fixture_capacity().0),
        prepared,
        output,
        observation,
        logical_work,
    })
}

/// A complete configuration and the declaration its selector selects.
struct SelectedConfig {
    analysis: Analysis,
    config: Config,
    selector: SelectorInput,
    selected: ProfileId,
}

/// `config` through Phases 1 to 3 under the fixture capacity: materialized,
/// analyzed into a complete model, and selected with `selector`.
fn select_config(config: &Value, selector: Selector) -> Result<SelectedConfig, PreparationFailure> {
    let (input_limits, structural_limits) = fixture_capacity();
    let source = serde_json::to_vec(config).map_err(|_| PreparationFailure::CoreInvariant)?;
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
    let selector = selector_input(selector, structural_limits.max_profile_id_bytes);
    let selection = analysis
        .select(&selector)
        .map_err(|_| PreparationFailure::CoreInvariant)?;
    let Selection::Selected(selected) = selection else {
        return Err(PreparationFailure::PrerequisiteUnavailable);
    };
    let selected = selected.id().clone();
    Ok(SelectedConfig {
        analysis,
        config,
        selector,
        selected,
    })
}

/// `members`, in submission order, read at 015's pre-invocation boundary.
fn submitted(members: &[Value]) -> Result<FormalCollection, PreparationFailure> {
    let members = members
        .iter()
        .map(serde_json::to_vec)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| PreparationFailure::CoreInvariant)?;
    let bytes = members.iter().map(Vec::as_slice).collect::<Vec<_>>();
    read_collection(Some(&bytes), &COLLECTION_LIMITS)
        .map_err(|_| PreparationFailure::PrerequisiteUnavailable)
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
