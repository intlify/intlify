// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Normative Resolver Registry Package: six members in fixed roles, three
//! closed schemas and the three registries they admit.
//!
//! The package is a container that lists each member's declared identity,
//! revision, and file digest under its role, in role order; each member's
//! bytes are supplied beside it. Construction checks the container, rows 31
//! to 35 of the construction-failure precedence, with the container's
//! envelope schema and its six roles in order as part of the envelope row.
//! It then checks each member in role order, rows 36 to 40, all of one
//! member's rows before the next's: its bytes are present, it declares the
//! identity and a revision the Root Package admits for its role and the file
//! digest of its bytes, and it is strict UTF-8 JSON that is, for a schema,
//! a Draft 7 schema the meta-schema admits and the validator reads, and for
//! a registry, valid against the schema its package admitted for it. Row 41
//! then reads the registries and checks that they form the required closed
//! graph.
//!
//! The Safe Origin Kind, Check, and Finding Registries are written here
//! ([`safe_origin`], [`check`], [`finding`]). Row 41 reads each of them,
//! checks its own graph, and reads the Finding Registry against the Check
//! Registry, so that a code's checks are exactly the rows that list it. The
//! rows' occurrence Evidence and its references to the Safe Origin Kind
//! Registry come with the Evidence stages.
//!
//! The container is written here too ([`container`]): under each role, in
//! role order, the tuple of the member this crate writes for it (D15:2266).
//! The resolver's dispatch tables write every member, so the committed
//! bytes and the file digests compiled in keep the tables and the package
//! the same (D15:2279). Construction's last row compares the package it
//! admitted with those digests.

pub(crate) mod check;
pub(crate) mod finding;
pub(crate) mod occurrence;
pub(crate) mod policy;
pub(crate) mod safe_origin;

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use intlify_shared_json::decode;

use super::admission::{Admitted, ConstructionAdmission, RegistryMembers};
use super::component::{self, Artifact, Declared, Refusal, Rows, Submitted};
use super::failure::{Check, Component};
use super::root::ConstructionRoot;
use super::validator::schema::Schema;

use self::check::CheckRegistry;
use self::finding::FindingRegistry;
use self::safe_origin::SafeOriginKindRegistry;

/// The package's identity.
pub(crate) const IDENTITY: &str = "intlify-project-profile-resolver-registry-package";

/// The package's revision.
pub(crate) const REVISION: &str = "0";

/// The container's path below the conformance suite's root.
pub(crate) const PATH: &str = "registries/package-v0.json";

/// The container's file digest, which the resolver is compiled from.
pub(crate) const DIGEST: &str =
    "sha256:a3d658fd7d96bef223e3d407c50ec0de8d1ce4a8a36878d9dbf21d04cd2529d3";

/// A rank's spelling in a registry: a shortest decimal string.
pub(crate) const RANK_PATTERN: &str = "^(0|[1-9][0-9]*)$";

/// One of the package's six roles, declared in role order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RegistryRole {
    SafeOriginKindRegistrySchema,
    CheckRegistrySchema,
    FindingRegistrySchema,
    SafeOriginKindRegistry,
    CheckRegistry,
    FindingRegistry,
}

impl RegistryRole {
    /// Every role, in role order.
    pub(crate) const ALL: [Self; 6] = [
        Self::SafeOriginKindRegistrySchema,
        Self::CheckRegistrySchema,
        Self::FindingRegistrySchema,
        Self::SafeOriginKindRegistry,
        Self::CheckRegistry,
        Self::FindingRegistry,
    ];

    /// The role's name in the container.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::SafeOriginKindRegistrySchema => "safe-origin-kind-registry-schema",
            Self::CheckRegistrySchema => "check-registry-schema",
            Self::FindingRegistrySchema => "finding-registry-schema",
            Self::SafeOriginKindRegistry => "safe-origin-kind-registry",
            Self::CheckRegistry => "check-registry",
            Self::FindingRegistry => "finding-registry",
        }
    }

    /// The schema role a registry is checked against, or `None` for a schema.
    pub(crate) const fn schema(self) -> Option<Self> {
        match self {
            Self::SafeOriginKindRegistry => Some(Self::SafeOriginKindRegistrySchema),
            Self::CheckRegistry => Some(Self::CheckRegistrySchema),
            Self::FindingRegistry => Some(Self::FindingRegistrySchema),
            Self::SafeOriginKindRegistrySchema
            | Self::CheckRegistrySchema
            | Self::FindingRegistrySchema => None,
        }
    }

    /// The file digest of the member the resolver is compiled from for the
    /// role.
    pub(crate) const fn implemented(self) -> &'static str {
        match self {
            Self::SafeOriginKindRegistrySchema => safe_origin::SCHEMA_DIGEST,
            Self::CheckRegistrySchema => check::SCHEMA_DIGEST,
            Self::FindingRegistrySchema => finding::SCHEMA_DIGEST,
            Self::SafeOriginKindRegistry => safe_origin::DIGEST,
            Self::CheckRegistry => check::DIGEST,
            Self::FindingRegistry => finding::DIGEST,
        }
    }

    /// What the Root Package admits for the role.
    pub(crate) const fn admitted(self, members: &RegistryMembers) -> &Admitted {
        match self {
            Self::SafeOriginKindRegistrySchema => &members.safe_origin_kind_registry_schema,
            Self::CheckRegistrySchema => &members.check_registry_schema,
            Self::FindingRegistrySchema => &members.finding_registry_schema,
            Self::SafeOriginKindRegistry => &members.safe_origin_kind_registry,
            Self::CheckRegistry => &members.check_registry,
            Self::FindingRegistry => &members.finding_registry,
        }
    }
}

const ROWS: Rows = Rows::of(Component::RegistryPackage);
const MEMBER_PRESENCE_ROW: Refusal = Refusal::at(Component::RegistryMember, Check::Presence);
const MEMBER_IDENTITY_ROW: Refusal = Refusal::at(Component::RegistryMember, Check::Identity);
const MEMBER_REVISION_ROW: Refusal = Refusal::at(Component::RegistryMember, Check::Revision);
const MEMBER_DIGEST_ROW: Refusal = Refusal::at(Component::RegistryMember, Check::FileDigest);
const MEMBER_SCHEMA_ROW: Refusal = Refusal::at(Component::RegistryMember, Check::MemberSchema);
const CROSS_REFERENCES_ROW: Refusal =
    Refusal::at(Component::RegistryPackage, Check::CrossReferences);

/// The Registry Package as construction is given it: the container, and each
/// member's bytes by role.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SubmittedPackage {
    pub(crate) component: Submitted,
    pub(crate) members: BTreeMap<RegistryRole, Artifact>,
}

/// A Registry Package construction admitted.
#[derive(Debug)]
pub(crate) struct AdmittedPackage {
    /// What its tuple declares.
    pub(crate) declared: Declared,
    /// What its container declares for each member, in role order.
    pub(crate) members: [Declared; 6],
    /// The Safe Origin Kind Registry.
    pub(crate) safe_origin_kinds: SafeOriginKindRegistry,
    /// The Check Registry.
    pub(crate) checks: CheckRegistry,
    /// The Finding Registry, related to the Check Registry.
    pub(crate) findings: FindingRegistry,
}

/// Check the Registry Package construction is given: rows 31 to 41 of the
/// construction-failure precedence.
pub(crate) fn admit(
    root: &ConstructionRoot,
    submitted: Option<&SubmittedPackage>,
) -> Result<AdmittedPackage, Refusal> {
    let admission = &root.admission().components.registry_package;
    let checked = component::check(
        root,
        ROWS,
        admission,
        submitted.map(|package| &package.component),
        read_container,
    )?;
    let supplied = submitted.map(|package| &package.members);
    let members = checked.read;
    let mut schemas = BTreeMap::new();
    let mut registries = BTreeMap::new();
    for (role, declared) in RegistryRole::ALL.into_iter().zip(&members) {
        let artifact = supplied
            .and_then(|members| members.get(&role))
            .ok_or(MEMBER_PRESENCE_ROW)?;
        match admit_member(root, role, declared, artifact, &schemas)? {
            Member::Schema(schema) => {
                schemas.insert(role, schema);
            }
            Member::Registry(registry) => {
                registries.insert(role, registry);
            }
        }
    }
    // Every role was checked above, so each registry is there.
    let registry = |role: RegistryRole| registries[&role];
    let safe_origin_kinds =
        SafeOriginKindRegistry::read(registry(RegistryRole::SafeOriginKindRegistry))
            .ok_or(CROSS_REFERENCES_ROW)?;
    let checks =
        CheckRegistry::read(registry(RegistryRole::CheckRegistry)).ok_or(CROSS_REFERENCES_ROW)?;
    let findings = FindingRegistry::read(
        registry(RegistryRole::FindingRegistry),
        &checks,
        &safe_origin_kinds,
    )
    .ok_or(CROSS_REFERENCES_ROW)?;
    Ok(AdmittedPackage {
        declared: checked.declared,
        members,
        safe_origin_kinds,
        checks,
        findings,
    })
}

/// One admitted member: a schema, read by the validator, or a registry valid
/// against the schema its package admitted for it.
enum Member<'a> {
    Schema(Schema),
    Registry(&'a Value),
}

/// Check one member whose bytes are present: rows 37 to 40 of the
/// construction-failure precedence. `schemas` holds the schemas the package
/// admitted before it, by role.
fn admit_member<'a>(
    root: &ConstructionRoot,
    role: RegistryRole,
    declared: &Declared,
    artifact: &'a Artifact,
    schemas: &BTreeMap<RegistryRole, Schema>,
) -> Result<Member<'a>, Refusal> {
    let admitted = role.admitted(&root.admission().registry_members);
    if declared.identity != admitted.identity {
        return Err(MEMBER_IDENTITY_ROW);
    }
    if !admitted.revisions.contains(&declared.revision) {
        return Err(MEMBER_REVISION_ROW);
    }
    if component::file_digest(artifact.bytes()).as_str() != declared.digest {
        return Err(MEMBER_DIGEST_ROW);
    }
    match role.schema() {
        None => artifact
            .value()
            .filter(|document| component::declares_draft_7(document))
            .and_then(|document| component::draft_7_schema(root, document))
            .map(Member::Schema),
        Some(schema) => artifact
            .value()
            .filter(|registry| {
                schemas
                    .get(&schema)
                    .is_some_and(|schema| schema.validate(registry).valid)
            })
            .map(Member::Registry),
    }
    .ok_or(MEMBER_SCHEMA_ROW)
}

/// The package's container, given each member's bytes in role order: under
/// each role, in role order, the identity and revision the Root Package
/// admits for the role and the file digest of its member's bytes
/// (D15:2266).
pub(crate) fn container(members: [&[u8]; 6]) -> Value {
    let admission = ConstructionAdmission::revision_zero();
    let mut listed = Map::new();
    for (role, bytes) in RegistryRole::ALL.into_iter().zip(members) {
        let admitted = role.admitted(&admission.registry_members);
        listed.insert(
            role.name().to_owned(),
            json!({
                "identity": admitted.identity,
                "revision": admitted.revisions.first(),
                "digest": component::file_digest(bytes).as_str()
            }),
        );
    }
    json!({ "members": listed })
}

/// Read a container its envelope schema admitted: the six roles in role
/// order, each with its member's declared tuple.
fn read_container(value: &Value) -> Option<[Declared; 6]> {
    let members = value.get("members")?.as_object()?;
    if !members
        .keys()
        .map(String::as_str)
        .eq(RegistryRole::ALL.map(RegistryRole::name))
    {
        return None;
    }
    let tuples = members
        .values()
        .map(|tuple| decode::typed::<Declared>(tuple.clone()).ok())
        .collect::<Option<Vec<_>>>()?;
    tuples.try_into().ok()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::construction::admission::{ConstructionAdmission, EnvelopeSchema};
    use crate::construction::component::testing::{row, submitted};
    use crate::construction::envelope;

    use RegistryRole as R;

    const SAFE_ORIGIN_SCHEMA: &[u8] =
        include_bytes!("../../conformance/registries/safe-origin-kind-v0.schema.json");
    const SAFE_ORIGINS: &[u8] =
        include_bytes!("../../conformance/registries/safe-origin-kind-v0.json");
    const CHECK_SCHEMA: &[u8] = include_bytes!("../../conformance/registries/check-v0.schema.json");
    const CHECKS: &[u8] = include_bytes!("../../conformance/registries/check-v0.json");
    const FINDING_SCHEMA: &[u8] =
        include_bytes!("../../conformance/registries/finding-v0.schema.json");
    const FINDINGS: &[u8] = include_bytes!("../../conformance/registries/finding-v0.json");
    const PACKAGE: &[u8] = include_bytes!("../../conformance/registries/package-v0.json");
    const NO_ROWS: &[u8] = br#"{"rows": []}"#;
    const OPEN_SCHEMA: &[u8] = br#"{"$schema": "http://json-schema.org/draft-07/schema#"}"#;

    fn root() -> &'static ConstructionRoot {
        ConstructionRoot::built_in().unwrap()
    }

    /// Each member's bytes: the committed registries and their schemas.
    fn members() -> Vec<(RegistryRole, Vec<u8>)> {
        vec![
            (R::SafeOriginKindRegistrySchema, SAFE_ORIGIN_SCHEMA.to_vec()),
            (R::CheckRegistrySchema, CHECK_SCHEMA.to_vec()),
            (R::FindingRegistrySchema, FINDING_SCHEMA.to_vec()),
            (R::SafeOriginKindRegistry, SAFE_ORIGINS.to_vec()),
            (R::CheckRegistry, CHECKS.to_vec()),
            (R::FindingRegistry, FINDINGS.to_vec()),
        ]
    }

    /// The same members, with one role's bytes replaced.
    fn members_with(role: RegistryRole, bytes: &[u8]) -> Vec<(RegistryRole, Vec<u8>)> {
        let mut members = members();
        for member in &mut members {
            if member.0 == role {
                member.1 = bytes.to_vec();
            }
        }
        members
    }

    /// A container that declares each member as construction admits it, with
    /// the file digest of its bytes.
    fn container_of(members: &[(RegistryRole, Vec<u8>)]) -> Value {
        let mut listed = serde_json::Map::new();
        for (role, bytes) in members {
            listed.insert(
                role.name().to_owned(),
                json!({
                    "identity": format!("intlify-project-profile-{}", role.name()),
                    "revision": "0",
                    "digest": component::file_digest(bytes).as_str()
                }),
            );
        }
        json!({ "members": listed })
    }

    /// A package of this container, declared as itself, and these members.
    fn package_of(container: &Value, members: &[(RegistryRole, Vec<u8>)]) -> SubmittedPackage {
        SubmittedPackage {
            component: submitted(
                IDENTITY,
                REVISION,
                &serde_json::to_vec_pretty(container).unwrap(),
            ),
            members: members
                .iter()
                .map(|(role, bytes)| (*role, Artifact::read(bytes.clone())))
                .collect(),
        }
    }

    fn package(members: &[(RegistryRole, Vec<u8>)]) -> SubmittedPackage {
        package_of(&container_of(members), members)
    }

    fn checked(package: &SubmittedPackage) -> Option<usize> {
        row(admit(root(), Some(package)))
    }

    #[test]
    fn the_roles_are_015s_six_in_their_fixed_order() {
        assert_eq!(
            RegistryRole::ALL.map(RegistryRole::name),
            [
                "safe-origin-kind-registry-schema",
                "check-registry-schema",
                "finding-registry-schema",
                "safe-origin-kind-registry",
                "check-registry",
                "finding-registry",
            ]
        );
        let mut sorted = RegistryRole::ALL;
        sorted.sort();
        assert_eq!(sorted, RegistryRole::ALL);
        // The Root Package names them alike, in the same order: the admission
        // body, and the container's envelope schema.
        let admission = serde_json::to_value(ConstructionAdmission::revision_zero()).unwrap();
        assert_eq!(
            admission["registryMembers"]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            RegistryRole::ALL.map(RegistryRole::name)
        );
        assert_eq!(
            envelope::schema(EnvelopeSchema::RegistryPackage)["properties"]["members"]["required"],
            json!(RegistryRole::ALL.map(RegistryRole::name))
        );
        // Each role is admitted under its own identity.
        let members = &ConstructionAdmission::revision_zero().registry_members;
        for role in RegistryRole::ALL {
            assert_eq!(
                role.admitted(members).identity,
                format!("intlify-project-profile-{}", role.name())
            );
        }
    }

    #[test]
    fn a_registry_is_checked_against_its_own_schema_and_a_schema_against_none() {
        assert_eq!(
            RegistryRole::ALL.map(RegistryRole::schema),
            [
                None,
                None,
                None,
                Some(R::SafeOriginKindRegistrySchema),
                Some(R::CheckRegistrySchema),
                Some(R::FindingRegistrySchema),
            ]
        );
    }

    #[test]
    fn construction_admits_a_package_with_the_committed_registries() {
        let admitted = admit(root(), Some(&package(&members()))).unwrap();
        assert_eq!(admitted.declared.identity, IDENTITY);
        assert_eq!(
            admitted.safe_origin_kinds,
            SafeOriginKindRegistry::revision_zero()
        );
        assert_eq!(admitted.checks, CheckRegistry::revision_zero());
        assert_eq!(admitted.findings, FindingRegistry::revision_zero());
    }

    /// The committed container lists each member's tuple, as construction
    /// admits it, in role order (D15:2266).
    #[test]
    fn the_container_lists_each_members_tuple_in_role_order() {
        let members = members();
        let bytes: Vec<&[u8]> = members.iter().map(|(_, bytes)| bytes.as_slice()).collect();
        let written = container(bytes.try_into().unwrap());
        let committed = serde_json::from_slice::<Value>(PACKAGE).unwrap();
        assert_eq!(written, container_of(&members));
        assert_eq!(committed, written);
        // A map's equality ignores its order, so the roles' is checked apart.
        let roles = |container: &Value| -> Vec<String> {
            container["members"]
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect()
        };
        assert_eq!(roles(&written), RegistryRole::ALL.map(RegistryRole::name));
        assert_eq!(roles(&committed), RegistryRole::ALL.map(RegistryRole::name));
    }

    #[test]
    fn construction_admits_the_committed_package() {
        let package = SubmittedPackage {
            component: submitted(IDENTITY, REVISION, PACKAGE),
            members: members()
                .into_iter()
                .map(|(role, bytes)| (role, Artifact::read(bytes)))
                .collect(),
        };
        let admitted = admit(root(), Some(&package)).unwrap();
        assert_eq!(admitted.declared.digest, DIGEST);
        assert_eq!(admitted.findings, FindingRegistry::revision_zero());
    }

    /// Each role's pin is the file digest of the member this crate commits
    /// for it, and construction keeps what the container declares for each
    /// member, for the last row to compare.
    #[test]
    fn each_role_implements_its_committed_member() {
        let members = members();
        for (role, bytes) in &members {
            assert_eq!(
                role.implemented(),
                component::file_digest(bytes).as_str(),
                "{role:?}"
            );
        }
        let admitted = admit(root(), Some(&package(&members))).unwrap();
        for (role, member) in RegistryRole::ALL.into_iter().zip(&admitted.members) {
            assert_eq!(
                member,
                &Declared {
                    identity: format!("intlify-project-profile-{}", role.name()),
                    revision: REVISION.to_owned(),
                    digest: role.implemented().to_owned(),
                    semantic_digest: None,
                }
            );
        }
    }

    #[test]
    fn each_package_check_refuses_with_its_row() {
        let members = members();
        let valid = package(&members);
        assert_eq!(checked(&valid), None);
        assert_eq!(row(admit(root(), None)), Some(31));
        let container = container_of(&members);
        let listed = container["members"].as_object().unwrap();
        let mut reversed = serde_json::Map::new();
        for role in RegistryRole::ALL.into_iter().rev() {
            reversed.insert(role.name().to_owned(), listed[role.name()].clone());
        }
        let mut missing = container.clone();
        missing["members"]
            .as_object_mut()
            .unwrap()
            .remove("finding-registry");
        let mut extra_tuple_member = valid.clone();
        extra_tuple_member.component.tuple["semanticDigest"] =
            extra_tuple_member.component.tuple["digest"].clone();
        let bytes = serde_json::to_vec_pretty(&container).unwrap();
        let with_component = |component: Submitted| SubmittedPackage {
            component,
            ..valid.clone()
        };
        let mut compact = valid.component.clone();
        compact.artifact = Artifact::read(serde_json::to_vec(&container).unwrap());
        for (case, expected) in [
            (extra_tuple_member, 32),
            (package_of(&json!({ "members": reversed }), &members), 32),
            (package_of(&missing, &members), 32),
            (
                with_component(submitted(IDENTITY, REVISION, b"{\"members\": {}")),
                32,
            ),
            (
                with_component(submitted("intlify-project-profile", REVISION, &bytes)),
                33,
            ),
            (with_component(submitted(IDENTITY, "1", &bytes)), 34),
            (with_component(compact), 35),
        ] {
            assert_eq!(checked(&case), Some(expected), "{case:?}");
        }
    }

    #[test]
    fn each_member_check_refuses_with_its_row() {
        let members = members();
        let container = container_of(&members);
        let mut absent = package(&members);
        absent.members.remove(&R::CheckRegistry);
        let declared = |member: &str, field: &str, value: &str| {
            let mut container = container.clone();
            container["members"][member][field] = json!(value);
            package_of(&container, &members)
        };
        let mut other_bytes = package(&members);
        other_bytes.members.insert(
            R::FindingRegistry,
            Artifact::read(b"{\"rows\":[]}".to_vec()),
        );
        for (case, expected) in [
            (absent, 36),
            (
                declared(
                    "check-registry-schema",
                    "identity",
                    "intlify-project-profile-check-registry",
                ),
                37,
            ),
            (declared("check-registry-schema", "revision", "1"), 38),
            (other_bytes, 39),
        ] {
            assert_eq!(checked(&case), Some(expected), "{case:?}");
        }
        // A schema member that is not strict UTF-8 JSON, does not declare
        // Draft 7, fails the meta-schema, which the validator would read, or
        // is not read by the validator, and a registry that is not strict
        // UTF-8 JSON or fails its admitted schema, each declared with its own
        // digest.
        for (role, bytes) in [
            (R::CheckRegistrySchema, b"{".as_slice()),
            (R::CheckRegistrySchema, br#"{"type": "object"}"#),
            (
                R::CheckRegistrySchema,
                br#"{"$schema": "http://json-schema.org/draft-07/schema#", "title": 1}"#,
            ),
            (
                R::CheckRegistrySchema,
                br#"{"$schema": "http://json-schema.org/draft-07/schema#", "type": "integral"}"#,
            ),
            (
                R::CheckRegistrySchema,
                br#"{"$schema": "http://json-schema.org/draft-07/schema#", "$ref": "other.json"}"#,
            ),
            (R::CheckRegistry, b"{\"rows\": [],}"),
            (R::CheckRegistry, br#"{"rows": {}}"#),
            (R::SafeOriginKindRegistry, NO_ROWS),
        ] {
            assert_eq!(
                checked(&package(&members_with(role, bytes))),
                Some(40),
                "{role:?} {}",
                String::from_utf8_lossy(bytes)
            );
        }
    }

    #[test]
    fn each_member_passes_all_its_rows_before_the_next() {
        // The first member fails its last row while the second fails its
        // first: the first member's row is reported.
        let first_fails = members_with(R::SafeOriginKindRegistrySchema, br#"{"type": "object"}"#);
        let mut container = container_of(&first_fails);
        container["members"]["check-registry-schema"]["identity"] = json!("another");
        assert_eq!(checked(&package_of(&container, &first_fails)), Some(40));
        // With the first fixed, the second member's row is.
        let members = members();
        let mut container = container_of(&members);
        container["members"]["check-registry-schema"]["identity"] = json!("another");
        assert_eq!(checked(&package_of(&container, &members)), Some(37));
    }

    #[test]
    fn a_registry_is_checked_against_the_schema_its_package_admitted() {
        // A stricter schema refuses the committed registry.
        let strict = br#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "properties": { "rows": { "maxItems": 3 } }
        }"#;
        assert_eq!(
            checked(&package(&members_with(
                R::SafeOriginKindRegistrySchema,
                strict
            ))),
            Some(40)
        );
        // A schema that admits anything passes a registry construction cannot
        // read, which the graph's row then refuses.
        let mut open = members_with(R::SafeOriginKindRegistrySchema, OPEN_SCHEMA);
        open[3].1 = br#"{"rows": "none"}"#.to_vec();
        assert_eq!(checked(&package(&open)), Some(41));
        // A registry is checked against its own role's schema, not another's.
        let mut other_open = members_with(R::CheckRegistrySchema, OPEN_SCHEMA);
        other_open[3].1 = br#"{"rows": "none"}"#.to_vec();
        assert_eq!(checked(&package(&other_open)), Some(40));
    }

    #[test]
    fn the_safe_origin_kind_registry_must_not_repeat_an_id_or_a_rank() {
        let committed: Value = serde_json::from_slice(SAFE_ORIGINS).unwrap();
        for (field, value) in [("id", "configuration-file"), ("rank", "0")] {
            let mut registry = committed.clone();
            registry["rows"][1][field] = json!(value);
            let bytes = serde_json::to_vec_pretty(&registry).unwrap();
            // The schema admits it; the graph does not.
            assert_eq!(
                checked(&package(&members_with(R::SafeOriginKindRegistry, &bytes))),
                Some(41),
                "{field}"
            );
        }
    }

    #[test]
    fn the_check_registry_must_form_a_closed_graph() {
        let committed: Value = serde_json::from_slice(CHECKS).unwrap();
        // A repeated check ID, a relation that scopes itself, two checks that
        // wait on each other, and a dependency record with two owners.
        let mut repeated = committed.clone();
        repeated["rows"][1]["checkId"] = committed["rows"][0]["checkId"].clone();
        repeated["rows"][1]["ruleId"] = committed["rows"][0]["ruleId"].clone();
        let mut circular = committed.clone();
        let mut waiting = committed.clone();
        let mut shared = committed;
        for row in circular["rows"].as_array_mut().unwrap() {
            if row["subjectKind"] == "relation" {
                row["parentSubject"] =
                    json!({ "phaseId": "hydration-validation", "subjectKind": "relation" });
            }
        }
        for row in waiting["rows"].as_array_mut().unwrap() {
            if row["checkId"] == "hydration.relation.unique" {
                row["prerequisiteCheckIds"] = json!(["hydration.relation.distinct-endpoints"]);
            } else if row["checkId"] == "hydration.relation.distinct-endpoints" {
                row["prerequisiteCheckIds"] = json!(["hydration.relation.unique"]);
            }
        }
        for row in shared["rows"].as_array_mut().unwrap() {
            if row["checkId"] == "dependency.targets.established" {
                row["ownedDependencies"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({ "recordKind": "semantic-dependency", "id": "coverage" }));
            }
        }
        for (case, registry) in [
            ("repeated", repeated),
            ("circular", circular),
            ("waiting on each other", waiting),
            ("owned twice", shared),
        ] {
            let bytes = serde_json::to_vec_pretty(&registry).unwrap();
            // The schema admits it; the graph does not.
            assert_eq!(
                checked(&package(&members_with(R::CheckRegistry, &bytes))),
                Some(41),
                "{case}"
            );
        }
    }

    #[test]
    fn the_finding_registry_must_relate_to_the_check_registry() {
        let findings: Value = serde_json::from_slice(FINDINGS).unwrap();
        let checks: Value = serde_json::from_slice(CHECKS).unwrap();
        // A code whose checks leave one of the rows that list it out.
        let mut short = findings;
        for row in short["rows"].as_array_mut().unwrap() {
            if row["code"] == "project-profile-hydration-relation-invalid" {
                row["allowedCheckIds"].as_array_mut().unwrap().pop();
            }
        }
        let short = serde_json::to_vec_pretty(&short).unwrap();
        // A check that no longer lists a code the Finding Registry gives it,
        // a Check Registry closed by itself.
        let mut dropped = checks;
        for row in dropped["rows"].as_array_mut().unwrap() {
            if row["checkId"] == "hydration.target.single-role" {
                row["possibleFindingCodes"] = json!([]);
                row["possibleEvidenceRecordKinds"] = json!(["shared-evaluation-status"]);
                row["findingPayloadStrategy"]["details"] = json!([]);
            }
        }
        let dropped = serde_json::to_vec_pretty(&dropped).unwrap();
        for (case, members) in [
            ("a check left out", members_with(R::FindingRegistry, &short)),
            (
                "a check that lists nothing",
                members_with(R::CheckRegistry, &dropped),
            ),
        ] {
            // The schemas admit both; the cross-references do not.
            assert_eq!(checked(&package(&members)), Some(41), "{case}");
        }
    }

    #[test]
    fn the_finding_registry_rows_occurrence_evidence_must_be_015s() {
        let committed: Value = serde_json::from_slice(FINDINGS).unwrap();
        let with = |code: &str, change: &dyn Fn(&mut Value)| {
            let mut registry = committed.clone();
            for row in registry["rows"].as_array_mut().unwrap() {
                if row["code"] == code {
                    change(row);
                }
            }
            serde_json::to_vec_pretty(&registry).unwrap()
        };
        // A selector Finding that relates no declaration though its row still
        // names their policy, and the one row that carries the pins without
        // them, closed by itself, so that the policy pins a shape no Evidence
        // carries.
        let unrelated = with("project-profile-selector-unknown", &|row| {
            row["occurrenceRelatedEvidenceKindsByCheck"][0]["kinds"] = json!([]);
        });
        let unreachable = with("project-profile-resource-policy-not-admitted", &|row| {
            row["occurrenceRelatedEvidenceKindsByCheck"][0]["kinds"] =
                json!(["configuration-value"]);
            for column in [
                "evidenceDisclosurePolicies",
                "allowedSafeOriginKindsByEvidenceKind",
            ] {
                row[column]
                    .as_array_mut()
                    .unwrap()
                    .retain(|entry| entry["evidenceKind"] != "verification-input");
            }
        });
        for (case, bytes) in [
            ("a related kind left out", unrelated),
            ("a shape no Evidence carries", unreachable),
        ] {
            // The schema admits both; the cross-references do not.
            assert_eq!(
                checked(&package(&members_with(R::FindingRegistry, &bytes))),
                Some(41),
                "{case}"
            );
        }
    }

    #[test]
    fn the_finding_registry_policies_must_be_015s_under_the_admitted_origins() {
        // A selector's redacted value disclosed literally.
        let mut literal: Value = serde_json::from_slice(FINDINGS).unwrap();
        for policy in literal["policies"].as_array_mut().unwrap() {
            for component in policy["components"].as_array_mut().unwrap() {
                if component["component"] == "selector.redacted-value" {
                    component["disclosure"] = json!("non-secret-literal");
                    component.as_object_mut().unwrap().remove("tokenProjection");
                }
            }
        }
        let literal = serde_json::to_vec_pretty(&literal).unwrap();
        // A Safe Origin Kind Registry under which the vocabulary no longer
        // supplies the artifact input the policies point into.
        let mut narrowed: Value = serde_json::from_slice(SAFE_ORIGINS).unwrap();
        for row in narrowed["rows"].as_array_mut().unwrap() {
            if row["id"] == "intent-surface-class-vocabulary" {
                row["permittedEvidenceKinds"] = json!(["specification-input"]);
            }
        }
        let narrowed = serde_json::to_vec_pretty(&narrowed).unwrap();
        for (case, members) in [
            (
                "a redacted value disclosed literally",
                members_with(R::FindingRegistry, &literal),
            ),
            (
                "an origin the kind may not use",
                members_with(R::SafeOriginKindRegistry, &narrowed),
            ),
        ] {
            // The schemas admit both; the cross-references do not.
            assert_eq!(checked(&package(&members)), Some(41), "{case}");
        }
    }
}
