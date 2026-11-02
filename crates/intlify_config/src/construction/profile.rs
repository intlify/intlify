// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Active Project Profile Specification and its schema.
//!
//! Design 015's Profile Specification is number-free data that pins the rule
//! identities the design defines: the value each specification slot supports,
//! the locale canonicalization dataset, the phases in rank order, the semantic
//! groups of the profile view, the 52 `projectProfileResolution` bounds, the
//! four other construction authorities, and the domains of the subject and
//! evidence tokens, of the two source revisions, and of the Programmatic Entry
//! Snapshot, with the Snapshot's marker reasons. Revision `"0"` writes the body
//! from the values this crate implements, and its schema from the same types.
//!
//! The schema constrains only the form of each value: closed objects, tokens,
//! digests, names, and non-empty arrays without repeats. No position admits a
//! number, so a schema-valid body is in 017's value domain. What each value
//! must be is fixed by the body's semantic digest, 017's `H` over the complete
//! body, which neither the body's formatting nor its member order enters.
//! Construction compares that digest, not the file digest, with the one the
//! resolver was compiled from.
//!
//! Construction checks the Profile Specification Schema it is given, rows 2 to
//! 7 of the construction-failure precedence, and then the Profile
//! Specification, rows 8 to 14, against the schema it admitted.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use intlify_shared_json::encoding::{hash, Domain, EncodingFailure};
use intlify_shared_json::token::{IntegrityDigest, ID_PATTERN};

use crate::input_limits::Bound;
use crate::locale::{Supported, VersionedIdentity};
use crate::resolver::evaluation::Phase;
use crate::resolver::limits::ResolutionLimits;

use super::admission::{ComponentAdmission, ConstructionAdmission};
use super::component::{self, Declared, Refusal, Rows, Submitted};
use super::envelope::DIGEST_PATTERN;
use super::failure::{Check, Component};
use super::root::ConstructionRoot;
use super::validator::schema::Schema;

/// The Profile Specification's identity.
pub(crate) const IDENTITY: &str = "intlify-project-profile";

/// The revision of the Profile Specification and of its schema.
pub(crate) const REVISION: &str = "0";

/// The Profile Specification Schema's identity.
pub(crate) const SCHEMA_IDENTITY: &str = "intlify-project-profile-specification-schema";

/// The Profile Specification Schema's path below the conformance suite's
/// root.
pub(crate) const SCHEMA_PATH: &str = "authorities/project-profile-specification-v0.schema.json";

/// The Profile Specification's path below the conformance suite's root.
pub(crate) const PATH: &str = "authorities/project-profile-specification-v0.json";

/// The Profile Specification Schema's file digest, which the resolver is
/// compiled from.
pub(crate) const SCHEMA_DIGEST: &str =
    "sha256:c6ccee21a1735594ea1bd204d16839a554140a705e0f5c637bcccf3ca7385efd";

/// The Profile Specification's semantic digest, which the resolver is compiled
/// from.
pub(crate) const SEMANTIC_DIGEST: &str =
    "sha256:bd067d3318adce9ddc4f5a21b1ed2949309644883e5232781be02ee03c0d2e0c";

/// The domain 017 registers for the Profile Specification's semantic digest.
pub(crate) const SEMANTIC_DOMAIN: &str = "project-profile-specification-semantic-digest";

const LOCALE_CANONICALIZATION: &str = "intlify-locale-canonicalization";
const LOCALE_NEGOTIATION: &str = "intlify-locale-negotiation";
const MESSAGE_LOCALE_FALLBACK: &str = "intlify-message-locale-fallback";
const COVERAGE: &str = "intlify-localization-coverage";
const DELIVERY_GRAPH: &str = "intlify-delivery-graph";
const DELIVERY_PLACEMENT: &str = "intlify-delivery-placement";

/// The profile view's members, one per semantic group, in the order of 015's
/// semantic inventory.
pub(crate) const SEMANTIC_GROUPS: [&str; 12] = [
    "profileSpecification",
    "projectIdentity",
    "localeCanonicalization",
    "projectLocales",
    "localeNegotiation",
    "messageFallback",
    "coverage",
    "requiredPolicies",
    "optionalPolicies",
    "targets",
    "deploymentGroups",
    "delivery",
];

/// The domain separator of a Redacted subject token.
pub(crate) const SUBJECT_TOKEN_DOMAIN: &str = "intlify.project-profile.subject";

/// The domain separator of a Redacted evidence token.
pub(crate) const EVIDENCE_TOKEN_DOMAIN: &str = "intlify.project-profile.evidence";

/// The domain separator of a Source-content revision.
pub(crate) const FILE_SOURCE_REVISION_DOMAIN: &str =
    "intlify.project-profile.file-source-revision.v0";

/// The domain separator of a Call-Site Source revision.
pub(crate) const CALL_SITE_SOURCE_REVISION_DOMAIN: &str =
    "intlify.project-profile.call-site-source-revision.v0";

/// The domain separator of a Programmatic Entry Snapshot's content digest.
pub(crate) const SNAPSHOT_DOMAIN: &str = "intlify.programmatic-entry-snapshot.v0";

/// The reasons a Snapshot's rejected-node marker carries: the ordinary data
/// violations, then a bootstrap inspection overrun.
pub(crate) const MARKER_REASONS: [&str; 10] = [
    "unsupported-type",
    "non-finite-number",
    "non-portable-number",
    "invalid-unicode",
    "non-string-key",
    "dynamic-container",
    "dynamic-property",
    "sparse-array",
    "cycle",
    "bootstrap-limit-exceeded",
];

/// A member name of the profile view.
const MEMBER_NAME_PATTERN: &str = "^[a-z][A-Za-z0-9]*$";

/// A bound's path inside `projectProfileResolution`: its group's path and its
/// name, joined by dots.
const BOUND_PATH_PATTERN: &str = r"^[a-z][A-Za-z0-9]*(?:\.[a-z][A-Za-z0-9]*)+$";

/// A specification or an authority, by its identity and revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Pin {
    #[schemars(pattern(ID_PATTERN))]
    pub(crate) identity: String,
    #[schemars(pattern(ID_PATTERN))]
    pub(crate) revision: String,
}

/// The value of a slot with no fixed value: the invocation asserts an
/// artifact, which is admitted against the assertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Asserted {
    Asserted,
}

/// What one specification slot supports: one specification, or `"asserted"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub(crate) enum SlotValue {
    Pinned(Pin),
    Asserted(Asserted),
}

/// The eight specification slots, in the canonical order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct SpecificationSlots {
    pub(crate) project_profile: SlotValue,
    pub(crate) locale_canonicalization: SlotValue,
    pub(crate) locale_negotiation: SlotValue,
    pub(crate) message_locale_fallback: SlotValue,
    pub(crate) coverage: SlotValue,
    pub(crate) intent_surface_class_vocabulary: SlotValue,
    pub(crate) delivery_graph: SlotValue,
    pub(crate) delivery_placement: SlotValue,
}

/// The locale canonicalization dataset the canonicalization specification
/// admits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Dataset {
    #[schemars(pattern(ID_PATTERN))]
    pub(crate) identity: String,
    #[schemars(pattern(DIGEST_PATTERN))]
    pub(crate) semantic_digest: String,
}

/// The four other construction authorities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Authorities {
    pub(crate) profile_specification_schema: Pin,
    pub(crate) configuration_schema_authority_set: Pin,
    pub(crate) resolver_capability_specification: Pin,
    pub(crate) registry_package: Pin,
}

/// The domain separators of the Redacted subject and evidence tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TokenDomains {
    #[schemars(pattern(ID_PATTERN))]
    pub(crate) subject: String,
    #[schemars(pattern(ID_PATTERN))]
    pub(crate) evidence: String,
}

/// The domain separators of the Source-content and Call-Site Source
/// revisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RevisionDomains {
    #[schemars(pattern(ID_PATTERN))]
    pub(crate) file_source: String,
    #[schemars(pattern(ID_PATTERN))]
    pub(crate) call_site_source: String,
}

/// The Programmatic Entry Snapshot's digest domain separator and its marker
/// reasons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Snapshot {
    #[schemars(pattern(ID_PATTERN))]
    pub(crate) domain: String,
    #[schemars(
        length(min = 1),
        extend("uniqueItems" = true),
        inner(pattern(ID_PATTERN))
    )]
    pub(crate) marker_reasons: Vec<String>,
}

/// The Profile Specification's body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProjectProfileSpecification {
    pub(crate) specification_slots: SpecificationSlots,
    pub(crate) locale_canonicalization_dataset: Dataset,
    #[schemars(
        length(min = 1),
        extend("uniqueItems" = true),
        inner(pattern(ID_PATTERN))
    )]
    pub(crate) phases: Vec<String>,
    #[schemars(
        length(min = 1),
        extend("uniqueItems" = true),
        inner(pattern(MEMBER_NAME_PATTERN))
    )]
    pub(crate) semantic_groups: Vec<String>,
    #[schemars(
        length(min = 1),
        extend("uniqueItems" = true),
        inner(pattern(BOUND_PATH_PATTERN))
    )]
    pub(crate) resource_bounds: Vec<String>,
    pub(crate) authorities: Authorities,
    pub(crate) token_domains: TokenDomains,
    pub(crate) revision_domains: RevisionDomains,
    pub(crate) snapshot: Snapshot,
}

fn pin(identity: &str) -> Pin {
    Pin {
        identity: identity.to_owned(),
        revision: REVISION.to_owned(),
    }
}

fn pinned(identity: &str) -> SlotValue {
    SlotValue::Pinned(pin(identity))
}

fn admitted(component: &ComponentAdmission) -> Pin {
    Pin {
        identity: component.identity.clone(),
        revision: component
            .revisions
            .first()
            .cloned()
            .expect("a component admits a revision"),
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|&value| value.to_owned()).collect()
}

impl ProjectProfileSpecification {
    /// The revision-`"0"` body.
    ///
    /// # Panics
    ///
    /// Panics when this crate's own inventories disagree with one another,
    /// which the tests rule out.
    pub(crate) fn revision_zero() -> Self {
        let canonicalization = Supported::<&'static str>::revision_zero();
        let pins = canonicalization
            .expected(&VersionedIdentity {
                identity: LOCALE_CANONICALIZATION,
                revision: REVISION,
            })
            .expect("revision \"0\" admits its canonicalization specification");
        let one = Bound::new(1).expect("one is a bound");
        let bounds = ResolutionLimits::from_bounds(|_| Some(one))
            .expect("every bound is supplied")
            .bounds();
        let components = ConstructionAdmission::revision_zero().components;
        Self {
            specification_slots: SpecificationSlots {
                project_profile: pinned(IDENTITY),
                locale_canonicalization: pinned(LOCALE_CANONICALIZATION),
                locale_negotiation: pinned(LOCALE_NEGOTIATION),
                message_locale_fallback: pinned(MESSAGE_LOCALE_FALLBACK),
                coverage: pinned(COVERAGE),
                intent_surface_class_vocabulary: SlotValue::Asserted(Asserted::Asserted),
                delivery_graph: pinned(DELIVERY_GRAPH),
                delivery_placement: pinned(DELIVERY_PLACEMENT),
            },
            locale_canonicalization_dataset: Dataset {
                identity: pins.dataset.identity.to_owned(),
                semantic_digest: pins.dataset.semantic_digest.to_owned(),
            },
            phases: strings(&Phase::ALL.map(Phase::id)),
            semantic_groups: strings(&SEMANTIC_GROUPS),
            resource_bounds: bounds
                .iter()
                .map(|(id, _)| format!("{}.{}", id.group.path(), id.name))
                .collect(),
            authorities: Authorities {
                profile_specification_schema: admitted(&components.profile_specification_schema),
                configuration_schema_authority_set: admitted(
                    &components.configuration_schema_authority_set,
                ),
                resolver_capability_specification: admitted(
                    &components.resolver_capability_specification,
                ),
                registry_package: admitted(&components.registry_package),
            },
            token_domains: TokenDomains {
                subject: SUBJECT_TOKEN_DOMAIN.to_owned(),
                evidence: EVIDENCE_TOKEN_DOMAIN.to_owned(),
            },
            revision_domains: RevisionDomains {
                file_source: FILE_SOURCE_REVISION_DOMAIN.to_owned(),
                call_site_source: CALL_SITE_SOURCE_REVISION_DOMAIN.to_owned(),
            },
            snapshot: Snapshot {
                domain: SNAPSHOT_DOMAIN.to_owned(),
                marker_reasons: strings(&MARKER_REASONS),
            },
        }
    }
}

/// A Profile Specification body's semantic digest: 017's `H` over the
/// complete body, under the domain 017 registers for it.
///
/// # Errors
///
/// Returns 017's encoding failure, such as for a body with a number, which
/// the schema admits nowhere.
pub(crate) fn semantic_digest(body: &Value) -> Result<IntegrityDigest, EncodingFailure> {
    hash(Domain::literal(SEMANTIC_DOMAIN), body).map(IntegrityDigest::from_hash)
}

/// The Profile Specification Schema's document, written from the body's
/// types.
///
/// # Errors
///
/// Returns the serialization failure of the generated schema, which the types
/// rule out.
pub(crate) fn schema() -> Result<Value, serde_json::Error> {
    crate::schema::draft7_schema::<ProjectProfileSpecification>()
}

const SCHEMA_ROWS: Rows = Rows::of(Component::ProfileSpecificationSchema);
const SCHEMA_META_SCHEMA_ROW: Refusal =
    Refusal::at(Component::ProfileSpecificationSchema, Check::MetaSchema);
const ROWS: Rows = Rows::of(Component::ProfileSpecification);
const BODY_SCHEMA_ROW: Refusal = Refusal::at(Component::ProfileSpecification, Check::BodySchema);
const SEMANTIC_DIGEST_ROW: Refusal =
    Refusal::at(Component::ProfileSpecification, Check::SemanticDigest);

/// A Profile Specification Schema construction admitted.
#[derive(Debug)]
pub(crate) struct AdmittedSchema {
    /// What its tuple declares.
    pub(crate) declared: Declared,
    /// The schema, read by the validator.
    pub(crate) schema: Schema,
}

/// Check the Profile Specification Schema construction is given: rows 2 to 7
/// of the construction-failure precedence.
///
/// The envelope row adds that the schema declares Draft 7, and the last row
/// that it is valid against the Draft 7 meta-schema and read by the validator.
pub(crate) fn admit_schema(
    root: &ConstructionRoot,
    submitted: Option<&Submitted>,
) -> Result<AdmittedSchema, Refusal> {
    let admission = &root.admission().components.profile_specification_schema;
    let checked = component::check(root, SCHEMA_ROWS, admission, submitted, |document| {
        component::declares_draft_7(document).then_some(())
    })?;
    let schema = component::draft_7_schema(root, checked.value).ok_or(SCHEMA_META_SCHEMA_ROW)?;
    Ok(AdmittedSchema {
        declared: checked.declared,
        schema,
    })
}

/// A Profile Specification construction admitted.
#[derive(Debug)]
pub(crate) struct AdmittedProfileSpecification {
    /// What its tuple declares.
    pub(crate) declared: Declared,
    /// The body.
    pub(crate) body: Value,
    /// The body's semantic digest, the one its tuple declares.
    pub(crate) semantic_digest: IntegrityDigest,
}

/// Check the Profile Specification construction is given: rows 8 to 14 of the
/// construction-failure precedence, against the Profile Specification Schema
/// construction admitted.
///
/// The body must be valid against that schema, and have the semantic digest
/// its tuple declares. Another schema may admit a number, but a body with one
/// is outside 017's value domain and has no semantic digest, so it has none
/// its tuple could declare.
pub(crate) fn admit(
    root: &ConstructionRoot,
    schema: &AdmittedSchema,
    submitted: Option<&Submitted>,
) -> Result<AdmittedProfileSpecification, Refusal> {
    let admission = &root.admission().components.profile_specification;
    let checked = component::check(root, ROWS, admission, submitted, |_| Some(()))?;
    if !schema.schema.validate(checked.value).valid {
        return Err(BODY_SCHEMA_ROW);
    }
    let semantic_digest = semantic_digest(checked.value)
        .ok()
        .filter(|digest| checked.declared.semantic_digest.as_deref() == Some(digest.as_str()))
        .ok_or(SEMANTIC_DIGEST_ROW)?;
    Ok(AdmittedProfileSpecification {
        declared: checked.declared,
        body: checked.value.clone(),
        semantic_digest,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::construction::admission::DRAFT_7;
    use crate::construction::component::testing::{row, submitted};
    use crate::construction::component::Artifact;
    use crate::construction::meta_schema::DRAFT_7_META_SCHEMA;
    use crate::resolver::capability::CapabilityBound;
    use crate::resolver::specification::Slot;

    fn body() -> Value {
        serde_json::to_value(ProjectProfileSpecification::revision_zero()).unwrap()
    }

    /// The schema as this crate's validator and the oracle read it.
    struct Judges {
        schema: Schema,
        oracle: jsonschema::Validator,
    }

    impl Judges {
        fn new() -> Self {
            let document = schema().unwrap();
            Self {
                schema: Schema::compile(&document).unwrap(),
                oracle: jsonschema::draft7::options()
                    .should_validate_formats(false)
                    .build(&document)
                    .unwrap(),
            }
        }

        /// Whether `body` is valid, on which the two must agree.
        fn valid(&self, body: &Value) -> bool {
            let valid = self.schema.validate(body).valid;
            assert_eq!(valid, self.oracle.is_valid(body), "{body}");
            valid
        }
    }

    /// The JSON Pointer of every node of `value`, the root first.
    fn pointers(value: &Value) -> Vec<String> {
        let mut pointers = Vec::new();
        let mut pending = vec![(String::new(), value)];
        while let Some((pointer, value)) = pending.pop() {
            match value {
                Value::Object(members) => {
                    for (name, member) in members {
                        let name = name.replace('~', "~0").replace('/', "~1");
                        pending.push((format!("{pointer}/{name}"), member));
                    }
                }
                Value::Array(items) => {
                    for (index, item) in items.iter().enumerate() {
                        pending.push((format!("{pointer}/{index}"), item));
                    }
                }
                _ => {}
            }
            pointers.push(pointer);
        }
        pointers
    }

    /// `value` with every object's members in the reverse order.
    fn reversed(value: &Value) -> Value {
        match value {
            Value::Object(members) => Value::Object(
                members
                    .iter()
                    .rev()
                    .map(|(name, member)| (name.clone(), reversed(member)))
                    .collect(),
            ),
            Value::Array(items) => Value::Array(items.iter().map(reversed).collect()),
            _ => value.clone(),
        }
    }

    #[test]
    fn the_slots_are_015s_eight_with_the_value_each_supports() {
        let body = body();
        let slots = body["specificationSlots"].as_object().unwrap();
        assert_eq!(
            slots.keys().map(String::as_str).collect::<Vec<_>>(),
            Slot::ALL.map(Slot::name)
        );
        let pinned = |identity: &str| json!({ "identity": identity, "revision": "0" });
        assert_eq!(
            body["specificationSlots"],
            json!({
                "project-profile": pinned("intlify-project-profile"),
                "locale-canonicalization": pinned("intlify-locale-canonicalization"),
                "locale-negotiation": pinned("intlify-locale-negotiation"),
                "message-locale-fallback": pinned("intlify-message-locale-fallback"),
                "coverage": pinned("intlify-localization-coverage"),
                "intent-surface-class-vocabulary": "asserted",
                "delivery-graph": pinned("intlify-delivery-graph"),
                "delivery-placement": pinned("intlify-delivery-placement"),
            })
        );
        // The canonicalization specification admits the body's dataset.
        let canonicalization = Supported::<&'static str>::revision_zero();
        let admitted = canonicalization
            .expected(&VersionedIdentity {
                identity: "intlify-locale-canonicalization",
                revision: "0",
            })
            .unwrap();
        assert_eq!(
            body["localeCanonicalizationDataset"],
            json!({
                "identity": admitted.dataset.identity,
                "semanticDigest": admitted.dataset.semantic_digest,
            })
        );
        assert_eq!(
            body["localeCanonicalizationDataset"]["identity"],
            "intlify-locale-canonicalization-cldr-48.2.0"
        );
    }

    #[test]
    fn the_body_lists_015s_inventories_in_their_order() {
        let body = body();
        assert_eq!(
            body.as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            [
                "specificationSlots",
                "localeCanonicalizationDataset",
                "phases",
                "semanticGroups",
                "resourceBounds",
                "authorities",
                "tokenDomains",
                "revisionDomains",
                "snapshot",
            ]
        );
        assert_eq!(body["phases"], json!(Phase::ALL.map(Phase::id)));
        assert_eq!(body["phases"][0], "resolver-bootstrap-admission");
        assert_eq!(body["phases"][15], "reporting-finalization");
        assert_eq!(
            body["semanticGroups"],
            json!([
                "profileSpecification",
                "projectIdentity",
                "localeCanonicalization",
                "projectLocales",
                "localeNegotiation",
                "messageFallback",
                "coverage",
                "requiredPolicies",
                "optionalPolicies",
                "targets",
                "deploymentGroups",
                "delivery",
            ])
        );
        // The bounds are the Capability Specification's mirrored paths without
        // their namespace, in its order.
        let bounds = body["resourceBounds"].as_array().unwrap();
        assert_eq!(bounds.len(), 52);
        assert_eq!(bounds[0], "configurationInput.maxNodes");
        assert_eq!(bounds[51], "diagnostics.shared.maxTotalCanonicalBytes");
        let mirrored = CapabilityBound::inventory().map(|(bound, _)| bound.to_string());
        for (bound, mirror) in bounds.iter().zip(&mirrored[..52]) {
            assert_eq!(
                format!("policyComparable.{}", bound.as_str().unwrap()),
                *mirror
            );
        }
        let pinned = |identity: &str| json!({ "identity": identity, "revision": "0" });
        assert_eq!(
            body["authorities"],
            json!({
                "profileSpecificationSchema":
                    pinned("intlify-project-profile-specification-schema"),
                "configurationSchemaAuthoritySet":
                    pinned("intlify-configuration-schema-authority-set"),
                "resolverCapabilitySpecification":
                    pinned("intlify-project-profile-resolver-capability"),
                "registryPackage": pinned("intlify-project-profile-resolver-registry-package"),
            })
        );
        assert_eq!(
            body["tokenDomains"],
            json!({
                "subject": "intlify.project-profile.subject",
                "evidence": "intlify.project-profile.evidence",
            })
        );
        assert_eq!(
            body["revisionDomains"],
            json!({
                "fileSource": "intlify.project-profile.file-source-revision.v0",
                "callSiteSource": "intlify.project-profile.call-site-source-revision.v0",
            })
        );
        assert_eq!(
            body["snapshot"],
            json!({
                "domain": "intlify.programmatic-entry-snapshot.v0",
                "markerReasons": [
                    "unsupported-type",
                    "non-finite-number",
                    "non-portable-number",
                    "invalid-unicode",
                    "non-string-key",
                    "dynamic-container",
                    "dynamic-property",
                    "sparse-array",
                    "cycle",
                    "bootstrap-limit-exceeded",
                ],
            })
        );
    }

    #[test]
    fn the_body_names_the_identities_construction_admits() {
        let admission = ConstructionAdmission::revision_zero();
        assert_eq!(
            admission.components.profile_specification.identity,
            IDENTITY
        );
        assert_eq!(
            admission.components.profile_specification_schema.identity,
            SCHEMA_IDENTITY
        );
        for component in [
            &admission.components.profile_specification,
            &admission.components.profile_specification_schema,
        ] {
            assert_eq!(component.revisions, [REVISION]);
        }
        let body = ProjectProfileSpecification::revision_zero();
        assert_eq!(
            body.specification_slots.project_profile,
            SlotValue::Pinned(pin(IDENTITY))
        );
        assert_eq!(
            body.authorities.profile_specification_schema,
            pin(SCHEMA_IDENTITY)
        );
    }

    #[test]
    fn the_body_reads_back_as_its_type() {
        let body = body();
        let read: ProjectProfileSpecification = serde_json::from_value(body.clone()).unwrap();
        assert_eq!(read, ProjectProfileSpecification::revision_zero());
        assert_eq!(serde_json::to_value(read).unwrap(), body);
    }

    #[test]
    fn the_semantic_digest_covers_the_values_and_not_their_form() {
        let body = body();
        let digest = semantic_digest(&body).unwrap();
        assert_eq!(
            digest,
            IntegrityDigest::from_hash(
                hash(
                    Domain::literal("project-profile-specification-semantic-digest"),
                    &body
                )
                .unwrap()
            )
        );
        // Neither the member order nor the formatting enters it.
        let reversed = reversed(&body);
        assert_ne!(
            serde_json::to_string(&reversed).unwrap(),
            serde_json::to_string(&body).unwrap()
        );
        assert_eq!(semantic_digest(&reversed).unwrap(), digest);
        // Every value does.
        for pointer in pointers(&body) {
            if let Some(Value::String(text)) = body.pointer(&pointer) {
                let mut changed = body.clone();
                *changed.pointer_mut(&pointer).unwrap() = json!(format!("{text}x"));
                assert_ne!(semantic_digest(&changed).unwrap(), digest, "{pointer}");
            }
        }
        // A number has no digest, which is why the schema admits none.
        let mut numbered = body;
        numbered["phases"][0] = json!(0);
        assert_eq!(semantic_digest(&numbered), Err(EncodingFailure::Number));
    }

    #[test]
    fn the_schema_is_a_draft_7_schema_the_validator_reads() {
        let document = schema().unwrap();
        assert_eq!(document["$schema"], DRAFT_7);
        let meta_schema: Value = serde_json::from_str(DRAFT_7_META_SCHEMA).unwrap();
        assert!(
            Schema::compile(&meta_schema)
                .unwrap()
                .validate(&document)
                .valid
        );
        assert!(jsonschema::draft7::meta::validator().is_valid(&document));
        assert!(Judges::new().valid(&body()));
    }

    #[test]
    fn the_schema_closes_every_object_and_requires_every_member() {
        let judges = Judges::new();
        let body = body();
        let mut objects = 0;
        for pointer in pointers(&body) {
            let Some(Value::Object(members)) = body.pointer(&pointer) else {
                continue;
            };
            objects += 1;
            let mut extra = body.clone();
            extra
                .pointer_mut(&pointer)
                .and_then(Value::as_object_mut)
                .unwrap()
                .insert("unknown".to_owned(), json!("unknown"));
            assert!(!judges.valid(&extra), "{pointer}");
            for name in members.keys() {
                let mut missing = body.clone();
                missing
                    .pointer_mut(&pointer)
                    .and_then(Value::as_object_mut)
                    .unwrap()
                    .remove(name);
                assert!(!judges.valid(&missing), "{pointer}/{name}");
            }
        }
        // The root, the slots, seven pinned slots, the dataset, the authorities
        // and their four pins, the two domain objects, and the Snapshot.
        assert_eq!(objects, 18);
    }

    #[test]
    fn the_schema_checks_the_form_of_every_value() {
        let judges = Judges::new();
        let body = body();
        for (pointer, replacement) in [
            ("/specificationSlots/coverage", json!("pinned")),
            ("/specificationSlots/coverage", json!(null)),
            (
                "/specificationSlots/coverage",
                json!(["intlify-localization-coverage", "0"]),
            ),
            (
                "/specificationSlots/coverage/identity",
                json!("Intlify-Localization-Coverage"),
            ),
            ("/specificationSlots/coverage/revision", json!("")),
            (
                "/specificationSlots/intent-surface-class-vocabulary",
                json!("Asserted"),
            ),
            (
                "/specificationSlots/intent-surface-class-vocabulary",
                json!(true),
            ),
            ("/localeCanonicalizationDataset/identity", json!("-cldr")),
            (
                "/localeCanonicalizationDataset/semanticDigest",
                json!("sha256:6182"),
            ),
            (
                "/localeCanonicalizationDataset/semanticDigest",
                json!("SHA256:6182421172339b8c416890e20b3121ab40e412a896df6316d8e991c756505e16"),
            ),
            ("/phases", json!([])),
            ("/phases", json!("resolver-bootstrap-admission")),
            ("/phases/1", json!("resolver-bootstrap-admission")),
            ("/phases/1", json!("Entry-Admission")),
            ("/semanticGroups", json!([])),
            ("/semanticGroups/0", json!("profile-specification")),
            ("/semanticGroups/0", json!("ProfileSpecification")),
            ("/semanticGroups/1", json!("profileSpecification")),
            ("/resourceBounds", json!([])),
            ("/resourceBounds/0", json!("maxNodes")),
            ("/resourceBounds/0", json!("configurationInput.max-nodes")),
            ("/resourceBounds/0", json!("configurationInput..maxNodes")),
            ("/resourceBounds/1", json!("configurationInput.maxNodes")),
            (
                "/authorities/registryPackage",
                json!("intlify-project-profile-resolver-registry-package"),
            ),
            (
                "/tokenDomains/subject",
                json!("Intlify.Project-Profile.Subject"),
            ),
            ("/tokenDomains/evidence", json!("intlify..evidence-")),
            ("/revisionDomains/fileSource", json!("intlify.file source")),
            ("/revisionDomains/callSiteSource", json!("")),
            ("/snapshot/domain", json!(".v0")),
            ("/snapshot/markerReasons", json!([])),
            ("/snapshot/markerReasons/1", json!("unsupported-type")),
            ("/snapshot/markerReasons/1", json!("Non-Finite-Number")),
        ] {
            let mut changed = body.clone();
            *changed.pointer_mut(pointer).unwrap() = replacement;
            assert!(!judges.valid(&changed), "{pointer}: {changed}");
        }
    }

    #[test]
    fn no_position_of_the_body_admits_a_number() {
        let judges = Judges::new();
        let body = body();
        let pointers = pointers(&body);
        for pointer in &pointers {
            for number in [json!(0), json!(1), json!(-2.5)] {
                let mut changed = body.clone();
                *changed.pointer_mut(pointer).unwrap() = number;
                assert!(!judges.valid(&changed), "{pointer}");
            }
            if let Some(Value::Array(_)) = body.pointer(pointer) {
                let mut longer = body.clone();
                longer
                    .pointer_mut(pointer)
                    .and_then(Value::as_array_mut)
                    .unwrap()
                    .push(json!(0));
                assert!(!judges.valid(&longer), "{pointer}");
            }
        }
        // Every node: 18 objects, 4 arrays, and 120 strings.
        assert_eq!(pointers.len(), 142);
    }

    const COMMITTED_SCHEMA: &[u8] = include_bytes!(
        "../../conformance/authorities/project-profile-specification-v0.schema.json"
    );
    const COMMITTED_BODY: &[u8] =
        include_bytes!("../../conformance/authorities/project-profile-specification-v0.json");
    const DRAFT_6: &str = "http://json-schema.org/draft-06/schema#";

    fn root() -> &'static ConstructionRoot {
        ConstructionRoot::built_in().unwrap()
    }

    fn bytes(value: &Value) -> Vec<u8> {
        serde_json::to_vec_pretty(value).unwrap()
    }

    /// The bytes of `value` after `change`.
    fn changed(value: &Value, change: impl FnOnce(&mut Value)) -> Vec<u8> {
        let mut value = value.clone();
        change(&mut value);
        bytes(&value)
    }

    /// A Profile Specification Schema that declares the identity and revision
    /// construction admits, and the file digest of `bytes`.
    fn schema_submitted(bytes: &[u8]) -> Submitted {
        submitted(SCHEMA_IDENTITY, REVISION, bytes)
    }

    /// A Profile Specification that declares `identity`, `revision`, the file
    /// digest of `bytes`, and the semantic digest of their value, or a digest
    /// of nothing when they have none.
    fn body_submitted_as(identity: &str, revision: &str, bytes: &[u8]) -> Submitted {
        let mut submitted = submitted(identity, revision, bytes);
        let semantic = component::strict_json(bytes)
            .and_then(|value| semantic_digest(&value).ok())
            .map_or_else(
                || format!("sha256:{}", "0".repeat(64)),
                |digest| digest.as_str().to_owned(),
            );
        submitted.tuple["semanticDigest"] = json!(semantic);
        submitted
    }

    fn body_submitted(bytes: &[u8]) -> Submitted {
        body_submitted_as(IDENTITY, REVISION, bytes)
    }

    fn committed_schema() -> AdmittedSchema {
        admit_schema(root(), Some(&schema_submitted(COMMITTED_SCHEMA))).unwrap()
    }

    #[test]
    fn construction_admits_the_committed_schema_and_profile_specification() {
        let schema = committed_schema();
        assert_eq!(
            schema.declared,
            Declared {
                identity: SCHEMA_IDENTITY.to_owned(),
                revision: REVISION.to_owned(),
                digest: SCHEMA_DIGEST.to_owned(),
                semantic_digest: None,
            }
        );
        // The schema read is the one this crate writes.
        assert!(schema.schema.validate(&body()).valid);
        assert!(!schema.schema.validate(&json!({})).valid);
        let admitted = admit(root(), &schema, Some(&body_submitted(COMMITTED_BODY))).unwrap();
        assert_eq!(admitted.body, body());
        assert_eq!(admitted.semantic_digest.as_str(), SEMANTIC_DIGEST);
        assert_eq!(admitted.declared.identity, IDENTITY);
        assert_eq!(
            admitted.declared.semantic_digest.as_deref(),
            Some(SEMANTIC_DIGEST)
        );
    }

    #[test]
    fn each_schema_check_refuses_with_its_row() {
        let committed: Value = serde_json::from_slice(COMMITTED_SCHEMA).unwrap();
        assert_eq!(row(admit_schema(root(), None)), Some(2));
        let mut semantic = schema_submitted(COMMITTED_SCHEMA);
        semantic.tuple["semanticDigest"] = json!(SEMANTIC_DIGEST);
        let mut reformatted = schema_submitted(COMMITTED_SCHEMA);
        reformatted.artifact = Artifact::read(serde_json::to_vec(&committed).unwrap());
        let cases = [
            // A component tuple declares no semantic digest.
            (semantic, 3),
            (schema_submitted(b"{\"$schema\": "), 3),
            (schema_submitted(b"true"), 3),
            (
                schema_submitted(&changed(&committed, |document| {
                    document.as_object_mut().unwrap().remove("$schema");
                })),
                3,
            ),
            (
                schema_submitted(&changed(&committed, |document| {
                    document["$schema"] = json!(DRAFT_6);
                })),
                3,
            ),
            (submitted(IDENTITY, REVISION, COMMITTED_SCHEMA), 4),
            (submitted(SCHEMA_IDENTITY, "1", COMMITTED_SCHEMA), 5),
            // The same schema in other bytes than the declared ones.
            (reformatted, 6),
            // The meta-schema refuses a title that is not a string; the
            // validator, a nested `$schema` and a reference to another document.
            (
                schema_submitted(&changed(&committed, |document| {
                    document["title"] = json!(1);
                })),
                7,
            ),
            (
                schema_submitted(&changed(&committed, |document| {
                    document["definitions"]["Pin"]["$schema"] = json!(DRAFT_7);
                })),
                7,
            ),
            (
                schema_submitted(&changed(&committed, |document| {
                    document["properties"]["phases"] = json!({ "$ref": "other.json" });
                })),
                7,
            ),
        ];
        for (submitted, expected) in cases {
            assert_eq!(
                row(admit_schema(root(), Some(&submitted))),
                Some(expected),
                "{}",
                submitted.tuple
            );
        }
    }

    #[test]
    fn the_schema_s_rows_keep_their_order() {
        let committed: Value = serde_json::from_slice(COMMITTED_SCHEMA).unwrap();
        let undeclared = changed(&committed, |document| {
            document.as_object_mut().unwrap().remove("$schema");
        });
        let untitled = changed(&committed, |document| document["title"] = json!(1));
        let first = |submitted: Submitted| row(admit_schema(root(), Some(&submitted)));
        assert_eq!(first(submitted(IDENTITY, REVISION, &undeclared)), Some(3));
        // The meta-schema's row comes after every other.
        assert_eq!(first(submitted(IDENTITY, REVISION, &untitled)), Some(4));
        assert_eq!(first(submitted(SCHEMA_IDENTITY, "1", &untitled)), Some(5));
        let mut digest = schema_submitted(&untitled);
        digest.tuple["digest"] = json!(SCHEMA_DIGEST);
        assert_eq!(first(digest), Some(6));
        assert_eq!(first(schema_submitted(&untitled)), Some(7));
    }

    #[test]
    fn each_body_check_refuses_with_its_row() {
        let schema = committed_schema();
        let body = body();
        assert_eq!(row(admit(root(), &schema, None)), Some(8));
        let mut reformatted = body_submitted(COMMITTED_BODY);
        reformatted.artifact = Artifact::read(serde_json::to_vec(&body).unwrap());
        let mut semantic = body_submitted(COMMITTED_BODY);
        semantic.tuple["semanticDigest"] = json!(format!("sha256:{}", "0".repeat(64)));
        let cases = [
            // The tuple must declare a semantic digest.
            (submitted(IDENTITY, REVISION, COMMITTED_BODY), 9),
            (body_submitted(b"{\"phases\": []"), 9),
            (
                body_submitted_as(SCHEMA_IDENTITY, REVISION, COMMITTED_BODY),
                10,
            ),
            (body_submitted_as(IDENTITY, "1", COMMITTED_BODY), 11),
            (reformatted, 12),
            (
                body_submitted(&changed(&body, |body| body["phases"] = json!([]))),
                13,
            ),
            // The schema admits no number.
            (
                body_submitted(&changed(&body, |body| body["phases"][0] = json!(1))),
                13,
            ),
            (semantic, 14),
        ];
        for (submitted, expected) in cases {
            assert_eq!(
                row(admit(root(), &schema, Some(&submitted))),
                Some(expected),
                "{}",
                submitted.tuple
            );
        }
    }

    #[test]
    fn the_body_s_rows_keep_their_order() {
        let schema = committed_schema();
        let first = |submitted: Submitted| row(admit(root(), &schema, Some(&submitted)));
        assert_eq!(
            first(body_submitted_as(SCHEMA_IDENTITY, "1", b"{")),
            Some(9)
        );
        let empty = changed(&body(), |body| body["phases"] = json!([]));
        let mut digest = body_submitted(&empty);
        digest.tuple["digest"] = json!(component::file_digest(COMMITTED_BODY).as_str());
        assert_eq!(first(digest), Some(12));
        let mut semantic = body_submitted(&empty);
        semantic.tuple["semanticDigest"] = json!(SEMANTIC_DIGEST);
        assert_eq!(first(semantic), Some(13));
    }

    #[test]
    fn a_body_with_a_number_has_no_semantic_digest_to_declare() {
        // A schema that admits every body.
        let permissive = bytes(&json!({ "$schema": DRAFT_7 }));
        let schema = admit_schema(root(), Some(&schema_submitted(&permissive))).unwrap();
        let numbered = changed(&body(), |body| body["phases"][0] = json!(1));
        let mut submitted = body_submitted(&numbered);
        for declared in [
            format!("sha256:{}", "0".repeat(64)),
            SEMANTIC_DIGEST.to_owned(),
        ] {
            submitted.tuple["semanticDigest"] = json!(declared);
            assert_eq!(row(admit(root(), &schema, Some(&submitted))), Some(14));
        }
        // The same schema admits the body without the number.
        let admitted = admit(root(), &schema, Some(&body_submitted(COMMITTED_BODY))).unwrap();
        assert_eq!(admitted.semantic_digest.as_str(), SEMANTIC_DIGEST);
    }

    #[test]
    fn these_rows_compare_the_declared_tuple_and_not_the_compiled_digests() {
        let schema = committed_schema();
        // A body this crate does not implement passes, declared as itself:
        // comparing with the compiled digest is construction's last row.
        let mut other = body();
        other["phases"].as_array_mut().unwrap().reverse();
        let admitted = admit(root(), &schema, Some(&body_submitted(&bytes(&other)))).unwrap();
        assert_ne!(admitted.semantic_digest.as_str(), SEMANTIC_DIGEST);
        assert_eq!(admitted.body, other);
        // The body in other bytes, its members in another order, is the same
        // Profile Specification.
        let reordered = serde_json::to_vec(&reversed(&body())).unwrap();
        let admitted = admit(root(), &schema, Some(&body_submitted(&reordered))).unwrap();
        assert_eq!(admitted.semantic_digest.as_str(), SEMANTIC_DIGEST);
        assert_ne!(
            admitted.declared.digest,
            component::file_digest(COMMITTED_BODY).as_str()
        );
    }
}
