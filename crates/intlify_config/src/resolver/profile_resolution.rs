// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Semantic Profile Resolver over one invocation: Phases 4 to 13 of the
//! Deterministic Resolution Algorithm, in 015's order, over one evaluation in
//! which Phase 0 ran before configuration entry, so that its Finding and
//! status remain whatever entry finds (D15:2145), and Phase 1 then admitted
//! the complete input ([`super::entry`]); without its value there is no Shared
//! projection, and these phases do not run (D15:2369). Each phase takes the
//! values the earlier ones admitted, or the causes of their absence, and
//! evaluates every check whose prerequisites were admitted, so a blocking
//! result leaves only the checks that depend on it not evaluated. Once the phases that establish a semantic group have run, the
//! entry records the unit that owns the group's semantic dependency, which
//! waits on what they established (D15:2516). The `profileSpecification`
//! group's owner is a unit of Phase 2, which [`super::structural_admission`]
//! records.
//!
//! The semantic phases start from the declaration Phase 3 provisionally
//! selected once structural admission admitted its boundary and its
//! `resourceLimits` reference, whatever else of it or of the root it rejected
//! (D15:2131, 2367, 3434; plan 025 interpretation #218), and read each of its
//! fields on its own ([`super::declaration_fields`]), from what each
//! candidate left at its value ([`super::value_admissions`]): a value
//! structural admission did not admit, whether a field, a map entry, an array
//! element, or a `policies` slot, holds back only the checks that read it,
//! with its Findings as their causes, and its siblings continue (D15:2367,
//! 3340).
//! Without a selection, each check that reads the declaration, or the Policy
//! its reference selects, is not evaluated with what held the selection back,
//! while Phase 4 still checks the collection under the capability (D15:2131,
//! 2132). Profile projection waits on the whole root (D15:2142). What the entry
//! returns is crate-private semantic state, not a `LocalizationProjectProfile`,
//! a checked or blocked outcome, or Resolution Evidence. The profile
//! projection is 015's Implementation Phase 5.
//!
//! [`invoke_file`] runs one file entry's invocation from Phase 0: Phase 1 over
//! the file, Phases 2 and 3 over the value it admits, and these phases over
//! the configuration Phase 3 selected. [`invoke_programmatic`] runs a
//! programmatic entry's alike, Phase 1 over its Snapshot
//! ([`super::snapshot_entry`]). Without a value either leaves instead the
//! blocking Phase 1 Finding Keys that make the Shared projection unavailable
//! (D15:2369). Either way it leaves what the entry dependency records name,
//! the selector projected under the final projection authority Phase 4 fixed,
//! or the capability's without a value, beside the Profile IDs the root
//! declares (D15:807, 816-817, 2516). With a value, it also copies that
//! value out as the canonical tree of the Materialized Resolution Input Set
//! after Phase 1 and before Phase 2 consumes the document (D15:2657). The
//! construction and invocation inputs it takes are assembled outside it, by
//! [`super::invocation`]. A replay runs the phases after Phase 1 over the
//! retained value ([`resolve_materialized`], [`super::replay`]).

use crate::artifacts::collection::FormalCollection;
use crate::artifacts::vocabulary::FormalVocabulary;
use crate::boundary::capability::ImplementationCapability;
use crate::boundary::verification::VerificationInput;
use crate::input_limits::{Bound, InputLimits, ValueCounts};
use crate::locale::{
    CanonicalizationFailure, FormalData, Provider, SemanticPins, VersionedIdentity,
};
use crate::materialize::MaterializedDocument;
use crate::model::{IntlifyConfig, ProfileDeclaration, ProfileId};
use crate::references::{PolicyReference, TargetProfileReference};
use crate::snapshot::Node;
use crate::structural::selection::{ProvisionalSelection, SelectionInvariant, SelectorInput};
use crate::structural::{
    AdmissionInvariant, AuthoringSchema, StructuralAnalysis, StructuralLimits,
};

use super::artifact_resolution::{admit_collection, ArtifactSet, CollectionInvariant};
use super::coverage::{resolve_coverage, CoveragePolicy};
use super::declaration_fields::DeclarationFields;
use super::declaration_fragments::DeclarationFragments;
use super::declaration_observer::{DeclarationObserver, ObserverInvariant};
use super::delivery::{resolve_delivery, DeliveryPolicy};
use super::dependency::{own, EntryInputs, SemanticGroup, OPTIONAL_POLICIES, REQUIRED_POLICIES};
use super::entry::{admit_entry, ConfigurationEntry, EntryFile, EntryInvariant};
use super::evaluation::{and, Admitted, Causes, Evaluation, EvaluationInvariant};
use super::evidence::EvidenceInvariant;
use super::evidence_component::ComponentInvariant;
use super::exact_resolution::{
    resolve_fields, ArtifactBindings, ReferenceInvariant, ResolvedReferences,
};
use super::fallback::{resolve_fallback, MessageFallbackPolicy};
use super::finalization::FinalizationInvariant;
use super::groups::{resolve_groups, GroupInvariant, ResolvedGroups};
use super::hydration::{validate_hydration, HydrationInvariant, ResolvedHydration};
use super::identity::{resolve_project_identity, ProjectIdentity};
use super::locales::{resolve_locales, LocaleInvariant, ResolvedLocales};
use super::negotiation::{resolve_negotiation, NegotiationProfile};
use super::profile_selection::{select_profile, ProfileSelectionInvariant};
use super::reporting::ReportingInvariant;
use super::resource_policy::{
    admit_resource_policy, ConfigurationMeasures, DeclarationReferences, PolicyInvariant,
    PolicyOutcome, ReferenceOccurrence,
};
use super::snapshot_entry::{
    admit_snapshot_entry, EntrySnapshot, ProgrammaticEntry, SnapshotEntryLimits,
};
use super::specification::{
    admit_project_profile, admit_specifications, SpecificationInput, SpecificationInvariant,
    SpecificationSet, SupportedSpecifications,
};
use super::structural_admission::{
    admit_structure, whole, StructuralAdmission, StructuralInvariant,
};
use super::targets::{resolve_targets, ResolvedTargets, TargetInvariant};
use super::value_admissions::ValueAdmissions;
use super::vocabulary::SurfaceVocabulary;

type Analysis = StructuralAnalysis<PolicyReference, TargetProfileReference>;
type Config = IntlifyConfig<PolicyReference, TargetProfileReference>;
type Declaration = ProfileDeclaration<PolicyReference, TargetProfileReference>;

/// One invocation's configuration past Phases 1 to 3, as the semantic phases
/// read it: the fields of the declaration Phase 3 provisionally selected once
/// structural admission admitted its boundary and its `resourceLimits`
/// reference, whatever else of it or of the root it rejected (D15:2131,
/// 2367, 3434), each read on its own, where a check of it looks (D15:790),
/// and the `resourceLimits` reference it selected with, or, without a
/// selection, the causes of its absence in their place (D15:2131); the
/// selector; what Phase 4 rechecks of those phases' measures; and whether
/// structural admission admitted the whole root, which profile projection
/// needs (D15:2142).
pub(crate) struct SelectedConfiguration<'a> {
    fields: DeclarationFields<PolicyReference, TargetProfileReference>,
    /// Every reference of the declaration as Phase 4's bounds read it, whether
    /// or not structural admission admitted it, or the causes of no selection.
    occurrences: Admitted<Vec<ReferenceOccurrence>>,
    /// Where a check of the selected declaration looks, when there is one.
    observer: Option<DeclarationObserver<'a>>,
    resource_limits: Result<&'a PolicyReference, Causes>,
    selector: &'a SelectorInput,
    value: Admitted<ValueCounts>,
    /// How many declarations the root's `profiles` object holds, and every
    /// declared Profile ID, in their unsigned byte order.
    declarations: Admitted<(u64, Vec<ProfileId>)>,
    structural_units: Admitted<u64>,
    /// Whether structural admission admitted the whole root, or the causes.
    whole: Admitted<()>,
}

impl<'a> SelectedConfiguration<'a> {
    /// The configuration of `analysis`, which `entry` holds, once Phase 3
    /// provisionally selected one of its declarations, `selection`, with
    /// `selector`, each field as `values` holds what structural admission's
    /// candidates left at and below it, observed in the entry (D15:790).
    /// `whole` is whether structural admission admitted the whole root.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of checks that admit a field the analysis
    /// does not.
    pub(crate) fn new(
        values: &ValueAdmissions,
        (analysis, entry): (&'a Analysis, &'a dyn ConfigurationEntry),
        (selector, selection): (&'a SelectorInput, &'a ProvisionalSelection<PolicyReference>),
        whole: Admitted<()>,
    ) -> Result<Self, ResolverInvariant> {
        let fields = DeclarationFields::read(values, analysis, selection.id().as_str())
            .map_err(ResolverInvariant::Structural)?;
        let occurrences = DeclarationFragments::new(values, analysis, selection.id().as_str())
            .and_then(|fragments| fragments.reference_occurrences())
            .map_err(ResolverInvariant::Structural)?;
        let declarations = analysis
            .declarations()
            .map_err(admission)?
            .ok_or_else(unreported)?;
        let profiles = analysis.profiles_value().ok_or_else(unreported)?;
        let observer = DeclarationObserver::new(entry, profiles, selection.id().as_str())
            .map_err(ResolverInvariant::Observer)?;
        Ok(Self {
            fields,
            occurrences: Ok(occurrences),
            observer: Some(observer),
            resource_limits: Ok(selection.resource_limits()),
            selector,
            value: Ok(analysis.value_counts()),
            declarations: Ok(declarations),
            structural_units: Ok(analysis.structural_units().ok_or_else(unreported)?),
            whole,
        })
    }

    /// The configuration once Phase 3 selected no declaration, with
    /// `selector`, for `causes`, why it selected none: each field of the
    /// declaration and the `resourceLimits` reference are unavailable with
    /// them, so that each check that reads one is not evaluated with them
    /// (D15:2131). Without a Policy no recheck runs, each waiting on the same
    /// causes, so the measures are unavailable with them, but for the
    /// declarations `analysis` holds once its `profiles` member is an object:
    /// each declared Profile ID still names its recheck (D15:1171, 2184).
    ///
    /// # Errors
    ///
    /// Returns the defect of a key with the syntax that is no Profile ID.
    pub(crate) fn unselected(
        analysis: Option<&Analysis>,
        (selector, causes): (&'a SelectorInput, &Causes),
        whole: Admitted<()>,
    ) -> Result<Self, ResolverInvariant> {
        let declarations = analysis
            .map(Analysis::declarations)
            .transpose()
            .map_err(admission)?
            .flatten()
            .ok_or_else(|| causes.clone());
        Ok(Self {
            fields: DeclarationFields::unavailable(causes),
            occurrences: Err(causes.clone()),
            observer: None,
            resource_limits: Err(causes.clone()),
            selector,
            value: Err(causes.clone()),
            declarations,
            structural_units: Err(causes.clone()),
            whole,
        })
    }

    fn references(&self) -> DeclarationReferences<'_> {
        self.fields.references()
    }

    fn measures(&self) -> ConfigurationMeasures<'_> {
        let declarations = self.declarations.as_ref().map_err(Causes::clone);
        ConfigurationMeasures {
            value: self.value.clone(),
            declarations: declarations.clone().map(|(count, _)| *count),
            profile_ids: declarations.map(|(_, ids)| ids.as_slice()),
            selector: self.selector,
            structural_units: self.structural_units.clone(),
        }
    }

    /// Every Profile ID the root's `profiles` object declares, in unsigned
    /// byte order, or none when structural admission did not admit the object:
    /// the declared Profile IDs a selector can name (D15:807).
    pub(crate) fn declared(&self) -> &[ProfileId] {
        self.declarations
            .as_ref()
            .map_or(&[], |(_, ids)| ids.as_slice())
    }
}

/// The resolver bug of checks that admitted what structural admission's
/// analysis lacks.
fn unreported() -> ResolverInvariant {
    ResolverInvariant::Structural(StructuralInvariant::UnreportedFailure)
}

/// The resolver bug of an analysis whose model does not hold what it read.
fn admission(invariant: AdmissionInvariant) -> ResolverInvariant {
    ResolverInvariant::Structural(StructuralInvariant::Admission(invariant))
}

/// One invocation's inputs besides the configuration, as resolver
/// construction and 015's pre-invocation boundary admitted them: what the
/// resolver supports, the Profile Resolution Specification Input, the Admitted
/// Implementation Capability, 018's Resource Limit Policy Verification Input,
/// and the formally admitted collection and vocabulary. [`super::invocation`]
/// takes them from a constructed resolver and an admitted Resolver Invocation
/// Input Set; tests and the benchmark also build them.
pub(crate) struct Invocation<'a, Id> {
    pub(crate) supported: &'a SupportedSpecifications<Id>,
    pub(crate) specifications: &'a SpecificationInput<Id>,
    pub(crate) capability: &'a ImplementationCapability,
    pub(crate) verification: &'a VerificationInput,
    pub(crate) collection: &'a FormalCollection,
    pub(crate) vocabulary: &'a FormalVocabulary,
}

/// What each phase admitted, or the causes of its absence. The canonicalizer
/// Phase 5 binds runs Phases 7 and 8 but is no semantic value, so it is not
/// kept; the pins it was bound under are.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SemanticState<'s, Id> {
    /// Phase 2: whether structural admission admitted the whole root, which a
    /// complete `IntlifyConfig` and so profile projection need (D15:2142,
    /// 2367).
    pub(crate) structure: Admitted<()>,
    /// Phase 4: the confirmed policy, which also confirms the provisional
    /// selection, and the selector's String Length Fact.
    pub(crate) policy: PolicyOutcome,
    /// Phases 0 and 5: the complete Profile Resolution Specification Set.
    pub(crate) specifications: Admitted<SpecificationSet<Id>>,
    /// Phase 5: the canonicalization specification and the dataset the
    /// canonicalization data was bound under, which a checked profile
    /// retains (D15:1313).
    pub(crate) canonicalization: Admitted<SemanticPins<Id>>,
    /// Phase 5: the admitted vocabulary.
    pub(crate) vocabulary: Admitted<SurfaceVocabulary>,
    /// Phase 6.
    pub(crate) identity: Admitted<ProjectIdentity>,
    /// Phase 7.
    pub(crate) locales: ResolvedLocales,
    /// Phase 8.
    pub(crate) negotiation: Admitted<NegotiationProfile<Id>>,
    pub(crate) fallback: Admitted<MessageFallbackPolicy<Id>>,
    pub(crate) coverage: Admitted<CoveragePolicy<Id>>,
    /// Phase 9: the Set's artifact each present reference names, and the
    /// Resolved Profile Artifact Bindings.
    pub(crate) references: Admitted<ResolvedReferences<'s>>,
    pub(crate) bindings: Admitted<ArtifactBindings<'s>>,
    /// Phase 10.
    pub(crate) delivery: Admitted<DeliveryPolicy<Id>>,
    /// Phase 11: each declared target's subset, effective default, and
    /// Target Profile.
    pub(crate) targets: Admitted<ResolvedTargets<'s>>,
    /// Phase 12: each group's members, and whether the groups partition the
    /// targets.
    pub(crate) groups: Admitted<ResolvedGroups>,
    /// Phase 13: each group's checked hydration relations.
    pub(crate) hydration: Admitted<ResolvedHydration>,
}

/// A resolver defect some phase found, which no input can cause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResolverInvariant {
    Evaluation(EvaluationInvariant),
    /// Phase 4 found the selector observed under a bound below the final
    /// projection authority.
    Selection(SelectionInvariant),
    /// The provider failed in a way 015 never reports to an author.
    Canonicalization(CanonicalizationFailure),
    /// Phase 1 found a defect of strict file entry itself.
    Entry(EntryInvariant),
    /// Phase 2 found a defect of structural admission itself.
    Structural(StructuralInvariant),
    /// Phase 3 found a defect of profile selection itself.
    ProfileSelection(ProfileSelectionInvariant),
    /// A later phase's Evidence named a component without a path.
    Component(ComponentInvariant),
    /// A later phase looked at a value of the selected declaration that has
    /// no Evidence.
    Observer(ObserverInvariant),
    /// The entry dependencies named a source identity or a selector without
    /// a projection.
    Evidence(EvidenceInvariant),
    /// Reporting left a record without its view.
    Reporting(ReportingInvariant),
    /// Reporting finalization found a record the earlier stages fixed wrong,
    /// or a reserve past its bound, boxed since a rare bug need not widen
    /// every result.
    Finalization(Box<FinalizationInvariant>),
    /// Profile projection lacked a result, for these causes, although the
    /// invocation found no blocking Finding.
    Unprojected(Causes),
    /// The invocation's inputs were admitted against another resolver's
    /// Resolver Construction Identity, so no outcome could carry one equal to
    /// both the resolver's and the admitted reference (D15:1150).
    OtherConstruction,
    /// A replayed Materialized Resolution Input Set's value held a marker,
    /// which no value that entry admitted holds (D15:2657).
    RetainedMarker,
}

impl From<ReportingInvariant> for ResolverInvariant {
    fn from(invariant: ReportingInvariant) -> Self {
        Self::Reporting(invariant)
    }
}

impl From<FinalizationInvariant> for ResolverInvariant {
    fn from(invariant: FinalizationInvariant) -> Self {
        Self::Finalization(Box::new(invariant))
    }
}

impl From<ProfileSelectionInvariant> for ResolverInvariant {
    fn from(invariant: ProfileSelectionInvariant) -> Self {
        match invariant {
            ProfileSelectionInvariant::Evaluation(invariant) => Self::Evaluation(invariant),
            invariant => Self::ProfileSelection(invariant),
        }
    }
}

impl From<StructuralInvariant> for ResolverInvariant {
    fn from(invariant: StructuralInvariant) -> Self {
        match invariant {
            StructuralInvariant::Evaluation(invariant) => Self::Evaluation(invariant),
            invariant => Self::Structural(invariant),
        }
    }
}

impl From<EntryInvariant> for ResolverInvariant {
    fn from(invariant: EntryInvariant) -> Self {
        match invariant {
            EntryInvariant::Evaluation(invariant) => Self::Evaluation(invariant),
            invariant => Self::Entry(invariant),
        }
    }
}

impl From<EvaluationInvariant> for ResolverInvariant {
    fn from(invariant: EvaluationInvariant) -> Self {
        Self::Evaluation(invariant)
    }
}

impl From<GroupInvariant> for ResolverInvariant {
    fn from(invariant: GroupInvariant) -> Self {
        match invariant {
            GroupInvariant::Evaluation(invariant) => Self::Evaluation(invariant),
            GroupInvariant::Observer(invariant) => Self::Observer(invariant),
        }
    }
}

impl From<HydrationInvariant> for ResolverInvariant {
    fn from(invariant: HydrationInvariant) -> Self {
        match invariant {
            HydrationInvariant::Evaluation(invariant) => Self::Evaluation(invariant),
            HydrationInvariant::Component(invariant) => Self::Component(invariant),
            HydrationInvariant::Observer(invariant) => Self::Observer(invariant),
        }
    }
}

impl From<TargetInvariant> for ResolverInvariant {
    fn from(invariant: TargetInvariant) -> Self {
        match invariant {
            TargetInvariant::Evaluation(invariant) => Self::Evaluation(invariant),
            TargetInvariant::Component(invariant) => Self::Component(invariant),
            TargetInvariant::Observer(invariant) => Self::Observer(invariant),
        }
    }
}

impl From<ReferenceInvariant> for ResolverInvariant {
    fn from(invariant: ReferenceInvariant) -> Self {
        match invariant {
            ReferenceInvariant::Evaluation(invariant) => Self::Evaluation(invariant),
            ReferenceInvariant::Component(invariant) => Self::Component(invariant),
            ReferenceInvariant::Observer(invariant) => Self::Observer(invariant),
        }
    }
}

impl From<CollectionInvariant> for ResolverInvariant {
    fn from(invariant: CollectionInvariant) -> Self {
        match invariant {
            CollectionInvariant::Evaluation(invariant) => Self::Evaluation(invariant),
            CollectionInvariant::Component(invariant) => Self::Component(invariant),
        }
    }
}

impl From<SpecificationInvariant> for ResolverInvariant {
    fn from(invariant: SpecificationInvariant) -> Self {
        match invariant {
            SpecificationInvariant::Evaluation(invariant) => Self::Evaluation(invariant),
            SpecificationInvariant::Component(invariant) => Self::Component(invariant),
        }
    }
}

impl From<PolicyInvariant> for ResolverInvariant {
    fn from(invariant: PolicyInvariant) -> Self {
        match invariant {
            PolicyInvariant::Evaluation(invariant) => Self::Evaluation(invariant),
            PolicyInvariant::Selection(invariant) => Self::Selection(invariant),
            PolicyInvariant::Component(invariant) => Self::Component(invariant),
            PolicyInvariant::Observer(invariant) => Self::Observer(invariant),
        }
    }
}

impl From<LocaleInvariant> for ResolverInvariant {
    fn from(invariant: LocaleInvariant) -> Self {
        match invariant {
            LocaleInvariant::Evaluation(invariant) => Self::Evaluation(invariant),
            LocaleInvariant::Canonicalization(failure) => Self::Canonicalization(failure),
            LocaleInvariant::Observer(invariant) => Self::Observer(invariant),
        }
    }
}

/// Phases 4 to 13 of one invocation over `evaluation`, in 015's order, with
/// each semantic group's owner once the phases that establish the group have
/// run. `project_profile` is what Phase 0 admitted, which the invocation
/// records before configuration entry. `data` is the canonicalization data past the
/// pre-invocation boundary, which Phase 5 reads the rest of and binds, leaving
/// it to the caller to retain with the invocation's other inputs (D15:2589).
/// The Profile Resolution Artifact Set Phase 9 admits is kept in `set`, which the
/// resolved references and the bindings borrow.
pub(crate) fn resolve_profile<'s, P, D>(
    evaluation: &mut Evaluation,
    project_profile: &Admitted<VersionedIdentity<P::Identity>>,
    configuration: &SelectedConfiguration<'_>,
    invocation: &Invocation<'s, P::Identity>,
    data: &D,
    set: &'s mut Option<Admitted<ArtifactSet<'s>>>,
) -> Result<SemanticState<'s, P::Identity>, ResolverInvariant>
where
    P: Provider,
    P::Identity: AsRef<str>,
    D: FormalData<Provider = P>,
{
    let fields = &configuration.fields;
    let declared = configuration.references();
    let policy = admit_resource_policy(
        evaluation,
        invocation.capability,
        invocation.collection,
        configuration.resource_limits.as_ref().copied(),
        (
            configuration.occurrences.as_deref(),
            configuration.observer.as_ref(),
        ),
        &configuration.measures(),
        invocation.verification,
    )?;
    let confirmed = policy.confirmed.as_ref();
    let admitted = admit_specifications(
        evaluation,
        invocation.supported,
        invocation.specifications,
        confirmed,
        invocation.vocabulary,
        data,
        &invocation.capability.bootstrap_only().locale_data,
    )?;
    let canonicalizer = admitted.canonicalizer.as_ref();
    own(
        evaluation,
        SemanticGroup::LocaleCanonicalization,
        held(canonicalizer),
    )?;
    // A record constructed since classifies a locale key by the canonicalizer
    // Phase 5 bound (D15:776).
    let observer = configuration
        .observer
        .as_ref()
        .map(|observer| match canonicalizer {
            Ok(canonicalizer) => observer.with_locales(canonicalizer),
            Err(_) => *observer,
        });
    let canonicalization = canonicalizer
        .map(|canonicalizer| canonicalizer.binding().pins.clone())
        .map_err(Causes::clone);
    let identity = resolve_project_identity(
        evaluation,
        confirmed,
        fields.project_id(),
        fields.selection_scope(),
    )?;
    own(
        evaluation,
        SemanticGroup::ProjectIdentity,
        held(identity.as_ref()),
    )?;
    let locale_fields = fields.locale_fields();
    let locales = resolve_locales(
        evaluation,
        confirmed,
        canonicalizer,
        &locale_fields,
        observer.as_ref(),
    )?;
    own(
        evaluation,
        SemanticGroup::ProjectLocales,
        held(locales.project.as_ref()),
    )?;
    let negotiation = resolve_negotiation(
        evaluation,
        confirmed,
        &admitted.locale_negotiation,
        &locales,
        observer.as_ref(),
    )?;
    own(
        evaluation,
        SemanticGroup::LocaleNegotiation,
        held(negotiation.as_ref()),
    )?;
    let fallback = resolve_fallback(
        evaluation,
        confirmed,
        &admitted.message_locale_fallback,
        canonicalizer,
        &locales,
        &locale_fields.message_fallback,
        observer.as_ref(),
    )?;
    own(
        evaluation,
        SemanticGroup::MessageFallback,
        held(fallback.as_ref()),
    )?;
    let coverage = resolve_coverage(
        evaluation,
        confirmed,
        &admitted.coverage,
        (admitted.vocabulary.as_ref(), canonicalizer),
        &locales,
        &locale_fields.coverage,
        observer.as_ref(),
    )?;
    own(evaluation, SemanticGroup::Coverage, held(coverage.as_ref()))?;
    let set = &*set.insert(admit_collection(
        evaluation,
        confirmed,
        invocation.collection,
    )?);
    // Each reference keeps its own result, which a target reads; the
    // bindings need every one. A target's reference waits on its bounds.
    let results = resolve_fields(
        evaluation,
        set.as_ref(),
        &declared,
        (locales.targets.as_deref(), observer.as_ref()),
    )?;
    // Both policy groups are the confirmed declaration's, and each waits on
    // the object apart from its slots and only on the slots it holds
    // (D15:2137, 2365, 3445). The required one also holds the confirmed
    // policy's bounds.
    own(
        evaluation,
        SemanticGroup::RequiredPolicies,
        and(held(confirmed), results.policies(&REQUIRED_POLICIES)),
    )?;
    own(
        evaluation,
        SemanticGroup::OptionalPolicies,
        and(held(confirmed), results.policies(&OPTIONAL_POLICIES)),
    )?;
    let references = results.all();
    let bindings = references
        .as_ref()
        .map(ArtifactBindings::bind)
        .map_err(Causes::clone);
    let delivery = resolve_delivery(
        evaluation,
        &admitted.delivery_graph,
        &admitted.delivery_placement,
        fields.delivery(),
    )?;
    own(evaluation, SemanticGroup::Delivery, held(delivery.as_ref()))?;
    let targets = resolve_targets(evaluation, &locales, (&results, observer.as_ref()))?;
    own(
        evaluation,
        SemanticGroup::Targets,
        targets
            .as_ref()
            .map_or_else(|causes| Err(causes.clone()), ResolvedTargets::complete),
    )?;
    let groups = resolve_groups(
        evaluation,
        confirmed,
        &fields.group_fields(),
        (targets.as_ref(), observer.as_ref()),
    )?;
    // Every relation shares the negotiation profile; none compares it.
    let hydration = validate_hydration(
        evaluation,
        confirmed,
        &fields.hydration_fields(),
        targets.as_ref(),
        (groups.as_ref(), observer.as_ref()),
        negotiation.as_ref().map(|_| ()).map_err(Causes::clone),
    )?;
    // The groups' members and partition, and their checked relations; the
    // negotiation profile the relations share is `localeNegotiation`'s.
    own(
        evaluation,
        SemanticGroup::DeploymentGroups,
        and(
            groups.as_ref().map_or_else(
                |causes| Err(causes.clone()),
                |groups| groups.partition().clone(),
            ),
            hydration
                .as_ref()
                .map_or_else(|causes| Err(causes.clone()), ResolvedHydration::complete),
        ),
    )?;
    Ok(SemanticState {
        structure: configuration.whole.clone(),
        specifications: SpecificationSet::new(project_profile, &admitted),
        canonicalization,
        vocabulary: admitted.vocabulary,
        policy,
        identity,
        locales,
        negotiation,
        fallback,
        coverage,
        references,
        bindings,
        delivery,
        targets,
        groups,
        hydration,
    })
}

/// Whether a phase established `result`, or the causes of its absence: what
/// a semantic group's owner waits on.
fn held<T>(result: Result<T, &Causes>) -> Admitted<()> {
    result.map(|_| ()).map_err(Causes::clone)
}

/// The authoring schema structural admission reads a configuration with.
pub(crate) type ConfigurationSchema = AuthoringSchema<PolicyReference, TargetProfileReference>;

/// What one entry's invocation leaves for its outcome: the semantic state
/// once the entry materialized a value, or else the blocking Phase 1 Finding
/// Keys that leave the Shared projection unavailable (D15:2369), and what the
/// entry dependency records name (D15:2516).
#[derive(Debug)]
pub(crate) struct Invoked<'s, Id> {
    pub(crate) state: Result<SemanticState<'s, Id>, Causes>,
    pub(crate) entry: EntryInputs,
}

/// One file entry's invocation over `evaluation`: Phase 0, then Phase 1 over
/// `file` under `limits` and under the capability's path bounds, the ones
/// path admission applies (plan 025 decision #299), Phase 2 over the value
/// Phase 1 admits with `schema` under `structural`, Phase 3 selecting
/// `selector`'s declaration, and the semantic phases over the configuration,
/// or, without a declaration Phase 3 selected, over the causes of its
/// absence (D15:2131). Without a value there is no Shared projection and so
/// no semantic state (D15:2369). The selector's final projection authority is
/// the one Phase 4 fixed, or the capability's without a value, and the
/// Profile IDs it may name are those the root's `profiles` object declares,
/// none without a value (D15:807, 816-817). `value` receives the canonical
/// tree of the value Phase 1 admits, and stays as it was without one
/// (D15:2657).
///
/// # Errors
///
/// Returns the resolver bug that a phase or the entry dependencies found.
pub(crate) fn invoke_file<'s, P, D>(
    evaluation: &mut Evaluation,
    (file, limits): (&EntryFile<'_>, InputLimits),
    (schema, structural, selector): (&ConfigurationSchema, StructuralLimits, &SelectorInput),
    invocation: &Invocation<'s, P::Identity>,
    data: &D,
    (set, value): (&'s mut Option<Admitted<ArtifactSet<'s>>>, &mut Option<Node>),
) -> Result<Invoked<'s, P::Identity>, ResolverInvariant>
where
    P: Provider,
    P::Identity: AsRef<str>,
    D: FormalData<Provider = P>,
{
    let project_profile =
        admit_project_profile(evaluation, invocation.supported, invocation.specifications)?;
    let paths = &invocation.capability.bootstrap_only().evidence_metadata;
    let resolved = match admit_entry(evaluation, file, limits, paths)? {
        Ok(document) => resolve_entered(
            evaluation,
            (&project_profile, file, document),
            (schema, structural, selector),
            invocation,
            data,
            (set, value),
        )?,
        Err(causes) => Resolved::unentered(causes, invocation),
    };
    let entry = EntryInputs::file(file, selector, resolved.authority, &resolved.declared)
        .map_err(ResolverInvariant::Evidence)?;
    Ok(Invoked {
        state: resolved.state,
        entry,
    })
}

/// One programmatic entry's invocation over `evaluation`, as a file entry's
/// with Phase 1 over the Snapshot of `entry` under `limits`. The phases after
/// it observe the entry through the document Phase 1 materialized
/// (D15:2129, 3228).
///
/// # Errors
///
/// Returns the resolver bug that a phase or the entry dependencies found.
pub(crate) fn invoke_programmatic<'s, P, D>(
    evaluation: &mut Evaluation,
    (entry, limits): (&EntrySnapshot<'_>, SnapshotEntryLimits),
    (schema, structural, selector): (&ConfigurationSchema, StructuralLimits, &SelectorInput),
    invocation: &Invocation<'s, P::Identity>,
    data: &D,
    (set, value): (&'s mut Option<Admitted<ArtifactSet<'s>>>, &mut Option<Node>),
) -> Result<Invoked<'s, P::Identity>, ResolverInvariant>
where
    P: Provider,
    P::Identity: AsRef<str>,
    D: FormalData<Provider = P>,
{
    let project_profile =
        admit_project_profile(evaluation, invocation.supported, invocation.specifications)?;
    let paths = &invocation.capability.bootstrap_only().evidence_metadata;
    let resolved = match admit_snapshot_entry(evaluation, entry, limits, paths)? {
        Ok(document) => {
            // A later phase names a value or a key by its locus in the
            // document, so the entry is built before the document moves.
            let programmatic = ProgrammaticEntry::new(entry, &document);
            resolve_entered(
                evaluation,
                (&project_profile, &programmatic, document),
                (schema, structural, selector),
                invocation,
                data,
                (set, value),
            )?
        }
        Err(causes) => Resolved::unentered(causes, invocation),
    };
    let inputs = EntryInputs::programmatic(entry, selector, resolved.authority, &resolved.declared)
        .map_err(ResolverInvariant::Evidence)?;
    Ok(Invoked {
        state: resolved.state,
        entry: inputs,
    })
}

/// What an invocation's phases leave: the semantic state or the causes of its
/// absence, and for the entry dependencies the selector's final projection
/// authority and the Profile IDs the selector may name (D15:807, 816-817).
struct Resolved<'s, Id> {
    state: Result<SemanticState<'s, Id>, Causes>,
    authority: Bound,
    declared: Vec<ProfileId>,
}

impl<Id> Resolved<'_, Id> {
    /// Without a value, for `causes`: the capability's authority, and no
    /// declared Profile ID (D15:2369).
    fn unentered(causes: Causes, invocation: &Invocation<'_, Id>) -> Self {
        Self {
            state: Err(causes),
            authority: invocation
                .capability
                .policy_comparable()
                .configuration_input
                .structural
                .max_profile_id_bytes,
            declared: Vec::new(),
        }
    }
}

/// The phases after Phase 1 over `document`, the value it admitted, observed
/// through `entry` ([`resolve_materialized`]), the document's value first
/// copied into `value`.
fn resolve_entered<'s, P, D>(
    evaluation: &mut Evaluation,
    (project_profile, entry, document): (
        &Admitted<VersionedIdentity<P::Identity>>,
        &dyn ConfigurationEntry,
        MaterializedDocument,
    ),
    (schema, structural, selector): (&ConfigurationSchema, StructuralLimits, &SelectorInput),
    invocation: &Invocation<'s, P::Identity>,
    data: &D,
    (set, value): (&'s mut Option<Admitted<ArtifactSet<'s>>>, &mut Option<Node>),
) -> Result<Resolved<'s, P::Identity>, ResolverInvariant>
where
    P: Provider,
    P::Identity: AsRef<str>,
    D: FormalData<Provider = P>,
{
    // The Materialized Resolution Input Set's value, copied once Phase 1
    // materialized it and before Phase 2 consumes the document (D15:2657).
    *value = Some(document.canonical_value());
    let (state, declared) = resolve_materialized(
        evaluation,
        (project_profile, entry, document),
        (schema, structural, selector),
        invocation,
        data,
        set,
    )?;
    Ok(Resolved {
        authority: state.policy.authority,
        declared,
        state: Ok(state),
    })
}

/// Phases 2 to 13 over `document`, a materialized value, observed through
/// `entry`: Phases 2 and 3, and the semantic phases over the configuration
/// Phase 3 selected or over the causes of its absence (D15:2131). It leaves
/// the semantic state and the Profile IDs the root declares. An invocation
/// runs them over the document Phase 1 admitted, and a replay over the one
/// of its retained value (D15:2659).
///
/// # Errors
///
/// Returns the resolver bug that a phase found.
pub(crate) fn resolve_materialized<'s, P, D>(
    evaluation: &mut Evaluation,
    (project_profile, entry, document): (
        &Admitted<VersionedIdentity<P::Identity>>,
        &dyn ConfigurationEntry,
        MaterializedDocument,
    ),
    (schema, structural, selector): (&ConfigurationSchema, StructuralLimits, &SelectorInput),
    invocation: &Invocation<'s, P::Identity>,
    data: &D,
    set: &'s mut Option<Admitted<ArtifactSet<'s>>>,
) -> Result<(SemanticState<'s, P::Identity>, Vec<ProfileId>), ResolverInvariant>
where
    P: Provider,
    P::Identity: AsRef<str>,
    D: FormalData<Provider = P>,
{
    let StructuralAdmission {
        analysis: structure,
        values,
    } = admit_structure(
        evaluation,
        entry,
        document,
        schema,
        structural,
        held(project_profile.as_ref()),
    )?;
    // Phase 3 records its checks whatever Phase 2 admitted, with its causes
    // where it admitted nothing (D15:2131). The semantic phases read the
    // declaration it selects once structural admission admitted its boundary
    // and its `resourceLimits` reference, whatever else of it or of the root
    // it rejected (plan 025 interpretation #218), and without one each check
    // that reads the declaration waits on the causes (D15:2131).
    let selection = select_profile(
        evaluation,
        entry,
        structure.as_ref().ok(),
        selector,
        structural,
    )?;
    let root = whole(evaluation);
    let configuration = match (&structure, &selection) {
        (Ok(analysis), Ok(selection)) => {
            SelectedConfiguration::new(&values, (analysis, entry), (selector, selection), root)?
        }
        (_, Err(causes)) => {
            SelectedConfiguration::unselected(structure.as_ref().ok(), (selector, causes), root)?
        }
        (Err(_), Ok(_)) => {
            return Err(ProfileSelectionInvariant::UnreportedFailure.into());
        }
    };
    let state = resolve_profile(
        evaluation,
        project_profile,
        &configuration,
        invocation,
        data,
        set,
    )?;
    Ok((state, configuration.declared().to_vec()))
}

// `handoff`'s tests resolve the same inputs.
#[cfg(test)]
pub(super) mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    use intlify_shared_json::token::Token;
    use serde_json::{json, Value};

    use super::*;
    use crate::artifacts::collection::{read_collection, CollectionLimits, CollectionReport};
    use crate::artifacts::content::sample;
    use crate::artifacts::envelope::{envelope_of, ArtifactKind, ExactTuple, FormalMember};
    use crate::artifacts::vectors::vocabulary::{self as vectors, formal_of};
    use crate::boundary::call_site::AdmittedCallSite;
    use crate::boundary::snapshot::{read_report, submitted, SnapshotReport};
    use crate::boundary::source::{file_source_revision, read_entry_pair, SourceLimits};
    use crate::construction::identity::ConstructionIdentity;
    use crate::construction::registry::check::testing::{check_id, requirements};
    use crate::construction::registry::check::CheckRegistry;
    use crate::input_limits::{Bound, InputLimits, RawInputLimits};
    use crate::locale::fixtures::{fixture_pins, FixtureProvider};
    use crate::locale::{
        CanonicalLocale, DataNotAdmitted, DataReading, LocaleDataLimits, ProviderFailure,
        Supported, VersionedIdentity,
    };
    use crate::materialize::materialize_file;
    use crate::model::CoverageMode;
    use crate::profile_fixtures::complete_config;
    use crate::resolver::call_site_span::fixtures::{mirrored, sidecar};
    use crate::resolver::capability::{BootstrapLimits, EvidenceMetadataLimits};
    use crate::resolver::delivery::PlacementMode;
    use crate::resolver::dependency::{self, EntryDependency, SemanticGroup};
    use crate::resolver::entry::fixtures::{classify, FileFixture};
    use crate::resolver::entry::{admit_entry, input, OWNER};
    use crate::resolver::evaluation::{
        Check, Code, IssueKey, Phase, Reason, Status, Subject, Unit,
    };
    use crate::resolver::evidence::Source;
    use crate::resolver::evidence_metadata::fixtures::{unbounded, with};
    use crate::resolver::evidence_metadata::MetadataBound;
    use crate::resolver::evidence_token::EvidenceTokens;
    use crate::resolver::exact_resolution;
    use crate::resolver::groups::{self, GroupSubject};
    use crate::resolver::hydration::{self, HydrationSubject, RelationKey};
    use crate::resolver::limits::{fixture_limits, BoundId, Group, ResolutionLimits, SECTION};
    use crate::resolver::locales::fixtures::canonical;
    use crate::resolver::locales::{self, LocaleKey, LocaleSubject, Location};
    use crate::resolver::occurrence::OccurrenceCandidate;
    use crate::resolver::outcome::{outcome, Outcome};
    use crate::resolver::profile_selection::{capability, selector, SelectionSubject};
    use crate::resolver::profile_view::{project, ProfileView};
    use crate::resolver::resource_policy::{
        self, ArtifactSubject, PolicySlot, PolicySubject, ReferenceField,
    };
    use crate::resolver::snapshot_entry::fixtures::ENTRY;
    use crate::resolver::specification::{
        admit_project_profile, ActiveProfileSpecification, Slot, VocabularyAssertion,
    };
    use crate::resolver::structural_admission::{selection, CAPABILITY};
    use crate::resolver::subject::{EntryPath, SubjectKind};
    use crate::resolver::targets::{self, TargetSubject};
    use crate::resolver::token::{Sha256, Specification};
    use crate::snapshot::from_json;
    use crate::structural::selection::{Selection, StringLengthFact};
    use crate::structural::AuthoringSchema;

    type Id = &'static str;
    type Schema = AuthoringSchema<PolicyReference, TargetProfileReference>;

    /// The Active Project Profile Specification the invocations here run
    /// under.
    pub(crate) const ACTIVE: VersionedIdentity<Id> = VersionedIdentity {
        identity: "test-only-profile-specification",
        revision: "0",
    };

    /// Each policy slot besides `resourceLimits`, the kind its role takes, and
    /// the identity of the artifact it names.
    const POLICIES: [(&str, ArtifactKind, &str); 6] = [
        ("trust", ArtifactKind::TrustPolicy, "storefront-trust"),
        (
            "sourceAdmission",
            ArtifactKind::SourceAdmissionPolicy,
            "storefront-source",
        ),
        (
            "approval",
            ArtifactKind::ApprovalPolicy,
            "storefront-approval",
        ),
        (
            "selection",
            ArtifactKind::SelectionPolicy,
            "storefront-selection",
        ),
        (
            "providerRouting",
            ArtifactKind::ProviderRoutingPolicy,
            "storefront-routing",
        ),
        (
            "glossarySet",
            ArtifactKind::GlossarySet,
            "storefront-glossary",
        ),
    ];

    /// Each target and the identity of the Target Profile it names.
    const TARGETS: [(&str, &str); 2] = [
        ("browser", "storefront-browser"),
        ("server", "storefront-server"),
    ];

    /// Where `Inputs::complete` submits the trust policy.
    const TRUST: usize = 1;

    /// The phases the entry runs, in 015's order.
    const PHASES: [Phase; 11] = [
        Phase::ResolverBootstrapAdmission,
        Phase::ResourcePolicyAdmission,
        Phase::SpecificationAdmission,
        Phase::ProjectIdentityResolution,
        Phase::LocaleResolution,
        Phase::LocalePolicyResolution,
        Phase::ArtifactReferenceResolution,
        Phase::DeliveryResolution,
        Phase::TargetResolution,
        Phase::GroupResolution,
        Phase::HydrationValidation,
    ];

    /// Phase 0's one unit.
    const PHASE_0: Unit = Unit {
        phase: Phase::ResolverBootstrapAdmission,
        subject: Subject::Slot(Slot::ProjectProfile),
        check: Check::SupportedAssertion,
    };

    /// Limits no test reaches.
    const ROOM: CollectionLimits = CollectionLimits {
        member_bytes: u64::MAX,
        total_bytes: u64::MAX,
        depth: u64::MAX,
    };

    /// A sealed Target Profile with the sample content declaring `roles`.
    fn target_profile(identity: &str, roles: &[&str]) -> Value {
        let mut content = sample(ArtifactKind::TargetProfile);
        content["hydration"]["roles"] = json!(roles);
        envelope_of(ArtifactKind::TargetProfile, identity, "1", content)
    }

    fn bound(value: u64) -> Bound {
        Bound::new(value).unwrap()
    }

    /// The capability's bounds, each above the policy's. Phases 1 to 3 run
    /// under its `configurationInput`.
    fn capability_limits() -> ResolutionLimits {
        fixture_limits(100_000)
    }

    /// The selected policy's bounds, with room for every input here.
    pub(crate) fn policy_limits() -> ResolutionLimits {
        fixture_limits(50_000)
    }

    /// The Resource Limit Policy setting `limits`.
    fn resource_limit_policy(limits: &ResolutionLimits) -> Value {
        envelope_of(
            ArtifactKind::ResourceLimitPolicy,
            "storefront-limits",
            "1",
            json!({ SECTION: limits.section() }),
        )
    }

    /// `limits`, but each of `bounds` at its limit.
    fn tightened(limits: &ResolutionLimits, bounds: &[(BoundId, u64)]) -> ResolutionLimits {
        let current = limits.bounds();
        ResolutionLimits::from_bounds(|id| {
            bounds.iter().find(|(bound, _)| *bound == id).map_or_else(
                || {
                    current
                        .iter()
                        .find(|(bound, _)| *bound == id)
                        .map(|&(_, value)| value)
                },
                |&(_, limit)| Bound::new(limit),
            )
        })
        .unwrap()
    }

    /// A sealed envelope of `kind` with an admitted content.
    fn sealed(kind: ArtifactKind, identity: &str) -> Value {
        envelope_of(kind, identity, "1", sample(kind))
    }

    fn tuple_of(member: &Value) -> ExactTuple {
        FormalMember::new(member.clone())
            .unwrap()
            .envelope()
            .unwrap()
            .tuple()
            .clone()
    }

    /// The reference a declaration writes for `tuple`.
    fn reference_value(tuple: &ExactTuple) -> Value {
        json!({
            "kind": tuple.kind.spelling(),
            "identity": tuple.identity.as_str(),
            "revision": tuple.revision.as_str(),
            "specificationRevision": tuple.specification_revision.as_str(),
            "semanticDigest": tuple.semantic_digest.as_str(),
        })
    }

    fn collection(members: &[Value]) -> FormalCollection {
        let texts: Vec<String> = members.iter().map(Value::to_string).collect();
        let bytes: Vec<&[u8]> = texts.iter().map(String::as_bytes).collect();
        read_collection(Some(CollectionReport::Formal(&bytes)), &ROOM).unwrap()
    }

    const fn identity(identity: Id, revision: Id) -> VersionedIdentity<Id> {
        VersionedIdentity { identity, revision }
    }

    /// All eight slots, each asserting what the resolver supports.
    fn specifications() -> SpecificationInput<Id> {
        SpecificationInput {
            project_profile: Some(ACTIVE),
            locale_canonicalization: Some(fixture_pins().specification),
            locale_negotiation: Some(identity("intlify-locale-negotiation", "0")),
            message_locale_fallback: Some(identity("intlify-message-locale-fallback", "0")),
            coverage: Some(identity("intlify-localization-coverage", "0")),
            intent_surface_class_vocabulary: Some(VocabularyAssertion {
                identity: vectors::IDENTITY,
                revision: vectors::REVISION,
                semantic_digest: vectors::SEMANTIC_DIGEST,
            }),
            delivery_graph: Some(identity("intlify-delivery-graph", "0")),
            delivery_placement: Some(identity("intlify-delivery-placement", "0")),
        }
    }

    /// The file `config` is submitted as, and the bounds strict file entry
    /// reads it under.
    fn entry_input(config: &Value, capability: &ResolutionLimits) -> (Arc<[u8]>, InputLimits) {
        let limits = InputLimits {
            raw: RawInputLimits {
                max_file_bytes: bound(1_000_000),
                max_parser_tokens: bound(1_000_000),
            },
            value: capability.configuration_input.value,
        };
        (Arc::from(serde_json::to_vec(config).unwrap()), limits)
    }

    /// Phases 1 and 2 over `config` under the capability's
    /// `configurationInput`.
    fn analyze(config: &Value, capability: &ResolutionLimits) -> Analysis {
        let (source, input) = entry_input(config, capability);
        let doc = materialize_file(source, input).unwrap();
        Schema::for_model()
            .unwrap()
            .analyze(Arc::new(doc), capability.configuration_input.structural)
            .unwrap()
    }

    /// Reverses the member order of every object in `value`.
    pub(crate) fn reverse_objects(value: &mut Value) {
        match value {
            Value::Object(map) => {
                let mut members: Vec<(String, Value)> = std::mem::take(map).into_iter().collect();
                members.reverse();
                for (name, mut member) in members {
                    reverse_objects(&mut member);
                    map.insert(name, member);
                }
            }
            Value::Array(values) => values.iter_mut().for_each(reverse_objects),
            _ => {}
        }
    }

    /// Reverses every array under a member whose schema declares it a set:
    /// the requested locales and coverage selectors, the rules, and each
    /// group's members and hydration relations. A fallback sequence's order
    /// is semantic, so it stays, and nothing under `messageFallback` is read
    /// by name: a source locale there may spell a set's name (D15:516, 1519,
    /// 2184).
    fn reverse_sets(value: &mut Value) {
        match value {
            Value::Object(members) => {
                for (name, member) in members.iter_mut() {
                    if name == "messageFallback" {
                        continue;
                    }
                    if let (
                        "requestedLocales"
                        | "intentSurfaceClasses"
                        | "rules"
                        | "members"
                        | "hydrationRelations",
                        Value::Array(items),
                    ) = (name.as_str(), &mut *member)
                    {
                        items.reverse();
                    }
                    reverse_sets(member);
                }
            }
            Value::Array(values) => values.iter_mut().for_each(reverse_sets),
            _ => {}
        }
    }

    /// One invocation's inputs, which a test edits before resolving them.
    pub(crate) struct Inputs {
        config: Value,
        /// The Profile ID the selector names.
        selected: &'static str,
        /// Whether no selector is submitted, in place of one naming
        /// `selected`.
        unnamed: bool,
        members: Vec<Value>,
        specifications: SpecificationInput<Id>,
        /// The members of the vocabulary submitted against its slot.
        surfaces: &'static [&'static str],
        capability: ResolutionLimits,
        /// What trusted setup pinned in 018's Verification Input: the policy
        /// the declaration selects, whatever a test then changes.
        pinned: ExactTuple,
        /// The capability's `evidenceMetadata` bounds. At their minima the
        /// resolver's own Evidence metadata work would exhaust both
        /// diagnostic domains of every outcome, so they are `unbounded`
        /// unless a test sets them.
        metadata: EvidenceMetadataLimits,
    }

    impl Inputs {
        /// Profile `web`: source `en`, requested `ja` and `en`, default `en`,
        /// every optional field declared, and each reference naming one
        /// submitted artifact exactly; all eight slots asserted. Both targets
        /// request `ja` and `en`, and the server's Target Profile declares
        /// `ssr-renderer` and the browser's `browser-hydration-client`. The
        /// selected policy is submitted first, pinned by the Verification
        /// Input, and the trust policy second.
        pub(crate) fn complete() -> Self {
            let mut config = complete_config();
            let profiles = config["profiles"].as_object_mut().unwrap();
            let mut web = profiles.remove("app").unwrap();
            web["requestedLocales"] = json!(["ja", "en"]);
            web["defaultRequestedLocale"] = json!("en");
            web["defaultSourceLocale"] = json!("en");
            web["localeNegotiation"] = json!({"aliases": {"de": "en"}});
            web["targetProfiles"]["browser"]["requestedLocales"] = json!(["ja", "en"]);
            web["targetProfiles"]["server"]["requestedLocales"] = json!(["ja", "en"]);
            profiles.insert("web".into(), web);
            let policy = resource_limit_policy(&policy_limits());
            let mut inputs = Self {
                config,
                selected: "web",
                unnamed: false,
                members: vec![policy.clone()],
                specifications: specifications(),
                surfaces: &vectors::MEMBERS,
                capability: capability_limits(),
                pinned: tuple_of(&policy),
                metadata: unbounded(),
            };
            inputs.select_policy(policy);
            for (slot, kind, identity) in POLICIES {
                let member = sealed(kind, identity);
                inputs.declaration()["policies"][slot] = reference_value(&tuple_of(&member));
                inputs.members.push(member);
            }
            for (target, identity) in TARGETS {
                let role = match target {
                    "server" => "ssr-renderer",
                    _ => "browser-hydration-client",
                };
                let member = target_profile(identity, &[role]);
                inputs.declaration()["targetProfiles"][target]["profile"] =
                    reference_value(&tuple_of(&member));
                inputs.members.push(member);
            }
            inputs
        }

        /// Submits `member` in place of the Target Profile `target` names, and
        /// makes the target name it.
        fn replace_target_profile(&mut self, target: &str, member: Value) {
            let tuple = tuple_of(&member);
            let named = self.declaration()["targetProfiles"][target]["profile"]["identity"].clone();
            let index = self
                .members
                .iter()
                .position(|candidate| candidate["body"]["identity"] == named)
                .unwrap();
            self.members[index] = member;
            self.declaration()["targetProfiles"][target]["profile"] = reference_value(&tuple);
        }

        /// `complete`'s profile requesting one locale, so that a check over
        /// the requested set waits once that locale's unit does not pass: a
        /// unit over several values waits only when every one is excluded
        /// (D15:2184).
        pub(crate) fn one_locale() -> Self {
            let mut inputs = Self::complete();
            let app = inputs.declaration();
            app["requestedLocales"] = json!(["en"]);
            app["targetProfiles"]["browser"]["requestedLocales"] = json!(["en"]);
            app["targetProfiles"]["server"]["requestedLocales"] = json!(["en"]);
            app.as_object_mut().unwrap().remove("messageFallback");
            app["coverage"] = json!({"defaultMode": "direct-required"});
            inputs
        }

        /// `complete`'s profile with a second group, `jobs`: its one member
        /// `worker` requests only `ja`, overrides the default with it, and
        /// names a Target Profile of its own, submitted last.
        pub(crate) fn two_groups() -> Self {
            let mut inputs = Self::complete();
            let member = target_profile("storefront-worker", &[]);
            inputs.declaration()["targetProfiles"]["worker"] = json!({
                "profile": reference_value(&tuple_of(&member)),
                "requestedLocales": ["ja"],
                "defaultRequestedLocale": "ja",
            });
            inputs.declaration()["deploymentGroups"]["jobs"] =
                json!({"members": ["worker"], "hydrationRelations": []});
            inputs.members.push(member);
            inputs
        }

        /// `complete`'s profile under a vocabulary without surface classes,
        /// asserted exactly, and without the coverage rules, which name them.
        pub(crate) fn without_surface_classes() -> Self {
            let mut inputs = Self::complete();
            inputs.surfaces = &[];
            inputs.specifications.intent_surface_class_vocabulary = Some(VocabularyAssertion {
                identity: vectors::IDENTITY,
                revision: vectors::REVISION,
                semantic_digest: vectors::EMPTY_SEMANTIC_DIGEST,
            });
            inputs.declaration()["coverage"]
                .as_object_mut()
                .unwrap()
                .remove("rules");
            inputs
        }

        pub(crate) fn declaration(&mut self) -> &mut Value {
            &mut self.config["profiles"][self.selected]
        }

        /// What a binding submits for these inputs besides the specification
        /// input: the configuration, the Profile ID the selector names, the
        /// collection's members, the vocabulary's members, the capability, and
        /// the policy the Verification Input pins.
        pub(crate) fn submitted(
            &self,
        ) -> (
            &Value,
            &str,
            &[Value],
            &[&str],
            ImplementationCapability,
            &ExactTuple,
        ) {
            let capability = ImplementationCapability::fixture_owned(self.capability.clone())
                .with_evidence_metadata(self.metadata);
            let config = &self.config;
            (
                config,
                self.selected,
                &self.members,
                self.surfaces,
                capability,
                &self.pinned,
            )
        }

        /// The capability's `evidenceMetadata` bounds `limits` in place of
        /// the unbounded ones.
        pub(crate) fn metadata(&mut self, limits: EvidenceMetadataLimits) {
            self.metadata = limits;
        }

        /// The collection with `member` submitted after its other members.
        pub(crate) fn submit(&mut self, member: Value) {
            self.members.push(member);
        }

        /// The vocabulary with `surfaces` as its submitted members.
        pub(crate) fn submit_surfaces(&mut self, surfaces: &'static [&'static str]) {
            self.surfaces = surfaces;
        }

        /// The specification input without its `project-profile` assertion,
        /// which Phase 0 then finds missing (D15:1125).
        pub(crate) fn unasserted(&mut self) {
            self.specifications.project_profile = None;
        }

        /// The same inputs with the selected declaration declared again under
        /// `id`, which the selector then names: two declarations differing
        /// only in their Profile IDs.
        pub(crate) fn selecting(&self, id: &'static str) -> Self {
            let mut config = self.config.clone();
            config["profiles"][id] = config["profiles"][self.selected].clone();
            Self {
                config,
                selected: id,
                unnamed: self.unnamed,
                members: self.members.clone(),
                specifications: self.specifications.clone(),
                surfaces: self.surfaces,
                capability: self.capability.clone(),
                pinned: self.pinned.clone(),
                metadata: self.metadata,
            }
        }

        /// The invocation of the configuration's file, or of `bytes` when
        /// given, for `inspect` with what it recorded and what it left.
        fn entered<R>(
            &self,
            bytes: Option<Arc<[u8]>>,
            inspect: impl FnOnce(Evaluation, Invoked<'_, Id>) -> R,
        ) -> R {
            self.entered_under(bytes, |_, evaluation, invoked| inspect(evaluation, invoked))
        }

        /// The outcome of the invocation of the configuration's file, or of
        /// `bytes` when given, under `identity`, for `inspect` with what the
        /// invocation recorded and what it left.
        pub(crate) fn outcome<R>(
            &self,
            bytes: Option<Arc<[u8]>>,
            identity: ConstructionIdentity,
            inspect: impl FnOnce(&Evaluation, &Invoked<'_, Id>, Outcome<'_, '_>) -> R,
        ) -> R {
            self.entered_under(bytes, |invocation, evaluation, invoked| {
                let outcome =
                    outcome(&evaluation, &invoked, identity, invocation, &Sha256).unwrap();
                inspect(&evaluation, &invoked, outcome)
            })
        }

        /// The same inputs with each of `capability` at its limit among the
        /// capability's `policyComparable` bounds, and the selected policy's
        /// bounds each of `policy` at its limit, which a test keeps
        /// compatible with the capability.
        pub(crate) fn bounding(
            &self,
            capability: &[(BoundId, u64)],
            policy: &[(BoundId, u64)],
        ) -> Self {
            let mut inputs = Self {
                config: self.config.clone(),
                selected: self.selected,
                unnamed: self.unnamed,
                members: self.members.clone(),
                specifications: self.specifications.clone(),
                surfaces: self.surfaces,
                capability: tightened(&self.capability, capability),
                pinned: self.pinned.clone(),
                metadata: self.metadata,
            };
            inputs.select_policy(resource_limit_policy(&tightened(&policy_limits(), policy)));
            inputs
        }

        /// The invocation of `entered`, for `inspect` also with the
        /// invocation's other inputs.
        pub(crate) fn entered_under<R>(
            &self,
            bytes: Option<Arc<[u8]>>,
            inspect: impl FnOnce(&Invocation<'_, Id>, Evaluation, Invoked<'_, Id>) -> R,
        ) -> R {
            let (config, limits) = entry_input(&self.config, &self.capability);
            let fixture = FileFixture::new(&self.capability);
            let file = fixture.file(bytes.unwrap_or(config));
            let selector = self.selector();
            self.invoked(|invocation| {
                let mut evaluation = Evaluation::default();
                let mut set = None;
                let invoked = invoke_file(
                    &mut evaluation,
                    (&file, limits),
                    (
                        &Schema::for_model().unwrap(),
                        self.capability.configuration_input.structural,
                        &selector,
                    ),
                    invocation,
                    &FixtureProvider::new(),
                    (&mut set, &mut None),
                )
                .unwrap();
                inspect(invocation, evaluation, invoked)
            })
        }

        /// The invocation of the Programmatic Entry Snapshot of the
        /// configuration's value, beside `call_site` when given, for
        /// `inspect` with the invocation's other inputs, what it recorded, and
        /// what it left: the boundary admits the Snapshot and a pair whose
        /// revision is its digest, and Phase 1 reads it under the capability's
        /// root-value bounds, as it reads the file (D15:743, 3228).
        pub(crate) fn programmatic_under<R>(
            &self,
            call_site: Option<&AdmittedCallSite>,
            inspect: impl FnOnce(&Invocation<'_, Id>, Evaluation, Invoked<'_, Id>) -> R,
        ) -> R {
            let root = from_json(&self.config);
            let room = bound(1_000_000);
            let bounds = BootstrapLimits::from_bounds(|_| Some(room))
                .unwrap()
                .snapshot;
            let report = SnapshotReport::Formal {
                root: submitted(&root),
                digest: root.digest(),
            };
            let snapshot = read_report(Some(&report), &bounds).unwrap();
            let pair_limits = SourceLimits {
                identity_bytes: room,
                revision_bytes: room,
            };
            let digest = snapshot.digest();
            let pair = read_entry_pair(Some(ENTRY), Some(digest), digest, pair_limits).unwrap();
            let classify = classify(&self.capability);
            let entry = EntrySnapshot {
                snapshot: &snapshot,
                source: Source {
                    identity: pair.identity(),
                    marked: true,
                    locator: None,
                },
                call_site,
                classify: &*classify,
            };
            let limits = SnapshotEntryLimits {
                snapshot: bounds,
                value: self.capability.configuration_input.value,
            };
            let selector = self.selector();
            self.invoked(|invocation| {
                let mut evaluation = Evaluation::default();
                let mut set = None;
                let invoked = invoke_programmatic(
                    &mut evaluation,
                    (&entry, limits),
                    (
                        &Schema::for_model().unwrap(),
                        self.capability.configuration_input.structural,
                        &selector,
                    ),
                    invocation,
                    &FixtureProvider::new(),
                    (&mut set, &mut None),
                )
                .unwrap();
                inspect(invocation, evaluation, invoked)
            })
        }

        /// The outcome of `programmatic_under`'s invocation under `identity`,
        /// for `inspect` with what the invocation recorded and what it left.
        pub(crate) fn programmatic_outcome<R>(
            &self,
            call_site: Option<&AdmittedCallSite>,
            identity: ConstructionIdentity,
            inspect: impl FnOnce(&Evaluation, &Invoked<'_, Id>, Outcome<'_, '_>) -> R,
        ) -> R {
            self.programmatic_under(call_site, |invocation, evaluation, invoked| {
                let outcome =
                    outcome(&evaluation, &invoked, identity, invocation, &Sha256).unwrap();
                inspect(&evaluation, &invoked, outcome)
            })
        }

        /// A sidecar whose source is the configuration's file and whose
        /// mappings give each value and key its token's span there.
        pub(crate) fn mirror(&self) -> AdmittedCallSite {
            let text = serde_json::to_string(&self.config).unwrap();
            sidecar(&text, None, mirrored(&text))
        }

        /// Selects `policy`, submitted in place of the selected one wherever
        /// the collection holds it, and pins it.
        fn select_policy(&mut self, policy: Value) {
            let index = self
                .members
                .iter()
                .position(|member| tuple_of(member) == self.pinned)
                .expect("the selected Policy is submitted");
            self.pinned = tuple_of(&policy);
            self.declaration()["policies"]["resourceLimits"] = reference_value(&self.pinned);
            self.members[index] = policy;
        }

        /// The same inputs with every set, object, and the collection in
        /// another order.
        pub(crate) fn reordered(&self) -> Self {
            let mut config = self.config.clone();
            let web = &mut config["profiles"][self.selected];
            for path in [
                "/requestedLocales",
                "/targetProfiles/browser/requestedLocales",
            ] {
                web.pointer_mut(path)
                    .unwrap()
                    .as_array_mut()
                    .unwrap()
                    .reverse();
            }
            reverse_objects(&mut config);
            let mut members = self.members.clone();
            members.reverse();
            Self {
                config,
                selected: self.selected,
                unnamed: self.unnamed,
                members,
                specifications: self.specifications.clone(),
                surfaces: self.surfaces,
                capability: self.capability.clone(),
                pinned: self.pinned.clone(),
                metadata: self.metadata,
            }
        }

        /// The same inputs with every schema-declared unordered collection
        /// in another order, whatever its members: each set, every object's
        /// members, and the artifact collection (D15:3353).
        pub(crate) fn permuted(&self) -> Self {
            let mut config = self.config.clone();
            reverse_sets(&mut config);
            reverse_objects(&mut config);
            let mut members = self.members.clone();
            members.reverse();
            Self {
                config,
                selected: self.selected,
                unnamed: self.unnamed,
                members,
                specifications: self.specifications.clone(),
                surfaces: self.surfaces,
                capability: self.capability.clone(),
                pinned: self.pinned.clone(),
                metadata: self.metadata,
            }
        }

        /// The selector that names the declaration `selected`, or none.
        fn selector(&self) -> SelectorInput {
            let bound = self
                .capability
                .configuration_input
                .structural
                .max_profile_id_bytes;
            if self.unnamed {
                SelectorInput::absent(bound)
            } else {
                SelectorInput::string(self.selected, bound)
            }
        }

        /// Phases 1 to 3 over the configuration outside any evaluation,
        /// selecting the declaration, and the invocation's other inputs, for
        /// `run`.
        fn prepared<R>(
            &self,
            run: impl FnOnce(&SelectedConfiguration<'_>, &Invocation<'_, Id>) -> R,
        ) -> R {
            let analysis = analyze(&self.config, &self.capability);
            assert!(analysis.is_complete());
            let (bytes, _) = entry_input(&self.config, &self.capability);
            let fixture = FileFixture::new(&self.capability);
            let file = fixture.file(bytes);
            let selector = self.selector();
            let Ok(Selection::Selected(selection)) = analysis.select(&selector) else {
                panic!("{} is not selected", self.selected);
            };
            let configuration = SelectedConfiguration::new(
                &ValueAdmissions::default(),
                (&analysis, &file),
                (&selector, &selection),
                Ok(()),
            )
            .unwrap();
            self.invoked(|invocation| run(&configuration, invocation))
        }

        /// The invocation's inputs besides the configuration, for `run`.
        fn invoked<R>(&self, run: impl FnOnce(&Invocation<'_, Id>) -> R) -> R {
            let supported = SupportedSpecifications::revision_zero(
                ActiveProfileSpecification::fixture(ACTIVE),
                Supported::new(vec![fixture_pins()]).unwrap(),
            );
            let capability = ImplementationCapability::fixture_owned(self.capability.clone())
                .with_evidence_metadata(self.metadata);
            let collection = collection(&self.members);
            let vocabulary = formal_of(self.surfaces);
            let verification = VerificationInput::fixture_owned([self.pinned.clone()]);
            run(&Invocation {
                supported: &supported,
                specifications: &self.specifications,
                capability: &capability,
                verification: &verification,
                collection: &collection,
                vocabulary: &vocabulary,
            })
        }

        /// The entry over the prepared inputs; `inspect` reads what the
        /// invocation recorded and what it returned.
        fn attempt<R>(
            &self,
            inspect: impl FnOnce(
                Evaluation,
                Result<Option<SemanticState<'_, Id>>, ResolverInvariant>,
            ) -> R,
        ) -> R {
            self.attempt_in(Evaluation::default(), inspect)
        }

        /// The entry over the prepared inputs, recording into `evaluation`.
        /// Every unit it leaves not evaluated waits on each of its causes
        /// through its Check Registry row's prerequisites.
        fn attempt_in<R>(
            &self,
            mut evaluation: Evaluation,
            inspect: impl FnOnce(
                Evaluation,
                Result<Option<SemanticState<'_, Id>>, ResolverInvariant>,
            ) -> R,
        ) -> R {
            let (bytes, limits) = entry_input(&self.config, &self.capability);
            let fixture = FileFixture::new(&self.capability);
            let file = fixture.file(bytes);
            let selector = self.selector();
            self.invoked(|invocation| {
                let mut set = None;
                let state = invoke_file(
                    &mut evaluation,
                    (&file, limits),
                    (
                        &Schema::for_model().unwrap(),
                        self.capability.configuration_input.structural,
                        &selector,
                    ),
                    invocation,
                    &FixtureProvider::new(),
                    (&mut set, &mut None),
                )
                .map(|invoked| invoked.state.ok());
                let registry = CheckRegistry::revision_zero();
                let requirements = requirements(&registry);
                for (unit, status) in evaluation.statuses() {
                    if let Status::NotEvaluated(causes) = status {
                        for cause in causes.keys() {
                            assert!(
                                requirements[row_of(unit)].contains(row_of(cause.unit())),
                                "{unit:?} waits on {:?}",
                                cause.unit()
                            );
                        }
                    }
                }
                inspect(evaluation, state)
            })
        }

        pub(crate) fn resolve<R>(
            &self,
            inspect: impl FnOnce(Evaluation, SemanticState<'_, Id>) -> R,
        ) -> R {
            self.attempt(|evaluation, state| {
                let state = state.unwrap().expect("entry admits the file");
                inspect(evaluation, state)
            })
        }

        /// Phase 14 over the entry's semantic state, for `inspect` with what
        /// the invocation recorded.
        pub(crate) fn viewed<R>(
            &self,
            inspect: impl FnOnce(Evaluation, Admitted<ProfileView>) -> R,
        ) -> R {
            let (bytes, limits) = entry_input(&self.config, &self.capability);
            let fixture = FileFixture::new(&self.capability);
            let file = fixture.file(bytes);
            let selector = self.selector();
            self.invoked(|invocation| {
                let mut evaluation = Evaluation::default();
                let mut set = None;
                let state = invoke_file(
                    &mut evaluation,
                    (&file, limits),
                    (
                        &Schema::for_model().unwrap(),
                        self.capability.configuration_input.structural,
                        &selector,
                    ),
                    invocation,
                    &FixtureProvider::new(),
                    (&mut set, &mut None),
                )
                .unwrap()
                .state
                .expect("entry admits the file");
                let view = project(&state, invocation.supported);
                inspect(evaluation, view)
            })
        }

        /// Phase 14 over the entry's semantic state: the profile view, or
        /// the causes of the semantic results it needs and does not have.
        pub(crate) fn view(&self) -> Admitted<ProfileView> {
            self.viewed(|_, view| view)
        }

        /// The specification input, which a test edits before resolving.
        pub(crate) fn specifications(&mut self) -> &mut SpecificationInput<Id> {
            &mut self.specifications
        }

        /// Phase 14 over the semantic state of an invocation on a resolver
        /// supporting `supported`, beside the vocabulary artifact
        /// `vocabulary` and the canonicalization data `data` in place of the
        /// fixture's.
        pub(crate) fn view_under<P, D>(
            &self,
            supported: &SupportedSpecifications<Id>,
            vocabulary: &FormalVocabulary,
            data: &D,
        ) -> Admitted<ProfileView>
        where
            P: Provider<Identity = Id>,
            D: FormalData<Provider = P>,
        {
            self.prepared(|configuration, invocation| {
                let invocation = Invocation {
                    supported,
                    vocabulary,
                    ..*invocation
                };
                let mut evaluation = Evaluation::default();
                let mut set = None;
                let state =
                    invoke(&mut evaluation, configuration, &invocation, data, &mut set).unwrap();
                project(&state, supported)
            })
        }

        /// Seals again, after `edit` to its body, the artifact the reference
        /// at `pointer` in the declaration names, submits it in its place, and
        /// makes the reference name it; the Verification Input pins it when it
        /// is the selected policy.
        pub(crate) fn reseal(&mut self, pointer: &str, edit: impl FnOnce(&mut Value)) {
            let reference = self.declaration().pointer(pointer).unwrap().clone();
            let index = self
                .members
                .iter()
                .position(|member| reference_value(&tuple_of(member)) == reference)
                .expect("the reference names a submitted artifact");
            let earlier = tuple_of(&self.members[index]);
            let mut body = self.members[index]["body"].clone();
            edit(&mut body);
            let content = body["content"].take();
            let member = envelope_of(
                earlier.kind,
                body["identity"].as_str().unwrap(),
                body["revision"].as_str().unwrap(),
                content,
            );
            let tuple = tuple_of(&member);
            if earlier == self.pinned {
                self.pinned = tuple.clone();
            }
            *self.declaration().pointer_mut(pointer).unwrap() = reference_value(&tuple);
            self.members[index] = member;
        }
    }

    /// One invocation's entry over `evaluation` past a configuration prepared
    /// outside it: Phase 0, which runs before configuration entry, the
    /// `profileSpecification` group's owner that Phase 2 records, and then
    /// the semantic phases over `configuration`.
    fn invoke<'s, P, D>(
        evaluation: &mut Evaluation,
        configuration: &SelectedConfiguration<'_>,
        invocation: &Invocation<'s, P::Identity>,
        data: &D,
        set: &'s mut Option<Admitted<ArtifactSet<'s>>>,
    ) -> Result<SemanticState<'s, P::Identity>, ResolverInvariant>
    where
        P: Provider,
        P::Identity: AsRef<str>,
        D: FormalData<Provider = P>,
    {
        let project_profile =
            admit_project_profile(evaluation, invocation.supported, invocation.specifications)?;
        own(
            evaluation,
            SemanticGroup::ProfileSpecification,
            held(project_profile.as_ref()),
        )?;
        resolve_profile(
            evaluation,
            &project_profile,
            configuration,
            invocation,
            data,
            set,
        )
    }

    /// The check ID of the Check Registry row a unit belongs to.
    fn row_of(unit: &Unit) -> &'static str {
        check_id(unit.subject.kind(), unit.check)
    }

    /// Every blocking result, as its unit, code, and reason.
    fn issues(evaluation: &Evaluation) -> Vec<(Unit, Code, Option<Reason>)> {
        evaluation
            .issues()
            .map(|key| (key.unit().clone(), key.code(), key.reason()))
            .collect()
    }

    /// The only blocking result.
    fn only_issue(evaluation: &Evaluation) -> IssueKey {
        let mut keys = evaluation.issues();
        let key = keys.next().unwrap().clone();
        assert_eq!(keys.next(), None);
        key
    }

    /// The phases with a unit that was not evaluated, and every key those
    /// units wait on.
    fn suppressed(evaluation: &Evaluation) -> (BTreeSet<Phase>, BTreeSet<IssueKey>) {
        let mut phases = BTreeSet::new();
        let mut keys = BTreeSet::new();
        for (unit, status) in evaluation.statuses() {
            if let Status::NotEvaluated(causes) = status {
                phases.insert(unit.phase);
                keys.extend(causes.keys().cloned());
            }
        }
        (phases, keys)
    }

    fn keys(causes: &Causes) -> BTreeSet<IssueKey> {
        causes.keys().cloned().collect()
    }

    /// The names of the state's values that are absent, in the state's
    /// order. `locales` stands for the project's locales.
    fn absent(state: &SemanticState<'_, Id>) -> Vec<&'static str> {
        [
            ("policy", state.policy.confirmed.is_err()),
            ("specifications", state.specifications.is_err()),
            ("canonicalization", state.canonicalization.is_err()),
            ("vocabulary", state.vocabulary.is_err()),
            ("identity", state.identity.is_err()),
            ("locales", state.locales.project.is_err()),
            ("negotiation", state.negotiation.is_err()),
            ("fallback", state.fallback.is_err()),
            ("coverage", state.coverage.is_err()),
            ("references", state.references.is_err()),
            ("bindings", state.bindings.is_err()),
            ("delivery", state.delivery.is_err()),
            (
                "targets",
                state
                    .targets
                    .as_ref()
                    .map_or(true, |targets| targets.complete().is_err()),
            ),
            (
                "groups",
                state
                    .groups
                    .as_ref()
                    .map_or(true, |groups| groups.partition().is_err()),
            ),
            (
                "hydration",
                state
                    .hydration
                    .as_ref()
                    .map_or(true, |hydration| hydration.complete().is_err()),
            ),
        ]
        .into_iter()
        .filter_map(|(name, absent)| absent.then_some(name))
        .collect()
    }

    /// Every name `absent` returns when no policy was confirmed.
    const EVERY: [&str; 15] = [
        "policy",
        "specifications",
        "canonicalization",
        "vocabulary",
        "identity",
        "locales",
        "negotiation",
        "fallback",
        "coverage",
        "references",
        "bindings",
        "delivery",
        "targets",
        "groups",
        "hydration",
    ];

    /// Whether every unit of Phases 5 to 13 waits on `key` alone.
    fn later_phases_wait_on(evaluation: &Evaluation, key: &IssueKey) -> bool {
        evaluation
            .statuses()
            .filter(|(unit, _)| unit.phase > Phase::ResourcePolicyAdmission)
            .all(|(_, status)| {
                matches!(status, Status::NotEvaluated(causes) if keys(causes) == BTreeSet::from([key.clone()]))
            })
    }

    /// The semantic groups whose owner waits, in the inventory's order: the
    /// groups the invocation did not establish. Every group has its owner.
    fn unestablished(evaluation: &Evaluation) -> Vec<SemanticGroup> {
        SemanticGroup::ALL
            .into_iter()
            .filter(|&group| match evaluation.status(&dependency::unit(group)) {
                Some(Status::Evaluated) => false,
                Some(Status::NotEvaluated(_)) => true,
                None => panic!("{group:?} has no owner"),
            })
            .collect()
    }

    /// Every group but `profileSpecification`, which Phase 0 alone
    /// establishes: what an unconfirmed policy leaves unestablished.
    const AFTER_POLICY: [SemanticGroup; 11] = [
        SemanticGroup::ProjectIdentity,
        SemanticGroup::LocaleCanonicalization,
        SemanticGroup::ProjectLocales,
        SemanticGroup::LocaleNegotiation,
        SemanticGroup::MessageFallback,
        SemanticGroup::Coverage,
        SemanticGroup::RequiredPolicies,
        SemanticGroup::OptionalPolicies,
        SemanticGroup::Targets,
        SemanticGroup::DeploymentGroups,
        SemanticGroup::Delivery,
    ];

    fn spellings(locales: &[CanonicalLocale]) -> Vec<&str> {
        locales.iter().map(CanonicalLocale::as_str).collect()
    }

    /// The state with Phase 7's occurrences, which carry their Entry
    /// locations, replaced by their subjects in order.
    /// What `evaluation` records apart from where in the file a structural
    /// candidate looked, which follows the file's order: each unit's status,
    /// each candidate's under its logical unit, and every Finding, notice,
    /// and occurrence candidate.
    fn unplaced(evaluation: &Evaluation) -> impl PartialEq + std::fmt::Debug + '_ {
        let mut candidates: Vec<(Unit, Option<Vec<IssueKey>>)> = evaluation
            .candidates()
            .map(|(unit, status)| {
                let causes = match status {
                    Status::Evaluated => None,
                    Status::NotEvaluated(causes) => Some(causes.keys().cloned().collect()),
                };
                (unit.logical(), causes)
            })
            .collect();
        candidates.sort();
        // Where a candidate was observed is presentation Evidence, which
        // reordering a set moves (D15:2389): only what each one found stays.
        let mut observed: Vec<&IssueKey> = evaluation
            .occurrence_candidates()
            .iter()
            .map(|candidate| &candidate.finding)
            .collect();
        observed.sort();
        (
            statuses(evaluation),
            candidates,
            evaluation.findings(),
            observed,
        )
    }

    fn unlocated(
        mut state: SemanticState<'_, Id>,
    ) -> (Vec<(LocaleSubject, bool)>, SemanticState<'_, Id>) {
        let mut subjects: Vec<_> = state
            .locales
            .occurrences
            .drain(..)
            .map(|occurrence| (occurrence.subject, occurrence.corrected))
            .collect();
        subjects.sort();
        (subjects, state)
    }

    /// A defect Phase 1's evaluation finds is the evaluation's, as in every
    /// other phase, and a defect of strict file entry is the entry's own.
    #[test]
    fn entry_keeps_evaluation_defects_apart_from_its_own() {
        let inputs = Inputs::complete();
        let (bytes, limits) = entry_input(&inputs.config, &inputs.capability);
        let fixture = FileFixture::new(&inputs.capability);
        let file = fixture.file(bytes);
        let mut evaluation = Evaluation::default();
        assert!(admit_entry(&mut evaluation, &file, limits, &unbounded())
            .unwrap()
            .is_ok());
        let Err(twice) = admit_entry(&mut evaluation, &file, limits, &unbounded()) else {
            panic!("Phase 1 records its units once");
        };
        assert_eq!(
            ResolverInvariant::from(twice),
            ResolverInvariant::Evaluation(EvaluationInvariant::UnitRecordedTwice(OWNER))
        );
        assert_eq!(
            ResolverInvariant::from(EntryInvariant::UnreportedFailure),
            ResolverInvariant::Entry(EntryInvariant::UnreportedFailure)
        );
    }

    /// When Phase 1 admits no value, the Shared projection is unavailable for
    /// Phase 1's blocking Findings alone: Phase 0's explain no missing value
    /// (D15:1165, 2369).
    #[test]
    fn the_shared_projection_is_unavailable_for_phase_1s_findings_alone() {
        let mut inputs = Inputs::complete();
        inputs.specifications.project_profile = None;
        let (_, limits) = entry_input(&inputs.config, &inputs.capability);
        let fixture = FileFixture::new(&inputs.capability);
        let file = fixture.file(Arc::from(&br#"{"schemaVersion":"#[..]));
        inputs.prepared(|_, invocation| {
            let mut evaluation = Evaluation::default();
            admit_project_profile(
                &mut evaluation,
                invocation.supported,
                invocation.specifications,
            )
            .unwrap()
            .unwrap_err();
            let Ok(Err(causes)) = admit_entry(&mut evaluation, &file, limits, &unbounded()) else {
                panic!("the file is not strict JSON");
            };
            assert_eq!(
                causes.keys().map(IssueKey::unit).collect::<Vec<_>>(),
                [&input(Check::WellFormedJson)]
            );
            assert_eq!(evaluation.issues().count(), 2);
        });
    }

    /// When strict file entry admits no value there is no Shared projection,
    /// so no unit after Phase 1 is recorded, while Phase 0 and Phase 1 keep
    /// their statuses and Findings (D15:2145, 2369).
    #[test]
    fn without_an_entered_value_no_unit_after_entry_is_recorded() {
        let inputs = Inputs::complete();
        let (_, limits) = entry_input(&inputs.config, &inputs.capability);
        let fixture = FileFixture::new(&inputs.capability);
        let file = fixture.file(Arc::from(&br#"{"schemaVersion":"#[..]));
        let selector = inputs.selector();
        inputs.invoked(|invocation| {
            let mut evaluation = Evaluation::default();
            let mut set = None;
            let invoked = invoke_file(
                &mut evaluation,
                (&file, limits),
                (
                    &Schema::for_model().unwrap(),
                    inputs.capability.configuration_input.structural,
                    &selector,
                ),
                invocation,
                &FixtureProvider::new(),
                (&mut set, &mut None),
            );
            assert!(matches!(invoked, Ok(Invoked { state: Err(_), .. })));
            let phases: BTreeSet<Phase> =
                evaluation.statuses().map(|(unit, _)| unit.phase).collect();
            assert_eq!(
                phases,
                BTreeSet::from([Phase::ResolverBootstrapAdmission, Phase::EntryAdmission])
            );
            let codes: Vec<Code> = evaluation.issues().map(IssueKey::code).collect();
            assert_eq!(codes, [Code::JsonInvalid]);
        });
    }

    /// The views of the `N` entry dependency records `invoked` names, their
    /// redacted components' tokens assigned on their own: nothing else in
    /// these invocations collides with them under this digest.
    fn entry_views<const N: usize>(invoked: &Invoked<'_, Id>) -> [Value; N] {
        let specification = Specification {
            identity: "intlify-project-profile",
            revision: "0",
        };
        let tokens = EvidenceTokens::assign(specification, invoked.entry.redacted(), &Sha256);
        let records = invoked.entry.retained(specification, &tokens).unwrap();
        let views: Vec<Value> = records.iter().map(EntryDependency::view).collect();
        views.try_into().unwrap()
    }

    /// The selector the entry dependencies name is projected under the final
    /// projection authority beside the declared Profile IDs: the selected
    /// declaration's ID is a safe Profile ID; past the policy's bound it is
    /// over the limit and redacted; and without a value the capability
    /// measures it, with no declared Profile ID to name. The entry source
    /// names the Source-content revision of the file's bytes (D15:743, 807,
    /// 816-819, 2516).
    #[test]
    fn the_entry_dependencies_name_the_final_selector_projection() {
        let complete = Inputs::complete();
        let (bytes, _) = entry_input(&complete.config, &complete.capability);
        complete.entered(None, |_, invoked| {
            assert!(invoked.state.is_ok());
            let [source, selector, _] = entry_views(&invoked);
            assert_eq!(selector["selector"], json!({ "safe-profile-id": "web" }));
            assert_eq!(source["revision"], json!(file_source_revision(&bytes)));
        });
        let mut limited = Inputs::complete();
        let mut limits = policy_limits();
        limits.configuration_input.structural.max_profile_id_bytes = bound(2);
        limited.select_policy(resource_limit_policy(&limits));
        limited.entered(None, |_, invoked| {
            let state = invoked.state.as_ref().unwrap();
            assert_eq!(state.policy.authority, bound(2));
            let [_, selector, _] = entry_views(&invoked);
            let redacted = &selector["selector"]["redacted-value"];
            assert_eq!(redacted["type"], json!("string"));
            assert_eq!(redacted["length"], json!("over-limit"));
        });
        let broken: Arc<[u8]> = Arc::from(&br#"{"schemaVersion":"#[..]);
        complete.entered(Some(Arc::clone(&broken)), |_, invoked| {
            let causes = invoked.state.as_ref().unwrap_err();
            assert_eq!(
                causes.keys().map(IssueKey::unit).collect::<Vec<_>>(),
                [&input(Check::WellFormedJson)]
            );
            let [source, selector, _] = entry_views(&invoked);
            let redacted = &selector["selector"]["redacted-value"];
            assert_eq!(redacted["length"], json!({ "exact": "3" }));
            assert_eq!(source["revision"], json!(file_source_revision(&broken)));
        });
    }

    /// When the schema selection selects no schema, Phase 2 records its
    /// owner, the selection's checks, the analysis's measure waiting on the
    /// failed check, and the `profileSpecification` group's owner, and Phase
    /// 3 its count of the declarations and the selector's checks that compare
    /// it with them, waiting on the same check, and no Profile ID. Every later
    /// phase records its checks waiting on the same check, but Phase 4's
    /// checks of the collection under the capability, which read no
    /// declaration (D15:2130, 2131, 2132, 2151).
    #[test]
    fn without_a_selected_schema_each_later_check_waits_on_the_version() {
        let mut inputs = Inputs::complete();
        inputs.config["schemaVersion"] = json!("1");
        inputs.attempt(|evaluation, state| {
            let state = state.unwrap().expect("entry admits the file");
            assert!(state.structure.is_err() && state.policy.confirmed.is_err());
            let phases: BTreeSet<Phase> =
                evaluation.statuses().map(|(unit, _)| unit.phase).collect();
            let mut ran = BTreeSet::from(PHASES);
            ran.extend([
                Phase::EntryAdmission,
                Phase::StructuralAdmission,
                Phase::ProfileSelection,
            ]);
            assert_eq!(phases, ran);
            let version = selection(Check::SupportedSchemaVersion);
            assert_eq!(
                issues(&evaluation),
                [(version.clone(), Code::SchemaVersionUnsupported, None)]
            );
            let waiting: Vec<&Unit> = evaluation
                .statuses()
                .filter(|(unit, status)| {
                    unit.phase <= Phase::ProfileSelection
                        && matches!(status, Status::NotEvaluated(_))
                })
                .map(|(unit, _)| unit)
                .collect();
            assert_eq!(
                waiting,
                [
                    &CAPABILITY,
                    &capability(SelectionSubject::Profiles),
                    &selector(Check::RequiredSelector),
                    &selector(Check::DeclaredSelector),
                ]
            );
            assert_eq!(
                evaluated_after_selection(&evaluation),
                BTreeSet::from(COLLECTION_UNDER_CAPABILITY)
            );
            // Without an analysis no Profile ID names a recheck.
            assert_eq!(rechecked_ids(&evaluation), []);
            for (unit, status) in evaluation.statuses() {
                if let Status::NotEvaluated(causes) = status {
                    let units: Vec<&Unit> = causes.keys().map(IssueKey::unit).collect();
                    assert_eq!(units, [&version], "{unit:?}");
                }
            }
        });
    }

    /// Without a selected declaration each later check that reads it, or the
    /// Policy its reference selects, waits on what held the selection back,
    /// and Phase 4 still checks the collection under the capability: here the
    /// selector names no declaration, while structural admission admitted the
    /// whole root (D15:2131, 2132).
    #[test]
    fn without_a_selection_each_later_check_waits_on_its_causes() {
        let mut inputs = Inputs::complete();
        inputs.selected = "absent";
        inputs.attempt(|evaluation, state| {
            let state = state.unwrap().expect("entry admits the file");
            assert!(state.structure.is_ok() && state.policy.confirmed.is_err());
            let declared = selector(Check::DeclaredSelector);
            assert_eq!(
                issues(&evaluation),
                [(declared.clone(), Code::SelectorUnknown, None)]
            );
            assert_eq!(
                evaluated_after_selection(&evaluation),
                BTreeSet::from(COLLECTION_UNDER_CAPABILITY)
            );
            // Each Profile ID structural admission admitted still names its
            // recheck under the Policy, which waits (D15:1171).
            assert_eq!(
                rechecked_ids(&evaluation),
                [PolicySubject::ProfileId(
                    serde_json::from_value(json!("web")).unwrap()
                )]
            );
            let waiting = evaluation
                .statuses()
                .filter(|(unit, _)| unit.phase > Phase::ProfileSelection)
                .filter_map(|(unit, status)| match status {
                    Status::NotEvaluated(causes) => Some((unit, causes)),
                    Status::Evaluated => None,
                });
            for (unit, causes) in waiting {
                let units: Vec<&Unit> = causes.keys().map(IssueKey::unit).collect();
                assert_eq!(units, [&declared], "{unit:?}");
            }
        });
    }

    /// A missing or unknown selection gives no profile, whole or partial:
    /// with two declarations and no selector, or a selector naming neither,
    /// the outcome is blocked on the selector's one Finding, with a complete
    /// bundle whose every later status waits on it, no Policy confirmed, and
    /// no semantic result. A Profile ID a file writes twice is Phase 1's
    /// duplicate member, so no value exists to select from (D15:802, 2129,
    /// 2131, 2295, 2297, 3463).
    #[test]
    fn a_missing_or_unknown_selection_leaves_no_partial_profile() {
        use crate::resolver::outcome::tests::{identity, reporting};
        use crate::resolver::reporting::Record;
        // Named after the Policy is selected, which edits the declaration
        // the selector names.
        let mut unnamed = reporting(&Inputs::complete().selecting("shop"), &[], &[]);
        unnamed.unnamed = true;
        let mut unknown = reporting(&Inputs::complete(), &[], &[]);
        unknown.selected = "absent";
        for (inputs, check, code) in [
            (unnamed, Check::RequiredSelector, Code::SelectorRequired),
            (unknown, Check::DeclaredSelector, Code::SelectorUnknown),
        ] {
            let held = selector(check);
            inputs.outcome(None, identity(), |evaluation, invoked, outcome| {
                assert_eq!(issues(evaluation), [(held.clone(), code, None)]);
                let state = invoked.state.as_ref().unwrap();
                assert_eq!(absent(state), EVERY);
                assert!(outcome.checked.is_none());
                let evidence = &outcome.evidence;
                assert!(evidence.complete() && evidence.unavailable().is_empty());
                let shared = evidence.shared.as_ref().unwrap();
                let mut waiting = 0;
                for (owner, retained) in &shared.records {
                    if let Record::Status(Status::NotEvaluated(causes)) = retained.record {
                        let units: Vec<&Unit> = causes.keys().map(IssueKey::unit).collect();
                        assert_eq!(units, [&held], "{owner:?}");
                        waiting += 1;
                    }
                }
                assert!(waiting > 0);
            });
        }
        let inputs = Inputs::complete().selecting("shop");
        let text = serde_json::to_string(&inputs.config).unwrap();
        let map = r#""profiles":{"#;
        let at = text.find(map).unwrap() + map.len();
        let twice = format!(r#"{}"shop":{{}},{}"#, &text[..at], &text[at..]);
        reporting(&inputs, &[], &[]).outcome(
            Some(Arc::from(twice.into_bytes())),
            identity(),
            |evaluation, _, outcome| {
                let duplicate = (input(Check::UniqueMembers), Code::JsonMemberDuplicate, None);
                assert_eq!(issues(evaluation), [duplicate]);
                assert!(evaluation
                    .statuses()
                    .all(|(unit, _)| unit.phase <= Phase::EntryAdmission));
                assert!(outcome.checked.is_none() && outcome.evidence.shared.is_none());
            },
        );
    }

    /// Without a `profiles` object no declaration is counted or selected:
    /// each later check waits on its Finding, and no Profile ID names a
    /// recheck (D15:2131, 2184).
    #[test]
    fn without_a_profiles_object_each_later_check_waits_on_its_finding() {
        let mut inputs = Inputs::complete();
        inputs.config.as_object_mut().unwrap().remove("profiles");
        inputs.attempt(|evaluation, state| {
            let state = state.unwrap().expect("entry admits the file");
            assert!(state.structure.is_err() && state.policy.confirmed.is_err());
            let found = issues(&evaluation);
            let [(missing, Code::RequiredFieldMissing, None)] = found.as_slice() else {
                panic!("{found:?}");
            };
            assert_eq!(
                evaluated_after_selection(&evaluation),
                BTreeSet::from(COLLECTION_UNDER_CAPABILITY)
            );
            assert_eq!(rechecked_ids(&evaluation), []);
            for (unit, status) in evaluation.statuses() {
                if let (true, Status::NotEvaluated(causes)) =
                    (unit.phase > Phase::ProfileSelection, status)
                {
                    let units: Vec<&Unit> = causes.keys().map(IssueKey::unit).collect();
                    assert_eq!(units, [missing], "{unit:?}");
                }
            }
        });
    }

    /// The rows after Phase 3 whose units wait when no declaration is
    /// selected, with how many units of each, in unit order: the units whose
    /// subject needs no declared Profile ID.
    const WAITING_AFTER_SELECTION: [(&str, usize); 83] = [
        ("resource-policy.references.within-capability", 1),
        ("resource-policy.policy-reference.within-capability", 7),
        ("resource-policy.reference-bytes.within-capability", 1),
        ("resource-policy.resource-limits.within-metadata-scan", 1),
        ("resource-policy.resource-limits.bootstrap-candidate", 1),
        ("resource-policy.resource-limits.within-envelope-depth", 1),
        (
            "resource-policy.resource-limits.within-decoded-allocation",
            1,
        ),
        ("resource-policy.resource-limits.within-validation-work", 1),
        ("resource-policy.resource-limits.candidate-body", 1),
        ("resource-policy.resource-limits.compatible", 1),
        ("resource-policy.root-nodes.within-bound", 1),
        ("resource-policy.root-depth.within-bound", 1),
        ("resource-policy.root-collection-entries.within-bound", 1),
        ("resource-policy.root-string-bytes.within-bound", 1),
        ("resource-policy.root-longest-string.within-bound", 1),
        ("resource-policy.profiles.within-bound", 1),
        ("resource-policy.selector.within-bound", 1),
        ("resource-policy.structural-analysis.within-bound", 1),
        ("resource-policy.artifacts.within-bound", 1),
        ("resource-policy.policy-artifacts.within-bound", 1),
        ("resource-policy.target-profile-artifacts.within-bound", 1),
        ("resource-policy.references.within-bound", 1),
        ("resource-policy.artifact.within-bound", 9),
        ("resource-policy.artifact-bytes.within-bound", 1),
        ("resource-policy.policy-reference.within-bound", 7),
        ("resource-policy.reference-bytes.within-bound", 1),
        ("specification.locale-canonicalization.supported", 1),
        ("specification.locale-negotiation.supported", 1),
        ("specification.message-locale-fallback.supported", 1),
        ("specification.coverage.supported", 1),
        ("specification.intent-surface-class-vocabulary.supported", 1),
        ("specification.delivery-graph.supported", 1),
        ("specification.delivery-placement.supported", 1),
        ("specification.vocabulary.intact", 1),
        ("specification.vocabulary.asserted", 1),
        ("specification.locale-data.within-payloads", 1),
        ("specification.locale-data.within-payload-bytes", 1),
        ("specification.locale-data.manifest-intact", 1),
        ("specification.locale-data.markers-readable", 1),
        ("specification.locale-data.within-marker-bytes", 1),
        ("specification.locale-data.conformance-readable", 1),
        ("specification.locale-data.within-validity-groups", 1),
        ("specification.locale-data.within-codes", 1),
        ("specification.locale-data.within-extension-keys", 1),
        ("specification.locale-data.within-types", 1),
        ("specification.locale-data.within-corrections", 1),
        ("specification.locale-data.within-validation-work", 1),
        ("specification.locale-data.dataset-intact", 1),
        ("specification.locale-data.matching", 1),
        ("dependency.locale-canonicalization.established", 1),
        ("identity.declaration-field.opaque", 2),
        ("dependency.project-identity.established", 1),
        ("locale.occurrences.within-bound", 1),
        ("locale.requested-set.within-bound", 1),
        ("locale.default-source.canonical", 1),
        ("locale.default-requested.canonical", 1),
        ("locale.default-requested.member", 1),
        ("locale.target-profiles.within-bound", 1),
        ("dependency.project-locales.established", 1),
        ("negotiation.aliases.within-bound", 1),
        ("fallback.sources.within-bound", 1),
        ("coverage.rules.within-bound", 1),
        ("coverage.locale-selectors.within-bound", 1),
        ("coverage.surface-selectors.within-bound", 1),
        ("coverage.cells.within-bound", 1),
        ("coverage.comparisons.within-bound", 1),
        ("coverage.table.complete", 1),
        ("dependency.locale-negotiation.established", 1),
        ("dependency.message-fallback.established", 1),
        ("dependency.coverage.established", 1),
        ("artifact.member.admissible", 9),
        ("artifact.member.single-occurrence", 9),
        ("artifact.conflict-group.consistent-content", 9),
        ("artifact.policy-reference.resolved", 7),
        ("dependency.required-policies.established", 1),
        ("dependency.optional-policies.established", 1),
        ("delivery.placement.effective-mode", 1),
        ("dependency.delivery.established", 1),
        ("dependency.targets.established", 1),
        ("group.groups.within-bound", 1),
        ("group.memberships.within-bound", 1),
        ("hydration.relations.within-bound", 1),
        ("dependency.deployment-groups.established", 1),
    ];

    /// That `evaluation` holds one Finding, of `code`, that `waiting` are the
    /// rows through Phase 3 that wait, and that every unit that waits, in any
    /// phase, waits on that Finding alone, while after Phase 3 only Phase 4's
    /// checks of the collection under the capability are evaluated and each
    /// unit of `WAITING_AFTER_SELECTION` waits, beside the recheck of each
    /// Profile ID structural admission admitted.
    fn assert_each_later_check_waits_on_its_finding(
        evaluation: &Evaluation,
        code: Code,
        waiting: &[&str],
    ) {
        let found = only_issue(evaluation);
        assert_eq!(found.code(), code);
        let through_selection: Vec<&str> = evaluation
            .statuses()
            .filter(|(unit, status)| {
                unit.phase <= Phase::ProfileSelection && matches!(status, Status::NotEvaluated(_))
            })
            .map(|(unit, _)| row_of(unit))
            .collect();
        assert_eq!(through_selection, waiting);
        assert_eq!(
            evaluated_after_selection(evaluation),
            BTreeSet::from(COLLECTION_UNDER_CAPABILITY)
        );
        let mut after_selection: BTreeMap<&str, usize> = BTreeMap::new();
        for (unit, status) in evaluation.statuses() {
            if let Status::NotEvaluated(causes) = status {
                assert_eq!(keys(causes), BTreeSet::from([found.clone()]), "{unit:?}");
                if unit.phase > Phase::ProfileSelection {
                    *after_selection.entry(row_of(unit)).or_default() += 1;
                }
            }
        }
        let mut expected = BTreeMap::from(WAITING_AFTER_SELECTION);
        let ids = rechecked_ids(evaluation).len();
        if ids > 0 {
            expected.insert("resource-policy.profile-id.within-bound", ids);
        }
        assert_eq!(after_selection, expected);
    }

    /// The rows of the schema selection's four checks, in rank order.
    const SCHEMA_SELECTION: [&str; 4] = [
        "structural.schema-selection.object-root",
        "structural.schema-selection.present-schema-version",
        "structural.schema-selection.string-schema-version",
        "structural.schema-selection.supported-schema-version",
    ];

    /// A root that is no object, and a version missing, of another type, or
    /// unsupported, leave no schema to read the value under: each later
    /// selection or semantic check waits on that one Finding, with no
    /// synthetic Finding of its own, and only the collection is still checked
    /// under the capability (D15:2130, 2151, 3341, 3447).
    #[test]
    fn without_a_schema_every_later_check_waits_on_what_held_it_back() {
        type Edit = fn(&mut Value);
        let cases: [(Edit, usize, Code); 8] = [
            (|root| *root = json!([]), 0, Code::FieldTypeInvalid),
            (|root| *root = json!(42), 0, Code::FieldTypeInvalid),
            (|root| *root = json!("root"), 0, Code::FieldTypeInvalid),
            (|root| *root = json!(true), 0, Code::FieldTypeInvalid),
            (|root| *root = Value::Null, 0, Code::FieldTypeInvalid),
            (
                |root| {
                    root.as_object_mut().unwrap().remove("schemaVersion");
                },
                1,
                Code::RequiredFieldMissing,
            ),
            (
                |root| root["schemaVersion"] = json!(0),
                2,
                Code::FieldTypeInvalid,
            ),
            (
                |root| root["schemaVersion"] = json!("1"),
                3,
                Code::SchemaVersionUnsupported,
            ),
        ];
        for (edit, failed, code) in cases {
            let mut inputs = Inputs::complete();
            edit(&mut inputs.config);
            inputs.attempt(|evaluation, state| {
                let state = state.unwrap().expect("entry admits the file");
                assert!(state.structure.is_err() && state.policy.confirmed.is_err());
                assert_eq!(
                    row_of(only_issue(&evaluation).unit()),
                    SCHEMA_SELECTION[failed]
                );
                let mut waiting = SCHEMA_SELECTION[failed + 1..].to_vec();
                waiting.extend([
                    "structural.analysis.within-capability",
                    "selection.profiles.within-capability",
                    "selection.selector.required",
                    "selection.selector.declared",
                ]);
                assert_each_later_check_waits_on_its_finding(&evaluation, code, &waiting);
                assert_eq!(rechecked_ids(&evaluation), []);
            });
        }
    }

    /// A `profiles` member of another type or with no declaration, and a
    /// selected declaration that is no object, hold the selection back: each
    /// later check waits on that one structural Finding (D15:2131, 2184,
    /// 3341). A declaration that is no object keeps its admitted key, so its
    /// Profile ID still names a recheck, which waits too.
    #[test]
    fn without_a_selectable_declaration_every_later_check_waits_on_its_boundary() {
        type Edit = fn(&mut Value);
        let root_field = "structural.root.field-value";
        let count = "selection.profiles.within-capability";
        let required = "selection.selector.required";
        let declared = "selection.selector.declared";
        let cases: [(Edit, Code, &[&str], bool); 7] = [
            (
                |root| root["profiles"] = json!([]),
                Code::FieldTypeInvalid,
                &[root_field, count, required, declared],
                false,
            ),
            (
                |root| root["profiles"] = json!(42),
                Code::FieldTypeInvalid,
                &[root_field, count, required, declared],
                false,
            ),
            (
                |root| root["profiles"] = json!({}),
                Code::FieldValueInvalid,
                &[required, declared],
                false,
            ),
            (
                |root| root["profiles"]["web"] = json!(42),
                Code::FieldTypeInvalid,
                &["structural.profile.known-field"],
                true,
            ),
            (
                |root| root["profiles"]["web"] = json!([]),
                Code::FieldTypeInvalid,
                &["structural.profile.known-field"],
                true,
            ),
            (
                |root| root["profiles"]["web"] = json!("web"),
                Code::FieldTypeInvalid,
                &["structural.profile.known-field"],
                true,
            ),
            (
                |root| root["profiles"]["web"] = Value::Null,
                Code::FieldTypeInvalid,
                &["structural.profile.known-field"],
                true,
            ),
        ];
        for (edit, code, waiting, named) in cases {
            let mut inputs = Inputs::complete();
            edit(&mut inputs.config);
            inputs.attempt(|evaluation, state| {
                let state = state.unwrap().expect("entry admits the file");
                assert!(state.structure.is_err() && state.policy.confirmed.is_err());
                assert_each_later_check_waits_on_its_finding(&evaluation, code, waiting);
                assert_eq!(rechecked_ids(&evaluation).len(), usize::from(named));
            });
        }
    }

    /// Without the canonicalization slot no locale can be interpreted: every
    /// check that needs a canonical locale, through the locale, policy,
    /// target, and hydration phases, waits on that one Finding, while a phase
    /// that compares only Target IDs still runs (D15:2133, 2151, 2365, 3447).
    #[test]
    fn without_canonicalization_semantics_each_locale_check_waits_on_the_slot() {
        let mut inputs = Inputs::complete();
        inputs.specifications.locale_canonicalization = None;
        inputs.attempt(|evaluation, state| {
            let state = state.unwrap().expect("entry admits the file");
            let found = only_issue(&evaluation);
            assert_eq!(
                (found.code(), found.reason()),
                (
                    Code::SpecificationInputIncomplete,
                    Some(Reason::MissingRequiredSlot)
                )
            );
            assert_eq!(
                suppressed(&evaluation),
                (
                    BTreeSet::from([
                        Phase::SpecificationAdmission,
                        Phase::LocaleResolution,
                        Phase::LocalePolicyResolution,
                        Phase::TargetResolution,
                        Phase::HydrationValidation,
                    ]),
                    BTreeSet::from([found])
                )
            );
            assert_eq!(
                absent(&state),
                [
                    "specifications",
                    "canonicalization",
                    "locales",
                    "negotiation",
                    "fallback",
                    "coverage",
                    "targets",
                    "hydration"
                ]
            );
        });
    }

    /// A missing or empty `deploymentGroups` and a group with no members are
    /// blocking Findings of the fields they name, and a Group ID a file names
    /// twice is Phase 1's duplicate-member Finding, which leaves no value at
    /// all. No group is inferred for the targets: Phase 12 has no partition
    /// and no such group, and the outcome no profile (D15:2438, 3530).
    #[test]
    fn a_missing_or_empty_group_is_a_finding_and_no_group_is_inferred() {
        use crate::resolver::outcome::tests::{identity, reporting};
        use crate::resolver::structural_admission::{EntryKey, Field, StructuralSubject as S};
        type Edit = fn(&mut Value);
        let web = || EntryKey::identity("web");
        let declared = |path: &[&str]| S::ProfileField(web(), Field::of(path));
        let cases: [(Edit, S, Check, Code); 3] = [
            (
                |app| {
                    app.as_object_mut().unwrap().remove("deploymentGroups");
                },
                declared(&["deploymentGroups"]),
                Check::RequiredField,
                Code::RequiredFieldMissing,
            ),
            (
                |app| app["deploymentGroups"] = json!({}),
                declared(&["deploymentGroups"]),
                Check::FieldValue,
                Code::FieldValueInvalid,
            ),
            (
                |app| app["deploymentGroups"]["web"]["members"] = json!([]),
                S::GroupField(web(), web(), Field::of(&["members"])),
                Check::FieldValue,
                Code::FieldValueInvalid,
            ),
        ];
        for (edit, subject, check, code) in cases {
            let mut inputs = Inputs::complete();
            edit(inputs.declaration());
            let found = Unit {
                phase: Phase::StructuralAdmission,
                subject: Subject::Structural(subject),
                check,
            };
            reporting(&inputs, &[], &[]).outcome(
                None,
                identity(),
                |evaluation, invoked, outcome| {
                    assert!(outcome.checked.is_none());
                    assert_eq!(issues(evaluation), [(found.clone(), code, None)]);
                    let groups = invoked.state.as_ref().unwrap().groups.as_ref();
                    assert!(groups.map_or(true, |groups| groups.partition().is_err()));
                    let id = serde_json::from_value(json!("web")).unwrap();
                    let members = groups.ok().and_then(|groups| groups.get(&id));
                    assert!(matches!(members, None | Some(Err(_))), "{found:?}");
                },
            );
        }
        let inputs = Inputs::complete();
        let text = serde_json::to_string(&inputs.config).unwrap();
        let map = r#""deploymentGroups":{"#;
        let at = text.find(map).unwrap() + map.len();
        let twice = format!(
            r#"{}"web":{{"members":["browser"]}},{}"#,
            &text[..at],
            &text[at..]
        );
        inputs.entered(
            Some(Arc::from(twice.into_bytes())),
            |evaluation, invoked| {
                assert_eq!(
                    issues(&evaluation),
                    [(input(Check::UniqueMembers), Code::JsonMemberDuplicate, None)]
                );
                assert!(invoked.state.is_err());
            },
        );
    }

    /// Phase 4 observes a bound over the selected declaration's references at
    /// the declaration, in the file entry admitted (D15:790, 1918).
    #[test]
    fn a_reference_bound_is_observed_at_the_declaration_in_the_file() {
        use crate::resolver::evidence_token::EvidenceTokens;
        use crate::resolver::occurrence::redacted;
        use crate::resolver::resource_policy::ArtifactSubject;
        use crate::resolver::token::Sha256;
        const PROFILE: crate::resolver::token::Specification<'static> =
            crate::resolver::token::Specification {
                identity: "intlify-project-profile",
                revision: "0",
            };
        let mut inputs = Inputs::complete();
        inputs.capability.artifact_admission.max_artifact_references = bound(1);
        inputs.attempt(|evaluation, _| {
            let candidates = evaluation.occurrence_candidates();
            let tokens = EvidenceTokens::assign(PROFILE, redacted(candidates), &Sha256);
            let references = resource_policy::unit(
                PolicySubject::Capability(ArtifactSubject::References),
                Check::WithinCapability,
            );
            let observed: Vec<Value> = candidates
                .iter()
                .filter(|candidate| *candidate.finding.unit() == references)
                .map(|candidate| candidate.primary.retained(PROFILE, &tokens).unwrap().view())
                .collect();
            assert_eq!(observed.len(), 1);
            assert_eq!(observed[0]["origin"], "configuration-file");
            assert_eq!(
                observed[0]["value"],
                json!({ "exact-json-pointer": "/profiles/web" })
            );
        });
    }

    /// The rows of Phase 4's checks of the collection under the capability,
    /// which read no declaration (D15:2132).
    const COLLECTION_UNDER_CAPABILITY: [&str; 5] = [
        "resource-policy.artifacts.within-capability",
        "resource-policy.policy-artifacts.within-capability",
        "resource-policy.target-profile-artifacts.within-capability",
        "resource-policy.artifact.within-capability",
        "resource-policy.artifact-bytes.within-capability",
    ];

    /// The subjects of Phase 4's rechecks of the declared Profile IDs.
    fn rechecked_ids(evaluation: &Evaluation) -> Vec<PolicySubject> {
        evaluation
            .statuses()
            .filter_map(|(unit, _)| match &unit.subject {
                Subject::ResourcePolicy(subject @ PolicySubject::ProfileId(_)) => {
                    Some(subject.clone())
                }
                _ => None,
            })
            .collect()
    }

    /// The rows of the units after Phase 3 that were evaluated.
    fn evaluated_after_selection(evaluation: &Evaluation) -> BTreeSet<&'static str> {
        evaluation
            .statuses()
            .filter(|(unit, status)| {
                unit.phase > Phase::ProfileSelection && **status == Status::Evaluated
            })
            .map(|(unit, _)| row_of(unit))
            .collect()
    }

    #[test]
    fn every_phase_admits_a_complete_invocation() {
        let inputs = Inputs::complete();
        inputs.resolve(|evaluation, state| {
            assert_eq!(issues(&evaluation), []);
            assert_eq!(evaluation.notices().count(), 0);
            assert!(evaluation
                .statuses()
                .all(|(_, status)| *status == Status::Evaluated));
            // Phase 1 records its owner and the complete input's ten checks
            // (D15:1876, 2129, 2516), and Phase 2 its owner, the schema
            // selection's four checks, the analysis's measure, the units of
            // its scope levels, and the `profileSpecification` group's owner
            // (D15:2130, 2173, 2516). The root's nine are the types of the
            // root, `$schema`, `profiles`, and `schemaVersion`, the two
            // required members, the closed values of `profiles` and
            // `schemaVersion`, and the root's known members; each entry's key
            // is one unit, the profile's, its two targets', and its group's,
            // and the fields under each entry are units of its own.
            let phases: BTreeSet<Phase> =
                evaluation.statuses().map(|(unit, _)| unit.phase).collect();
            let mut ran = BTreeSet::from(PHASES);
            ran.extend([
                Phase::EntryAdmission,
                Phase::StructuralAdmission,
                Phase::ProfileSelection,
            ]);
            assert_eq!(phases, ran);
            let count = |phase| {
                evaluation
                    .statuses()
                    .filter(|(unit, _)| unit.phase == phase)
                    .count()
            };
            assert_eq!(count(Phase::EntryAdmission), 11);
            let mut kinds: BTreeMap<SubjectKind, usize> = BTreeMap::new();
            for (unit, _) in evaluation.statuses() {
                if unit.phase == Phase::StructuralAdmission {
                    *kinds.entry(unit.subject.kind()).or_default() += 1;
                }
            }
            assert_eq!(
                kinds,
                BTreeMap::from([
                    (SubjectKind::MaterializedDependencies, 1),
                    (SubjectKind::ConfigurationSchemaSelection, 4),
                    (SubjectKind::CapabilityStructuralAnalysis, 1),
                    (SubjectKind::RootField, 9),
                    (SubjectKind::ProfileKey, 1),
                    (SubjectKind::ProfileField, 147),
                    (SubjectKind::TargetProfileKey, 2),
                    (SubjectKind::TargetField, 49),
                    (SubjectKind::DeploymentGroupKey, 1),
                    (SubjectKind::GroupField, 16),
                    (SubjectKind::ProfileSpecificationDependency, 1),
                ])
            );
            assert_eq!(count(Phase::StructuralAdmission), 232);
            // Phase 3 counts the declarations, checks the one Profile ID, and
            // runs the four checks of the string selector (D15:1885, 1886,
            // 2295-2297, Decision 015-196).
            assert_eq!(count(Phase::ProfileSelection), 6);
            assert_eq!(unestablished(&evaluation), []);
            let project = state.locales.project.as_ref().unwrap();
            assert_eq!(spellings(project.requested().as_slice()), ["en", "ja"]);
            assert_eq!(project.requested_default().as_str(), "en");
            assert_eq!(
                project.source_default().map(CanonicalLocale::as_str),
                Some("en")
            );
            // Each of the nine references names its own artifact, and each
            // makes one binding.
            let references = state.references.as_ref().unwrap();
            assert_eq!(references.iter().count(), 9);
            for (field, tuple, artifact) in references.iter() {
                assert_eq!(
                    artifact.member.envelope().unwrap().tuple(),
                    tuple,
                    "{field:?}"
                );
            }
            let mut submitted: Vec<ExactTuple> = inputs.members.iter().map(tuple_of).collect();
            submitted.sort();
            let bound: Vec<ExactTuple> = state
                .bindings
                .as_ref()
                .unwrap()
                .iter()
                .map(|(tuple, _)| tuple.clone())
                .collect();
            assert_eq!(bound, submitted);
            // The browser's own default and the server's inherited one, each
            // target with the Target Profile its reference names.
            let targets = state.targets.as_ref().unwrap();
            for (id, requested, default) in [
                ("browser", vec!["en", "ja"], "en"),
                ("server", vec!["en", "ja"], "en"),
            ] {
                let id: crate::model::TargetId = serde_json::from_value(json!(id)).unwrap();
                let target = targets.get(&id).unwrap().resolved.as_ref().unwrap();
                assert_eq!(spellings(target.requested.as_slice()), requested);
                assert_eq!(target.effective_default.as_str(), default);
                let field = ReferenceField::TargetProfile(id);
                let named = references
                    .iter()
                    .find(|(own, _, _)| **own == field)
                    .unwrap();
                assert_eq!(target.profile.0, named.1);
            }
            // One group holds both targets.
            let groups = state.groups.as_ref().unwrap();
            let members: Vec<(&str, Vec<&str>)> = groups
                .iter()
                .map(|(id, members)| {
                    let members = members.as_ref().unwrap();
                    let members = members.iter().map(crate::model::TargetId::as_str);
                    (id.as_str(), members.collect())
                })
                .collect();
            assert_eq!(members, [("web", vec!["browser", "server"])]);
            // The server hydrates the browser.
            let hydration = state.hydration.as_ref().unwrap();
            let relations: Vec<(&str, &str)> = hydration
                .get(&serde_json::from_value(json!("web")).unwrap())
                .unwrap()
                .as_ref()
                .unwrap()
                .iter()
                .map(|(server, client)| (server.as_str(), client.as_str()))
                .collect();
            assert_eq!(relations, [("server", "browser")]);
            assert_eq!(absent(&state), Vec::<&str>::new());
            assert_eq!(
                state.policy.selector_length,
                Some(StringLengthFact::Exact(3))
            );
        });
    }

    /// Every check a file entry records, of every kind but the kinds declared
    /// only, of which no invocation records a unit yet (Decision 015-202).
    #[test]
    fn a_complete_invocation_runs_every_check_of_every_kind() {
        Inputs::complete().resolve(|evaluation, _| {
            let ran: BTreeSet<(SubjectKind, Check)> = evaluation
                .statuses()
                .map(|(unit, _)| (unit.subject.kind(), unit.check))
                .collect();
            let listed: BTreeSet<(SubjectKind, Check)> = SubjectKind::ALL
                .into_iter()
                .filter(|kind| !kind.declared_only())
                .flat_map(|kind| kind.checks().iter().map(move |&check| (kind, check)))
                .filter(|(_, check)| check.recorded_for(EntryPath::File))
                .collect();
            assert_eq!(ran, listed);
        });
    }

    /// Every unit's status.
    fn candidates(evaluation: &Evaluation) -> BTreeMap<Unit, Status> {
        evaluation
            .candidates()
            .map(|(unit, status)| (unit.clone(), status.clone()))
            .collect()
    }

    fn statuses(evaluation: &Evaluation) -> BTreeMap<Unit, Status> {
        evaluation
            .statuses()
            .map(|(unit, status)| (unit.clone(), status.clone()))
            .collect()
    }

    /// Each candidate of two complete invocations blocks in turn, with the
    /// other candidates of its unit. A candidate then not evaluated with the
    /// blocked unit's result among its causes, or not recorded because only
    /// that result names its subject, waits on it: its row reaches the blocked
    /// unit's row through the Check Registry's prerequisites, and each
    /// prerequisite a row lists is such a wait. Candidates show what a unit
    /// over several values hides: it stays evaluated while any candidate is
    /// (D15:2184).
    #[test]
    fn the_check_registry_lists_what_each_check_waits_on() {
        let mut waits = BTreeSet::new();
        for inputs in [
            Inputs::complete(),
            Inputs::two_groups(),
            Inputs::one_locale(),
        ] {
            let complete = inputs.resolve(|evaluation, _| candidates(&evaluation));
            assert!(complete.values().all(|status| *status == Status::Evaluated));
            for blocked in complete.keys() {
                let (after, reached) = inputs.attempt_in(
                    Evaluation::blocking(blocked.clone()),
                    |evaluation, state| {
                        let state =
                            state.unwrap_or_else(|invariant| panic!("{blocked:?}: {invariant:?}"));
                        // The last phase the invocation reached, when it
                        // stopped before the semantic phases.
                        let reached = state
                            .is_none()
                            .then(|| evaluation.statuses().map(|(unit, _)| unit.phase).max())
                            .flatten();
                        (candidates(&evaluation), reached)
                    },
                );
                let logical = blocked.logical();
                for (unit, status) in &after {
                    if let Status::NotEvaluated(causes) = status {
                        if causes.keys().any(|cause| *cause.unit() == logical) {
                            waits.insert((row_of(unit), row_of(blocked)));
                        }
                    }
                }
                for unit in complete.keys().filter(|unit| !after.contains_key(unit)) {
                    // Without a value from entry admission there is no Shared
                    // projection, which no prerequisite names: the phases the
                    // invocation did not reach wait on no prerequisite.
                    if reached.is_some_and(|reached| unit.phase > reached) {
                        continue;
                    }
                    waits.insert((row_of(unit), row_of(blocked)));
                }
            }
        }
        let registry = CheckRegistry::revision_zero();
        let requirements = requirements(&registry);
        for &(dependent, prerequisite) in &waits {
            assert!(
                requirements[dependent].contains(prerequisite),
                "{dependent} waits on {prerequisite}"
            );
        }
        // A file records no unit of a check only a Snapshot has something to
        // examine for, so a wait on one is observed once a programmatic
        // invocation exists (Decision 015-202).
        let recorded: BTreeSet<&str> = registry
            .rows
            .iter()
            .filter(|row| {
                let rank: usize = row.check_rank.parse().unwrap();
                row.subject_kind.checks()[rank].recorded_for(EntryPath::File)
            })
            .map(|row| row.check_id.as_str())
            .collect();
        for row in &registry.rows {
            for prerequisite in row
                .prerequisite_check_ids
                .iter()
                .filter(|prerequisite| recorded.contains(prerequisite.as_str()))
            {
                assert!(
                    waits.contains(&(row.check_id.as_str(), prerequisite.as_str())),
                    "{} lists {prerequisite}",
                    row.check_id
                );
            }
        }
    }

    #[test]
    fn each_phase_resolves_its_own_field_under_its_own_slot() {
        Inputs::complete().resolve(|_, state| {
            let input = specifications();
            assert_eq!(
                state.specifications,
                Ok(SpecificationSet {
                    project_profile: ACTIVE,
                    locale_canonicalization: fixture_pins().specification,
                    locale_negotiation: input.locale_negotiation.unwrap(),
                    message_locale_fallback: input.message_locale_fallback.unwrap(),
                    coverage: input.coverage.unwrap(),
                    intent_surface_class_vocabulary: input.intent_surface_class_vocabulary.unwrap(),
                    delivery_graph: input.delivery_graph.unwrap(),
                    delivery_placement: input.delivery_placement.unwrap(),
                })
            );
            let classes: Vec<&str> = state
                .vocabulary
                .as_ref()
                .unwrap()
                .members()
                .iter()
                .map(Token::as_str)
                .collect();
            assert_eq!(classes, vectors::MEMBERS);
            let project = state.identity.unwrap();
            assert_eq!(
                (
                    project.project_id().as_str(),
                    project.selection_scope().as_str()
                ),
                ("storefront", "storefront.production")
            );
            let negotiation = state.negotiation.unwrap();
            assert_eq!(
                negotiation.specification(),
                &identity("intlify-locale-negotiation", "0")
            );
            assert_eq!(
                negotiation.aliases(),
                &BTreeMap::from([(canonical("de"), canonical("en"))])
            );
            let fallback = state.fallback.unwrap();
            assert_eq!(
                fallback.specification(),
                &identity("intlify-message-locale-fallback", "0")
            );
            assert_eq!(
                fallback.sequences().keys().collect::<Vec<_>>(),
                [&canonical("ja")]
            );
            let coverage = state.coverage.unwrap();
            assert_eq!(
                coverage.specification(),
                &identity("intlify-localization-coverage", "0")
            );
            // The rules decide every `ja` cell; the default decides the rest
            // but `en` checkout, which a rule also requires directly.
            let allowed: Vec<(&str, &str)> = coverage
                .table()
                .filter(|(_, _, mode)| *mode == CoverageMode::FallbackAllowed)
                .map(|(locale, surface, _)| (locale.as_str(), surface.as_str()))
                .collect();
            assert_eq!(
                allowed,
                [("ja", "checkout"), ("ja", "internal-tool"), ("ja", "nav")]
            );
            let delivery = state.delivery.unwrap();
            assert_eq!(
                (delivery.graph(), delivery.placement(), delivery.mode()),
                (
                    &identity("intlify-delivery-graph", "0"),
                    &identity("intlify-delivery-placement", "0"),
                    PlacementMode::Duplicate
                )
            );
        });
    }

    #[test]
    fn phase_4_rechecks_what_phases_1_to_3_measured() {
        let mut inputs = Inputs::complete();
        let mut limits = policy_limits();
        let input = &mut limits.configuration_input;
        input.value.max_nodes = bound(1);
        input.structural.max_profile_id_bytes = bound(2);
        input.structural.max_structural_analysis_units = bound(1);
        inputs.select_policy(resource_limit_policy(&limits));
        inputs.resolve(|evaluation, state| {
            let rechecked: Vec<PolicySubject> = issues(&evaluation)
                .into_iter()
                .map(|(unit, code, _)| match unit.subject {
                    Subject::ResourcePolicy(subject) if code == Code::ResourceLimitExceeded => {
                        subject
                    }
                    other => panic!("{other:?} {code:?}"),
                })
                .collect();
            assert_eq!(
                rechecked,
                [
                    PolicySubject::Nodes,
                    PolicySubject::ProfileId(serde_json::from_value(json!("web")).unwrap()),
                    PolicySubject::Selector,
                    PolicySubject::StructuralAnalysis
                ]
            );
            assert_eq!(absent(&state), EVERY);
            assert_eq!(unestablished(&evaluation), AFTER_POLICY);
            assert_eq!(
                state.policy.selector_length,
                Some(StringLengthFact::OverLimit)
            );
        });
    }

    /// Phase 4 rechecks the exact measures: a Policy whose bounds are what
    /// Phases 1 and 2 measured admits each of them (D15:2132).
    #[test]
    fn phase_4_rechecks_the_exact_measures() {
        let mut inputs = Inputs::complete();
        let mut limits = policy_limits();
        // Selecting another Policy changes only the reference's digest, of
        // the same length, so no measure.
        inputs.select_policy(resource_limit_policy(&limits));
        let analysis = analyze(&inputs.config, &inputs.capability);
        let input = &mut limits.configuration_input;
        input.value.max_nodes = bound(analysis.value_counts().nodes);
        input.structural.max_structural_analysis_units =
            bound(analysis.structural_units().unwrap());
        inputs.select_policy(resource_limit_policy(&limits));
        inputs.resolve(|evaluation, state| {
            assert_eq!(issues(&evaluation), []);
            assert!(state.policy.confirmed.is_ok());
        });
    }

    #[test]
    fn two_spellings_of_one_requested_locale_are_a_duplicate_keeping_both() {
        let mut inputs = Inputs::complete();
        inputs.declaration()["requestedLocales"] = json!(["ja", "en-US", "en", "EN-us"]);
        inputs.resolve(|evaluation, state| {
            let subject = LocaleSubject::RequestedLocale(LocaleKey::Canonical(canonical("en-US")));
            let duplicate = locales::unit(subject.clone(), Check::UniqueLocale);
            assert_eq!(
                issues(&evaluation),
                [(duplicate.clone(), Code::LocaleDuplicate, None)]
            );
            // Each spelling is a candidate at its own token beside the other
            // (D15:2410), and the corrected one's notice is at its own token.
            let pointer = |index: u64| {
                json!({ "exact-json-pointer": format!("/profiles/web/requestedLocales/{index}") })
            };
            let located = |unit: &Unit| -> Vec<(Value, Vec<Value>)> {
                observed_views(&evaluation, unit)
                    .into_iter()
                    .map(|(primary, related)| {
                        (
                            primary["value"].clone(),
                            related
                                .into_iter()
                                .map(|related| related["value"].clone())
                                .collect(),
                        )
                    })
                    .collect()
            };
            assert_eq!(
                located(&duplicate),
                [
                    (pointer(1), vec![pointer(3)]),
                    (pointer(3), vec![pointer(1)])
                ]
            );
            assert_eq!(
                located(&locales::unit(subject.clone(), Check::CanonicalLocale)),
                [(pointer(3), vec![])]
            );
            // Both spellings keep their place, and neither wins: the project's
            // locales wait on the duplicate.
            let places: Vec<&Location> = state
                .locales
                .occurrences
                .iter()
                .filter(|occurrence| occurrence.subject == subject)
                .map(|occurrence| &occurrence.location)
                .collect();
            assert_eq!(
                places,
                [&Location::RequestedLocale(1), &Location::RequestedLocale(3)]
            );
            let key = only_issue(&evaluation);
            assert_eq!(
                keys(state.locales.project.as_ref().unwrap_err()),
                BTreeSet::from([key])
            );
            assert_eq!(absent(&state), ["locales"]);
            assert_eq!(unestablished(&evaluation), [SemanticGroup::ProjectLocales]);
        });
    }

    #[test]
    fn a_selected_reference_differing_only_in_its_digest_selects_no_other_policy() {
        let mut inputs = Inputs::complete();
        inputs.declaration()["policies"]["resourceLimits"]["semanticDigest"] =
            json!(format!("sha256:{}", "0".repeat(64)));
        inputs.resolve(|evaluation, state| {
            // The decision reports with the code 015 maps it to, which exact
            // resolution shares.
            let decision =
                resource_policy::unit(PolicySubject::ResourceLimits, Check::BootstrapCandidate);
            assert_eq!(
                issues(&evaluation),
                [(
                    decision,
                    Code::ArtifactReferenceNotAdmitted,
                    Some(Reason::DigestMismatch)
                )]
            );
            assert_eq!(absent(&state), EVERY);
            // Every unit of Phases 5 to 13, and Phase 4's after the decision,
            // waits on the decision alone; neither Phase 0 nor the owner of
            // the group it establishes does.
            assert_eq!(unestablished(&evaluation), AFTER_POLICY);
            assert_eq!(
                suppressed(&evaluation),
                (
                    BTreeSet::from_iter(PHASES[1..].iter().copied()),
                    BTreeSet::from([only_issue(&evaluation)])
                )
            );
            assert!(later_phases_wait_on(&evaluation, &only_issue(&evaluation)));
        });
    }

    #[test]
    fn a_second_envelope_of_the_selected_policy_is_the_bootstraps_duplicate() {
        let mut inputs = Inputs::complete();
        inputs.members.push(inputs.members[0].clone());
        inputs.resolve(|evaluation, state| {
            // The collection's code and reason, on the decision's unit.
            let decision =
                resource_policy::unit(PolicySubject::ResourceLimits, Check::BootstrapCandidate);
            assert_eq!(
                issues(&evaluation),
                [(
                    decision,
                    Code::ArtifactCollectionNotAdmitted,
                    Some(Reason::DuplicateEnvelope)
                )]
            );
            // The collection admission waits on the policy, so it never
            // checks the pair.
            assert_eq!(
                suppressed(&evaluation),
                (
                    BTreeSet::from_iter(PHASES[1..].iter().copied()),
                    BTreeSet::from([only_issue(&evaluation)])
                )
            );
            assert!(later_phases_wait_on(&evaluation, &only_issue(&evaluation)));
            assert_eq!(absent(&state), EVERY);
        });
    }

    #[test]
    fn a_second_envelope_of_another_artifact_is_the_collections_duplicate() {
        let mut inputs = Inputs::complete();
        inputs.members.push(inputs.members[TRUST].clone());
        inputs.resolve(|evaluation, state| {
            let [(unit, code, reason)] = issues(&evaluation).try_into().unwrap();
            assert!(matches!(unit.subject, Subject::ArtifactCollection(_)));
            assert_eq!(
                (code, reason),
                (
                    Code::ArtifactCollectionNotAdmitted,
                    Some(Reason::DuplicateEnvelope)
                )
            );
            // The bootstrap checks only the selected policy's candidates. The
            // targets are checked, but their Target Profiles need the Set, and
            // so do their directions and the relation's roles.
            assert_eq!(
                absent(&state),
                ["references", "bindings", "targets", "hydration"]
            );
            // No Set: every reference waits on the collection, and so do each
            // target's directions and the relation's checks of the two Target
            // Profiles.
            let key = only_issue(&evaluation);
            assert_eq!(
                suppressed(&evaluation),
                (
                    BTreeSet::from([
                        Phase::ArtifactReferenceResolution,
                        Phase::TargetResolution,
                        Phase::HydrationValidation
                    ]),
                    BTreeSet::from([key.clone()])
                )
            );
            assert_eq!(
                keys(state.bindings.as_ref().unwrap_err()),
                BTreeSet::from([key])
            );
            assert_eq!(
                unestablished(&evaluation),
                [
                    SemanticGroup::RequiredPolicies,
                    SemanticGroup::OptionalPolicies,
                    SemanticGroup::Targets,
                    SemanticGroup::DeploymentGroups
                ]
            );
        });
    }

    #[test]
    fn two_bodies_sharing_all_but_the_digest_are_conflicting_content() {
        // Another approval policy body under the same kind, identity,
        // revision, and specification revision. A Trust Policy has one valid
        // body, so it cannot conflict with another valid one.
        let mut inputs = Inputs::complete();
        inputs.members.push(envelope_of(
            ArtifactKind::ApprovalPolicy,
            "storefront-approval",
            "1",
            json!({"localizedApproval": "not-required"}),
        ));
        inputs.resolve(|evaluation, state| {
            let [(unit, code, reason)] = issues(&evaluation).try_into().unwrap();
            assert!(matches!(unit.subject, Subject::ArtifactCollection(_)));
            assert_eq!(
                (code, reason),
                (
                    Code::ArtifactCollectionNotAdmitted,
                    Some(Reason::ConflictingContent)
                )
            );
            assert_eq!(
                absent(&state),
                ["references", "bindings", "targets", "hydration"]
            );
        });
        // Another body of the selected policy is the bootstrap's conflict.
        let mut inputs = Inputs::complete();
        inputs
            .members
            .push(resource_limit_policy(&fixture_limits(60_000)));
        inputs.resolve(|evaluation, state| {
            let decision =
                resource_policy::unit(PolicySubject::ResourceLimits, Check::BootstrapCandidate);
            assert_eq!(
                issues(&evaluation),
                [(
                    decision,
                    Code::ArtifactCollectionNotAdmitted,
                    Some(Reason::ConflictingContent)
                )]
            );
            assert_eq!(absent(&state), EVERY);
            assert!(later_phases_wait_on(&evaluation, &only_issue(&evaluation)));
        });
    }

    #[test]
    fn a_missing_project_profile_slot_leaves_only_its_groups_owner_waiting() {
        let mut inputs = Inputs::complete();
        inputs.unasserted();
        inputs.resolve(|evaluation, state| {
            assert_eq!(
                issues(&evaluation),
                [(
                    PHASE_0,
                    Code::SpecificationInputIncomplete,
                    Some(Reason::MissingRequiredSlot)
                )]
            );
            // No semantic check waits on Phase 0: only the owner of the group
            // it establishes does.
            let owner = dependency::unit(SemanticGroup::ProfileSpecification);
            let Some(Status::NotEvaluated(causes)) = evaluation.status(&owner) else {
                panic!("the owner was evaluated");
            };
            assert_eq!(keys(causes), BTreeSet::from([only_issue(&evaluation)]));
            assert!(evaluation
                .statuses()
                .all(|(unit, status)| *unit == owner || *status == Status::Evaluated));
            assert_eq!(
                unestablished(&evaluation),
                [SemanticGroup::ProfileSpecification]
            );
            // Only the Set, which needs all eight slots, is missing.
            assert_eq!(
                keys(state.specifications.as_ref().unwrap_err()),
                BTreeSet::from([only_issue(&evaluation)])
            );
            assert_eq!(absent(&state), ["specifications"]);
        });
    }

    #[test]
    fn a_policy_needing_more_than_the_capability_leaves_the_selection_unconfirmed() {
        let mut inputs = Inputs::complete();
        let mut limits = policy_limits();
        let capability = capability_limits().locale_resolution.max_requested_locales;
        limits.locale_resolution.max_requested_locales = bound(capability.get() + 1);
        inputs.select_policy(resource_limit_policy(&limits));
        inputs.resolve(|evaluation, state| {
            let comparison =
                resource_policy::unit(PolicySubject::ResourceLimits, Check::CompatiblePolicy);
            assert_eq!(
                issues(&evaluation),
                [(comparison, Code::ResourcePolicyCapabilityIncompatible, None)]
            );
            assert_eq!(absent(&state), EVERY);
            assert_eq!(unestablished(&evaluation), AFTER_POLICY);
            assert!(later_phases_wait_on(&evaluation, &only_issue(&evaluation)));
            assert_eq!(
                suppressed(&evaluation),
                (
                    BTreeSet::from_iter(PHASES[1..].iter().copied()),
                    BTreeSet::from([only_issue(&evaluation)])
                )
            );
        });
    }

    /// When the selected Resource Limit Policy cannot be admitted, because it
    /// needs more than the capability allows or is not submitted, the
    /// invocation gives one outcome, blocked on that failure alone, with no
    /// Policy confirmed and every domain bounded by the capability alone:
    /// neither the failed Policy's own diagnostic bounds nor those of the
    /// admissible Policy another declaration names, submitted beside it, end
    /// a domain the capability leaves room for, while the capability's Shared
    /// `maxRecords` one under the Shared records refuses the last Shared
    /// slice, after Phase 4 (D15:1940, 2132, 2391, 3467).
    #[test]
    fn a_selected_policy_that_cannot_be_admitted_leaves_one_outcome_under_the_capability() {
        use crate::resolver::evaluation::ExhaustedBound;
        use crate::resolver::outcome::tests::{entry, identity, reporting, shared};
        type Fail<'f> = &'f dyn Fn(&mut Inputs);
        // Bounds that would end both domains at their first records.
        let narrow = |limits: &ResolutionLimits| {
            tightened(
                limits,
                &[(entry("maxRecords"), 1), (shared("maxRecords"), 1)],
            )
        };
        let incompatible = |inputs: &mut Inputs| {
            let mut limits = narrow(&policy_limits());
            let capability = inputs.capability.locale_resolution.max_requested_locales;
            limits.locale_resolution.max_requested_locales = bound(capability.get() + 1);
            inputs.select_policy(resource_limit_policy(&limits));
        };
        let unsubmitted = |inputs: &mut Inputs| {
            let pinned = inputs.pinned.clone();
            inputs.members.retain(|member| tuple_of(member) != pinned);
        };
        let compared =
            resource_policy::unit(PolicySubject::ResourceLimits, Check::CompatiblePolicy);
        let decided =
            resource_policy::unit(PolicySubject::ResourceLimits, Check::BootstrapCandidate);
        let cases: [(Fail<'_>, &Unit); 2] = [(&incompatible, &compared), (&unsubmitted, &decided)];
        for (fail, failed) in cases {
            let failing = |capability: &[(BoundId, u64)]| {
                let mut inputs = reporting(&Inputs::complete(), capability, &[]);
                let other = envelope_of(
                    ArtifactKind::ResourceLimitPolicy,
                    "admin-limits",
                    "1",
                    json!({ SECTION: narrow(&policy_limits()).section() }),
                );
                let mut admin = inputs.declaration().clone();
                admin["policies"]["resourceLimits"] = reference_value(&tuple_of(&other));
                inputs.config["profiles"]["admin"] = admin;
                inputs.members.push(other);
                fail(&mut inputs);
                inputs
            };
            let (records, last) =
                failing(&[]).outcome(None, identity(), |evaluation, invoked, outcome| {
                    let found = issues(evaluation);
                    let [(unit, ..)] = &found[..] else {
                        panic!("{found:?}");
                    };
                    assert_eq!(unit, failed);
                    let policy = &invoked.state.as_ref().unwrap().policy;
                    assert!(policy.confirmed.is_err() && policy.diagnostics.is_none());
                    assert_eq!(outcome.identity, identity());
                    assert!(outcome.checked.is_none());
                    let evidence = &outcome.evidence;
                    assert!(evidence.complete());
                    let shared = &evidence.shared.as_ref().unwrap().records;
                    let last = shared.iter().map(|(owner, _)| *owner).max().unwrap();
                    assert!(*last > compared);
                    (crate::artifacts::count(shared.len()), last.clone())
                });
            let room = (shared("maxRecords"), records - 1);
            failing(&[room]).outcome(None, identity(), |_, _, outcome| {
                let evidence = &outcome.evidence;
                assert!(evidence.entry.terminal.is_none(), "{failed:?}");
                let shared = evidence.shared.as_ref().unwrap();
                let exhausted = shared.terminal.as_ref().unwrap().exhausted;
                assert_eq!(exhausted.cursor, &last, "{failed:?}");
                assert_eq!(exhausted.bound, ExhaustedBound::Domain(room.0));
            });
        }
    }

    #[test]
    fn a_trust_reference_without_its_artifact_is_not_found_and_the_rest_resolve() {
        let mut inputs = Inputs::complete();
        inputs.members.remove(TRUST);
        inputs.resolve(|evaluation, state| {
            let trust = exact_resolution::unit(ReferenceField::Policy(PolicySlot::Trust));
            assert_eq!(
                issues(&evaluation),
                [(
                    trust,
                    Code::ArtifactReferenceNotAdmitted,
                    Some(Reason::NotFound)
                )]
            );
            // Every other reference was resolved, the targets' among them;
            // only the bindings, which need every reference, are missing, and
            // only the owner of the required policies waits.
            let key = only_issue(&evaluation);
            let waiting: Vec<(&Unit, BTreeSet<IssueKey>)> = evaluation
                .statuses()
                .filter_map(|(unit, status)| match status {
                    Status::NotEvaluated(causes) => Some((unit, keys(causes))),
                    Status::Evaluated => None,
                })
                .collect();
            assert_eq!(
                waiting,
                [(
                    &dependency::unit(SemanticGroup::RequiredPolicies),
                    BTreeSet::from([key.clone()])
                )]
            );
            assert_eq!(
                keys(state.references.as_ref().unwrap_err()),
                BTreeSet::from([key.clone()])
            );
            assert_eq!(
                keys(state.bindings.as_ref().unwrap_err()),
                BTreeSet::from([key])
            );
            assert_eq!(absent(&state), ["references", "bindings"]);
        });
    }

    /// 015 checks the Target ID map's `maxTargetProfiles` in Phase 7, before
    /// every later check of a target. One target more than it allows leaves
    /// each Target Profile reference waiting on it in Phase 9, while every
    /// policy's reference resolves.
    #[test]
    fn a_target_map_over_its_bound_leaves_every_target_profile_reference_waiting() {
        let mut inputs = Inputs::complete();
        let mut limits = policy_limits();
        limits.target_grouping.max_target_profiles = bound(1);
        inputs.select_policy(resource_limit_policy(&limits));
        inputs.resolve(|evaluation, state| {
            let key = only_issue(&evaluation);
            assert_eq!(
                key.unit(),
                &locales::unit(LocaleSubject::TargetProfiles, Check::WithinBound)
            );
            let mut evaluated = Vec::new();
            let mut waiting = Vec::new();
            for (unit, status) in evaluation.statuses() {
                let Subject::ArtifactReference(field) = &unit.subject else {
                    continue;
                };
                match status {
                    Status::Evaluated => evaluated.push(field.clone()),
                    Status::NotEvaluated(causes) => {
                        assert_eq!(keys(causes), BTreeSet::from([key.clone()]), "{field:?}");
                        waiting.push(field.clone());
                    }
                }
            }
            assert_eq!(evaluated.len(), 7);
            assert!(evaluated
                .iter()
                .all(|field| matches!(field, ReferenceField::Policy(_))));
            assert_eq!(
                waiting,
                TARGETS.map(|(id, _)| {
                    ReferenceField::TargetProfile(serde_json::from_value(json!(id)).unwrap())
                })
            );
            assert_eq!(
                keys(state.references.as_ref().unwrap_err()),
                BTreeSet::from([key])
            );
            assert_eq!(
                absent(&state),
                ["references", "bindings", "targets", "groups", "hydration"]
            );
            // The policy groups need no Target Profile reference.
            assert_eq!(
                unestablished(&evaluation),
                [SemanticGroup::Targets, SemanticGroup::DeploymentGroups]
            );
        });
    }

    /// The views of what each occurrence candidate of `unit` observed, its
    /// primary Evidence and its related Evidence, in the order recorded.
    fn observed_views(evaluation: &Evaluation, unit: &Unit) -> Vec<(Value, Vec<Value>)> {
        use crate::resolver::evidence_token::EvidenceTokens;
        use crate::resolver::occurrence::redacted;
        use crate::resolver::token::Sha256;
        const PROFILE: crate::resolver::token::Specification<'static> =
            crate::resolver::token::Specification {
                identity: "intlify-project-profile",
                revision: "0",
            };
        let candidates = evaluation.occurrence_candidates();
        let tokens = EvidenceTokens::assign(PROFILE, redacted(candidates), &Sha256);
        let view = |projection: &crate::resolver::evidence::EvidenceProjection| {
            projection.retained(PROFILE, &tokens).unwrap().view()
        };
        candidates
            .iter()
            .filter(|candidate| candidate.finding.unit() == unit)
            .map(|candidate| {
                (
                    view(&candidate.primary),
                    candidate.related.iter().map(view).collect(),
                )
            })
            .collect()
    }

    /// Past a bound the confirmed policy supplies, Phase 7 observes the
    /// declaration's field in the file beside the policy Phase 4 admitted,
    /// under the exact reference that keys it (D15:824, 834, 2249).
    #[test]
    fn a_locale_bound_is_observed_beside_the_admitted_policy() {
        use intlify_shared_json::encoding::encode;

        use crate::resolver::evidence_source::hex;
        let mut inputs = Inputs::complete();
        let mut limits = policy_limits();
        limits.target_grouping.max_target_profiles = bound(1);
        let policy = resource_limit_policy(&limits);
        inputs.select_policy(policy.clone());
        inputs.resolve(|evaluation, _| {
            let map = locales::unit(LocaleSubject::TargetProfiles, Check::WithinBound);
            let observed = observed_views(&evaluation, &map);
            assert_eq!(observed.len(), 1);
            let (primary, related) = &observed[0];
            assert_eq!(primary["origin"], "configuration-file");
            assert_eq!(
                primary["value"],
                json!({ "exact-json-pointer": "/profiles/web/targetProfiles" })
            );
            let reference = reference_value(&tuple_of(&policy));
            assert_eq!(
                related,
                &[json!({
                    "kind": "artifact-input",
                    "origin": "profile-resolution-artifact-collection",
                    "component": [
                        { "fixed-role": "artifact-member" },
                        { "safe-key": hex(&encode(&reference).unwrap()) },
                    ],
                })]
            );
        });
    }

    /// Coverage observes through the same observer: past the rule count's
    /// bound, the rules are observed beside the policy Phase 4 admitted,
    /// which supplies the bound (D15:1901, 2249).
    #[test]
    fn a_coverage_count_is_observed_beside_the_admitted_policy() {
        use crate::resolver::coverage::{self, CoverageSubject};
        let mut inputs = Inputs::complete();
        let mut limits = policy_limits();
        // The complete declaration submits two rules.
        limits.locale_resolution.max_coverage_rules = bound(1);
        inputs.select_policy(resource_limit_policy(&limits));
        inputs.resolve(|evaluation, _| {
            let rules = coverage::unit(CoverageSubject::Rules, Check::WithinBound);
            assert_eq!(
                issues(&evaluation),
                [(rules.clone(), Code::ResourceLimitExceeded, None)]
            );
            let observed = observed_views(&evaluation, &rules);
            assert_eq!(observed.len(), 1);
            let (primary, related) = &observed[0];
            assert_eq!(
                primary["value"],
                json!({ "exact-json-pointer": "/profiles/web/coverage/rules" })
            );
            assert_eq!(
                related.iter().map(|view| &view["kind"]).collect::<Vec<_>>(),
                [&json!("artifact-input")]
            );
        });
    }

    /// Phase 12 observes through the same observer: past the member count's
    /// bound, a group's members are observed beside the policy Phase 4
    /// admitted, which supplies the bound (D15:2249).
    #[test]
    fn a_group_count_is_observed_beside_the_admitted_policy() {
        use crate::resolver::groups::{self, GroupSubject};
        let mut inputs = Inputs::complete();
        let mut limits = policy_limits();
        // The complete declaration's group names two members.
        limits.target_grouping.max_members_per_group = bound(1);
        inputs.select_policy(resource_limit_policy(&limits));
        inputs.resolve(|evaluation, _| {
            let web = serde_json::from_value(json!("web")).unwrap();
            let members = groups::unit(GroupSubject::Members(web), Check::WithinBound);
            assert_eq!(
                issues(&evaluation),
                [(members.clone(), Code::ResourceLimitExceeded, None)]
            );
            let observed = observed_views(&evaluation, &members);
            assert_eq!(observed.len(), 1);
            let (primary, related) = &observed[0];
            assert_eq!(
                primary["value"],
                json!({ "exact-json-pointer": "/profiles/web/deploymentGroups/web/members" })
            );
            assert_eq!(
                related.iter().map(|view| &view["kind"]).collect::<Vec<_>>(),
                [&json!("artifact-input")]
            );
        });
    }

    /// Without its Target ID map, a declaration's groups are still checked,
    /// but whether a member names a declared Target ID waits on the map, and
    /// Phase 12 observes nothing the declaration lacks.
    #[test]
    fn a_group_beside_no_target_map_leaves_its_members_unknown() {
        use crate::resolver::groups::{self, GroupSubject};
        let mut inputs = Inputs::complete();
        inputs
            .declaration()
            .as_object_mut()
            .unwrap()
            .remove("targetProfiles");
        inputs.resolve(|evaluation, _| {
            for id in ["browser", "server"] {
                let member = GroupSubject::Member(
                    serde_json::from_value(json!("web")).unwrap(),
                    serde_json::from_value(json!(id)).unwrap(),
                );
                let known = groups::unit(member, Check::KnownMember);
                assert!(
                    matches!(evaluation.status(&known), Some(Status::NotEvaluated(_))),
                    "{id}"
                );
            }
        });
    }

    fn group_id(id: &str) -> crate::model::DeploymentGroupId {
        serde_json::from_value(json!(id)).unwrap()
    }

    fn target_id(id: &str) -> crate::model::TargetId {
        serde_json::from_value(json!(id)).unwrap()
    }

    /// The bound's unit over a Phase 12 `subject`.
    fn group_bound(subject: crate::resolver::groups::GroupSubject) -> Unit {
        crate::resolver::groups::unit(subject, Check::WithinBound)
    }

    fn group_member(group: &str, member: &str, check: Check) -> Unit {
        use crate::resolver::groups::{unit, GroupSubject};
        unit(
            GroupSubject::Member(group_id(group), target_id(member)),
            check,
        )
    }

    fn group_place(target: &str) -> Unit {
        use crate::resolver::groups::{unit, GroupSubject};
        unit(
            GroupSubject::Assignment(target_id(target)),
            Check::SingleGroup,
        )
    }

    /// Every blocking result of Phase 12, as its unit and code.
    fn group_issues(evaluation: &Evaluation) -> Vec<(Unit, Code)> {
        issues(evaluation)
            .into_iter()
            .filter(|(unit, _, _)| unit.phase == Phase::GroupResolution)
            .map(|(unit, code, _)| (unit, code))
            .collect()
    }

    /// What `unit` waits on.
    fn causes_of(evaluation: &Evaluation, unit: &Unit) -> BTreeSet<IssueKey> {
        let Some(Status::NotEvaluated(causes)) = evaluation.status(unit) else {
            panic!("{unit:?} is not waiting");
        };
        keys(causes)
    }

    /// Every check of the group `id` with the one member `member`.
    fn group_checked(id: &str, member: &str) -> [Unit; 5] {
        use crate::resolver::groups::GroupSubject as G;
        [
            group_bound(G::Group(group_id(id))),
            group_bound(G::Members(group_id(id))),
            group_member(id, member, Check::WithinBound),
            group_member(id, member, Check::KnownMember),
            group_member(id, member, Check::UniqueMember),
        ]
    }

    /// A group that failed holds back no other group: both counts and the
    /// other group's checks run, with no Finding made up for it, and the
    /// partition waits on what failed (D15:2149, 2184, 2365, 2367, 3445).
    #[test]
    fn a_group_that_failed_leaves_the_other_groups_checked() {
        use crate::resolver::groups::GroupSubject as G;
        for failed in [
            json!(7),
            json!({}),
            json!({"members": 7}),
            json!({"members": []}),
        ] {
            let mut inputs = Inputs::two_groups();
            inputs.declaration()["deploymentGroups"]["web"] = failed.clone();
            inputs.resolve(|evaluation, state| {
                all_evaluated(
                    &evaluation,
                    &[
                        group_bound(G::Groups),
                        group_bound(G::Memberships),
                        group_bound(G::Group(group_id("web"))),
                    ],
                );
                all_evaluated(&evaluation, &group_checked("jobs", "worker"));
                assert_eq!(group_issues(&evaluation), [], "{failed}");
                let members = group_bound(G::Members(group_id("web")));
                let why = causes_of(&evaluation, &members);
                assert!(why
                    .iter()
                    .all(|key| key.unit().phase == Phase::StructuralAdmission));
                let groups = state.groups.as_ref().unwrap();
                assert!(groups.get(&group_id("jobs")).unwrap().is_ok(), "{failed}");
                assert!(groups.get(&group_id("web")).unwrap().is_err(), "{failed}");
                assert_eq!(keys(groups.partition().as_ref().unwrap_err()), why);
            });
        }
    }

    /// A member that failed holds back none of its siblings: the group's
    /// count takes it, each other member is checked, and what they find
    /// among themselves is found; the group, which needs every member, waits
    /// (D15:1934, 2184, 2190, 2367, 2438).
    #[test]
    fn a_member_that_failed_leaves_its_siblings_checked() {
        use crate::resolver::groups::GroupSubject as G;
        let mut inputs = Inputs::two_groups();
        inputs.declaration()["deploymentGroups"]["web"]["members"] =
            json!(["browser", 7, "server", "phone", "server"]);
        inputs.resolve(|evaluation, state| {
            all_evaluated(
                &evaluation,
                &[
                    group_bound(G::Memberships),
                    group_bound(G::Members(group_id("web"))),
                    group_member("web", "browser", Check::WithinBound),
                    group_member("web", "browser", Check::KnownMember),
                    group_member("web", "browser", Check::UniqueMember),
                    group_member("web", "server", Check::KnownMember),
                ],
            );
            assert_eq!(
                group_issues(&evaluation),
                [
                    (
                        group_member("web", "phone", Check::KnownMember),
                        Code::GroupMemberUnknown
                    ),
                    (
                        group_member("web", "server", Check::UniqueMember),
                        Code::GroupMemberDuplicate
                    ),
                ]
            );
            // A member keeps its position among the elements, the failed one
            // included.
            let phone = group_member("web", "phone", Check::KnownMember);
            let (primary, _) = &observed_views(&evaluation, &phone)[0];
            assert_eq!(
                primary["value"],
                json!({ "exact-json-pointer": "/profiles/web/deploymentGroups/web/members/3" })
            );
            let groups = state.groups.as_ref().unwrap();
            assert!(groups.get(&group_id("web")).unwrap().is_err());
            assert!(groups.get(&group_id("jobs")).unwrap().is_ok());
            let named = groups.named(&group_id("web")).unwrap().as_ref().unwrap();
            assert_eq!(named.holds(&target_id("browser")), Ok(true));
            assert!(named.holds(&target_id("worker")).is_err());
            let jobs = groups.named(&group_id("jobs")).unwrap().as_ref().unwrap();
            assert_eq!(jobs.holds(&target_id("browser")), Ok(false));
            assert!(groups.partition().is_err());
        });
    }

    /// The group bounds count what was submitted: a member that failed is
    /// an occurrence of its array and of the total, and an entry whose key
    /// failed is a group of the map whose members the total still takes
    /// (D15:1932, 1934, 1935).
    #[test]
    fn the_group_counts_take_every_occurrence_beside_what_failed() {
        use crate::resolver::evaluation::{Actual, LimitExceeded};
        use crate::resolver::groups::GroupSubject as G;
        let found = |edit: fn(&mut Value), limit: fn(&mut ResolutionLimits), over: G, total| {
            let mut inputs = Inputs::two_groups();
            edit(inputs.declaration());
            let mut limits = policy_limits();
            limit(&mut limits);
            inputs.select_policy(resource_limit_policy(&limits));
            inputs.resolve(|evaluation, _| {
                let over = group_bound(over);
                let key = evaluation
                    .issues()
                    .find(|key| key.unit() == &over)
                    .unwrap_or_else(|| panic!("{over:?} found nothing"))
                    .clone();
                assert!(matches!(
                    evaluation.limit(&key),
                    Some(LimitExceeded { actual: Actual::Exact(actual), .. }) if actual == total
                ));
            });
        };
        let failed_member: fn(&mut Value) = |app| {
            app["deploymentGroups"]["web"]["members"] = json!(["browser", 7, "server"]);
        };
        found(
            failed_member,
            |limits| limits.target_grouping.max_members_per_group = bound(2),
            G::Members(group_id("web")),
            3,
        );
        found(
            failed_member,
            |limits| limits.target_grouping.max_membership_occurrences = bound(3),
            G::Memberships,
            4,
        );
        let failed_key: fn(&mut Value) = |app| {
            app["deploymentGroups"]["Web App"] = json!({"members": ["browser", "server"]});
        };
        found(
            failed_key,
            |limits| limits.target_grouping.max_deployment_groups = bound(2),
            G::Groups,
            3,
        );
        found(
            failed_key,
            |limits| limits.target_grouping.max_membership_occurrences = bound(4),
            G::Memberships,
            5,
        );
    }

    /// The total waits only once no group's members can be read, since a
    /// unit over several values evaluates what is admitted (D15:2184, 3354).
    #[test]
    fn the_membership_total_waits_only_when_every_array_failed() {
        use crate::resolver::groups::GroupSubject as G;
        let mut inputs = Inputs::two_groups();
        inputs.declaration()["deploymentGroups"]["web"]["members"] = json!(7);
        inputs.declaration()["deploymentGroups"]["jobs"]["members"] = json!(7);
        inputs.resolve(|evaluation, _| {
            let why = causes_of(&evaluation, &group_bound(G::Memberships));
            assert_eq!(why.len(), 2);
            assert!(why
                .iter()
                .all(|key| key.unit().phase == Phase::StructuralAdmission));
            all_evaluated(&evaluation, &[group_bound(G::Groups)]);
        });
    }

    /// An entry whose key failed has no subject in Phase 12, and the targets
    /// it might hold cannot be proved placed or unplaced; the other group
    /// goes on (D15:2184, 2365, 3354).
    #[test]
    fn a_group_whose_key_failed_has_no_subject() {
        use crate::resolver::groups::GroupSubject as G;
        let mut inputs = Inputs::two_groups();
        let groups = inputs.declaration()["deploymentGroups"]
            .as_object_mut()
            .unwrap();
        let web = groups.remove("web").unwrap();
        groups.insert("Web App".into(), web);
        inputs.resolve(|evaluation, state| {
            all_evaluated(
                &evaluation,
                &[group_bound(G::Groups), group_bound(G::Memberships)],
            );
            all_evaluated(&evaluation, &group_checked("jobs", "worker"));
            assert_eq!(group_issues(&evaluation), []);
            let named: Vec<_> = evaluation
                .statuses()
                .filter_map(|(unit, _)| match &unit.subject {
                    Subject::Group(G::Group(id)) => Some(id.as_str().to_owned()),
                    _ => None,
                })
                .collect();
            assert_eq!(named, ["jobs"]);
            let key = causes_of(&evaluation, &group_place("browser"));
            assert_eq!(key.len(), 1);
            assert!(key
                .iter()
                .all(|key| key.unit().phase == Phase::StructuralAdmission));
            waits_on_exactly(&evaluation, &group_place("server"), &key);
            let groups = state.groups.as_ref().unwrap();
            assert_eq!(keys(groups.partition().as_ref().unwrap_err()), key);
        });
    }

    /// A target's place is found from the groups that can be read: one that
    /// a group names is placed whatever else failed, and one that none names
    /// waits on exactly what could not be read, since it may be there
    /// (D15:2184, 2365, 2438).
    #[test]
    fn a_place_is_found_from_the_groups_that_can_be_read() {
        type Case = (
            fn(&mut Value),
            &'static [&'static str],
            &'static [&'static str],
        );
        const ALONE: &[&str] = &["browser", "server"];
        let cases: [Case; 8] = [
            (
                |app| app["deploymentGroups"]["web"] = json!(7),
                &["worker"],
                ALONE,
            ),
            (
                |app| app["deploymentGroups"]["web"] = json!({}),
                &["worker"],
                ALONE,
            ),
            (
                |app| app["deploymentGroups"]["web"]["members"] = json!(7),
                &["worker"],
                ALONE,
            ),
            (
                |app| app["deploymentGroups"]["web"]["members"] = json!([]),
                &["worker"],
                ALONE,
            ),
            (
                |app| {
                    let groups = app["deploymentGroups"].as_object_mut().unwrap();
                    let web = groups.remove("web").unwrap();
                    groups.insert("Web App".into(), web);
                },
                &["worker"],
                ALONE,
            ),
            (
                |app| {
                    app["deploymentGroups"]["web"]["members"] = json!(["browser", 7, "server"]);
                },
                &["browser", "server", "worker"],
                &[],
            ),
            // A group whose key failed places nothing, and the partition still
            // lacks it beside every target placed.
            (
                |app| app["deploymentGroups"]["Web App"] = json!({"members": ["browser"]}),
                &["browser", "server", "worker"],
                &[],
            ),
            (
                |app| app["deploymentGroups"]["web"]["members"] = json!(["browser", 7]),
                &["browser", "worker"],
                &["server"],
            ),
        ];
        for (edit, placed, waiting) in cases {
            let mut inputs = Inputs::two_groups();
            edit(inputs.declaration());
            inputs.resolve(|evaluation, state| {
                let places: Vec<Unit> = placed.iter().map(|id| group_place(id)).collect();
                all_evaluated(&evaluation, &places);
                assert_eq!(group_issues(&evaluation), []);
                let why: BTreeSet<IssueKey> = evaluation.issues().cloned().collect();
                for target in waiting {
                    waits_on_exactly(&evaluation, &group_place(target), &why);
                }
                assert!(state.groups.as_ref().unwrap().partition().is_err());
            });
        }
    }

    /// A target that two groups name is found whatever else failed, while
    /// one that no admitted element names waits on what failed: it cannot be
    /// proved unassigned (D15:2365, 2438).
    #[test]
    fn a_target_two_groups_name_is_found_beside_a_member_that_failed() {
        let mut inputs = Inputs::two_groups();
        inputs.declaration()["deploymentGroups"]["web"]["members"] = json!(["browser", 7]);
        inputs.declaration()["deploymentGroups"]["jobs"]["members"] = json!(["worker", "browser"]);
        inputs.resolve(|evaluation, _| {
            assert_eq!(
                group_issues(&evaluation),
                [(group_place("browser"), Code::TargetAssignedToMultipleGroups)]
            );
            all_evaluated(&evaluation, &[group_place("worker")]);
            let why = causes_of(&evaluation, &group_place("server"));
            assert_eq!(why.len(), 1);
            assert!(why
                .iter()
                .all(|key| key.unit().phase == Phase::StructuralAdmission));
        });
    }

    /// Beside a Target ID key that failed, a member naming an admitted Target
    /// ID is known, and one naming none is unproved rather than unknown
    /// (D15:2184, 2365, 2367).
    #[test]
    fn a_member_beside_a_target_key_that_failed_is_unproved_only_when_it_names_none() {
        let mut inputs = Inputs::two_groups();
        let target = inputs.declaration()["targetProfiles"]["browser"].clone();
        inputs.declaration()["targetProfiles"]["Web App"] = target;
        inputs.declaration()["deploymentGroups"]["web"]["members"] =
            json!(["browser", "server", "phone"]);
        inputs.resolve(|evaluation, state| {
            all_evaluated(
                &evaluation,
                &[
                    group_member("web", "browser", Check::KnownMember),
                    group_member("web", "server", Check::KnownMember),
                    group_place("browser"),
                    group_place("server"),
                    group_place("worker"),
                ],
            );
            assert_eq!(group_issues(&evaluation), []);
            let why = causes_of(
                &evaluation,
                &group_member("web", "phone", Check::KnownMember),
            );
            assert_eq!(why.len(), 1);
            assert!(why
                .iter()
                .all(|key| key.unit().phase == Phase::StructuralAdmission));
            let groups = state.groups.as_ref().unwrap();
            assert!(groups.partition().is_err());
        });
        // Every admitted target placed, the partition still lacks the entry
        // whose key failed, and nothing else.
        let mut inputs = Inputs::two_groups();
        let target = inputs.declaration()["targetProfiles"]["browser"].clone();
        inputs.declaration()["targetProfiles"]["Web App"] = target;
        inputs.resolve(|evaluation, state| {
            assert_eq!(group_issues(&evaluation), []);
            let found: BTreeSet<IssueKey> = evaluation.issues().cloned().collect();
            let groups = state.groups.as_ref().unwrap();
            assert_eq!(keys(groups.partition().as_ref().unwrap_err()), found);
        });
    }

    /// A target that failed in a field other than its key still takes its
    /// place: Phase 12 reads no more of it than its Target ID (D15:2367,
    /// 3445).
    #[test]
    fn a_target_whose_field_failed_is_still_placed() {
        use crate::resolver::groups::GroupSubject as G;
        let mut inputs = Inputs::two_groups();
        inputs.declaration()["targetProfiles"]["browser"]["requestedLocales"] = json!([7]);
        inputs.resolve(|evaluation, state| {
            all_evaluated(&evaluation, &group_checked("jobs", "worker"));
            all_evaluated(
                &evaluation,
                &[
                    group_bound(G::Members(group_id("web"))),
                    group_member("web", "browser", Check::KnownMember),
                    group_member("web", "server", Check::KnownMember),
                    group_place("browser"),
                    group_place("server"),
                    group_place("worker"),
                ],
            );
            assert_eq!(group_issues(&evaluation), []);
            assert_eq!(state.groups.as_ref().unwrap().partition(), &Ok(()));
        });
    }

    /// Without a Group ID map to read, each declared target's place is still
    /// recorded, waiting on why there is none (D15:1171, 2184).
    #[test]
    fn a_place_beside_no_group_map_waits_on_the_map() {
        use crate::resolver::groups::GroupSubject as G;
        let edits: [fn(&mut Value); 2] = [
            |app| app["deploymentGroups"] = json!(7),
            |app| {
                app.as_object_mut().unwrap().remove("deploymentGroups");
            },
        ];
        for edit in edits {
            let mut inputs = Inputs::complete();
            edit(inputs.declaration());
            inputs.resolve(|evaluation, _| {
                let why = causes_of(&evaluation, &group_bound(G::Groups));
                for target in ["browser", "server"] {
                    waits_on_exactly(&evaluation, &group_place(target), &why);
                }
            });
        }
    }

    /// A group past its own bound is left out of what the places read: a
    /// target another group names is placed, and one only it names waits on
    /// the bound (D15:1934, 2184, 2365).
    #[test]
    fn a_group_past_its_bound_holds_back_only_the_targets_it_alone_could_place() {
        use crate::resolver::groups::GroupSubject as G;
        let mut inputs = Inputs::two_groups();
        let mut limits = policy_limits();
        limits.target_grouping.max_members_per_group = bound(1);
        inputs.select_policy(resource_limit_policy(&limits));
        inputs.resolve(|evaluation, _| {
            let over = group_bound(G::Members(group_id("web")));
            assert_eq!(
                group_issues(&evaluation),
                [(over.clone(), Code::ResourceLimitExceeded)]
            );
            all_evaluated(&evaluation, &[group_place("worker")]);
            let found = BTreeSet::from([evaluation
                .issues()
                .find(|key| key.unit() == &over)
                .unwrap()
                .clone()]);
            for target in ["browser", "server"] {
                waits_on_exactly(&evaluation, &group_place(target), &found);
            }
        });
    }

    /// Phase 13 observes through the same observer: a relation's client that
    /// names no declared Target ID is observed at its field, beside the
    /// declared Target IDs (D15:2249).
    #[test]
    fn an_unknown_endpoint_is_observed_beside_the_declared_targets() {
        use crate::resolver::hydration::{self, EndpointField, HydrationSubject};
        let mut inputs = Inputs::complete();
        inputs.declaration()["deploymentGroups"]["web"]["hydrationRelations"] =
            json!([{"server": "server", "client": "phone"}]);
        inputs.resolve(|evaluation, _| {
            let endpoint = HydrationSubject::Endpoint(
                group_id("web"),
                RelationKey::Pair(target_id("server"), target_id("phone")),
                EndpointField::Client,
            );
            let known = hydration::unit(endpoint, Check::KnownEndpoint);
            let observed = observed_views(&evaluation, &known);
            assert_eq!(observed.len(), 1);
            let (primary, related) = &observed[0];
            assert_eq!(
                primary["value"],
                json!({
                    "exact-json-pointer":
                        "/profiles/web/deploymentGroups/web/hydrationRelations/0/client"
                })
            );
            assert_eq!(
                related
                    .iter()
                    .map(|view| &view["value"])
                    .collect::<Vec<_>>(),
                [&json!({ "exact-json-pointer": "/profiles/web/targetProfiles" })]
            );
        });
    }

    /// Without its Target ID map, whether a relation's endpoint names a
    /// declared Target ID waits on the map, and Phase 13 observes nothing the
    /// declaration lacks.
    #[test]
    fn a_relation_beside_no_target_map_leaves_its_endpoints_unknown() {
        use crate::resolver::hydration::{self, EndpointField, HydrationSubject};
        let mut inputs = Inputs::complete();
        inputs
            .declaration()
            .as_object_mut()
            .unwrap()
            .remove("targetProfiles");
        inputs.resolve(|evaluation, _| {
            for field in [EndpointField::Client, EndpointField::Server] {
                let endpoint = HydrationSubject::Endpoint(
                    group_id("web"),
                    RelationKey::Pair(target_id("server"), target_id("browser")),
                    field,
                );
                let known = hydration::unit(endpoint, Check::KnownEndpoint);
                assert!(
                    matches!(evaluation.status(&known), Some(Status::NotEvaluated(_))),
                    "{field:?}"
                );
            }
        });
    }

    /// Without the project's default, an endpoint that inherits it has no
    /// effective default: whether a relation's endpoints share one waits on
    /// it, and Phase 13 observes nothing the declaration lacks.
    #[test]
    fn a_relation_beside_no_project_default_leaves_its_default_unchecked() {
        use crate::resolver::hydration::{self, HydrationSubject};
        let mut inputs = Inputs::complete();
        inputs
            .declaration()
            .as_object_mut()
            .unwrap()
            .remove("defaultRequestedLocale");
        inputs.resolve(|evaluation, _| {
            // `server` inherits the default; `browser` declares its own.
            let relation = HydrationSubject::Relation(
                group_id("web"),
                RelationKey::Pair(target_id("server"), target_id("browser")),
            );
            let default = hydration::unit(relation, Check::SameEffectiveDefault);
            assert!(matches!(
                evaluation.status(&default),
                Some(Status::NotEvaluated(_))
            ));
        });
    }

    /// A subject of the group `web` as 015 projects it (D15:2190).
    fn of_web(subject: &Value) -> Value {
        json!({ "parent": { "identity": "web" }, "subject": subject })
    }

    /// The relation of `web` with both endpoints admitted.
    fn identified(server: &str, client: &str) -> Value {
        of_web(&json!({ "identity": { "server": server, "client": client } }))
    }

    /// The one subject of `web`'s relations without an identity: their
    /// Subject content projection (D15:2181, 2188, 2203).
    fn unidentified() -> Value {
        of_web(&json!({ "content": { "type": "object" } }))
    }

    /// The endpoint `field` names in `relation`.
    fn endpoint_of(relation: &Value, field: &str) -> Value {
        json!({ "parent": relation, "subject": [field] })
    }

    /// The roles of `target` among `web`'s relations.
    fn roles_of(target: &str) -> Value {
        of_web(&json!({ "identity": target }))
    }

    const ENDPOINT_CHECKS: [Check; 4] = [
        Check::WithinBound,
        Check::KnownEndpoint,
        Check::SameGroupEndpoint,
        Check::EndpointRole,
    ];

    const RELATION_CHECKS: [Check; 5] = [
        Check::DistinctEndpoints,
        Check::UniqueRelation,
        Check::SameRequestedLocales,
        Check::SameEffectiveDefault,
        Check::StaticCompatibility,
    ];

    /// The status of `check` over the Phase 13 subject projected as `subject`.
    fn phase_13(evaluation: &Evaluation, check: Check, subject: &Value) -> Option<Status> {
        evaluation
            .statuses()
            .find(|(unit, _)| {
                unit.phase == Phase::HydrationValidation
                    && unit.check == check
                    && crate::resolver::token::projected(&unit.subject) == *subject
            })
            .map(|(_, status)| status.clone())
    }

    /// Each of `checks` over `subject` was evaluated.
    fn checked_13(evaluation: &Evaluation, checks: &[Check], subject: &Value) {
        for &check in checks {
            assert_eq!(
                phase_13(evaluation, check, subject),
                Some(Status::Evaluated),
                "{check:?} {subject}"
            );
        }
    }

    /// Phase 13's Findings, by check, projected subject, code, and reason.
    fn issues_13(evaluation: &Evaluation) -> Vec<(Check, Value, Code, Option<Reason>)> {
        evaluation
            .issues()
            .filter(|key| key.unit().phase == Phase::HydrationValidation)
            .map(|key| {
                (
                    key.unit().check,
                    crate::resolver::token::projected(&key.unit().subject),
                    key.code(),
                    key.reason(),
                )
            })
            .collect()
    }

    fn relations_of(inputs: &mut Inputs, group: &str, relations: Value) {
        inputs.declaration()["deploymentGroups"][group]["hydrationRelations"] = relations;
    }

    /// A relation that failed holds back no other: the other's checks run,
    /// nothing is found for it, and it has no subject, since no object holds
    /// endpoints to key; what needs every relation waits on it (D15:2149,
    /// 2184, 2365, 2367, 3445).
    #[test]
    fn a_relation_that_failed_leaves_the_others_checked() {
        for failed in [json!(7), json!("relation"), json!(null), json!([])] {
            let mut inputs = Inputs::complete();
            relations_of(
                &mut inputs,
                "web",
                json!([{"server": "server", "client": "browser"}, failed.clone()]),
            );
            inputs.resolve(|evaluation, state| {
                let relation = identified("server", "browser");
                checked_13(&evaluation, &RELATION_CHECKS, &relation);
                for field in ["client", "server"] {
                    let endpoint = endpoint_of(&relation, field);
                    checked_13(&evaluation, &ENDPOINT_CHECKS, &endpoint);
                }
                for target in ["browser", "server"] {
                    checked_13(&evaluation, &[Check::SingleRole], &roles_of(target));
                }
                assert_eq!(issues_13(&evaluation), [], "{failed}");
                let none = unidentified();
                assert_eq!(phase_13(&evaluation, Check::UniqueRelation, &none), None);
                let why = BTreeSet::from([only_issue(&evaluation)]);
                let hydration = state.hydration.as_ref().unwrap();
                let web = hydration.get(&group_id("web")).unwrap();
                assert_eq!(keys(web.as_ref().unwrap_err()), why, "{failed}");
                assert_eq!(keys(hydration.complete().as_ref().unwrap_err()), why);
            });
        }
    }

    /// A relation beside members that failed checks all but whether the
    /// group names its endpoints, and observes no array the group lacks
    /// (D15:2184, 2365).
    #[test]
    fn a_relation_beside_members_that_failed_waits_only_on_them() {
        let mut inputs = Inputs::complete();
        inputs.declaration()["deploymentGroups"]["web"]
            .as_object_mut()
            .unwrap()
            .remove("members");
        inputs.resolve(|evaluation, _| {
            let why = BTreeSet::from([only_issue(&evaluation)]);
            let relation = identified("server", "browser");
            checked_13(&evaluation, &RELATION_CHECKS, &relation);
            for field in ["client", "server"] {
                let endpoint = endpoint_of(&relation, field);
                let others = [
                    Check::WithinBound,
                    Check::KnownEndpoint,
                    Check::EndpointRole,
                ];
                checked_13(&evaluation, &others, &endpoint);
                let Some(Status::NotEvaluated(found)) =
                    phase_13(&evaluation, Check::SameGroupEndpoint, &endpoint)
                else {
                    panic!("{endpoint} is not waiting");
                };
                assert_eq!(keys(&found), why);
            }
        });
    }

    /// Every occurrence counts: a relation that failed, and one in an entry
    /// whose key failed (D15:1936).
    #[test]
    fn the_relation_count_takes_every_occurrence_beside_what_failed() {
        use crate::resolver::evaluation::{Actual, LimitExceeded};
        let edits: [fn(&mut Value); 2] = [
            |app| {
                app["deploymentGroups"]["web"]["hydrationRelations"] =
                    json!([{"server": "server", "client": "browser"}, 7, {"server": 1}]);
            },
            |app| {
                app["deploymentGroups"]["Web App"] =
                    json!({"members": ["browser"], "hydrationRelations": [7, 7]});
            },
        ];
        for edit in edits {
            let mut inputs = Inputs::complete();
            edit(inputs.declaration());
            let mut limits = policy_limits();
            limits.target_grouping.max_hydration_relations = bound(2);
            inputs.select_policy(resource_limit_policy(&limits));
            inputs.resolve(|evaluation, _| {
                let over = hydration::unit(HydrationSubject::Relations, Check::WithinBound);
                let key = evaluation
                    .issues()
                    .find(|key| key.unit() == &over)
                    .expect("the count is past its bound")
                    .clone();
                assert!(matches!(
                    evaluation.limit(&key),
                    Some(LimitExceeded {
                        actual: Actual::Exact(3),
                        ..
                    })
                ));
            });
        }
    }

    /// The total waits only when no array can be read: an empty array is one
    /// that holds no occurrence, while an omitted one is no value at all
    /// (D15:569, 1936, 2184, 3354).
    #[test]
    fn the_relation_total_waits_only_when_every_array_failed() {
        let total = hydration::unit(HydrationSubject::Relations, Check::WithinBound);
        let mut inputs = Inputs::two_groups();
        relations_of(&mut inputs, "web", json!(7));
        relations_of(&mut inputs, "jobs", json!("relations"));
        inputs.resolve(|evaluation, _| {
            let why = causes_of(&evaluation, &total);
            assert_eq!(why.len(), 2);
            assert!(why
                .iter()
                .all(|key| key.unit().phase == Phase::StructuralAdmission));
        });
        let mut inputs = Inputs::two_groups();
        relations_of(&mut inputs, "web", json!(7));
        inputs.resolve(|evaluation, state| {
            all_evaluated(&evaluation, std::slice::from_ref(&total));
            // The array that failed holds back its own group alone.
            let hydration = state.hydration.as_ref().unwrap();
            assert!(hydration.get(&group_id("web")).unwrap().is_err());
            assert!(hydration.get(&group_id("jobs")).unwrap().is_ok());
        });
        let mut inputs = Inputs::two_groups();
        relations_of(&mut inputs, "web", json!(7));
        inputs.declaration()["deploymentGroups"]["jobs"]
            .as_object_mut()
            .unwrap()
            .remove("hydrationRelations");
        inputs.resolve(|evaluation, _| {
            assert_eq!(causes_of(&evaluation, &total).len(), 1);
        });
    }

    /// An undeclared member holds back no relation, while what needs the
    /// whole map waits on it, so no profile leaves a problem unseen inside
    /// what it could not read (D15:2367).
    #[test]
    fn what_needs_every_relation_waits_on_what_could_not_be_read() {
        let edits: [fn(&mut Value); 2] = [
            |web| web["hydrationRelations"][0]["undeclared"] = json!(1),
            |web| web["undeclared"] = json!(1),
        ];
        for edit in edits {
            let mut inputs = Inputs::complete();
            edit(&mut inputs.declaration()["deploymentGroups"]["web"]);
            inputs.resolve(|evaluation, state| {
                let relation = identified("server", "browser");
                checked_13(&evaluation, &RELATION_CHECKS, &relation);
                assert_eq!(issues_13(&evaluation), []);
                let why = BTreeSet::from([only_issue(&evaluation)]);
                let hydration = state.hydration.as_ref().unwrap();
                assert_eq!(keys(hydration.complete().as_ref().unwrap_err()), why);
            });
            assert!(inputs.view().is_err());
        }
    }

    /// A group whose key failed has no subject here, its relations still
    /// count, and what needs every group waits on it (D15:1936, 2184).
    #[test]
    fn a_group_whose_key_failed_has_no_relation_subject() {
        let mut inputs = Inputs::two_groups();
        let groups = inputs.declaration()["deploymentGroups"]
            .as_object_mut()
            .unwrap();
        let web = groups.remove("web").unwrap();
        groups.insert("Web App".into(), web);
        inputs.resolve(|evaluation, state| {
            let bound = hydration::unit(HydrationSubject::Relations, Check::WithinBound);
            all_evaluated(&evaluation, std::slice::from_ref(&bound));
            let units: Vec<_> = evaluation
                .statuses()
                .filter(|(unit, _)| matches!(unit.subject, Subject::Hydration(_)))
                .map(|(unit, _)| unit.clone())
                .collect();
            assert_eq!(units, [bound]);
            let hydration = state.hydration.as_ref().unwrap();
            assert!(hydration.get(&group_id("jobs")).unwrap().is_ok());
            assert!(hydration.complete().is_err());
        });
    }

    /// Each of `checks` over `subject` waits on exactly `causes`.
    fn waiting_13(
        evaluation: &Evaluation,
        checks: &[Check],
        subject: &Value,
        causes: &BTreeSet<IssueKey>,
    ) {
        for &check in checks {
            let Some(Status::NotEvaluated(found)) = phase_13(evaluation, check, subject) else {
                panic!("{check:?} {subject} is not waiting");
            };
            assert_eq!(&keys(&found), causes, "{check:?} {subject}");
        }
    }

    /// Where each occurrence of the Finding of `check` over `subject` is.
    fn pointers_13(evaluation: &Evaluation, check: Check, subject: &Value) -> Vec<Value> {
        use crate::resolver::declaration_observer::fixtures::view;
        let mut pointers: Vec<Value> = evaluation
            .occurrence_candidates()
            .iter()
            .filter(|candidate| {
                let unit = candidate.finding.unit();
                unit.check == check && crate::resolver::token::projected(&unit.subject) == *subject
            })
            .map(|candidate| view(&candidate.primary)["value"]["exact-json-pointer"].clone())
            .collect();
        pointers.sort_by_key(ToString::to_string);
        pointers
    }

    /// One endpoint that failed leaves the other checked: the relation has
    /// no identity, so it is keyed by its content; the endpoint that failed
    /// has no subject, and what compares the two waits on it (D15:1931, 2181,
    /// 2184, 2190, 2203, 2367).
    #[test]
    fn an_endpoint_that_failed_leaves_the_other_checked() {
        for failed in [
            json!({"server": "server", "client": 7}),
            json!({"server": "server"}),
            json!({"server": "server", "client": "Not An Id"}),
        ] {
            let mut inputs = Inputs::complete();
            relations_of(&mut inputs, "web", json!([failed.clone()]));
            inputs.resolve(|evaluation, state| {
                let relation = unidentified();
                let server = endpoint_of(&relation, "server");
                checked_13(&evaluation, &ENDPOINT_CHECKS, &server);
                let client = endpoint_of(&relation, "client");
                assert_eq!(phase_13(&evaluation, Check::WithinBound, &client), None);
                let why = BTreeSet::from([only_issue(&evaluation)]);
                waiting_13(&evaluation, &RELATION_CHECKS, &relation, &why);
                checked_13(&evaluation, &[Check::SingleRole], &roles_of("server"));
                let browser = roles_of("browser");
                assert_eq!(phase_13(&evaluation, Check::SingleRole, &browser), None);
                assert_eq!(issues_13(&evaluation), [], "{failed}");
                let hydration = state.hydration.as_ref().unwrap();
                assert_eq!(keys(hydration.complete().as_ref().unwrap_err()), why);
            });
        }
    }

    /// A relation with neither endpoint keeps its subject, whose checks wait
    /// on both failures, and has no endpoint or role to check (D15:2184,
    /// 2190).
    #[test]
    fn a_relation_with_neither_endpoint_waits_on_both() {
        let mut inputs = Inputs::complete();
        relations_of(
            &mut inputs,
            "web",
            json!([{"server": 7, "client": "No Id"}]),
        );
        inputs.resolve(|evaluation, _| {
            let why: BTreeSet<IssueKey> = evaluation.issues().cloned().collect();
            assert_eq!(why.len(), 2);
            let relation = unidentified();
            waiting_13(&evaluation, &RELATION_CHECKS, &relation, &why);
            for field in ["client", "server"] {
                let endpoint = endpoint_of(&relation, field);
                assert_eq!(phase_13(&evaluation, Check::WithinBound, &endpoint), None);
            }
            let roles = evaluation
                .statuses()
                .filter(|(unit, _)| unit.check == Check::SingleRole);
            assert_eq!(roles.count(), 0);
        });
    }

    /// The endpoint left reports what it alone proves, under the relation's
    /// content, and an endpoint that fails holds back only what needs it
    /// (D15:2190, 2365, 2367).
    #[test]
    fn the_endpoint_left_reports_what_it_alone_proves() {
        let found = |server: &str| {
            let mut inputs = Inputs::complete();
            let relation = json!([{"server": server, "client": 7}]);
            relations_of(&mut inputs, "web", relation);
            inputs.resolve(|evaluation, _| issues_13(&evaluation))
        };
        let at = endpoint_of(&unidentified(), "server");
        let invalid = |check, reason| (check, at.clone(), Code::HydrationRelationInvalid, reason);
        assert_eq!(
            found("phone"),
            [
                invalid(Check::KnownEndpoint, Some(Reason::UnknownEndpoint)),
                invalid(Check::SameGroupEndpoint, Some(Reason::CrossGroup)),
            ]
        );
        assert_eq!(
            found("browser"),
            [invalid(
                Check::EndpointRole,
                Some(Reason::ServerCapabilityMissing)
            )]
        );
    }

    /// A Target ID's two roles are proved from the endpoints admitted, and
    /// one role among them passes: a unit over several values evaluates what
    /// is admitted (D15:2098, 2184, 2365).
    #[test]
    fn a_target_two_relations_give_both_roles_is_found_beside_an_endpoint_that_failed() {
        for relations in [
            json!([
                {"server": "server", "client": 7},
                {"server": "browser", "client": "server"},
            ]),
            json!([
                {"server": 7, "client": "server"},
                {"server": "server", "client": "browser"},
            ]),
        ] {
            let mut inputs = Inputs::complete();
            relations_of(&mut inputs, "web", relations.clone());
            inputs.resolve(|evaluation, _| {
                let dual = (
                    Check::SingleRole,
                    roles_of("server"),
                    Code::HydrationRelationInvalid,
                    Some(Reason::DualRole),
                );
                assert!(issues_13(&evaluation).contains(&dual), "{relations}");
                checked_13(&evaluation, &[Check::SingleRole], &roles_of("browser"));
            });
        }
    }

    /// Relations without an identity share one subject, each checked on its
    /// own: a check of the endpoint under it finds each raw parent at its
    /// occurrences (D15:2190, 2203).
    #[test]
    fn relations_without_an_identity_share_a_subject_and_are_checked_apart() {
        let mut inputs = Inputs::complete();
        relations_of(
            &mut inputs,
            "web",
            json!([
                {"server": "phone", "client": 7},
                {"server": "server", "client": 8},
                {"server": "tablet", "client": 7},
                {"server": "phone", "client": 7},
            ]),
        );
        inputs.resolve(|evaluation, _| {
            let server = endpoint_of(&unidentified(), "server");
            let at = |index: usize| {
                json!(format!(
                    "/profiles/web/deploymentGroups/web/hydrationRelations/{index}/server"
                ))
            };
            assert_eq!(
                pointers_13(&evaluation, Check::KnownEndpoint, &server),
                [at(0), at(2), at(3)]
            );
            // Every raw parent joins one unit of each subject.
            for (check, subject) in [
                (Check::KnownEndpoint, server.clone()),
                (Check::UniqueRelation, unidentified()),
            ] {
                let units = evaluation.statuses().filter(|(unit, _)| {
                    unit.phase == Phase::HydrationValidation
                        && unit.check == check
                        && crate::resolver::token::projected(&unit.subject) == subject
                });
                assert_eq!(units.count(), 1, "{check:?}");
            }
            // The endpoint a raw parent spells that is declared passes on
            // its own, whatever another parent under the key found.
            checked_13(&evaluation, &[Check::SingleRole], &roles_of("server"));
        });
    }

    /// A relation's key can stand for distinct raw parents, so past
    /// `maxTargetIdBytes` an endpoint reports the first-over witness, even
    /// under an identity and for one value (D15:2203, 3355, Decision 015-157).
    #[test]
    fn an_endpoint_past_its_bound_reports_the_first_over_witness() {
        use crate::resolver::evaluation::{Actual, LimitExceeded};
        for (relation, parent) in [
            (
                json!({"server": "server", "client": "browser"}),
                identified("server", "browser"),
            ),
            (json!({"server": "server", "client": 7}), unidentified()),
        ] {
            let mut inputs = Inputs::complete();
            relations_of(&mut inputs, "web", json!([relation]));
            let mut limits = policy_limits();
            limits.target_grouping.max_target_id_bytes = bound(5);
            inputs.select_policy(resource_limit_policy(&limits));
            inputs.resolve(|evaluation, _| {
                let over = endpoint_of(&parent, "server");
                let key = evaluation
                    .issues()
                    .find(|key| {
                        key.unit().phase == Phase::HydrationValidation
                            && key.unit().check == Check::WithinBound
                            && crate::resolver::token::projected(&key.unit().subject) == over
                    })
                    .unwrap_or_else(|| panic!("{over} found nothing"))
                    .clone();
                assert!(matches!(
                    evaluation.limit(&key),
                    Some(LimitExceeded {
                        actual: Actual::AtLeast(6),
                        ..
                    })
                ));
            });
        }
    }

    /// A failure under a target that a relation does not name holds back
    /// none of that relation's checks (D15:2149, 2367, 3445).
    #[test]
    fn a_relation_waits_only_on_the_targets_it_names() {
        let mut inputs = Inputs::two_groups();
        inputs.declaration()["targetProfiles"]["worker"]["requestedLocales"] = json!(["ja", 42]);
        inputs.resolve(|evaluation, state| {
            let relation = identified("server", "browser");
            checked_13(&evaluation, &RELATION_CHECKS, &relation);
            for field in ["client", "server"] {
                let endpoint = endpoint_of(&relation, field);
                checked_13(&evaluation, &ENDPOINT_CHECKS, &endpoint);
            }
            assert_eq!(issues_13(&evaluation), []);
            let hydration = state.hydration.as_ref().unwrap();
            assert_eq!(hydration.complete(), Ok(()));
            assert!(state.targets.as_ref().unwrap().complete().is_err());
        });
    }

    /// A failure under a target a relation names holds back only what
    /// compares that fact (D15:2149, 2365, 2367).
    #[test]
    fn a_relation_beside_a_target_that_failed_waits_only_on_what_it_compares() {
        let mut inputs = Inputs::complete();
        inputs.declaration()["targetProfiles"]["browser"]["requestedLocales"] = json!(["ja", 42]);
        inputs.resolve(|evaluation, _| {
            let why = BTreeSet::from([only_issue(&evaluation)]);
            let relation = identified("server", "browser");
            checked_13(
                &evaluation,
                &[
                    Check::DistinctEndpoints,
                    Check::UniqueRelation,
                    Check::StaticCompatibility,
                ],
                &relation,
            );
            waiting_13(
                &evaluation,
                &[Check::SameRequestedLocales, Check::SameEffectiveDefault],
                &relation,
                &why,
            );
            for field in ["client", "server"] {
                let endpoint = endpoint_of(&relation, field);
                checked_13(&evaluation, &ENDPOINT_CHECKS, &endpoint);
            }
        });
    }

    /// An endpoint naming an admitted Target ID is declared whatever key
    /// failed; one naming none is unproved while a key failed, and nothing
    /// is found for it (D15:2365, 2367).
    #[test]
    fn an_endpoint_beside_a_target_key_that_failed_is_unproved_only_when_it_names_none() {
        let mut inputs = Inputs::complete();
        let copy = inputs.declaration()["targetProfiles"]["server"].clone();
        inputs.declaration()["targetProfiles"]["Web App"] = copy;
        relations_of(
            &mut inputs,
            "web",
            json!([
                {"server": "server", "client": "browser"},
                {"server": "server", "client": "phone"},
            ]),
        );
        inputs.resolve(|evaluation, _| {
            let why: BTreeSet<IssueKey> = evaluation
                .issues()
                .filter(|key| key.unit().phase == Phase::StructuralAdmission)
                .cloned()
                .collect();
            assert_eq!(why.len(), 1);
            let named = endpoint_of(&identified("server", "browser"), "client");
            checked_13(&evaluation, &[Check::KnownEndpoint], &named);
            let unnamed = endpoint_of(&identified("server", "phone"), "client");
            waiting_13(&evaluation, &[Check::KnownEndpoint], &unnamed, &why);
        });
    }

    /// An endpoint is in its group by the members that can be read: one
    /// they name passes, and one they do not name is unproved beside a
    /// member that failed, with no Finding (D15:2098, 2365, 2367).
    #[test]
    fn an_endpoint_is_in_its_group_by_the_members_that_can_be_read() {
        let mut inputs = Inputs::complete();
        inputs.declaration()["deploymentGroups"]["web"]["members"] =
            json!(["browser", 7, "server"]);
        relations_of(
            &mut inputs,
            "web",
            json!([
                {"server": "server", "client": "browser"},
                {"server": "server", "client": "phone"},
            ]),
        );
        inputs.resolve(|evaluation, _| {
            let member = |client: &str| endpoint_of(&identified("server", client), "client");
            checked_13(&evaluation, &[Check::SameGroupEndpoint], &member("browser"));
            assert!(matches!(
                phase_13(&evaluation, Check::SameGroupEndpoint, &member("phone")),
                Some(Status::NotEvaluated(_))
            ));
            assert!(!issues_13(&evaluation)
                .iter()
                .any(|(check, ..)| *check == Check::SameGroupEndpoint));
        });
    }

    /// An undeclared member under a target holds back no relation, while
    /// what needs every target waits on it (D15:2367).
    #[test]
    fn what_needs_every_target_waits_on_an_undeclared_member() {
        let mut inputs = Inputs::complete();
        inputs.declaration()["targetProfiles"]["browser"]["undeclared"] = json!(1);
        inputs.resolve(|evaluation, state| {
            checked_13(
                &evaluation,
                &RELATION_CHECKS,
                &identified("server", "browser"),
            );
            let why = BTreeSet::from([only_issue(&evaluation)]);
            let targets = state.targets.as_ref().unwrap();
            assert_eq!(keys(targets.complete().as_ref().unwrap_err()), why);
        });
        assert!(inputs.view().is_err());
    }

    /// The phases after Phase 5 classify a locale key by the canonicalizer it
    /// bound: Phase 7 observes a corrected alias key under its non-secret
    /// name and a rejected one redacted (D15:776).
    #[test]
    fn an_alias_key_is_classified_by_the_canonicalizer_phase_5_bound() {
        use crate::resolver::locales::fixtures::rejected;
        let mut inputs = Inputs::complete();
        inputs.declaration()["localeNegotiation"] = json!({"aliases": {"EN": "ja", "zz": "en"}});
        inputs.resolve(|evaluation, _| {
            let key = |key| {
                locales::unit(
                    LocaleSubject::NegotiationAliasKey(key),
                    Check::CanonicalLocale,
                )
            };
            let corrected =
                observed_views(&evaluation, &key(LocaleKey::Canonical(canonical("en"))));
            assert_eq!(corrected.len(), 1);
            assert_eq!(
                corrected[0].0["value"],
                json!({ "exact-json-pointer": "/profiles/web/localeNegotiation/aliases/EN" })
            );
            let unsafe_key = observed_views(&evaluation, &key(rejected()));
            assert_eq!(unsafe_key.len(), 1);
            assert!(unsafe_key[0].0["value"].get("exact-json-pointer").is_none());
        });
    }

    /// Phase 8 observes through what Phase 5 bound as well: a key two entries
    /// name is found at each entry's key under its non-secret name, beside
    /// the other's (D15:776, 2410).
    #[test]
    fn a_negotiation_key_is_classified_by_the_canonicalizer_phase_5_bound() {
        use crate::resolver::negotiation::{self, NegotiationSubject};
        let mut inputs = Inputs::complete();
        inputs.declaration()["localeNegotiation"] = json!({"aliases": {"EN": "ja", "en": "ja"}});
        inputs.resolve(|evaluation, _| {
            let duplicate = negotiation::unit(
                NegotiationSubject::Key(LocaleKey::Canonical(canonical("en"))),
                Check::UniqueLocale,
            );
            assert_eq!(
                issues(&evaluation),
                [(
                    duplicate.clone(),
                    Code::NegotiationAliasDuplicate,
                    Some(Reason::SameDestination)
                )]
            );
            let pointer = |name: &str| {
                json!({ "exact-json-pointer": format!("/profiles/web/localeNegotiation/aliases/{name}") })
            };
            assert_eq!(
                observed_views(&evaluation, &duplicate)
                    .into_iter()
                    .map(|(primary, related)| (
                        primary["value"].clone(),
                        related
                            .iter()
                            .map(|view| view["value"].clone())
                            .collect::<Vec<_>>()
                    ))
                    .collect::<Vec<_>>(),
                [
                    (pointer("EN"), vec![pointer("en")]),
                    (pointer("en"), vec![pointer("EN")]),
                ]
            );
        });
    }

    /// The message fallback observes through what Phase 5 bound as well: a
    /// source outside the requested locales is found at its key, under its
    /// non-secret name, beside them (D15:776, 2249).
    #[test]
    fn a_fallback_key_is_classified_by_the_canonicalizer_phase_5_bound() {
        use crate::resolver::fallback::{self, FallbackSubject};
        let mut inputs = Inputs::complete();
        inputs.declaration()["messageFallback"] = json!({"de": ["en"]});
        inputs.resolve(|evaluation, _| {
            let outside = fallback::unit(
                FallbackSubject::Source(LocaleKey::Canonical(canonical("de"))),
                Check::RequestedMember,
            );
            assert_eq!(
                issues(&evaluation),
                [(
                    outside.clone(),
                    Code::MessageFallbackSourceNotRequested,
                    None
                )]
            );
            let pointer =
                |path: &str| json!({ "exact-json-pointer": format!("/profiles/web/{path}") });
            assert_eq!(
                observed_views(&evaluation, &outside)
                    .into_iter()
                    .map(|(primary, related)| (
                        primary["value"].clone(),
                        related
                            .iter()
                            .map(|view| view["value"].clone())
                            .collect::<Vec<_>>()
                    ))
                    .collect::<Vec<_>>(),
                [(
                    pointer("messageFallback/de"),
                    vec![pointer("requestedLocales")]
                )]
            );
        });
    }

    #[test]
    fn a_target_locale_outside_the_project_leaves_the_target_and_its_relation_unresolved() {
        let mut inputs = Inputs::complete();
        inputs.declaration()["targetProfiles"]["server"]["requestedLocales"] = json!(["en", "de"]);
        inputs.resolve(|evaluation, state| {
            let server: crate::model::TargetId = serde_json::from_value(json!("server")).unwrap();
            let browser: crate::model::TargetId = serde_json::from_value(json!("browser")).unwrap();
            let subset = targets::unit(TargetSubject::Target(server.clone()), Check::ProjectSubset);
            assert_eq!(
                issues(&evaluation),
                [(subset.clone(), Code::TargetLocalesNotSubset, None)]
            );
            // Only the relation's comparison of the two locale sets waits, and
            // the owners of the two groups the target belongs to.
            let compared = hydration::unit(
                HydrationSubject::Relation(group_id("web"), RelationKey::Pair(server, browser)),
                Check::SameRequestedLocales,
            );
            let waiting: Vec<&Unit> = evaluation
                .statuses()
                .filter(|(_, status)| **status != Status::Evaluated)
                .map(|(unit, _)| unit)
                .collect();
            assert_eq!(
                waiting,
                [
                    &dependency::unit(SemanticGroup::Targets),
                    &compared,
                    &dependency::unit(SemanticGroup::DeploymentGroups)
                ]
            );
            assert_eq!(absent(&state), ["targets", "hydration"]);
        });
    }

    #[test]
    fn the_relations_share_the_negotiation_profile_and_never_compare_it() {
        let mut inputs = Inputs::complete();
        // An alias to a locale the project does not request leaves no
        // negotiation profile; the relation is checked all the same.
        inputs.declaration()["localeNegotiation"]["aliases"] = json!({"de": "fr"});
        inputs.resolve(|evaluation, state| {
            let key = only_issue(&evaluation);
            assert_eq!(key.code(), Code::NegotiationAliasDestinationNotRequested);
            let hydration = state.hydration.as_ref().unwrap();
            assert_eq!(hydration.complete(), Ok(()));
            assert_eq!(
                keys(hydration.negotiation().as_ref().unwrap_err()),
                BTreeSet::from([key])
            );
            assert_eq!(absent(&state), ["negotiation"]);
            // The deployment groups are established without it.
            assert_eq!(
                unestablished(&evaluation),
                [SemanticGroup::LocaleNegotiation]
            );
        });
    }

    /// The dataset is part of the `localeCanonicalization` group, so data that
    /// disagrees with the admitted specification leaves the group, and every
    /// group a canonical locale is part of, unestablished; the slot itself was
    /// admitted.
    #[test]
    fn data_disagreeing_with_its_specification_leaves_the_canonicalization_group_waiting() {
        Inputs::complete().prepared(|configuration, invocation| {
            let mut pins = fixture_pins();
            pins.dataset.semantic_digest = "test-only-other-content-pin";
            let supported = SupportedSpecifications::revision_zero(
                ActiveProfileSpecification::fixture(ACTIVE),
                Supported::new(vec![pins]).unwrap(),
            );
            let invocation = Invocation {
                supported: &supported,
                ..*invocation
            };
            let mut evaluation = Evaluation::default();
            let mut set = None;
            invoke(
                &mut evaluation,
                configuration,
                &invocation,
                &FixtureProvider::new(),
                &mut set,
            )
            .unwrap();
            assert_eq!(only_issue(&evaluation).code(), Code::LocaleDataNotAdmitted);
            assert_eq!(
                unestablished(&evaluation),
                [
                    SemanticGroup::LocaleCanonicalization,
                    SemanticGroup::ProjectLocales,
                    SemanticGroup::LocaleNegotiation,
                    SemanticGroup::MessageFallback,
                    SemanticGroup::Coverage,
                    SemanticGroup::Targets,
                    SemanticGroup::DeploymentGroups
                ]
            );
        });
    }

    /// The optional policies are the confirmed declaration's even when both
    /// are `null`, which no reference check reads: the group waits on the
    /// confirmed policy alone.
    #[test]
    fn null_optional_policies_are_established_once_the_policy_is_confirmed() {
        let mut inputs = Inputs::complete();
        for slot in ["providerRouting", "glossarySet"] {
            inputs.declaration()["policies"][slot] = Value::Null;
        }
        inputs.resolve(|evaluation, _| {
            assert_eq!(issues(&evaluation), []);
            assert_eq!(unestablished(&evaluation), []);
        });
        let mut limits = policy_limits();
        let capability = capability_limits().locale_resolution.max_requested_locales;
        limits.locale_resolution.max_requested_locales = bound(capability.get() + 1);
        inputs.select_policy(resource_limit_policy(&limits));
        inputs.resolve(|evaluation, _| {
            let owner = dependency::unit(SemanticGroup::OptionalPolicies);
            let Some(Status::NotEvaluated(causes)) = evaluation.status(&owner) else {
                panic!("the owner was evaluated");
            };
            assert_eq!(keys(causes), BTreeSet::from([only_issue(&evaluation)]));
            assert_eq!(unestablished(&evaluation), AFTER_POLICY);
        });
    }

    #[test]
    fn a_server_without_its_024_role_leaves_only_the_hydration_unresolved() {
        let mut inputs = Inputs::complete();
        inputs.replace_target_profile("server", target_profile("storefront-server", &[]));
        inputs.resolve(|evaluation, state| {
            let [(unit, code, reason)] = issues(&evaluation).try_into().unwrap();
            assert!(matches!(unit.subject, Subject::Hydration(_)));
            assert_eq!(
                (code, reason),
                (
                    Code::HydrationRelationInvalid,
                    Some(Reason::ServerCapabilityMissing)
                )
            );
            assert_eq!(absent(&state), ["hydration"]);
            assert_eq!(
                unestablished(&evaluation),
                [SemanticGroup::DeploymentGroups]
            );
        });
    }

    #[test]
    fn a_target_in_no_group_leaves_only_the_groups_unresolved() {
        let mut inputs = Inputs::complete();
        inputs.declaration()["deploymentGroups"]["web"]["members"] = json!(["browser"]);
        inputs.declaration()["deploymentGroups"]["web"]["hydrationRelations"] = json!([]);
        inputs.resolve(|evaluation, state| {
            let server: crate::model::TargetId = serde_json::from_value(json!("server")).unwrap();
            let placed = groups::unit(GroupSubject::Assignment(server), Check::SingleGroup);
            assert_eq!(
                issues(&evaluation),
                [(placed, Code::TargetUnassigned, None)]
            );
            assert_eq!(absent(&state), ["groups"]);
            assert_eq!(
                unestablished(&evaluation),
                [SemanticGroup::DeploymentGroups]
            );
        });
    }

    #[test]
    fn reordering_sets_objects_and_the_collection_changes_no_semantic_state() {
        let complete = Inputs::complete();
        // A duplicate requested locale and a missing trust policy, so that
        // issues and causes are compared too.
        let mut blocked = Inputs::complete();
        blocked.declaration()["requestedLocales"] = json!(["ja", "en-US", "en", "EN-us"]);
        blocked.members.remove(TRUST);
        for authored in [complete, blocked] {
            let reordered = authored.reordered();
            assert_ne!(
                serde_json::to_vec(&authored.config).unwrap(),
                serde_json::to_vec(&reordered.config).unwrap()
            );
            authored.resolve(|first_evaluation, first| {
                reordered.resolve(|second_evaluation, second| {
                    assert_eq!(unplaced(&first_evaluation), unplaced(&second_evaluation));
                    assert_eq!(unlocated(first), unlocated(second));
                });
            });
        }
    }

    /// `bounding` submits the Policy it selects in place of the selected one
    /// wherever the collection holds it, last once `reordered` reversed the
    /// collection, so every other member stays submitted and the invocation
    /// confirms the Policy it pins.
    #[test]
    fn bounding_reordered_inputs_replaces_the_selected_policy_where_it_is() {
        let reordered = Inputs::complete().reordered();
        let index = reordered
            .members
            .iter()
            .position(|member| tuple_of(member) == reordered.pinned)
            .unwrap();
        assert_eq!(index, reordered.members.len() - 1);
        let id = BoundId {
            group: Group::ConfigurationInput,
            name: "maxProfileIdBytes",
        };
        let bounded = reordered.bounding(&[], &[(id, 32)]);
        let policy = resource_limit_policy(&tightened(&policy_limits(), &[(id, 32)]));
        assert_eq!(bounded.pinned, tuple_of(&policy));
        let mut members = reordered.members.clone();
        members[index] = policy;
        assert_eq!(bounded.members, members);
        bounded.resolve(|evaluation, state| {
            assert_eq!(issues(&evaluation), []);
            assert_eq!(state.policy.authority, bound(32));
        });
    }

    /// Phase 14 needs every semantic result: there is a view exactly when
    /// every semantic group is established, and otherwise the view waits on
    /// what the groups' owners wait on, whichever groups those are.
    #[test]
    fn the_view_exists_exactly_when_every_group_is_established() {
        type Edit = (&'static str, fn(&mut Value));
        let mut unasserted = Inputs::complete();
        unasserted.specifications.project_profile = None;
        let mut unconfirmed = Inputs::complete();
        let mut limits = policy_limits();
        let capability = capability_limits().locale_resolution.max_requested_locales;
        limits.locale_resolution.max_requested_locales = bound(capability.get() + 1);
        unconfirmed.select_policy(resource_limit_policy(&limits));
        let mut untrusted = Inputs::complete();
        untrusted.members.remove(TRUST);
        let mut roleless = Inputs::complete();
        roleless.replace_target_profile("server", target_profile("storefront-server", &[]));
        let mut cases = vec![
            ("complete", Inputs::complete()),
            ("no project-profile assertion", unasserted),
            ("a policy needing more than the capability", unconfirmed),
            ("a trust policy not submitted", untrusted),
            ("a server without its role", roleless),
        ];
        let edits: [Edit; 5] = [
            ("a target in no group", |declaration| {
                declaration["deploymentGroups"]["web"]["members"] = json!(["browser"]);
            }),
            ("a surface class outside the vocabulary", |declaration| {
                declaration["coverage"]["rules"][1]["intentSurfaceClasses"] = json!(["unknown"]);
            }),
            ("a target locale outside the project", |declaration| {
                declaration["targetProfiles"]["browser"]["requestedLocales"] = json!(["de"]);
            }),
            ("a fallback to its own key", |declaration| {
                declaration["messageFallback"]["ja"] = json!(["ja"]);
            }),
            ("a glossary set not submitted", |declaration| {
                declaration["policies"]["glossarySet"]["revision"] = json!("2");
            }),
        ];
        for (name, edit) in edits {
            let mut inputs = Inputs::complete();
            edit(inputs.declaration());
            cases.push((name, inputs));
        }
        for (name, inputs) in cases {
            inputs.viewed(|evaluation, view| {
                let groups = unestablished(&evaluation);
                let waiting: BTreeSet<IssueKey> = groups
                    .iter()
                    .flat_map(|&group| match evaluation.status(&dependency::unit(group)) {
                        Some(Status::NotEvaluated(causes)) => keys(causes),
                        _ => unreachable!("{group:?} is unestablished"),
                    })
                    .collect();
                assert_eq!(view.is_ok(), name == "complete", "{name}");
                match view {
                    Ok(_) => assert_eq!(groups, [], "{name}"),
                    Err(causes) => {
                        assert_ne!(groups, [], "{name}");
                        assert_eq!(keys(&causes), waiting, "{name}");
                    }
                }
            });
        }
    }

    /// The semantic phases read the declaration Phase 3 selected from the
    /// analysis: checks that select a declaration the analysis does not admit
    /// are a resolver bug.
    #[test]
    fn a_selected_declaration_the_analysis_lacks_is_a_resolver_bug() {
        let complete = Inputs::complete();
        let capability = capability_limits();
        let analysis = analyze(&complete.config, &capability);
        let selector = SelectorInput::string(
            "web",
            capability
                .configuration_input
                .structural
                .max_profile_id_bytes,
        );
        let Ok(Selection::Selected(selection)) = analysis.select(&selector) else {
            panic!("web is not selected");
        };
        let fixture = FileFixture::new(&capability);
        let configuration = |config: &Value, analysis| {
            let file = fixture.file(entry_input(config, &capability).0);
            SelectedConfiguration::new(
                &ValueAdmissions::default(),
                (analysis, &file),
                (&selector, &selection),
                Ok(()),
            )
            .map(|_| ())
        };
        assert!(configuration(&complete.config, &analysis).is_ok());
        let mut partial = complete.config.clone();
        partial["profiles"]["web"]["projectId"] = json!("Store Front");
        let analyzed = analyze(&partial, &capability);
        assert_eq!(
            configuration(&partial, &analyzed).err(),
            Some(ResolverInvariant::Structural(
                StructuralInvariant::UnreportedFailure
            ))
        );
    }

    /// The structural Findings of phase 2, as their units and codes.
    fn structural_issues(evaluation: &Evaluation) -> Vec<(Unit, Code)> {
        issues(evaluation)
            .into_iter()
            .filter(|(unit, _, _)| unit.phase == Phase::StructuralAdmission)
            .map(|(unit, code, _)| (unit, code))
            .collect()
    }

    /// A structural failure outside the selected declaration leaves the
    /// semantic phases running over the declaration, while profile
    /// projection, which needs a complete `IntlifyConfig`, waits on the
    /// structural Findings (D15:2142, 2367, 3434).
    #[test]
    fn a_failure_outside_the_selected_declaration_holds_back_only_the_view() {
        let unselected = |inputs: &mut Inputs| {
            let mut other = inputs.declaration().clone();
            other["requestedLocales"] = json!("en");
            inputs.config["profiles"]["admin"] = other;
        };
        let undeclared = |inputs: &mut Inputs| inputs.config["extra"] = json!(true);
        for edit in [unselected, undeclared] {
            let mut inputs = Inputs::complete();
            edit(&mut inputs);
            inputs.viewed(|evaluation, view| {
                let structural = structural_issues(&evaluation);
                assert_eq!(structural.len(), 1, "{structural:?}");
                let expected: BTreeSet<IssueKey> = evaluation
                    .issues()
                    .filter(|key| key.unit().phase == Phase::StructuralAdmission)
                    .cloned()
                    .collect();
                assert_eq!(keys(&view.unwrap_err()), expected);
                // The semantic phases ran over the selected declaration.
                assert!(evaluation
                    .statuses()
                    .any(|(unit, _)| unit.phase == Phase::HydrationValidation));
                assert!(issues(&evaluation)
                    .iter()
                    .all(|(unit, _, _)| unit.phase == Phase::StructuralAdmission));
            });
        }
    }

    /// An edit that breaks the structure of a declaration the selector does
    /// not select gives a new outcome, blocked on that declaration's one
    /// structural Finding with a complete bundle, and composes nothing of it
    /// into the selected declaration: every Shared record of the checks after
    /// structural admission is the one the configuration gives while the
    /// other declaration is valid (D15:2131, 2142, 2367, 2587, 3434; plan 025
    /// decision #212).
    #[test]
    fn breaking_an_unselected_declaration_blocks_without_composing_it() {
        use crate::resolver::outcome::tests::{identity, reporting, shown, Owned};
        use crate::resolver::structural_admission::{EntryKey, Field, StructuralSubject as S};
        let with_admin = |locales: Value| {
            let mut inputs = Inputs::complete();
            let mut admin = inputs.declaration().clone();
            admin["requestedLocales"] = locales;
            inputs.config["profiles"]["admin"] = admin;
            reporting(&inputs, &[], &[]).outcome(None, identity(), |evaluation, _, outcome| {
                assert_eq!(outcome.identity, identity());
                let evidence = &outcome.evidence;
                assert!(evidence.complete() && evidence.unavailable().is_empty());
                let shown = shown(&outcome);
                let (records, _) = shown.shared.unwrap();
                let later: Owned = records
                    .into_iter()
                    .filter(|(owner, ..)| owner.phase > Phase::StructuralAdmission)
                    .collect();
                (issues(evaluation), shown.checked.is_some(), later)
            })
        };
        let (none, checked, valid) = with_admin(json!(["ja", "en"]));
        assert!(none.is_empty() && checked);
        let (found, checked, broken) = with_admin(json!("en"));
        let admin = S::ProfileField(
            EntryKey::identity("admin"),
            Field::of(&["requestedLocales"]),
        );
        let unit = Unit {
            phase: Phase::StructuralAdmission,
            subject: Subject::Structural(admin),
            check: Check::FieldType,
        };
        assert_eq!(found, [(unit, Code::FieldTypeInvalid, None)]);
        assert!(!checked);
        assert!(!broken.is_empty());
        assert_eq!(broken, valid);
    }

    /// A structural failure inside the selected declaration holds back only
    /// the checks that read its field, with its Finding as their cause, while
    /// the semantic phases read the declaration's other fields (D15:2367,
    /// 3340; plan 025 interpretation #218). Profile projection still waits on
    /// the whole root (D15:2142).
    #[test]
    fn a_failure_inside_the_selected_declaration_holds_back_only_what_reads_it() {
        let mut inputs = Inputs::complete();
        inputs.declaration()["defaultSourceLocale"] = json!(1);
        inputs.attempt(|evaluation, state| {
            let state = state.unwrap().unwrap();
            let structural = structural_issues(&evaluation);
            assert_eq!(structural.len(), 1, "{structural:?}");
            assert!(state.structure.is_err() && state.identity.is_ok());
            let held: Vec<(Unit, Status)> = statuses(&evaluation)
                .into_iter()
                .filter(|(unit, status)| {
                    unit.phase > Phase::ProfileSelection
                        && matches!(status, Status::NotEvaluated(_))
                })
                .collect();
            assert!(held
                .iter()
                .any(|(unit, _)| unit.phase == Phase::LocaleResolution));
            for (unit, status) in &held {
                let Status::NotEvaluated(causes) = status else {
                    unreachable!()
                };
                assert!(
                    causes
                        .keys()
                        .all(|cause| cause.unit().phase == Phase::StructuralAdmission),
                    "{unit:?} waits on {causes:?}"
                );
            }
        });
    }

    /// Phase 4's two units of `subject`: under the capability and as the
    /// admitted policy's recheck.
    fn stages(subject: ArtifactSubject) -> [Unit; 2] {
        [
            resource_policy::unit(
                PolicySubject::Capability(subject.clone()),
                Check::WithinCapability,
            ),
            resource_policy::unit(PolicySubject::Recheck(subject), Check::WithinBound),
        ]
    }

    fn slot(slot: PolicySlot) -> ArtifactSubject {
        ArtifactSubject::Reference(ReferenceField::Policy(slot))
    }

    fn profile_of(id: &str) -> ArtifactSubject {
        let id = serde_json::from_value(json!(id)).unwrap();
        ArtifactSubject::Reference(ReferenceField::TargetProfile(id))
    }

    /// After `edit`, the selection is confirmed beside the one Finding: each
    /// of `waiting` waits on it, and the count, the total, and each of
    /// `checked` are evaluated.
    fn confirmed_beside(
        edit: impl FnOnce(&mut Value),
        waiting: &[ArtifactSubject],
        checked: &[ArtifactSubject],
    ) {
        let mut inputs = Inputs::complete();
        edit(inputs.declaration());
        inputs.resolve(|evaluation, state| {
            let found = only_issue(&evaluation);
            assert!(state.policy.confirmed.is_ok() && state.identity.is_ok());
            assert!(state.structure.is_err());
            for unit in waiting.iter().cloned().flat_map(stages) {
                let Some(Status::NotEvaluated(causes)) = evaluation.status(&unit) else {
                    panic!("{unit:?} is not waiting");
                };
                assert_eq!(keys(causes), BTreeSet::from([found.clone()]), "{unit:?}");
            }
            let totals = [ArtifactSubject::References, ArtifactSubject::ReferenceBytes];
            for unit in checked.iter().cloned().chain(totals).flat_map(stages) {
                assert_eq!(
                    evaluation.status(&unit),
                    Some(&Status::Evaluated),
                    "{unit:?}"
                );
            }
        });
    }

    /// One policy slot that fails, or is missing, has no canonical bytes: its
    /// byte checks wait on its Finding, an independent slot's are evaluated,
    /// and the selection is confirmed (D15:1918, 1924, 2149, 2367; plan 025
    /// decision #342).
    #[test]
    fn a_policy_slot_that_fails_leaves_the_selection_confirmed() {
        let (trust, approval) = (slot(PolicySlot::Trust), slot(PolicySlot::Approval));
        confirmed_beside(
            |app| app["policies"]["trust"] = json!(42),
            std::slice::from_ref(&trust),
            &[approval.clone(), profile_of("server")],
        );
        confirmed_beside(
            |app| {
                app["policies"].as_object_mut().unwrap().remove("trust");
            },
            &[trust],
            &[approval],
        );
    }

    /// A failure in a target's field other than `profile` is no failure of a
    /// reference, and one target's `profile` that fails leaves the other's
    /// checked (D15:1921, 1924, 2367; plan 025 decision #342).
    #[test]
    fn a_target_that_fails_leaves_the_selection_confirmed() {
        confirmed_beside(
            |app| app["targetProfiles"]["server"]["requestedLocales"] = json!(["en", 42]),
            &[],
            &[profile_of("browser"), profile_of("server")],
        );
        confirmed_beside(
            |app| app["targetProfiles"]["server"]["profile"] = json!(42),
            &[profile_of("server")],
            &[profile_of("browser")],
        );
    }

    /// A reference that failed still occurs, and past the bound it is a
    /// Finding, which holds the selection back. A missing one does not
    /// occur, so eight are within a bound of eight (D15:1736, 1918, 1924).
    #[test]
    fn a_bound_past_its_limit_beside_a_failed_reference_holds_the_selection_back() {
        let count = BoundId {
            group: Group::ArtifactAdmission,
            name: "maxArtifactReferences",
        };
        let mut over = Inputs::complete().bounding(&[], &[(count, 8)]);
        over.declaration()["targetProfiles"]["server"]["profile"] = json!(42);
        over.resolve(|evaluation, state| {
            assert!(state.policy.confirmed.is_err());
            let [_, recheck] = stages(ArtifactSubject::References);
            let key = evaluation.issues().find(|key| *key.unit() == recheck);
            assert_eq!(
                evaluation
                    .limit(key.unwrap())
                    .map(|exceeded| exceeded.actual),
                Some(crate::resolver::evaluation::Actual::Exact(9))
            );
        });
        let mut within = Inputs::complete().bounding(&[], &[(count, 8)]);
        let policies = within.declaration()["policies"].as_object_mut();
        policies.unwrap().remove("trust");
        within.resolve(|_, state| assert!(state.policy.confirmed.is_ok()));
    }

    /// The unit of `check` over a Phase 7 `subject`.
    fn phase_7(subject: crate::resolver::locales::LocaleSubject, check: Check) -> Unit {
        crate::resolver::locales::unit(subject, check)
    }

    /// The bound counts spellings, so no field that failed holds it back: a
    /// value that is no string is no spelling, an invalid string is one, and
    /// an entry under a key that failed still holds its own (D15:1896, 1907,
    /// 2184, 3348).
    #[test]
    fn the_occurrence_bound_counts_the_spellings_beside_a_field_that_failed() {
        use crate::resolver::evaluation::Actual;
        use crate::resolver::locales::LocaleSubject as L;
        type Edit = fn(&mut Value);
        let total = |edit: Edit| {
            let mut inputs = Inputs::complete();
            edit(inputs.declaration());
            inputs.resolve(|evaluation, state| {
                let bound = phase_7(L::Occurrences, Check::WithinBound);
                assert_eq!(evaluation.status(&bound), Some(&Status::Evaluated));
                state.locales.occurrence_total
            })
        };
        let Some(Actual::Exact(whole)) = total(|_| {}) else {
            panic!("the complete declaration has an exact total");
        };
        let cases: [(u64, u64, Edit); 16] = [
            (1, 0, |app| app["requestedLocales"] = json!(["ja", 42])),
            (0, 0, |app| app["requestedLocales"] = json!(["ja", "zz"])),
            (2, 0, |app| app["requestedLocales"] = json!("en")),
            (1, 0, |app| {
                app.as_object_mut()
                    .unwrap()
                    .remove("defaultRequestedLocale");
            }),
            (1, 0, |app| app["defaultRequestedLocale"] = json!(42)),
            (1, 0, |app| app["defaultSourceLocale"] = json!(42)),
            (1, 0, |app| {
                app["localeNegotiation"]["aliases"]["de"] = json!(1);
            }),
            (2, 0, |app| app["localeNegotiation"] = json!(7)),
            (0, 0, |app| app["messageFallback"]["ja"] = json!(["en", 7])),
            (1, 0, |app| app["messageFallback"]["ja"] = json!([7])),
            (1, 0, |app| {
                app["coverage"]["rules"][0]["requestedLocales"] = json!([7]);
            }),
            (2, 0, |app| app["coverage"] = json!(false)),
            (1, 0, |app| {
                app["targetProfiles"]["server"]["requestedLocales"] = json!(["ja", 7]);
            }),
            (1, 0, |app| {
                app["targetProfiles"]["browser"]["defaultRequestedLocale"] = json!(7);
            }),
            (2, 0, |app| app["targetProfiles"]["server"] = json!(7)),
            (0, 1, |app| {
                app["targetProfiles"]["Web App"] =
                    json!({"profile": 1, "requestedLocales": ["en", 7]});
            }),
        ];
        for (index, (fewer, more, edit)) in cases.into_iter().enumerate() {
            assert_eq!(
                total(edit),
                Some(Actual::Exact(whole - fewer + more)),
                "{index}"
            );
        }
    }

    /// One element of the project's `requestedLocales` that failed leaves the
    /// others spelled, each repeat still found and each correction suggested
    /// (D15:1382, 2367, 3445).
    #[test]
    fn a_requested_element_that_failed_leaves_its_siblings_checked() {
        use crate::resolver::locales::fixtures::key;
        use crate::resolver::locales::LocaleSubject as L;
        let mut inputs = Inputs::complete();
        inputs.declaration()["requestedLocales"] = json!(["ja", 42, "EN", "en"]);
        inputs.resolve(|evaluation, state| {
            let en = L::RequestedLocale(key("en"));
            let ja = L::RequestedLocale(key("ja"));
            assert!(evaluation
                .issues()
                .any(|issue| *issue.unit() == phase_7(en.clone(), Check::UniqueLocale)));
            assert_eq!(
                evaluation.status(&phase_7(ja, Check::UniqueLocale)),
                Some(&Status::Evaluated)
            );
            assert!(evaluation
                .notices()
                .any(|notice| *notice.unit() == phase_7(en.clone(), Check::CanonicalLocale)));
            assert!(state.locales.project.is_err());
        });
    }

    /// A missing project default holds back only what needs it: the target
    /// locale Phase 7 finds invalid is a Finding beside it, and the default's
    /// own checks are named and wait on it (D15:1157, 1171, 2365, 2367, 2416,
    /// 3446; plan 025 interpretation #252).
    #[test]
    fn a_missing_default_leaves_an_invalid_target_locale_found() {
        use crate::resolver::locales::fixtures::rejected;
        use crate::resolver::locales::LocaleSubject as L;
        let mut inputs = Inputs::complete();
        let app = inputs.declaration();
        app.as_object_mut()
            .unwrap()
            .remove("defaultRequestedLocale");
        app["targetProfiles"]["server"]["requestedLocales"] = json!(["ja", "zz"]);
        let server: crate::model::TargetId = serde_json::from_value(json!("server")).unwrap();
        inputs.resolve(|evaluation, _| {
            let found: Vec<Code> = evaluation.issues().map(IssueKey::code).collect();
            assert_eq!(found, [Code::RequiredFieldMissing, Code::LocaleInvalid]);
            let missing = evaluation
                .issues()
                .find(|key| key.code() == Code::RequiredFieldMissing)
                .unwrap()
                .clone();
            let invalid = phase_7(
                L::TargetRequestedLocale(server.clone(), rejected()),
                Check::CanonicalLocale,
            );
            assert!(evaluation.issues().any(|key| *key.unit() == invalid));
            for check in [Check::CanonicalLocale, Check::RequestedMember] {
                let Some(Status::NotEvaluated(causes)) =
                    evaluation.status(&phase_7(L::DefaultRequestedLocale, check))
                else {
                    panic!("the default's {check:?} is not waiting");
                };
                assert_eq!(keys(causes), BTreeSet::from([missing.clone()]));
            }
            let inherited = crate::resolver::targets::unit(
                crate::resolver::targets::TargetSubject::Target(server.clone()),
                Check::SupportedDefault,
            );
            let Some(Status::NotEvaluated(causes)) = evaluation.status(&inherited) else {
                panic!("the inherited default is not waiting");
            };
            assert!(keys(causes).contains(&missing));
        });
    }

    /// Membership in a set with an element that failed is proved from the
    /// elements it holds: one of them passes, and any other waits, since
    /// proving it outside needs the identity that element lacks (D15:2365,
    /// 2367).
    #[test]
    fn a_set_with_a_failed_element_proves_only_what_it_holds() {
        use SubjectKind as K;
        let membership = |requested: Value, check: Check| {
            let mut inputs = Inputs::complete();
            inputs.declaration()["requestedLocales"] = requested;
            inputs.resolve(|evaluation, _| {
                let found = only_issue(&evaluation);
                assert_eq!(found.code(), Code::FieldTypeInvalid);
                let mut held = BTreeMap::<K, bool>::new();
                for (unit, status) in evaluation.statuses() {
                    if unit.check == check {
                        let waits = matches!(status, Status::NotEvaluated(_));
                        if let Status::NotEvaluated(causes) = status {
                            assert_eq!(keys(causes), BTreeSet::from([found.clone()]));
                        }
                        held.insert(unit.subject.kind(), waits);
                    }
                }
                held
            })
        };
        // Every locale named is held: nothing waits.
        let held = membership(json!(["ja", "en", 42]), Check::RequestedMember);
        assert_eq!(
            held,
            BTreeMap::from([
                (K::DefaultRequestedLocale, false),
                (K::NegotiationDestination, false),
                (K::FallbackSource, false),
                (K::CoverageLocale, false),
            ])
        );
        assert_eq!(
            membership(json!(["ja", "en", 42]), Check::ProjectSubset),
            BTreeMap::from([(K::Target, false)])
        );
        // `ja` is not held, so what names it waits, and what names `en` passes.
        let held = membership(json!(["en", 42]), Check::RequestedMember);
        assert_eq!(
            held,
            BTreeMap::from([
                (K::DefaultRequestedLocale, false),
                (K::NegotiationDestination, false),
                (K::FallbackSource, true),
                (K::CoverageLocale, true),
            ])
        );
        assert_eq!(
            membership(json!(["en", 42]), Check::ProjectSubset),
            BTreeMap::from([(K::Target, true)])
        );
    }

    /// A target with no admitted value in its set waits on why, and on what
    /// the project's set lacks, which it would need to compare a locale it
    /// does not hold (D15:2184, 2365, 2367, 3354).
    #[test]
    fn a_target_without_its_set_waits_on_the_projects_missing_element_too() {
        let mut inputs = Inputs::complete();
        let app = inputs.declaration();
        app["requestedLocales"] = json!(["ja", "en", 42]);
        app["targetProfiles"]["server"]["requestedLocales"] = json!(["zz"]);
        let server = serde_json::from_value(json!("server")).unwrap();
        inputs.resolve(|evaluation, _| {
            let subset = crate::resolver::targets::unit(
                crate::resolver::targets::TargetSubject::Target(server),
                Check::ProjectSubset,
            );
            let every: BTreeSet<IssueKey> = evaluation.issues().cloned().collect();
            assert_eq!(every.len(), 2);
            let Some(Status::NotEvaluated(causes)) = evaluation.status(&subset) else {
                panic!("the subset is not waiting");
            };
            assert_eq!(keys(causes), every);
        });
    }

    /// The rows that wait on one target's failed `profile`: what needs every
    /// target, and the one relation comparing that Target Profile.
    const WAITING_ON_A_TARGET: &str = "dependency.deployment-groups.established dependency.targets.established hydration.relation.static-compatibility";

    /// The rows of the failed reference: its own bounds and the checks that
    /// need its Target Profile wait on it, and the other target's run.
    const PER_TARGET: &str = "artifact.target-profile-reference.resolved hydration.endpoint.role resource-policy.target-profile-reference.within-bound resource-policy.target-profile-reference.within-capability target.requested-locales.directed";

    /// End to end, one target's failed `profile` leaves one Finding, and each
    /// row is evaluated or waits on it alone; only the failed reference's own
    /// rows are both (D15:1924, 2149, 2367, 3446).
    #[test]
    fn a_failed_target_profile_changes_only_what_reads_the_targets() {
        let rows = |evaluation: &Evaluation| {
            let mut rows: BTreeMap<&str, [usize; 2]> = BTreeMap::new();
            for (unit, status) in evaluation.statuses() {
                if unit.phase >= Phase::ResourcePolicyAdmission {
                    let at = usize::from(*status != Status::Evaluated);
                    rows.entry(row_of(unit)).or_default()[at] += 1;
                }
            }
            rows
        };
        let split = |names: &'static str| names.split(' ').collect::<BTreeSet<&str>>();
        let complete = Inputs::complete().resolve(|evaluation, _| rows(&evaluation));
        let mut inputs = Inputs::complete();
        inputs.declaration()["targetProfiles"]["server"]["profile"] = json!(42);
        inputs.resolve(|evaluation, state| {
            let found = only_issue(&evaluation);
            assert!(state.policy.confirmed.is_ok() && state.structure.is_err());
            for (unit, status) in evaluation.statuses() {
                if let Status::NotEvaluated(causes) = status {
                    assert_eq!(keys(causes), BTreeSet::from([found.clone()]), "{unit:?}");
                }
            }
            let after = rows(&evaluation);
            let names = |kept: fn(&[usize; 2]) -> bool| -> BTreeSet<&str> {
                let kept = after.iter().filter(|(_, counts)| kept(counts));
                kept.map(|(row, _)| *row).collect()
            };
            assert_eq!(
                names(|[on, off]| *on == 0 && *off > 0),
                split(WAITING_ON_A_TARGET)
            );
            assert!(complete.keys().all(|row| after.contains_key(row)));
            assert_eq!(names(|[on, off]| *on > 0 && *off > 0), split(PER_TARGET));
        });
    }

    /// Each of `units` was evaluated.
    fn all_evaluated(evaluation: &Evaluation, units: &[Unit]) {
        for unit in units {
            assert_eq!(
                evaluation.status(unit),
                Some(&Status::Evaluated),
                "{unit:?}"
            );
        }
    }

    /// `unit` waits on exactly `causes`.
    fn waits_on_exactly(evaluation: &Evaluation, unit: &Unit, causes: &BTreeSet<IssueKey>) {
        let Some(Status::NotEvaluated(found)) = evaluation.status(unit) else {
            panic!("{unit:?} is not waiting");
        };
        assert_eq!(&keys(found), causes, "{unit:?}");
    }

    /// One element of a target's `requestedLocales` that failed holds back
    /// neither its sibling nor the other target's checks, and the map's bound
    /// and each Target ID's are evaluated (D15:1930, 1931, 2367, 3445).
    #[test]
    fn a_target_element_that_failed_leaves_every_other_locale_check() {
        use crate::resolver::locales::fixtures::{key, target};
        use crate::resolver::locales::LocaleSubject as L;
        let mut inputs = Inputs::complete();
        inputs.declaration()["targetProfiles"]["server"]["requestedLocales"] = json!(["ja", 42]);
        inputs.resolve(|evaluation, _| {
            only_issue(&evaluation);
            let locale = |id: &str, spelling: &str, check| {
                phase_7(L::TargetRequestedLocale(target(id), key(spelling)), check)
            };
            all_evaluated(
                &evaluation,
                &[
                    phase_7(L::TargetProfiles, Check::WithinBound),
                    phase_7(L::TargetId(target("browser")), Check::WithinBound),
                    phase_7(L::TargetId(target("server")), Check::WithinBound),
                    locale("browser", "ja", Check::CanonicalLocale),
                    locale("browser", "en", Check::UniqueLocale),
                    locale("server", "ja", Check::CanonicalLocale),
                    locale("server", "ja", Check::UniqueLocale),
                    phase_7(
                        L::TargetDefaultRequestedLocale(target("browser")),
                        Check::CanonicalLocale,
                    ),
                ],
            );
        });
    }

    /// An entry whose value is no object keeps its key's subject: its bound
    /// is checked and its default's unit waits on why, while the other
    /// target's checks run (D15:1171, 2184, 2367).
    #[test]
    fn a_target_entry_that_is_no_object_keeps_its_subject() {
        use crate::resolver::locales::fixtures::{key, target};
        use crate::resolver::locales::LocaleSubject as L;
        let mut inputs = Inputs::complete();
        inputs.declaration()["targetProfiles"]["server"] = json!(7);
        inputs.resolve(|evaluation, _| {
            let found = BTreeSet::from([only_issue(&evaluation)]);
            all_evaluated(
                &evaluation,
                &[
                    phase_7(L::TargetId(target("server")), Check::WithinBound),
                    phase_7(
                        L::TargetRequestedLocale(target("browser"), key("ja")),
                        Check::CanonicalLocale,
                    ),
                ],
            );
            waits_on_exactly(
                &evaluation,
                &phase_7(
                    L::TargetDefaultRequestedLocale(target("server")),
                    Check::CanonicalLocale,
                ),
                &found,
            );
        });
    }

    /// A map that is no object has no entry to read, but its bound's unit is
    /// named by its path, and waits on why (D15:1157, 1171, 1930).
    #[test]
    fn a_target_map_that_failed_still_records_its_bound() {
        use crate::resolver::locales::LocaleSubject as L;
        let without: [fn(&mut Value); 2] = [
            |app| app["targetProfiles"] = json!(7),
            |app| {
                app.as_object_mut().unwrap().remove("targetProfiles");
            },
        ];
        for edit in without {
            let mut inputs = Inputs::complete();
            edit(inputs.declaration());
            inputs.resolve(|evaluation, _| {
                let found = BTreeSet::from([only_issue(&evaluation)]);
                let bound = phase_7(L::TargetProfiles, Check::WithinBound);
                waits_on_exactly(&evaluation, &bound, &found);
                assert!(!evaluation
                    .statuses()
                    .any(|(unit, _)| matches!(unit.subject, Subject::Locale(L::TargetId(_)))));
            });
        }
    }

    /// An entry whose key fails counts under `maxTargetProfiles` and has no
    /// later subject of its own, while the entries beside it wait only on the
    /// map's bound (D15:1930, 2184, 3354).
    #[test]
    fn a_target_whose_key_fails_counts_without_a_subject() {
        use crate::resolver::evaluation::Actual;
        use crate::resolver::locales::fixtures::target;
        use crate::resolver::locales::LocaleSubject as L;
        let mut inputs = Inputs::complete();
        let copy = inputs.declaration()["targetProfiles"]["server"].clone();
        inputs.declaration()["targetProfiles"]["Web App"] = copy;
        let mut limits = policy_limits();
        limits.target_grouping.max_target_profiles = bound(2);
        inputs.select_policy(resource_limit_policy(&limits));
        inputs.resolve(|evaluation, _| {
            let map = phase_7(L::TargetProfiles, Check::WithinBound);
            let over = evaluation
                .issues()
                .find(|key| *key.unit() == map)
                .expect("the entry with the invalid key counts")
                .clone();
            assert_eq!(
                evaluation.limit(&over).map(|exceeded| exceeded.actual),
                Some(Actual::Exact(3))
            );
            for id in ["browser", "server"] {
                waits_on_exactly(
                    &evaluation,
                    &phase_7(L::TargetId(target(id)), Check::WithinBound),
                    &BTreeSet::from([over.clone()]),
                );
            }
            let ids = evaluation
                .statuses()
                .filter(|(unit, _)| matches!(unit.subject, Subject::Locale(L::TargetId(_))))
                .count();
            assert_eq!(ids, 2);
        });
    }

    /// An entry whose key fails has no reference to resolve, while what
    /// needs every reference, or every target, waits on its Finding, and
    /// each target beside it resolves (D15:2184, 2367, 3354).
    #[test]
    fn a_target_whose_key_fails_holds_back_what_needs_every_target() {
        use crate::resolver::locales::fixtures::target;
        let mut inputs = Inputs::complete();
        let copy = inputs.declaration()["targetProfiles"]["server"].clone();
        inputs.declaration()["targetProfiles"]["Web App"] = copy;
        inputs.resolve(|evaluation, state| {
            let found = BTreeSet::from([only_issue(&evaluation)]);
            assert_eq!(keys(state.references.as_ref().unwrap_err()), found);
            assert_eq!(keys(state.bindings.as_ref().unwrap_err()), found);
            let targets = state.targets.as_ref().unwrap();
            for id in ["browser", "server"] {
                assert!(targets.get(&target(id)).unwrap().resolved.is_ok(), "{id}");
            }
            assert_eq!(keys(targets.complete().as_ref().unwrap_err()), found);
        });
    }

    /// One target's failed `profile` holds back only the checks that need
    /// that profile: its reference and its directions wait on the Finding,
    /// and every other check of it, and of the other target, is evaluated
    /// (D15:2139, 2367, 3445, 3446).
    #[test]
    fn a_target_profile_that_failed_stops_only_what_needs_it() {
        use crate::resolver::locales::fixtures::target;
        let edits: [fn(&mut Value); 2] = [
            |server| server["profile"] = json!(42),
            |server| {
                server.as_object_mut().unwrap().remove("profile");
            },
        ];
        for edit in edits {
            let mut inputs = Inputs::complete();
            edit(&mut inputs.declaration()["targetProfiles"]["server"]);
            inputs.resolve(|evaluation, state| {
                let found = BTreeSet::from([only_issue(&evaluation)]);
                let check =
                    |id: &str, check| targets::unit(TargetSubject::Target(target(id)), check);
                let reference =
                    |id: &str| exact_resolution::unit(ReferenceField::TargetProfile(target(id)));
                all_evaluated(
                    &evaluation,
                    &[
                        reference("browser"),
                        check("browser", Check::ProjectSubset),
                        check("browser", Check::SupportedDefault),
                        check("browser", Check::DirectedLocales),
                        check("server", Check::ProjectSubset),
                        check("server", Check::SupportedDefault),
                    ],
                );
                waits_on_exactly(&evaluation, &reference("server"), &found);
                waits_on_exactly(
                    &evaluation,
                    &check("server", Check::DirectedLocales),
                    &found,
                );
                let targets = state.targets.as_ref().unwrap();
                assert!(targets.get(&target("browser")).unwrap().resolved.is_ok());
                assert!(targets.get(&target("server")).unwrap().resolved.is_err());
                assert_eq!(keys(targets.complete().as_ref().unwrap_err()), found);
            });
        }
    }

    /// What Phase 11 records of the target `server` once its
    /// `requestedLocales` is `requested` and the project's `project`: the
    /// three checks' results, as `None` when evaluated and otherwise the
    /// number of causes, and the codes found.
    fn server_checks(project: Value, requested: Value) -> (Vec<Option<usize>>, Vec<Code>) {
        use crate::resolver::locales::fixtures::target;
        let mut inputs = Inputs::complete();
        let app = inputs.declaration();
        app["requestedLocales"] = project;
        app["targetProfiles"]["server"]["requestedLocales"] = requested;
        inputs.resolve(|evaluation, _| {
            let results = [
                Check::ProjectSubset,
                Check::SupportedDefault,
                Check::DirectedLocales,
            ]
            .map(|check| {
                let unit = targets::unit(TargetSubject::Target(target("server")), check);
                match evaluation.status(&unit) {
                    Some(Status::Evaluated) => None,
                    Some(Status::NotEvaluated(causes)) => Some(keys(causes).len()),
                    None => panic!("{unit:?} was not recorded"),
                }
            });
            let codes = evaluation.issues().map(IssueKey::code).collect();
            (results.to_vec(), codes)
        })
    }

    /// A target's checks over its set read the values admitted: one that the
    /// project's set proves outside it is a Finding, and one it cannot prove
    /// waits. A default its admitted values do not hold is unproved beside
    /// an element without an identity (D15:2184, 2365, 2367, 3354).
    #[test]
    fn a_target_set_with_a_failed_element_is_checked_over_what_it_holds() {
        let type_found = Code::FieldTypeInvalid;
        // Every admitted value is in the project's set: only the inherited
        // default `en`, which the set does not hold, waits.
        assert_eq!(
            server_checks(json!(["ja", "en"]), json!(["ja", 42])),
            (vec![None, Some(1), None], vec![type_found])
        );
        // `fr` is proved outside a project set with every identity, and
        // the Target Profile has no direction for it.
        assert_eq!(
            server_checks(json!(["ja", "en"]), json!(["fr", 42])),
            (
                vec![None, Some(1), None],
                vec![
                    type_found,
                    Code::TargetLocalesNotSubset,
                    Code::TargetLocaleDirectionMissing
                ]
            )
        );
        // Beside an element of the project's set without an identity, `fr`
        // is unproved and no Finding is made up.
        assert_eq!(
            server_checks(json!(["ja", "en", 42]), json!(["fr", 42])),
            (
                vec![Some(1), Some(1), None],
                vec![type_found, type_found, Code::TargetLocaleDirectionMissing]
            )
        );
        // With no value admitted, each of the three waits on why.
        assert_eq!(
            server_checks(json!(["ja", "en"]), json!([42])),
            (vec![Some(1), Some(1), Some(1)], vec![type_found])
        );
    }

    /// One alias destination that failed holds back only what names it: the
    /// map's bound counts it, every key and every other destination is
    /// checked, and the profile, built from every entry, waits on it
    /// (D15:1477, 1898, 2367, 2418).
    #[test]
    fn an_alias_destination_that_failed_leaves_every_other_alias_check() {
        use crate::resolver::locales::fixtures::key;
        use crate::resolver::locales::LocaleSubject as L;
        use crate::resolver::negotiation::{unit as alias, NegotiationSubject as N};
        let mut inputs = Inputs::complete();
        inputs.declaration()["localeNegotiation"]["aliases"] = json!({"de": 42, "fr": "ja"});
        inputs.resolve(|evaluation, state| {
            let found = BTreeSet::from([only_issue(&evaluation)]);
            all_evaluated(
                &evaluation,
                &[
                    phase_7(L::NegotiationAliasKey(key("de")), Check::CanonicalLocale),
                    phase_7(L::NegotiationAliasKey(key("fr")), Check::CanonicalLocale),
                    phase_7(
                        L::NegotiationAliasValue(key("fr"), key("ja")),
                        Check::CanonicalLocale,
                    ),
                    alias(N::Aliases, Check::WithinBound),
                    alias(N::Key(key("de")), Check::UniqueLocale),
                    alias(N::Key(key("fr")), Check::UniqueLocale),
                    alias(N::Destination(key("fr"), key("ja")), Check::RequestedMember),
                ],
            );
            // The destination that failed is no spelling: nothing names it.
            assert!(!evaluation.statuses().any(|(unit, _)| matches!(
                &unit.subject,
                Subject::Locale(L::NegotiationAliasValue(owner, _))
                    | Subject::Negotiation(N::Destination(owner, _))
                    if *owner == key("de")
            )));
            assert_eq!(keys(state.negotiation.as_ref().unwrap_err()), found);
        });
    }

    /// An undeclared member of `localeNegotiation` is a failure of that
    /// object alone: its aliases are checked, and the profile waits on it
    /// (D15:2184, 2367, 3354).
    #[test]
    fn an_undeclared_negotiation_member_holds_back_only_the_profile() {
        use crate::resolver::locales::fixtures::key;
        use crate::resolver::locales::LocaleSubject as L;
        use crate::resolver::negotiation::{unit as alias, NegotiationSubject as N};
        let mut inputs = Inputs::complete();
        inputs.declaration()["localeNegotiation"]["extra"] = json!(1);
        inputs.resolve(|evaluation, state| {
            let found = BTreeSet::from([only_issue(&evaluation)]);
            all_evaluated(
                &evaluation,
                &[
                    phase_7(L::NegotiationAliasKey(key("de")), Check::CanonicalLocale),
                    phase_7(
                        L::NegotiationAliasValue(key("de"), key("en")),
                        Check::CanonicalLocale,
                    ),
                    alias(N::Aliases, Check::WithinBound),
                    alias(N::Key(key("de")), Check::UniqueLocale),
                    alias(N::Destination(key("de"), key("en")), Check::RequestedMember),
                ],
            );
            assert_eq!(keys(state.negotiation.as_ref().unwrap_err()), found);
        });
    }

    /// Entries naming one key beside a destination that failed: its reason
    /// needs that destination, so the duplicate is unproved rather than
    /// reported with a reason made up (D15:1477, 2365, 2418).
    #[test]
    fn a_duplicate_alias_key_beside_a_failed_destination_waits_on_it() {
        use crate::resolver::locales::fixtures::key;
        use crate::resolver::negotiation::{unit as alias, NegotiationSubject as N};
        let mut inputs = Inputs::complete();
        inputs.declaration()["localeNegotiation"]["aliases"] = json!({"EN": 7, "en": "ja"});
        inputs.resolve(|evaluation, _| {
            let found = BTreeSet::from([only_issue(&evaluation)]);
            waits_on_exactly(
                &evaluation,
                &alias(N::Key(key("en")), Check::UniqueLocale),
                &found,
            );
        });
    }

    /// One fallback candidate that failed holds back only itself: its
    /// position has no unit, while the sequence's bound counts those
    /// admitted, every other candidate is checked, and a repeat among them is
    /// found; the policy, built from every candidate, waits on it (D15:1519,
    /// 1899, 1900, 2184, 2367, 2422, 3354).
    #[test]
    fn a_fallback_candidate_that_failed_leaves_every_other_candidate_checked() {
        use crate::resolver::fallback::{unit as fallback, FallbackSubject as F};
        use crate::resolver::locales::fixtures::key;
        use crate::resolver::locales::LocaleSubject as L;
        let mut inputs = Inputs::complete();
        inputs.declaration()["messageFallback"] = json!({
            "en": ["ja"],
            "ja": ["en", 42, "en", {"kind": "intent-source-locale"}],
        });
        inputs.resolve(|evaluation, state| {
            let candidate =
                |source, position, check| fallback(F::Candidate(key(source), position), check);
            all_evaluated(
                &evaluation,
                &[
                    phase_7(L::MessageFallbackKey(key("en")), Check::CanonicalLocale),
                    phase_7(L::MessageFallbackKey(key("ja")), Check::CanonicalLocale),
                    fallback(F::Sources, Check::WithinBound),
                    fallback(F::Source(key("ja")), Check::UniqueLocale),
                    fallback(F::Source(key("ja")), Check::RequestedMember),
                    fallback(F::Sequence(key("ja")), Check::WithinBound),
                    fallback(F::Sequence(key("en")), Check::WithinBound),
                    candidate("ja", 0, Check::CanonicalLocale),
                    candidate("ja", 0, Check::DistinctFromSource),
                    candidate("ja", 2, Check::CanonicalLocale),
                    candidate("ja", 2, Check::DistinctFromSource),
                    candidate("en", 0, Check::CanonicalLocale),
                    fallback(F::Literal(key("ja"), key("en")), Check::UniqueCandidate),
                    fallback(F::Literal(key("en"), key("ja")), Check::UniqueCandidate),
                    fallback(F::IntentSource(key("ja")), Check::UniqueCandidate),
                ],
            );
            assert_eq!(
                evaluation.status(&candidate("ja", 1, Check::CanonicalLocale)),
                None
            );
            let repeat = fallback(F::Literal(key("ja"), key("en")), Check::UniqueCandidate);
            let codes: Vec<(Code, bool)> = evaluation
                .issues()
                .map(|issue| (issue.code(), *issue.unit() == repeat))
                .collect();
            assert_eq!(
                codes,
                [
                    (Code::FieldValueInvalid, false),
                    (Code::MessageFallbackSequenceInvalid, true)
                ]
            );
            let every: BTreeSet<IssueKey> = evaluation.issues().cloned().collect();
            assert_eq!(keys(state.fallback.as_ref().unwrap_err()), every);
        });
    }

    /// A sequence that failed as a whole holds back only its own checks: its
    /// key's are evaluated, its bound waits on why, and the other key's
    /// sequence is checked (D15:1519, 2184, 2367, 3354).
    #[test]
    fn a_fallback_sequence_that_failed_leaves_its_key_and_the_others_checked() {
        use crate::resolver::fallback::{unit as fallback, FallbackSubject as F};
        use crate::resolver::locales::fixtures::key;
        for failed in [json!(7), json!([]), json!([42])] {
            let mut inputs = Inputs::complete();
            inputs.declaration()["messageFallback"] = json!({"en": ["ja"], "ja": failed});
            inputs.resolve(|evaluation, state| {
                let found = BTreeSet::from([only_issue(&evaluation)]);
                all_evaluated(
                    &evaluation,
                    &[
                        fallback(F::Sources, Check::WithinBound),
                        fallback(F::Source(key("ja")), Check::UniqueLocale),
                        fallback(F::Source(key("ja")), Check::RequestedMember),
                        fallback(F::Sequence(key("en")), Check::WithinBound),
                        fallback(F::Candidate(key("en"), 0), Check::CanonicalLocale),
                    ],
                );
                waits_on_exactly(
                    &evaluation,
                    &fallback(F::Sequence(key("ja")), Check::WithinBound),
                    &found,
                );
                assert_eq!(
                    evaluation.status(&fallback(
                        F::Candidate(key("ja"), 0),
                        Check::CanonicalLocale
                    )),
                    None
                );
                assert_eq!(keys(state.fallback.as_ref().unwrap_err()), found);
            });
        }
    }

    /// Keys that collide report the collision, and their sequences' checks
    /// wait on it alone, whatever a sequence's own failure (D15:1519).
    #[test]
    fn a_colliding_fallback_key_holds_back_its_sequences_on_the_collision() {
        use crate::resolver::fallback::{unit as fallback, FallbackSubject as F};
        use crate::resolver::locales::fixtures::key;
        let mut inputs = Inputs::complete();
        inputs.declaration()["messageFallback"] = json!({"EN": [42], "en": ["ja"]});
        inputs.resolve(|evaluation, _| {
            let collision = fallback(F::Source(key("en")), Check::UniqueLocale);
            let found: BTreeSet<IssueKey> = evaluation
                .issues()
                .filter(|issue| *issue.unit() == collision)
                .cloned()
                .collect();
            assert_eq!(found.len(), 1);
            waits_on_exactly(
                &evaluation,
                &fallback(F::Sequence(key("en")), Check::WithinBound),
                &found,
            );
        });
    }

    /// The logical unit of `check` over a locale item of any rule.
    fn coverage_locale(spelling: &str, check: Check) -> Unit {
        use crate::resolver::coverage::{unit, CoverageSubject as C, RuleContent};
        use crate::resolver::locales::fixtures::key;
        unit(C::Locale(RuleContent::projected(), key(spelling)), check)
    }

    /// The logical unit of `check` over a surface item of any rule naming the
    /// vocabulary member `member`.
    fn coverage_surface(member: &str, check: Check) -> Unit {
        use crate::resolver::coverage::{unit, CoverageSubject as C, RuleContent, SurfaceKey};
        unit(
            C::SurfaceClass(
                RuleContent::projected(),
                SurfaceKey::Member(Token::new(member).unwrap()),
            ),
            check,
        )
    }

    /// The logical unit normalizing every rule.
    fn coverage_rule() -> Unit {
        use crate::resolver::coverage::{unit, CoverageSubject as C, RuleContent};
        unit(C::Rule(RuleContent::projected()), Check::NormalizedDomain)
    }

    fn coverage_count(subject: crate::resolver::coverage::CoverageSubject) -> Unit {
        crate::resolver::coverage::unit(subject, Check::WithinBound)
    }

    fn coverage_cell(locale: &str, surface: &str) -> Unit {
        use crate::resolver::coverage::{unit, CoverageSubject as C};
        use crate::resolver::locales::fixtures::canonical;
        unit(
            C::Cell(canonical(locale), Token::new(surface).unwrap()),
            Check::EffectiveMode,
        )
    }

    fn coverage_table() -> Unit {
        use crate::resolver::coverage::{unit, CoverageSubject as C};
        unit(C::Table, Check::CompleteTable)
    }

    /// With every count able to read an admitted selector, a rule that failed
    /// still holds back every cell and the table, which need every rule
    /// (D15:2426, 3409).
    #[test]
    fn a_rule_that_failed_holds_back_every_cell_beside_an_admitted_rule() {
        for failed in [
            json!(7),
            json!({"requestedLocales": 7, "intentSurfaceClasses": ["nav"], "mode": "direct-required"}),
            json!({"requestedLocales": ["ja"], "intentSurfaceClasses": 7, "mode": "direct-required"}),
        ] {
            let mut inputs = Inputs::complete();
            inputs.declaration()["coverage"] = json!({"rules": [failed, {
                "requestedLocales": ["ja"],
                "intentSurfaceClasses": ["nav"],
                "mode": "fallback-allowed"
            }]});
            inputs.resolve(|evaluation, state| {
                let found = BTreeSet::from([only_issue(&evaluation)]);
                for unit in [
                    coverage_cell("en", "nav"),
                    coverage_cell("ja", "nav"),
                    coverage_table(),
                ] {
                    waits_on_exactly(&evaluation, &unit, &found);
                }
                assert_eq!(keys(state.coverage.as_ref().unwrap_err()), found);
            });
        }
    }

    /// One rule that failed, in whatever way, holds back no other rule's
    /// items, no count, and no bound over the table's size: only what needs
    /// every rule waits, and no Finding is made up for the rule that failed
    /// (D15:1901, 1902, 1903, 1905, 2190, 2365, 2367, 2426).
    #[test]
    fn a_coverage_rule_that_failed_leaves_the_other_rules_checked() {
        use crate::resolver::coverage::CoverageSubject as C;
        for failed in [
            json!(7),
            json!({"mode": "direct-required"}),
            json!({"requestedLocales": 7, "mode": "direct-required"}),
            json!({"requestedLocales": [], "mode": "direct-required"}),
            json!({"requestedLocales": ["ja", 7], "mode": "direct-required"}),
            json!({"intentSurfaceClasses": [7], "mode": "direct-required"}),
            json!({"requestedLocales": ["ja"], "mode": 7}),
            json!({"requestedLocales": ["ja"]}),
        ] {
            let mut inputs = Inputs::complete();
            inputs.declaration()["coverage"] = json!({"rules": [failed, {
                "requestedLocales": ["fr"],
                "intentSurfaceClasses": ["nav"],
                "mode": "fallback-allowed"
            }]});
            inputs.resolve(|evaluation, state| {
                all_evaluated(
                    &evaluation,
                    &[
                        coverage_count(C::Rules),
                        coverage_count(C::LocaleSelectors),
                        coverage_count(C::SurfaceSelectors),
                        coverage_count(C::Cells),
                        coverage_count(C::Comparisons),
                        coverage_locale("fr", Check::CanonicalLocale),
                        coverage_locale("fr", Check::UniqueLocale),
                        coverage_locale("fr", Check::RequestedMember),
                        coverage_surface("nav", Check::VocabularyMember),
                        coverage_surface("nav", Check::UniqueSurfaceClass),
                    ],
                );
                let outside: Vec<_> = issues(&evaluation)
                    .into_iter()
                    .filter(|(unit, _, _)| unit.phase == Phase::LocalePolicyResolution)
                    .collect();
                assert_eq!(
                    outside,
                    [(
                        coverage_locale("fr", Check::RequestedMember),
                        Code::CoverageRuleInvalid,
                        Some(Reason::LocaleNotRequested)
                    )]
                );
                assert_eq!(evaluation.issues().count(), 2);
                assert!(state.coverage.is_err());
            });
        }
    }

    /// One selector item that failed holds back neither its siblings' checks
    /// nor what they find among themselves; the rule, which needs every item,
    /// waits (D15:1567, 2367, 2426, 3409).
    #[test]
    fn a_selector_item_that_failed_leaves_its_siblings_checked() {
        use crate::resolver::coverage::{unit, CoverageSubject as C, RuleContent, SurfaceKey};
        use crate::resolver::locales::Content;
        let mut inputs = Inputs::complete();
        inputs.declaration()["coverage"] = json!({"rules": [{
            "requestedLocales": ["fr", 7, "ja", "ja"],
            "intentSurfaceClasses": ["nav", 7, "unknown", "nav"],
            "mode": "fallback-allowed"
        }]});
        inputs.resolve(|evaluation, state| {
            let unknown = unit(
                C::SurfaceClass(
                    RuleContent::projected(),
                    SurfaceKey::Content(Content::projected()),
                ),
                Check::VocabularyMember,
            );
            let semantic: Vec<_> = issues(&evaluation)
                .into_iter()
                .filter(|(unit, _, _)| unit.phase == Phase::LocalePolicyResolution)
                .collect();
            assert_eq!(
                semantic,
                [
                    (
                        coverage_locale("fr", Check::RequestedMember),
                        Code::CoverageRuleInvalid,
                        Some(Reason::LocaleNotRequested)
                    ),
                    (
                        coverage_locale("ja", Check::UniqueLocale),
                        Code::LocaleDuplicate,
                        None
                    ),
                    (
                        coverage_surface("nav", Check::UniqueSurfaceClass),
                        Code::CoverageRuleInvalid,
                        Some(Reason::SurfaceClassDuplicate)
                    ),
                    (
                        unknown,
                        Code::CoverageRuleInvalid,
                        Some(Reason::SurfaceClassUnknown)
                    ),
                ]
            );
            let every: BTreeSet<IssueKey> = evaluation.issues().cloned().collect();
            waits_on_exactly(&evaluation, &coverage_rule(), &every);
            assert_eq!(keys(state.coverage.as_ref().unwrap_err()), every);
        });
    }

    /// Each count takes every occurrence it names, whatever became of its
    /// value: past the bound with a rule or item that failed among them
    /// (D15:1901, 1902, 1903, 1905).
    #[test]
    fn the_coverage_counts_take_every_occurrence_beside_what_failed() {
        use crate::resolver::coverage::CoverageSubject as C;
        use crate::resolver::evaluation::Actual;
        use crate::resolver::limits::LocaleResolutionLimits as Limits;
        type Tighten = fn(&mut Limits);
        let cases: [(Tighten, Value, C, u64); 4] = [
            (
                |limits| limits.max_coverage_rules = bound(2),
                json!([7, {"requestedLocales": ["ja"], "mode": "direct-required"}, {"mode": 7}]),
                C::Rules,
                3,
            ),
            (
                |limits| limits.max_coverage_locale_selector_occurrences = bound(2),
                json!([7, {"requestedLocales": ["ja", 7, true], "mode": "direct-required"}]),
                C::LocaleSelectors,
                3,
            ),
            (
                |limits| limits.max_coverage_surface_selector_occurrences = bound(1),
                json!([{"intentSurfaceClasses": ["nav", 7], "mode": "direct-required"}]),
                C::SurfaceSelectors,
                2,
            ),
            (
                |limits| limits.max_coverage_rule_cell_comparisons = bound(11),
                json!([7, {"requestedLocales": ["ja"], "mode": "direct-required"}]),
                C::Comparisons,
                12,
            ),
        ];
        for (tighten, rules, subject, total) in cases {
            let mut inputs = Inputs::complete();
            let mut limits = policy_limits();
            tighten(&mut limits.locale_resolution);
            inputs.select_policy(resource_limit_policy(&limits));
            inputs.declaration()["coverage"] = json!({ "rules": rules });
            inputs.resolve(|evaluation, _| {
                let unit = coverage_count(subject);
                let key = evaluation
                    .issues()
                    .find(|key| *key.unit() == unit)
                    .unwrap_or_else(|| panic!("{unit:?} found nothing"));
                assert_eq!(
                    evaluation.limit(key).unwrap().actual,
                    Actual::Exact(total),
                    "{unit:?}"
                );
            });
        }
    }

    /// A selector count waits only when every selector it would read failed:
    /// a rule or selector that failed has no item to count, and one beside it
    /// that was admitted is counted (D15:1902, 1903, 2184, 3354).
    #[test]
    fn a_selector_count_waits_only_when_every_selector_failed() {
        use crate::resolver::coverage::CoverageSubject as C;
        for (rules, locale_waits, surface_waits) in [
            (json!([7]), true, true),
            (
                json!([{"requestedLocales": 7, "mode": "direct-required"}]),
                true,
                false,
            ),
            (
                json!([
                    {"requestedLocales": 7, "mode": "direct-required"},
                    {"requestedLocales": ["ja"], "mode": "direct-required"}
                ]),
                false,
                false,
            ),
        ] {
            let mut inputs = Inputs::complete();
            inputs.declaration()["coverage"] = json!({ "rules": rules });
            inputs.resolve(|evaluation, _| {
                let every: BTreeSet<IssueKey> = evaluation.issues().cloned().collect();
                for (subject, waits) in [
                    (C::LocaleSelectors, locale_waits),
                    (C::SurfaceSelectors, surface_waits),
                ] {
                    let unit = coverage_count(subject);
                    if waits {
                        waits_on_exactly(&evaluation, &unit, &every);
                    } else {
                        all_evaluated(&evaluation, &[unit]);
                    }
                }
            });
        }
    }

    /// A rule whose mode failed still has a domain, which a duplicate is
    /// found against, while the mode the table needs is missing (D15:2365,
    /// 2426).
    #[test]
    fn a_rule_whose_mode_failed_still_holds_its_domain() {
        use crate::resolver::coverage::{unit, CoverageSubject as C, Domain};
        use crate::resolver::locales::fixtures::canonical;
        let mut inputs = Inputs::complete();
        inputs.declaration()["coverage"] = json!({"rules": [
            {"requestedLocales": ["ja"], "mode": 7},
            {"requestedLocales": ["ja"], "mode": "fallback-allowed"}
        ]});
        inputs.resolve(|evaluation, state| {
            let surfaces = vectors::MEMBERS.map(|member| Token::new(member).unwrap());
            let domain = Domain::of(vec![canonical("ja")], surfaces.to_vec());
            let duplicate = unit(C::Domain(domain), Check::UniqueDomain);
            let found: Vec<_> = issues(&evaluation)
                .into_iter()
                .filter(|(unit, _, _)| *unit == duplicate)
                .map(|(_, code, _)| code)
                .collect();
            assert_eq!(found, [Code::CoverageRuleDuplicate]);
            assert!(state.coverage.is_err());
        });
    }

    /// A cell no rule matches is what needs the default mode, so a default
    /// that failed holds back only such cells: a conflict between rules is
    /// still found (D15:2365, 2426).
    #[test]
    fn a_default_mode_that_failed_holds_back_only_the_cells_taking_it() {
        let mut inputs = Inputs::complete();
        inputs.declaration()["coverage"] = json!({
            "defaultMode": 7,
            "rules": [
                {"requestedLocales": ["ja"], "mode": "fallback-allowed"},
                {"intentSurfaceClasses": ["checkout"], "mode": "direct-required"}
            ]
        });
        inputs.resolve(|evaluation, state| {
            let conflict = coverage_cell("ja", "checkout");
            let cause = evaluation
                .issues()
                .find(|key| key.unit().phase == Phase::StructuralAdmission)
                .expect("the default mode failed")
                .clone();
            let found: Vec<_> = issues(&evaluation)
                .into_iter()
                .filter(|(unit, _, _)| *unit == conflict)
                .map(|(_, code, _)| code)
                .collect();
            assert_eq!(found, [Code::CoverageRuleConflict]);
            all_evaluated(&evaluation, &[conflict, coverage_cell("ja", "nav")]);
            waits_on_exactly(
                &evaluation,
                &coverage_cell("en", "nav"),
                &BTreeSet::from([cause]),
            );
            let every: BTreeSet<IssueKey> = evaluation.issues().cloned().collect();
            waits_on_exactly(&evaluation, &coverage_table(), &every);
            assert_eq!(keys(state.coverage.as_ref().unwrap_err()), every);
        });
    }

    /// An unknown default mode blocks coverage, so the table keeps no Basis
    /// even when a rule decides every cell (D15:2426, 3607).
    #[test]
    fn a_default_mode_that_failed_holds_back_the_table() {
        let mut inputs = Inputs::complete();
        inputs.declaration()["coverage"] = json!({
            "defaultMode": 7,
            "rules": [{"requestedLocales": ["ja", "en"], "mode": "fallback-allowed"}]
        });
        inputs.resolve(|evaluation, _| {
            let found = BTreeSet::from([only_issue(&evaluation)]);
            waits_on_exactly(&evaluation, &coverage_table(), &found);
            assert!(evaluation.decision_basis().is_empty());
        });
    }

    /// A cell a rule known to match never takes the default, so beside a rule
    /// that failed it waits on that rule alone, and only a cell no such rule
    /// matches waits on the default as well (D15:2365).
    #[test]
    fn a_cell_waits_on_the_default_only_when_no_known_rule_matches_it() {
        let mut inputs = Inputs::complete();
        inputs.declaration()["coverage"] = json!({
            "defaultMode": 7,
            "rules": [7, {"requestedLocales": ["ja"], "mode": "fallback-allowed"}]
        });
        inputs.resolve(|evaluation, _| {
            let every: BTreeSet<IssueKey> = evaluation.issues().cloned().collect();
            assert_eq!(every.len(), 2);
            let Some(Status::NotEvaluated(causes)) = evaluation.status(&coverage_cell("ja", "nav"))
            else {
                panic!("the cell waits");
            };
            assert_eq!(causes.keys().count(), 1);
            waits_on_exactly(&evaluation, &coverage_cell("en", "nav"), &every);
        });
    }

    /// A rule whose mode failed still holds its domain, so a cell it matches
    /// waits on that mode and not on the default, and a cell no rule matches
    /// on the default and not on that mode (D15:2365).
    #[test]
    fn a_cell_a_rule_without_a_mode_matches_waits_on_that_mode_alone() {
        let only = |coverage: Value| {
            let mut inputs = Inputs::complete();
            inputs.declaration()["coverage"] = coverage;
            inputs.resolve(|evaluation, _| only_issue(&evaluation))
        };
        let rule = json!([{"requestedLocales": ["ja"], "mode": 7}]);
        let default = BTreeSet::from([only(json!({"defaultMode": 7}))]);
        let mode = BTreeSet::from([only(json!({"rules": rule}))]);
        let mut inputs = Inputs::complete();
        inputs.declaration()["coverage"] = json!({"defaultMode": 7, "rules": rule});
        inputs.resolve(|evaluation, _| {
            waits_on_exactly(&evaluation, &coverage_cell("ja", "nav"), &mode);
            waits_on_exactly(&evaluation, &coverage_cell("en", "nav"), &default);
        });
    }

    /// A conflict needs only the modes of the rules that conflict: a mode
    /// that failed in a rule outside the cell, or in a third maximally
    /// specific rule, leaves it found, while the table waits (D15:2365, 2410,
    /// 2426).
    #[test]
    fn a_conflict_is_found_beside_a_mode_that_failed() {
        for rules in [
            json!([
                {"requestedLocales": ["en"], "intentSurfaceClasses": ["nav"], "mode": 7},
                {"requestedLocales": ["ja"], "mode": "direct-required"},
                {"intentSurfaceClasses": ["nav"], "mode": "fallback-allowed"}
            ]),
            json!([
                {
                    "requestedLocales": ["ja"],
                    "intentSurfaceClasses": ["nav", "internal-tool"],
                    "mode": 7
                },
                {
                    "requestedLocales": ["ja"],
                    "intentSurfaceClasses": ["nav", "checkout"],
                    "mode": "direct-required"
                },
                {
                    "requestedLocales": ["en", "ja"],
                    "intentSurfaceClasses": ["nav"],
                    "mode": "fallback-allowed"
                }
            ]),
        ] {
            let mut inputs = Inputs::complete();
            inputs.declaration()["coverage"] = json!({"rules": rules});
            inputs.resolve(|evaluation, state| {
                let conflicts: Vec<Unit> = issues(&evaluation)
                    .into_iter()
                    .filter(|(_, code, _)| *code == Code::CoverageRuleConflict)
                    .map(|(unit, _, _)| unit)
                    .collect();
                assert_eq!(conflicts, [coverage_cell("ja", "nav")]);
                assert!(matches!(
                    evaluation.status(&coverage_table()),
                    Some(Status::NotEvaluated(_))
                ));
                assert!(state.coverage.is_err());
            });
        }
    }

    /// A maximally specific rule whose mode failed holds back the cells it
    /// could decide, even beside a rule with a mode, and no other cell
    /// (D15:2365).
    #[test]
    fn a_maximal_rule_without_a_mode_holds_back_only_its_cells() {
        let mut inputs = Inputs::complete();
        inputs.declaration()["coverage"] = json!({"rules": [
            {
                "requestedLocales": ["ja"],
                "intentSurfaceClasses": ["nav", "internal-tool"],
                "mode": 7
            },
            {
                "requestedLocales": ["ja"],
                "intentSurfaceClasses": ["nav", "checkout"],
                "mode": "direct-required"
            }
        ]});
        inputs.resolve(|evaluation, _| {
            let mode = BTreeSet::from([only_issue(&evaluation)]);
            for surface in ["nav", "internal-tool"] {
                waits_on_exactly(&evaluation, &coverage_cell("ja", surface), &mode);
            }
            for (locale, surface) in [("ja", "checkout"), ("en", "nav")] {
                assert_eq!(
                    evaluation.status(&coverage_cell(locale, surface)),
                    Some(&Status::Evaluated)
                );
            }
        });
    }

    /// Each mode that failed is a cause of what needs it: a cell both rules
    /// decide waits on the two, a cell one rule decides on that rule's alone,
    /// and the table on both (D15:2365, 2426).
    #[test]
    fn each_mode_that_failed_is_a_cause_of_what_needs_it() {
        let mut inputs = Inputs::complete();
        inputs.declaration()["coverage"] = json!({"rules": [
            {
                "requestedLocales": ["ja"],
                "intentSurfaceClasses": ["nav", "internal-tool"],
                "mode": 7
            },
            {
                "requestedLocales": ["ja"],
                "intentSurfaceClasses": ["nav", "checkout"],
                "mode": "x"
            }
        ]});
        inputs.resolve(|evaluation, _| {
            let of = |code| {
                evaluation
                    .issues()
                    .filter(|key| key.code() == code)
                    .cloned()
                    .collect::<BTreeSet<IssueKey>>()
            };
            let (typed, valued) = (of(Code::FieldTypeInvalid), of(Code::FieldValueInvalid));
            assert_eq!((typed.len(), valued.len()), (1, 1));
            let both: BTreeSet<IssueKey> = typed.union(&valued).cloned().collect();
            waits_on_exactly(&evaluation, &coverage_cell("ja", "internal-tool"), &typed);
            waits_on_exactly(&evaluation, &coverage_cell("ja", "checkout"), &valued);
            waits_on_exactly(&evaluation, &coverage_cell("ja", "nav"), &both);
            waits_on_exactly(&evaluation, &coverage_table(), &both);
        });
    }

    /// A domain two rules hold keeps the mode of each as a cause of the table,
    /// though the rule whose mode failed orders after the other (D15:2365,
    /// 2426).
    #[test]
    fn a_domain_held_twice_keeps_each_rules_mode_as_a_cause() {
        let mut inputs = Inputs::complete();
        inputs.declaration()["coverage"] = json!({"rules": [
            {"requestedLocales": ["ja"], "mode": "fallback-allowed"},
            {"requestedLocales": ["ja"], "mode": []}
        ]});
        inputs.resolve(|evaluation, _| {
            let codes: BTreeSet<&str> =
                evaluation.issues().map(|key| key.code().as_str()).collect();
            let expected = [Code::CoverageRuleDuplicate, Code::FieldTypeInvalid];
            assert_eq!(codes, expected.map(Code::as_str).into());
            let every: BTreeSet<IssueKey> = evaluation.issues().cloned().collect();
            waits_on_exactly(&evaluation, &coverage_table(), &every);
        });
    }

    /// A rule narrower rules outrank at every cell it matches holds back no
    /// cell, and its unknown mode still blocks the table (D15:2426, 3607).
    #[test]
    fn a_rule_outranked_at_every_cell_holds_back_only_the_table() {
        let mut inputs = Inputs::complete();
        inputs.declaration()["coverage"] = json!({"rules": [
            {"requestedLocales": ["en", "ja"], "intentSurfaceClasses": ["nav"], "mode": 7},
            {"requestedLocales": ["en"], "intentSurfaceClasses": ["nav"], "mode": "direct-required"},
            {"requestedLocales": ["ja"], "intentSurfaceClasses": ["nav"], "mode": "fallback-allowed"}
        ]});
        inputs.resolve(|evaluation, state| {
            let mode = BTreeSet::from([only_issue(&evaluation)]);
            for locale in ["en", "ja"] {
                assert_eq!(
                    evaluation.status(&coverage_cell(locale, "nav")),
                    Some(&Status::Evaluated)
                );
            }
            waits_on_exactly(&evaluation, &coverage_table(), &mode);
            assert_eq!(keys(state.coverage.as_ref().unwrap_err()), mode);
        });
    }

    /// The table needs the mode of every rule it reads, a rule's whose
    /// selector failed and so holds no domain included: under a vocabulary
    /// with no surface class, where no cell waits on the selector, the table
    /// waits on that mode alone, and with cells it waits on the mode beside
    /// the selector (D15:2365, 2426, 3607).
    #[test]
    fn the_table_waits_on_the_mode_of_a_rule_whose_selector_failed() {
        let rule = json!({"intentSurfaceClasses": ["nowhere"], "mode": 7});
        for mut inputs in [Inputs::without_surface_classes(), Inputs::complete()] {
            let empty = inputs.surfaces.is_empty();
            inputs.declaration()["coverage"]["rules"] = json!([rule]);
            inputs.resolve(|evaluation, state| {
                let found: BTreeSet<IssueKey> = evaluation.issues().cloned().collect();
                let mode: BTreeSet<IssueKey> = found
                    .iter()
                    .filter(|key| key.code() == Code::FieldTypeInvalid)
                    .cloned()
                    .collect();
                assert_eq!((found.len(), mode.len()), (2, 1));
                let table = if empty { &mode } else { &found };
                waits_on_exactly(&evaluation, &coverage_table(), table);
                assert!(evaluation.decision_basis().is_empty());
                assert_eq!(keys(state.coverage.as_ref().unwrap_err()), found);
            });
        }
    }

    /// No check reads an undeclared member under `coverage` or a rule: every
    /// cell and the table, whose only prerequisite is its cells, are
    /// evaluated, and the policy built from all of `coverage` waits on it
    /// (D15:1609, 2149, 2367).
    #[test]
    fn an_undeclared_coverage_member_holds_back_only_the_policy() {
        for edit in [
            (|app: &mut Value| app["coverage"]["extra"] = json!(1)) as fn(&mut Value),
            |app| app["coverage"]["rules"][0]["extra"] = json!(1),
        ] {
            let mut inputs = Inputs::complete();
            edit(inputs.declaration());
            inputs.resolve(|evaluation, state| {
                let found = BTreeSet::from([only_issue(&evaluation)]);
                all_evaluated(
                    &evaluation,
                    &[
                        coverage_rule(),
                        coverage_cell("en", "nav"),
                        coverage_cell("ja", "checkout"),
                        coverage_table(),
                    ],
                );
                assert_eq!(evaluation.decision_basis().len(), 6);
                assert_eq!(keys(state.coverage.as_ref().unwrap_err()), found);
            });
        }
    }

    /// A declaration whose key lacks the identifier syntax counts under the
    /// policy's `maxProfiles`, while no later phase has a subject for its ID
    /// (D15:1885, 1886, 2184).
    #[test]
    fn a_declaration_whose_key_fails_counts_without_an_id_subject() {
        let mut inputs = Inputs::complete();
        let copy = inputs.declaration().clone();
        inputs.config["profiles"]["Admin Site"] = copy;
        let mut limits = policy_limits();
        limits.configuration_input.structural.max_profiles = bound(1);
        inputs.select_policy(resource_limit_policy(&limits));
        inputs.resolve(|evaluation, _| {
            let count = evaluation
                .issues()
                .find(|key| key.unit().subject == Subject::ResourcePolicy(PolicySubject::Profiles))
                .cloned()
                .unwrap();
            assert_eq!(
                evaluation.limit(&count).map(|exceeded| exceeded.actual),
                Some(crate::resolver::evaluation::Actual::Exact(2))
            );
            let ids: Vec<&PolicySubject> = evaluation
                .statuses()
                .filter_map(|(unit, _)| match &unit.subject {
                    Subject::ResourcePolicy(subject @ PolicySubject::ProfileId(_)) => Some(subject),
                    _ => None,
                })
                .collect();
            assert_eq!(
                ids,
                [&PolicySubject::ProfileId(
                    serde_json::from_value(json!("web")).unwrap()
                )]
            );
        });
    }

    #[test]
    fn a_spelling_the_provider_cannot_answer_is_a_resolver_defect() {
        // The finite provider has no row for `ja-JP`.
        let mut inputs = Inputs::complete();
        inputs.declaration()["requestedLocales"] = json!(["ja-JP", "en"]);
        inputs.attempt(|_, state| {
            assert_eq!(
                state.unwrap_err(),
                ResolverInvariant::Canonicalization(CanonicalizationFailure::Provider(
                    ProviderFailure::UnsupportedInput
                ))
            );
        });
    }

    /// The capability's `localeData` bounds reach Phase 5, which reads the
    /// rest of the data past the boundary under them. Data whose rest does
    /// not read is not admitted, a Finding of the data's unit rather than a
    /// refusal before invocation (D15:3165).
    #[test]
    fn phase_5_reads_the_data_under_the_capabilitys_locale_data_bounds() {
        #[derive(PartialEq, Eq)]
        struct Behind(LocaleDataLimits);

        impl FormalData for Behind {
            type Provider = FixtureProvider;

            fn read(&self, limits: &LocaleDataLimits) -> DataReading<FixtureProvider> {
                DataReading::settled(if *limits == self.0 {
                    Ok(FixtureProvider::new())
                } else {
                    Err(DataNotAdmitted)
                })
            }
        }

        Inputs::complete().prepared(|configuration, invocation| {
            // Bounds unlike the fixture capability's minima, so that only the
            // capability's own bounds can be the ones read under.
            let seven = Bound::new(7).unwrap();
            let bounds = LocaleDataLimits {
                max_payloads: seven,
                max_payload_bytes: seven,
                max_marker_bytes: seven,
                max_validity_groups: seven,
                max_codes: seven,
                max_extension_keys: seven,
                max_types: seven,
                max_corrections: seven,
                max_validation_work_units: seven,
            };
            let capability = invocation.capability.clone().with_locale_data(bounds);
            let invocation = Invocation {
                capability: &capability,
                ..*invocation
            };
            let resolved = |data: &Behind| {
                let mut evaluation = Evaluation::default();
                let mut set = None;
                let canonicalized =
                    invoke(&mut evaluation, configuration, &invocation, data, &mut set)
                        .unwrap()
                        .canonicalization
                        .is_ok();
                (issues(&evaluation), canonicalized)
            };
            assert_eq!(resolved(&Behind(bounds)), (vec![], true));
            let mut other = bounds;
            other.max_codes = Bound::new(bounds.max_codes.get() + 1).unwrap();
            let (found, canonicalized) = resolved(&Behind(other));
            let dataset = crate::resolver::locale_data::unit(Check::IntactDataset);
            assert_eq!(found, [(dataset, Code::LocaleDataNotAdmitted, None)]);
            assert!(!canonicalized);
        });
    }

    /// The invocation records Phase 0 before configuration entry: Phase 0's
    /// status, Finding, and occurrence candidate stand when no configuration
    /// follows, and the semantic phases record no Phase 0 unit of their own
    /// (D15:2145).
    #[test]
    fn phase_0_is_recorded_apart_from_the_semantic_phases() {
        let mut inputs = Inputs::complete();
        inputs.specifications.project_profile = None;
        inputs.prepared(|configuration, invocation| {
            let mut alone = Evaluation::default();
            admit_project_profile(&mut alone, invocation.supported, invocation.specifications)
                .unwrap()
                .unwrap_err();
            assert_eq!(
                alone
                    .statuses()
                    .map(|(unit, status)| (unit.clone(), status.clone()))
                    .collect::<Vec<_>>(),
                [(PHASE_0, Status::Evaluated)]
            );
            assert_eq!(
                issues(&alone),
                [(
                    PHASE_0,
                    Code::SpecificationInputIncomplete,
                    Some(Reason::MissingRequiredSlot)
                )]
            );
            assert_eq!(alone.occurrence_candidates().len(), 1);
            let mut semantic = Evaluation::default();
            let mut set = None;
            resolve_profile(
                &mut semantic,
                &Ok(ACTIVE),
                configuration,
                invocation,
                &FixtureProvider::new(),
                &mut set,
            )
            .unwrap();
            assert_eq!(semantic.status(&PHASE_0), None);
            assert!(semantic.occurrence_candidates().is_empty());
        });
    }

    #[test]
    fn an_evaluation_runs_one_invocation() {
        Inputs::complete().prepared(|configuration, invocation| {
            let mut evaluation = Evaluation::default();
            let (mut first, mut second) = (None, None);
            assert!(invoke(
                &mut evaluation,
                configuration,
                invocation,
                &FixtureProvider::new(),
                &mut first
            )
            .is_ok());
            assert_eq!(
                invoke(
                    &mut evaluation,
                    configuration,
                    invocation,
                    &FixtureProvider::new(),
                    &mut second
                )
                .unwrap_err(),
                ResolverInvariant::Evaluation(EvaluationInvariant::UnitRecordedTwice(PHASE_0))
            );
        });
    }

    #[test]
    fn each_phases_defect_is_the_resolvers() {
        assert_eq!(
            ResolverInvariant::from(PolicyInvariant::Selection(
                SelectionInvariant::BootstrapBoundMismatch
            )),
            ResolverInvariant::Selection(SelectionInvariant::BootstrapBoundMismatch)
        );
        let twice = EvaluationInvariant::UnitRecordedTwice(PHASE_0);
        for invariant in [
            ResolverInvariant::from(PolicyInvariant::Evaluation(twice.clone())),
            ResolverInvariant::from(LocaleInvariant::Evaluation(twice.clone())),
        ] {
            assert_eq!(invariant, ResolverInvariant::Evaluation(twice.clone()));
        }
    }

    /// Either entry's Phase 1 reads the path bounds of the capability that
    /// path admission reads: under a tight `maxLogicalPathSegments`, a string
    /// past a tight single-string bound too deep for it keeps no projected
    /// path, and path admission leaves each candidate's primary Evidence as
    /// it would have left the projected one (D15:1851; plan 025 decision
    /// #299).
    #[test]
    fn phase_1_reads_the_path_bounds_path_admission_applies() {
        let single = BoundId {
            group: Group::ConfigurationInput,
            name: "maxSingleStringBytes",
        };
        let tight = with(MetadataBound::LogicalPathSegments, 1);
        let candidates = |metadata, programmatic| {
            let mut inputs = Inputs::complete().bounding(&[(single, 2)], &[]);
            inputs.metadata(metadata);
            let found = |_: &Invocation<'_, Id>, evaluation: Evaluation, _: Invoked<'_, Id>| {
                evaluation.occurrence_candidates().to_vec()
            };
            if programmatic {
                inputs.programmatic_under(None, found)
            } else {
                inputs.entered_under(None, found)
            }
        };
        let primaries = |candidates: &[OccurrenceCandidate]| {
            candidates
                .iter()
                .map(|candidate| candidate.primary.admitted(&tight).unwrap())
                .collect::<Vec<_>>()
        };
        for programmatic in [false, true] {
            let bounded = candidates(tight, programmatic);
            let whole = candidates(unbounded(), programmatic);
            assert_ne!(bounded, whole, "{programmatic}");
            assert_eq!(primaries(&bounded), primaries(&whole), "{programmatic}");
        }
    }

    /// Phase 9's unit of the reference at `slot`.
    fn resolved_slot(slot: PolicySlot) -> Unit {
        exact_resolution::unit(ReferenceField::Policy(slot))
    }

    /// One `policies` slot that fails, or is missing, leaves only its own
    /// reference unresolved, and only the group that holds the slot waits
    /// (D15:2137, 2365, 2367, 3445).
    #[test]
    fn a_policy_slot_that_failed_stops_only_its_reference_and_its_group() {
        use SemanticGroup::{OptionalPolicies, RequiredPolicies};
        type Edit = fn(&mut Value);
        let cases: [(PolicySlot, Edit, SemanticGroup, SemanticGroup); 4] = [
            (
                PolicySlot::Trust,
                |policies| policies["trust"] = json!(42),
                RequiredPolicies,
                OptionalPolicies,
            ),
            (
                PolicySlot::Approval,
                |policies| {
                    policies.as_object_mut().unwrap().remove("approval");
                },
                RequiredPolicies,
                OptionalPolicies,
            ),
            (
                PolicySlot::GlossarySet,
                |policies| policies["glossarySet"] = json!(42),
                OptionalPolicies,
                RequiredPolicies,
            ),
            (
                PolicySlot::ProviderRouting,
                |policies| {
                    policies.as_object_mut().unwrap().remove("providerRouting");
                },
                OptionalPolicies,
                RequiredPolicies,
            ),
        ];
        for (failed, edit, waiting, established) in cases {
            let mut inputs = Inputs::complete();
            edit(&mut inputs.declaration()["policies"]);
            inputs.resolve(|evaluation, state| {
                let found = BTreeSet::from([only_issue(&evaluation)]);
                for slot in PolicySlot::ALL {
                    let unit = resolved_slot(slot);
                    if slot == failed {
                        waits_on_exactly(&evaluation, &unit, &found);
                    } else {
                        all_evaluated(&evaluation, &[unit]);
                    }
                }
                waits_on_exactly(&evaluation, &dependency::unit(waiting), &found);
                all_evaluated(&evaluation, &[dependency::unit(established)]);
                // The bindings need every reference.
                assert_eq!(state.references.as_ref().err().map(keys), Some(found));
                assert!(state.bindings.is_err());
            });
        }
    }

    /// A member `policies` does not declare is in no slot, so every reference
    /// is still resolved, while both groups of the object wait on its Finding,
    /// as the groups of `localeNegotiation` and `coverage` do (D15:2184, 2367;
    /// plan 025 interpretations #346, #347).
    #[test]
    fn an_undeclared_policies_member_holds_back_no_reference() {
        let mut inputs = Inputs::complete();
        inputs.declaration()["policies"]["extra"] = json!(1);
        inputs.resolve(|evaluation, state| {
            let key = only_issue(&evaluation);
            assert_eq!(key.code(), Code::UnknownField);
            let units = PolicySlot::ALL.map(resolved_slot);
            all_evaluated(&evaluation, &units);
            let found = BTreeSet::from([key]);
            for group in [
                SemanticGroup::RequiredPolicies,
                SemanticGroup::OptionalPolicies,
            ] {
                waits_on_exactly(&evaluation, &dependency::unit(group), &found);
            }
            assert!(state.references.is_ok() && state.bindings.is_ok());
            assert!(state.structure.is_err());
        });
    }

    /// Without a selection each `policies` slot still names its reference's
    /// units, which wait on what held the selection back, while no unit is
    /// made up for a target, whose key only the declaration holds (D15:1171,
    /// 2131, 2184; plan 025 interpretation #220).
    #[test]
    fn without_a_selection_each_slot_records_its_reference_units() {
        let mut inputs = Inputs::complete();
        inputs.selected = "absent";
        inputs.attempt(|evaluation, state| {
            state.unwrap().expect("entry admits the file");
            let found = BTreeSet::from([only_issue(&evaluation)]);
            for slot_id in PolicySlot::ALL {
                let [capability, recheck] = stages(slot(slot_id));
                for unit in [capability, recheck, resolved_slot(slot_id)] {
                    waits_on_exactly(&evaluation, &unit, &found);
                }
            }
            assert!(!evaluation.statuses().any(|(unit, _)| matches!(
                unit.subject,
                Subject::ArtifactReference(ReferenceField::TargetProfile(_))
            )));
        });
    }

    /// Both policy groups are the confirmed declaration's: even over an empty
    /// collection, whose admission has no unit to wait on the policy, each
    /// waits on what left the policy unconfirmed (D15:2132, 2516).
    #[test]
    fn an_empty_collection_leaves_both_policy_groups_waiting_on_the_policy() {
        let mut inputs = Inputs::complete();
        inputs.members.clear();
        inputs.resolve(|evaluation, state| {
            assert!(state.policy.confirmed.is_err());
            let decision = evaluation
                .issues()
                .find(|key| key.unit().phase == Phase::ResourcePolicyAdmission)
                .unwrap();
            for group in [
                SemanticGroup::RequiredPolicies,
                SemanticGroup::OptionalPolicies,
            ] {
                let Some(Status::NotEvaluated(causes)) =
                    evaluation.status(&dependency::unit(group))
                else {
                    panic!("{group:?} is not waiting");
                };
                assert!(keys(causes).contains(decision), "{group:?}");
            }
        });
    }

    /// Without a confirmed policy an empty collection proves nothing about a
    /// reference: every unit of Phase 9 waits on what left the policy
    /// unconfirmed, and the phase adds no Finding (D15:2137, 2365).
    #[test]
    fn an_empty_collection_without_a_confirmed_policy_finds_nothing_in_phase_9() {
        let mut inputs = Inputs::complete();
        inputs.members.clear();
        inputs.resolve(|evaluation, state| {
            assert!(state.policy.confirmed.is_err());
            let found: BTreeSet<IssueKey> = evaluation.issues().cloned().collect();
            assert!(!found.is_empty());
            let later = found
                .iter()
                .filter(|key| key.unit().phase >= Phase::ArtifactReferenceResolution);
            assert_eq!(later.count(), 0);
            let mut units = PolicySlot::ALL.map(resolved_slot).to_vec();
            units.extend(
                ["browser", "server"]
                    .map(|id| {
                        ReferenceField::TargetProfile(serde_json::from_value(json!(id)).unwrap())
                    })
                    .map(exact_resolution::unit),
            );
            for unit in &units {
                let Some(Status::NotEvaluated(causes)) = evaluation.status(unit) else {
                    panic!("{unit:?} is not waiting");
                };
                assert!(causes.keys().next().is_some(), "{unit:?}");
                assert!(keys(causes).is_subset(&found), "{unit:?}");
            }
        });
    }

    /// One place of the selected declaration made invalid, and what the
    /// invocation must then do.
    struct Broken {
        /// The field, map entry, array element, or `policies` slot.
        place: &'static str,
        edit: fn(&mut Value),
        /// The row and code of the one Finding it causes.
        found: (&'static str, Code),
        /// Each row of a semantic phase with a unit that waits on the Finding.
        waiting: &'static str,
        /// Each row of a semantic phase that lost a unit and keeps some, with
        /// how many stay evaluated: the independent siblings.
        kept: &'static str,
    }

    /// Every place the matrix breaks: each field of the declaration but
    /// `policies`, which holds the selected reference and has a test of its
    /// own, and a representative map entry, array element, and slot of each
    /// field that holds several values.
    fn broken_places() -> Vec<Broken> {
        vec![
            Broken {
                place: "projectId",
                edit: |app| app["projectId"] = json!(1),
                found: ("structural.profile.field-type", Code::FieldTypeInvalid),
                waiting: "dependency.project-identity.established identity.declaration-field.opaque",
                kept: "identity.declaration-field.opaque=1",
            },
            Broken {
                place: "selectionScope",
                edit: |app| app["selectionScope"] = json!(1),
                found: ("structural.profile.field-type", Code::FieldTypeInvalid),
                waiting: "dependency.project-identity.established identity.declaration-field.opaque",
                kept: "identity.declaration-field.opaque=1",
            },
            Broken {
                place: "requestedLocales",
                edit: |app| app["requestedLocales"] = json!("en"),
                found: ("structural.profile.field-type", Code::FieldTypeInvalid),
                waiting: "coverage.cells.within-bound coverage.comparisons.within-bound coverage.locale.member coverage.rule.normalized-domain coverage.table.complete dependency.coverage.established dependency.deployment-groups.established dependency.locale-negotiation.established dependency.message-fallback.established dependency.project-locales.established dependency.targets.established fallback.source.member hydration.relation.same-requested-locales locale.default-requested.member locale.requested-set.within-bound negotiation.destination.member target.requested-locales.project-subset",
                kept: "",
            },
            Broken {
                place: "requestedLocales element",
                edit: |app| app["requestedLocales"] = json!(["ja", 7]),
                found: ("structural.profile.field-type", Code::FieldTypeInvalid),
                waiting: "coverage.cells.within-bound coverage.comparisons.within-bound coverage.table.complete dependency.coverage.established dependency.deployment-groups.established dependency.locale-negotiation.established dependency.project-locales.established dependency.targets.established hydration.relation.same-requested-locales locale.default-requested.member negotiation.destination.member target.requested-locales.project-subset",
                kept: "coverage.domain.unique=2 locale.requested.canonical=1 locale.requested.unique=1",
            },
            Broken {
                place: "requestedLocales invalid locale",
                edit: |app| app["requestedLocales"] = json!(["ja", "zz"]),
                found: ("locale.requested.canonical", Code::LocaleInvalid),
                waiting: "coverage.cells.within-bound coverage.comparisons.within-bound coverage.table.complete dependency.coverage.established dependency.deployment-groups.established dependency.locale-negotiation.established dependency.project-locales.established dependency.targets.established hydration.relation.same-requested-locales locale.default-requested.member locale.requested.unique negotiation.destination.member target.requested-locales.project-subset",
                kept: "coverage.domain.unique=2 locale.requested.unique=1",
            },
            Broken {
                place: "defaultRequestedLocale",
                edit: |app| app["defaultRequestedLocale"] = json!(7),
                found: ("structural.profile.field-type", Code::FieldTypeInvalid),
                waiting: "dependency.deployment-groups.established dependency.project-locales.established dependency.targets.established hydration.relation.same-effective-default locale.default-requested.canonical locale.default-requested.member target.effective-default.supported",
                kept: "target.effective-default.supported=1",
            },
            Broken {
                place: "defaultSourceLocale",
                edit: |app| app["defaultSourceLocale"] = json!(7),
                found: ("structural.profile.field-type", Code::FieldTypeInvalid),
                waiting: "dependency.project-locales.established locale.default-source.canonical",
                kept: "",
            },
            Broken {
                place: "localeNegotiation",
                edit: |app| app["localeNegotiation"] = json!(7),
                found: ("structural.profile.field-type", Code::FieldTypeInvalid),
                waiting: "dependency.locale-negotiation.established negotiation.aliases.within-bound",
                kept: "",
            },
            Broken {
                place: "alias entry",
                edit: |app| app["localeNegotiation"]["aliases"]["de"] = json!(7),
                found: ("structural.profile.field-type", Code::FieldTypeInvalid),
                waiting: "dependency.locale-negotiation.established",
                kept: "",
            },
            Broken {
                place: "messageFallback",
                edit: |app| app["messageFallback"] = json!(7),
                found: ("structural.profile.field-type", Code::FieldTypeInvalid),
                waiting: "dependency.message-fallback.established fallback.sources.within-bound",
                kept: "",
            },
            Broken {
                place: "fallback entry",
                edit: |app| app["messageFallback"]["ja"] = json!(7),
                found: ("structural.profile.field-type", Code::FieldTypeInvalid),
                waiting: "dependency.message-fallback.established fallback.sequence.within-bound",
                kept: "",
            },
            Broken {
                place: "fallback candidate",
                edit: |app| app["messageFallback"]["ja"] = json!(["en", 7]),
                found: ("structural.profile.field-value", Code::FieldValueInvalid),
                waiting: "dependency.message-fallback.established",
                kept: "",
            },
            Broken {
                place: "coverage",
                edit: |app| app["coverage"] = json!(7),
                found: ("structural.profile.field-type", Code::FieldTypeInvalid),
                waiting: "coverage.cell.effective-mode coverage.comparisons.within-bound coverage.locale-selectors.within-bound coverage.rules.within-bound coverage.surface-selectors.within-bound coverage.table.complete dependency.coverage.established",
                kept: "",
            },
            Broken {
                place: "coverage rule",
                edit: |app| app["coverage"]["rules"][1] = json!(7),
                found: ("structural.profile.field-type", Code::FieldTypeInvalid),
                waiting: "coverage.cell.effective-mode coverage.table.complete dependency.coverage.established",
                kept: "coverage.domain.unique=2",
            },
            Broken {
                place: "coverage rule mode",
                edit: |app| app["coverage"]["rules"][0]["mode"] = json!("x"),
                found: ("structural.profile.field-value", Code::FieldValueInvalid),
                waiting: "coverage.cell.effective-mode coverage.table.complete dependency.coverage.established",
                kept: "coverage.cell.effective-mode=4",
            },
            Broken {
                place: "coverage selector item",
                edit: |app| app["coverage"]["rules"][0]["requestedLocales"] = json!(["ja", 7]),
                found: ("structural.profile.field-type", Code::FieldTypeInvalid),
                waiting: "coverage.cell.effective-mode coverage.table.complete dependency.coverage.established",
                kept: "coverage.domain.unique=2",
            },
            Broken {
                place: "required policy slot",
                edit: |app| app["policies"]["trust"] = json!(42),
                found: ("structural.profile.field-type", Code::FieldTypeInvalid),
                waiting: "artifact.policy-reference.resolved dependency.required-policies.established resource-policy.policy-reference.within-bound resource-policy.policy-reference.within-capability",
                kept: "artifact.policy-reference.resolved=6 resource-policy.policy-reference.within-bound=6 resource-policy.policy-reference.within-capability=6",
            },
            Broken {
                place: "missing policy slot",
                edit: |app| {
                    app["policies"].as_object_mut().unwrap().remove("approval");
                },
                found: ("structural.profile.required-field", Code::RequiredFieldMissing),
                waiting: "artifact.policy-reference.resolved dependency.required-policies.established resource-policy.policy-reference.within-bound resource-policy.policy-reference.within-capability",
                kept: "artifact.policy-reference.resolved=6 resource-policy.policy-reference.within-bound=6 resource-policy.policy-reference.within-capability=6",
            },
            Broken {
                place: "optional policy slot",
                edit: |app| app["policies"]["glossarySet"] = json!(42),
                found: ("structural.profile.field-value", Code::FieldValueInvalid),
                waiting: "artifact.policy-reference.resolved dependency.optional-policies.established resource-policy.policy-reference.within-bound resource-policy.policy-reference.within-capability",
                kept: "artifact.policy-reference.resolved=6 resource-policy.policy-reference.within-bound=6 resource-policy.policy-reference.within-capability=6",
            },
            Broken {
                place: "undeclared policies member",
                edit: |app| app["policies"]["extra"] = json!(1),
                found: ("structural.profile.known-field", Code::UnknownField),
                waiting: "dependency.optional-policies.established dependency.required-policies.established",
                kept: "",
            },
            Broken {
                place: "unresolved policy reference",
                edit: |app| app["policies"]["trust"]["revision"] = json!("2"),
                found: ("artifact.policy-reference.resolved", Code::ArtifactReferenceNotAdmitted),
                waiting: "dependency.required-policies.established",
                kept: "",
            },
            Broken {
                place: "targetProfiles",
                edit: |app| app["targetProfiles"] = json!(7),
                found: ("structural.profile.field-type", Code::FieldTypeInvalid),
                waiting: "dependency.deployment-groups.established dependency.targets.established group.member.known hydration.endpoint.known hydration.endpoint.role hydration.relation.same-effective-default hydration.relation.same-requested-locales hydration.relation.static-compatibility locale.target-profiles.within-bound",
                kept: "",
            },
            Broken {
                place: "target entry",
                edit: |app| app["targetProfiles"]["server"] = json!(7),
                found: ("structural.target.field-type", Code::FieldTypeInvalid),
                waiting: "artifact.target-profile-reference.resolved dependency.deployment-groups.established dependency.targets.established hydration.endpoint.role hydration.relation.same-effective-default hydration.relation.same-requested-locales hydration.relation.static-compatibility locale.target-default-requested.canonical resource-policy.target-profile-reference.within-bound resource-policy.target-profile-reference.within-capability target.effective-default.supported target.requested-locales.directed target.requested-locales.project-subset",
                kept: "artifact.target-profile-reference.resolved=1 hydration.endpoint.role=1 locale.target-requested.canonical=2 locale.target-requested.unique=2 resource-policy.target-profile-reference.within-bound=1 resource-policy.target-profile-reference.within-capability=1 target.effective-default.supported=1 target.requested-locales.directed=1 target.requested-locales.project-subset=1",
            },
            Broken {
                place: "target key",
                edit: |app| app["targetProfiles"]["Web App"] = app["targetProfiles"]["server"].clone(),
                found: ("structural.target-key.field-value", Code::FieldValueInvalid),
                waiting: "dependency.deployment-groups.established dependency.targets.established",
                kept: "",
            },
            Broken {
                place: "target locale element",
                edit: |app| app["targetProfiles"]["server"]["requestedLocales"] = json!(["en", 42]),
                found: ("structural.target.field-type", Code::FieldTypeInvalid),
                waiting: "dependency.deployment-groups.established dependency.targets.established hydration.relation.same-effective-default hydration.relation.same-requested-locales",
                kept: "locale.target-requested.canonical=3 locale.target-requested.unique=3",
            },
            Broken {
                place: "target invalid locale",
                edit: |app| app["targetProfiles"]["server"]["requestedLocales"] = json!(["en", "zz"]),
                found: ("locale.target-requested.canonical", Code::LocaleInvalid),
                waiting: "dependency.deployment-groups.established dependency.targets.established hydration.relation.same-effective-default hydration.relation.same-requested-locales locale.target-requested.unique",
                kept: "locale.target-requested.unique=3",
            },
            Broken {
                place: "target profile",
                edit: |app| app["targetProfiles"]["server"]["profile"] = json!(42),
                found: ("structural.target.field-type", Code::FieldTypeInvalid),
                waiting: "artifact.target-profile-reference.resolved dependency.deployment-groups.established dependency.targets.established hydration.endpoint.role hydration.relation.static-compatibility resource-policy.target-profile-reference.within-bound resource-policy.target-profile-reference.within-capability target.requested-locales.directed",
                kept: "artifact.target-profile-reference.resolved=1 hydration.endpoint.role=1 resource-policy.target-profile-reference.within-bound=1 resource-policy.target-profile-reference.within-capability=1 target.requested-locales.directed=1",
            },
            Broken {
                place: "unresolved target profile",
                edit: |app| app["targetProfiles"]["server"]["profile"]["revision"] = json!("2"),
                found: ("artifact.target-profile-reference.resolved", Code::ArtifactReferenceNotAdmitted),
                waiting: "dependency.deployment-groups.established dependency.targets.established hydration.endpoint.role hydration.relation.static-compatibility target.requested-locales.directed",
                kept: "hydration.endpoint.role=1 target.requested-locales.directed=1",
            },
            Broken {
                place: "deploymentGroups",
                edit: |app| app["deploymentGroups"] = json!(7),
                found: ("structural.profile.field-type", Code::FieldTypeInvalid),
                waiting: "dependency.deployment-groups.established group.groups.within-bound group.memberships.within-bound group.target.single-group hydration.relations.within-bound",
                kept: "",
            },
            Broken {
                place: "group entry",
                edit: |app| app["deploymentGroups"]["web"] = json!(7),
                found: ("structural.group.field-type", Code::FieldTypeInvalid),
                waiting: "dependency.deployment-groups.established group.members.within-bound group.memberships.within-bound group.target.single-group hydration.relations.within-bound",
                kept: "",
            },
            Broken {
                place: "group key",
                edit: |app| app["deploymentGroups"]["Bad Key"] = app["deploymentGroups"]["web"].clone(),
                found: ("structural.group-key.field-value", Code::FieldValueInvalid),
                waiting: "dependency.deployment-groups.established",
                kept: "",
            },
            Broken {
                place: "group member",
                edit: |app| app["deploymentGroups"]["web"]["members"] = json!(["browser", 7]),
                found: ("structural.group.field-type", Code::FieldTypeInvalid),
                waiting: "dependency.deployment-groups.established group.target.single-group hydration.endpoint.same-group",
                kept: "group.member.known=1 group.member.unique=1 group.member.within-bound=1 group.target.single-group=1 hydration.endpoint.same-group=1",
            },
            Broken {
                place: "hydration relation",
                edit: |app| app["deploymentGroups"]["web"]["hydrationRelations"] = json!([7]),
                found: ("structural.group.field-type", Code::FieldTypeInvalid),
                waiting: "dependency.deployment-groups.established",
                kept: "",
            },
            Broken {
                place: "hydration endpoint",
                edit: |app| app["deploymentGroups"]["web"]["hydrationRelations"][0]["client"] = json!(7),
                found: ("structural.group.field-type", Code::FieldTypeInvalid),
                waiting: "dependency.deployment-groups.established hydration.relation.distinct-endpoints hydration.relation.same-effective-default hydration.relation.same-requested-locales hydration.relation.static-compatibility hydration.relation.unique",
                kept: "hydration.endpoint.known=1 hydration.endpoint.role=1 hydration.endpoint.same-group=1 hydration.endpoint.within-bound=1 hydration.target.single-role=1",
            },
            Broken {
                place: "delivery",
                edit: |app| app["delivery"] = json!({"placement": "hoist"}),
                found: ("structural.profile.field-value", Code::FieldValueInvalid),
                waiting: "delivery.placement.effective-mode dependency.delivery.established",
                kept: "",
            },
        ]
    }

    /// The rows each row waits on through the Check Registry's prerequisites.
    type Requirements<'r> = BTreeMap<&'r str, BTreeSet<&'r str>>;

    /// What `evaluation` recorded, once it is checked against the Check
    /// Registry: a unit waits only on a Finding of the invocation whose row
    /// its row requires, and each semantic unit of `complete` whose row needs
    /// no Finding's row is still evaluated.
    fn only_dependents_wait(
        evaluation: &Evaluation,
        complete: &BTreeMap<Unit, Status>,
        requires: &Requirements<'_>,
        place: &str,
    ) -> BTreeMap<Unit, Status> {
        let found: BTreeSet<&IssueKey> = evaluation.issues().collect();
        let after = statuses(evaluation);
        for (unit, status) in &after {
            let Status::NotEvaluated(causes) = status else {
                continue;
            };
            for cause in causes.keys() {
                assert!(
                    found.contains(cause),
                    "{place}: {unit:?} waits on {cause:?}"
                );
                assert!(
                    requires[row_of(unit)].contains(row_of(cause.unit())),
                    "{place}: {} does not require {}",
                    row_of(unit),
                    row_of(cause.unit())
                );
            }
        }
        // Structural admission has units only where a value occurs (plan 025
        // interpretation #207), so an edit moves those of its own value.
        let semantic = complete
            .keys()
            .filter(|unit| unit.phase > Phase::ProfileSelection);
        for unit in semantic {
            let row = row_of(unit);
            let dependent = found.iter().any(|key| {
                let cause = row_of(key.unit());
                row == cause || requires[row].contains(cause)
            });
            if !dependent {
                assert_eq!(
                    after.get(unit),
                    Some(&Status::Evaluated),
                    "{place}: {unit:?} needs no Finding"
                );
            }
        }
        after
    }

    /// The Conformance rows that one invalid value stops only what needs it
    /// (D15:3340, 3445, 3446), over a whole invocation. Breaking each place
    /// in turn must report exactly its one Finding, leave not evaluated
    /// exactly the rows the case lists, each unit waiting on that Finding
    /// through the Check Registry's prerequisites, and keep the sibling units
    /// the case counts evaluated: what is independent of the value continues,
    /// and every row the registry does not make dependent on the Finding's
    /// row keeps every unit of the complete invocation.
    #[test]
    fn each_invalid_value_stops_only_the_checks_that_need_it() {
        let registry = CheckRegistry::revision_zero();
        let requires = requirements(&registry);
        let complete = Inputs::complete().resolve(|evaluation, _| statuses(&evaluation));
        let mut evaluated: BTreeMap<&str, usize> = BTreeMap::new();
        for unit in complete.keys() {
            *evaluated.entry(row_of(unit)).or_default() += 1;
        }
        for case in broken_places() {
            let place = case.place;
            let mut inputs = Inputs::complete();
            (case.edit)(inputs.declaration());
            inputs.resolve(|evaluation, _| {
                let key = only_issue(&evaluation);
                assert_eq!((row_of(key.unit()), key.code()), case.found, "{place}");
                let after = only_dependents_wait(&evaluation, &complete, &requires, place);
                let semantic = || {
                    after
                        .iter()
                        .filter(|(unit, _)| unit.phase > Phase::ProfileSelection)
                };
                let waiting: BTreeSet<&str> = semantic()
                    .filter(|(_, status)| matches!(status, Status::NotEvaluated(_)))
                    .map(|(unit, _)| row_of(unit))
                    .collect();
                assert_eq!(
                    waiting,
                    case.waiting.split_whitespace().collect(),
                    "{place}"
                );
                let mut kept: BTreeMap<&str, usize> = BTreeMap::new();
                for (unit, _) in semantic().filter(|(_, status)| **status == Status::Evaluated) {
                    *kept.entry(row_of(unit)).or_default() += 1;
                }
                kept.retain(|row, count| *count < evaluated[row]);
                let listed: BTreeMap<&str, usize> = case
                    .kept
                    .split_whitespace()
                    .map(|each| {
                        let (row, count) = each.split_once('=').unwrap();
                        (row, count.parse().unwrap())
                    })
                    .collect();
                assert_eq!(kept, listed, "{place}");
            });
        }
    }

    /// The `policies` object itself holds the selected reference, so without
    /// it no declaration is selected: the wide stop 015 allows for the
    /// selected declaration's boundary (D15:2131, 2367). Each slot still names
    /// its reference's units, which wait on that Finding.
    #[test]
    fn a_policies_object_that_failed_holds_the_selection_back() {
        let registry = CheckRegistry::revision_zero();
        let requires = requirements(&registry);
        let complete = Inputs::complete().resolve(|evaluation, _| statuses(&evaluation));
        let mut inputs = Inputs::complete();
        inputs.declaration()["policies"] = json!(7);
        inputs.attempt(|evaluation, state| {
            let state = state.unwrap().expect("entry admits the file");
            assert!(state.policy.confirmed.is_err());
            let found = BTreeSet::from([only_issue(&evaluation)]);
            only_dependents_wait(&evaluation, &complete, &requires, "policies");
            for unit in PolicySlot::ALL.map(resolved_slot) {
                waits_on_exactly(&evaluation, &unit, &found);
            }
        });
    }

    /// Independent values that fail together are all reported in the one
    /// invocation, each with the Finding it has alone, rather than stopping
    /// at the first (D15:2149, 3445).
    #[test]
    fn independent_failures_are_all_found_in_one_invocation() {
        let independent = [
            "projectId",
            "requestedLocales invalid locale",
            "defaultSourceLocale",
            "alias entry",
            "fallback candidate",
            "coverage rule mode",
            "required policy slot",
            "optional policy slot",
            "target locale element",
            "unresolved target profile",
            "group member",
            "hydration endpoint",
            "delivery",
        ];
        let cases = broken_places();
        let edits: Vec<&Broken> = independent
            .iter()
            .map(|place| cases.iter().find(|case| case.place == *place).unwrap())
            .collect();
        let mut alone = BTreeSet::new();
        let mut together = Inputs::complete();
        for case in &edits {
            let mut inputs = Inputs::complete();
            (case.edit)(inputs.declaration());
            alone.insert(inputs.resolve(|evaluation, _| only_issue(&evaluation)));
            (case.edit)(together.declaration());
        }
        assert_eq!(alone.len(), independent.len());
        let found = together
            .resolve(|evaluation, _| evaluation.issues().cloned().collect::<BTreeSet<IssueKey>>());
        assert_eq!(found, alone);
    }
}
