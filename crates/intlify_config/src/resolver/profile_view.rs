// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Phase 14, `profile-projection`: a checked profile's canonical semantic
//! projection, presented as its profile view (D15:1187-1237, 2136).
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
//! Decision Basis and Evidence, and redundant authoring rules (D15:1206).

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
/// semantic results it needs and does not have. It needs every semantic
/// group, so it has a view exactly when every group's owner was evaluated, and
/// otherwise waits on what those owners wait on.
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
    both(both(identities, policies), deployment).map(
        |(
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

    use serde_json::json;

    use super::*;
    use crate::artifacts::vectors::vocabulary as vectors;
    use crate::locale::fixtures::fixture_pins;
    use crate::resolver::dependency::SemanticGroup;
    use crate::resolver::profile_resolution::tests::{policy_limits, Inputs};
    use crate::resolver::specification::FIXTURE_PROFILE_DIGEST;

    /// The committed profile view vectors, relative to the crate.
    const VECTOR_FILE: &str = "fixtures/profile-view/vectors.json";

    /// An edit of the selected declaration.
    type Edit = fn(&mut Value);

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
