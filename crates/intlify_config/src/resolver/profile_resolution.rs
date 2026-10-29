// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Semantic Profile Resolver over one invocation: Phase 0 and Phases 4
//! to 13 of the Deterministic Resolution Algorithm, in 015's order, over one
//! evaluation. Each phase takes the values the earlier ones admitted, or the
//! causes of their absence, and evaluates every check whose prerequisites were
//! admitted, so a blocking result leaves only the checks that depend on it not
//! evaluated. The entry adds no check of its own.
//!
//! Phases 1 to 3 record no units yet, so the entry starts from their complete
//! results: a configuration structural admission admitted whole, and the
//! declaration Phase 3 provisionally selected in it. What the entry returns is
//! crate-private semantic state, not a `LocalizationProjectProfile`, a checked
//! or blocked outcome, or Resolution Evidence. The profile projection is 015's
//! Implementation Phase 5.

use crate::artifacts::collection::FormalCollection;
use crate::artifacts::vocabulary::FormalVocabulary;
use crate::input_limits::ValueCounts;
use crate::locale::{CanonicalizationFailure, Provider};
use crate::model::{IntlifyConfig, ProfileDeclaration};
use crate::references::{PolicyReference, TargetProfileReference};
use crate::structural::selection::{ProvisionalSelection, SelectionInvariant, SelectorInput};
use crate::structural::StructuralAnalysis;

use super::artifact_resolution::{admit_collection, ArtifactSet};
use super::bootstrap::VerificationInput;
use super::coverage::{resolve_coverage, CoveragePolicy};
use super::delivery::{resolve_delivery, DeliveryPolicy};
use super::evaluation::{Admitted, Causes, Evaluation, EvaluationInvariant};
use super::exact_resolution::{resolve_fields, ArtifactBindings, ResolvedReferences};
use super::fallback::{resolve_fallback, MessageFallbackPolicy};
use super::groups::{resolve_groups, ResolvedGroups};
use super::hydration::{validate_hydration, ResolvedHydration};
use super::identity::{resolve_project_identity, ProjectIdentity};
use super::limits::ImplementationCapability;
use super::locales::{resolve_locales, LocaleFields, LocaleInvariant, ResolvedLocales};
use super::negotiation::{resolve_negotiation, NegotiationProfile};
use super::resource_policy::{
    admit_resource_policy, ConfigurationMeasures, DeclarationReferences, PolicyInvariant,
    PolicyOutcome,
};
use super::specification::{
    admit_project_profile, admit_specifications, SpecificationInput, SpecificationSet,
    SupportedSpecifications,
};
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
    /// The UTF-8 bytes of every declared Profile ID, in the IDs' unsigned
    /// byte order.
    profile_ids: Vec<u64>,
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
            profile_ids: profiles
                .iter()
                .map(|(id, _)| {
                    u64::try_from(id.as_str().len()).expect("an in-memory length fits u64")
                })
                .collect(),
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
/// kept.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SemanticState<'s, Id> {
    /// Phase 4: the confirmed policy, which also confirms the provisional
    /// selection, and the selector's String Length Fact.
    pub(crate) policy: PolicyOutcome,
    /// Phases 0 and 5: the complete Profile Resolution Specification Set.
    pub(crate) specifications: Admitted<SpecificationSet<Id>>,
    /// Phase 5: the surface classes of the admitted vocabulary.
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

/// Phase 0 and Phases 4 to 13 of one invocation over `evaluation`, in 015's
/// order. `data` is the canonicalization data Phase 5 binds. The Profile
/// Resolution Artifact Set Phase 9 admits is kept in `set`, which the resolved
/// references and the bindings borrow.
pub(crate) fn resolve_profile<'s, P>(
    evaluation: &mut Evaluation,
    configuration: &CompleteConfiguration<'_>,
    invocation: &Invocation<'s, P::Identity>,
    data: P,
    set: &'s mut Option<Admitted<ArtifactSet<'s>>>,
) -> Result<SemanticState<'s, P::Identity>, ResolverInvariant>
where
    P: Provider,
    P::Identity: AsRef<str>,
{
    let declaration = configuration.declaration;
    let declared = configuration.references();
    // The assertion never selects semantics, so nothing waits on Phase 0.
    let project_profile =
        admit_project_profile(evaluation, invocation.supported, invocation.specifications)?;
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
    )?;
    let canonicalizer = admitted.canonicalizer.as_ref();
    let identity = resolve_project_identity(
        evaluation,
        confirmed,
        Ok(declaration.project_id.clone()),
        Ok(declaration.selection_scope.clone()),
    )?;
    let fields = LocaleFields::declared(declaration);
    let locales = resolve_locales(evaluation, confirmed, canonicalizer, &fields)?;
    let negotiation = resolve_negotiation(
        evaluation,
        confirmed,
        &admitted.locale_negotiation,
        &locales,
    )?;
    let fallback = resolve_fallback(
        evaluation,
        confirmed,
        &admitted.message_locale_fallback,
        &locales,
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
    let set = &*set.insert(admit_collection(
        evaluation,
        confirmed,
        invocation.collection,
    )?);
    // Each reference keeps its own result, which a target reads; the
    // bindings need every one.
    let results = resolve_fields(evaluation, set.as_ref(), &declared)?;
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
    let targets = resolve_targets(
        evaluation,
        confirmed,
        Ok(&declaration.target_profiles),
        &locales,
        &results,
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
    Ok(SemanticState {
        specifications: SpecificationSet::new(&project_profile, &admitted),
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
    use crate::artifacts::vectors::vocabulary::{self as vectors, formal};
    use crate::input_limits::{Bound, InputLimits, RawInputLimits};
    use crate::locale::fixtures::{fixture_pins, FixtureProvider};
    use crate::locale::{CanonicalLocale, ProviderFailure, Supported, VersionedIdentity};
    use crate::materialize::materialize_file;
    use crate::model::CoverageMode;
    use crate::profile_fixtures::complete_config;
    use crate::resolver::delivery::PlacementMode;
    use crate::resolver::evaluation::{
        Check, Code, IssueKey, Phase, Reason, Status, Subject, Unit,
    };
    use crate::resolver::exact_resolution;
    use crate::resolver::groups::{self, GroupSubject};
    use crate::resolver::hydration::{self, HydrationSubject};
    use crate::resolver::limits::{fixture_limits, ResolutionLimits, SECTION};
    use crate::resolver::locales::fixtures::canonical;
    use crate::resolver::locales::{self, LocaleKey, LocaleSubject, Location};
    use crate::resolver::resource_policy::{self, PolicySlot, PolicySubject, ReferenceField};
    use crate::resolver::specification::{Slot, VocabularyAssertion};
    use crate::resolver::targets::{self, TargetSubject};
    use crate::structural::selection::{Selection, StringLengthFact};
    use crate::structural::AuthoringSchema;

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
        members: u64::MAX,
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
    fn policy_limits() -> ResolutionLimits {
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

    /// Phases 1 and 2 over `config` under the capability's
    /// `configurationInput`.
    fn analyze(config: &Value, capability: &ResolutionLimits) -> Analysis {
        let input = InputLimits {
            raw: RawInputLimits {
                max_file_bytes: bound(1_000_000),
                max_parser_tokens: bound(1_000_000),
            },
            value: capability.configuration_input.value,
        };
        let doc = materialize_file(Arc::from(serde_json::to_vec(config).unwrap()), input).unwrap();
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
        members: Vec<Value>,
        specifications: SpecificationInput<Id>,
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
                members: vec![policy.clone()],
                specifications: specifications(),
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

        pub(crate) fn declaration(&mut self) -> &mut Value {
            &mut self.config["profiles"]["web"]
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
            let web = &mut config["profiles"]["web"];
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
                members,
                specifications: self.specifications.clone(),
                capability: self.capability.clone(),
                pinned: self.pinned.clone(),
            }
        }

        /// Phases 1 to 3 over the configuration, selecting `web`, and the
        /// invocation's other inputs, for `run`.
        fn prepared<R>(
            &self,
            run: impl FnOnce(&CompleteConfiguration<'_>, &Invocation<'_, Id>) -> R,
        ) -> R {
            let analysis = analyze(&self.config, &self.capability);
            let config = analysis.construct().unwrap().unwrap();
            let selector = SelectorInput::string(
                "web",
                self.capability
                    .configuration_input
                    .structural
                    .max_profile_id_bytes,
            );
            let Ok(Selection::Selected(selection)) = analysis.select(&selector) else {
                panic!("web is not selected");
            };
            let configuration =
                CompleteConfiguration::new(&analysis, &config, &selector, &selection).unwrap();
            let supported = SupportedSpecifications::revision_zero(
                ACTIVE,
                Supported::new(vec![fixture_pins()]).unwrap(),
            );
            let capability = ImplementationCapability::fixture_owned(self.capability.clone());
            let collection = collection(&self.members);
            let vocabulary = formal();
            let verification = VerificationInput::exact_pins([self.pinned.clone()]).unwrap();
            run(
                &configuration,
                &Invocation {
                    supported: &supported,
                    specifications: &self.specifications,
                    capability: &capability,
                    verification: &verification,
                    collection: &collection,
                    vocabulary: &vocabulary,
                },
            )
        }

        /// The entry over the prepared inputs; `inspect` reads what the
        /// invocation recorded and what it returned.
        fn attempt<R>(
            &self,
            inspect: impl FnOnce(Evaluation, Result<SemanticState<'_, Id>, ResolverInvariant>) -> R,
        ) -> R {
            self.prepared(|configuration, invocation| {
                let mut evaluation = Evaluation::default();
                let mut set = None;
                let state = resolve_profile(
                    &mut evaluation,
                    configuration,
                    invocation,
                    FixtureProvider::new(),
                    &mut set,
                );
                inspect(evaluation, state)
            })
        }

        pub(crate) fn resolve<R>(
            &self,
            inspect: impl FnOnce(Evaluation, SemanticState<'_, Id>) -> R,
        ) -> R {
            self.attempt(|evaluation, state| inspect(evaluation, state.unwrap()))
        }
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
    const EVERY: [&str; 14] = [
        "policy",
        "specifications",
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

    #[test]
    fn every_phase_admits_a_complete_invocation() {
        let inputs = Inputs::complete();
        inputs.resolve(|evaluation, state| {
            assert_eq!(issues(&evaluation), []);
            assert_eq!(evaluation.notices().count(), 0);
            assert!(evaluation
                .statuses()
                .all(|(_, status)| *status == Status::Evaluated));
            let phases: BTreeSet<Phase> =
                evaluation.statuses().map(|(unit, _)| unit.phase).collect();
            assert_eq!(phases, BTreeSet::from(PHASES));
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
                    PolicySubject::ProfileId(0),
                    PolicySubject::Selector,
                    PolicySubject::StructuralAnalysis
                ]
            );
            assert_eq!(absent(&state), EVERY);
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
            // Every unit of Phases 5 to 10, and Phase 4's after the decision,
            // waits on the decision alone; Phase 0 does not.
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
    fn a_missing_project_profile_slot_blocks_phase_0_and_nothing_waits_on_it() {
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
            assert!(evaluation
                .statuses()
                .all(|(_, status)| *status == Status::Evaluated));
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
            // only the bindings, which need every reference, are missing.
            assert!(evaluation
                .statuses()
                .all(|(_, status)| *status == Status::Evaluated));
            let key = only_issue(&evaluation);
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
            // Only the relation's comparison of the two locale sets waits.
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
            assert_eq!(waiting, [&compared]);
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

    #[test]
    fn an_evaluation_runs_one_invocation() {
        Inputs::complete().prepared(|configuration, invocation| {
            let mut evaluation = Evaluation::default();
            let (mut first, mut second) = (None, None);
            assert!(resolve_profile(
                &mut evaluation,
                configuration,
                invocation,
                FixtureProvider::new(),
                &mut first
            )
            .is_ok());
            assert_eq!(
                resolve_profile(
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
