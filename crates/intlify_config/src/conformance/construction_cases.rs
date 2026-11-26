// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The suite's Resolver Construction Cases: one that constructs from the
//! committed input, and one for each row of the construction-failure
//! precedence, refused with that row's reason (D15:3035-3105).
//!
//! Each fixture is a complete five-component Resolver Construction Input Set
//! (D15:3252), changed only where its row looks, so that every row before it
//! holds (D15:3060). A body the case does not change is a reference to the
//! committed file below the suite root; one it changes is a file of its own
//! beside the fixture, or its bytes in lowercase hexadecimal when they are
//! not strict UTF-8 JSON (D15:3243-3248). A failing case expects its row's
//! reason, and the construction-root bounds' row also the bound the input
//! exceeds. The constructing case expects the identity of the committed
//! construction vector, which an independent checker derives, so no case's
//! expectation is what the construction it checks computes.
//!
//! Each case traces the decisions that state its rule and, as its rule ID,
//! the row's `015.construction.<component>.<check>` (D15:2174); the
//! constructing case traces every row's.

use serde_json::{json, Map, Value};

use crate::construction::admission::{ConstructionBoundId, DRAFT_7};
use crate::construction::component::{file_digest, strict_json};
use crate::construction::failure::{PrecedenceRow, PRECEDENCE};
use crate::construction::generate::{self, Artifact};
use crate::construction::registry::{self, RegistryRole};
use crate::construction::{authority_set, capability, profile};
use crate::model::CONFIGURATION_SCHEMA_VERSION;
use crate::resolver::evidence_source::hex;

use super::case::{ManifestKind, REVISION};
use super::index::SuiteFile;

/// The case that constructs from the committed input.
pub(crate) const CONSTRUCTED: &str = "015-construction-revision-0";

/// Where the manifests lie below the suite root.
const CASES: &str = "cases/construction";

/// Where the fixtures, and the bodies they keep in files, lie below the suite
/// root.
const FIXTURES: &str = "fixtures/construction";

/// The construction vectors, whose identity the constructing case expects.
const VECTORS: &[u8] =
    include_bytes!("../../conformance/construction/construction-vectors-v0.json");

/// The construction-root bounds' `input.maxDepth`, which the bounds' case
/// exceeds by one.
const MAX_DEPTH: usize = 64;

/// The cases this crate writes: their files, and the index entries that list
/// their manifests.
pub(crate) struct Cases {
    pub(crate) files: Vec<SuiteFile>,
    pub(crate) listing: Vec<Value>,
}

/// One of the five components, in the order of the input set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Schema,
    Specification,
    Set,
    Capability,
    Package,
}

impl Part {
    const ALL: [Self; 5] = [
        Self::Schema,
        Self::Specification,
        Self::Set,
        Self::Capability,
        Self::Package,
    ];

    /// The component's member of the fixture (D15:3252).
    const fn name(self) -> &'static str {
        match self {
            Self::Schema => "profileSpecificationSchema",
            Self::Specification => "profileSpecification",
            Self::Set => "configurationSchemaAuthoritySet",
            Self::Capability => "resolverCapabilitySpecification",
            Self::Package => "registryPackage",
        }
    }

    /// The name of the file a changed body of the component is kept in.
    const fn file(self) -> &'static str {
        match self {
            Self::Schema => "profile-specification-schema",
            Self::Specification => "profile-specification",
            Self::Set => "configuration-schema-authority-set",
            Self::Capability => "resolver-capability-specification",
            Self::Package => "registry-package",
        }
    }
}

/// One body of the input: the committed file at its path, or bytes the case
/// changed.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Body {
    Committed(String, Vec<u8>),
    Changed(Vec<u8>),
}

impl Body {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Committed(_, bytes) | Self::Changed(bytes) => bytes,
        }
    }
}

/// One component: its declared tuple, its body, and a container's members.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Component {
    tuple: Value,
    body: Body,
    members: Option<Vec<(String, Body)>>,
}

impl Component {
    /// A component declaring `identity`, `revision`, and the file digest of
    /// the committed `artifact`.
    fn committed(identity: &str, revision: &str, artifact: Artifact) -> Self {
        Self {
            tuple: json!({
                "identity": identity,
                "revision": revision,
                "digest": file_digest(&artifact.bytes).as_str(),
            }),
            body: Body::Committed(artifact.path.to_owned(), artifact.bytes),
            members: None,
        }
    }

    /// Declare the file digest of the body's bytes, as a component whose
    /// digest check holds does.
    fn declare(&mut self) {
        self.tuple["digest"] = json!(file_digest(self.body.bytes()).as_str());
    }

    /// Change the body's value and declare its new bytes.
    fn edit(&mut self, change: impl FnOnce(&mut Value)) {
        self.body = Body::Changed(edited(self.body.bytes(), change));
        self.declare();
    }

    /// A container's member.
    fn member(&mut self, key: &str) -> &mut Body {
        self.members
            .iter_mut()
            .flatten()
            .find(|(name, _)| name == key)
            .map(|(_, body)| body)
            .expect("the container has the member")
    }

    /// Change a container's record of a member and declare the container's
    /// new bytes.
    fn record(&mut self, key: &str, change: impl FnOnce(&mut Value)) {
        self.edit(|body| change(&mut body["members"][key]));
    }

    /// Give a container's member `bytes`, and record their file digest.
    fn replace(&mut self, key: &str, bytes: Vec<u8>) {
        let digest = file_digest(&bytes);
        *self.member(key) = Body::Changed(bytes);
        self.record(key, |record| record["digest"] = json!(digest.as_str()));
    }
}

/// The input of one case: each component, `None` when absent.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Input([Option<Component>; 5]);

impl Input {
    /// The committed input: every committed authority and member, each
    /// declared as construction admits it.
    fn committed() -> Result<Self, serde_json::Error> {
        let [profile_schema, profile, set, mapped, capability] =
            <[Artifact; 5]>::try_from(generate::authorities()?)
                .unwrap_or_else(|_| unreachable!("five authorities are written"));
        let [origin_schema, origins, check_schema, checks, finding_schema, findings, container] =
            <[Artifact; 7]>::try_from(generate::registries()?)
                .unwrap_or_else(|_| unreachable!("six members and the container are written"));
        let mut specification = Component::committed(profile::IDENTITY, profile::REVISION, profile);
        let semantic = generate::authority_pins()?.profile_specification_semantic;
        specification.tuple["semanticDigest"] = json!(semantic);
        let mut set = Component::committed(authority_set::IDENTITY, authority_set::REVISION, set);
        set.members = Some(vec![(
            CONFIGURATION_SCHEMA_VERSION.to_owned(),
            Body::Committed(mapped.path.to_owned(), mapped.bytes),
        )]);
        let mut package = Component::committed(registry::IDENTITY, registry::REVISION, container);
        let members = [
            origin_schema,
            check_schema,
            finding_schema,
            origins,
            checks,
            findings,
        ];
        package.members = Some(
            RegistryRole::ALL
                .into_iter()
                .zip(members)
                .map(|(role, member)| {
                    let body = Body::Committed(member.path.to_owned(), member.bytes);
                    (role.name().to_owned(), body)
                })
                .collect(),
        );
        Ok(Self([
            Some(Component::committed(
                profile::SCHEMA_IDENTITY,
                profile::REVISION,
                profile_schema,
            )),
            Some(specification),
            Some(set),
            Some(Component::committed(
                capability::IDENTITY,
                capability::REVISION,
                capability,
            )),
            Some(package),
        ]))
    }

    fn part(&mut self, part: Part) -> &mut Component {
        self.0[part as usize]
            .as_mut()
            .expect("the committed input has every component")
    }

    fn remove(&mut self, part: Part) {
        self.0[part as usize] = None;
    }
}

/// `bytes` read as JSON, changed, and written again.
fn edited(bytes: &[u8], change: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut value = strict_json(bytes).expect("a committed body is strict JSON");
    change(&mut value);
    generate::write(&value).expect("a JSON value serializes")
}

/// A digest that is no body's this input has.
fn other() -> Value {
    json!(file_digest(b"other").as_str())
}

/// A Draft 7 schema that the Draft 7 meta-schema refuses.
fn invalid_schema() -> Vec<u8> {
    generate::write(&json!({ "$schema": DRAFT_7, "type": 5 })).expect("a JSON value serializes")
}

/// Bytes that are no JSON value.
const NOT_JSON: &[u8] = b"{";

/// For each row of the construction-failure precedence, in order, the change
/// of the committed input that the row refuses and no row before it does.
#[rustfmt::skip]
fn changes() -> [fn(&mut Input); 43] {
    use Part::{Capability, Package, Schema, Set, Specification};
    const ORIGINS: &str = "safe-origin-kind-registry";
    const FINDINGS: &str = "finding-registry";
    const MAPPED: &str = CONFIGURATION_SCHEMA_VERSION;
    [
        // 1: an array nested past the construction root's depth.
        |input| {
            let open = "[".repeat(MAX_DEPTH + 1);
            let close = "]".repeat(MAX_DEPTH + 1);
            let schema = input.part(Schema);
            schema.body = Body::Changed(format!("{open}{close}\n").into_bytes());
            schema.declare();
        },
        // 2-7: the Profile Specification Schema.
        |input| input.remove(Schema),
        |input| input.part(Schema).body = Body::Changed(NOT_JSON.to_vec()),
        |input| input.part(Schema).tuple["identity"] = json!("intlify-other"),
        |input| input.part(Schema).tuple["revision"] = json!("1"),
        |input| input.part(Schema).tuple["digest"] = other(),
        |input| {
            let schema = input.part(Schema);
            schema.body = Body::Changed(invalid_schema());
            schema.declare();
        },
        // 8-14: the Profile Specification.
        |input| input.remove(Specification),
        |input| input.part(Specification).body = Body::Changed(NOT_JSON.to_vec()),
        |input| input.part(Specification).tuple["identity"] = json!("intlify-other"),
        |input| input.part(Specification).tuple["revision"] = json!("1"),
        |input| input.part(Specification).tuple["digest"] = other(),
        |input| {
            let specification = input.part(Specification);
            specification.edit(|body| *body = json!({}));
            let semantic = profile::semantic_digest(&json!({})).expect("{} has no number");
            specification.tuple["semanticDigest"] = json!(semantic.as_str());
        },
        |input| input.part(Specification).tuple["semanticDigest"] = other(),
        // 15-19: the Configuration Schema Authority Set.
        |input| input.remove(Set),
        |input| input.part(Set).body = Body::Changed(NOT_JSON.to_vec()),
        |input| input.part(Set).tuple["identity"] = json!("intlify-other"),
        |input| input.part(Set).tuple["revision"] = json!("1"),
        |input| input.part(Set).tuple["digest"] = other(),
        // 20-24: the configuration schema it maps.
        |input| input.part(Set).record(MAPPED, |record| record["identity"] = json!("intlify-other")),
        |input| input.part(Set).record(MAPPED, |record| record["revision"] = json!("1")),
        |input| input.part(Set).record(MAPPED, |record| {
            record["dialect"] = json!("https://json-schema.org/draft/2020-12/schema");
        }),
        |input| input.part(Set).record(MAPPED, |record| record["digest"] = other()),
        |input| input.part(Set).replace(MAPPED, invalid_schema()),
        // 25-30: the Resolver Capability Specification.
        |input| input.remove(Capability),
        |input| input.part(Capability).body = Body::Changed(NOT_JSON.to_vec()),
        |input| input.part(Capability).tuple["identity"] = json!("intlify-other"),
        |input| input.part(Capability).tuple["revision"] = json!("1"),
        |input| input.part(Capability).tuple["digest"] = other(),
        |input| input.part(Capability).edit(|body| *body = json!({})),
        // 31-35: the Registry Package.
        |input| input.remove(Package),
        |input| input.part(Package).body = Body::Changed(NOT_JSON.to_vec()),
        |input| input.part(Package).tuple["identity"] = json!("intlify-other"),
        |input| input.part(Package).tuple["revision"] = json!("1"),
        |input| input.part(Package).tuple["digest"] = other(),
        // 36-40: a member of the Registry Package.
        |input| {
            if let Some(members) = &mut input.part(Package).members {
                members.retain(|(role, _)| role != FINDINGS);
            }
        },
        |input| input.part(Package).record(FINDINGS, |record| record["identity"] = json!("intlify-other")),
        |input| input.part(Package).record(FINDINGS, |record| record["revision"] = json!("1")),
        |input| input.part(Package).record(FINDINGS, |record| record["digest"] = other()),
        |input| input.part(Package).replace(ORIGINS, generate::write(&json!({})).expect("a JSON value serializes")),
        // 41: an ID the Safe Origin Kind Registry repeats.
        |input| {
            let package = input.part(Package);
            let origins = edited(package.member(ORIGINS).bytes(), |registry| {
                registry["rows"][1]["id"] = json!("configuration-file");
            });
            package.replace(ORIGINS, origins);
        },
        // 42: a set that belongs to another Profile Specification.
        |input| input.part(Set).edit(|set| set["profileSpecification"]["semanticDigest"] = other()),
        // 43: a Registry Package the resolver was not compiled from.
        |input| {
            let package = input.part(Package);
            let body = strict_json(package.body.bytes()).expect("a committed body is strict JSON");
            package.body = Body::Changed(serde_json::to_vec(&body).expect("a JSON value serializes"));
            package.declare();
        },
    ]
}

/// The decisions that state a row's rule: the decision of its component, and
/// the one that fixes the order of the rows (Decision 015-142).
const fn decisions(row: PrecedenceRow) -> &'static [&'static str] {
    use crate::construction::failure::{Check, Component};
    match (row.component, row.check) {
        (Component::All, Check::ConstructionBounds) => &["015-141", "015-142"],
        (Component::All, Check::CrossAuthorityRules) => &["015-124", "015-142"],
        (Component::All, _) => &["015-142"],
        (Component::ProfileSpecificationSchema, _) => &["015-107", "015-142"],
        (Component::ProfileSpecification, _) => &["015-140", "015-142"],
        (Component::ConfigurationSchemaAuthoritySet | Component::ConfigurationSchemaMember, _) => {
            &["015-108", "015-142"]
        }
        (Component::ResolverCapabilitySpecification, _) => &["015-110", "015-142"],
        (Component::RegistryPackage | Component::RegistryMember, _) => &["015-086", "015-142"],
    }
}

/// The decisions the constructing case traces: the construction input set
/// and its identity, the identity's derivation, and the declared tuples.
const CONSTRUCTED_DECISIONS: [&str; 3] = ["015-095", "015-125", "015-140"];

/// A row's component and check, as the construction-admission body spells
/// them.
fn spelled(row: PrecedenceRow) -> Result<(String, String), serde_json::Error> {
    let row = serde_json::to_value(row)?;
    let text = |name: &str| row[name].as_str().unwrap_or_default().to_owned();
    Ok((text("component"), text("check")))
}

/// A row's construction rule ID (D15:2174).
fn rule(row: PrecedenceRow) -> Result<String, serde_json::Error> {
    let (component, check) = spelled(row)?;
    Ok(format!("015.construction.{component}.{check}"))
}

/// The ID of the case for a row: `015-construction-`, the component in
/// kebab-case, unless the row checks the whole input, and the check, a
/// presence check named for what it refuses.
fn case_id(row: PrecedenceRow) -> Result<String, serde_json::Error> {
    let (component, check) = spelled(row)?;
    let check = match check.as_str() {
        "presence" => "missing",
        "construction-bounds" => "bounds",
        other => other,
    }
    .to_owned();
    if component == "all" {
        return Ok(format!("015-construction-{check}"));
    }
    let mut kebab = String::new();
    for character in component.chars() {
        if character.is_ascii_uppercase() {
            kebab.push('-');
        }
        kebab.push(character.to_ascii_lowercase());
    }
    Ok(format!("015-construction-{kebab}-{check}"))
}

/// What a case expects of a row: no identity and the row's reason, the
/// bounds' row also the bound its input exceeds.
fn refused(row: PrecedenceRow) -> Value {
    use crate::construction::failure::Check;
    let mut expected = Map::new();
    expected.insert("constructed".into(), json!(false));
    expected.insert("resolverConstructionIdentity".into(), Value::Null);
    expected.insert("failureReason".into(), json!(row.reason));
    if row.check == Check::ConstructionBounds {
        let bound = ConstructionBoundId::InputMaxDepth;
        expected.insert("failureDetails".into(), json!({ "boundId": bound }));
    }
    expected.insert("resolverInvoked".into(), json!(false));
    Value::Object(expected)
}

/// What the constructing case expects: the committed construction vector's
/// identity.
fn constructed() -> Result<Value, serde_json::Error> {
    let vectors: Value = serde_json::from_slice(VECTORS)?;
    Ok(json!({
        "constructed": true,
        "resolverConstructionIdentity": vectors["constructionIdentity"]["text"],
        "resolverInvoked": false,
    }))
}

/// A body as a raw input of case `id`'s fixture, a changed one that is
/// strict UTF-8 JSON kept in a file named `name` beside the fixture.
fn raw(id: &str, name: &str, body: &Body, files: &mut Vec<SuiteFile>) -> Value {
    let reference =
        |path: &str, bytes: &[u8]| json!({ "path": path, "digest": file_digest(bytes).as_str() });
    match body {
        Body::Committed(path, bytes) => reference(path, bytes),
        Body::Changed(bytes) if strict_json(bytes).is_some() => {
            let path = format!("{FIXTURES}/{id}/{name}.json");
            let value = reference(&path, bytes);
            files.push(SuiteFile {
                path,
                bytes: bytes.clone(),
            });
            value
        }
        Body::Changed(bytes) => json!({ "bytes": hex(bytes) }),
    }
}

/// Case `id`'s fixture of `input`, and the files of the bodies it changed.
fn fixture(id: &str, input: &Input, files: &mut Vec<SuiteFile>) -> Value {
    let mut fixture = Map::new();
    for (part, component) in Part::ALL.into_iter().zip(&input.0) {
        let Some(component) = component else {
            continue;
        };
        let mut value = Map::new();
        value.insert("tuple".into(), json!({ "json": component.tuple }));
        value.insert("bytes".into(), raw(id, part.file(), &component.body, files));
        if let Some(members) = &component.members {
            let members = members
                .iter()
                .map(|(key, body)| {
                    let name = format!("{}.{key}", part.file());
                    (key.clone(), raw(id, &name, body, files))
                })
                .collect::<Map<_, _>>();
            value.insert("members".into(), Value::Object(members));
        }
        fixture.insert(part.name().into(), Value::Object(value));
    }
    Value::Object(fixture)
}

/// One case before it is written: its ID, input, expected outcome, and
/// traceability.
struct Case {
    id: String,
    input: Input,
    expected: Value,
    decisions: Vec<&'static str>,
    rules: Vec<String>,
}

/// Every case: the constructing one, and one for each row.
fn all() -> Result<Vec<Case>, serde_json::Error> {
    let committed = Input::committed()?;
    let mut cases = vec![Case {
        id: CONSTRUCTED.to_owned(),
        input: committed.clone(),
        expected: constructed()?,
        decisions: CONSTRUCTED_DECISIONS.to_vec(),
        rules: PRECEDENCE.into_iter().map(rule).collect::<Result<_, _>>()?,
    }];
    for (row, change) in PRECEDENCE.into_iter().zip(changes()) {
        let mut input = committed.clone();
        change(&mut input);
        cases.push(Case {
            id: case_id(row)?,
            input,
            expected: refused(row),
            decisions: decisions(row).to_vec(),
            rules: vec![rule(row)?],
        });
    }
    cases.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(cases)
}

/// The suite's construction cases, in ascending unsigned byte order of their
/// IDs: each manifest, fixture, and changed body kept in a file, and the
/// index entry of each manifest.
///
/// # Errors
///
/// Returns the serialization failure of a value, which their values rule
/// out.
pub(crate) fn cases() -> Result<Cases, serde_json::Error> {
    let mut files = Vec::new();
    let mut listing = Vec::new();
    for case in all()? {
        let fixture = generate::write(&fixture(&case.id, &case.input, &mut files))?;
        let fixture_path = format!("{FIXTURES}/{}.json", case.id);
        let manifest = generate::write(&json!({
            "suiteRevision": REVISION,
            "manifestKind": ManifestKind::ResolverConstruction.name(),
            "id": case.id,
            "constructionInputFixture": {
                "path": fixture_path,
                "digest": file_digest(&fixture).as_str(),
            },
            "expected": case.expected,
            "traceability": {
                "decisionIds": case.decisions,
                "ruleIds": case.rules,
                "checkIds": [],
            },
        }))?;
        let path = format!("{CASES}/{}.json", case.id);
        listing.push(json!({
            "id": case.id,
            "path": path,
            "digest": file_digest(&manifest).as_str(),
        }));
        files.push(SuiteFile {
            path: fixture_path,
            bytes: fixture,
        });
        files.push(SuiteFile {
            path,
            bytes: manifest,
        });
    }
    Ok(Cases { files, listing })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use super::*;
    use crate::conformance::construction;
    use crate::conformance::fixture::Bodies;
    use crate::construction::component::Refusal;
    use crate::construction::flow::construct;
    use crate::construction::root::ConstructionRoot;

    /// The committed file at `path` below the suite root.
    fn committed_file(path: &str) -> Option<Vec<u8>> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("conformance");
        std::fs::read(root.join(path)).ok()
    }

    #[test]
    fn the_committed_cases_are_the_ones_this_crate_writes() {
        let cases = cases().unwrap();
        for file in &cases.files {
            assert_eq!(
                committed_file(&file.path).as_deref(),
                Some(file.bytes.as_slice()),
                "{} is stale; regenerate it with the generate_conformance_suite example",
                file.path
            );
        }
        assert_eq!(cases.listing.len(), 44);
    }

    /// The bodies a case's input has, by the paths its fixture references.
    fn bodies(input: &Input, files: &[SuiteFile]) -> Bodies {
        let mut bodies = files
            .iter()
            .map(|file| (file.path.clone(), file.bytes.clone()))
            .collect::<Bodies>();
        for component in input.0.iter().flatten() {
            let members = component.members.iter().flatten().map(|(_, body)| body);
            for body in std::iter::once(&component.body).chain(members) {
                if let Body::Committed(path, bytes) = body {
                    bodies.insert(path.clone(), bytes.clone());
                }
            }
        }
        bodies
    }

    /// Each failing case's input is refused by exactly its row, which no
    /// other case's is, and the constructing case's input constructs: a
    /// reason two rows share cannot hide a case that fails before its row.
    #[test]
    fn each_case_is_refused_by_its_own_row() {
        let root = ConstructionRoot::built_in().unwrap();
        let mut rows = BTreeSet::new();
        for case in all().unwrap() {
            let mut files = Vec::new();
            let fixture = fixture(&case.id, &case.input, &mut files);
            let input = construction::input(&fixture, &bodies(&case.input, &files));
            let refusal = construct(root, &input).err();
            if case.id == CONSTRUCTED {
                assert_eq!(refusal, None);
                assert_eq!(case.rules.len(), PRECEDENCE.len());
                continue;
            }
            assert!(case.decisions.contains(&"015-142"), "{}", case.id);
            let number = refusal.map(Refusal::number).unwrap_or_default();
            let row = PRECEDENCE[number - 1];
            assert_eq!(case.rules, [rule(row).unwrap()], "{}", case.id);
            assert_eq!(construction::outcome(&input), case.expected, "{}", case.id);
            assert!(rows.insert(number), "{}", case.id);
        }
        assert_eq!(rows.len(), PRECEDENCE.len());
    }

    /// The constructing case expects the committed vector's identity, which
    /// is the one construction derives.
    #[test]
    fn the_constructing_case_expects_the_vector_s_identity() {
        let expected = constructed().unwrap();
        let text = expected["resolverConstructionIdentity"].as_str().unwrap();
        assert!(text.starts_with("rci0_"));
        let input = crate::construction::flow::testing::committed();
        assert_eq!(construction::outcome(&input), expected);
    }

    /// The IDs are unique, in the case-ID grammar, and name their rows.
    #[test]
    fn the_ids_name_their_rows() {
        let ids = PRECEDENCE
            .into_iter()
            .map(|row| case_id(row).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), ids.len());
        assert_eq!(ids[0], "015-construction-bounds");
        assert_eq!(
            ids[1],
            "015-construction-profile-specification-schema-missing"
        );
        assert_eq!(ids[38], "015-construction-registry-member-file-digest");
        assert_eq!(ids[42], "015-construction-implemented-authority");
        // `015-` and lowercase kebab-case, as the Case Schema's grammar has it.
        assert!(ids.iter().all(|id| {
            id.starts_with("015-")
                && id.split('-').all(|word| {
                    !word.is_empty()
                        && word.chars().all(|character| {
                            character.is_ascii_lowercase() || character.is_ascii_digit()
                        })
                })
        }));
        assert!(!ids.contains(&CONSTRUCTED.to_owned()));
    }
}
