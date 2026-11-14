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
//! Phase 2 records its scope levels' units, and Phase 3 its units, with their
//! Findings later (plan 025 PR 5f-2d, 5f-3), so the entry starts from their
//! complete results: a configuration structural admission admitted whole, and
//! the declaration Phase 3 provisionally selected in it. What the entry
//! returns is crate-private semantic state, not a `LocalizationProjectProfile`,
//! a checked or blocked outcome, or Resolution Evidence. The profile
//! projection is 015's Implementation Phase 5.

use crate::artifacts::collection::FormalCollection;
use crate::artifacts::vocabulary::FormalVocabulary;
use crate::boundary::capability::ImplementationCapability;
use crate::boundary::verification::VerificationInput;
use crate::input_limits::ValueCounts;
use crate::locale::{
    CanonicalizationFailure, FormalData, Provider, SemanticPins, VersionedIdentity,
};
use crate::model::{IntlifyConfig, ProfileDeclaration, ProfileId};
use crate::references::{PolicyReference, TargetProfileReference};
use crate::structural::selection::{ProvisionalSelection, SelectionInvariant, SelectorInput};
use crate::structural::StructuralAnalysis;

use super::artifact_resolution::{admit_collection, ArtifactSet};
use super::coverage::{resolve_coverage, CoveragePolicy};
use super::delivery::{resolve_delivery, DeliveryPolicy};
use super::dependency::{own, SemanticGroup, OPTIONAL_POLICIES, REQUIRED_POLICIES};
use super::entry::EntryInvariant;
use super::evaluation::{and, Admitted, Causes, Evaluation, EvaluationInvariant};
use super::exact_resolution::{resolve_fields, ArtifactBindings, ResolvedReferences};
use super::fallback::{resolve_fallback, MessageFallbackPolicy};
use super::groups::{resolve_groups, ResolvedGroups};
use super::hydration::{validate_hydration, ResolvedHydration};
use super::identity::{resolve_project_identity, ProjectIdentity};
use super::locales::{resolve_locales, LocaleFields, LocaleInvariant, ResolvedLocales};
use super::negotiation::{resolve_negotiation, NegotiationProfile};
use super::resource_policy::{
    admit_resource_policy, ConfigurationMeasures, DeclarationReferences, PolicyInvariant,
    PolicyOutcome,
};
use super::specification::{
    admit_specifications, SpecificationInput, SpecificationSet, SupportedSpecifications,
};
use super::structural_admission::StructuralInvariant;
use super::targets::{resolve_targets, ResolvedTargets};
use super::vocabulary::SurfaceVocabulary;

type Analysis = StructuralAnalysis<PolicyReference, TargetProfileReference>;
type Config = IntlifyConfig<PolicyReference, TargetProfileReference>;
type Declaration = ProfileDeclaration<PolicyReference, TargetProfileReference>;

/// One invocation's configuration past Phases 1 to 3: the declaration Phase 3
/// provisionally selected in a configuration structural admission admitted
/// whole, the `resourceLimits` reference it selected with, the selector, and
/// what Phase 4 rechecks of those phases' measures.
pub(crate) struct CompleteConfiguration<'a> {
    declaration: &'a Declaration,
    resource_limits: &'a PolicyReference,
    selector: &'a SelectorInput,
    value: ValueCounts,
    /// Every declared Profile ID, in their unsigned byte order.
    profile_ids: Vec<ProfileId>,
    structural_units: u64,
}

impl<'a> CompleteConfiguration<'a> {
    /// The configuration when structural admission admitted the whole root of
    /// `analysis`, `config` is the model it constructed, and Phase 3
    /// provisionally selected one of its declarations with `selector`; none
    /// otherwise. A configuration admitted only in part enters 015's
    /// resolution with fragment admission statuses, which come with
    /// Implementation Phase 5.
    pub(crate) fn new(
        analysis: &Analysis,
        config: &'a Config,
        selector: &'a SelectorInput,
        selection: &'a ProvisionalSelection<PolicyReference>,
    ) -> Option<Self> {
        if !analysis.is_complete() {
            return None;
        }
        let profiles = config.profiles();
        Some(Self {
            declaration: profiles.get(selection.id())?,
            resource_limits: selection.resource_limits(),
            selector,
            value: analysis.value_counts(),
            profile_ids: profiles.iter().map(|(id, _)| id.clone()).collect(),
            structural_units: analysis.structural_units()?,
        })
    }

    fn references(&self) -> DeclarationReferences<'a> {
        DeclarationReferences {
            policies: Ok(&self.declaration.policies),
            target_profiles: Ok(&self.declaration.target_profiles),
        }
    }

    fn measures(&self) -> ConfigurationMeasures<'_> {
        ConfigurationMeasures {
            value: Ok(self.value),
            profile_ids: Ok(&self.profile_ids),
            selector: self.selector,
            structural_units: Ok(self.structural_units),
        }
    }
}

/// One invocation's inputs besides the configuration, as resolver
/// construction and 015's pre-invocation boundary admitted them: what the
/// resolver supports, the Profile Resolution Specification Input, the Admitted
/// Implementation Capability, 018's Resource Limit Policy Verification Input,
/// and the formally admitted collection and vocabulary. Until 015's
/// Implementation Phase 5 constructs them, only tests do.
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

impl From<PolicyInvariant> for ResolverInvariant {
    fn from(invariant: PolicyInvariant) -> Self {
        match invariant {
            PolicyInvariant::Evaluation(invariant) => Self::Evaluation(invariant),
            PolicyInvariant::Selection(invariant) => Self::Selection(invariant),
        }
    }
}

impl From<LocaleInvariant> for ResolverInvariant {
    fn from(invariant: LocaleInvariant) -> Self {
        match invariant {
            LocaleInvariant::Evaluation(invariant) => Self::Evaluation(invariant),
            LocaleInvariant::Canonicalization(failure) => Self::Canonicalization(failure),
        }
    }
}

/// Phases 4 to 13 of one invocation over `evaluation`, in 015's order, with
/// each semantic group's owner once the phases that establish the group have
/// run. `project_profile` is what Phase 0 admitted, which the invocation
/// records before configuration entry. `data` is the canonicalization data past the
/// pre-invocation boundary, which Phase 5 reads the rest of and binds. The
/// Profile Resolution Artifact Set Phase 9 admits is kept in `set`, which the
/// resolved references and the bindings borrow.
pub(crate) fn resolve_profile<'s, P, D>(
    evaluation: &mut Evaluation,
    project_profile: &Admitted<VersionedIdentity<P::Identity>>,
    configuration: &CompleteConfiguration<'_>,
    invocation: &Invocation<'s, P::Identity>,
    data: D,
    set: &'s mut Option<Admitted<ArtifactSet<'s>>>,
) -> Result<SemanticState<'s, P::Identity>, ResolverInvariant>
where
    P: Provider,
    P::Identity: AsRef<str>,
    D: FormalData<Provider = P>,
{
    let declaration = configuration.declaration;
    let declared = configuration.references();
    let policy = admit_resource_policy(
        evaluation,
        invocation.capability,
        invocation.collection,
        Ok(configuration.resource_limits),
        &declared,
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
    let canonicalization = canonicalizer
        .map(|canonicalizer| canonicalizer.binding().pins.clone())
        .map_err(Causes::clone);
    let identity = resolve_project_identity(
        evaluation,
        confirmed,
        Ok(declaration.project_id.clone()),
        Ok(declaration.selection_scope.clone()),
    )?;
    own(
        evaluation,
        SemanticGroup::ProjectIdentity,
        held(identity.as_ref()),
    )?;
    let fields = LocaleFields::declared(declaration);
    let locales = resolve_locales(evaluation, confirmed, canonicalizer, &fields)?;
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
        fields.message_fallback.clone(),
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
        admitted.vocabulary.as_ref(),
        canonicalizer,
        &locales,
        fields.coverage.clone(),
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
        locales.targets.as_deref(),
    )?;
    // Both policy groups are the confirmed declaration's, and the required
    // one also holds the confirmed policy's bounds.
    let policies = and(held(confirmed), held(declared.policies.as_ref()));
    own(
        evaluation,
        SemanticGroup::RequiredPolicies,
        and(policies.clone(), results.policies(&REQUIRED_POLICIES)),
    )?;
    own(
        evaluation,
        SemanticGroup::OptionalPolicies,
        and(policies, results.policies(&OPTIONAL_POLICIES)),
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
        Ok(declaration.delivery.as_option()),
    )?;
    own(evaluation, SemanticGroup::Delivery, held(delivery.as_ref()))?;
    let targets = resolve_targets(
        evaluation,
        Ok(&declaration.target_profiles),
        &locales,
        &results,
    )?;
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
        Ok(&declaration.deployment_groups),
        targets.as_ref(),
    )?;
    // Every relation shares the negotiation profile; none compares it.
    let hydration = validate_hydration(
        evaluation,
        confirmed,
        Ok(&declaration.deployment_groups),
        targets.as_ref(),
        groups.as_ref(),
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

// `handoff`'s tests resolve the same inputs.
#[cfg(test)]
pub(super) mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    use intlify_shared_json::token::Token;
    use serde_json::{json, Value};

    use super::*;
    use crate::artifacts::collection::{read_collection, CollectionLimits};
    use crate::artifacts::content::sample;
    use crate::artifacts::envelope::{envelope_of, ArtifactKind, ExactTuple, FormalMember};
    use crate::artifacts::vectors::vocabulary::{self as vectors, formal_of};
    use crate::construction::registry::check::testing::{check_id, requirements};
    use crate::construction::registry::check::CheckRegistry;
    use crate::input_limits::{Bound, InputLimits, RawInputLimits};
    use crate::locale::fixtures::{fixture_pins, FixtureProvider};
    use crate::locale::{
        CanonicalLocale, DataNotAdmitted, LocaleDataLimits, ProviderFailure, Supported,
        VersionedIdentity,
    };
    use crate::materialize::materialize_file;
    use crate::model::CoverageMode;
    use crate::profile_fixtures::complete_config;
    use crate::resolver::delivery::PlacementMode;
    use crate::resolver::dependency::{self, SemanticGroup};
    use crate::resolver::entry::fixtures::FileFixture;
    use crate::resolver::entry::{admit_entry, input, EntryFile, OWNER};
    use crate::resolver::evaluation::{
        Check, Code, IssueKey, Phase, Reason, Status, Subject, Unit,
    };
    use crate::resolver::exact_resolution;
    use crate::resolver::groups::{self, GroupSubject};
    use crate::resolver::hydration::{self, HydrationSubject};
    use crate::resolver::limits::{fixture_limits, ResolutionLimits, SECTION};
    use crate::resolver::locales::fixtures::canonical;
    use crate::resolver::locales::{self, LocaleKey, LocaleSubject, Location};
    use crate::resolver::profile_view::{project, ProfileView};
    use crate::resolver::resource_policy::{self, PolicySlot, PolicySubject, ReferenceField};
    use crate::resolver::specification::{
        admit_project_profile, ActiveProfileSpecification, Slot, VocabularyAssertion, DATA,
    };
    use crate::resolver::structural_admission::{admit_structure, selection, CAPABILITY};
    use crate::resolver::subject::SubjectKind;
    use crate::resolver::targets::{self, TargetSubject};
    use crate::structural::selection::{Selection, StringLengthFact};
    use crate::structural::{AuthoringSchema, StructuralLimits};

    type Id = &'static str;
    type Schema = AuthoringSchema<PolicyReference, TargetProfileReference>;

    const ACTIVE: VersionedIdentity<Id> = VersionedIdentity {
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
        read_collection(Some(&bytes), &ROOM).unwrap()
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
    fn reverse_objects(value: &mut Value) {
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

    /// One invocation's inputs, which a test edits before resolving them.
    pub(crate) struct Inputs {
        config: Value,
        /// The Profile ID the selector names.
        selected: &'static str,
        members: Vec<Value>,
        specifications: SpecificationInput<Id>,
        /// The members of the vocabulary submitted against its slot.
        surfaces: &'static [&'static str],
        capability: ResolutionLimits,
        /// What trusted setup pinned in 018's Verification Input: the policy
        /// the declaration selects, whatever a test then changes.
        pinned: ExactTuple,
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
                members: vec![policy.clone()],
                specifications: specifications(),
                surfaces: &vectors::MEMBERS,
                capability: capability_limits(),
                pinned: tuple_of(&policy),
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

        /// The same inputs with the selected declaration declared again under
        /// `id`, which the selector then names: two declarations differing
        /// only in their Profile IDs.
        pub(crate) fn selecting(&self, id: &'static str) -> Self {
            let mut config = self.config.clone();
            config["profiles"][id] = config["profiles"][self.selected].clone();
            Self {
                config,
                selected: id,
                members: self.members.clone(),
                specifications: self.specifications.clone(),
                surfaces: self.surfaces,
                capability: self.capability.clone(),
                pinned: self.pinned.clone(),
            }
        }

        /// Selects `policy`, submitted first in place of the selected one,
        /// and pins it.
        fn select_policy(&mut self, policy: Value) {
            self.pinned = tuple_of(&policy);
            self.declaration()["policies"]["resourceLimits"] = reference_value(&self.pinned);
            self.members[0] = policy;
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
                members,
                specifications: self.specifications.clone(),
                surfaces: self.surfaces,
                capability: self.capability.clone(),
                pinned: self.pinned.clone(),
            }
        }

        /// The selector that names the declaration `selected`.
        fn selector(&self) -> SelectorInput {
            SelectorInput::string(
                self.selected,
                self.capability
                    .configuration_input
                    .structural
                    .max_profile_id_bytes,
            )
        }

        /// Phases 1 to 3 over the configuration outside any evaluation,
        /// selecting the declaration, and the invocation's other inputs, for
        /// `run`.
        fn prepared<R>(
            &self,
            run: impl FnOnce(&CompleteConfiguration<'_>, &Invocation<'_, Id>) -> R,
        ) -> R {
            let analysis = analyze(&self.config, &self.capability);
            let config = analysis.construct().unwrap().unwrap();
            let selector = self.selector();
            let Ok(Selection::Selected(selection)) = analysis.select(&selector) else {
                panic!("{} is not selected", self.selected);
            };
            let configuration =
                CompleteConfiguration::new(&analysis, &config, &selector, &selection).unwrap();
            self.invoked(|invocation| run(&configuration, invocation))
        }

        /// The invocation's inputs besides the configuration, for `run`.
        fn invoked<R>(&self, run: impl FnOnce(&Invocation<'_, Id>) -> R) -> R {
            let supported = SupportedSpecifications::revision_zero(
                ActiveProfileSpecification::fixture(ACTIVE),
                Supported::new(vec![fixture_pins()]).unwrap(),
            );
            let capability = ImplementationCapability::fixture_owned(self.capability.clone());
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
                let state = invoke_entry(
                    &mut evaluation,
                    (&file, limits),
                    (self.capability.configuration_input.structural, &selector),
                    invocation,
                    FixtureProvider::new(),
                    &mut set,
                );
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
                let state = invoke_entry(
                    &mut evaluation,
                    (&file, limits),
                    (self.capability.configuration_input.structural, &selector),
                    invocation,
                    FixtureProvider::new(),
                    &mut set,
                )
                .unwrap()
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
    }

    /// One invocation's entry over `evaluation` past a configuration prepared
    /// outside it: Phase 0, which runs before configuration entry, the
    /// `profileSpecification` group's owner that Phase 2 records, and then
    /// the semantic phases over `configuration`.
    fn invoke<'s, P, D>(
        evaluation: &mut Evaluation,
        configuration: &CompleteConfiguration<'_>,
        invocation: &Invocation<'s, P::Identity>,
        data: D,
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

    /// One invocation over `evaluation` from its entry: Phase 0, then Phase 1
    /// over `file` under `limits`, Phase 2 over the value Phase 1 admits with
    /// the model's configuration schema under `structural`, Phase 3 selecting
    /// `selector`'s declaration, and the semantic phases over the
    /// configuration. Without a value there is no Shared projection (D15:2369),
    /// and without a configuration structural admission admitted whole and its
    /// selected declaration, no semantic phase starts yet (plan 025 PR 5f-2f):
    /// either way there is no semantic state.
    fn invoke_entry<'s, P, D>(
        evaluation: &mut Evaluation,
        (file, limits): (&EntryFile<'_>, InputLimits),
        (structural, selector): (StructuralLimits, &SelectorInput),
        invocation: &Invocation<'s, P::Identity>,
        data: D,
        set: &'s mut Option<Admitted<ArtifactSet<'s>>>,
    ) -> Result<Option<SemanticState<'s, P::Identity>>, ResolverInvariant>
    where
        P: Provider,
        P::Identity: AsRef<str>,
        D: FormalData<Provider = P>,
    {
        let project_profile =
            admit_project_profile(evaluation, invocation.supported, invocation.specifications)?;
        let Ok(document) = admit_entry(evaluation, file, limits)? else {
            return Ok(None);
        };
        let schema = Schema::for_model().unwrap();
        let Ok(analysis) = admit_structure(
            evaluation,
            document,
            &schema,
            structural,
            held(project_profile.as_ref()),
        )?
        else {
            return Ok(None);
        };
        let Some(config) = analysis.construct().unwrap() else {
            return Ok(None);
        };
        let Ok(Selection::Selected(selection)) = analysis.select(selector) else {
            return Ok(None);
        };
        let Some(configuration) =
            CompleteConfiguration::new(&analysis, &config, selector, &selection)
        else {
            return Ok(None);
        };
        resolve_profile(
            evaluation,
            &project_profile,
            &configuration,
            invocation,
            data,
            set,
        )
        .map(Some)
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
        assert!(admit_entry(&mut evaluation, &file, limits).unwrap().is_ok());
        let Err(twice) = admit_entry(&mut evaluation, &file, limits) else {
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
            let Ok(Err(causes)) = admit_entry(&mut evaluation, &file, limits) else {
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
            let state = invoke_entry(
                &mut evaluation,
                (&file, limits),
                (inputs.capability.configuration_input.structural, &selector),
                invocation,
                FixtureProvider::new(),
                &mut set,
            );
            assert!(matches!(state, Ok(None)));
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

    /// When the schema selection selects no schema, Phase 2 records its
    /// owner, the selection's checks, the analysis's measure waiting on the
    /// failed check, and the `profileSpecification` group's owner, and no
    /// semantic phase starts (D15:2130).
    #[test]
    fn a_configuration_without_a_selected_schema_starts_no_semantic_phase() {
        let mut inputs = Inputs::complete();
        inputs.config["schemaVersion"] = json!("1");
        inputs.attempt(|evaluation, state| {
            assert!(matches!(state, Ok(None)));
            let phases: BTreeSet<Phase> =
                evaluation.statuses().map(|(unit, _)| unit.phase).collect();
            assert_eq!(
                phases,
                BTreeSet::from([
                    Phase::ResolverBootstrapAdmission,
                    Phase::EntryAdmission,
                    Phase::StructuralAdmission
                ])
            );
            assert_eq!(
                issues(&evaluation),
                [(
                    selection(Check::SupportedSchemaVersion),
                    Code::SchemaVersionUnsupported,
                    None
                )]
            );
            let waiting: Vec<&Unit> = evaluation
                .statuses()
                .filter(|(_, status)| matches!(status, Status::NotEvaluated(_)))
                .map(|(unit, _)| unit)
                .collect();
            assert_eq!(waiting, [&CAPABILITY]);
        });
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
            // selection's four checks, the analysis's measure, and the
            // `profileSpecification` group's owner (D15:2130, 2516).
            let phases: BTreeSet<Phase> =
                evaluation.statuses().map(|(unit, _)| unit.phase).collect();
            let mut ran = BTreeSet::from(PHASES);
            ran.extend([Phase::EntryAdmission, Phase::StructuralAdmission]);
            assert_eq!(phases, ran);
            let count = |phase| {
                evaluation
                    .statuses()
                    .filter(|(unit, _)| unit.phase == phase)
                    .count()
            };
            assert_eq!(count(Phase::EntryAdmission), 11);
            assert_eq!(count(Phase::StructuralAdmission), 7);
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

    /// Every check of every kind but the kinds declared only, of which no
    /// invocation records a unit yet.
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
        for inputs in [Inputs::complete(), Inputs::two_groups()] {
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
                    // projection, which no prerequisite names, and without a
                    // configuration structural admission admitted whole no
                    // semantic phase starts yet (plan 025 PR 5f-2f): the
                    // phases the invocation did not reach wait on no
                    // prerequisite.
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
        for row in &registry.rows {
            for prerequisite in &row.prerequisite_check_ids {
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

    #[test]
    fn two_spellings_of_one_requested_locale_are_a_duplicate_keeping_both() {
        let mut inputs = Inputs::complete();
        inputs.declaration()["requestedLocales"] = json!(["ja", "en-US", "en", "EN-us"]);
        inputs.resolve(|evaluation, state| {
            let subject = LocaleSubject::RequestedLocale(LocaleKey::Canonical(canonical("en-US")));
            let duplicate = locales::unit(subject.clone(), Check::UniqueLocale);
            assert_eq!(
                issues(&evaluation),
                [(duplicate, Code::LocaleDuplicate, None)]
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
        inputs.specifications.project_profile = None;
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
                HydrationSubject::Relation(
                    serde_json::from_value(json!("web")).unwrap(),
                    server,
                    browser,
                ),
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
                FixtureProvider::new(),
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
                    assert_eq!(first_evaluation, second_evaluation);
                    assert_eq!(unlocated(first), unlocated(second));
                });
            });
        }
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

    #[test]
    fn only_a_complete_configuration_with_its_selection_starts_a_resolution() {
        let complete = Inputs::complete();
        let capability = capability_limits();
        let analysis = analyze(&complete.config, &capability);
        let config = analysis.construct().unwrap().unwrap();
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
        assert!(CompleteConfiguration::new(&analysis, &config, &selector, &selection).is_some());
        // An analysis structural admission did not admit whole.
        let mut partial = complete.config.clone();
        partial["profiles"]["web"]["projectId"] = json!("Store Front");
        let partial = analyze(&partial, &capability);
        assert!(!partial.is_complete());
        assert!(CompleteConfiguration::new(&partial, &config, &selector, &selection).is_none());
        // A selection the model does not declare.
        let mut other = complete.config.clone();
        let profiles = other["profiles"].as_object_mut().unwrap();
        let web = profiles.remove("web").unwrap();
        profiles.insert("admin".into(), web);
        let other = analyze(&other, &capability).construct().unwrap().unwrap();
        assert!(CompleteConfiguration::new(&analysis, &other, &selector, &selection).is_none());
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
        struct Behind(LocaleDataLimits);

        impl FormalData for Behind {
            type Provider = FixtureProvider;

            fn admit(self, limits: &LocaleDataLimits) -> Result<FixtureProvider, DataNotAdmitted> {
                if *limits == self.0 {
                    Ok(FixtureProvider::new())
                } else {
                    Err(DataNotAdmitted)
                }
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
            let resolved = |data: Behind| {
                let mut evaluation = Evaluation::default();
                let mut set = None;
                let canonicalized =
                    invoke(&mut evaluation, configuration, &invocation, data, &mut set)
                        .unwrap()
                        .canonicalization
                        .is_ok();
                (issues(&evaluation), canonicalized)
            };
            assert_eq!(resolved(Behind(bounds)), (vec![], true));
            let mut other = bounds;
            other.max_codes = Bound::new(bounds.max_codes.get() + 1).unwrap();
            let (found, canonicalized) = resolved(Behind(other));
            assert_eq!(found, [(DATA, Code::LocaleDataNotAdmitted, None)]);
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
                FixtureProvider::new(),
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
                FixtureProvider::new(),
                &mut first
            )
            .is_ok());
            assert_eq!(
                invoke(
                    &mut evaluation,
                    configuration,
                    invocation,
                    FixtureProvider::new(),
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
}
