// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Root Package's artifacts and the construction authorities, written from
//! this crate's values.
//!
//! Every member is written first, and the Root Package artifact then lists each
//! member's file digest. A file digest covers whitespace, member order, and
//! line endings, so the bytes are fixed here: two-space indentation, members in
//! the order the values hold them, and one final newline. The checked-in
//! artifacts are kept out of repository formatting and line-ending conversion.
//!
//! The authorities follow the same form, except the schema mapped to
//! `schemaVersion` `"0"`, whose bytes are the configuration schema as this
//! crate publishes it. The Profile Specification's semantic digest goes into
//! the Configuration Schema Authority Set, and so does the mapped schema's
//! file digest. The Registry Package's members follow the same form.
//!
//! The construction vectors come last: they construct the written
//! authorities and members under the built-in root and the digests this crate
//! pins, and record the Root Package's canonical preimage and the identity
//! construction derives, so they wait for the pins after a value changes.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use intlify_shared_json::encoding::digest_bytes;
use intlify_shared_json::token::IntegrityDigest;

use super::admission::{ComponentAdmission, ConstructionAdmission};
use super::authority_set::{self, ConfigurationSchemaAuthoritySet, SubmittedSet};
use super::capability::{self, CapabilitySpecification};
use super::component::{self, file_digest, Submitted};
use super::flow::{self, ConstructionInput};
use super::identity;
use super::profile::{self, ProjectProfileSpecification};
use super::registry::check::{self, CheckRegistry};
use super::registry::finding::{self, FindingRegistry};
use super::registry::safe_origin::{self, SafeOriginKindRegistry};
use super::registry::{self as package, RegistryRole, SubmittedPackage};
use super::root::{canonical_digest, canonical_preimage, ConstructionRoot, RootRole, PATH};
use super::rules::CrossAuthorityRules;
use super::{envelope, meta_schema};
use crate::model::CONFIGURATION_SCHEMA_VERSION;

/// The construction vectors' path below the conformance suite's root.
pub(crate) const VECTORS_PATH: &str = "construction/construction-vectors-v0.json";

/// One artifact and its path below the conformance suite's root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub path: &'static str,
    pub bytes: Vec<u8>,
}

/// The Registry Package members' file digests the implementation compiles in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryPins {
    pub safe_origin_kind_registry_schema: String,
    pub safe_origin_kind_registry: String,
    pub check_registry_schema: String,
    pub check_registry: String,
    pub finding_registry_schema: String,
    pub finding_registry: String,
    pub registry_package: String,
}

/// The two digests the implementation compiles in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pinned {
    pub digest: String,
    pub canonical_digest: String,
}

/// The authorities' digests the implementation compiles in: the Profile
/// Specification's semantic digest, and the file digests of the other four.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorityPins {
    pub profile_specification_schema: String,
    pub profile_specification_semantic: String,
    pub configuration_schema_authority_set: String,
    pub configuration_schema: String,
    pub resolver_capability_specification: String,
}

fn write(value: &Value) -> Result<Vec<u8>, serde_json::Error> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    Ok(text.into_bytes())
}

fn member(role: RootRole) -> Result<Value, serde_json::Error> {
    match role {
        RootRole::ConstructionAdmissionSpecification => {
            serde_json::to_value(ConstructionAdmission::revision_zero())
        }
        RootRole::ConstructionAdmissionSchema => {
            crate::schema::draft7_schema::<ConstructionAdmission>()
        }
        RootRole::AuthorityEnvelopeSchemaPackage => Ok(envelope::package()),
        RootRole::JsonSchemaMetaSchemaPackage => Ok(meta_schema::package()),
        RootRole::CrossAuthorityRules => serde_json::to_value(CrossAuthorityRules::revision_zero()),
    }
}

/// Write every artifact: the five members in role order, then the Root
/// Package.
///
/// # Errors
///
/// Returns the serialization failure of a member, which the values rule out.
pub fn artifacts() -> Result<Vec<Artifact>, serde_json::Error> {
    let mut artifacts = Vec::new();
    let mut listed = Map::new();
    for role in RootRole::ALL {
        let bytes = write(&member(role)?)?;
        listed.insert(
            role.name().to_owned(),
            json!({ "digest": file_digest(&bytes).as_str() }),
        );
        artifacts.push(Artifact {
            path: role.path(),
            bytes,
        });
    }
    artifacts.push(Artifact {
        path: PATH,
        bytes: write(&json!({ "members": listed }))?,
    });
    Ok(artifacts)
}

/// Write the construction authorities: the Profile Specification Schema, the
/// Profile Specification, the Configuration Schema Authority Set, the schema
/// it maps to `schemaVersion` `"0"`, and the Resolver Capability
/// Specification.
///
/// # Errors
///
/// Returns the serialization failure of an authority, which the values rule
/// out.
///
/// # Panics
///
/// Panics when the Profile Specification has no semantic digest, which its
/// number-free body rules out.
pub fn authorities() -> Result<Vec<Artifact>, serde_json::Error> {
    let body = serde_json::to_value(ProjectProfileSpecification::revision_zero())?;
    let semantic_digest =
        profile::semantic_digest(&body).expect("the Profile Specification is number-free");
    let member = authority_set::member_bytes()?;
    let set = ConfigurationSchemaAuthoritySet::revision_zero(
        semantic_digest.as_str(),
        file_digest(&member).as_str(),
    );
    Ok(vec![
        Artifact {
            path: profile::SCHEMA_PATH,
            bytes: write(&profile::schema()?)?,
        },
        Artifact {
            path: profile::PATH,
            bytes: write(&body)?,
        },
        Artifact {
            path: authority_set::PATH,
            bytes: write(&serde_json::to_value(set)?)?,
        },
        Artifact {
            path: authority_set::MEMBER_PATH,
            bytes: member,
        },
        Artifact {
            path: capability::PATH,
            bytes: write(&serde_json::to_value(
                CapabilitySpecification::revision_zero(),
            )?)?,
        },
    ])
}

/// Write the Registry Package's members, the Safe Origin Kind Registry's
/// schema and the registry, then the Check Registry's, then the Finding
/// Registry's, and then the container that lists them.
///
/// # Errors
///
/// Returns the serialization failure of a member, which the values rule out.
pub fn registries() -> Result<Vec<Artifact>, serde_json::Error> {
    let mut artifacts = vec![
        Artifact {
            path: safe_origin::SCHEMA_PATH,
            bytes: write(&safe_origin::schema()?)?,
        },
        Artifact {
            path: safe_origin::PATH,
            bytes: write(&serde_json::to_value(
                SafeOriginKindRegistry::revision_zero(),
            )?)?,
        },
        Artifact {
            path: check::SCHEMA_PATH,
            bytes: write(&check::schema()?)?,
        },
        Artifact {
            path: check::PATH,
            bytes: write(&serde_json::to_value(CheckRegistry::revision_zero())?)?,
        },
        Artifact {
            path: finding::SCHEMA_PATH,
            bytes: write(&finding::schema()?)?,
        },
        Artifact {
            path: finding::PATH,
            bytes: write(&serde_json::to_value(FindingRegistry::revision_zero())?)?,
        },
    ];
    // The members in role order: the three schemas, then the registries.
    let bytes = |index: usize| artifacts[index].bytes.as_slice();
    let container =
        package::container([bytes(0), bytes(2), bytes(4), bytes(1), bytes(3), bytes(5)]);
    artifacts.push(Artifact {
        path: package::PATH,
        bytes: write(&container)?,
    });
    Ok(artifacts)
}

/// The Registry Package members' digests the implementation compiles in.
///
/// # Errors
///
/// Returns the serialization failure of a member, which the values rule out.
pub fn registry_pins() -> Result<RegistryPins, serde_json::Error> {
    let [schema, registry, check_schema, check_registry, finding_schema, finding_registry, container] =
        <[Artifact; 7]>::try_from(registries()?)
            .unwrap_or_else(|_| unreachable!("six members and the container are written"));
    Ok(RegistryPins {
        safe_origin_kind_registry_schema: file_digest(&schema.bytes).as_str().to_owned(),
        safe_origin_kind_registry: file_digest(&registry.bytes).as_str().to_owned(),
        check_registry_schema: file_digest(&check_schema.bytes).as_str().to_owned(),
        check_registry: file_digest(&check_registry.bytes).as_str().to_owned(),
        finding_registry_schema: file_digest(&finding_schema.bytes).as_str().to_owned(),
        finding_registry: file_digest(&finding_registry.bytes).as_str().to_owned(),
        registry_package: file_digest(&container.bytes).as_str().to_owned(),
    })
}

/// The authorities' digests the implementation compiles in.
///
/// # Errors
///
/// Returns the serialization failure of an authority, which the values rule
/// out.
///
/// # Panics
///
/// Panics when the written Profile Specification is not JSON or has no
/// semantic digest, which [`authorities`] rules out.
pub fn authority_pins() -> Result<AuthorityPins, serde_json::Error> {
    let [schema, body, set, member, capability] = <[Artifact; 5]>::try_from(authorities()?)
        .unwrap_or_else(|_| unreachable!("five authorities are written"));
    let body: Value = serde_json::from_slice(&body.bytes)?;
    Ok(AuthorityPins {
        profile_specification_schema: file_digest(&schema.bytes).as_str().to_owned(),
        profile_specification_semantic: profile::semantic_digest(&body)
            .expect("the Profile Specification is number-free")
            .as_str()
            .to_owned(),
        configuration_schema_authority_set: file_digest(&set.bytes).as_str().to_owned(),
        configuration_schema: file_digest(&member.bytes).as_str().to_owned(),
        resolver_capability_specification: file_digest(&capability.bytes).as_str().to_owned(),
    })
}

/// The two digests of the Root Package these artifacts make.
///
/// # Errors
///
/// Returns the serialization failure of a member, which the values rule out.
pub fn pinned() -> Result<Pinned, serde_json::Error> {
    let artifacts = artifacts()?;
    let members = RootRole::ALL
        .iter()
        .zip(&artifacts)
        .map(|(role, artifact)| (*role, digest_bytes(&artifact.bytes)))
        .collect::<Vec<_>>();
    let root = artifacts
        .last()
        .map(|artifact| digest_bytes(&artifact.bytes))
        .unwrap_or_default();
    Ok(Pinned {
        digest: IntegrityDigest::from_hash(root).as_str().to_owned(),
        canonical_digest: IntegrityDigest::from_hash(canonical_digest(&members))
            .as_str()
            .to_owned(),
    })
}

/// Hexadecimal digits of bytes.
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|&byte| {
            [
                DIGITS[usize::from(byte >> 4)],
                DIGITS[usize::from(byte & 0x0f)],
            ]
        })
        .map(char::from)
        .collect()
}

/// The construction input of every authority and member this crate writes,
/// each declared as construction admits it.
fn written_input() -> Result<ConstructionInput, serde_json::Error> {
    let admission = ConstructionAdmission::revision_zero();
    let components = &admission.components;
    let declared = |admitted: &ComponentAdmission, bytes: Vec<u8>| Submitted {
        tuple: json!({
            "identity": admitted.identity,
            "revision": admitted.revisions.first(),
            "digest": file_digest(&bytes).as_str()
        }),
        artifact: component::Artifact::read(bytes),
    };
    let [schema, body, set, member, capability] = <[Artifact; 5]>::try_from(authorities()?)
        .unwrap_or_else(|_| unreachable!("five authorities are written"));
    let [origin_schema, origins, check_schema, checks, finding_schema, findings, container] =
        <[Artifact; 7]>::try_from(registries()?)
            .unwrap_or_else(|_| unreachable!("six members and the container are written"));
    let semantic_digest = profile::semantic_digest(&serde_json::from_slice(&body.bytes)?)
        .expect("the Profile Specification is number-free");
    let mut profile_specification = declared(&components.profile_specification, body.bytes);
    profile_specification.tuple["semanticDigest"] = json!(semantic_digest.as_str());
    let members = [
        origin_schema,
        check_schema,
        finding_schema,
        origins,
        checks,
        findings,
    ];
    Ok(ConstructionInput {
        profile_specification_schema: Some(declared(
            &components.profile_specification_schema,
            schema.bytes,
        )),
        profile_specification: Some(profile_specification),
        configuration_schema_authority_set: Some(SubmittedSet {
            component: declared(&components.configuration_schema_authority_set, set.bytes),
            members: BTreeMap::from([(
                CONFIGURATION_SCHEMA_VERSION.to_owned(),
                component::Artifact::read(member.bytes),
            )]),
        }),
        resolver_capability_specification: Some(declared(
            &components.resolver_capability_specification,
            capability.bytes,
        )),
        registry_package: Some(SubmittedPackage {
            component: declared(&components.registry_package, container.bytes),
            members: RegistryRole::ALL
                .into_iter()
                .zip(members)
                .map(|(role, member)| (role, component::Artifact::read(member.bytes)))
                .collect(),
        }),
    })
}

/// Write the construction vectors (D15:987): the Root Package's canonical
/// preimage and digest, and the Resolver Construction Identity of every
/// authority and member this crate writes, constructed under the built-in
/// root: its preimage, its digest, and its text.
///
/// # Errors
///
/// Returns why the vectors cannot be written: a member's serialization
/// failure, which the values rule out, or the written authorities not
/// constructing, as when the pinned digests are not yet theirs.
pub fn vectors() -> Result<Artifact, String> {
    let serialization = |error: serde_json::Error| error.to_string();
    let mut members = Vec::new();
    for role in RootRole::ALL {
        let bytes = member(role)
            .and_then(|value| write(&value))
            .map_err(serialization)?;
        members.push((role, digest_bytes(&bytes)));
    }
    let root_preimage = canonical_preimage(&members);
    let input = written_input().map_err(serialization)?;
    let root = ConstructionRoot::built_in()
        .map_err(|failure| format!("the built-in Root Package is unusable: {failure:?}"))?;
    let admitted = flow::construct(root, &input).map_err(|refusal| {
        format!(
            "the written authorities do not construct: row {} refuses them with {:?}",
            refusal.number(),
            refusal.reason()
        )
    })?;
    let identity = identity::derive(root, &admitted);
    let body = json!({
        "rootPackage": {
            "preimage": hex(&root_preimage),
            "canonicalDigest": IntegrityDigest::from_hash(digest_bytes(&root_preimage)).as_str()
        },
        "constructionIdentity": {
            "preimage": hex(&identity::preimage(root, &admitted)),
            "digest": IntegrityDigest::from_hash(*identity.bytes()).as_str(),
            "text": identity.text()
        }
    });
    Ok(Artifact {
        path: VECTORS_PATH,
        bytes: write(&body).map_err(serialization)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::root::{RootPackage, CANONICAL_DIGEST, DIGEST};
    use crate::construction::validator::schema::Schema;

    /// The committed authorities, at their paths.
    const COMMITTED: [(&str, &[u8]); 5] = [
        (
            profile::SCHEMA_PATH,
            include_bytes!(
                "../../conformance/authorities/project-profile-specification-v0.schema.json"
            ),
        ),
        (
            profile::PATH,
            include_bytes!("../../conformance/authorities/project-profile-specification-v0.json"),
        ),
        (
            authority_set::PATH,
            include_bytes!(
                "../../conformance/authorities/configuration-schema-authority-set-v0.json"
            ),
        ),
        (
            authority_set::MEMBER_PATH,
            include_bytes!("../../conformance/authorities/configuration-v0.schema.json"),
        ),
        (
            capability::PATH,
            include_bytes!("../../conformance/authorities/resolver-capability-v0.json"),
        ),
    ];

    /// The committed Registry Package members and container, at their paths.
    const COMMITTED_REGISTRIES: [(&str, &[u8]); 7] = [
        (
            safe_origin::SCHEMA_PATH,
            include_bytes!("../../conformance/registries/safe-origin-kind-v0.schema.json"),
        ),
        (
            safe_origin::PATH,
            include_bytes!("../../conformance/registries/safe-origin-kind-v0.json"),
        ),
        (
            check::SCHEMA_PATH,
            include_bytes!("../../conformance/registries/check-v0.schema.json"),
        ),
        (
            check::PATH,
            include_bytes!("../../conformance/registries/check-v0.json"),
        ),
        (
            finding::SCHEMA_PATH,
            include_bytes!("../../conformance/registries/finding-v0.schema.json"),
        ),
        (
            finding::PATH,
            include_bytes!("../../conformance/registries/finding-v0.json"),
        ),
        (
            package::PATH,
            include_bytes!("../../conformance/registries/package-v0.json"),
        ),
    ];

    #[test]
    fn the_committed_registries_are_the_ones_this_crate_writes() {
        let registries = registries().unwrap();
        assert_eq!(registries.len(), 7);
        for (artifact, (path, bytes)) in registries.iter().zip(COMMITTED_REGISTRIES) {
            assert_eq!(artifact.path, path);
            assert_eq!(
                artifact.bytes, bytes,
                "{path} is stale; regenerate it with the generate_construction_artifacts example"
            );
        }
    }

    #[test]
    fn the_pinned_registry_digests_are_the_ones_the_registries_make() {
        assert_eq!(
            registry_pins().unwrap(),
            RegistryPins {
                safe_origin_kind_registry_schema: safe_origin::SCHEMA_DIGEST.to_owned(),
                safe_origin_kind_registry: safe_origin::DIGEST.to_owned(),
                check_registry_schema: check::SCHEMA_DIGEST.to_owned(),
                check_registry: check::DIGEST.to_owned(),
                finding_registry_schema: finding::SCHEMA_DIGEST.to_owned(),
                finding_registry: finding::DIGEST.to_owned(),
                registry_package: package::DIGEST.to_owned(),
            },
            "the pinned digests in construction/registry.rs or its safe_origin.rs, check.rs, or finding.rs are stale"
        );
    }

    #[test]
    fn the_committed_artifacts_are_the_ones_this_crate_writes() {
        let package = RootPackage::built_in().unwrap();
        let artifacts = artifacts().unwrap();
        assert_eq!(artifacts.len(), 6);
        for (role, artifact) in RootRole::ALL.iter().zip(&artifacts) {
            assert_eq!(
                package.member(*role),
                artifact.bytes.as_slice(),
                "{} is stale; regenerate it with the generate_construction_artifacts example",
                artifact.path
            );
        }
        assert_eq!(artifacts[5].path, PATH);
    }

    #[test]
    fn the_pinned_digests_are_the_ones_the_artifacts_make() {
        assert_eq!(
            pinned().unwrap(),
            Pinned {
                digest: DIGEST.to_owned(),
                canonical_digest: CANONICAL_DIGEST.to_owned(),
            },
            "the pinned digests in construction/root.rs are stale"
        );
    }

    #[test]
    fn the_committed_authorities_are_the_ones_this_crate_writes() {
        let authorities = authorities().unwrap();
        assert_eq!(authorities.len(), 5);
        for (artifact, (path, bytes)) in authorities.iter().zip(COMMITTED) {
            assert_eq!(artifact.path, path);
            assert_eq!(
                artifact.bytes, bytes,
                "{path} is stale; regenerate it with the generate_construction_artifacts example"
            );
        }
    }

    #[test]
    fn the_pinned_authority_digests_are_the_ones_the_authorities_make() {
        assert_eq!(
            authority_pins().unwrap(),
            AuthorityPins {
                profile_specification_schema: profile::SCHEMA_DIGEST.to_owned(),
                profile_specification_semantic: profile::SEMANTIC_DIGEST.to_owned(),
                configuration_schema_authority_set: authority_set::DIGEST.to_owned(),
                configuration_schema: authority_set::MEMBER_DIGEST.to_owned(),
                resolver_capability_specification: capability::DIGEST.to_owned(),
            },
            "the pinned digests in construction/profile.rs, construction/authority_set.rs, \
             and construction/capability.rs are stale"
        );
    }

    #[test]
    fn the_set_belongs_to_the_written_profile_specification_and_maps_the_written_schema() {
        let authorities = authorities().unwrap();
        let [schema, body, set, member] = [0, 1, 2, 3].map(|index| &authorities[index].bytes);
        let body: Value = serde_json::from_slice(body).unwrap();
        let set: Value = serde_json::from_slice(set).unwrap();
        assert_eq!(
            set["profileSpecification"],
            json!({
                "identity": profile::IDENTITY,
                "revision": profile::REVISION,
                "semanticDigest": profile::semantic_digest(&body).unwrap().as_str(),
            })
        );
        assert_eq!(set["members"]["0"]["digest"], file_digest(member).as_str());
        // Each artifact is at its path: the schema and the body are the ones
        // their module writes, and the mapped schema is the published one.
        let schema: Value = serde_json::from_slice(schema).unwrap();
        assert_eq!(schema, profile::schema().unwrap());
        assert_eq!(
            body,
            serde_json::to_value(ProjectProfileSpecification::revision_zero()).unwrap()
        );
        assert_eq!(*member, authority_set::member_bytes().unwrap());
        // The written body is valid against the written schema, which closes
        // it.
        let read = Schema::compile(&schema).unwrap();
        let oracle = jsonschema::draft7::new(&schema).unwrap();
        assert!(read.validate(&body).valid);
        assert!(oracle.is_valid(&body));
        let mut unknown = body;
        unknown["unknown"] = json!("unknown");
        assert!(!read.validate(&unknown).valid);
        assert!(!oracle.is_valid(&unknown));
    }

    /// The committed construction vectors are the ones this crate writes,
    /// and their Root Package digest is the pinned canonical digest.
    #[test]
    fn the_committed_vectors_are_the_ones_this_crate_writes() {
        let vectors = vectors().unwrap();
        assert_eq!(vectors.path, VECTORS_PATH);
        assert_eq!(
            vectors.bytes,
            include_bytes!("../../conformance/construction/construction-vectors-v0.json"),
            "rerun the generator with --write"
        );
        let value: Value = serde_json::from_slice(&vectors.bytes).unwrap();
        assert_eq!(value["rootPackage"]["canonicalDigest"], CANONICAL_DIGEST);
    }

    #[test]
    fn every_artifact_ends_with_one_newline_and_has_no_carriage_return() {
        for artifact in artifacts()
            .unwrap()
            .into_iter()
            .chain(authorities().unwrap())
            .chain(registries().unwrap())
            .chain(vectors())
        {
            assert!(artifact.bytes.ends_with(b"}\n"), "{}", artifact.path);
            assert!(!artifact.bytes.ends_with(b"\n\n"), "{}", artifact.path);
            assert!(!artifact.bytes.contains(&b'\r'), "{}", artifact.path);
        }
    }

    #[test]
    fn the_admission_body_validates_against_its_own_schema() {
        let artifacts = artifacts().unwrap();
        let body: Value = serde_json::from_slice(&artifacts[0].bytes).unwrap();
        let schema: Value = serde_json::from_slice(&artifacts[1].bytes).unwrap();
        assert_eq!(schema["$schema"], super::super::admission::DRAFT_7);
        assert!(jsonschema::draft7::meta::validator().is_valid(&schema));
        let validator = jsonschema::draft7::new(&schema).unwrap();
        assert!(
            validator.is_valid(&body),
            "{:?}",
            validator.iter_errors(&body).collect::<Vec<_>>()
        );
        // The schema closes the inventory: an unknown bound, unit, check, or
        // reason is refused, and so is a missing row.
        for (pointer, replacement) in [
            ("/bounds/0/id", json!("input.maxEverything")),
            ("/bounds/0/unit", json!("byte")),
            ("/bounds/0/value", json!("0")),
            ("/bounds/0/value", json!(16_777_216)),
            ("/failurePrecedence/0/check", json!("bounds")),
            ("/failurePrecedence/0/reason", json!("construction-failed")),
            ("/crossAuthorityRules/0", json!("profile-match")),
            (
                "/components/registryPackage/envelopeSchemas/1",
                json!("registry"),
            ),
        ] {
            let mut changed = body.clone();
            *changed.pointer_mut(pointer).unwrap() = replacement;
            assert!(!validator.is_valid(&changed), "{pointer}");
        }
        let mut short = body;
        short["failurePrecedence"].as_array_mut().unwrap().pop();
        assert!(!validator.is_valid(&short));
    }
}
