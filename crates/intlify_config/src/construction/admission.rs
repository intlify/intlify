// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The construction-admission body: the closed inventory every construction
//! is admitted under.
//!
//! Design 015 lists what the body holds: the construction-root bounds with
//! their values and counting units, the five components and the identity and
//! revisions each admits, the mapped configuration schemas, the six Registry
//! Package members, the reference to each component's envelope schema, the
//! construction-failure precedence, and the cross-authority rules in their
//! order. The checked-in body is the sole source of the bound values; the
//! design does not repeat them.
//!
//! The body is generated from these values, and its schema is generated from
//! the same types, so the two cannot describe different inventories.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::input_limits::Bound;

use super::failure::{PrecedenceRow, PRECEDENCE};
use super::rules::CrossAuthorityRule;

/// One construction-root bound, in the canonical order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) enum ConstructionBoundId {
    #[serde(rename = "input.maxTotalArtifactBytes")]
    InputMaxTotalArtifactBytes,
    #[serde(rename = "input.maxSingleArtifactBytes")]
    InputMaxSingleArtifactBytes,
    #[serde(rename = "input.maxDepth")]
    InputMaxDepth,
    #[serde(rename = "input.maxNodes")]
    InputMaxNodes,
    #[serde(rename = "input.maxCollectionEntries")]
    InputMaxCollectionEntries,
    #[serde(rename = "input.maxTotalStringBytes")]
    InputMaxTotalStringBytes,
    #[serde(rename = "input.maxSingleStringBytes")]
    InputMaxSingleStringBytes,
    #[serde(rename = "schema.maxConfigurationSchemas")]
    SchemaMaxConfigurationSchemas,
    #[serde(rename = "schema.maxSingleArtifactBytes")]
    SchemaMaxSingleArtifactBytes,
    #[serde(rename = "schema.maxTotalArtifactBytes")]
    SchemaMaxTotalArtifactBytes,
    #[serde(rename = "schema.maxNodes")]
    SchemaMaxNodes,
    #[serde(rename = "schema.maxReferences")]
    SchemaMaxReferences,
    #[serde(rename = "schema.maxValidationWorkUnits")]
    SchemaMaxValidationWorkUnits,
    #[serde(rename = "registry.maxRows")]
    RegistryMaxRows,
    #[serde(rename = "registry.maxSingleMemberArtifactBytes")]
    RegistryMaxSingleMemberArtifactBytes,
    #[serde(rename = "registry.maxTotalArtifactBytes")]
    RegistryMaxTotalArtifactBytes,
    #[serde(rename = "registry.maxValidationWorkUnits")]
    RegistryMaxValidationWorkUnits,
    #[serde(rename = "crossAuthority.maxChecks")]
    CrossAuthorityMaxChecks,
}

/// What one unit of a construction bound counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ConstructionUnit {
    ArtifactByte,
    JsonLevel,
    JsonNode,
    JsonCollectionEntry,
    StringByte,
    ConfigurationSchema,
    SchemaNode,
    SchemaReference,
    ValidationPair,
    RegistryRow,
    CrossAuthorityCheck,
}

const fn value(count: u64) -> Bound {
    match Bound::new(count) {
        Some(bound) => bound,
        None => panic!("a construction bound is a bound"),
    }
}

const MEBIBYTE: u64 = 1024 * 1024;

impl ConstructionBoundId {
    /// Every construction bound, in the canonical order.
    pub(crate) const ALL: [Self; 18] = [
        Self::InputMaxTotalArtifactBytes,
        Self::InputMaxSingleArtifactBytes,
        Self::InputMaxDepth,
        Self::InputMaxNodes,
        Self::InputMaxCollectionEntries,
        Self::InputMaxTotalStringBytes,
        Self::InputMaxSingleStringBytes,
        Self::SchemaMaxConfigurationSchemas,
        Self::SchemaMaxSingleArtifactBytes,
        Self::SchemaMaxTotalArtifactBytes,
        Self::SchemaMaxNodes,
        Self::SchemaMaxReferences,
        Self::SchemaMaxValidationWorkUnits,
        Self::RegistryMaxRows,
        Self::RegistryMaxSingleMemberArtifactBytes,
        Self::RegistryMaxTotalArtifactBytes,
        Self::RegistryMaxValidationWorkUnits,
        Self::CrossAuthorityMaxChecks,
    ];

    /// The unit design 015's table gives this bound.
    pub(crate) const fn unit(self) -> ConstructionUnit {
        use ConstructionUnit as U;
        match self {
            Self::InputMaxTotalArtifactBytes
            | Self::InputMaxSingleArtifactBytes
            | Self::SchemaMaxSingleArtifactBytes
            | Self::SchemaMaxTotalArtifactBytes
            | Self::RegistryMaxSingleMemberArtifactBytes
            | Self::RegistryMaxTotalArtifactBytes => U::ArtifactByte,
            Self::InputMaxDepth => U::JsonLevel,
            Self::InputMaxNodes => U::JsonNode,
            Self::InputMaxCollectionEntries => U::JsonCollectionEntry,
            Self::InputMaxTotalStringBytes | Self::InputMaxSingleStringBytes => U::StringByte,
            Self::SchemaMaxConfigurationSchemas => U::ConfigurationSchema,
            Self::SchemaMaxNodes => U::SchemaNode,
            Self::SchemaMaxReferences => U::SchemaReference,
            Self::SchemaMaxValidationWorkUnits | Self::RegistryMaxValidationWorkUnits => {
                U::ValidationPair
            }
            Self::RegistryMaxRows => U::RegistryRow,
            Self::CrossAuthorityMaxChecks => U::CrossAuthorityCheck,
        }
    }

    /// The revision-`"0"` value of this bound.
    ///
    /// These are this implementation's values. They leave room for every
    /// authority revision `"0"` constructs from, and each is finite, so an
    /// oversized or deeply nested input is refused before anything else
    /// reads it.
    pub(crate) const fn value(self) -> Bound {
        match self {
            Self::InputMaxTotalArtifactBytes
            | Self::InputMaxTotalStringBytes
            | Self::RegistryMaxTotalArtifactBytes => value(16 * MEBIBYTE),
            Self::InputMaxSingleArtifactBytes | Self::RegistryMaxSingleMemberArtifactBytes => {
                value(4 * MEBIBYTE)
            }
            Self::InputMaxDepth | Self::SchemaMaxConfigurationSchemas => value(64),
            Self::InputMaxNodes | Self::InputMaxCollectionEntries => value(1 << 20),
            Self::InputMaxSingleStringBytes
            | Self::SchemaMaxReferences
            | Self::RegistryMaxRows
            | Self::CrossAuthorityMaxChecks => value(1 << 16),
            Self::SchemaMaxSingleArtifactBytes => value(MEBIBYTE),
            Self::SchemaMaxTotalArtifactBytes => value(8 * MEBIBYTE),
            Self::SchemaMaxNodes => value(1 << 18),
            Self::SchemaMaxValidationWorkUnits => value(1 << 28),
            Self::RegistryMaxValidationWorkUnits => value(1 << 30),
        }
    }
}

/// One bound's record: its ID, its value as a decimal string, and its unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BoundRecord {
    pub(crate) id: ConstructionBoundId,
    #[schemars(regex(pattern = r"^[1-9][0-9]{0,19}$"))]
    pub(crate) value: String,
    pub(crate) unit: ConstructionUnit,
}

/// An envelope schema in the authority-envelope schema package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum EnvelopeSchema {
    // `{ identity, revision, digest }`.
    ComponentTuple,
    // `{ identity, revision, digest, semanticDigest }`.
    ProfileSpecificationTuple,
    ConfigurationSchemaAuthoritySet,
    RegistryPackage,
}

/// The identity and revisions one construction slot admits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Admitted {
    pub(crate) identity: String,
    pub(crate) revisions: Vec<String>,
}

/// What one component admits, and the envelope schemas its form is checked
/// against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ComponentAdmission {
    pub(crate) identity: String,
    pub(crate) revisions: Vec<String>,
    pub(crate) envelope_schemas: Vec<EnvelopeSchema>,
}

/// The five construction components, in their fixed order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Components {
    pub(crate) profile_specification_schema: ComponentAdmission,
    pub(crate) profile_specification: ComponentAdmission,
    pub(crate) configuration_schema_authority_set: ComponentAdmission,
    pub(crate) resolver_capability_specification: ComponentAdmission,
    pub(crate) registry_package: ComponentAdmission,
}

/// What a mapped configuration schema admits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ConfigurationSchemaAdmission {
    pub(crate) identity: String,
    pub(crate) revisions: Vec<String>,
    pub(crate) dialects: Vec<String>,
}

/// The six Registry Package members, in their fixed order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) struct RegistryMembers {
    pub(crate) safe_origin_kind_registry_schema: Admitted,
    pub(crate) check_registry_schema: Admitted,
    pub(crate) finding_registry_schema: Admitted,
    pub(crate) safe_origin_kind_registry: Admitted,
    pub(crate) check_registry: Admitted,
    pub(crate) finding_registry: Admitted,
}

/// The construction-admission body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ConstructionAdmission {
    #[schemars(length(equal = 18))]
    pub(crate) bounds: Vec<BoundRecord>,
    pub(crate) components: Components,
    pub(crate) configuration_schemas: ConfigurationSchemaAdmission,
    pub(crate) registry_members: RegistryMembers,
    #[schemars(length(equal = 43))]
    pub(crate) failure_precedence: Vec<PrecedenceRow>,
    #[schemars(length(equal = 4))]
    pub(crate) cross_authority_rules: Vec<CrossAuthorityRule>,
}

/// The one dialect revision `"0"` admits.
pub(crate) const DRAFT_7: &str = "http://json-schema.org/draft-07/schema#";

const REVISION: &str = "0";

fn revision_zero() -> Vec<String> {
    vec![REVISION.to_owned()]
}

fn component(identity: &str, envelope_schemas: &[EnvelopeSchema]) -> ComponentAdmission {
    ComponentAdmission {
        identity: identity.to_owned(),
        revisions: revision_zero(),
        envelope_schemas: envelope_schemas.to_vec(),
    }
}

fn member(role: &str) -> Admitted {
    Admitted {
        identity: format!("intlify-project-profile-{role}"),
        revisions: revision_zero(),
    }
}

impl ConstructionAdmission {
    /// The revision-`"0"` body.
    pub(crate) fn revision_zero() -> Self {
        use EnvelopeSchema as E;
        Self {
            bounds: ConstructionBoundId::ALL
                .iter()
                .map(|&id| BoundRecord {
                    id,
                    value: id.value().get().to_string(),
                    unit: id.unit(),
                })
                .collect(),
            components: Components {
                profile_specification_schema: component(
                    "intlify-project-profile-specification-schema",
                    &[E::ComponentTuple],
                ),
                profile_specification: component(
                    "intlify-project-profile",
                    &[E::ProfileSpecificationTuple],
                ),
                configuration_schema_authority_set: component(
                    "intlify-configuration-schema-authority-set",
                    &[E::ComponentTuple, E::ConfigurationSchemaAuthoritySet],
                ),
                resolver_capability_specification: component(
                    "intlify-project-profile-resolver-capability",
                    &[E::ComponentTuple],
                ),
                registry_package: component(
                    "intlify-project-profile-resolver-registry-package",
                    &[E::ComponentTuple, E::RegistryPackage],
                ),
            },
            configuration_schemas: ConfigurationSchemaAdmission {
                identity: "intlify-configuration-schema".to_owned(),
                revisions: revision_zero(),
                dialects: vec![DRAFT_7.to_owned()],
            },
            registry_members: RegistryMembers {
                safe_origin_kind_registry_schema: member("safe-origin-kind-registry-schema"),
                check_registry_schema: member("check-registry-schema"),
                finding_registry_schema: member("finding-registry-schema"),
                safe_origin_kind_registry: member("safe-origin-kind-registry"),
                check_registry: member("check-registry"),
                finding_registry: member("finding-registry"),
            },
            failure_precedence: PRECEDENCE.to_vec(),
            cross_authority_rules: CrossAuthorityRule::ALL.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_bounds_are_015s_eighteen_in_canonical_order() {
        let ids = ConstructionBoundId::ALL
            .iter()
            .map(|id| serde_json::to_value(id).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            [
                "input.maxTotalArtifactBytes",
                "input.maxSingleArtifactBytes",
                "input.maxDepth",
                "input.maxNodes",
                "input.maxCollectionEntries",
                "input.maxTotalStringBytes",
                "input.maxSingleStringBytes",
                "schema.maxConfigurationSchemas",
                "schema.maxSingleArtifactBytes",
                "schema.maxTotalArtifactBytes",
                "schema.maxNodes",
                "schema.maxReferences",
                "schema.maxValidationWorkUnits",
                "registry.maxRows",
                "registry.maxSingleMemberArtifactBytes",
                "registry.maxTotalArtifactBytes",
                "registry.maxValidationWorkUnits",
                "crossAuthority.maxChecks",
            ]
            .map(|id| json!(id))
        );
    }

    #[test]
    fn each_bound_counts_in_the_unit_015s_table_gives_it() {
        use ConstructionBoundId as B;
        use ConstructionUnit as U;
        for (id, unit) in [
            (B::InputMaxTotalArtifactBytes, U::ArtifactByte),
            (B::InputMaxSingleArtifactBytes, U::ArtifactByte),
            (B::InputMaxDepth, U::JsonLevel),
            (B::InputMaxNodes, U::JsonNode),
            (B::InputMaxCollectionEntries, U::JsonCollectionEntry),
            (B::InputMaxTotalStringBytes, U::StringByte),
            (B::InputMaxSingleStringBytes, U::StringByte),
            (B::SchemaMaxConfigurationSchemas, U::ConfigurationSchema),
            (B::SchemaMaxSingleArtifactBytes, U::ArtifactByte),
            (B::SchemaMaxTotalArtifactBytes, U::ArtifactByte),
            (B::SchemaMaxNodes, U::SchemaNode),
            (B::SchemaMaxReferences, U::SchemaReference),
            (B::SchemaMaxValidationWorkUnits, U::ValidationPair),
            (B::RegistryMaxRows, U::RegistryRow),
            (B::RegistryMaxSingleMemberArtifactBytes, U::ArtifactByte),
            (B::RegistryMaxTotalArtifactBytes, U::ArtifactByte),
            (B::RegistryMaxValidationWorkUnits, U::ValidationPair),
            (B::CrossAuthorityMaxChecks, U::CrossAuthorityCheck),
        ] {
            assert_eq!(id.unit(), unit, "{id:?}");
        }
        assert_eq!(
            [
                U::ArtifactByte,
                U::JsonLevel,
                U::JsonNode,
                U::JsonCollectionEntry,
                U::StringByte,
                U::ConfigurationSchema,
                U::SchemaNode,
                U::SchemaReference,
                U::ValidationPair,
                U::RegistryRow,
                U::CrossAuthorityCheck,
            ]
            .map(|unit| serde_json::to_value(unit).unwrap()),
            [
                "artifact-byte",
                "json-level",
                "json-node",
                "json-collection-entry",
                "string-byte",
                "configuration-schema",
                "schema-node",
                "schema-reference",
                "validation-pair",
                "registry-row",
                "cross-authority-check",
            ]
            .map(|unit| json!(unit))
        );
    }

    #[test]
    fn a_single_bound_never_exceeds_its_total() {
        use ConstructionBoundId as B;
        for (single, total) in [
            (
                B::InputMaxSingleArtifactBytes,
                B::InputMaxTotalArtifactBytes,
            ),
            (B::InputMaxSingleStringBytes, B::InputMaxTotalStringBytes),
            (
                B::SchemaMaxSingleArtifactBytes,
                B::SchemaMaxTotalArtifactBytes,
            ),
            (
                B::RegistryMaxSingleMemberArtifactBytes,
                B::RegistryMaxTotalArtifactBytes,
            ),
        ] {
            assert!(single.value().get() <= total.value().get(), "{single:?}");
        }
        // A schema or registry artifact is also an input artifact.
        let input = B::InputMaxTotalArtifactBytes.value().get();
        assert!(B::SchemaMaxTotalArtifactBytes.value().get() <= input);
        assert!(B::RegistryMaxTotalArtifactBytes.value().get() <= input);
    }

    #[test]
    fn the_body_admits_015s_components_members_and_dialect_at_revision_zero() {
        let body = serde_json::to_value(ConstructionAdmission::revision_zero()).unwrap();
        assert_eq!(
            body["components"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(role, admitted)| (role.as_str(), admitted["identity"].as_str().unwrap()))
                .collect::<Vec<_>>(),
            [
                (
                    "profileSpecificationSchema",
                    "intlify-project-profile-specification-schema"
                ),
                ("profileSpecification", "intlify-project-profile"),
                (
                    "configurationSchemaAuthoritySet",
                    "intlify-configuration-schema-authority-set"
                ),
                (
                    "resolverCapabilitySpecification",
                    "intlify-project-profile-resolver-capability"
                ),
                (
                    "registryPackage",
                    "intlify-project-profile-resolver-registry-package"
                ),
            ]
        );
        assert_eq!(
            body["registryMembers"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(role, admitted)| (role.as_str(), admitted["identity"].as_str().unwrap()))
                .collect::<Vec<_>>(),
            [
                (
                    "safe-origin-kind-registry-schema",
                    "intlify-project-profile-safe-origin-kind-registry-schema"
                ),
                (
                    "check-registry-schema",
                    "intlify-project-profile-check-registry-schema"
                ),
                (
                    "finding-registry-schema",
                    "intlify-project-profile-finding-registry-schema"
                ),
                (
                    "safe-origin-kind-registry",
                    "intlify-project-profile-safe-origin-kind-registry"
                ),
                ("check-registry", "intlify-project-profile-check-registry"),
                (
                    "finding-registry",
                    "intlify-project-profile-finding-registry"
                ),
            ]
        );
        assert_eq!(
            body["configurationSchemas"],
            json!({
                "identity": "intlify-configuration-schema",
                "revisions": ["0"],
                "dialects": ["http://json-schema.org/draft-07/schema#"]
            })
        );
        // Every slot admits revision "0" alone.
        for slot in body["components"]
            .as_object()
            .unwrap()
            .values()
            .chain(body["registryMembers"].as_object().unwrap().values())
        {
            assert_eq!(slot["revisions"], json!(["0"]));
        }
    }

    #[test]
    fn a_container_is_checked_against_its_tuple_and_its_container_schema() {
        let body = ConstructionAdmission::revision_zero();
        assert_eq!(
            body.components
                .configuration_schema_authority_set
                .envelope_schemas,
            [
                EnvelopeSchema::ComponentTuple,
                EnvelopeSchema::ConfigurationSchemaAuthoritySet
            ]
        );
        assert_eq!(
            body.components.registry_package.envelope_schemas,
            [
                EnvelopeSchema::ComponentTuple,
                EnvelopeSchema::RegistryPackage
            ]
        );
        assert_eq!(
            body.components.profile_specification.envelope_schemas,
            [EnvelopeSchema::ProfileSpecificationTuple]
        );
    }

    #[test]
    fn every_bound_value_reads_back_as_a_bound() {
        for record in ConstructionAdmission::revision_zero().bounds {
            assert_eq!(
                Bound::read(&record.value).map(Bound::get),
                Ok(record.id.value().get()),
                "{:?}",
                record.id
            );
        }
    }
}
