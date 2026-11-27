// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Suite Index and its schema.
//!
//! The Suite Index pins every artifact of the suite: each entry names the
//! artifact's identity and revision, a suite-root-relative path, and its file
//! digest (D15:2858, 3031). Its schema is the runner's trust root
//! (D15:3029). Like the authority-envelope schemas, it constrains only the
//! form of each value: an identity or a revision is a token, a digest is a
//! `sha256:` presentation, a path is suite-root-relative, and each object
//! holds exactly its members. Which identity, revision, and digests an entry
//! carries, and the order of the roles, which a JSON Schema cannot express,
//! are the runner's to compare ([`super::runner`]).
//!
//! A suite-root-relative path is one or more segments joined by `/`, each
//! starting with an ASCII letter or digit and holding only ASCII letters,
//! digits, `.`, `_`, and `-`. No such path is absolute or empty, or holds a
//! `.` or `..` segment, a backslash, a URL's scheme, or a drive (D15:2858).
//!
//! This crate writes the schema and the index from its own values, as it
//! writes the construction artifacts: the index names each construction root
//! member, authority, and Registry Package member at the path this crate
//! writes it to, with the file digest of the bytes it writes there. It pins
//! the Case Schema ([`super::case`]) the same way, and lists each manifest by
//! its ID, path, and file digest in the array of its kind, in ascending
//! unsigned byte order of the IDs. The construction cases
//! ([`super::construction_cases`]) and the input admission cases of the
//! boundary kinds ([`super::admission_cases`]) are written; the other arrays
//! stay empty until their cases are.

use serde_json::{json, Map, Value};

use intlify_shared_json::token::ID_PATTERN;

use crate::construction::admission::ConstructionAdmission;
use crate::construction::component::file_digest;
use crate::construction::envelope::{closed, digest, document, token};
use crate::construction::generate::{self, Artifact};
use crate::construction::registry::RegistryRole;
use crate::construction::root::{self, RootRole};
use crate::model::CONFIGURATION_SCHEMA_VERSION;

use super::case::{self, ManifestKind, CASE_ID_PATTERN, OBLIGATION_ID_PATTERN};
use super::{admission_cases, construction_cases};

/// The Suite Index Schema's identity.
pub(crate) const SCHEMA_IDENTITY: &str = "intlify-project-profile-conformance-suite-index";

/// The Suite Index Schema's revision.
pub(crate) const SCHEMA_REVISION: &str = "0";

/// The Suite Index Schema's fixed path below the suite root.
pub(crate) const SCHEMA_PATH: &str = "schemas/suite-index-v0.schema.json";

/// The Suite Index Schema's file digest: the runner's trust root.
pub(crate) const SCHEMA_DIGEST: &str =
    "sha256:91cd21269a9f33ba8154616e581aeaeb080a8d35c8f5af5e1994a4e9a219e2b4";

/// The suite revision an index declares.
pub(crate) const SUITE_REVISION: &str = "0";

/// Where this crate writes its Suite Index below the suite root. The runner
/// never looks for an index there: it is handed the index's path.
pub(crate) const PATH: &str = "suite-index-v0.json";

/// A suite-root-relative path.
pub(crate) const PATH_PATTERN: &str =
    "^[A-Za-z0-9][A-Za-z0-9._-]*(?:/[A-Za-z0-9][A-Za-z0-9._-]*)*$";

/// One member of the Suite Index, in the order this crate writes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Member {
    SuiteRevision,
    SuiteIndexSchema,
    ResolverConstructionRootPackage,
    ProfileSpecificationSchema,
    ProfileSpecification,
    ConfigurationSchemaAuthoritySet,
    ResolverCapabilitySpecification,
    CaseSchema,
    RegistryPackage,
    ResolverConstructionCases,
    ResolverInputAdmissionCases,
    ResolverCases,
    HandoffVectors,
    ConditionalObligations,
}

impl Member {
    /// Every member, in the order this crate writes them.
    pub const ALL: [Self; 14] = [
        Self::SuiteRevision,
        Self::SuiteIndexSchema,
        Self::ResolverConstructionRootPackage,
        Self::ProfileSpecificationSchema,
        Self::ProfileSpecification,
        Self::ConfigurationSchemaAuthoritySet,
        Self::ResolverCapabilitySpecification,
        Self::CaseSchema,
        Self::RegistryPackage,
        Self::ResolverConstructionCases,
        Self::ResolverInputAdmissionCases,
        Self::ResolverCases,
        Self::HandoffVectors,
        Self::ConditionalObligations,
    ];

    /// The member's name in the index.
    pub const fn name(self) -> &'static str {
        match self {
            Self::SuiteRevision => "suiteRevision",
            Self::SuiteIndexSchema => "suiteIndexSchema",
            Self::ResolverConstructionRootPackage => "resolverConstructionRootPackage",
            Self::ProfileSpecificationSchema => "profileSpecificationSchema",
            Self::ProfileSpecification => "profileSpecification",
            Self::ConfigurationSchemaAuthoritySet => "configurationSchemaAuthoritySet",
            Self::ResolverCapabilitySpecification => "resolverCapabilitySpecification",
            Self::CaseSchema => "caseSchema",
            Self::RegistryPackage => "registryPackage",
            Self::ResolverConstructionCases => "resolverConstructionCases",
            Self::ResolverInputAdmissionCases => "resolverInputAdmissionCases",
            Self::ResolverCases => "resolverCases",
            Self::HandoffVectors => "handoffVectors",
            Self::ConditionalObligations => "conditionalObligations",
        }
    }

    /// The kind of the manifests the member lists, when it lists manifests.
    pub(crate) const fn manifests(self) -> Option<ManifestKind> {
        match self {
            Self::ResolverConstructionCases => Some(ManifestKind::ResolverConstruction),
            Self::ResolverInputAdmissionCases => Some(ManifestKind::ResolverInputAdmission),
            Self::ResolverCases => Some(ManifestKind::Resolver),
            Self::HandoffVectors => Some(ManifestKind::HandoffVector),
            Self::ConditionalObligations => Some(ManifestKind::ConditionalObligation),
            Self::SuiteRevision
            | Self::SuiteIndexSchema
            | Self::ResolverConstructionRootPackage
            | Self::ProfileSpecificationSchema
            | Self::ProfileSpecification
            | Self::ConfigurationSchemaAuthoritySet
            | Self::ResolverCapabilitySpecification
            | Self::CaseSchema
            | Self::RegistryPackage => None,
        }
    }
}

/// A suite-root-relative path.
fn path() -> Value {
    json!({ "type": "string", "pattern": PATH_PATTERN })
}

/// An entry that pins one artifact: its identity, its revision, its path,
/// its file digest, and then `more`.
fn pinned(more: &[(&str, Value)]) -> Value {
    let mut members = vec![
        ("identity", token()),
        ("revision", token()),
        ("path", path()),
        ("digest", digest()),
    ];
    members.extend_from_slice(more);
    closed(&members)
}

/// The Suite Index Schema's document.
pub(crate) fn schema() -> Value {
    let roots = RootRole::ALL.map(|role| {
        (
            role.name(),
            closed(&[("path", path()), ("digest", digest())]),
        )
    });
    let registries = RegistryRole::ALL.map(|role| (role.name(), pinned(&[])));
    let mapped = json!({
        "type": "object",
        "minProperties": 1,
        "propertyNames": { "pattern": ID_PATTERN },
        "additionalProperties": closed(&[
            ("identity", token()),
            ("revision", token()),
            ("dialect", json!({ "type": "string", "minLength": 1 })),
            ("path", path()),
            ("digest", digest()),
        ]),
    });
    let members = Member::ALL.map(|member| {
        let schema = match member {
            Member::SuiteRevision => token(),
            Member::SuiteIndexSchema
            | Member::ProfileSpecificationSchema
            | Member::ResolverCapabilitySpecification
            | Member::CaseSchema => pinned(&[]),
            Member::ResolverConstructionRootPackage => {
                pinned(&[("canonicalDigest", digest()), ("members", closed(&roots))])
            }
            Member::ProfileSpecification => pinned(&[("semanticDigest", digest())]),
            Member::ConfigurationSchemaAuthoritySet => pinned(&[
                (
                    "profileSpecification",
                    closed(&[
                        ("identity", token()),
                        ("revision", token()),
                        ("semanticDigest", digest()),
                    ]),
                ),
                ("members", mapped.clone()),
            ]),
            Member::RegistryPackage => pinned(&[("members", closed(&registries))]),
            Member::ResolverConstructionCases
            | Member::ResolverInputAdmissionCases
            | Member::ResolverCases
            | Member::HandoffVectors
            | Member::ConditionalObligations => {
                let id = if member == Member::ConditionalObligations {
                    OBLIGATION_ID_PATTERN
                } else {
                    CASE_ID_PATTERN
                };
                json!({
                    "type": "array",
                    "items": closed(&[
                        ("id", json!({ "type": "string", "pattern": id })),
                        ("path", path()),
                        ("digest", digest()),
                    ]),
                })
            }
        };
        (member.name(), schema)
    });
    document(closed(&members))
}

/// The Suite Index Schema's file digest, which the runner pins.
///
/// # Errors
///
/// Returns the serialization failure of the schema, which its value rules
/// out.
pub fn schema_digest() -> Result<String, serde_json::Error> {
    Ok(file_digest(&generate::write(&schema())?)
        .as_str()
        .to_owned())
}

/// A file of the suite this crate writes: its path below the suite root and
/// its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuiteFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

impl From<Artifact> for SuiteFile {
    fn from(artifact: Artifact) -> Self {
        Self {
            path: artifact.path.to_owned(),
            bytes: artifact.bytes,
        }
    }
}

/// Write the Suite Index Schema, the Case Schema, the Suite Index, which pins
/// both and lists the cases, and then each case's manifest, fixture, and the
/// bodies its fixture keeps in files of their own.
///
/// # Errors
///
/// Returns the serialization failure of an artifact, which the values rule
/// out.
pub fn artifacts() -> Result<Vec<SuiteFile>, serde_json::Error> {
    let schema = generate::write(&schema())?;
    let case_schema = case::artifact()?;
    let construction = construction_cases::cases()?;
    let admission = admission_cases::cases()?;
    let index = generate::write(&index(
        &schema,
        &case_schema,
        &construction.listing,
        &admission.listing,
    )?)?;
    let mut files = vec![
        SuiteFile {
            path: SCHEMA_PATH.to_owned(),
            bytes: schema,
        },
        case_schema.into(),
        SuiteFile {
            path: PATH.to_owned(),
            bytes: index,
        },
    ];
    files.extend(construction.files);
    files.extend(admission.files);
    Ok(files)
}

/// An entry that pins `artifact` under `identity` and `revision`.
fn entry(identity: &str, revision: Option<&String>, artifact: &Artifact) -> Value {
    json!({
        "identity": identity,
        "revision": revision,
        "path": artifact.path,
        "digest": file_digest(&artifact.bytes).as_str(),
    })
}

/// The index of the artifacts this crate writes, under the identities and
/// revisions the Root Package admits, of the schema written as `schema`, of
/// the Case Schema, and of the construction and input admission cases
/// `construction` and `input_admission` list.
fn index(
    schema: &[u8],
    case_schema: &Artifact,
    construction: &[Value],
    input_admission: &[Value],
) -> Result<Value, serde_json::Error> {
    let admission = ConstructionAdmission::revision_zero();
    let components = &admission.components;
    let root_artifacts = generate::artifacts()?;
    let [profile_schema, profile, set, mapped, capability] =
        <[Artifact; 5]>::try_from(generate::authorities()?)
            .unwrap_or_else(|_| unreachable!("five authorities are written"));
    let [origin_schema, origins, check_schema, checks, finding_schema, findings, container] =
        <[Artifact; 7]>::try_from(generate::registries()?)
            .unwrap_or_else(|_| unreachable!("six members and the container are written"));
    let semantic_digest = generate::authority_pins()?.profile_specification_semantic;
    let profile_tuple = json!({
        "identity": components.profile_specification.identity,
        "revision": components.profile_specification.revisions.first(),
        "semanticDigest": semantic_digest,
    });
    let mut index = Map::new();
    for member in Member::ALL {
        let value = match member {
            Member::SuiteRevision => json!(SUITE_REVISION),
            Member::SuiteIndexSchema => json!({
                "identity": SCHEMA_IDENTITY,
                "revision": SCHEMA_REVISION,
                "path": SCHEMA_PATH,
                "digest": file_digest(schema).as_str(),
            }),
            Member::ResolverConstructionRootPackage => {
                let package = &root_artifacts[RootRole::ALL.len()];
                let mut value = json!({
                    "identity": root::IDENTITY,
                    "revision": root::REVISION,
                    "path": package.path,
                    "digest": file_digest(&package.bytes).as_str(),
                    "canonicalDigest": generate::pinned()?.canonical_digest,
                });
                let members = RootRole::ALL
                    .into_iter()
                    .zip(&root_artifacts)
                    .map(|(role, artifact)| {
                        (
                            role.name().to_owned(),
                            json!({
                                "path": artifact.path,
                                "digest": file_digest(&artifact.bytes).as_str(),
                            }),
                        )
                    })
                    .collect::<Map<_, _>>();
                value["members"] = Value::Object(members);
                value
            }
            Member::ProfileSpecificationSchema => entry(
                &components.profile_specification_schema.identity,
                components.profile_specification_schema.revisions.first(),
                &profile_schema,
            ),
            Member::ProfileSpecification => {
                let mut value = entry(
                    &components.profile_specification.identity,
                    components.profile_specification.revisions.first(),
                    &profile,
                );
                value["semanticDigest"] = json!(semantic_digest);
                value
            }
            Member::ConfigurationSchemaAuthoritySet => {
                let schemas = &admission.configuration_schemas;
                let mut value = entry(
                    &components.configuration_schema_authority_set.identity,
                    components
                        .configuration_schema_authority_set
                        .revisions
                        .first(),
                    &set,
                );
                value["profileSpecification"] = profile_tuple.clone();
                let mut member = entry(&schemas.identity, schemas.revisions.first(), &mapped);
                member["dialect"] = json!(schemas.dialects.first());
                let member = ["identity", "revision", "dialect", "path", "digest"]
                    .map(|name| (name.to_owned(), member[name].clone()))
                    .into_iter()
                    .collect::<Map<_, _>>();
                value["members"] = json!({ CONFIGURATION_SCHEMA_VERSION: member });
                value
            }
            Member::ResolverCapabilitySpecification => entry(
                &components.resolver_capability_specification.identity,
                components
                    .resolver_capability_specification
                    .revisions
                    .first(),
                &capability,
            ),
            Member::RegistryPackage => {
                let mut value = entry(
                    &components.registry_package.identity,
                    components.registry_package.revisions.first(),
                    &container,
                );
                let members = [
                    &origin_schema,
                    &check_schema,
                    &finding_schema,
                    &origins,
                    &checks,
                    &findings,
                ];
                let members = RegistryRole::ALL
                    .into_iter()
                    .zip(members)
                    .map(|(role, artifact)| {
                        let admitted = role.admitted(&admission.registry_members);
                        (
                            role.name().to_owned(),
                            entry(&admitted.identity, admitted.revisions.first(), artifact),
                        )
                    })
                    .collect::<Map<_, _>>();
                value["members"] = Value::Object(members);
                value
            }
            Member::CaseSchema => entry(
                case::IDENTITY,
                Some(&case::REVISION.to_owned()),
                case_schema,
            ),
            Member::ResolverConstructionCases => Value::Array(construction.to_vec()),
            Member::ResolverInputAdmissionCases => Value::Array(input_admission.to_vec()),
            Member::ResolverCases | Member::HandoffVectors | Member::ConditionalObligations => {
                json!([])
            }
        };
        index.insert(member.name().to_owned(), value);
    }
    Ok(Value::Object(index))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::construction::admission::DRAFT_7;
    use crate::construction::validator::schema::Schema;

    /// The committed schema and index, at their paths.
    const COMMITTED: [(&str, &[u8]); 3] = [
        (
            SCHEMA_PATH,
            include_bytes!("../../conformance/schemas/suite-index-v0.schema.json"),
        ),
        (
            case::PATH,
            include_bytes!("../../conformance/schemas/case-v0.schema.json"),
        ),
        (
            PATH,
            include_bytes!("../../conformance/suite-index-v0.json"),
        ),
    ];

    /// The index this crate writes.
    fn written() -> Value {
        serde_json::from_slice(&artifacts().unwrap()[2].bytes).unwrap()
    }

    /// Whether the schema admits an index, as this crate's validator and the
    /// independent oracle both decide.
    fn admits(index: &Value) -> bool {
        let schema = schema();
        let read = Schema::compile(&schema).unwrap().validate(index).valid;
        let oracle = jsonschema::draft7::new(&schema).unwrap().is_valid(index);
        assert_eq!(read, oracle, "the validators disagree on {index}");
        read
    }

    #[test]
    fn the_committed_schema_and_index_are_the_ones_this_crate_writes() {
        let artifacts = artifacts().unwrap();
        let mut cases = construction_cases::cases().unwrap().files;
        cases.extend(admission_cases::cases().unwrap().files);
        assert_eq!(artifacts.len(), COMMITTED.len() + cases.len());
        assert_eq!(artifacts[COMMITTED.len()..], cases);
        for (artifact, (path, bytes)) in artifacts.iter().zip(COMMITTED) {
            assert_eq!(artifact.path, path);
            assert_eq!(
                artifact.bytes, bytes,
                "{path} is stale; regenerate it with the generate_conformance_suite example"
            );
            assert!(artifact.bytes.ends_with(b"}\n"), "{path}");
            assert!(!artifact.bytes.ends_with(b"\n\n"), "{path}");
            assert!(!artifact.bytes.contains(&b'\r'), "{path}");
        }
    }

    #[test]
    fn the_pinned_schema_digest_is_the_written_schema_s() {
        assert_eq!(
            schema_digest().unwrap(),
            SCHEMA_DIGEST,
            "the pinned digest in conformance/index.rs is stale"
        );
        assert_eq!(
            file_digest(&artifacts().unwrap()[0].bytes).as_str(),
            SCHEMA_DIGEST
        );
    }

    /// The schema is a Draft 7 schema both validators read, and the index
    /// this crate writes is valid against it, with its members in their
    /// order.
    #[test]
    fn the_written_index_is_valid_against_the_draft_7_schema() {
        let schema = schema();
        assert_eq!(schema["$schema"], DRAFT_7);
        assert!(jsonschema::draft7::meta::validator().is_valid(&schema));
        let index = written();
        assert!(admits(&index));
        let names = index.as_object().unwrap().keys().collect::<Vec<_>>();
        assert_eq!(names, Member::ALL.map(Member::name));
        assert_eq!(index["suiteIndexSchema"]["digest"], SCHEMA_DIGEST);
    }

    /// The index names every artifact this crate writes by its path and file
    /// digest, but the construction vectors, which are no member, and the
    /// fixtures and bodies that the manifests it lists name.
    #[test]
    fn the_index_names_every_artifact_this_crate_writes() {
        fn collect<'a>(value: &'a Value, named: &mut BTreeMap<&'a str, &'a str>) {
            match value {
                Value::Object(members) => {
                    if let (Some(path), Some(digest)) = (members.get("path"), members.get("digest"))
                    {
                        named.insert(path.as_str().unwrap(), digest.as_str().unwrap());
                    }
                    members.values().for_each(|member| collect(member, named));
                }
                Value::Array(items) => items.iter().for_each(|item| collect(item, named)),
                _ => {}
            }
        }
        let index = written();
        let mut named = BTreeMap::new();
        collect(&index, &mut named);
        let written = artifacts().unwrap();
        let manifests = written
            .iter()
            .filter(|file| file.path.starts_with("cases/"))
            .cloned();
        let artifacts = generate::artifacts()
            .unwrap()
            .into_iter()
            .chain(generate::authorities().unwrap())
            .chain(generate::registries().unwrap())
            .map(SuiteFile::from)
            .chain(written.iter().take(2).cloned())
            .chain(manifests)
            .map(|file| (file.path, file_digest(&file.bytes).as_str().to_owned()))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(named.len(), 20 + 44 + 118);
        assert_eq!(
            named,
            artifacts
                .iter()
                .map(|(path, digest)| (path.as_str(), digest.as_str()))
                .collect::<BTreeMap<_, _>>()
        );
    }

    #[test]
    fn a_path_is_suite_root_relative() {
        let with = |path: &str| {
            let mut index = written();
            index["registryPackage"]["members"]["check-registry"]["path"] = json!(path);
            index
        };
        for path in [
            "a",
            "0",
            "construction/root-package-v0.json",
            "cases/conditional/015-XV-001.json",
            "a/b.c/d_e-f",
            "a./b..c",
        ] {
            assert!(admits(&with(path)), "{path}");
        }
        for path in [
            "",
            "/a",
            "a/",
            "a//b",
            ".",
            "..",
            "./a",
            "a/./b",
            "a/../b",
            "../a",
            ".hidden",
            "-a",
            "_a",
            "a\\b",
            "C:/a",
            "c:",
            "https://example.com/a.json",
            "file:a",
            "a b",
            "a\tb",
            "\u{e9}.json",
            "a\n",
        ] {
            assert!(!admits(&with(path)), "{path:?}");
        }
    }

    /// Each manifest array lists closed references whose ID has its kind's
    /// grammar: a case or vector ID, or, in the obligations' array, an
    /// obligation ID.
    #[test]
    fn a_manifest_array_lists_ids_of_its_kind() {
        let digest = format!("sha256:{}", "0".repeat(64));
        let listing = |member: Member, id: &str| {
            let mut index = written();
            index[member.name()] = json!([{ "id": id, "path": "cases/a.json", "digest": digest }]);
            index
        };
        for member in Member::ALL
            .into_iter()
            .filter(|member| member.manifests().is_some())
        {
            let (own, other) = if member == Member::ConditionalObligations {
                ("015-XV-001", "015-construction-revision-0")
            } else {
                ("015-construction-revision-0", "015-XV-001")
            };
            assert!(admits(&listing(member, own)), "{member:?}");
            assert!(!admits(&listing(member, other)), "{member:?}");
            for id in [
                "015-",
                "015-Upper",
                "016-a",
                "015-a--b",
                "015-XV-01",
                "015-XX-001",
            ] {
                assert!(!admits(&listing(member, id)), "{member:?} {id}");
            }
        }
    }

    /// The schema constrains form only: another identity, revision, or
    /// digest of the right form is the runner's to compare, and another
    /// mapped schema version is too.
    #[test]
    fn the_schema_checks_form_only() {
        let mut index = written();
        index["suiteRevision"] = json!("1");
        index["profileSpecification"]["identity"] = json!("another-identity");
        index["registryPackage"]["members"]["check-registry"]["revision"] = json!("7");
        index["resolverConstructionRootPackage"]["canonicalDigest"] =
            json!(format!("sha256:{}", "0".repeat(64)));
        let mapped = index["configurationSchemaAuthoritySet"]["members"]["0"].clone();
        index["configurationSchemaAuthoritySet"]["members"]["1"] = mapped;
        assert!(admits(&index));
    }

    /// Every object of the index is closed: each of its members is required
    /// and nothing else is admitted, and every value has its form.
    #[test]
    fn the_schema_closes_every_object_and_value() {
        fn objects(value: &Value, pointer: &str, found: &mut Vec<String>) {
            if let Value::Object(members) = value {
                found.push(pointer.to_owned());
                for (name, member) in members {
                    objects(member, &format!("{pointer}/{name}"), found);
                }
            }
        }
        let index = written();
        let mut pointers = Vec::new();
        objects(&index, "", &mut pointers);
        assert_eq!(pointers.len(), 25);
        for pointer in &pointers {
            let names = index.pointer(pointer).unwrap().as_object().unwrap().keys();
            for name in names {
                let mut fewer = index.clone();
                fewer
                    .pointer_mut(pointer)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(name);
                assert!(!admits(&fewer), "{pointer} without {name}");
            }
            let mut more = index.clone();
            more.pointer_mut(pointer).unwrap()["unknown"] = json!("unknown");
            assert!(!admits(&more), "{pointer} with one member too many");
        }
        let out_of_form = [
            ("/suiteRevision", json!("Zero")),
            ("/suiteRevision", json!(0)),
            ("/suiteIndexSchema/identity", json!("Suite-Index")),
            ("/suiteIndexSchema/revision", json!("")),
            ("/suiteIndexSchema/digest", json!("sha256:ABC")),
            (
                "/resolverConstructionRootPackage/canonicalDigest",
                json!("sha1:0"),
            ),
            (
                "/resolverConstructionRootPackage/members/cross-authority-rules/digest",
                json!(null),
            ),
            ("/profileSpecification/semanticDigest", json!("")),
            (
                "/configurationSchemaAuthoritySet/members/0/dialect",
                json!(""),
            ),
            ("/configurationSchemaAuthoritySet/members/0", json!("0")),
            ("/configurationSchemaAuthoritySet/members", json!({})),
            (
                "/registryPackage/members/finding-registry/identity",
                json!(["a"]),
            ),
            ("/registryPackage/members", json!([])),
            ("/resolverCases", json!({})),
            (
                "/handoffVectors",
                json!([{ "id": "015-a", "path": "a.json" }]),
            ),
        ];
        for (pointer, value) in out_of_form {
            let mut changed = index.clone();
            *changed.pointer_mut(pointer).unwrap() = value.clone();
            assert!(!admits(&changed), "{pointer} = {value}");
        }
        let mut renamed = index;
        let set = &mut renamed["configurationSchemaAuthoritySet"]["members"];
        let mapped = set.as_object_mut().unwrap().remove("0").unwrap();
        set["Zero"] = mapped;
        assert!(
            !admits(&renamed),
            "a mapped schema version that is no token"
        );
    }
}
