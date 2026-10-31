// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Root Package's artifacts, written from this crate's values.
//!
//! Every member is written first, and the Root Package artifact then lists each
//! member's file digest. A file digest covers whitespace, member order, and
//! line endings, so the bytes are fixed here: two-space indentation, members in
//! the order the values hold them, and one final newline. The checked-in
//! artifacts are kept out of repository formatting and line-ending conversion.

use serde_json::{json, Map, Value};

use intlify_shared_json::encoding::digest_bytes;
use intlify_shared_json::token::IntegrityDigest;

use super::admission::ConstructionAdmission;
use super::root::{canonical_digest, RootRole, PATH};
use super::rules::CrossAuthorityRules;
use super::{envelope, meta_schema};

/// One artifact and its path below the conformance suite's root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub path: &'static str,
    pub bytes: Vec<u8>,
}

/// The two digests the implementation compiles in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pinned {
    pub digest: String,
    pub canonical_digest: String,
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
            json!({ "digest": IntegrityDigest::from_hash(digest_bytes(&bytes)).as_str() }),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::root::{RootPackage, CANONICAL_DIGEST, DIGEST};

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
    fn every_artifact_ends_with_one_newline_and_has_no_carriage_return() {
        for artifact in artifacts().unwrap() {
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
