// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The conformance runner's entry: open the suite at the Suite Index it is
//! handed.
//!
//! The suite root is the directory that holds the index (D15:2858). Opening
//! reads the Suite Index Schema and what the index names, and nothing else,
//! and never through a symbolic link below the suite root, which could reach
//! a file outside it. It checks in this order, and the first failure stops it:
//!
//! 1. the trust root: the Suite Index Schema at its fixed path below the
//!    suite root has the file digest the runner pins (D15:3029);
//! 2. the index is strict UTF-8 JSON and valid against that schema;
//! 3. the index is a revision-`"0"` index (D15:3031): each member pins the
//!    identity, revision, and digests of the artifact this crate is compiled
//!    from, and the Root Package's and the Registry Package's roles come in
//!    their fixed order. Where a body lies below the suite root is the
//!    index's to say, so no path is compared but the schema's, which is
//!    fixed; and the Profile Specification is pinned by its semantic digest,
//!    so its file digest is not compared either;
//! 4. that every manifest is listed once: each array lists its IDs in
//!    ascending unsigned byte order, and no ID occurs in two arrays
//!    (D15:3031);
//! 5. the closure: each body the index names has the file digest the index
//!    declares for it, the Profile Specification's body has the semantic
//!    digest the index declares, and each manifest is strict UTF-8 JSON, valid
//!    against the Case Schema, with the `id` that lists it and the
//!    `manifestKind` of its array.
//!
//! An artifact whose file digest is the one this crate pins has fixed
//! contents, so the Root Package's member list, the Configuration Schema
//! Authority Set's Profile Specification, and the Registry Package's container
//! need no reading of their own.

use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

use serde_json::{json, Map, Value};

use crate::construction::admission::{ComponentAdmission, ConstructionAdmission};
use crate::construction::component::{file_digest, strict_json};
use crate::construction::registry::{self, RegistryRole};
use crate::construction::root::{self, RootPackage, RootRole};
use crate::construction::validator::schema::Schema;
use crate::construction::{authority_set, capability, profile};
use crate::model::CONFIGURATION_SCHEMA_VERSION;

use super::case::{self, ManifestKind};
use super::index::{
    Member, SCHEMA_DIGEST, SCHEMA_IDENTITY, SCHEMA_PATH, SCHEMA_REVISION, SUITE_REVISION,
};

/// Why a suite does not open: a Conformance harness failure (D15:2858).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HarnessFailure {
    /// No file is at the path, or it cannot be read.
    Missing(String),
    /// The path passes through a symbolic link below the suite root, so the
    /// file it reaches may lie outside the suite.
    SymbolicLink(String),
    /// The file's bytes do not have the digest the index declares for them,
    /// or, for the Suite Index Schema, the one the runner pins.
    DigestMismatch(String),
    /// The file is not strict UTF-8 JSON.
    Malformed(String),
    /// The index is not valid against the Suite Index Schema, or a manifest
    /// against the Case Schema: it lacks a member, leaving a body it must list
    /// unlisted, holds one too many, or spells a value outside its form.
    SchemaInvalid(String),
    /// The member is not revision `"0"`'s: it pins another artifact, its
    /// roles are out of order, the Profile Specification's body does not have
    /// the semantic digest it declares, or a manifest it lists is out of
    /// order, listed twice, or has another `id` or `manifestKind`.
    NotRevisionZero(Member),
}

impl fmt::Display for HarnessFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(path) => write!(formatter, "{path} is missing or unreadable"),
            Self::SymbolicLink(path) => {
                write!(formatter, "{path} passes through a symbolic link")
            }
            Self::DigestMismatch(path) => {
                write!(formatter, "{path} does not have its declared digest")
            }
            Self::Malformed(path) => write!(formatter, "{path} is not strict UTF-8 JSON"),
            Self::SchemaInvalid(path) => {
                write!(formatter, "{path} is not valid against its schema")
            }
            Self::NotRevisionZero(member) => write!(
                formatter,
                "the index's {} is not revision \"0\"'s",
                member.name()
            ),
        }
    }
}

/// A suite that opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    /// The suite-root-relative path and file digest of each body the runner
    /// verified, in the order it verified them: the Suite Index Schema first,
    /// then each member's bodies in the order of the index's members.
    pub verified: Vec<(String, String)>,
}

/// Open the suite whose Suite Index is at `index`.
///
/// # Errors
///
/// Returns the first harness failure.
pub fn open(index: &Path) -> Result<Opened, HarnessFailure> {
    let Some(root) = index.parent() else {
        return Err(HarnessFailure::Missing(index.display().to_string()));
    };
    let schema = trust_root(root)?;
    let value = read_index(index, &schema)?;
    let pinned = Member::ALL
        .into_iter()
        .filter(|member| member.manifests().is_none());
    for member in pinned {
        revision_zero(member, &value[member.name()])?;
    }
    listed_once(&value)?;
    let mut closure = Closure {
        root,
        verified: vec![(SCHEMA_PATH.to_owned(), SCHEMA_DIGEST.to_owned())],
        case_schema: None,
    };
    for member in Member::ALL {
        closure.verify(member, &value[member.name()])?;
    }
    Ok(Opened {
        verified: closure.verified,
    })
}

/// Read a body below the suite root, refusing a path that passes through a
/// symbolic link: the suite is the files below its root, and a link could
/// reach one outside it with the bytes the index declares. A segment that is
/// absent is left for the read to report.
fn read(root: &Path, path: &str) -> Result<Vec<u8>, HarnessFailure> {
    let mut file = root.to_path_buf();
    for segment in path.split('/') {
        file.push(segment);
        let link = std::fs::symlink_metadata(&file)
            .is_ok_and(|metadata| metadata.file_type().is_symlink());
        if link {
            return Err(HarnessFailure::SymbolicLink(path.to_owned()));
        }
    }
    std::fs::read(&file).map_err(|_| HarnessFailure::Missing(path.to_owned()))
}

/// Read the Suite Index Schema from its fixed path, as the one the runner
/// pins.
fn trust_root(root: &Path) -> Result<Schema, HarnessFailure> {
    let bytes = read(root, SCHEMA_PATH)?;
    if file_digest(&bytes).as_str() != SCHEMA_DIGEST {
        return Err(HarnessFailure::DigestMismatch(SCHEMA_PATH.to_owned()));
    }
    Ok(compile(&bytes))
}

/// Read a schema whose bytes are the ones the runner pins.
fn compile(bytes: &[u8]) -> Schema {
    let document = strict_json(bytes).expect("a pinned schema is strict JSON");
    Schema::compile(&document).expect("a pinned schema compiles")
}

/// Read the index, valid against the schema.
fn read_index(index: &Path, schema: &Schema) -> Result<Value, HarnessFailure> {
    let name = || index.display().to_string();
    let bytes = std::fs::read(index).map_err(|_| HarnessFailure::Missing(name()))?;
    let value = strict_json(&bytes).ok_or_else(|| HarnessFailure::Malformed(name()))?;
    if schema.validate(&value).valid {
        Ok(value)
    } else {
        Err(HarnessFailure::SchemaInvalid(name()))
    }
}

/// Check that a member pins the artifact this crate is compiled from, with
/// its roles in their order.
fn revision_zero(member: Member, value: &Value) -> Result<(), HarnessFailure> {
    let mut pinned = value.clone();
    if member != Member::SuiteIndexSchema {
        without_paths(&mut pinned);
    }
    if member == Member::ProfileSpecification {
        pinned
            .as_object_mut()
            .expect("the schema makes an entry an object")
            .remove("digest");
    }
    let in_order = match member {
        Member::ResolverConstructionRootPackage => {
            in_role_order(value, &RootRole::ALL.map(RootRole::name))
        }
        Member::RegistryPackage => in_role_order(value, &RegistryRole::ALL.map(RegistryRole::name)),
        _ => true,
    };
    if pinned == canonical(member) && in_order {
        Ok(())
    } else {
        Err(HarnessFailure::NotRevisionZero(member))
    }
}

/// Check that every manifest ID is listed once: each array in ascending
/// unsigned byte order, and no ID in two arrays.
fn listed_once(value: &Value) -> Result<(), HarnessFailure> {
    let mut seen = BTreeSet::new();
    let arrays = Member::ALL
        .into_iter()
        .filter(|member| member.manifests().is_some());
    for member in arrays {
        let mut previous = None;
        for entry in entries(&value[member.name()]) {
            let id = text(&entry["id"]);
            if previous.is_some_and(|previous| previous >= id) || !seen.insert(id) {
                return Err(HarnessFailure::NotRevisionZero(member));
            }
            previous = Some(id);
        }
    }
    Ok(())
}

/// The entries a member names: each item of an array, or the member and each
/// of its roles or mapped schemas.
fn entries(value: &Value) -> Vec<&Value> {
    if let Value::Array(items) = value {
        return items.iter().collect();
    }
    let mut entries = vec![value];
    if let Some(members) = value.get("members").and_then(Value::as_object) {
        entries.extend(members.values());
    }
    entries
}

/// Remove every `path` member, at any depth.
fn without_paths(value: &mut Value) {
    if let Value::Object(members) = value {
        members.remove("path");
        members.values_mut().for_each(without_paths);
    }
}

/// Whether an entry's roles come in their order.
fn in_role_order(entry: &Value, roles: &[&str]) -> bool {
    entry["members"]
        .as_object()
        .expect("the schema makes the roles an object")
        .keys()
        .map(String::as_str)
        .eq(roles.iter().copied())
}

/// What a revision-`"0"` index pins for a member, without the paths but the
/// schema's, and without the Profile Specification's file digest.
fn canonical(member: Member) -> Value {
    let admission = ConstructionAdmission::revision_zero();
    let components = &admission.components;
    let pinned = |admitted: &ComponentAdmission, digest: &str| {
        json!({
            "identity": admitted.identity,
            "revision": admitted.revisions.first(),
            "digest": digest,
        })
    };
    let profile_specification = json!({
        "identity": components.profile_specification.identity,
        "revision": components.profile_specification.revisions.first(),
        "semanticDigest": profile::SEMANTIC_DIGEST,
    });
    match member {
        Member::SuiteRevision => json!(SUITE_REVISION),
        Member::SuiteIndexSchema => json!({
            "identity": SCHEMA_IDENTITY,
            "revision": SCHEMA_REVISION,
            "path": SCHEMA_PATH,
            "digest": SCHEMA_DIGEST,
        }),
        Member::ResolverConstructionRootPackage => {
            let package = RootPackage::built_in()
                .expect("the built-in Root Package is the one this crate pins");
            let members = RootRole::ALL
                .into_iter()
                .map(|role| {
                    (
                        role.name().to_owned(),
                        json!({ "digest": file_digest(package.member(role)).as_str() }),
                    )
                })
                .collect::<Map<_, _>>();
            json!({
                "identity": root::IDENTITY,
                "revision": root::REVISION,
                "digest": root::DIGEST,
                "canonicalDigest": root::CANONICAL_DIGEST,
                "members": members,
            })
        }
        Member::ProfileSpecificationSchema => pinned(
            &components.profile_specification_schema,
            profile::SCHEMA_DIGEST,
        ),
        Member::ProfileSpecification => profile_specification,
        Member::ConfigurationSchemaAuthoritySet => {
            let schemas = &admission.configuration_schemas;
            let mut set = pinned(
                &components.configuration_schema_authority_set,
                authority_set::DIGEST,
            );
            set["profileSpecification"] = profile_specification;
            set["members"] = json!({
                CONFIGURATION_SCHEMA_VERSION: {
                    "identity": schemas.identity,
                    "revision": schemas.revisions.first(),
                    "dialect": schemas.dialects.first(),
                    "digest": authority_set::MEMBER_DIGEST,
                }
            });
            set
        }
        Member::ResolverCapabilitySpecification => pinned(
            &components.resolver_capability_specification,
            capability::DIGEST,
        ),
        Member::CaseSchema => json!({
            "identity": case::IDENTITY,
            "revision": case::REVISION,
            "digest": case::DIGEST,
        }),
        Member::RegistryPackage => {
            let members = RegistryRole::ALL
                .into_iter()
                .map(|role| {
                    let admitted = role.admitted(&admission.registry_members);
                    (
                        role.name().to_owned(),
                        json!({
                            "identity": admitted.identity,
                            "revision": admitted.revisions.first(),
                            "digest": role.implemented(),
                        }),
                    )
                })
                .collect::<Map<_, _>>();
            let mut package = pinned(&components.registry_package, registry::DIGEST);
            package["members"] = Value::Object(members);
            package
        }
        Member::ResolverConstructionCases
        | Member::ResolverInputAdmissionCases
        | Member::ResolverCases
        | Member::HandoffVectors
        | Member::ConditionalObligations => {
            unreachable!("a manifest array pins no artifact of this crate")
        }
    }
}

/// The closure as the runner verifies it: the bodies verified so far, and the
/// Case Schema once its body is.
struct Closure<'a> {
    root: &'a Path,
    verified: Vec<(String, String)>,
    case_schema: Option<Schema>,
}

impl Closure<'_> {
    /// Verify each body a member names: its file digest, the Profile
    /// Specification's semantic digest, and each manifest.
    fn verify(&mut self, member: Member, value: &Value) -> Result<(), HarnessFailure> {
        if matches!(member, Member::SuiteRevision | Member::SuiteIndexSchema) {
            return Ok(());
        }
        for entry in entries(value) {
            let path = text(&entry["path"]);
            let digest = text(&entry["digest"]);
            let bytes = read(self.root, path)?;
            if file_digest(&bytes).as_str() != digest {
                return Err(HarnessFailure::DigestMismatch(path.to_owned()));
            }
            if member == Member::ProfileSpecification {
                semantic(path, &bytes, text(&entry["semanticDigest"]))?;
            } else if member == Member::CaseSchema {
                self.case_schema = Some(compile(&bytes));
            } else if let Some(kind) = member.manifests() {
                self.manifest(member, kind, entry, &bytes)?;
            }
            self.verified.push((path.to_owned(), digest.to_owned()));
        }
        Ok(())
    }

    /// Check a manifest: strict UTF-8 JSON, valid against the Case Schema,
    /// with the `id` of the entry that lists it and the kind of its array.
    fn manifest(
        &self,
        member: Member,
        kind: ManifestKind,
        entry: &Value,
        bytes: &[u8],
    ) -> Result<(), HarnessFailure> {
        let path = text(&entry["path"]);
        let manifest =
            strict_json(bytes).ok_or_else(|| HarnessFailure::Malformed(path.to_owned()))?;
        let schema = self
            .case_schema
            .as_ref()
            .expect("the Case Schema is verified before the manifests");
        if !schema.validate(&manifest).valid {
            return Err(HarnessFailure::SchemaInvalid(path.to_owned()));
        }
        if manifest["id"] != entry["id"] || manifest["manifestKind"] != kind.name() {
            return Err(HarnessFailure::NotRevisionZero(member));
        }
        Ok(())
    }
}

/// A member the schema makes a string.
fn text(value: &Value) -> &str {
    value
        .as_str()
        .expect("the Suite Index Schema makes it a string")
}

/// Check that the Profile Specification's body has the semantic digest the
/// index declares.
fn semantic(path: &str, bytes: &[u8], declared: &str) -> Result<(), HarnessFailure> {
    let body = strict_json(bytes).ok_or_else(|| HarnessFailure::Malformed(path.to_owned()))?;
    if profile::semantic_digest(&body).is_ok_and(|digest| digest.as_str() == declared) {
        Ok(())
    } else {
        Err(HarnessFailure::NotRevisionZero(
            Member::ProfileSpecification,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::conformance::index::PATH;
    use crate::construction::generate;

    /// Every body the committed suite holds, in the order the runner verifies
    /// them.
    const BODIES: [&str; 20] = [
        "schemas/suite-index-v0.schema.json",
        "construction/root-package-v0.json",
        "construction/construction-admission-v0.json",
        "construction/construction-admission-v0.schema.json",
        "construction/authority-envelope-schemas-v0.json",
        "construction/json-schema-meta-schemas-v0.json",
        "construction/cross-authority-rules-v0.json",
        "authorities/project-profile-specification-v0.schema.json",
        "authorities/project-profile-specification-v0.json",
        "authorities/configuration-schema-authority-set-v0.json",
        "authorities/configuration-v0.schema.json",
        "authorities/resolver-capability-v0.json",
        "schemas/case-v0.schema.json",
        "registries/package-v0.json",
        "registries/safe-origin-kind-v0.schema.json",
        "registries/check-v0.schema.json",
        "registries/finding-v0.schema.json",
        "registries/safe-origin-kind-v0.json",
        "registries/check-v0.json",
        "registries/finding-v0.json",
    ];

    /// The committed suite's root.
    fn committed() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance")
    }

    /// Copy a directory's files and subdirectories into another.
    fn copy_tree(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), &target).unwrap();
            }
        }
    }

    /// A copy of the committed suite in a directory of its own.
    struct Suite {
        dir: tempfile::TempDir,
    }

    impl Suite {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            copy_tree(&committed(), dir.path());
            Self { dir }
        }

        fn root(&self) -> &Path {
            self.dir.path()
        }

        fn index_path(&self) -> PathBuf {
            self.root().join(PATH)
        }

        fn index_name(&self) -> String {
            self.index_path().display().to_string()
        }

        fn open(&self) -> Result<Opened, HarnessFailure> {
            open(&self.index_path())
        }

        fn read(&self, path: &str) -> Vec<u8> {
            std::fs::read(self.root().join(path)).unwrap()
        }

        fn write(&self, path: &str, bytes: &[u8]) {
            let target = self.root().join(path);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(target, bytes).unwrap();
        }

        fn remove(&self, path: &str) {
            std::fs::remove_file(self.root().join(path)).unwrap();
        }

        fn change_index(&self, change: impl FnOnce(&mut Value)) {
            let mut index: Value = serde_json::from_slice(&self.read(PATH)).unwrap();
            change(&mut index);
            self.write(PATH, &generate::write(&index).unwrap());
        }

        /// Replace a body, and declare its new file digest at `pointer`.
        fn replace(&self, path: &str, bytes: &[u8], pointer: &str) {
            self.write(path, bytes);
            self.change_index(|index| {
                *index.pointer_mut(pointer).unwrap() = json!(file_digest(bytes).as_str());
            });
        }
    }

    fn other_digest() -> Value {
        json!(format!("sha256:{}", "0".repeat(64)))
    }

    #[test]
    fn the_committed_suite_opens_and_every_body_it_names_is_verified() {
        let opened = open(&committed().join(PATH)).unwrap();
        let paths = opened
            .verified
            .iter()
            .map(|(path, _)| path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(paths, BODIES);
        for (path, digest) in &opened.verified {
            let bytes = std::fs::read(committed().join(path)).unwrap();
            assert_eq!(file_digest(&bytes).as_str(), digest, "{path}");
        }
    }

    /// The suite root is the directory that holds the index, wherever that
    /// is, and the runner looks nowhere else.
    #[test]
    fn the_suite_root_is_the_directory_that_holds_the_index() {
        let suite = Suite::new();
        assert_eq!(suite.open().unwrap().verified.len(), BODIES.len());
        let elsewhere = tempfile::tempdir().unwrap();
        let moved = elsewhere.path().join(PATH);
        std::fs::copy(suite.index_path(), &moved).unwrap();
        assert_eq!(
            open(&moved),
            Err(HarnessFailure::Missing(SCHEMA_PATH.to_owned()))
        );
        assert_eq!(
            open(Path::new("")),
            Err(HarnessFailure::Missing(String::new()))
        );
    }

    /// The runner reads the schema at its fixed path only, as the bytes it
    /// pins, and an index cannot name another schema, not even a copy.
    #[test]
    fn the_schema_is_read_from_its_fixed_path_and_must_be_the_pinned_one() {
        let suite = Suite::new();
        suite.remove(SCHEMA_PATH);
        assert_eq!(
            suite.open(),
            Err(HarnessFailure::Missing(SCHEMA_PATH.to_owned()))
        );

        let suite = Suite::new();
        let mut bytes = suite.read(SCHEMA_PATH);
        bytes.insert(1, b' ');
        suite.write(SCHEMA_PATH, &bytes);
        assert_eq!(
            suite.open(),
            Err(HarnessFailure::DigestMismatch(SCHEMA_PATH.to_owned()))
        );

        let suite = Suite::new();
        suite.write("schemas/copy.schema.json", &suite.read(SCHEMA_PATH));
        suite.change_index(|index| {
            index["suiteIndexSchema"]["path"] = json!("schemas/copy.schema.json");
        });
        assert_eq!(
            suite.open(),
            Err(HarnessFailure::NotRevisionZero(Member::SuiteIndexSchema))
        );
    }

    #[test]
    fn an_index_that_is_missing_or_not_strict_json_does_not_open() {
        let suite = Suite::new();
        let missing = suite.root().join("none.json");
        assert_eq!(
            open(&missing),
            Err(HarnessFailure::Missing(missing.display().to_string()))
        );
        for bytes in [
            &b"{\"suiteRevision\": \"0\", \"suiteRevision\": \"0\"}"[..],
            b"{",
            b"\xff",
            b"{} {}",
        ] {
            suite.write(PATH, bytes);
            assert_eq!(
                suite.open(),
                Err(HarnessFailure::Malformed(suite.index_name()))
            );
        }
    }

    /// The schema refuses an index before it is compared or any body it names
    /// is read: here each also declares another suite revision and has lost a
    /// body.
    #[test]
    fn an_index_the_schema_refuses_does_not_open() {
        let changes: [fn(&mut Value); 5] = [
            |index| {
                index.as_object_mut().unwrap().remove("registryPackage");
            },
            |index| index["caseSchema"] = json!({}),
            |index| index["profileSpecification"]["path"] = json!("../profile.json"),
            |index| index["profileSpecification"]["digest"] = json!("sha256:ABC"),
            |index| {
                let roles = index["registryPackage"]["members"].as_object_mut();
                roles.unwrap().remove("check-registry");
            },
        ];
        for change in changes {
            let suite = Suite::new();
            suite.change_index(|index| {
                change(index);
                index["suiteRevision"] = json!("1");
            });
            suite.remove(registry::PATH);
            assert_eq!(
                suite.open(),
                Err(HarnessFailure::SchemaInvalid(suite.index_name()))
            );
        }
    }

    /// Each member is compared with the artifact this crate is compiled from
    /// before any body is read: here each case has also lost a body.
    #[test]
    fn a_member_that_pins_another_artifact_is_not_revision_zero() {
        let root = Member::ResolverConstructionRootPackage;
        let set = Member::ConfigurationSchemaAuthoritySet;
        let package = Member::RegistryPackage;
        let cases = [
            ("/suiteRevision", json!("1"), Member::SuiteRevision),
            (
                "/suiteIndexSchema/identity",
                json!("other"),
                Member::SuiteIndexSchema,
            ),
            (
                "/suiteIndexSchema/revision",
                json!("1"),
                Member::SuiteIndexSchema,
            ),
            (
                "/suiteIndexSchema/digest",
                other_digest(),
                Member::SuiteIndexSchema,
            ),
            (
                "/resolverConstructionRootPackage/identity",
                json!("other"),
                root,
            ),
            (
                "/resolverConstructionRootPackage/revision",
                json!("1"),
                root,
            ),
            (
                "/resolverConstructionRootPackage/digest",
                other_digest(),
                root,
            ),
            (
                "/resolverConstructionRootPackage/canonicalDigest",
                other_digest(),
                root,
            ),
            (
                "/resolverConstructionRootPackage/members/cross-authority-rules/digest",
                other_digest(),
                root,
            ),
            (
                "/profileSpecificationSchema/identity",
                json!("other"),
                Member::ProfileSpecificationSchema,
            ),
            (
                "/profileSpecificationSchema/revision",
                json!("1"),
                Member::ProfileSpecificationSchema,
            ),
            (
                "/profileSpecificationSchema/digest",
                other_digest(),
                Member::ProfileSpecificationSchema,
            ),
            (
                "/profileSpecification/identity",
                json!("other"),
                Member::ProfileSpecification,
            ),
            (
                "/profileSpecification/revision",
                json!("1"),
                Member::ProfileSpecification,
            ),
            (
                "/profileSpecification/semanticDigest",
                other_digest(),
                Member::ProfileSpecification,
            ),
            (
                "/configurationSchemaAuthoritySet/identity",
                json!("other"),
                set,
            ),
            ("/configurationSchemaAuthoritySet/revision", json!("1"), set),
            (
                "/configurationSchemaAuthoritySet/digest",
                other_digest(),
                set,
            ),
            (
                "/configurationSchemaAuthoritySet/profileSpecification/semanticDigest",
                other_digest(),
                set,
            ),
            (
                "/configurationSchemaAuthoritySet/members/0/identity",
                json!("other"),
                set,
            ),
            (
                "/configurationSchemaAuthoritySet/members/0/dialect",
                json!("https://json-schema.org/draft/2020-12/schema"),
                set,
            ),
            (
                "/configurationSchemaAuthoritySet/members/0/digest",
                other_digest(),
                set,
            ),
            (
                "/resolverCapabilitySpecification/identity",
                json!("other"),
                Member::ResolverCapabilitySpecification,
            ),
            (
                "/resolverCapabilitySpecification/revision",
                json!("1"),
                Member::ResolverCapabilitySpecification,
            ),
            (
                "/resolverCapabilitySpecification/digest",
                other_digest(),
                Member::ResolverCapabilitySpecification,
            ),
            ("/caseSchema/identity", json!("other"), Member::CaseSchema),
            ("/caseSchema/revision", json!("1"), Member::CaseSchema),
            ("/caseSchema/digest", other_digest(), Member::CaseSchema),
            ("/registryPackage/identity", json!("other"), package),
            ("/registryPackage/revision", json!("1"), package),
            ("/registryPackage/digest", other_digest(), package),
            (
                "/registryPackage/members/finding-registry/identity",
                json!("other"),
                package,
            ),
            (
                "/registryPackage/members/finding-registry/revision",
                json!("1"),
                package,
            ),
            (
                "/registryPackage/members/finding-registry/digest",
                other_digest(),
                package,
            ),
        ];
        for (pointer, value, member) in cases {
            let suite = Suite::new();
            suite.change_index(|index| *index.pointer_mut(pointer).unwrap() = value);
            suite.remove(registry::PATH);
            assert_eq!(
                suite.open(),
                Err(HarnessFailure::NotRevisionZero(member)),
                "{pointer}"
            );
        }
        // Another mapped schema version is not revision "0"'s either.
        let suite = Suite::new();
        suite.change_index(|index| {
            let mapped = index[set.name()]["members"].as_object_mut().unwrap();
            let schema = mapped.remove("0").unwrap();
            mapped.insert("1".to_owned(), schema);
        });
        assert_eq!(suite.open(), Err(HarnessFailure::NotRevisionZero(set)));
    }

    /// A conditional obligation's manifest, inactive, for row `row` of the
    /// obligation table.
    fn obligation(row: usize) -> Value {
        let (id, activation, reason, owner) = case::OBLIGATIONS[row];
        json!({
            "suiteRevision": "0",
            "manifestKind": "conditional-obligation",
            "id": id,
            "activation": activation,
            "activationState": "inactive",
            "notApplicableReason": reason,
            "verificationOwner": owner,
            "traceability": { "decisionIds": ["015-051"], "ruleIds": [], "checkIds": [] }
        })
    }

    /// A Resolver Construction Case's manifest with this ID.
    fn construction_case(id: &str) -> Value {
        json!({
            "suiteRevision": "0",
            "manifestKind": "resolver-construction",
            "id": id,
            "constructionInputFixture": {
                "path": "fixtures/construction/a.json",
                "digest": format!("sha256:{}", "0".repeat(64))
            },
            "expected": {
                "constructed": false,
                "resolverConstructionIdentity": null,
                "failureReason": "registry-member-digest-mismatch",
                "resolverInvoked": false
            },
            "traceability": { "decisionIds": ["015-086"], "ruleIds": [], "checkIds": [] }
        })
    }

    /// A refused Resolver Input Admission Case's manifest with this ID.
    fn admission_case(id: &str) -> Value {
        json!({
            "suiteRevision": "0",
            "manifestKind": "resolver-input-admission",
            "id": id,
            "resolverConstruction": {
                "caseId": "015-a",
                "identityReference": format!("rci0_{}", "A".repeat(43))
            },
            "boundaryKind": "safe-origin-kind",
            "inputFixture": {
                "path": "fixtures/input-admission/a.json",
                "digest": format!("sha256:{}", "0".repeat(64))
            },
            "expected": {
                "admitted": false,
                "normalizedInput": null,
                "failureReason": "unknown-safe-origin-kind",
                "resolverInvoked": false
            },
            "verificationOwner": "intlify_config",
            "traceability": { "decisionIds": ["015-086"], "ruleIds": [], "checkIds": [] }
        })
    }

    impl Suite {
        /// Write each manifest's bytes below `cases/` and list it in the
        /// member's array, in the order given.
        fn list(&self, member: Member, manifests: &[(&str, Vec<u8>)]) {
            let entries = manifests
                .iter()
                .map(|(id, bytes)| {
                    let path = format!("cases/{}/{id}.json", member.name());
                    self.write(&path, bytes);
                    json!({ "id": id, "path": path, "digest": file_digest(bytes).as_str() })
                })
                .collect::<Vec<_>>();
            self.change_index(|index| index[member.name()] = Value::Array(entries));
        }
    }

    fn bytes(manifest: &Value) -> Vec<u8> {
        generate::write(manifest).unwrap()
    }

    #[test]
    fn listed_manifests_open_and_are_verified() {
        let suite = Suite::new();
        suite.list(
            Member::ResolverConstructionCases,
            &[(
                "015-construction-a",
                bytes(&construction_case("015-construction-a")),
            )],
        );
        suite.list(
            Member::ConditionalObligations,
            &[
                ("015-XV-001", bytes(&obligation(0))),
                ("015-XV-002", bytes(&obligation(1))),
            ],
        );
        let verified = suite.open().unwrap().verified;
        let paths = verified
            .iter()
            .skip(BODIES.len())
            .map(|(path, _)| path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            paths,
            [
                "cases/resolverConstructionCases/015-construction-a.json",
                "cases/conditionalObligations/015-XV-001.json",
                "cases/conditionalObligations/015-XV-002.json",
            ]
        );
    }

    /// Each array lists its IDs once, in ascending unsigned byte order, and
    /// no ID occurs in two arrays; the index is refused before any manifest
    /// is read.
    #[test]
    fn manifests_are_listed_once_in_ascending_order() {
        let obligations = Member::ConditionalObligations;
        let suite = Suite::new();
        suite.list(
            obligations,
            &[("015-XV-002", b"{".to_vec()), ("015-XV-001", b"{".to_vec())],
        );
        assert_eq!(
            suite.open(),
            Err(HarnessFailure::NotRevisionZero(obligations))
        );
        let suite = Suite::new();
        suite.list(
            obligations,
            &[("015-XV-001", b"{".to_vec()), ("015-XV-001", b"{".to_vec())],
        );
        assert_eq!(
            suite.open(),
            Err(HarnessFailure::NotRevisionZero(obligations))
        );
        // Two manifests, each of its own array's kind, may not share an ID.
        let admissions = Member::ResolverInputAdmissionCases;
        for (id, failure) in [
            ("015-b", None),
            ("015-a", Some(HarnessFailure::NotRevisionZero(admissions))),
        ] {
            let suite = Suite::new();
            suite.list(
                Member::ResolverConstructionCases,
                &[("015-a", bytes(&construction_case("015-a")))],
            );
            suite.list(admissions, &[(id, bytes(&admission_case(id)))]);
            assert_eq!(suite.open().err(), failure, "{id}");
        }
    }

    /// A listed manifest is a body like any other, strict UTF-8 JSON, valid
    /// against the Case Schema, and has the `id` that lists it and the kind
    /// of its array.
    #[test]
    fn a_listed_manifest_passes_the_case_schema_and_its_listing() {
        let cases = Member::ResolverConstructionCases;
        let path = "cases/resolverConstructionCases/015-a.json";
        let open = |member: Member, manifest: Vec<u8>| {
            let suite = Suite::new();
            suite.list(member, &[("015-a", manifest)]);
            suite.open()
        };
        assert!(open(cases, bytes(&construction_case("015-a"))).is_ok());
        assert_eq!(
            open(cases, b"{\"id\": \"015-a\", \"id\": \"015-a\"}".to_vec()),
            Err(HarnessFailure::Malformed(path.to_owned()))
        );
        let mut unknown = construction_case("015-a");
        unknown["unknown"] = json!("unknown");
        assert_eq!(
            open(cases, bytes(&unknown)),
            Err(HarnessFailure::SchemaInvalid(path.to_owned()))
        );
        assert_eq!(
            open(cases, bytes(&construction_case("015-b"))),
            Err(HarnessFailure::NotRevisionZero(cases))
        );
        assert_eq!(
            open(Member::ResolverCases, bytes(&construction_case("015-a"))),
            Err(HarnessFailure::NotRevisionZero(Member::ResolverCases))
        );
        let suite = Suite::new();
        suite.list(cases, &[("015-a", bytes(&construction_case("015-a")))]);
        suite.remove(path);
        assert_eq!(suite.open(), Err(HarnessFailure::Missing(path.to_owned())));
        let suite = Suite::new();
        suite.list(cases, &[("015-a", bytes(&construction_case("015-a")))]);
        let mut changed = suite.read(path);
        changed.push(b'\n');
        suite.write(path, &changed);
        assert_eq!(
            suite.open(),
            Err(HarnessFailure::DigestMismatch(path.to_owned()))
        );
    }

    #[test]
    fn roles_out_of_their_order_are_not_revision_zero() {
        for member in [
            Member::ResolverConstructionRootPackage,
            Member::RegistryPackage,
        ] {
            let suite = Suite::new();
            suite.change_index(|index| {
                let roles = index[member.name()]["members"].as_object().unwrap().clone();
                let mut roles = roles.into_iter().collect::<Vec<_>>();
                roles.rotate_left(1);
                index[member.name()]["members"] = Value::Object(roles.into_iter().collect());
            });
            assert_eq!(suite.open(), Err(HarnessFailure::NotRevisionZero(member)));
        }
    }

    /// Where a body lies below the suite root is the index's to say.
    #[test]
    fn a_body_may_lie_anywhere_below_the_suite_root() {
        let suite = Suite::new();
        let bytes = suite.read(capability::PATH);
        suite.remove(capability::PATH);
        suite.write("moved/capability.json", &bytes);
        suite.change_index(|index| {
            index["resolverCapabilitySpecification"]["path"] = json!("moved/capability.json");
        });
        let verified = suite.open().unwrap().verified;
        assert!(verified.contains(&(
            "moved/capability.json".to_owned(),
            capability::DIGEST.to_owned()
        )));
        assert!(!verified.iter().any(|(path, _)| path == capability::PATH));
    }

    #[test]
    fn a_body_that_is_missing_or_changed_does_not_open() {
        for path in &BODIES[1..] {
            let suite = Suite::new();
            suite.remove(path);
            assert_eq!(
                suite.open(),
                Err(HarnessFailure::Missing((*path).to_owned())),
                "{path}"
            );
            let suite = Suite::new();
            let mut bytes = suite.read(path);
            bytes.push(b'\n');
            suite.write(path, &bytes);
            assert_eq!(
                suite.open(),
                Err(HarnessFailure::DigestMismatch((*path).to_owned())),
                "{path}"
            );
        }
    }

    /// The Profile Specification is pinned by its semantic digest: another
    /// representation of its body opens, and another body does not.
    #[test]
    fn the_profile_specification_is_pinned_by_its_semantic_digest() {
        let pointer = "/profileSpecification/digest";
        let body: Value =
            serde_json::from_slice(&std::fs::read(committed().join(profile::PATH)).unwrap())
                .unwrap();

        let suite = Suite::new();
        suite.replace(profile::PATH, &serde_json::to_vec(&body).unwrap(), pointer);
        assert!(suite.open().is_ok());

        let mut other = body.clone();
        other["unknown"] = json!("unknown");
        let suite = Suite::new();
        suite.replace(profile::PATH, &serde_json::to_vec(&other).unwrap(), pointer);
        assert_eq!(
            suite.open(),
            Err(HarnessFailure::NotRevisionZero(
                Member::ProfileSpecification
            ))
        );

        let mut numbered = body;
        numbered["unknown"] = json!(1);
        let suite = Suite::new();
        suite.replace(
            profile::PATH,
            &serde_json::to_vec(&numbered).unwrap(),
            pointer,
        );
        assert_eq!(
            suite.open(),
            Err(HarnessFailure::NotRevisionZero(
                Member::ProfileSpecification
            ))
        );

        let suite = Suite::new();
        suite.replace(profile::PATH, b"{", pointer);
        assert_eq!(
            suite.open(),
            Err(HarnessFailure::Malformed(profile::PATH.to_owned()))
        );
    }

    /// A body reached through a symbolic link below the suite root does not
    /// open, though the link reaches the bytes the index declares: a linked
    /// file, a linked directory on the way, and the schema at its fixed path.
    #[cfg(unix)]
    #[test]
    fn a_body_reached_through_a_symbolic_link_does_not_open() {
        let outside = tempfile::tempdir().unwrap();
        copy_tree(&committed(), outside.path());
        let link = |suite: &Suite, path: &str| {
            let at = suite.root().join(path);
            if at.is_dir() {
                std::fs::remove_dir_all(&at).unwrap();
            } else {
                std::fs::remove_file(&at).unwrap();
            }
            std::os::unix::fs::symlink(outside.path().join(path), at).unwrap();
        };
        for (linked, refused) in [
            (capability::PATH, capability::PATH),
            ("registries", registry::PATH),
            (SCHEMA_PATH, SCHEMA_PATH),
        ] {
            let suite = Suite::new();
            link(&suite, linked);
            assert_eq!(
                suite.open(),
                Err(HarnessFailure::SymbolicLink(refused.to_owned())),
                "{linked}"
            );
        }
    }

    #[test]
    fn each_failure_says_what_failed() {
        let path = "registries/check-v0.json".to_owned();
        for (failure, message) in [
            (
                HarnessFailure::Missing(path.clone()),
                "registries/check-v0.json is missing or unreadable",
            ),
            (
                HarnessFailure::SymbolicLink(path.clone()),
                "registries/check-v0.json passes through a symbolic link",
            ),
            (
                HarnessFailure::DigestMismatch(path.clone()),
                "registries/check-v0.json does not have its declared digest",
            ),
            (
                HarnessFailure::Malformed(path.clone()),
                "registries/check-v0.json is not strict UTF-8 JSON",
            ),
            (
                HarnessFailure::SchemaInvalid(path),
                "registries/check-v0.json is not valid against its schema",
            ),
            (
                HarnessFailure::NotRevisionZero(Member::RegistryPackage),
                "the index's registryPackage is not revision \"0\"'s",
            ),
        ] {
            assert_eq!(failure.to_string(), message);
        }
    }
}
