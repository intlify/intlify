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
//!    `manifestKind` of its array; then each fixture a construction case or
//!    an input admission case names has the digest the case declares for it,
//!    is strict UTF-8 JSON of the form the Case Schema gives it, the latter
//!    its boundary kind's, and every body it references has the digest the
//!    fixture declares (D15:3247). A body two references name is read once,
//!    and both must declare its digest;
//! 6. that each input admission case can run: the construction case it
//!    names is listed, constructs, and has the identity the case states, and
//!    the harness admits the capability value of a boundary that reads
//!    capability bounds and, under it, the programmatic pair's canonical
//!    Snapshot (D15:3158, 3162, 3252). An admission step's case has no such
//!    part: its capability is one of its slots, or bounds its entry, and a
//!    refusal is the step's own result (D15:3166, 3168).
//! 7. that each Handoff Vector reproduces its producer: the Resolver Case its
//!    `producerCaseId` names is listed, and each of its expected outcomes is
//!    checked and gives the vector's facts and bindings, the semantic groups
//!    the facts name and the binding rows their values reference (D15:3330,
//!    3348); and that each conditional obligation is inactive, as an
//!    evaluated activation must compute from the Suite Index and the Profile
//!    Specification, since an active one would need cases its row names,
//!    which no revision-`"0"` manifest can name (D15:3371).
//!
//! Steps 5 to 7 go entry by entry: an entry's body, its manifest and
//! fixture, and an input admission case's construction are checked before
//! the next entry's body is read.
//!
//! A suite that opened runs its cases from what it verified, and reads no
//! file again ([`Opened::run`]). A construction case constructs from its
//! fixture's input under the built-in construction root
//! ([`super::construction`]), and an input admission case evaluates its
//! boundary on its fixture's input under the construction it names
//! ([`super::admission`]); the case passes when what construction or the
//! boundary does is what the case expects. A case that does not pass is a
//! conformance failure of the implementation, not a harness failure.
//!
//! An artifact whose file digest is the one this crate pins has fixed
//! contents, so the Root Package's member list, the Configuration Schema
//! Authority Set's Profile Specification, and the Registry Package's container
//! need no reading of their own.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use serde_json::{json, Map, Value};

use crate::construction::admission::{ComponentAdmission, ConstructionAdmission, DRAFT_7};
use crate::construction::component::{file_digest, strict_json};
use crate::construction::registry::{self, RegistryRole};
use crate::construction::resolver::{build, ConstructedResolver};
use crate::construction::root::{self, RootPackage, RootRole};
use crate::construction::validator::schema::Schema;
use crate::construction::{authority_set, capability, profile};
use crate::model::CONFIGURATION_SCHEMA_VERSION;
use crate::resolver::origin::SafeOriginKind;

use super::admission;
use super::case::{self, input_fixture, ManifestKind, RelationKind};
use super::construction::{self, Constructed};
use super::fixture::{references, Bodies};
use super::handoff_vectors::produced;
use super::index::{
    Member, SCHEMA_DIGEST, SCHEMA_IDENTITY, SCHEMA_PATH, SCHEMA_REVISION, SUITE_REVISION,
};
use super::obligations;
use super::resolution::{self, Resolved, Unrunnable};

/// Why a suite does not open: a Conformance harness failure (D15:2858).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HarnessFailure {
    /// No file is at the path, or it cannot be read.
    Missing(String),
    /// The path passes through a symbolic link below the suite root, so the
    /// file it reaches may lie outside the suite.
    SymbolicLink(String),
    /// The file's bytes do not have the digest the index, or the manifest or
    /// fixture that references them, declares for them, or, for the Suite
    /// Index Schema, the one the runner pins.
    DigestMismatch(String),
    /// The file is not strict UTF-8 JSON.
    Malformed(String),
    /// The index is not valid against the Suite Index Schema, or a manifest
    /// or a fixture against the Case Schema: it lacks a member, leaving a body
    /// it must list unlisted, holds one too many, or spells a value outside
    /// its form.
    SchemaInvalid(String),
    /// The member is not revision `"0"`'s: it pins another artifact, its
    /// roles are out of order, the Profile Specification's body does not have
    /// the semantic digest it declares, or a manifest it lists is out of
    /// order, listed twice, or has another `id` or `manifestKind`.
    NotRevisionZero(Member),
    /// The Resolver Input Admission Case or Resolver Case with this ID cannot
    /// run: its `resolverConstruction` names no construction case the suite
    /// lists, one that does not construct, or another identity than the one
    /// constructed; for an input admission case, the capability value or the
    /// canonical Snapshot the harness admits before the boundary is refused;
    /// for a Resolver Case, an expected outcome names another identity, or an
    /// entry's inputs are not admitted as its fixtures give them (D15:3158,
    /// 3162, 3219, 3252).
    InvalidCase(String),
    /// The Handoff Vector with this ID does not reproduce its producer: its
    /// `producerCaseId` names no listed Resolver Case, or an expected outcome
    /// of that case is not checked or gives other facts or bindings than the
    /// vector holds (D15:3330, 3348).
    UnreproducedVector(String),
    /// The conditional obligation with this ID is not in the state the
    /// harness finds: it is active, or its evaluated activation holds
    /// (D15:3371).
    UnmetObligation(String),
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
            Self::InvalidCase(id) => write!(formatter, "case {id} cannot run"),
            Self::UnreproducedVector(id) => {
                write!(
                    formatter,
                    "handoff vector {id} does not reproduce its producer"
                )
            }
            Self::UnmetObligation(id) => {
                write!(formatter, "conditional obligation {id} is not inactive")
            }
        }
    }
}

/// A suite that opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    /// The suite-root-relative path and file digest of each body the runner
    /// verified, once each, in the order it verified them: the Suite Index
    /// Schema first, then each member's bodies in the order of the index's
    /// members, a manifest followed by its fixture and the bodies the fixture
    /// references first.
    pub verified: Vec<(String, String)>,
    /// Every verified body, by its path.
    bodies: Bodies,
    /// Each construction case, in the order of its array.
    construction: Vec<ListedCase>,
    /// Each input admission case, in the order of its array.
    admission: Vec<AdmissionCase>,
    /// Each Resolver Case, in the order of its array.
    resolver: Vec<ResolverCase>,
}

/// A case the suite lists: its ID, its fixture, and what it expects.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ListedCase {
    id: String,
    fixture: Value,
    expected: Value,
}

/// An input admission case the suite lists: the case, its boundary kind, and
/// what its boundary is evaluated against.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AdmissionCase {
    case: ListedCase,
    kind: &'static str,
    constructed: Constructed,
}

/// A Resolver Case the suite lists: its ID, the construction case it names
/// and the identity it states, its common input fixture, and each paired
/// entry, the file entry first (D15:3174-3216).
#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolverCase {
    id: String,
    construction: String,
    reference: String,
    common: Value,
    entries: Vec<ResolverEntry>,
    relations: Vec<Relation>,
}

/// A relation a Resolver Case declares to another: its kind and the other
/// case's ID (D15:3305). Its role and class say why the case holds it, and
/// the harness checks only the kind.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Relation {
    kind: RelationKind,
    case: String,
}

/// One paired entry of a Resolver Case: its side, its admission input
/// fixture, and the outcome it expects.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolverEntry {
    side: &'static str,
    fixture: Value,
    expected: Value,
}

/// One outcome of a run, a case or a Resolver Case's entry, its duties, or
/// its pair: what it expects, and what the implementation did, in the same
/// form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub id: String,
    pub expected: Value,
    pub actual: Value,
}

impl Outcome {
    /// Whether the implementation did what the case expects.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.expected == self.actual
    }
}

impl Opened {
    /// Run each case, in the order of the index's arrays, a Resolver Case
    /// once its entries ran as their outcomes.
    #[must_use]
    pub fn run(&self) -> Vec<Outcome> {
        let construction = self.construction.iter().map(|case| Outcome {
            id: case.id.clone(),
            expected: case.expected.clone(),
            actual: construction::outcome(&construction::input(&case.fixture, &self.bodies)),
        });
        let admission = self.admission.iter().map(|admission| Outcome {
            id: admission.case.id.clone(),
            expected: admission.case.expected.clone(),
            actual: admission::outcome(
                admission.kind,
                &admission.case.fixture,
                &self.bodies,
                &admission.constructed,
            ),
        });
        // Every entry of every Resolver Case resolves first, since a relation
        // reads another case's entries (D15:3305).
        let mut resolvers = BTreeMap::new();
        for case in &self.resolver {
            resolvers
                .entry(case.construction.as_str())
                .or_insert_with(|| built(&self.construction, &case.construction, &self.bodies));
        }
        let constructed = |case: &ResolverCase| {
            resolvers[case.construction.as_str()]
                .as_ref()
                .expect("the runner established the case's construction")
        };
        let resolved = self
            .resolver
            .iter()
            .map(|case| {
                case.entries
                    .iter()
                    .map(|entry| {
                        resolution::resolved(
                            &case.common,
                            &entry.fixture,
                            &self.bodies,
                            constructed(case),
                            &case.reference,
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let mut resolver = Vec::new();
        for (case, done) in self.resolver.iter().zip(&resolved) {
            resolver.extend(resolver_outcomes(case, done));
            resolver.extend(self.relation_outcomes(case, done, &resolved, constructed(case)));
        }
        construction.chain(admission).chain(resolver).collect()
    }

    /// Whether each relation of `case` holds for each side both cases pair,
    /// `case`'s entry the later and the other case's the earlier, once both
    /// entries resolve (D15:3305): `<id>/relation/<n>/<side>`, with the
    /// relation's other case and kind.
    fn relation_outcomes(
        &self,
        case: &ResolverCase,
        done: &[Result<Resolved, Unrunnable>],
        resolved: &[Vec<Result<Resolved, Unrunnable>>],
        constructed: &ConstructedResolver,
    ) -> Vec<Outcome> {
        let mut outcomes = Vec::new();
        for (index, relation) in case.relations.iter().enumerate() {
            let other = self
                .resolver
                .iter()
                .position(|other| other.id == relation.case)
                .expect("the runner checked each relation's case");
            for (entry, later) in case.entries.iter().zip(done) {
                let earlier = self.resolver[other]
                    .entries
                    .iter()
                    .zip(&resolved[other])
                    .find(|(own, _)| own.side == entry.side)
                    .map(|(_, done)| done);
                if let (Some(Ok(earlier)), Ok(later)) = (earlier, later) {
                    let holds = resolution::related(relation.kind, earlier, later, constructed);
                    let view = |holds: bool| json!({ "case": relation.case, "kind": relation.kind.id(), "holds": holds });
                    outcomes.push(Outcome {
                        id: format!("{}/relation/{index}/{}", case.id, entry.side),
                        expected: view(true),
                        actual: view(holds),
                    });
                }
            }
        }
        outcomes
    }
}

/// What each paired entry of `case`, resolved as `done`, does beside what it
/// expects, the duties its outcome breaks beside none, and, when both
/// entries are paired and resolve, whether they agree where 015 makes them
/// agree (D15:3227, 3229, 3233): `<id>/<side>`, `<id>/<side>/duties`, then
/// `<id>/pair`. An entry the resolver does not resolve, which only a defect
/// of the resolver makes, fails its own outcomes and has no pair.
fn resolver_outcomes(case: &ResolverCase, done: &[Result<Resolved, Unrunnable>]) -> Vec<Outcome> {
    let mut outcomes = Vec::new();
    for (entry, done) in case.entries.iter().zip(done) {
        let id = format!("{}/{}", case.id, entry.side);
        let (actual, duties) = match done {
            Ok(done) => (
                done.outcome.clone(),
                json!(resolution::duties(&done.outcome)),
            ),
            Err(unrunnable) => {
                let unrunnable = json!({ "unrunnable": format!("{unrunnable:?}") });
                (unrunnable.clone(), unrunnable)
            }
        };
        outcomes.push(Outcome {
            id: id.clone(),
            expected: entry.expected.clone(),
            actual,
        });
        outcomes.push(Outcome {
            id: format!("{id}/duties"),
            expected: json!([]),
            actual: duties,
        });
    }
    if let [Ok(file), Ok(programmatic)] = done {
        let materialized =
            file.materialized.is_some() && file.materialized == programmatic.materialized;
        outcomes.push(Outcome {
            id: format!("{}/pair", case.id),
            expected: json!(true),
            actual: json!(resolution::paired(
                &file.outcome,
                &programmatic.outcome,
                materialized
            )),
        });
    }
    outcomes
}

/// The resolver the construction case `id` of `construction` builds, `None`
/// when the suite lists no such case or construction refuses its input.
fn built(construction: &[ListedCase], id: &str, bodies: &Bodies) -> Option<ConstructedResolver> {
    let case = construction.iter().find(|case| case.id == id)?;
    let root = root::ConstructionRoot::built_in().expect("the built-in construction root verifies");
    build(root, construction::input(&case.fixture, bodies)).ok()
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
        index: &value,
        profile: None,
        verified: vec![(SCHEMA_PATH.to_owned(), SCHEMA_DIGEST.to_owned())],
        bodies: Bodies::new(),
        case_schema: None,
        fixture_schemas: BTreeMap::new(),
        construction: Vec::new(),
        constructions: BTreeMap::new(),
        admission: Vec::new(),
        resolvers: BTreeMap::new(),
        resolver: Vec::new(),
    };
    for member in Member::ALL {
        closure.verify(member, &value[member.name()])?;
    }
    closure.relations()?;
    Ok(Opened {
        verified: closure.verified,
        bodies: closure.bodies,
        construction: closure.construction,
        admission: closure.admission,
        resolver: closure.resolver,
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

/// The closure as the runner verifies it: the bodies verified so far, the
/// Case Schema and its document once its body is, the forms of the fixtures
/// read from it, and the cases listed so far.
struct Closure<'a> {
    root: &'a Path,
    /// The index, from which the harness computes an evaluated activation.
    index: &'a Value,
    /// The Profile Specification's body, once it is verified.
    profile: Option<Value>,
    verified: Vec<(String, String)>,
    bodies: Bodies,
    case_schema: Option<(Schema, Value)>,
    fixture_schemas: BTreeMap<&'static str, Schema>,
    construction: Vec<ListedCase>,
    /// What each construction case an admission case names constructs, once.
    constructions: BTreeMap<String, Option<Constructed>>,
    admission: Vec<AdmissionCase>,
    /// The resolver each construction case a Resolver Case names builds,
    /// once.
    resolvers: BTreeMap<String, Option<ConstructedResolver>>,
    resolver: Vec<ResolverCase>,
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
            let bytes = self.body(path, text(&entry["digest"]))?;
            if member == Member::ProfileSpecification {
                semantic(path, &bytes, text(&entry["semanticDigest"]))?;
                self.profile = strict_json(&bytes);
            } else if member == Member::CaseSchema {
                let document = strict_json(&bytes).expect("a pinned schema is strict JSON");
                self.case_schema = Some((compile(&bytes), document));
            } else if let Some(kind) = member.manifests() {
                self.manifest(member, kind, entry, &bytes)?;
            }
        }
        Ok(())
    }

    /// Read the body at `path` with the file digest declared for it, once: a
    /// body already verified must be declared with the digest it has.
    fn body(&mut self, path: &str, digest: &str) -> Result<Vec<u8>, HarnessFailure> {
        let bytes = match self.bodies.get(path) {
            Some(bytes) => bytes.clone(),
            None => read(self.root, path)?,
        };
        if file_digest(&bytes).as_str() != digest {
            return Err(HarnessFailure::DigestMismatch(path.to_owned()));
        }
        if !self.bodies.contains_key(path) {
            self.verified.push((path.to_owned(), digest.to_owned()));
            self.bodies.insert(path.to_owned(), bytes.clone());
        }
        Ok(bytes)
    }

    /// Read a fixture: the body its reference names, strict UTF-8 JSON of the
    /// form the Case Schema's `definition` gives it, and every body it
    /// references.
    fn fixture(
        &mut self,
        reference: &Value,
        definition: &'static str,
    ) -> Result<Value, HarnessFailure> {
        let path = text(&reference["path"]);
        let bytes = self.body(path, text(&reference["digest"]))?;
        let fixture =
            strict_json(&bytes).ok_or_else(|| HarnessFailure::Malformed(path.to_owned()))?;
        if !self.fixture_schema(definition).validate(&fixture).valid {
            return Err(HarnessFailure::SchemaInvalid(path.to_owned()));
        }
        for raw in references(&fixture) {
            self.body(text(&raw["path"]), text(&raw["digest"]))?;
        }
        Ok(fixture)
    }

    /// The form the Case Schema gives a fixture: its `definition`, as the root
    /// of the schema's definitions.
    fn fixture_schema(&mut self, definition: &'static str) -> &Schema {
        let (_, document) = self
            .case_schema
            .as_ref()
            .expect("the Case Schema is verified before the manifests");
        self.fixture_schemas.entry(definition).or_insert_with(|| {
            let root = json!({
                "$schema": DRAFT_7,
                "$ref": format!("#/definitions/{definition}"),
                "definitions": document["definitions"],
            });
            Schema::compile(&root).expect("a definition of the pinned Case Schema compiles")
        })
    }

    /// Check a manifest: strict UTF-8 JSON, valid against the Case Schema,
    /// with the `id` of the entry that lists it and the kind of its array;
    /// and list a construction case, or an input admission case, with its
    /// fixture.
    fn manifest(
        &mut self,
        member: Member,
        kind: ManifestKind,
        entry: &Value,
        bytes: &[u8],
    ) -> Result<(), HarnessFailure> {
        let path = text(&entry["path"]);
        let manifest =
            strict_json(bytes).ok_or_else(|| HarnessFailure::Malformed(path.to_owned()))?;
        let (schema, _) = self
            .case_schema
            .as_ref()
            .expect("the Case Schema is verified before the manifests");
        if !schema.validate(&manifest).valid {
            return Err(HarnessFailure::SchemaInvalid(path.to_owned()));
        }
        if manifest["id"] != entry["id"] || manifest["manifestKind"] != kind.name() {
            return Err(HarnessFailure::NotRevisionZero(member));
        }
        if kind == ManifestKind::ResolverConstruction {
            let fixture = self.fixture(
                &manifest["constructionInputFixture"],
                "constructionInputFixture",
            )?;
            self.construction.push(ListedCase {
                id: text(&manifest["id"]).to_owned(),
                fixture,
                expected: manifest["expected"].clone(),
            });
        }
        if kind == ManifestKind::ResolverInputAdmission {
            self.admission_case(&manifest)?;
        }
        if kind == ManifestKind::Resolver {
            self.resolver_case(&manifest)?;
        }
        if kind == ManifestKind::HandoffVector {
            self.handoff_vector(&manifest)?;
        }
        if kind == ManifestKind::ConditionalObligation {
            let id = text(&manifest["id"]);
            let profile = self
                .profile
                .as_ref()
                .expect("the Profile Specification is verified before the manifests");
            let active = manifest["activationState"] == "active";
            if !obligations::met(id, active, self.index, profile) {
                return Err(HarnessFailure::UnmetObligation(id.to_owned()));
            }
        }
        Ok(())
    }

    /// Check that a Handoff Vector reproduces its producer: the Resolver
    /// Case its `producerCaseId` names is listed, and each of its expected
    /// outcomes gives the vector's facts and bindings (D15:3330, 3348). The
    /// Resolver Cases precede the vectors in the index.
    fn handoff_vector(&self, manifest: &Value) -> Result<(), HarnessFailure> {
        let facts = manifest["producerProfileFacts"]
            .as_object()
            .expect("the Case Schema makes the facts an object");
        let held = Some((
            manifest["producerProfileFacts"].clone(),
            manifest["producerArtifactBindings"].clone(),
        ));
        let reproduced = self
            .resolver
            .iter()
            .find(|case| case.id == text(&manifest["producerCaseId"]))
            .is_some_and(|case| {
                case.entries.iter().all(|entry| {
                    produced(&entry.expected, facts.keys().map(String::as_str)) == held
                })
            });
        if reproduced {
            Ok(())
        } else {
            Err(HarnessFailure::UnreproducedVector(
                text(&manifest["id"]).to_owned(),
            ))
        }
    }

    /// List a Resolver Case with its fixtures, once the construction case it
    /// names builds a resolver, each side's entry fixture submits through
    /// that side's own adapter, each expected outcome names the identity the
    /// case states, and the boundaries admit each paired entry's inputs as its
    /// fixtures give them, which they refuse under an identity other than the
    /// constructed one; a case that does not meet them cannot run (D15:3219,
    /// 3233, 3252).
    fn resolver_case(&mut self, manifest: &Value) -> Result<(), HarnessFailure> {
        let id = text(&manifest["id"]).to_owned();
        let invalid = || HarnessFailure::InvalidCase(id.clone());
        let construction = &manifest["resolverConstruction"];
        let case_id = text(&construction["caseId"]).to_owned();
        let reference = text(&construction["identityReference"]).to_owned();
        let common = self.fixture(
            &manifest["commonInvocationInputFixture"],
            "commonInvocationInputFixture",
        )?;
        let mut entries = Vec::new();
        let sides = [
            ("file", SafeOriginKind::ConfigurationFile),
            ("programmatic", SafeOriginKind::ConfigurationProgrammatic),
        ];
        for (side, source) in sides {
            let entry = &manifest["entries"][side];
            if entry.get("unpaired").is_some() {
                continue;
            }
            let fixture = self.fixture(
                &entry["entryAdmissionInputFixture"],
                "entryAdmissionInputFixture",
            )?;
            if fixture.get(source.id()).is_none() {
                return Err(invalid());
            }
            let expected =
                self.fixture(&entry["expectedOutcomeFixture"], "expectedOutcomeFixture")?;
            if expected["resolverConstructionIdentity"] != reference.as_str() {
                return Err(invalid());
            }
            entries.push(ResolverEntry {
                side,
                fixture,
                expected,
            });
        }
        if !self.resolvers.contains_key(&case_id) {
            let built = built(&self.construction, &case_id, &self.bodies);
            self.resolvers.insert(case_id.clone(), built);
        }
        let constructed = self.resolvers[&case_id].as_ref().ok_or_else(invalid)?;
        for entry in &entries {
            resolution::admissible(
                &common,
                &entry.fixture,
                &self.bodies,
                constructed,
                &reference,
            )
            .map_err(|_| invalid())?;
        }
        let relations = manifest
            .get("relations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|relation| Relation {
                kind: RelationKind::ALL
                    .into_iter()
                    .find(|kind| relation["kind"] == kind.id())
                    .expect("the Case Schema closes the relation kinds"),
                case: text(&relation["case"]).to_owned(),
            })
            .collect();
        self.resolver.push(ResolverCase {
            id,
            construction: case_id,
            reference,
            common,
            entries,
            relations,
        });
        Ok(())
    }

    /// Check each relation once every case is listed: it names a listed
    /// Resolver Case under the same construction case that pairs one of the
    /// declaring case's sides; a case whose relation does not cannot run
    /// (D15:3305).
    fn relations(&self) -> Result<(), HarnessFailure> {
        for case in &self.resolver {
            for relation in &case.relations {
                let other = self.resolver.iter().find(|other| other.id == relation.case);
                let related = other.is_some_and(|other| {
                    other.construction == case.construction
                        && other
                            .entries
                            .iter()
                            .any(|entry| case.entries.iter().any(|own| own.side == entry.side))
                });
                if !related {
                    return Err(HarnessFailure::InvalidCase(case.id.clone()));
                }
            }
        }
        Ok(())
    }

    /// List an input admission case with its fixture, once its construction
    /// is established and the parts the harness admits before the boundary
    /// are admitted (D15:3158).
    fn admission_case(&mut self, manifest: &Value) -> Result<(), HarnessFailure> {
        let (kind, definition) = input_fixture(text(&manifest["boundaryKind"]))
            .expect("the Case Schema closes the kinds");
        let id = text(&manifest["id"]).to_owned();
        let fixture = self.fixture(&manifest["inputFixture"], definition)?;
        let constructed = self
            .constructed(&manifest["resolverConstruction"])
            .filter(|constructed| admission::prepared(kind, &fixture, constructed))
            .ok_or_else(|| HarnessFailure::InvalidCase(id.clone()))?;
        self.admission.push(AdmissionCase {
            case: ListedCase {
                id,
                fixture,
                expected: manifest["expected"].clone(),
            },
            kind,
            constructed,
        });
        Ok(())
    }

    /// What the construction case that `reference` names constructs, when the
    /// suite lists that case, it constructs, and with the identity `reference`
    /// states. Each case is constructed once.
    fn constructed(&mut self, reference: &Value) -> Option<Constructed> {
        let id = text(&reference["caseId"]);
        if !self.constructions.contains_key(id) {
            let constructed = self
                .construction
                .iter()
                .find(|case| case.id == id)
                .and_then(|case| {
                    construction::constructed(&construction::input(&case.fixture, &self.bodies))
                });
            self.constructions.insert(id.to_owned(), constructed);
        }
        self.constructions[id].clone().filter(|constructed| {
            constructed.identity.text() == text(&reference["identityReference"])
        })
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
    use crate::conformance::construction_cases::{self, CONSTRUCTED};
    use crate::conformance::index::PATH;
    use crate::conformance::{admission_cases, handoff_vectors, resolver_cases};
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
        assert_eq!(paths[..BODIES.len()], BODIES);
        // Then each case's manifest, its fixture, and the bodies only the
        // fixture names, each once.
        let cases = paths[BODIES.len()..]
            .iter()
            .map(|path| (*path).to_owned())
            .collect::<BTreeSet<_>>();
        assert_eq!(cases.len(), paths.len() - BODIES.len());
        assert_eq!(cases, written_cases());
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
        assert_eq!(
            suite.open().unwrap().verified.len(),
            BODIES.len() + written_cases().len()
        );
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

    /// The committed fixture of the constructing case.
    const FIXTURE: &str = "fixtures/construction/015-construction-revision-0.json";

    /// The paths of the files the committed cases, vectors, and
    /// obligations add to the suite.
    fn written_cases() -> BTreeSet<String> {
        let construction = construction_cases::cases().unwrap().files;
        let admission = admission_cases::cases().unwrap().files;
        let resolver = resolver_cases::cases().unwrap();
        let handoff = handoff_vectors::cases(&resolver).unwrap().files;
        let obligations = obligations::cases().unwrap().files;
        construction
            .into_iter()
            .chain(admission)
            .chain(resolver.files)
            .chain(handoff)
            .chain(obligations)
            .map(|file| file.path)
            .collect()
    }

    /// A Resolver Construction Case's manifest with this ID, of the committed
    /// constructing case's fixture.
    fn construction_case(id: &str) -> Value {
        let fixture = std::fs::read(committed().join(FIXTURE)).unwrap();
        json!({
            "suiteRevision": "0",
            "manifestKind": "resolver-construction",
            "id": id,
            "constructionInputFixture": {
                "path": FIXTURE,
                "digest": file_digest(&fixture).as_str()
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

    /// Where [`Suite::admission_fixture`] writes the fixture of
    /// [`admission_case`].
    const ADMISSION_FIXTURE: &str = "fixtures/input-admission/a.json";

    /// A refused Resolver Input Admission Case's manifest with this ID, of
    /// the `safe-origin-kind` fixture at [`ADMISSION_FIXTURE`], under the
    /// construction case `015-a` with the construction vector's identity.
    fn admission_case(id: &str, fixture: &[u8]) -> Value {
        json!({
            "suiteRevision": "0",
            "manifestKind": "resolver-input-admission",
            "id": id,
            "resolverConstruction": {
                "caseId": "015-a",
                "identityReference": construction_cases::identity().unwrap()
            },
            "boundaryKind": "safe-origin-kind",
            "inputFixture": {
                "path": ADMISSION_FIXTURE,
                "digest": file_digest(fixture).as_str()
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
        /// member's array, in the order given. Listing construction cases
        /// lists no input admission case and no Resolver Case, since each
        /// committed one names a committed construction case, and listing
        /// either lists no Handoff Vector, since each committed one names a
        /// committed Resolver Case.
        fn list(&self, member: Member, manifests: &[(&str, Vec<u8>)]) {
            let entries = manifests
                .iter()
                .map(|(id, bytes)| {
                    let path = format!("cases/{}/{id}.json", member.name());
                    self.write(&path, bytes);
                    json!({ "id": id, "path": path, "digest": file_digest(bytes).as_str() })
                })
                .collect::<Vec<_>>();
            self.change_index(|index| {
                index[member.name()] = Value::Array(entries);
                if member == Member::ResolverConstructionCases {
                    index[Member::ResolverInputAdmissionCases.name()] = json!([]);
                    index[Member::ResolverCases.name()] = json!([]);
                }
                if matches!(
                    member,
                    Member::ResolverConstructionCases | Member::ResolverCases
                ) {
                    index[Member::HandoffVectors.name()] = json!([]);
                }
            });
        }

        /// Write the `safe-origin-kind` fixture of [`admission_case`], and
        /// return its bytes.
        fn admission_fixture(&self) -> Vec<u8> {
            let fixture = bytes(&json!({ "slot": "configuration-file", "candidate": "x" }));
            self.write(ADMISSION_FIXTURE, &fixture);
            fixture
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
                FIXTURE,
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
            let fixture = suite.admission_fixture();
            suite.list(admissions, &[(id, bytes(&admission_case(id, &fixture)))]);
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

    /// Where a body lies below the suite root is the index's to say: with no
    /// fixture naming its old path, nothing reads it there.
    #[test]
    fn a_body_may_lie_anywhere_below_the_suite_root() {
        let suite = Suite::new();
        suite.list(Member::ResolverConstructionCases, &[]);
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
    /// representation of its body opens, and another body does not. No case
    /// is listed, since a fixture names the committed body by its file digest.
    #[test]
    fn the_profile_specification_is_pinned_by_its_semantic_digest() {
        let pointer = "/profileSpecification/digest";
        let body: Value =
            serde_json::from_slice(&std::fs::read(committed().join(profile::PATH)).unwrap())
                .unwrap();

        let suite = Suite::new();
        suite.list(Member::ResolverConstructionCases, &[]);
        suite.replace(profile::PATH, &serde_json::to_vec(&body).unwrap(), pointer);
        assert!(suite.open().is_ok());

        let mut other = body.clone();
        other["unknown"] = json!("unknown");
        let suite = Suite::new();
        suite.list(Member::ResolverConstructionCases, &[]);
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
        suite.list(Member::ResolverConstructionCases, &[]);
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
        suite.list(Member::ResolverConstructionCases, &[]);
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
            (
                HarnessFailure::InvalidCase("015-a".to_owned()),
                "case 015-a cannot run",
            ),
            (
                HarnessFailure::UnreproducedVector("015-a".to_owned()),
                "handoff vector 015-a does not reproduce its producer",
            ),
            (
                HarnessFailure::UnmetObligation("015-XV-001".to_owned()),
                "conditional obligation 015-XV-001 is not inactive",
            ),
        ] {
            assert_eq!(failure.to_string(), message);
        }
    }

    /// The committed cases run in the order of their array, and each does
    /// what it expects.
    #[test]
    fn the_committed_cases_run_and_pass() {
        let outcomes = open(&committed().join(PATH)).unwrap().run();
        let construction = construction_cases::cases().unwrap().listing;
        let admission = admission_cases::cases().unwrap().listing;
        let mut listed = construction
            .iter()
            .chain(&admission)
            .map(|entry| entry["id"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        // A Resolver Case runs each paired entry and its duties, then the
        // pair when both entries are paired, then each relation on each side
        // both cases pair.
        let resolver = resolver_cases::cases().unwrap();
        let manifest = |path: &str| -> Value {
            let file = resolver.files.iter().find(|file| file.path == path);
            serde_json::from_slice(&file.unwrap().bytes).unwrap()
        };
        let paired = |manifest: &Value| {
            ["file", "programmatic"]
                .into_iter()
                .filter(|side| manifest["entries"][side].get("unpaired").is_none())
                .collect::<Vec<_>>()
        };
        let manifests = resolver
            .listing
            .iter()
            .map(|entry| {
                let id = entry["id"].as_str().unwrap().to_owned();
                (id, manifest(entry["path"].as_str().unwrap()))
            })
            .collect::<BTreeMap<_, _>>();
        for entry in &resolver.listing {
            let id = entry["id"].as_str().unwrap();
            let manifest = &manifests[id];
            let sides = paired(manifest);
            for side in &sides {
                listed.extend([format!("{id}/{side}"), format!("{id}/{side}/duties")]);
            }
            if sides.len() == 2 {
                listed.push(format!("{id}/pair"));
            }
            let relations = manifest["relations"].as_array().into_iter().flatten();
            for (index, relation) in relations.enumerate() {
                let other = paired(&manifests[relation["case"].as_str().unwrap()]);
                for side in sides.iter().filter(|side| other.contains(side)) {
                    listed.push(format!("{id}/relation/{index}/{side}"));
                }
            }
        }
        assert!(listed.iter().any(|id| id.contains("/relation/")));
        let ids = outcomes
            .iter()
            .map(|outcome| outcome.id.clone())
            .collect::<Vec<_>>();
        assert_eq!(ids, listed);
        assert!(ids.contains(&CONSTRUCTED.to_owned()));
        for outcome in &outcomes {
            assert!(
                outcome.passed(),
                "{}: expected {}, got {}",
                outcome.id,
                outcome.expected,
                outcome.actual
            );
        }
    }

    /// The committed suite with the Resolver Case `id` as its only case
    /// besides the construction cases, its manifest changed by `change`,
    /// which may write fixtures below the suite root.
    fn resolver_suite(id: &str, change: impl FnOnce(&Suite, &mut Value)) -> Suite {
        let suite = Suite::new();
        let path = format!("cases/resolver/{id}.json");
        let mut manifest: Value = serde_json::from_slice(&suite.read(&path)).unwrap();
        change(&suite, &mut manifest);
        suite.list(Member::ResolverCases, &[(id, bytes(&manifest))]);
        suite.change_index(|index| {
            index[Member::ResolverInputAdmissionCases.name()] = json!([]);
        });
        suite
    }

    /// Replace the fixture the manifest names at `pointer` with a copy that
    /// `change` changes, written beside it.
    fn change_fixture(
        suite: &Suite,
        manifest: &mut Value,
        pointer: &str,
        change: impl FnOnce(&mut Value),
    ) {
        let path = manifest.pointer(pointer).unwrap()["path"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut fixture: Value = serde_json::from_slice(&suite.read(&path)).unwrap();
        change(&mut fixture);
        let changed = path.replace(".json", ".changed.json");
        let bytes = bytes(&fixture);
        suite.write(&changed, &bytes);
        *manifest.pointer_mut(pointer).unwrap() =
            json!({ "path": changed, "digest": file_digest(&bytes).as_str() });
    }

    /// The outcomes of the run whose IDs name `id`.
    fn outcomes_of(suite: &Suite, id: &str) -> Vec<Outcome> {
        suite
            .open()
            .unwrap()
            .run()
            .into_iter()
            .filter(|outcome| outcome.id.starts_with(&format!("{id}/")))
            .collect()
    }

    const STOREFRONT: &str = "015-storefront-checked";

    /// A Resolver Case runs each paired entry and its duties, then the pair
    /// when both entries are paired; one that names no construction case
    /// the suite lists, one that does not construct, or another identity,
    /// or whose expected outcome names another identity, cannot run
    /// (D15:3219).
    #[test]
    fn a_resolver_case_runs_under_an_established_construction_only() {
        let invalid = Err(HarnessFailure::InvalidCase(STOREFRONT.to_owned()));
        let outcomes = outcomes_of(&resolver_suite(STOREFRONT, |_, _| {}), STOREFRONT);
        let ids = outcomes
            .iter()
            .map(|outcome| outcome.id.trim_start_matches(STOREFRONT))
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            [
                "/file",
                "/file/duties",
                "/programmatic",
                "/programmatic/duties",
                "/pair"
            ]
        );
        assert!(outcomes.iter().all(Outcome::passed));
        let file_only = "015-entry-json-invalid";
        let outcomes = outcomes_of(&resolver_suite(file_only, |_, _| {}), file_only);
        assert_eq!(outcomes.len(), 2);
        assert!(outcomes.iter().all(Outcome::passed));
        let construction = |case: &str| {
            let case = case.to_owned();
            resolver_suite(STOREFRONT, move |_, manifest| {
                manifest["resolverConstruction"]["caseId"] = json!(case);
            })
        };
        assert_eq!(construction("015-c").open(), invalid);
        assert_eq!(
            construction("015-construction-registry-package-missing").open(),
            invalid
        );
        let mut other = construction_cases::identity().unwrap().into_bytes();
        other[5] = if other[5] == b'A' { b'B' } else { b'A' };
        let other = String::from_utf8(other).unwrap();
        let stated = other.clone();
        let another = resolver_suite(STOREFRONT, move |_, manifest| {
            manifest["resolverConstruction"]["identityReference"] = json!(stated);
        });
        assert_eq!(another.open(), invalid);
        let expected = resolver_suite(STOREFRONT, move |suite, manifest| {
            change_fixture(
                suite,
                manifest,
                "/entries/programmatic/expectedOutcomeFixture",
                |outcome| outcome["resolverConstructionIdentity"] = json!(other),
            );
        });
        assert_eq!(expected.open(), invalid);
    }

    /// A pair whose entries reach two Materialized Resolution Input Sets
    /// agrees in its construction identity only: each entry passes against
    /// its own expectation, and the pair passes (D15:3233).
    #[test]
    fn a_pair_that_reaches_two_sets_agrees_in_its_identity_only() {
        let default = "cases/resolver/015-locale-default-not-in-project.json";
        let suite = resolver_suite(STOREFRONT, |suite, manifest| {
            let other: Value = serde_json::from_slice(&suite.read(default)).unwrap();
            manifest["entries"]["programmatic"] = other["entries"]["programmatic"].clone();
        });
        let outcomes = outcomes_of(&suite, STOREFRONT);
        assert_eq!(outcomes.len(), 5);
        assert!(outcomes.iter().all(Outcome::passed), "{outcomes:?}");
    }

    /// Each side of a Resolver Case submits through its own adapter: a file
    /// entry fixture on the programmatic side, or a programmatic one on the
    /// file side, cannot run, so no adapter stands in for the other
    /// (D15:3233).
    #[test]
    fn each_side_submits_through_its_own_adapter() {
        let invalid = Err(HarnessFailure::InvalidCase(STOREFRONT.to_owned()));
        for (from, to) in [("file", "programmatic"), ("programmatic", "file")] {
            let suite = resolver_suite(STOREFRONT, |_, manifest| {
                manifest["entries"][to] = manifest["entries"][from].clone();
            });
            assert_eq!(suite.open(), invalid, "{from} on the {to} side");
        }
    }

    /// The committed suite with the Resolver Cases `ids` as its only cases
    /// besides the construction cases, each manifest changed by `change`.
    fn related_suite(ids: &[&str], change: impl Fn(&Suite, &str, &mut Value)) -> Suite {
        let suite = Suite::new();
        let mut manifests = ids
            .iter()
            .map(|id| {
                let path = format!("cases/resolver/{id}.json");
                let mut manifest: Value = serde_json::from_slice(&suite.read(&path)).unwrap();
                change(&suite, id, &mut manifest);
                (*id, bytes(&manifest))
            })
            .collect::<Vec<_>>();
        manifests.sort_by_key(|(id, _)| *id);
        suite.list(Member::ResolverCases, &manifests);
        suite.change_index(|index| {
            index[Member::ResolverInputAdmissionCases.name()] = json!([]);
        });
        suite
    }

    /// A relation names a listed Resolver Case under the same construction
    /// case, which pairs one of the declaring case's sides; otherwise the
    /// declaring case cannot run (D15:3305).
    #[test]
    fn a_relation_names_a_listed_case_under_one_construction_on_a_shared_side() {
        const ADDED: &str = "015-locale-requested-added";
        let invalid = Err(HarnessFailure::InvalidCase(ADDED.to_owned()));
        let opened = related_suite(&[STOREFRONT, ADDED], |_, _, _| {}).open();
        assert_eq!(opened.err(), None);
        assert_eq!(related_suite(&[ADDED], |_, _, _| {}).open(), invalid);
        // The storefront under a copy of the constructing case, which builds
        // the same resolver under another ID.
        let copy = "015-construction-copy";
        let suite = related_suite(&[STOREFRONT, ADDED], |suite, id, manifest| {
            if id == STOREFRONT {
                manifest["resolverConstruction"]["caseId"] = json!(copy);
                let path = format!("cases/construction/{CONSTRUCTED}.json");
                let mut construction: Value = serde_json::from_slice(&suite.read(&path)).unwrap();
                construction["id"] = json!(copy);
                let written = bytes(&construction);
                let path = format!("cases/construction/{copy}.json");
                suite.write(&path, &written);
                let digest = file_digest(&written);
                suite.change_index(|index| {
                    let entry = json!({ "id": copy, "path": path, "digest": digest.as_str() });
                    let cases = index[Member::ResolverConstructionCases.name()]
                        .as_array_mut()
                        .unwrap();
                    cases.push(entry);
                    cases.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
                });
            }
        });
        assert_eq!(suite.open(), invalid);
        // The storefront pairs only the file side, the declaring case only
        // the programmatic one.
        let suite = related_suite(&[STOREFRONT, ADDED], |_, id, manifest| {
            let (side, reason) = if id == STOREFRONT {
                ("programmatic", "raw-json-syntax")
            } else {
                ("file", "programmatic-entry-snapshot")
            };
            manifest["entries"][side] = json!({
                "unpaired": { "reason": reason, "verificationOwner": "intlify_config" }
            });
        });
        assert_eq!(suite.open(), invalid);
    }

    /// A relation that does not hold fails its outcome on each side both
    /// cases pair, beside the relation's case and kind, while every other
    /// outcome still passes (D15:3305).
    #[test]
    fn a_relation_that_does_not_hold_fails_on_each_side() {
        const ADDED: &str = "015-locale-requested-added";
        let suite = related_suite(&[STOREFRONT, ADDED], |_, id, manifest| {
            if id == ADDED {
                manifest["relations"][0]["kind"] = json!("profile-equal");
            }
        });
        let outcomes = suite.open().unwrap().run();
        let failed = outcomes
            .iter()
            .filter(|outcome| !outcome.passed())
            .collect::<Vec<_>>();
        let ids = failed
            .iter()
            .map(|outcome| outcome.id.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            [
                format!("{ADDED}/relation/0/file"),
                format!("{ADDED}/relation/0/programmatic")
            ]
        );
        for outcome in failed {
            let view = |holds: bool| json!({ "case": STOREFRONT, "kind": "profile-equal", "holds": holds });
            assert_eq!(outcome.expected, view(true));
            assert_eq!(outcome.actual, view(false));
        }
    }

    /// A Resolver Case's entry runs only on inputs its boundaries admit as
    /// its fixtures give them: each candidate the ID its slot fixes, each
    /// input admitted, and each locator canonical; otherwise the case cannot
    /// run (D15:3252).
    #[test]
    fn a_resolver_case_s_inputs_are_admitted_as_given() {
        let invalid = Err(HarnessFailure::InvalidCase(STOREFRONT.to_owned()));
        let entry = |side: &'static str, change: fn(&mut Value)| {
            resolver_suite(STOREFRONT, move |suite, manifest| {
                let pointer = format!("/entries/{side}/entryAdmissionInputFixture");
                change_fixture(suite, manifest, &pointer, change);
            })
        };
        let locator = entry("file", |fixture| {
            fixture["configuration-file"]["input"]["locator"]["payload"] =
                json!("./intlify.config.json");
        });
        assert_eq!(locator.open(), invalid);
        let identity = entry("programmatic", |fixture| {
            let input = &mut fixture["configuration-programmatic"]["input"];
            input.as_object_mut().unwrap().remove("identity");
        });
        assert_eq!(identity.open(), invalid);
        let candidate = resolver_suite(STOREFRONT, |suite, manifest| {
            change_fixture(
                suite,
                manifest,
                "/commonInvocationInputFixture",
                |fixture| {
                    fixture["implementation-capability"]["candidate"] =
                        json!("intent-surface-class-vocabulary");
                },
            );
        });
        assert_eq!(candidate.open(), invalid);
    }

    /// A Resolver Case's fixtures are verified before any case runs: each
    /// has its declared digest and the form the Case Schema gives it.
    #[test]
    fn a_resolver_case_s_fixtures_are_verified_before_any_case_runs() {
        let digest = resolver_suite(STOREFRONT, |_, manifest| {
            manifest["entries"]["file"]["expectedOutcomeFixture"]["digest"] = other_digest();
        });
        let path = format!("fixtures/resolver/{STOREFRONT}.file-expected.json");
        assert_eq!(digest.open(), Err(HarnessFailure::DigestMismatch(path)));
        for pointer in [
            "/commonInvocationInputFixture",
            "/entries/file/entryAdmissionInputFixture",
            "/entries/programmatic/expectedOutcomeFixture",
        ] {
            let suite = resolver_suite(STOREFRONT, |suite, manifest| {
                change_fixture(suite, manifest, pointer, |fixture| {
                    fixture["unknown"] = json!(true);
                });
            });
            let changed = suite.open();
            let named = |path: &String| path.ends_with(".changed.json");
            assert!(
                matches!(&changed, Err(HarnessFailure::SchemaInvalid(path)) if named(path)),
                "{pointer}: {changed:?}"
            );
        }
    }

    /// An entry whose expected outcome the resolver does not meet opens and
    /// runs, and only that entry's outcome does not pass.
    #[test]
    fn an_entry_whose_outcome_differs_does_not_pass() {
        let suite = resolver_suite(STOREFRONT, |suite, manifest| {
            change_fixture(
                suite,
                manifest,
                "/entries/file/expectedOutcomeFixture",
                |outcome| {
                    outcome["resolutionEvidence"]["shared"]["evaluationStatus"]
                        .as_array_mut()
                        .unwrap()
                        .pop();
                },
            );
        });
        let failed = outcomes_of(&suite, STOREFRONT)
            .into_iter()
            .filter(|outcome| !outcome.passed())
            .map(|outcome| outcome.id)
            .collect::<Vec<_>>();
        assert_eq!(failed, [format!("{STOREFRONT}/file")]);
    }

    /// A case whose expectation construction does not meet opens and runs,
    /// and does not pass: a conformance failure, not a harness failure.
    #[test]
    fn a_case_construction_does_not_meet_does_not_pass() {
        let suite = Suite::new();
        let id = "015-construction-a";
        suite.list(
            Member::ResolverConstructionCases,
            &[(id, bytes(&construction_case(id)))],
        );
        let outcomes = suite.open().unwrap().run();
        assert_eq!(outcomes.len(), 1);
        assert!(!outcomes[0].passed());
        assert_eq!(outcomes[0].expected["constructed"], false);
        assert_eq!(outcomes[0].actual["constructed"], true);
    }

    /// A construction case's fixture is verified before any case runs: it
    /// has its declared digest and is strict UTF-8 JSON of the form the Case
    /// Schema gives it, and each body it references has the digest it
    /// declares, a body already verified the one it was verified with.
    #[test]
    fn a_construction_fixture_is_verified_before_any_case_runs() {
        let cases = Member::ResolverConstructionCases;
        let path = "fixtures/construction/a.json";
        let open_with = |fixture: &[u8]| {
            let suite = Suite::new();
            suite.write(path, fixture);
            let mut manifest = construction_case("015-a");
            manifest["constructionInputFixture"] =
                json!({ "path": path, "digest": file_digest(fixture).as_str() });
            suite.list(cases, &[("015-a", bytes(&manifest))]);
            suite.open()
        };
        let fixture: Value =
            serde_json::from_slice(&std::fs::read(committed().join(FIXTURE)).unwrap()).unwrap();
        assert!(open_with(&bytes(&fixture)).is_ok());
        assert_eq!(
            open_with(b"{"),
            Err(HarnessFailure::Malformed(path.to_owned()))
        );
        for change in [
            |fixture: &mut Value| fixture["unknown"] = json!({}),
            |fixture: &mut Value| {
                fixture["profileSpecificationSchema"]["bytes"] = json!({ "bytes": "0" });
            },
            |fixture: &mut Value| {
                fixture["registryPackage"]["members"]["unknown-registry"] = json!({ "bytes": "" });
            },
        ] {
            let mut changed = fixture.clone();
            change(&mut changed);
            assert_eq!(
                open_with(&bytes(&changed)),
                Err(HarnessFailure::SchemaInvalid(path.to_owned()))
            );
        }
        let schema = fixture["profileSpecificationSchema"]["bytes"]["path"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut elsewhere = fixture.clone();
        elsewhere["profileSpecificationSchema"]["bytes"]["path"] = json!("nowhere.json");
        assert_eq!(
            open_with(&bytes(&elsewhere)),
            Err(HarnessFailure::Missing("nowhere.json".to_owned()))
        );
        let mut other = fixture;
        other["profileSpecificationSchema"]["bytes"]["digest"] = other_digest();
        assert_eq!(
            open_with(&bytes(&other)),
            Err(HarnessFailure::DigestMismatch(schema))
        );
        let listed = |change: &dyn Fn(&Suite)| {
            let suite = Suite::new();
            suite.list(cases, &[("015-a", bytes(&construction_case("015-a")))]);
            change(&suite);
            suite.open()
        };
        assert_eq!(
            listed(&|suite| suite.remove(FIXTURE)),
            Err(HarnessFailure::Missing(FIXTURE.to_owned()))
        );
        assert_eq!(
            listed(&|suite| {
                let mut changed = suite.read(FIXTURE);
                changed.push(b'\n');
                suite.write(FIXTURE, &changed);
            }),
            Err(HarnessFailure::DigestMismatch(FIXTURE.to_owned()))
        );
    }

    /// A suite whose only cases are one construction case `015-a` of the
    /// committed fixture at `construction`, and the input admission case
    /// `015-b` of the fixture `fixture`, its manifest changed by `change`.
    fn admission_suite(construction: &str, fixture: &Value, change: impl Fn(&mut Value)) -> Suite {
        let suite = Suite::new();
        let mut construction_manifest = construction_case("015-a");
        let construction_bytes = std::fs::read(committed().join(construction)).unwrap();
        construction_manifest["constructionInputFixture"] = json!({
            "path": construction,
            "digest": file_digest(&construction_bytes).as_str(),
        });
        suite.list(
            Member::ResolverConstructionCases,
            &[("015-a", bytes(&construction_manifest))],
        );
        let fixture = bytes(fixture);
        suite.write(ADMISSION_FIXTURE, &fixture);
        let mut manifest = admission_case("015-b", &fixture);
        change(&mut manifest);
        suite.list(
            Member::ResolverInputAdmissionCases,
            &[("015-b", bytes(&manifest))],
        );
        suite
    }

    /// An input admission case runs under the construction case it names,
    /// once the suite lists that case, it constructs, and with the identity
    /// the case states; a case without it cannot run, rather than expecting
    /// a boundary's failure (D15:3158).
    #[test]
    fn an_admission_case_runs_under_an_established_construction_only() {
        let origin = json!({ "slot": "configuration-file", "candidate": "configuration-file" });
        let invalid = Err(HarnessFailure::InvalidCase("015-b".to_owned()));
        let outcomes = admission_suite(FIXTURE, &origin, |_| {})
            .open()
            .unwrap()
            .run();
        assert_eq!(outcomes.len(), 2);
        assert_eq!(outcomes[1].id, "015-b");
        assert_eq!(
            outcomes[1].actual["normalizedInput"],
            json!({ "safeOriginKind": "configuration-file" })
        );
        let unlisted = admission_suite(FIXTURE, &origin, |manifest| {
            manifest["resolverConstruction"]["caseId"] = json!("015-c");
        });
        assert_eq!(unlisted.open(), invalid);
        let mut other = construction_cases::identity().unwrap().into_bytes();
        other[5] = if other[5] == b'A' { b'B' } else { b'A' };
        let other = String::from_utf8(other).unwrap();
        let another = admission_suite(FIXTURE, &origin, |manifest| {
            manifest["resolverConstruction"]["identityReference"] = json!(other);
        });
        assert_eq!(another.open(), invalid);
        let refused = "fixtures/construction/015-construction-registry-package-missing.json";
        assert_eq!(admission_suite(refused, &origin, |_| {}).open(), invalid);
    }

    /// A case whose capability value, or whose programmatic pair's canonical
    /// Snapshot, the harness refuses before the boundary cannot run
    /// (D15:3162, 3252).
    #[test]
    fn an_admission_case_whose_prepared_part_is_refused_cannot_run() {
        let invalid = Err(HarnessFailure::InvalidCase("015-b".to_owned()));
        let kind = |name: &'static str| {
            move |manifest: &mut Value| {
                manifest["boundaryKind"] = json!(name);
                manifest["expected"]["failureReason"] = json!(match name {
                    "external-profile-selector-binding" => "unsafe-host-inspection",
                    _ => "missing-entry-identity",
                });
            }
        };
        let selector = |capability: Value| json!({ "capability": { "json": capability }, "input": { "report": "unsafe" } });
        let suite = admission_suite(
            FIXTURE,
            &selector(admission_cases::capability()),
            kind("external-profile-selector-binding"),
        );
        assert!(suite.open().is_ok());
        let suite = admission_suite(
            FIXTURE,
            &selector(json!({})),
            kind("external-profile-selector-binding"),
        );
        assert_eq!(suite.open(), invalid);
        let entry = |digest: String| {
            json!({
                "capability": { "json": admission_cases::capability() },
                "input": {
                    "nonSecret": true,
                    "snapshot": { "snapshot": { "null": null }, "digest": digest },
                },
            })
        };
        let null = "sha256:1dcd2e02d5702083fb37800e12fca137b28d2b85ccddc89f378a06638607db7e";
        let suite = admission_suite(
            FIXTURE,
            &entry(null.to_owned()),
            kind("programmatic-entry-identity-revision"),
        );
        assert!(suite.open().is_ok());
        let suite = admission_suite(
            FIXTURE,
            &entry(format!("sha256:{}", "0".repeat(64))),
            kind("programmatic-entry-identity-revision"),
        );
        assert_eq!(suite.open(), invalid);
    }

    /// An input admission case's fixture is verified before any case runs:
    /// it has its declared digest and is strict UTF-8 JSON of its boundary
    /// kind's form, and each body it references has the digest it declares.
    #[test]
    fn an_admission_fixture_is_verified_before_any_case_runs() {
        let open_with = |fixture: &Value| admission_suite(FIXTURE, fixture, |_| {}).open();
        assert!(open_with(&json!({ "slot": "configuration-file" })).is_ok());
        assert_eq!(
            open_with(&json!({ "slot": "configuration-file", "unknown": 1 })),
            Err(HarnessFailure::SchemaInvalid(ADMISSION_FIXTURE.to_owned()))
        );
        let suite = admission_suite(FIXTURE, &json!({ "slot": "configuration-file" }), |_| {});
        suite.write(ADMISSION_FIXTURE, b"{");
        assert_eq!(
            suite.open(),
            Err(HarnessFailure::DigestMismatch(ADMISSION_FIXTURE.to_owned()))
        );
        let suite = admission_suite(
            FIXTURE,
            &json!({ "slot": "configuration-file" }),
            |manifest| {
                manifest["inputFixture"]["digest"] = json!(file_digest(b"{").as_str());
            },
        );
        suite.write(ADMISSION_FIXTURE, b"{");
        assert_eq!(
            suite.open(),
            Err(HarnessFailure::Malformed(ADMISSION_FIXTURE.to_owned()))
        );
        let reference = |digest: &str| {
            json!({
                "capability": { "json": admission_cases::capability() },
                "input": { "path": PATH, "digest": digest },
            })
        };
        let index_digest = |suite: &Suite| file_digest(&suite.read(PATH)).as_str().to_owned();
        let probe = Suite::new();
        let digest = index_digest(&probe);
        let suite = admission_suite(FIXTURE, &reference(&digest), |manifest| {
            manifest["boundaryKind"] = json!("profile-resolution-specification-input");
            manifest["expected"]["failureReason"] = json!("malformed-input");
        });
        assert_eq!(
            suite.open(),
            Err(HarnessFailure::DigestMismatch(PATH.to_owned()))
        );
    }

    /// An admission step's case runs though the capability it holds is
    /// refused: the refusal is the step's own result, not an invalid case
    /// (D15:3166, 3168).
    #[test]
    fn a_step_case_runs_under_a_refused_capability() {
        let path =
            committed().join("fixtures/admission/015-admission-entry-file-refused-capability.json");
        let fixture: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let suite = admission_suite(FIXTURE, &fixture, |manifest| {
            manifest["boundaryKind"] = json!("entry-admission-input-set");
            manifest["expected"] = json!({
                "admitted": false,
                "normalizedInput": null,
                "failures": [],
                "resolverInvoked": false,
            });
        });
        let outcomes = suite.open().unwrap().run();
        let step = outcomes
            .iter()
            .find(|outcome| outcome.id == "015-b")
            .unwrap();
        assert!(step.passed(), "{}", step.actual);
    }

    /// A programmatic entry's call-site number past binary64's range, which
    /// the Case Schema admits as a number, reaches the entry step as the
    /// infinity a binding reads; under a refused capability the step
    /// evaluates only the origins (D15:3168, 3252).
    #[test]
    fn a_step_case_reads_a_call_site_number_past_binary64_s_range() {
        let path =
            committed().join("fixtures/admission/015-admission-entry-programmatic-admitted.json");
        let mut fixture: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        fixture["capability"] = json!({ "json": {} });
        let mappings =
            &mut fixture["entry"]["configuration-programmatic"]["input"]["callSite"]["mappings"];
        mappings[0]["span"]["start"] = serde_json::from_str("1e999").unwrap();
        mappings[1]["path"] = serde_json::from_str(r#"[{ "index": -1e999 }]"#).unwrap();
        let suite = admission_suite(FIXTURE, &fixture, |manifest| {
            manifest["boundaryKind"] = json!("entry-admission-input-set");
            manifest["expected"] = json!({
                "admitted": false,
                "normalizedInput": null,
                "failures": [],
                "resolverInvoked": false,
            });
        });
        let outcomes = suite.open().unwrap().run();
        let step = outcomes
            .iter()
            .find(|outcome| outcome.id == "015-b")
            .unwrap();
        assert!(step.passed(), "{}", step.actual);
    }

    /// The committed suite with the manifest `id` of `member` changed by
    /// `change`, which may write fixtures below the suite root, and its new
    /// digest declared.
    fn changed_manifest(
        member: Member,
        id: &str,
        change: impl FnOnce(&Suite, &mut Value),
    ) -> Suite {
        let suite = Suite::new();
        let index: Value = serde_json::from_slice(&suite.read(PATH)).unwrap();
        let entries = index[member.name()].as_array().unwrap();
        let at = entries.iter().position(|entry| entry["id"] == id).unwrap();
        let path = entries[at]["path"].as_str().unwrap().to_owned();
        let mut manifest: Value = serde_json::from_slice(&suite.read(&path)).unwrap();
        change(&suite, &mut manifest);
        let pointer = format!("/{}/{at}/digest", member.name());
        suite.replace(&path, &bytes(&manifest), &pointer);
        suite
    }

    /// The first vector by ID, which the storefront produces.
    const VECTOR: &str = "015-handoff-group-selection-one-group-group-id";

    /// A change to a vector, beside its producer's expected outcome, which
    /// it may copy from.
    type Change = fn(&mut Value, &Value);

    /// A Handoff Vector reproduces its producer: the facts are whole groups
    /// of each expected outcome of a listed checked Resolver Case, and the
    /// bindings exactly the rows they reference, in the outcome's order
    /// (D15:3330, 3348).
    #[test]
    fn a_handoff_vector_reproduces_its_producer() {
        let vectors = Member::HandoffVectors;
        let unreproduced = Err(HarnessFailure::UnreproducedVector(VECTOR.to_owned()));
        let expected: Value = serde_json::from_slice(
            &std::fs::read(committed().join(resolver_cases::expected_path(STOREFRONT, "file")))
                .unwrap(),
        )
        .unwrap();
        // Another group, with the rows it references, is reproduced too.
        let suite = changed_manifest(vectors, VECTOR, |_, vector| {
            let facts = &mut vector["producerProfileFacts"];
            facts["requiredPolicies"] = expected["profile"]["requiredPolicies"].clone();
            let groups = facts
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            let (_, bindings) = produced(&expected, groups.iter().map(String::as_str)).unwrap();
            vector["producerArtifactBindings"] = bindings;
        });
        assert!(suite.open().is_ok());
        let changes: [(&str, Change); 7] = [
            ("a member out of order", |vector, _| {
                vector["producerProfileFacts"]["deploymentGroups"]["web"]["members"] =
                    json!(["server", "browser"]);
            }),
            ("a group without its rows", |vector, expected| {
                vector["producerProfileFacts"]["requiredPolicies"] =
                    expected["profile"]["requiredPolicies"].clone();
            }),
            ("a row missing", |vector, _| {
                vector["producerArtifactBindings"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
            }),
            ("the rows out of order", |vector, _| {
                vector["producerArtifactBindings"]
                    .as_array_mut()
                    .unwrap()
                    .reverse();
            }),
            ("a blocked producer", |vector, _| {
                vector["producerCaseId"] = json!("015-locale-default-not-in-project");
            }),
            ("another checked producer", |vector, _| {
                vector["producerCaseId"] = json!(resolver_cases::TWO_GROUPS);
            }),
            ("an unlisted producer", |vector, _| {
                vector["producerCaseId"] = json!("015-unlisted");
            }),
        ];
        for (change, apply) in changes {
            let suite = changed_manifest(vectors, VECTOR, |_, vector| apply(vector, &expected));
            assert_eq!(suite.open(), unreproduced, "{change}");
        }
        // Each expected outcome must reproduce it, the programmatic one too.
        let suite = changed_manifest(Member::ResolverCases, STOREFRONT, |suite, manifest| {
            let pointer = "/entries/programmatic/expectedOutcomeFixture";
            change_fixture(suite, manifest, pointer, |outcome| {
                outcome["profile"]["targets"]["browser"]["requestedLocales"] = json!(["ja"]);
            });
        });
        assert_eq!(suite.open(), unreproduced);
    }

    /// A conditional obligation is inactive: an active one, evaluated or
    /// declared, would need cases its row names, which no revision-"0"
    /// manifest can name (D15:3371).
    #[test]
    fn a_conditional_obligation_is_inactive_while_nothing_activates_it() {
        for id in ["015-XV-001", "015-XV-002", "015-AD-001", "015-HV-013"] {
            let suite = changed_manifest(Member::ConditionalObligations, id, |_, obligation| {
                obligation["activationState"] = json!("active");
                obligation
                    .as_object_mut()
                    .unwrap()
                    .remove("notApplicableReason");
            });
            assert_eq!(
                suite.open(),
                Err(HarnessFailure::UnmetObligation(id.to_owned())),
                "{id}"
            );
        }
    }
}
