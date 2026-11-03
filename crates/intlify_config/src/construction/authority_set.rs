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
//!
//! Construction checks the set it is given, rows 15 to 19 of the
//! construction-failure precedence, and then each mapped schema in
//! `schemaVersion` order, rows 20 to 24, all of one schema's rows before the
//! next schema's.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use intlify_shared_json::decode;

use crate::model::CONFIGURATION_SCHEMA_VERSION;

use super::admission::DRAFT_7;
use super::component::{self, Artifact, Declared, Refusal, Rows, Submitted};
use super::failure::{Check, Component};
use super::profile;
use super::root::ConstructionRoot;
use super::validator::schema::Schema;

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

const ROWS: Rows = Rows::of(Component::ConfigurationSchemaAuthoritySet);
const MEMBER_IDENTITY_ROW: Refusal =
    Refusal::at(Component::ConfigurationSchemaMember, Check::Identity);
const MEMBER_REVISION_ROW: Refusal =
    Refusal::at(Component::ConfigurationSchemaMember, Check::Revision);
const MEMBER_DIALECT_ROW: Refusal =
    Refusal::at(Component::ConfigurationSchemaMember, Check::Dialect);
const MEMBER_DIGEST_ROW: Refusal =
    Refusal::at(Component::ConfigurationSchemaMember, Check::FileDigest);
const MEMBER_SCHEMA_ROW: Refusal =
    Refusal::at(Component::ConfigurationSchemaMember, Check::MetaSchema);

/// The Configuration Schema Authority Set as construction is given it: the
/// component, and each mapped schema's bytes by `schemaVersion`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SubmittedSet {
    pub(crate) component: Submitted,
    pub(crate) members: BTreeMap<String, Artifact>,
}

/// A mapped configuration schema construction admitted.
#[derive(Debug)]
pub(crate) struct AdmittedMember {
    /// The set's record of it.
    pub(crate) member: Member,
    /// The schema, read by the validator.
    pub(crate) schema: Schema,
}

/// A Configuration Schema Authority Set construction admitted.
#[derive(Debug)]
pub(crate) struct AdmittedSet {
    /// What its tuple declares.
    pub(crate) declared: Declared,
    /// The Profile Specification it belongs to.
    pub(crate) profile_specification: ProfileAssociation,
    /// Each mapped schema, by `schemaVersion`.
    pub(crate) members: BTreeMap<String, AdmittedMember>,
}

/// Check the Configuration Schema Authority Set construction is given: rows
/// 15 to 24 of the construction-failure precedence.
///
/// The envelope row adds the container's envelope schema, its `schemaVersion`
/// keys in ascending unsigned UTF-8 order, and bytes for every schema it maps.
/// Bytes for a `schemaVersion` the set does not map are read by no check.
pub(crate) fn admit(
    root: &ConstructionRoot,
    submitted: Option<&SubmittedSet>,
) -> Result<AdmittedSet, Refusal> {
    let admission = &root
        .admission()
        .components
        .configuration_schema_authority_set;
    let supplied = submitted.map(|set| &set.members);
    let checked = component::check(
        root,
        ROWS,
        admission,
        submitted.map(|set| &set.component),
        |value| container(value, supplied?),
    )?;
    let (set, artifacts) = checked.read;
    let mut members = BTreeMap::new();
    for ((version, member), artifact) in set.members.into_iter().zip(artifacts) {
        let schema = admit_member(root, &member, artifact)?;
        members.insert(version, AdmittedMember { member, schema });
    }
    Ok(AdmittedSet {
        declared: checked.declared,
        profile_specification: set.profile_specification,
        members,
    })
}

/// Read a container its envelope schema admitted: its `schemaVersion` keys
/// must be in ascending unsigned UTF-8 order, and every schema it maps must
/// have bytes, which are returned in that order.
fn container<'a>(
    value: &Value,
    supplied: &'a BTreeMap<String, Artifact>,
) -> Option<(ConfigurationSchemaAuthoritySet, Vec<&'a Artifact>)> {
    let versions = value
        .get("members")?
        .as_object()?
        .keys()
        .collect::<Vec<_>>();
    if !versions
        .windows(2)
        .all(|pair| pair[0].as_bytes() < pair[1].as_bytes())
    {
        return None;
    }
    let set = decode::typed::<ConfigurationSchemaAuthoritySet>(value.clone()).ok()?;
    let artifacts = set
        .members
        .keys()
        .map(|version| supplied.get(version))
        .collect::<Option<Vec<_>>>()?;
    Some((set, artifacts))
}

/// Check one mapped schema: rows 20 to 24 of the construction-failure
/// precedence.
fn admit_member(
    root: &ConstructionRoot,
    member: &Member,
    artifact: &Artifact,
) -> Result<Schema, Refusal> {
    let admission = &root.admission().configuration_schemas;
    if member.identity != admission.identity {
        return Err(MEMBER_IDENTITY_ROW);
    }
    if !admission.revisions.contains(&member.revision) {
        return Err(MEMBER_REVISION_ROW);
    }
    if !admission.dialects.contains(&member.dialect) {
        return Err(MEMBER_DIALECT_ROW);
    }
    if component::file_digest(artifact.bytes()).as_str() != member.digest {
        return Err(MEMBER_DIGEST_ROW);
    }
    artifact
        .value()
        .filter(|document| component::declares_draft_7(document))
        .and_then(|document| component::draft_7_schema(root, document))
        .ok_or(MEMBER_SCHEMA_ROW)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::construction::admission::{ConstructionAdmission, EnvelopeSchema};
    use crate::construction::component::testing::{row, submitted};
    use crate::construction::envelope;
    use crate::construction::meta_schema::DRAFT_7_META_SCHEMA;

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

    const COMMITTED_SET: &[u8] =
        include_bytes!("../../conformance/authorities/configuration-schema-authority-set-v0.json");
    const COMMITTED_MEMBER: &[u8] =
        include_bytes!("../../conformance/authorities/configuration-v0.schema.json");
    const DRAFT_6: &str = "http://json-schema.org/draft-06/schema#";

    fn root() -> &'static ConstructionRoot {
        ConstructionRoot::built_in().unwrap()
    }

    /// The bytes of `value` after `change`.
    fn changed(value: &Value, change: impl FnOnce(&mut Value)) -> Vec<u8> {
        let mut value = value.clone();
        change(&mut value);
        serde_json::to_vec_pretty(&value).unwrap()
    }

    /// A set that declares `identity`, `revision`, and the file digest of
    /// `bytes`, with these mapped schemas' bytes.
    fn set_submitted_as(
        identity: &str,
        revision: &str,
        bytes: &[u8],
        members: &[(&str, &[u8])],
    ) -> SubmittedSet {
        SubmittedSet {
            component: submitted(identity, revision, bytes),
            members: members
                .iter()
                .map(|(version, bytes)| ((*version).to_owned(), Artifact::read(bytes.to_vec())))
                .collect(),
        }
    }

    fn set_submitted(bytes: &[u8], members: &[(&str, &[u8])]) -> SubmittedSet {
        set_submitted_as(IDENTITY, REVISION, bytes, members)
    }

    /// The record construction admits for a mapped schema with these bytes.
    fn record(bytes: &[u8]) -> Value {
        json!({
            "identity": MEMBER_IDENTITY,
            "revision": MEMBER_REVISION,
            "dialect": DRAFT_7,
            "digest": component::file_digest(bytes).as_str()
        })
    }

    /// The bytes of a set of the committed Profile Specification that maps
    /// these records, written in this order.
    fn set_bytes(records: &[(&str, Value)]) -> Vec<u8> {
        let mut members = serde_json::Map::new();
        for (version, record) in records {
            members.insert((*version).to_owned(), record.clone());
        }
        serde_json::to_vec_pretty(&json!({
            "profileSpecification": {
                "identity": profile::IDENTITY,
                "revision": profile::REVISION,
                "semanticDigest": profile::SEMANTIC_DIGEST
            },
            "members": members
        }))
        .unwrap()
    }

    /// A set that maps each `schemaVersion` to these bytes, recorded as
    /// construction admits them.
    fn mapping(members: &[(&str, &[u8])]) -> SubmittedSet {
        let records = members
            .iter()
            .map(|(version, bytes)| (*version, record(bytes)))
            .collect::<Vec<_>>();
        set_submitted(&set_bytes(&records), members)
    }

    fn first(submitted: &SubmittedSet) -> Option<usize> {
        row(admit(root(), Some(submitted)))
    }

    #[test]
    fn construction_admits_the_committed_set_and_its_schema() {
        let admitted = admit(
            root(),
            Some(&set_submitted(COMMITTED_SET, &[("0", COMMITTED_MEMBER)])),
        )
        .unwrap();
        assert_eq!(admitted.declared.identity, IDENTITY);
        assert_eq!(admitted.declared.digest, DIGEST);
        assert_eq!(
            admitted.profile_specification,
            ProfileAssociation {
                identity: profile::IDENTITY.to_owned(),
                revision: profile::REVISION.to_owned(),
                semantic_digest: profile::SEMANTIC_DIGEST.to_owned(),
            }
        );
        assert_eq!(admitted.members.keys().collect::<Vec<_>>(), ["0"]);
        let member = &admitted.members["0"];
        assert_eq!(
            member.member,
            ConfigurationSchemaAuthoritySet::revision_zero(profile::SEMANTIC_DIGEST, MEMBER_DIGEST)
                .members["0"]
        );
        // The schema read is the configuration schema: it admits a
        // configuration, and refuses another `schemaVersion`.
        let configuration = crate::profile_fixtures::minimal_config();
        assert!(member.schema.validate(&configuration).valid);
        let mut other = configuration;
        other["schemaVersion"] = json!("1");
        assert!(!member.schema.validate(&other).valid);
    }

    #[test]
    fn schema_versions_are_checked_in_ascending_unsigned_utf_8_order() {
        // Bytes, not numbers, order the keys: "10" comes before "2".
        let versions = ["0", "10", "2", "a", "a.b", "b"];
        let members = versions.map(|version| (version, COMMITTED_MEMBER));
        let admitted = admit(root(), Some(&mapping(&members))).unwrap();
        assert_eq!(admitted.members.keys().collect::<Vec<_>>(), versions);
        for (from, to) in [(1, 2), (0, 5), (3, 4)] {
            let mut swapped = members;
            swapped.swap(from, to);
            assert_eq!(first(&mapping(&swapped)), Some(16), "{swapped:?}");
        }
    }

    #[test]
    fn each_set_check_refuses_with_its_row() {
        let committed: Value = serde_json::from_slice(COMMITTED_SET).unwrap();
        let member = [("0", COMMITTED_MEMBER)];
        let with = |change: fn(&mut Value)| set_submitted(&changed(&committed, change), &member);
        assert_eq!(row(admit(root(), None)), Some(15));
        let mut semantic = set_submitted(COMMITTED_SET, &member);
        semantic.component.tuple["semanticDigest"] = json!(profile::SEMANTIC_DIGEST);
        let mut reformatted = set_submitted(COMMITTED_SET, &member);
        reformatted.component.artifact = Artifact::read(serde_json::to_vec(&committed).unwrap());
        let cases = [
            // A component tuple declares no semantic digest.
            (semantic, 16),
            (set_submitted(b"{\"members\": ", &member), 16),
            (
                with(|set| {
                    set["path"] = json!(PATH);
                }),
                16,
            ),
            (
                with(|set| {
                    set.as_object_mut().unwrap().remove("profileSpecification");
                }),
                16,
            ),
            (with(|set| set["members"] = json!({})), 16),
            (
                with(|set| {
                    let record = set["members"]["0"].clone();
                    set["members"] = json!({ "Zero": record });
                }),
                16,
            ),
            (
                with(|set| {
                    set["members"]["0"]
                        .as_object_mut()
                        .unwrap()
                        .remove("dialect");
                }),
                16,
            ),
            // Bytes for every mapped schema, under its own `schemaVersion`.
            (set_submitted(COMMITTED_SET, &[]), 16),
            (set_submitted(COMMITTED_SET, &[("1", COMMITTED_MEMBER)]), 16),
            (
                set_submitted_as(profile::IDENTITY, REVISION, COMMITTED_SET, &member),
                17,
            ),
            (set_submitted_as(IDENTITY, "1", COMMITTED_SET, &member), 18),
            // The same set in other bytes than the declared ones.
            (reformatted, 19),
        ];
        for (submitted, expected) in cases {
            assert_eq!(first(&submitted), Some(expected), "{submitted:?}");
        }
    }

    #[test]
    fn each_mapped_schema_check_refuses_with_its_row() {
        let schema: Value = serde_json::from_slice(COMMITTED_MEMBER).unwrap();
        let recorded = |change: fn(&mut Value)| {
            let mut record = record(COMMITTED_MEMBER);
            change(&mut record);
            set_submitted(&set_bytes(&[("0", record)]), &[("0", COMMITTED_MEMBER)])
        };
        let mapped = |change: fn(&mut Value)| {
            let bytes = changed(&schema, change);
            mapping(&[("0", &bytes)])
        };
        let cases = [
            (
                recorded(|record| record["identity"] = json!("intlify-configuration")),
                20,
            ),
            (recorded(|record| record["revision"] = json!("1")), 21),
            (recorded(|record| record["dialect"] = json!(DRAFT_6)), 22),
            // The digest of another file.
            (recorded(|record| record["digest"] = json!(DIGEST)), 23),
            (mapping(&[("0", b"{\"$schema\": ")]), 24),
            (mapping(&[("0", b"true")]), 24),
            (
                mapped(|schema| {
                    schema.as_object_mut().unwrap().remove("$schema");
                }),
                24,
            ),
            (mapped(|schema| schema["$schema"] = json!(DRAFT_6)), 24),
            // The meta-schema refuses a title that is not a string; the
            // validator, a reference to another document.
            (mapped(|schema| schema["title"] = json!(1)), 24),
            (
                mapped(|schema| {
                    schema["properties"]["profiles"] = json!({ "$ref": "other.json" });
                }),
                24,
            ),
        ];
        for (submitted, expected) in cases {
            assert_eq!(first(&submitted), Some(expected), "{submitted:?}");
        }
    }

    #[test]
    fn each_mapped_schema_passes_all_its_rows_before_the_next() {
        let mut unadmitted = record(COMMITTED_MEMBER);
        unadmitted["identity"] = json!("another");
        // "0" fails its last row, and "1" its first.
        let set = set_submitted(
            &set_bytes(&[("0", record(b"{")), ("1", unadmitted.clone())]),
            &[("0", b"{"), ("1", COMMITTED_MEMBER)],
        );
        assert_eq!(first(&set), Some(24));
        // "0" fails its first row, and "1" its last.
        let set = set_submitted(
            &set_bytes(&[("0", unadmitted), ("1", record(b"{"))]),
            &[("0", COMMITTED_MEMBER), ("1", b"{")],
        );
        assert_eq!(first(&set), Some(20));
    }

    #[test]
    fn bytes_for_a_schema_version_the_set_does_not_map_are_read_by_no_check() {
        let set = set_submitted(COMMITTED_SET, &[("0", COMMITTED_MEMBER), ("9", b"{")]);
        let admitted = admit(root(), Some(&set)).unwrap();
        assert_eq!(admitted.members.keys().collect::<Vec<_>>(), ["0"]);
    }

    #[test]
    fn the_set_s_rows_keep_their_order() {
        let member = [("0", COMMITTED_MEMBER)];
        assert_eq!(
            first(&set_submitted_as(
                profile::IDENTITY,
                REVISION,
                b"{}",
                &member
            )),
            Some(16)
        );
        let mut identity = set_submitted_as(profile::IDENTITY, "1", COMMITTED_SET, &member);
        identity.component.tuple["digest"] = json!(MEMBER_DIGEST);
        assert_eq!(first(&identity), Some(17));
        identity.component.tuple["identity"] = json!(IDENTITY);
        assert_eq!(first(&identity), Some(18));
        identity.component.tuple["revision"] = json!(REVISION);
        assert_eq!(first(&identity), Some(19));
        // The set's rows come before its members'.
        let mut unadmitted = record(COMMITTED_MEMBER);
        unadmitted["identity"] = json!("another");
        let mut digest = set_submitted(&set_bytes(&[("0", unadmitted)]), &member);
        digest.component.tuple["digest"] = json!(DIGEST);
        assert_eq!(first(&digest), Some(19));
    }
}
