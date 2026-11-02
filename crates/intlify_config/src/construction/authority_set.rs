// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Configuration Schema Authority Set: the container that maps each
//! configuration `schemaVersion` to the schema that admits it.
//!
//! The container names the Profile Specification it belongs to by identity,
//! revision, and semantic digest, which construction compares with the admitted
//! Profile Specification. Each member is one mapped configuration schema's
//! identity, revision, dialect, and file digest, keyed by its `schemaVersion`
//! in ascending unsigned UTF-8 order; the member's bytes are supplied beside
//! the container. Revision `"0"` maps one `schemaVersion`, `"0"`, to the
//! configuration schema this crate publishes, whose bytes are the ones the
//! schema generator writes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::CONFIGURATION_SCHEMA_VERSION;

use super::admission::DRAFT_7;
use super::profile;

/// The set's identity.
pub(crate) const IDENTITY: &str = "intlify-configuration-schema-authority-set";

/// The set's revision.
pub(crate) const REVISION: &str = "0";

/// The set's path below the conformance suite's root.
pub(crate) const PATH: &str = "authorities/configuration-schema-authority-set-v0.json";

/// The set's file digest, which the resolver is compiled from.
pub(crate) const DIGEST: &str =
    "sha256:52cf4c12db0949aff5209c509e1dc92a24eacd243a6364480f50005c883e8abe";

/// A mapped configuration schema's identity.
pub(crate) const MEMBER_IDENTITY: &str = "intlify-configuration-schema";

/// The revision of the schema mapped to `schemaVersion` `"0"`.
pub(crate) const MEMBER_REVISION: &str = "0";

/// The path of the schema mapped to `schemaVersion` `"0"` below the
/// conformance suite's root.
pub(crate) const MEMBER_PATH: &str = "authorities/configuration-v0.schema.json";

/// The file digest of the schema mapped to `schemaVersion` `"0"`, which the
/// resolver is compiled from.
pub(crate) const MEMBER_DIGEST: &str =
    "sha256:3fdaf3ba99feb64947154dc430a1fc573c438f559f526a203aecbab9c6eae2d1";

/// The Profile Specification a set belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProfileAssociation {
    pub(crate) identity: String,
    pub(crate) revision: String,
    pub(crate) semantic_digest: String,
}

/// One mapped configuration schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Member {
    pub(crate) identity: String,
    pub(crate) revision: String,
    pub(crate) dialect: String,
    pub(crate) digest: String,
}

/// The container's body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ConfigurationSchemaAuthoritySet {
    pub(crate) profile_specification: ProfileAssociation,
    pub(crate) members: BTreeMap<String, Member>,
}

impl ConfigurationSchemaAuthoritySet {
    /// The revision-`"0"` set: it belongs to the Profile Specification whose
    /// semantic digest is `semantic_digest`, and maps `schemaVersion` `"0"` to
    /// the configuration schema whose file digest is `member_digest`.
    pub(crate) fn revision_zero(semantic_digest: &str, member_digest: &str) -> Self {
        Self {
            profile_specification: ProfileAssociation {
                identity: profile::IDENTITY.to_owned(),
                revision: profile::REVISION.to_owned(),
                semantic_digest: semantic_digest.to_owned(),
            },
            members: BTreeMap::from([(
                CONFIGURATION_SCHEMA_VERSION.to_owned(),
                Member {
                    identity: MEMBER_IDENTITY.to_owned(),
                    revision: MEMBER_REVISION.to_owned(),
                    dialect: DRAFT_7.to_owned(),
                    digest: member_digest.to_owned(),
                },
            )]),
        }
    }
}

/// The bytes of the schema mapped to `schemaVersion` `"0"`: the configuration
/// schema as this crate publishes it.
///
/// # Errors
///
/// Returns the serialization failure of the generated schema, which the types
/// rule out.
pub(crate) fn member_bytes() -> Result<Vec<u8>, serde_json::Error> {
    crate::schema::format_schema(crate::schema::project_profile_config_schema()?)
        .map(String::into_bytes)
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::construction::admission::{ConstructionAdmission, EnvelopeSchema};
    use crate::construction::envelope;
    use crate::construction::meta_schema::DRAFT_7_META_SCHEMA;
    use crate::construction::validator::schema::Schema;

    const SEMANTIC: &str =
        "sha256:1111111111111111111111111111111111111111111111111111111111111111";
    const SCHEMA: &str = "sha256:2222222222222222222222222222222222222222222222222222222222222222";

    /// The published configuration schema.
    const PUBLISHED: &[u8] = include_bytes!("../../schema/project-profile-config-v0.schema.json");

    fn set() -> Value {
        serde_json::to_value(ConfigurationSchemaAuthoritySet::revision_zero(
            SEMANTIC, SCHEMA,
        ))
        .unwrap()
    }

    #[test]
    fn the_set_maps_schema_version_0_to_the_configuration_schema() {
        let set = set();
        assert_eq!(
            set,
            json!({
                "profileSpecification": {
                    "identity": "intlify-project-profile",
                    "revision": "0",
                    "semanticDigest": SEMANTIC,
                },
                "members": {
                    "0": {
                        "identity": "intlify-configuration-schema",
                        "revision": "0",
                        "dialect": "http://json-schema.org/draft-07/schema#",
                        "digest": SCHEMA,
                    },
                },
            })
        );
        // The members are written in the order the fields declare them.
        assert_eq!(
            serde_json::to_string(&set).unwrap(),
            format!(
                "{{\"profileSpecification\":{{\"identity\":\"intlify-project-profile\",\
                 \"revision\":\"0\",\"semanticDigest\":\"{SEMANTIC}\"}},\"members\":{{\"0\":\
                 {{\"identity\":\"intlify-configuration-schema\",\"revision\":\"0\",\"dialect\":\
                 \"http://json-schema.org/draft-07/schema#\",\"digest\":\"{SCHEMA}\"}}}}}}"
            )
        );
    }

    #[test]
    fn schema_versions_are_keyed_in_ascending_unsigned_utf_8_order() {
        let mut set = ConfigurationSchemaAuthoritySet::revision_zero(SEMANTIC, SCHEMA);
        let member = set.members["0"].clone();
        for version in ["10", "2", "a", "B", "\u{e9}"] {
            set.members.insert(version.to_owned(), member.clone());
        }
        let value = serde_json::to_value(&set).unwrap();
        assert_eq!(
            value["members"]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["0", "10", "2", "B", "a", "\u{e9}"]
        );
    }

    #[test]
    fn the_set_is_an_instance_of_its_envelope_schema() {
        let document = envelope::schema(EnvelopeSchema::ConfigurationSchemaAuthoritySet);
        let schema = Schema::compile(&document).unwrap();
        let oracle = jsonschema::draft7::new(&document).unwrap();
        let set = set();
        assert!(schema.validate(&set).valid);
        assert!(oracle.is_valid(&set));
    }

    #[test]
    fn the_set_names_what_construction_admits() {
        let admission = ConstructionAdmission::revision_zero();
        let component = &admission.components.configuration_schema_authority_set;
        assert_eq!(component.identity, IDENTITY);
        assert_eq!(component.revisions, [REVISION]);
        let schemas = &admission.configuration_schemas;
        assert_eq!(schemas.identity, MEMBER_IDENTITY);
        assert_eq!(schemas.revisions, [MEMBER_REVISION]);
        assert_eq!(schemas.dialects, [DRAFT_7]);
        assert_eq!(
            admission.components.profile_specification.identity,
            profile::IDENTITY
        );
    }

    #[test]
    fn the_member_is_the_configuration_schema_this_crate_publishes() {
        let bytes = member_bytes().unwrap();
        assert_eq!(bytes, PUBLISHED);
        let document: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(document["$schema"], DRAFT_7);
        let meta_schema: Value = serde_json::from_str(DRAFT_7_META_SCHEMA).unwrap();
        assert!(
            Schema::compile(&meta_schema)
                .unwrap()
                .validate(&document)
                .valid
        );
        assert!(Schema::compile(&document).is_ok());
    }

    #[test]
    fn the_set_reads_back_as_its_type() {
        let set = set();
        let read: ConfigurationSchemaAuthoritySet = serde_json::from_value(set.clone()).unwrap();
        assert_eq!(
            read,
            ConfigurationSchemaAuthoritySet::revision_zero(SEMANTIC, SCHEMA)
        );
        // Each of its objects is closed.
        for pointer in ["", "/profileSpecification", "/members/0"] {
            let mut unknown = set.clone();
            unknown
                .pointer_mut(pointer)
                .and_then(Value::as_object_mut)
                .unwrap()
                .insert("path".to_owned(), json!(MEMBER_PATH));
            assert!(
                serde_json::from_value::<ConfigurationSchemaAuthoritySet>(unknown).is_err(),
                "{pointer}"
            );
        }
    }
}
