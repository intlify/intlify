// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The construction input this crate commits: every revision-`"0"` authority
//! and registry member below `conformance/`, each declared beside its bytes as
//! construction admits it.
//!
//! Tests construct from it, and so do the benchmark, which binds the
//! canonicalization data the resolver this input builds admits and prepares
//! its cases with that resolver's configuration schema, and the developer
//! example, which analyzes with that schema too.

use std::collections::BTreeMap;

use serde_json::json;

use super::authority_set::{self, SubmittedSet};
use super::capability;
use super::component::{self, Artifact, Submitted};
use super::flow::ConstructionInput;
use super::profile;
use super::registry::{self, RegistryRole, SubmittedPackage};
use crate::model::CONFIGURATION_SCHEMA_VERSION;

pub(crate) const PROFILE_SPECIFICATION_SCHEMA: &[u8] =
    include_bytes!("../../conformance/authorities/project-profile-specification-v0.schema.json");
pub(crate) const PROFILE_SPECIFICATION: &[u8] =
    include_bytes!("../../conformance/authorities/project-profile-specification-v0.json");
pub(crate) const AUTHORITY_SET: &[u8] =
    include_bytes!("../../conformance/authorities/configuration-schema-authority-set-v0.json");
pub(crate) const CONFIGURATION_SCHEMA: &[u8] =
    include_bytes!("../../conformance/authorities/configuration-v0.schema.json");
pub(crate) const CAPABILITY: &[u8] =
    include_bytes!("../../conformance/authorities/resolver-capability-v0.json");
pub(crate) const PACKAGE: &[u8] = include_bytes!("../../conformance/registries/package-v0.json");
/// The six registry members, in role order.
pub(crate) const REGISTRY_MEMBERS: [&[u8]; 6] = [
    include_bytes!("../../conformance/registries/safe-origin-kind-v0.schema.json"),
    include_bytes!("../../conformance/registries/check-v0.schema.json"),
    include_bytes!("../../conformance/registries/finding-v0.schema.json"),
    include_bytes!("../../conformance/registries/safe-origin-kind-v0.json"),
    include_bytes!("../../conformance/registries/check-v0.json"),
    include_bytes!("../../conformance/registries/finding-v0.json"),
];

/// A component that declares `identity`, `revision`, and the file digest of
/// `bytes`.
pub(crate) fn declared(identity: &str, revision: &str, bytes: &[u8]) -> Submitted {
    Submitted {
        tuple: json!({
            "identity": identity,
            "revision": revision,
            "digest": component::file_digest(bytes).as_str()
        }),
        artifact: Artifact::read(bytes.to_vec()),
    }
}

/// A Profile Specification that declares the identity and revision
/// construction admits, the file digest of `bytes`, and the semantic digest
/// of their value when they have one.
pub(crate) fn profile_specification(bytes: &[u8]) -> Submitted {
    let mut submitted = declared(profile::IDENTITY, profile::REVISION, bytes);
    let semantic =
        component::strict_json(bytes).and_then(|value| profile::semantic_digest(&value).ok());
    if let Some(semantic) = semantic {
        submitted.tuple["semanticDigest"] = json!(semantic.as_str());
    }
    submitted
}

/// A set that declares the identity and revision construction admits and
/// the file digest of `bytes`, and maps the committed schema.
pub(crate) fn authority_set(bytes: &[u8]) -> SubmittedSet {
    SubmittedSet {
        component: declared(authority_set::IDENTITY, authority_set::REVISION, bytes),
        members: BTreeMap::from([(
            CONFIGURATION_SCHEMA_VERSION.to_owned(),
            Artifact::read(CONFIGURATION_SCHEMA.to_vec()),
        )]),
    }
}

/// A package of this container, declared as construction admits it, and
/// these members, by role.
pub(crate) fn package(container: &[u8], members: [&[u8]; 6]) -> SubmittedPackage {
    SubmittedPackage {
        component: declared(registry::IDENTITY, registry::REVISION, container),
        members: RegistryRole::ALL
            .into_iter()
            .zip(members)
            .map(|(role, bytes)| (role, Artifact::read(bytes.to_vec())))
            .collect(),
    }
}

/// The committed input: every committed authority and member, each declared
/// as construction admits it.
pub(crate) fn input() -> ConstructionInput {
    ConstructionInput {
        profile_specification_schema: Some(declared(
            profile::SCHEMA_IDENTITY,
            profile::REVISION,
            PROFILE_SPECIFICATION_SCHEMA,
        )),
        profile_specification: Some(profile_specification(PROFILE_SPECIFICATION)),
        configuration_schema_authority_set: Some(authority_set(AUTHORITY_SET)),
        resolver_capability_specification: Some(declared(
            capability::IDENTITY,
            capability::REVISION,
            CAPABILITY,
        )),
        registry_package: Some(package(PACKAGE, REGISTRY_MEMBERS)),
    }
}
