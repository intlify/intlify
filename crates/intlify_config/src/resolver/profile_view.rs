// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Phase 14, `profile-projection`: a checked profile's canonical semantic
//! projection, presented as its profile view (D15:1189-1239, 2142).
//!
//! The view is the closed, number-free JSON value with one member per semantic
//! group, in the order the Profile Specification's `semanticGroups` lists
//! them. A locale is its canonical identifier, a reference its five exact
//! members, and `projectProfileResolution` the admitted bounds as decimal
//! strings. Sets are arrays in their canonical order, maps are objects,
//! fallback candidates keep their semantic order, and explicit absence is
//! `null`. Two profiles are equal exactly when their views are: the Profile
//! Specification's identity, revision, and semantic digest, and every other
//! field (D15:191). The profile semantic digest is 017's `H` over the view
//! (D17:1436). The projection leaves out the Profile ID, the selector, the
//! configuration schema version, authoring evidence, Findings, the Coverage
//! Decision Basis and Evidence, and redundant authoring rules (D15:1208).

use std::collections::BTreeMap;

use intlify_shared_json::encoding::{hash, Domain, EncodingFailure};
use intlify_shared_json::token::Token;
use serde::Serialize;
use serde_json::Value;

use crate::artifacts::envelope::ExactTuple;
use crate::artifacts::SemanticDigest;
use crate::locale::{CanonicalLocale, VersionedIdentity};
use crate::model::{CoverageMode, IntentSourceKind};

use super::delivery::PlacementMode;
use super::evaluation::{both, Admitted, Causes};
use super::fallback::Candidate;
use super::hydration::ResolvedHydration;
use super::locales::LocaleSet;
use super::profile_resolution::SemanticState;
use super::resource_policy::{PolicySlot, ReferenceField};
use super::specification::SupportedSpecifications;
use super::targets::ResolvedTargets;

/// The digest domain of a profile view, which 017 registers (D17:1436).
const DOMAIN: &str = "localization-project-profile-semantic-digest";

/// A checked profile's view: its twelve semantic groups, in the order of the
/// Profile Specification's `semanticGroups`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfileView {
    profile_specification: Pinned,
    project_identity: ProjectIdentity,
    locale_canonicalization: LocaleCanonicalization,
    project_locales: ProjectLocales,
    locale_negotiation: LocaleNegotiation,
    message_fallback: MessageFallback,
    coverage: Coverage,
    required_policies: RequiredPolicies,
    optional_policies: OptionalPolicies,
    targets: BTreeMap<String, Target>,
    deployment_groups: BTreeMap<String, DeploymentGroup>,
    delivery: Delivery,
}

impl ProfileView {
    /// The profile semantic digest: `H` over the view under [`DOMAIN`]. The
    /// view holds no number, so only a view beyond the encoding's own depth or
    /// capacity has none.
    pub(crate) fn semantic_digest(&self) -> Result<SemanticDigest, EncodingFailure> {
        let view = serde_json::to_value(self).map_err(|_| EncodingFailure::Serialization)?;
        hash(Domain::literal(DOMAIN), &view).map(SemanticDigest::from_hash)
    }
}

/// A specification's identity and revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Specification {
    identity: String,
    revision: String,
}

impl Specification {
    fn of<Id: AsRef<str>>(specification: &VersionedIdentity<Id>) -> Self {
        Self {
            identity: specification.identity.as_ref().to_owned(),
            revision: specification.revision.as_ref().to_owned(),
        }
    }
}

/// An identity at an exact revision, pinned by its semantic digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Pinned {
    identity: String,
    revision: String,
    semantic_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectIdentity {
    project_id: String,
    selection_scope: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Dataset {
    identity: String,
    semantic_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct LocaleCanonicalization {
    specification: Specification,
    dataset: Dataset,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectLocales {
    requested_locales: Vec<String>,
    default_requested_locale: String,
    default_source_locale: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct LocaleNegotiation {
    specification: Specification,
    aliases: BTreeMap<String, String>,
}

/// One candidate of a fallback sequence: a definition locale, or the source
/// locale of each Intent, spelled as the authoring model spells it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
enum FallbackCandidate {
    Locale(String),
    IntentSource { kind: IntentSourceKind },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MessageFallback {
    specification: Specification,
    mappings: BTreeMap<String, Vec<FallbackCandidate>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Coverage {
    specification: Specification,
    vocabulary: Pinned,
    default_mode: CoverageMode,
    decisions: BTreeMap<String, BTreeMap<String, CoverageMode>>,
}

/// An exact reference: its five members, as a declaration writes them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Reference {
    kind: String,
    identity: String,
    revision: String,
    specification_revision: String,
    semantic_digest: String,
}

impl Reference {
    fn of(tuple: &ExactTuple) -> Self {
        Self {
            kind: tuple.kind.spelling().to_owned(),
            identity: tuple.identity.as_str().to_owned(),
            revision: tuple.revision.as_str().to_owned(),
            specification_revision: tuple.specification_revision.as_str().to_owned(),
            semantic_digest: tuple.semantic_digest.as_str().to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct RequiredPolicies {
    resource_limits: Reference,
    trust: Reference,
    source_admission: Reference,
    approval: Reference,
    selection: Reference,
    project_profile_resolution: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct OptionalPolicies {
    provider_routing: Option<Reference>,
    glossary_set: Option<Reference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Target {
    profile: Reference,
    requested_locales: Vec<String>,
    default_requested_locale: Option<String>,
    effective_default_requested_locale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HydrationRelation {
    server: String,
    client: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeploymentGroup {
    members: Vec<String>,
    hydration_relations: Vec<HydrationRelation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Delivery {
    graph: Specification,
    placement: Specification,
    mode: PlacementMode,
}

/// Phase 14 over one invocation's semantic state and the resolver's supported
/// specifications: the view of the checked profile, or the causes of the
/// results it needs and does not have. It needs a complete `IntlifyConfig`,
/// so structural admission must have admitted the whole root, and every
/// semantic group (D15:2142, 2367): it has a view exactly when the root was
/// admitted whole and every group's owner was evaluated, and otherwise waits
/// on the structural Findings and on what those owners wait on.
pub(crate) fn project<Id: AsRef<str>>(
    state: &SemanticState<'_, Id>,
    supported: &SupportedSpecifications<Id>,
) -> Admitted<ProfileView> {
    let identities = both(
        both(
            profile_specification(state, supported),
            project_identity(state),
        ),
        both(locale_canonicalization(state), project_locales(state)),
    );
    let policies = both(
        both(locale_negotiation(state), message_fallback(state)),
        both(coverage(state), policies(state)),
    );
    let deployment = both(
        both(targets(state), deployment_groups(state)),
        delivery(state),
    );
    let structure = state.structure.clone();
    both(structure, both(both(identities, policies), deployment)).map(
        |(
            (),
            (
                (
                    (
                        (profile_specification, project_identity),
                        (locale_canonicalization, project_locales),
                    ),
                    (
                        (locale_negotiation, message_fallback),
                        (coverage, (required_policies, optional_policies)),
                    ),
                ),
                ((targets, deployment_groups), delivery),
            ),
        )| ProfileView {
            profile_specification,
            project_identity,
            locale_canonicalization,
            project_locales,
            locale_negotiation,
            message_fallback,
            coverage,
            required_policies,
            optional_policies,
            targets,
            deployment_groups,
            delivery,
        },
    )
}

/// A result's value by reference, or the causes of its absence.
fn borrowed<T>(result: &Admitted<T>) -> Admitted<&T> {
    result.as_ref().map_err(Causes::clone)
}

/// A result's value once `check` holds of it too, or the causes of whichever
/// does not.
fn holding<T>(result: &Admitted<T>, check: impl FnOnce(&T) -> Admitted<()>) -> Admitted<&T> {
    borrowed(result).and_then(|value| check(value).map(|()| value))
}

fn locales(set: &LocaleSet) -> Vec<String> {
    set.as_slice().iter().map(locale).collect()
}

fn locale(locale: &CanonicalLocale) -> String {
    locale.as_str().to_owned()
}

/// The Active Project Profile Specification, once the complete Profile
/// Resolution Specification Set holds the `project-profile` assertion Phase 0
/// found to name it.
fn profile_specification<Id: AsRef<str>>(
    state: &SemanticState<'_, Id>,
    supported: &SupportedSpecifications<Id>,
) -> Admitted<Pinned> {
    let active = supported.project_profile();
    borrowed(&state.specifications).map(|_| Pinned {
        identity: active.specification.identity.as_ref().to_owned(),
        revision: active.specification.revision.as_ref().to_owned(),
        semantic_digest: active.semantic_digest.as_ref().to_owned(),
    })
}

fn project_identity<Id>(state: &SemanticState<'_, Id>) -> Admitted<ProjectIdentity> {
    borrowed(&state.identity).map(|identity| ProjectIdentity {
        project_id: identity.project_id().as_str().to_owned(),
        selection_scope: identity.selection_scope().as_str().to_owned(),
    })
}

fn locale_canonicalization<Id: AsRef<str>>(
    state: &SemanticState<'_, Id>,
) -> Admitted<LocaleCanonicalization> {
    borrowed(&state.canonicalization).map(|pins| LocaleCanonicalization {
        specification: Specification::of(&pins.specification),
        dataset: Dataset {
            identity: pins.dataset.identity.as_ref().to_owned(),
            semantic_digest: pins.dataset.semantic_digest.as_ref().to_owned(),
        },
    })
}

fn project_locales<Id>(state: &SemanticState<'_, Id>) -> Admitted<ProjectLocales> {
    borrowed(&state.locales.project).map(|project| ProjectLocales {
        requested_locales: locales(project.requested()),
        default_requested_locale: locale(project.requested_default()),
        default_source_locale: project.source_default().map(locale),
    })
}

fn locale_negotiation<Id: AsRef<str>>(
    state: &SemanticState<'_, Id>,
) -> Admitted<LocaleNegotiation> {
    borrowed(&state.negotiation).map(|negotiation| LocaleNegotiation {
        specification: Specification::of(negotiation.specification()),
        aliases: negotiation
            .aliases()
            .iter()
            .map(|(alias, target)| (locale(alias), locale(target)))
            .collect(),
    })
}

fn message_fallback<Id: AsRef<str>>(state: &SemanticState<'_, Id>) -> Admitted<MessageFallback> {
    borrowed(&state.fallback).map(|fallback| MessageFallback {
        specification: Specification::of(fallback.specification()),
        mappings: fallback
            .sequences()
            .iter()
            .map(|(key, sequence)| {
                let candidates = sequence
                    .iter()
                    .map(|candidate| match candidate {
                        Candidate::Literal(definition) => {
                            FallbackCandidate::Locale(locale(definition))
                        }
                        Candidate::IntentSource => FallbackCandidate::IntentSource {
                            kind: IntentSourceKind::IntentSourceLocale,
                        },
                    })
                    .collect();
                (locale(key), candidates)
            })
            .collect(),
    })
}

/// The coverage, with the vocabulary its table was checked against and a
/// decision row for every requested locale.
fn coverage<Id: AsRef<str>>(state: &SemanticState<'_, Id>) -> Admitted<Coverage> {
    borrowed(&state.coverage).map(|coverage| {
        let vocabulary = coverage.vocabulary();
        Coverage {
            specification: Specification::of(coverage.specification()),
            vocabulary: Pinned {
                identity: vocabulary.identity().as_str().to_owned(),
                revision: vocabulary.revision().as_str().to_owned(),
                semantic_digest: vocabulary.semantic_digest().as_str().to_owned(),
            },
            default_mode: coverage.default_mode(),
            decisions: decisions(coverage.requested(), coverage.table()),
        }
    })
}

/// The decision table by requested locale, then by surface class. Every
/// requested locale has a row, which holds no decision when the vocabulary has
/// no surface class.
fn decisions<'t>(
    requested: &LocaleSet,
    table: impl Iterator<Item = (&'t CanonicalLocale, &'t Token, CoverageMode)>,
) -> BTreeMap<String, BTreeMap<String, CoverageMode>> {
    let mut rows: BTreeMap<String, BTreeMap<String, CoverageMode>> = requested
        .as_slice()
        .iter()
        .map(|requested| (locale(requested), BTreeMap::new()))
        .collect();
    for (requested, surface, mode) in table {
        rows.entry(locale(requested))
            .or_default()
            .insert(surface.as_str().to_owned(), mode);
    }
    rows
}

/// Both policy groups: each slot's exact reference, and the confirmed
/// policy's bounds. A required slot never holds `null`, so Phase 9 resolved a
/// reference for each.
fn policies<Id>(state: &SemanticState<'_, Id>) -> Admitted<(RequiredPolicies, OptionalPolicies)> {
    both(
        borrowed(&state.references),
        borrowed(&state.policy.confirmed),
    )
    .map(|(references, confirmed)| {
        let reference = |slot: PolicySlot| {
            references
                .iter()
                .find(|(field, _, _)| **field == ReferenceField::Policy(slot))
                .map(|(_, tuple, _)| Reference::of(tuple))
        };
        let required =
            |slot: PolicySlot| reference(slot).expect("Phase 9 resolves every required policy");
        (
            RequiredPolicies {
                resource_limits: required(PolicySlot::ResourceLimits),
                trust: required(PolicySlot::Trust),
                source_admission: required(PolicySlot::SourceAdmission),
                approval: required(PolicySlot::Approval),
                selection: required(PolicySlot::Selection),
                project_profile_resolution: confirmed.limits().section(),
            },
            OptionalPolicies {
                provider_routing: reference(PolicySlot::ProviderRouting),
                glossary_set: reference(PolicySlot::GlossarySet),
            },
        )
    })
}

/// Every target, once Phase 11 resolved each: its Target Profile, requested
/// locales, the default it overrides when it does, and its effective default.
fn targets<Id>(state: &SemanticState<'_, Id>) -> Admitted<BTreeMap<String, Target>> {
    let (targets, declared) = both(
        holding(&state.targets, ResolvedTargets::complete),
        borrowed(&state.locales.targets),
    )?;
    Ok(targets
        .iter()
        .map(|(id, facts)| {
            let resolved = facts
                .resolved
                .as_ref()
                .expect("a complete map resolved every target");
            let default = declared
                .iter()
                .find(|locales| locales.id == *id)
                .and_then(|locales| locales.default.as_ref().ok())
                .expect("Phase 11 resolves a target only under the default Phase 7 admitted");
            let (tuple, _) = resolved.profile;
            (
                id.as_str().to_owned(),
                Target {
                    profile: Reference::of(tuple),
                    requested_locales: locales(&resolved.requested),
                    default_requested_locale: default.as_ref().map(locale),
                    effective_default_requested_locale: locale(&resolved.effective_default),
                },
            )
        })
        .collect())
}

/// Every group, once the groups partition the targets and each group's
/// hydration relations are checked: its members and its relations.
fn deployment_groups<Id>(
    state: &SemanticState<'_, Id>,
) -> Admitted<BTreeMap<String, DeploymentGroup>> {
    let (groups, hydration) = both(
        holding(&state.groups, |groups| groups.partition().clone()),
        holding(&state.hydration, ResolvedHydration::complete),
    )?;
    Ok(groups
        .iter()
        .map(|(id, members)| {
            let members = members
                .as_ref()
                .expect("a partition admitted every group's members");
            let relations = hydration
                .get(id)
                .and_then(|relations| relations.as_ref().ok())
                .expect("Phase 13 checked every group's relations");
            (
                id.as_str().to_owned(),
                DeploymentGroup {
                    members: members
                        .iter()
                        .map(|member| member.as_str().to_owned())
                        .collect(),
                    hydration_relations: relations
                        .iter()
                        .map(|(server, client)| HydrationRelation {
                            server: server.as_str().to_owned(),
                            client: client.as_str().to_owned(),
                        })
                        .collect(),
                },
            )
        })
        .collect())
}

fn delivery<Id: AsRef<str>>(state: &SemanticState<'_, Id>) -> Admitted<Delivery> {
    borrowed(&state.delivery).map(|delivery| Delivery {
        graph: Specification::of(delivery.graph()),
        placement: Specification::of(delivery.placement()),
        mode: delivery.mode(),
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::artifacts::content::sample;
    use crate::artifacts::envelope::{envelope_of, ArtifactKind, FormalMember};
    use crate::artifacts::vectors::vocabulary as vectors;
    use crate::locale::fixtures::{fixture_binding, fixture_pins, FixtureProvider};
    use crate::locale::{Provider, ProviderBinding, ProviderFailure, SemanticPins, Supported};
    use crate::resolver::dependency::SemanticGroup;
    use crate::resolver::invocation::tests::{example, resolved, resolver, Raw};
    use crate::resolver::invocation::Resolver;
    use crate::resolver::profile_resolution::tests::{
        policy_limits, reverse_objects, Inputs, ACTIVE,
    };
    use crate::resolver::specification::{
        ActiveProfileSpecification, SpecificationInput, VocabularyAssertion, FIXTURE_PROFILE_DIGEST,
    };

    type Id = &'static str;

    /// The committed profile view vectors, relative to the crate.
    const VECTOR_FILE: &str = "fixtures/profile-view/vectors.json";

    /// A semantic digest no fixture specification has.
    const OTHER_DIGEST: &str =
        "sha256:2222222222222222222222222222222222222222222222222222222222222222";

    /// Each reference a declaration writes, and its place in the view.
    const REFERENCES: [(&str, &str); 9] = [
        (
            "/policies/resourceLimits",
            "/requiredPolicies/resourceLimits",
        ),
        ("/policies/trust", "/requiredPolicies/trust"),
        (
            "/policies/sourceAdmission",
            "/requiredPolicies/sourceAdmission",
        ),
        ("/policies/approval", "/requiredPolicies/approval"),
        ("/policies/selection", "/requiredPolicies/selection"),
        (
            "/policies/providerRouting",
            "/optionalPolicies/providerRouting",
        ),
        ("/policies/glossarySet", "/optionalPolicies/glossarySet"),
        (
            "/targetProfiles/browser/profile",
            "/targets/browser/profile",
        ),
        ("/targetProfiles/server/profile", "/targets/server/profile"),
    ];

    /// An edit of the selected declaration.
    type Edit = fn(&mut Value);

    /// An edit of an exact tuple.
    type TupleEdit = fn(&mut ExactTuple);

    /// A named edit, and each member of the view it changes with the value
    /// the member then has.
    type Change = (&'static str, Edit, Vec<(&'static str, Value)>);

    fn checked(inputs: &Inputs) -> ProfileView {
        inputs.view().unwrap()
    }

    /// `Inputs::complete` with `edit` made to its declaration.
    fn edited(edit: Edit) -> Inputs {
        let mut inputs = Inputs::complete();
        edit(inputs.declaration());
        inputs
    }

    /// The view of `Inputs::complete`: what `declaration` states, under the
    /// specifications, vocabulary, and canonicalization data the resolver
    /// supports.
    fn complete_view(declaration: &Value) -> Value {
        let pins = fixture_pins();
        let policy = |slot: &str| declaration["policies"][slot].clone();
        let profile = |target: &str| declaration["targetProfiles"][target]["profile"].clone();
        json!({
            "profileSpecification": {
                "identity": "test-only-profile-specification",
                "revision": "0",
                "semanticDigest": FIXTURE_PROFILE_DIGEST
            },
            "projectIdentity": {
                "projectId": "storefront",
                "selectionScope": "storefront.production"
            },
            "localeCanonicalization": {
                "specification": {
                    "identity": pins.specification.identity,
                    "revision": pins.specification.revision
                },
                "dataset": {
                    "identity": pins.dataset.identity,
                    "semanticDigest": pins.dataset.semantic_digest
                }
            },
            "projectLocales": {
                "requestedLocales": ["en", "ja"],
                "defaultRequestedLocale": "en",
                "defaultSourceLocale": "en"
            },
            "localeNegotiation": {
                "specification": {"identity": "intlify-locale-negotiation", "revision": "0"},
                "aliases": {"de": "en"}
            },
            "messageFallback": {
                "specification": {"identity": "intlify-message-locale-fallback", "revision": "0"},
                "mappings": {"ja": ["en", {"kind": "intent-source-locale"}]}
            },
            "coverage": {
                "specification": {"identity": "intlify-localization-coverage", "revision": "0"},
                "vocabulary": {
                    "identity": vectors::IDENTITY,
                    "revision": vectors::REVISION,
                    "semanticDigest": vectors::SEMANTIC_DIGEST
                },
                "defaultMode": "direct-required",
                "decisions": {
                    "en": {
                        "checkout": "direct-required",
                        "internal-tool": "direct-required",
                        "nav": "direct-required"
                    },
                    "ja": {
                        "checkout": "fallback-allowed",
                        "internal-tool": "fallback-allowed",
                        "nav": "fallback-allowed"
                    }
                }
            },
            "requiredPolicies": {
                "resourceLimits": policy("resourceLimits"),
                "trust": policy("trust"),
                "sourceAdmission": policy("sourceAdmission"),
                "approval": policy("approval"),
                "selection": policy("selection"),
                "projectProfileResolution": policy_limits().section()
            },
            "optionalPolicies": {
                "providerRouting": policy("providerRouting"),
                "glossarySet": policy("glossarySet")
            },
            "targets": {
                "browser": {
                    "profile": profile("browser"),
                    "requestedLocales": ["en", "ja"],
                    "defaultRequestedLocale": "en",
                    "effectiveDefaultRequestedLocale": "en"
                },
                "server": {
                    "profile": profile("server"),
                    "requestedLocales": ["en", "ja"],
                    "defaultRequestedLocale": null,
                    "effectiveDefaultRequestedLocale": "en"
                }
            },
            "deploymentGroups": {
                "web": {
                    "members": ["browser", "server"],
                    "hydrationRelations": [{"server": "server", "client": "browser"}]
                }
            },
            "delivery": {
                "graph": {"identity": "intlify-delivery-graph", "revision": "0"},
                "placement": {"identity": "intlify-delivery-placement", "revision": "0"},
                "mode": "duplicate"
            }
        })
    }

    /// `Inputs::complete` without its second coverage rule, on which no
    /// cell's mode depends.
    fn redundant_rule() -> Inputs {
        edited(|declaration| {
            declaration["coverage"]["rules"]
                .as_array_mut()
                .unwrap()
                .remove(1);
        })
    }

    /// Other declarations of `Inputs::complete`'s profile, each differing in
    /// what the projection leaves out: the Profile ID and the selector, the
    /// order of sets, objects, and the collection, an omitted placement or
    /// default mode in place of the default, and a redundant rule.
    fn same_profile() -> [(&'static str, Inputs); 5] {
        let complete = Inputs::complete();
        [
            ("another-profile-id", complete.selecting("shop")),
            ("another-order", complete.reordered()),
            (
                "omitted-placement",
                edited(|declaration| {
                    declaration.as_object_mut().unwrap().remove("delivery");
                }),
            ),
            (
                "omitted-default-mode",
                edited(|declaration| {
                    declaration["coverage"]
                        .as_object_mut()
                        .unwrap()
                        .remove("defaultMode");
                }),
            ),
            ("redundant-rule", redundant_rule()),
        ]
    }

    /// Each change of a semantic field, named as its vector case is.
    fn changes() -> [Change; 13] {
        [
            (
                "project-id",
                |declaration| declaration["projectId"] = json!("outlet"),
                vec![("/projectIdentity/projectId", json!("outlet"))],
            ),
            (
                "selection-scope",
                |declaration| declaration["selectionScope"] = json!("storefront.staging"),
                vec![(
                    "/projectIdentity/selectionScope",
                    json!("storefront.staging"),
                )],
            ),
            (
                "requested-locales",
                |declaration| declaration["requestedLocales"] = json!(["fr", "ja", "en"]),
                vec![
                    (
                        "/projectLocales/requestedLocales",
                        json!(["en", "fr", "ja"]),
                    ),
                    ("/coverage/decisions/fr/nav", json!("direct-required")),
                ],
            ),
            (
                "default-requested-locale",
                |declaration| {
                    declaration["defaultRequestedLocale"] = json!("ja");
                    declaration["targetProfiles"]["browser"]["defaultRequestedLocale"] =
                        json!("ja");
                },
                vec![
                    ("/projectLocales/defaultRequestedLocale", json!("ja")),
                    ("/targets/server/defaultRequestedLocale", Value::Null),
                    (
                        "/targets/server/effectiveDefaultRequestedLocale",
                        json!("ja"),
                    ),
                ],
            ),
            (
                "default-source-locale",
                |declaration| declaration["defaultSourceLocale"] = json!("ja"),
                vec![("/projectLocales/defaultSourceLocale", json!("ja"))],
            ),
            (
                "no-default-source-locale",
                |declaration| {
                    declaration
                        .as_object_mut()
                        .unwrap()
                        .remove("defaultSourceLocale");
                },
                vec![("/projectLocales/defaultSourceLocale", Value::Null)],
            ),
            (
                "aliases",
                |declaration| declaration["localeNegotiation"]["aliases"]["de"] = json!("ja"),
                vec![("/localeNegotiation/aliases", json!({"de": "ja"}))],
            ),
            (
                "message-fallback",
                |declaration| {
                    declaration["messageFallback"]["ja"] =
                        json!([{"kind": "intent-source-locale"}, "en"]);
                },
                vec![(
                    "/messageFallback/mappings/ja",
                    json!([{"kind": "intent-source-locale"}, "en"]),
                )],
            ),
            (
                "default-mode",
                |declaration| declaration["coverage"]["defaultMode"] = json!("fallback-allowed"),
                vec![
                    ("/coverage/defaultMode", json!("fallback-allowed")),
                    ("/coverage/decisions/en/nav", json!("fallback-allowed")),
                ],
            ),
            (
                "rules",
                |declaration| {
                    declaration["coverage"]["rules"]
                        .as_array_mut()
                        .unwrap()
                        .remove(0);
                },
                vec![(
                    "/coverage/decisions/ja",
                    json!({
                        "checkout": "fallback-allowed",
                        "internal-tool": "direct-required",
                        "nav": "direct-required"
                    }),
                )],
            ),
            (
                "provider-routing",
                |declaration| declaration["policies"]["providerRouting"] = Value::Null,
                vec![("/optionalPolicies/providerRouting", Value::Null)],
            ),
            (
                "target-default",
                |declaration| {
                    declaration["targetProfiles"]["browser"]
                        .as_object_mut()
                        .unwrap()
                        .remove("defaultRequestedLocale");
                },
                vec![
                    ("/targets/browser/defaultRequestedLocale", Value::Null),
                    (
                        "/targets/browser/effectiveDefaultRequestedLocale",
                        json!("en"),
                    ),
                ],
            ),
            (
                "hydration-relations",
                |declaration| {
                    declaration["deploymentGroups"]["web"]["hydrationRelations"] = json!([]);
                },
                vec![("/deploymentGroups/web/hydrationRelations", json!([]))],
            ),
        ]
    }

    /// Every vector case, by its name: the complete profile, a second group,
    /// a vocabulary without surface classes, each semantic change, and each
    /// other declaration of the complete profile.
    fn cases() -> Vec<(&'static str, Inputs)> {
        let mut cases = vec![
            ("complete", Inputs::complete()),
            ("two-groups", Inputs::two_groups()),
            ("no-surface-classes", Inputs::without_surface_classes()),
        ];
        cases.extend(changes().map(|(id, edit, _)| (id, edited(edit))));
        cases.extend(same_profile());
        cases
    }

    /// The vectors: each distinct view once, under the name of the first case
    /// that projects it, with its profile semantic digest, and the view each
    /// case projects.
    fn document() -> Value {
        let mut views: Vec<(&'static str, ProfileView)> = Vec::new();
        let mut named = Vec::new();
        for (id, inputs) in cases() {
            let view = inputs
                .view()
                .unwrap_or_else(|causes| panic!("{id}: {causes:?}"));
            let name = if let Some(index) = views.iter().position(|(_, other)| *other == view) {
                views[index].0
            } else {
                views.push((id, view));
                id
            };
            named.push(json!({"id": id, "view": name}));
        }
        let views: Vec<Value> = views
            .iter()
            .map(|(id, view)| {
                json!({
                    "id": id,
                    "view": view,
                    "semanticDigest": view.semantic_digest().unwrap().as_str(),
                })
            })
            .collect();
        json!({
            "note": "Profile views of checked test profiles, projected by intlify_config, with \
                     their profile semantic digests. An independent implementation checks every \
                     view against 015's closed number-free shape: one member for each of the \
                     committed Profile Specification's semanticGroups, in its order; exact \
                     references of the kind 017 expects in each slot; the bounds its \
                     resourceBounds lists, as decimal strings; sets in ascending UTF-8 byte \
                     order; a decision row for every requested locale; each target's effective \
                     default; and the targets partitioned among the groups, each hydration \
                     relation inside its group. It recomputes each digest as 017's H under \
                     localization-project-profile-semantic-digest and checks that distinct views \
                     have distinct digests. Each case names the view its test inputs project, so \
                     the cases that share a view are declarations of one profile. None of these \
                     profiles is one a project declares.",
            "domain": DOMAIN,
            "views": views,
            "cases": named,
        })
    }

    fn committed() -> Value {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(VECTOR_FILE);
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    /// Every semantic group, in the inventory's order, as the declaration
    /// states it: canonical locales, references as written, and the
    /// confirmed policy's bounds as decimal strings.
    #[test]
    fn a_checked_profile_projects_every_semantic_group_in_inventory_order() {
        let mut inputs = Inputs::complete();
        let declaration = inputs.declaration().clone();
        let view = serde_json::to_value(checked(&inputs)).unwrap();
        assert_eq!(view, complete_view(&declaration));
        let members: Vec<&str> = view
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(members, SemanticGroup::ALL.map(SemanticGroup::id));
    }

    /// A target that overrides the default with the locale it then has, and
    /// a group without relations.
    #[test]
    fn a_second_group_projects_its_own_target_and_relations() {
        let mut inputs = Inputs::two_groups();
        let profile = inputs.declaration()["targetProfiles"]["worker"]["profile"].clone();
        let view = serde_json::to_value(checked(&inputs)).unwrap();
        assert_eq!(
            view["targets"]["worker"],
            json!({
                "profile": profile,
                "requestedLocales": ["ja"],
                "defaultRequestedLocale": "ja",
                "effectiveDefaultRequestedLocale": "ja"
            })
        );
        assert_eq!(
            view["deploymentGroups"]["jobs"],
            json!({"members": ["worker"], "hydrationRelations": []})
        );
    }

    /// 017's `H` over the view under the domain 017 registers. The view holds
    /// no number, so it has a digest.
    #[test]
    fn the_profile_semantic_digest_is_h_over_the_view() {
        let view = checked(&Inputs::complete());
        let expected = hash(
            Domain::literal("localization-project-profile-semantic-digest"),
            &serde_json::to_value(&view).unwrap(),
        )
        .unwrap();
        assert_eq!(
            view.semantic_digest().unwrap(),
            SemanticDigest::from_hash(expected)
        );
    }

    /// What the projection leaves out: each declaration `same_profile` lists
    /// has the complete profile's view and digest, and the redundant rule
    /// changes only the Coverage Decision Basis.
    #[test]
    fn declarations_of_one_profile_have_one_view_and_digest() {
        let complete = Inputs::complete();
        let view = checked(&complete);
        let digest = view.semantic_digest().unwrap();
        let basis =
            |inputs: &Inputs| inputs.resolve(|_, state| state.coverage.unwrap().basis().clone());
        assert_ne!(basis(&redundant_rule()), basis(&complete));
        for (name, inputs) in same_profile() {
            let other = checked(&inputs);
            assert_eq!(other, view, "{name}");
            assert_eq!(other.semantic_digest().unwrap(), digest, "{name}");
        }
    }

    /// Each change of a semantic field that `changes` lists: a profile of its
    /// own, whose view holds the change.
    #[test]
    fn each_semantic_change_changes_the_view_and_its_digest() {
        let view = checked(&Inputs::complete());
        let digest = view.semantic_digest().unwrap();
        for (name, edit, changed) in changes() {
            let other = edited(edit)
                .view()
                .unwrap_or_else(|causes| panic!("{name}: {causes:?}"));
            let value = serde_json::to_value(&other).unwrap();
            for (pointer, expected) in changed {
                assert_eq!(value.pointer(pointer), Some(&expected), "{name}: {pointer}");
            }
            assert_ne!(other, view, "{name}");
            assert_ne!(other.semantic_digest().unwrap(), digest, "{name}");
        }
        let two = checked(&Inputs::two_groups());
        assert_ne!(two, view);
        assert_ne!(two.semantic_digest().unwrap(), digest);
    }

    /// Every requested locale has a decision row, which holds no decision
    /// when the vocabulary has no surface class.
    #[test]
    fn every_requested_locale_has_a_decision_row() {
        let view = serde_json::to_value(checked(&Inputs::without_surface_classes())).unwrap();
        assert_eq!(view["coverage"]["decisions"], json!({"en": {}, "ja": {}}));
        assert_eq!(
            view["coverage"]["vocabulary"]["semanticDigest"],
            vectors::EMPTY_SEMANTIC_DIGEST
        );
    }

    /// `view` with each of `changed` set to its value.
    fn with(view: &Value, changed: &[(&str, Value)]) -> Value {
        let mut expected = view.clone();
        for (pointer, value) in changed {
            *expected.pointer_mut(pointer).unwrap() = value.clone();
        }
        expected
    }

    /// The members of an exact reference's view `before` that `after` holds
    /// otherwise.
    fn changed_members<'v>(before: &'v Value, after: &Value) -> Vec<&'v str> {
        before
            .as_object()
            .unwrap()
            .iter()
            .filter(|(name, value)| after.get(name.as_str()) != Some(*value))
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// The finite fixture's rows bound under other pins: the data of a
    /// canonicalization specification and dataset other than the fixture's.
    #[derive(Clone, PartialEq, Eq)]
    struct Repinned {
        rows: FixtureProvider,
        binding: ProviderBinding<Id>,
    }

    impl Repinned {
        fn under(pins: SemanticPins<Id>) -> Self {
            let mut binding = fixture_binding();
            binding.pins = pins;
            Self {
                rows: FixtureProvider::new(),
                binding,
            }
        }
    }

    impl Provider for Repinned {
        type Identity = Id;

        fn binding(&self) -> &ProviderBinding<Id> {
            &self.binding
        }

        fn canonicalize(&self, input: &str) -> Result<Arc<str>, ProviderFailure> {
            self.rows.canonicalize(input)
        }
    }

    /// What a resolver supports and the invocation asserts in each slot,
    /// which `Inputs` fixes at revision "0": the Active Project Profile
    /// Specification with its semantic digest, the canonicalization
    /// specification with its dataset, the other specifications in slot
    /// order, and the vocabulary's name.
    #[derive(Clone)]
    struct Slots {
        active: VersionedIdentity<Id>,
        digest: Id,
        pins: SemanticPins<Id>,
        negotiation: VersionedIdentity<Id>,
        fallback: VersionedIdentity<Id>,
        coverage: VersionedIdentity<Id>,
        graph: VersionedIdentity<Id>,
        placement: VersionedIdentity<Id>,
        vocabulary: VersionedIdentity<Id>,
    }

    impl Slots {
        fn revision_zero() -> Self {
            let zero = |identity| VersionedIdentity {
                identity,
                revision: "0",
            };
            Self {
                active: ACTIVE,
                digest: FIXTURE_PROFILE_DIGEST,
                pins: fixture_pins(),
                negotiation: zero("intlify-locale-negotiation"),
                fallback: zero("intlify-message-locale-fallback"),
                coverage: zero("intlify-localization-coverage"),
                graph: zero("intlify-delivery-graph"),
                placement: zero("intlify-delivery-placement"),
                vocabulary: VersionedIdentity {
                    identity: vectors::IDENTITY,
                    revision: vectors::REVISION,
                },
            }
        }

        /// The view of `Inputs::complete` on a resolver supporting these
        /// values, each slot asserting its own, beside the fixture
        /// vocabulary's members under this name and the fixture's rows bound
        /// under these pins.
        fn view(&self) -> ProfileView {
            let mut inputs = Inputs::complete();
            *inputs.specifications() = SpecificationInput {
                project_profile: Some(self.active.clone()),
                locale_canonicalization: Some(self.pins.specification.clone()),
                locale_negotiation: Some(self.negotiation.clone()),
                message_locale_fallback: Some(self.fallback.clone()),
                coverage: Some(self.coverage.clone()),
                intent_surface_class_vocabulary: Some(VocabularyAssertion {
                    identity: self.vocabulary.identity,
                    revision: self.vocabulary.revision,
                    semantic_digest: vectors::SEMANTIC_DIGEST,
                }),
                delivery_graph: Some(self.graph.clone()),
                delivery_placement: Some(self.placement.clone()),
            };
            let supported = SupportedSpecifications::new(
                ActiveProfileSpecification {
                    specification: self.active.clone(),
                    semantic_digest: self.digest,
                },
                Supported::new(vec![self.pins.clone()]).unwrap(),
                self.negotiation.clone(),
                self.fallback.clone(),
                self.coverage.clone(),
                self.graph.clone(),
                self.placement.clone(),
            );
            let vocabulary = vectors::formal_named(
                self.vocabulary.identity,
                self.vocabulary.revision,
                &vectors::MEMBERS,
            );
            let data = Repinned::under(self.pins.clone());
            inputs.view_under(&supported, &vocabulary, &data).unwrap()
        }

        /// The value these values project at `pointer` in the view.
        fn field(&mut self, pointer: &str) -> &mut Id {
            match pointer {
                "/profileSpecification/identity" => &mut self.active.identity,
                "/profileSpecification/revision" => &mut self.active.revision,
                "/profileSpecification/semanticDigest" => &mut self.digest,
                "/localeCanonicalization/specification/identity" => {
                    &mut self.pins.specification.identity
                }
                "/localeCanonicalization/specification/revision" => {
                    &mut self.pins.specification.revision
                }
                "/localeCanonicalization/dataset/identity" => &mut self.pins.dataset.identity,
                "/localeCanonicalization/dataset/semanticDigest" => {
                    &mut self.pins.dataset.semantic_digest
                }
                "/localeNegotiation/specification/identity" => &mut self.negotiation.identity,
                "/localeNegotiation/specification/revision" => &mut self.negotiation.revision,
                "/messageFallback/specification/identity" => &mut self.fallback.identity,
                "/messageFallback/specification/revision" => &mut self.fallback.revision,
                "/coverage/specification/identity" => &mut self.coverage.identity,
                "/coverage/specification/revision" => &mut self.coverage.revision,
                "/coverage/vocabulary/identity" => &mut self.vocabulary.identity,
                "/coverage/vocabulary/revision" => &mut self.vocabulary.revision,
                "/delivery/graph/identity" => &mut self.graph.identity,
                "/delivery/graph/revision" => &mut self.graph.revision,
                "/delivery/placement/identity" => &mut self.placement.identity,
                "/delivery/placement/revision" => &mut self.placement.revision,
                _ => unreachable!("{pointer} is no specification field"),
            }
        }
    }

    /// The view's fields that a specification slot, the Active Project
    /// Profile Specification's digest, or the canonicalization dataset fills.
    const SPECIFICATION_FIELDS: [&str; 19] = [
        "/profileSpecification/identity",
        "/profileSpecification/revision",
        "/profileSpecification/semanticDigest",
        "/localeCanonicalization/specification/identity",
        "/localeCanonicalization/specification/revision",
        "/localeCanonicalization/dataset/identity",
        "/localeCanonicalization/dataset/semanticDigest",
        "/localeNegotiation/specification/identity",
        "/localeNegotiation/specification/revision",
        "/messageFallback/specification/identity",
        "/messageFallback/specification/revision",
        "/coverage/specification/identity",
        "/coverage/specification/revision",
        "/coverage/vocabulary/identity",
        "/coverage/vocabulary/revision",
        "/delivery/graph/identity",
        "/delivery/graph/revision",
        "/delivery/placement/identity",
        "/delivery/placement/revision",
    ];

    /// D15:1189, 1195, 1197-1199, 1204, 2533, 2541, 3426, 3508: the
    /// specification each slot names, the Active Project Profile
    /// Specification's semantic digest, and the canonicalization dataset are
    /// each one field of the view. A resolver supporting another value, which
    /// the invocation asserts and the data or the vocabulary carries, projects
    /// the view with that field changed and nothing else, every locale in its
    /// canonical spelling, and another digest.
    #[test]
    fn each_specification_member_changes_only_its_own_field() {
        let revision_zero = Slots::revision_zero();
        let base = revision_zero.view();
        assert_eq!(base, checked(&Inputs::complete()));
        let view = serde_json::to_value(&base).unwrap();
        let digest = base.semantic_digest().unwrap();
        for pointer in SPECIFICATION_FIELDS {
            let value = if pointer.ends_with("/revision") {
                "1"
            } else if pointer.ends_with("/semanticDigest") {
                OTHER_DIGEST
            } else {
                "test-only-other-specification"
            };
            let mut slots = revision_zero.clone();
            *slots.field(pointer) = value;
            let other = slots.view();
            assert_eq!(
                serde_json::to_value(&other).unwrap(),
                with(&view, &[(pointer, json!(value))]),
                "{pointer}"
            );
            assert_ne!(other.semantic_digest().unwrap(), digest, "{pointer}");
        }
    }

    /// Another content of the artifact the reference at `pointer` names
    /// that revision "0" admits. A trust or selection policy has one admitted
    /// content each, and a Resource Limit Policy's content is the bounds
    /// `each_policy_bound_changes_its_own_field_and_the_policy_digest`
    /// changes.
    fn other_content(pointer: &str) -> Option<fn(&mut Value)> {
        let edit: fn(&mut Value) = match pointer {
            "/policies/sourceAdmission" => |content| content["sourceApproval"] = json!("required"),
            "/policies/approval" => |content| content["localizedApproval"] = json!("not-required"),
            "/policies/providerRouting" => {
                |content| content["configuration"]["identity"] = json!("storefront-other-routing")
            }
            "/policies/glossarySet" => |content| content["terms"] = json!("storefront-terms"),
            "/targetProfiles/browser/profile" | "/targetProfiles/server/profile" => {
                |content| content["limits"]["maxMessages"] = json!("999")
            }
            _ => return None,
        };
        Some(edit)
    }

    /// D15:1206, 1758, 2535, 2563, 3426, 3430, 3639: a reference reaches the
    /// view through its five exact members alone. The artifact a reference
    /// names, sealed again under another identity or revision or with
    /// another admitted content and then named by the reference, changes that
    /// one member of the reference in the view, and nothing else, and the
    /// profile semantic digest: no field of a body enters the view beside its
    /// reference. An absent Glossary Set is `null`, as an absent
    /// provider-routing policy is.
    #[test]
    fn each_reference_changes_the_view_only_through_its_exact_members() {
        let base = checked(&Inputs::complete());
        let view = serde_json::to_value(&base).unwrap();
        let digest = base.semantic_digest().unwrap();
        let mut cases = 0;
        for (declared, viewed) in REFERENCES {
            for member in ["identity", "revision", "semanticDigest"] {
                let content = other_content(declared);
                if member == "semanticDigest" && content.is_none() {
                    continue;
                }
                let mut inputs = Inputs::complete();
                inputs.reseal(declared, |body| match member {
                    "identity" => {
                        let renamed = format!("{}-renamed", body["identity"].as_str().unwrap());
                        body["identity"] = json!(renamed);
                    }
                    "revision" => body["revision"] = json!("2"),
                    _ => content.unwrap()(&mut body["content"]),
                });
                let reference = inputs.declaration().pointer(declared).unwrap().clone();
                let before = view.pointer(viewed).unwrap();
                assert_eq!(
                    changed_members(before, &reference),
                    [member],
                    "{declared} {member}"
                );
                let other = checked(&inputs);
                assert_eq!(
                    serde_json::to_value(&other).unwrap(),
                    with(&view, &[(viewed, reference)]),
                    "{declared} {member}"
                );
                assert_ne!(
                    other.semantic_digest().unwrap(),
                    digest,
                    "{declared} {member}"
                );
                cases += 1;
            }
        }
        assert_eq!(cases, 24);
        let absent = edited(|declaration| declaration["policies"]["glossarySet"] = Value::Null);
        let other = checked(&absent);
        assert_eq!(
            serde_json::to_value(&other).unwrap(),
            with(&view, &[("/optionalPolicies/glossarySet", Value::Null)])
        );
        assert_ne!(other.semantic_digest().unwrap(), digest);
    }

    /// D15:1239, 2563, 3430, 3639: a reference is its five exact members,
    /// each its own field. A tuple differing from the trust policy's in its
    /// kind, identity, revision, specification revision, or semantic digest
    /// projects another reference, and a view holding it is another profile
    /// with another digest. Revision "0" admits one kind in each slot,
    /// specification revision "0" alone, and one trust policy content, so
    /// those changes are made on the projection.
    #[test]
    fn a_reference_is_its_five_exact_members() {
        let view = checked(&Inputs::complete());
        let digest = view.semantic_digest().unwrap();
        let kind = ArtifactKind::TrustPolicy;
        let trust = envelope_of(kind, "storefront-trust", "1", sample(kind));
        let trust = FormalMember::new(trust).unwrap();
        let tuple = trust.envelope().unwrap().tuple().clone();
        assert_eq!(Reference::of(&tuple), view.required_policies.trust);
        let before = serde_json::to_value(&view.required_policies.trust).unwrap();
        let changes: [(&str, TupleEdit); 5] = [
            ("kind", |tuple| tuple.kind = ArtifactKind::ApprovalPolicy),
            ("identity", |tuple| {
                tuple.identity = Token::literal("storefront-other-trust");
            }),
            ("revision", |tuple| tuple.revision = Token::literal("2")),
            ("specificationRevision", |tuple| {
                tuple.specification_revision = Token::literal("1");
            }),
            ("semanticDigest", |tuple| {
                tuple.semantic_digest = SemanticDigest::from_hash([7; 32]);
            }),
        ];
        for (member, change) in changes {
            let mut changed = tuple.clone();
            change(&mut changed);
            let mut other = view.clone();
            other.required_policies.trust = Reference::of(&changed);
            let after = serde_json::to_value(&other.required_policies.trust).unwrap();
            assert_eq!(changed_members(&before, &after), [member]);
            assert_ne!(other, view, "{member}");
            assert_ne!(other.semantic_digest().unwrap(), digest, "{member}");
        }
    }

    /// D15:1200, 1740, 3426: each normalized `projectProfileResolution` bound
    /// is a field of the view. The selected policy with one bound lower
    /// changes that bound and the policy reference's semantic digest, which
    /// covers its content, and nothing else.
    #[test]
    fn each_policy_bound_changes_its_own_field_and_the_policy_digest() {
        let complete = Inputs::complete();
        let base = checked(&complete);
        let view = serde_json::to_value(&base).unwrap();
        let digest = base.semantic_digest().unwrap();
        let before = &view["requiredPolicies"]["resourceLimits"];
        for (id, bound) in policy_limits().bounds() {
            let lower = bound.get() - 1;
            let mut inputs = complete.bounding(&[], &[(id, lower)]);
            let reference = inputs.declaration()["policies"]["resourceLimits"].clone();
            assert_eq!(changed_members(before, &reference), ["semanticDigest"]);
            let pointer = format!(
                "/requiredPolicies/projectProfileResolution/{}/{}",
                id.group.path().replace('.', "/"),
                id.name
            );
            let other = checked(&inputs);
            assert_eq!(
                serde_json::to_value(&other).unwrap(),
                with(
                    &view,
                    &[
                        (pointer.as_str(), json!(lower.to_string())),
                        ("/requiredPolicies/resourceLimits", reference),
                    ]
                ),
                "{pointer}"
            );
            assert_ne!(other.semantic_digest().unwrap(), digest, "{pointer}");
        }
    }

    /// D15:1202, 1203, 2078, 3426: an existing target's requested-locale
    /// subset and an existing group's member set are fields of the view. The
    /// worker requesting `en` as well, or the server moved from `web`, where
    /// it hydrates nothing, into `jobs`, changes those fields alone.
    #[test]
    fn a_targets_subset_and_a_groups_members_change_only_their_own_fields() {
        let base = checked(&Inputs::two_groups());
        let view = serde_json::to_value(&base).unwrap();
        let mut wider = Inputs::two_groups();
        wider.declaration()["targetProfiles"]["worker"]["requestedLocales"] = json!(["ja", "en"]);
        let other = checked(&wider);
        assert_eq!(
            serde_json::to_value(&other).unwrap(),
            with(
                &view,
                &[("/targets/worker/requestedLocales", json!(["en", "ja"]))]
            )
        );
        assert_ne!(
            other.semantic_digest().unwrap(),
            base.semantic_digest().unwrap()
        );
        let apart = || {
            let mut inputs = Inputs::two_groups();
            inputs.declaration()["deploymentGroups"]["web"]["hydrationRelations"] = json!([]);
            inputs
        };
        let base = checked(&apart());
        let mut moved = apart();
        let groups = &mut moved.declaration()["deploymentGroups"];
        groups["web"]["members"] = json!(["browser"]);
        groups["jobs"]["members"] = json!(["worker", "server"]);
        let other = checked(&moved);
        assert_eq!(
            serde_json::to_value(&other).unwrap(),
            with(
                &serde_json::to_value(&base).unwrap(),
                &[
                    ("/deploymentGroups/web/members", json!(["browser"])),
                    (
                        "/deploymentGroups/jobs/members",
                        json!(["server", "worker"])
                    ),
                ]
            )
        );
        assert_ne!(
            other.semantic_digest().unwrap(),
            base.semantic_digest().unwrap()
        );
    }

    /// Every `en` in `value` spelled `EN`, which the fixture's data accepts
    /// as `en`.
    fn spell_en(value: &mut Value) {
        match value {
            Value::String(text) if *text == "en" => "EN".clone_into(text),
            Value::Array(items) => items.iter_mut().for_each(spell_en),
            Value::Object(members) => members.values_mut().for_each(spell_en),
            _ => {}
        }
    }

    /// D15:2541, 3431, 3505, 3622: an accepted spelling is not semantic, nor
    /// is the Finding that presents it. The declaration with every `en`
    /// spelled `EN`, its default source locale among them, resolves with a
    /// non-canonical notice for each spelling and has the complete profile's
    /// view and digest.
    #[test]
    fn accepted_spellings_keep_the_view_and_digest() {
        let mut spelled = Inputs::complete();
        spell_en(spelled.declaration());
        assert_eq!(spelled.declaration()["defaultSourceLocale"], "EN");
        let viewed = |inputs: &Inputs| {
            inputs.viewed(|evaluation, view| (evaluation.notices().count(), view.unwrap()))
        };
        let (none, view) = viewed(&Inputs::complete());
        let (notices, other) = viewed(&spelled);
        assert_eq!((none, notices), (0, 8));
        assert_eq!(other, view);
        assert_eq!(
            other.semantic_digest().unwrap(),
            view.semantic_digest().unwrap()
        );
    }

    /// What the committed resolver's checked outcome of `raw`'s file, or of
    /// `text` in its place, shows: its view and its bindings.
    fn checked_file(
        resolver: &Resolver<'_>,
        raw: &Raw,
        text: Option<&str>,
    ) -> (ProfileView, Vec<(ExactTuple, FormalMember)>) {
        let inputs = match text {
            Some(text) => raw.file_of(resolver, Arc::from(text.as_bytes()), true),
            None => raw.file(resolver, true),
        };
        resolved(resolver, &inputs).checked.expect("checked")
    }

    /// D15:2533, 3431: profile equality is not source-byte equality. The
    /// configuration's file spaced with CRLF line ends, or with characters
    /// escaped, has the same view, digest, and bindings.
    #[test]
    fn another_encoding_of_the_configuration_keeps_the_profile() {
        let resolver = resolver();
        let raw = example(|inputs| inputs);
        let compact = serde_json::to_string(&raw.config).unwrap();
        let (view, bindings) = checked_file(&resolver, &raw, Some(&compact));
        let spaced = serde_json::to_string_pretty(&raw.config)
            .unwrap()
            .replace('\n', "\r\n");
        let escaped = compact
            .replace("\"en\"", &format!("\"{}u0065n\"", '\\'))
            .replace('/', "\\/");
        for text in [spaced, escaped] {
            assert_ne!(text, compact);
            let (other, bound) = checked_file(&resolver, &raw, Some(&text));
            assert_eq!(other, view, "{text}");
            assert_eq!(
                other.semantic_digest().unwrap(),
                view.semantic_digest().unwrap()
            );
            assert_eq!(bound, bindings, "{text}");
        }
    }

    /// D15:1754, 1758, 2565, 3347: another conforming encoding of every
    /// artifact, its members in another order, spaced, and with a character
    /// escaped, is the same body under the same exact reference. The checked
    /// profile has the same view and digest, and the same bindings: each
    /// tuple bound to a body equal by value and by its canonical bytes. The
    /// sidecar binds the body as this invocation submitted it, in its own
    /// member order, so a consumer's handoff or cache of it may change while
    /// the profile does not.
    #[test]
    fn another_encoding_of_each_artifact_keeps_the_profile_and_its_bindings() {
        let resolver = resolver();
        let raw = example(|inputs| inputs);
        let (view, bindings) = checked_file(&resolver, &raw, None);
        let mut encoded = example(|inputs| inputs);
        let escaped = format!("\"{}u0073torefront", '\\');
        for member in &mut encoded.members {
            let mut value: Value = serde_json::from_slice(member).unwrap();
            reverse_objects(&mut value);
            let text = serde_json::to_string_pretty(&value)
                .unwrap()
                .replace("\"storefront", &escaped);
            assert!(text.contains(&escaped));
            *member = text.into_bytes();
        }
        let (other, bound) = checked_file(&resolver, &encoded, None);
        assert_eq!(other, view);
        assert_eq!(
            other.semantic_digest().unwrap(),
            view.semantic_digest().unwrap()
        );
        assert_eq!(bound, bindings);
        assert_eq!(bound.len(), REFERENCES.len());
        for ((_, member), (_, earlier)) in bound.iter().zip(&bindings) {
            assert_eq!(member.canonical_body(), earlier.canonical_body());
            assert_ne!(member.value().to_string(), earlier.value().to_string());
        }
    }

    /// D15:322, 426, 3413: `$schema` is editor metadata that selects
    /// nothing. The configuration with another `$schema` value, or with
    /// none, has the same view, digest, and bindings. The two values have the
    /// same Shared projection, and without the member it lacks only the
    /// status of the member's own type check.
    #[test]
    fn schema_metadata_keeps_the_profile() {
        let resolver = resolver();
        let shown = |schema: Option<&str>| {
            let mut raw = example(|inputs| inputs);
            let root = raw.config.as_object_mut().unwrap();
            assert!(root.contains_key("$schema"));
            match schema {
                Some(value) => root.insert("$schema".to_owned(), json!(value)),
                None => root.remove("$schema"),
            };
            resolved(&resolver, &raw.file(&resolver, true))
        };
        let present = shown(Some("https://invalid.example/editor-only-schema"));
        let (view, _) = present.checked.clone().expect("checked");
        let digest = view.semantic_digest().unwrap();
        let another = shown(Some("https://invalid.example/another-editor-schema.json"));
        assert_eq!(another.shared, present.shared);
        let absent = shown(None);
        let (with, _) = present.shared.as_ref().unwrap();
        let (without, _) = absent.shared.as_ref().unwrap();
        let only_with: Vec<&Value> = with
            .iter()
            .filter(|record| !without.contains(record))
            .map(|(_, _, view)| view)
            .collect();
        assert_eq!(with.len(), without.len() + 1);
        let [status] = only_with[..] else {
            panic!("{only_with:?}");
        };
        assert_eq!(status["unit"]["check"], "structural.root.field-type");
        assert_eq!(status["state"], "evaluated");
        for other in [another, absent] {
            assert_eq!(other.checked, present.checked);
            let (other, _) = other.checked.unwrap();
            assert_eq!(other.semantic_digest().unwrap(), digest);
        }
    }

    /// The committed vectors are the views the resolver projects for every
    /// case, with their digests. Equal decoded values are equal canonical
    /// bytes, so the repository formatter owns the file's layout.
    #[test]
    fn the_committed_vectors_are_the_resolvers() {
        assert_eq!(committed(), document());
    }

    #[test]
    #[ignore = "prints the vectors to review before committing them"]
    fn print_vectors_for_review() {
        println!("{}", serde_json::to_string_pretty(&document()).unwrap());
    }
}
