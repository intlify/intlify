// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Resolver Cases this crate writes (D15:3172-3237, 3271-3303).
//!
//! Every case starts from one storefront, the shape the benchmark's
//! Storefront recipe resolves: a profile `app` with two requested locales,
//! the five required policies and the two optional ones, and two targets in
//! one hydration group, each a sealed envelope of its sample content. The
//! selected Resource Limit Policy sets the scenario's limits, which the
//! implementation capability matches with room for every bootstrap bound,
//! and the Verification Input pins it. Each input is one its boundary admits
//! and each candidate the ID its slot fixes (D15:3252); the canonicalization
//! data is the committed ICU4X artifact, which the suite holds below its
//! root, since a fixture refers to nothing outside it (D15:2858).
//!
//! A file entry submits the configuration's bytes and a programmatic entry
//! the formal Snapshot of the value they spell, so each pair reaches one
//! Materialized Resolution Input Set (D15:3233). What each entry expects is
//! what the resolver does with it, written as a candidate for review: a
//! checked-in expectation is reviewable evidence, not output accepted
//! because the implementation produced it (D15:3789).

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::artifacts::content::sample;
use crate::artifacts::envelope::{envelope_of, ArtifactKind, ExactTuple, FormalMember};
use crate::artifacts::verification::{sealed, unsealed, VerificationInputArtifact};
use crate::boundary::capability::ImplementationCapability;
use crate::boundary::source::file_source_revision;
use crate::construction::component::file_digest;
use crate::construction::generate;
use crate::construction::registry::check::CheckRegistry;
use crate::construction::resolver::{committed, ConstructedResolver};
use crate::construction::root::ConstructionRoot;
use crate::input_limits::Bound;
use crate::locale::VersionedIdentity;
use crate::materialize::PortableNumber;
use crate::profile_fixtures::complete_config;
use crate::resolver::evidence_source::hex;
use crate::resolver::limits::{fixture_limits, Group, ResolutionLimits, SECTION};
use crate::resolver::origin::SafeOriginKind;
use crate::snapshot::Node;

use super::admission::node_view;
use super::case::{ManifestKind, REVISION};
use super::construction_cases::{Cases, CONSTRUCTED};
use super::fixture::{written, Bodies};
use super::index::SuiteFile;
use super::resolution::resolved;

/// Where the suite holds the Resolver Cases' manifests and their fixtures.
const CASES: &str = "cases/resolver";
const FIXTURES: &str = "fixtures/resolver";

/// Where the suite holds the committed canonicalization data, below its
/// root.
pub(crate) const DATA: &str = "fixtures/resolver/locale-data";

/// The committed ICU4X artifact: its manifest, and each payload at the
/// address the manifest names.
const MANIFEST: &[u8] = include_bytes!("../../../intlify_locale_icu4x/data/manifest.json");
const PAYLOADS: [(&str, &[u8]); 2] = [
    (
        "conformance.json",
        include_bytes!("../../../intlify_locale_icu4x/data/conformance.json"),
    ),
    (
        "markers.postcard",
        include_bytes!("../../../intlify_locale_icu4x/data/markers.postcard"),
    ),
];

const VOCABULARY_VECTORS: &[u8] = include_bytes!("../../fixtures/vocabulary/vectors.json");

/// The Profile ID every entry's selector names.
const SELECTED: &str = "app";

/// The Resource Limit Policy the profile selects.
const LIMITS: &str = "storefront-limits";

/// Every other policy the profile names: its slot, its kind, and its
/// identity.
const POLICIES: [(&str, ArtifactKind, &str); 6] = [
    ("trust", ArtifactKind::TrustPolicy, "storefront-trust"),
    (
        "sourceAdmission",
        ArtifactKind::SourceAdmissionPolicy,
        "storefront-source",
    ),
    (
        "approval",
        ArtifactKind::ApprovalPolicy,
        "storefront-approval",
    ),
    (
        "selection",
        ArtifactKind::SelectionPolicy,
        "storefront-selection",
    ),
    (
        "providerRouting",
        ArtifactKind::ProviderRoutingPolicy,
        "storefront-routing",
    ),
    (
        "glossarySet",
        ArtifactKind::GlossarySet,
        "storefront-glossary",
    ),
];

/// Each target: its ID, its Target Profile's identity, and its hydration
/// role.
const TARGETS: [(&str, &str, &str); 2] = [
    ("browser", "storefront-browser", "browser-hydration-client"),
    ("server", "storefront-server", "ssr-renderer"),
];

/// The Verification Input trusted setup supplies, by identity and revision.
const VERIFICATION: (&str, &str) = ("intlify-conformance-verification-input", "0");

/// A bootstrap bound with room for every input of the suite.
const ROOMY: &str = "1048576";

/// A bound with room for every artifact and every retained record.
const RECORDS: u64 = 1 << 30;

/// The record bounds the two exhaustion cases set: Shared exhausts within
/// its evaluation status, Entry after the first five records of a file
/// whose syntax fails.
const SHARED_RECORDS: u64 = 100;
const ENTRY_RECORDS: u64 = 5;

/// The scenario's limits: the fixture capacity's, with room for every
/// artifact the collection holds and, in both diagnostic domains, for every
/// record a case retains.
fn limits() -> ResolutionLimits {
    let current = fixture_limits(10_000).bounds();
    let room = Bound::new(RECORDS).expect("a finite positive bound");
    ResolutionLimits::from_bounds(|id| {
        if matches!(
            id.group,
            Group::ArtifactAdmission | Group::DiagnosticsEntry | Group::DiagnosticsShared
        ) {
            Some(room)
        } else {
            current
                .iter()
                .find(|(bound, _)| *bound == id)
                .map(|&(_, value)| value)
        }
    })
    .expect("every bound of the fixture limits is set")
}

/// The scenario's limits with the `maxRecords` bound of the diagnostic
/// domain `group` at `records`.
fn records_at(group: Group, records: u64) -> ResolutionLimits {
    let current = limits().bounds();
    let bound = Bound::new(records).expect("a finite positive bound");
    ResolutionLimits::from_bounds(|id| {
        if id.group == group && id.name == "maxRecords" {
            Some(bound)
        } else {
            current
                .iter()
                .find(|(other, _)| *other == id)
                .map(|&(_, value)| value)
        }
    })
    .expect("every bound of the scenario's limits is set")
}

/// The canonicalization data's files below the suite root, each with its
/// bytes: the manifest, then each payload.
pub(crate) fn data_files() -> Vec<(String, &'static [u8])> {
    std::iter::once(("manifest.json", MANIFEST))
        .chain(PAYLOADS)
        .map(|(name, bytes)| (format!("{DATA}/{name}"), bytes))
        .collect()
}

/// A reference to a file below the suite root.
fn reference(path: &str, bytes: &[u8]) -> Value {
    json!({
        "path": path,
        "digest": crate::construction::component::file_digest(bytes).as_str(),
    })
}

/// Raw bytes in a fixture.
fn raw(bytes: &[u8]) -> Value {
    json!({ "bytes": hex(bytes) })
}

/// The exact tuple a sealed member declares.
fn tuple_of(member: &Value) -> ExactTuple {
    FormalMember::new(member.clone())
        .ok()
        .and_then(|member| member.envelope().map(|envelope| envelope.tuple().clone()))
        .expect("a scenario member declares its tuple")
}

/// The reference a declaration writes for `tuple`.
fn reference_value(tuple: &ExactTuple) -> Value {
    json!({
        "kind": tuple.kind.spelling(),
        "identity": tuple.identity.as_str(),
        "revision": tuple.revision.as_str(),
        "specificationRevision": tuple.specification_revision.as_str(),
        "semanticDigest": tuple.semantic_digest.as_str(),
    })
}

/// The storefront every Resolver Case starts from.
pub(crate) struct Storefront {
    /// The configuration both entries submit.
    pub(crate) config: Value,
    /// The collection's members, the selected policy first.
    members: Vec<Value>,
    /// The sealed Verification Input pinning the selected policy.
    verification: Value,
    /// What trusted setup asserts about it.
    assertion: Value,
    /// The implementation capability's value.
    capability: Value,
}

impl Storefront {
    /// The storefront at the scenario's limits.
    pub(crate) fn new() -> Self {
        Self::at(limits())
    }

    /// The storefront whose selected policy sets `limits`.
    pub(crate) fn at(limits: ResolutionLimits) -> Self {
        let mut config = complete_config();
        let app = &mut config["profiles"][SELECTED];
        app["requestedLocales"] = json!(["ja", "en"]);
        app["defaultRequestedLocale"] = json!("en");
        app["localeNegotiation"] = json!({ "aliases": { "de": "en" } });
        let policy = envelope_of(
            ArtifactKind::ResourceLimitPolicy,
            LIMITS,
            "1",
            json!({ SECTION: limits.section() }),
        );
        app["policies"]["resourceLimits"] = reference_value(&tuple_of(&policy));
        let mut members = vec![policy];
        for (slot, kind, identity) in POLICIES {
            let member = envelope_of(kind, identity, "1", sample(kind));
            app["policies"][slot] = reference_value(&tuple_of(&member));
            members.push(member);
        }
        for (target, identity, role) in TARGETS {
            let mut content = sample(ArtifactKind::TargetProfile);
            content["hydration"]["roles"] = json!([role]);
            let member = envelope_of(ArtifactKind::TargetProfile, identity, "1", content);
            app["targetProfiles"][target]["requestedLocales"] = json!(["ja", "en"]);
            app["targetProfiles"][target]["profile"] = reference_value(&tuple_of(&member));
            members.push(member);
        }
        let pins = [reference_value(&tuple_of(&members[0]))];
        let verification = sealed(unsealed(VERIFICATION.0, VERIFICATION.1, &pins))
            .expect("the Verification Input seals");
        let read: VerificationInputArtifact = serde_json::from_value(verification.clone())
            .expect("the sealed Verification Input reads as its type");
        let assertion = json!({
            "identity": read.identity().as_str(),
            "revision": read.revision().as_str(),
            "semanticDigest": read
                .semantic_digest()
                .expect("the Verification Input has its digest")
                .as_str(),
        });
        let mut capability = ImplementationCapability::fixture_owned(limits).normalized();
        let bootstrap = capability["bootstrapOnly"]
            .as_object_mut()
            .expect("the capability has its bootstrap bounds");
        for namespace in bootstrap.values_mut() {
            for bound in namespace
                .as_object_mut()
                .expect("a namespace holds its bounds")
                .values_mut()
            {
                *bound = json!(ROOMY);
            }
        }
        Self {
            config,
            members,
            verification,
            assertion,
            capability,
        }
    }

    /// The common input fixture on the resolver `constructed`: each slot's
    /// candidate its own ID and its input one its boundary admits, the
    /// specification asserting what the resolver supports and the
    /// vocabulary the committed one (D15:3252).
    pub(crate) fn common(&self, constructed: &ConstructedResolver) -> Value {
        use SafeOriginKind as Kind;
        let vectors: Value =
            serde_json::from_slice(VOCABULARY_VECTORS).expect("the vocabulary vectors are JSON");
        let vocabulary = vectors["vocabularies"]
            .as_array()
            .and_then(|vectors| vectors.iter().find(|vector| vector["id"] == "fixture"))
            .expect("the vectors hold the fixture vocabulary");
        let artifact = &vocabulary["artifact"];
        let specification = specification(constructed, artifact, &vocabulary["semanticDigest"]);
        let slot = |kind: Kind, input: Value| json!({ "candidate": kind.id(), "input": input });
        let mut payloads = Map::new();
        for (name, bytes) in PAYLOADS {
            payloads.insert(name.to_owned(), reference(&format!("{DATA}/{name}"), bytes));
        }
        let members = self
            .members
            .iter()
            .map(|member| raw(&serde_json::to_vec(member).expect("a member encodes")))
            .collect::<Vec<_>>();
        json!({
            "assertion": self.assertion,
            Kind::ProfileResolutionSpecificationInput.id(): slot(
                Kind::ProfileResolutionSpecificationInput,
                raw(&serde_json::to_vec(&specification).expect("a JSON value encodes")),
            ),
            Kind::IntentSurfaceClassVocabulary.id(): slot(
                Kind::IntentSurfaceClassVocabulary,
                raw(&serde_json::to_vec(artifact).expect("a JSON value encodes")),
            ),
            Kind::LocaleCanonicalizationData.id(): slot(
                Kind::LocaleCanonicalizationData,
                json!({
                    "manifest": reference(&format!("{DATA}/manifest.json"), MANIFEST),
                    "payloads": payloads,
                }),
            ),
            Kind::ProfileResolutionArtifactCollection.id(): slot(
                Kind::ProfileResolutionArtifactCollection,
                json!({ "members": members }),
            ),
            Kind::ResourceLimitPolicyVerification.id(): slot(
                Kind::ResourceLimitPolicyVerification,
                raw(&serde_json::to_vec(&self.verification).expect("a JSON value encodes")),
            ),
            Kind::ImplementationCapability.id(): slot(
                Kind::ImplementationCapability,
                json!({ "json": self.capability }),
            ),
        })
    }

    /// The configuration's bytes, as a file entry submits them.
    pub(crate) fn bytes(&self) -> Vec<u8> {
        serde_json::to_vec(&self.config).expect("a JSON value encodes")
    }

    /// The file entry's admission input fixture: the configuration's bytes
    /// under their source identity and revision, with a relative-path
    /// locator, and the CLI selector naming the profile.
    pub(crate) fn file(&self) -> Value {
        file(&self.bytes())
    }

    /// The configuration's bytes without their last, which leaves its root
    /// object unclosed: raw JSON syntax no programmatic value can spell.
    fn unclosed(&self) -> Vec<u8> {
        let mut bytes = self.bytes();
        bytes.pop();
        bytes
    }

    /// The programmatic entry's admission input fixture: the formal Snapshot
    /// of the value the configuration spells, its revision the Snapshot's
    /// digest (D15:890), with a safe-label locator, and the programmatic
    /// selector naming the profile.
    pub(crate) fn programmatic(&self) -> Value {
        use SafeOriginKind as Kind;
        let root = node(&self.config);
        let digest = root.digest();
        json!({
            Kind::ConfigurationProgrammatic.id(): {
                "candidate": Kind::ConfigurationProgrammatic.id(),
                "input": {
                    "snapshot": { "snapshot": node_view(&root), "digest": digest },
                    "identity": "intlify-programmatic-entry.v0:storefront",
                    "nonSecret": true,
                    "revision": digest,
                    "locator": {
                        "variant": "safe-label",
                        "payload": "storefront",
                        "nonSecret": true,
                    },
                },
            },
            Kind::ProfileSelectorProgrammatic.id(): {
                "candidate": Kind::ProfileSelectorProgrammatic.id(),
                "input": { "report": "string", "value": SELECTED },
            },
        })
    }
}

/// The file entry's admission input fixture for `bytes`: under their source
/// identity and revision, with a relative-path locator, and the CLI selector
/// naming the profile.
fn file(bytes: &[u8]) -> Value {
    use SafeOriginKind as Kind;
    json!({
        Kind::ConfigurationFile.id(): {
            "candidate": Kind::ConfigurationFile.id(),
            "input": {
                "bytes": raw(bytes),
                "identity": "intlify-file-source.v0:storefront/intlify.config.json",
                "nonSecret": true,
                "revision": file_source_revision(bytes),
                "locator": {
                    "variant": "relative-path",
                    "payload": "intlify.config.json",
                    "nonSecret": true,
                },
            },
        },
        Kind::ProfileSelectorCli.id(): {
            "candidate": Kind::ProfileSelectorCli.id(),
            "input": { "report": "string", "value": SELECTED },
        },
    })
}

/// The Profile Resolution Specification Input asserting what `constructed`
/// supports in each slot, and the vocabulary `artifact` with its semantic
/// digest `digest` in its own (D15:3252).
fn specification(constructed: &ConstructedResolver, artifact: &Value, digest: &Value) -> Value {
    let supported = constructed.supported();
    let slot = |pinned: &VersionedIdentity<String>| {
        let (identity, revision) = (&pinned.identity, &pinned.revision);
        json!({ "identity": identity, "revision": revision })
    };
    let canonicalization = supported
        .locale_canonicalization()
        .entries()
        .first()
        .expect("the resolver supports a canonicalization specification");
    json!({
        "project-profile": slot(&supported.project_profile().specification),
        "locale-canonicalization": slot(&canonicalization.specification),
        "locale-negotiation": slot(supported.locale_negotiation()),
        "message-locale-fallback": slot(supported.message_locale_fallback()),
        "coverage": slot(supported.coverage()),
        "intent-surface-class-vocabulary": {
            "identity": artifact["identity"],
            "revision": artifact["revision"],
            "semanticDigest": digest,
        },
        "delivery-graph": slot(supported.delivery_graph()),
        "delivery-placement": slot(supported.delivery_placement()),
    })
}

/// The canonical Snapshot node of `value`.
fn node(value: &Value) -> Node {
    match value {
        Value::Null => Node::Null,
        Value::Bool(value) => Node::Boolean(*value),
        Value::Number(number) => Node::Number(
            number
                .as_f64()
                .and_then(PortableNumber::new)
                .expect("the storefront's numbers are portable"),
        ),
        Value::String(text) => Node::String(text.clone()),
        Value::Array(items) => Node::Array(items.iter().map(node).collect()),
        Value::Object(members) => Node::Object(
            members
                .iter()
                .map(|(name, member)| (name.clone(), node(member)))
                .collect(),
        ),
    }
}

/// What one side of a case submits: its entry admission input fixture, or
/// the closed reason it is left unpaired for (D15:3235, 3237).
enum Side {
    Paired(Value),
    Unpaired(&'static str),
}

/// One Resolver Case before it is written: its ID, its common input fixture,
/// both sides, and the decisions and checks it traces.
struct Case {
    id: &'static str,
    common: Value,
    file: Side,
    programmatic: Side,
    decisions: &'static [&'static str],
    checks: &'static [&'static str],
}

impl Case {
    /// Each side, by its name in the manifest.
    fn sides(&self) -> [(&'static str, &Side); 2] {
        [("file", &self.file), ("programmatic", &self.programmatic)]
    }
}

/// The suite's Resolver Cases on the resolver `constructed`, in ascending
/// unsigned byte order of their IDs: the storefront checked; its project
/// default outside its requested locales, blocked with Shared evidence; its
/// bytes unclosed, a file-only case without Shared evidence; and each
/// diagnostic domain exhausting, Entry before any Shared evidence and Shared
/// beside a complete Entry.
fn all(constructed: &ConstructedResolver) -> Vec<Case> {
    let storefront = Storefront::new();
    let common = storefront.common(constructed);
    let mut default = Storefront::new();
    default.config["profiles"][SELECTED]["defaultRequestedLocale"] = json!("fr");
    let entry = Storefront::at(records_at(Group::DiagnosticsEntry, ENTRY_RECORDS));
    let shared = Storefront::at(records_at(Group::DiagnosticsShared, SHARED_RECORDS));
    vec![
        Case {
            id: "015-entry-json-invalid",
            common: common.clone(),
            file: Side::Paired(file(&storefront.unclosed())),
            programmatic: Side::Unpaired("raw-json-syntax"),
            decisions: &["015-037", "015-065", "015-202"],
            checks: &["entry.input.well-formed-json"],
        },
        Case {
            id: "015-locale-default-not-in-project",
            common: common.clone(),
            file: Side::Paired(default.file()),
            programmatic: Side::Paired(default.programmatic()),
            decisions: &["015-028"],
            checks: &[
                "locale.default-requested.member",
                "target.effective-default.supported",
            ],
        },
        Case {
            id: "015-reporting-entry-limit-exceeded",
            common: entry.common(constructed),
            file: Side::Paired(file(&entry.unclosed())),
            programmatic: Side::Unpaired("raw-json-syntax"),
            decisions: &["015-060", "015-065", "015-067", "015-069"],
            checks: &["reporting.diagnostic-domain.complete"],
        },
        Case {
            id: "015-reporting-shared-limit-exceeded",
            common: shared.common(constructed),
            file: Side::Paired(shared.file()),
            programmatic: Side::Paired(shared.programmatic()),
            decisions: &["015-060", "015-067", "015-069"],
            checks: &["reporting.diagnostic-domain.complete"],
        },
        Case {
            id: "015-storefront-checked",
            common,
            file: Side::Paired(storefront.file()),
            programmatic: Side::Paired(storefront.programmatic()),
            decisions: &["015-037", "015-060", "015-064", "015-066"],
            checks: &[],
        },
    ]
}

/// The identity text of the resolver this crate commits, which every case
/// constructs.
fn identity(constructed: &ConstructedResolver) -> String {
    let root = ConstructionRoot::built_in().expect("the built-in construction root verifies");
    constructed.identity(root).text()
}

/// The canonicalization data's files, as the suite holds them and as the
/// runner keeps their verified bodies.
fn data_bodies() -> Bodies {
    data_files()
        .into_iter()
        .map(|(path, bytes)| (path, bytes.to_vec()))
        .collect()
}

/// The suite's Resolver Cases, in ascending unsigned byte order of their IDs:
/// the canonicalization data's files, then each case's raw inputs, entry
/// fixtures, expected outcomes, common input fixture, and manifest. Each
/// expected outcome is what the resolver does with its entry, a candidate
/// that review accepts or refuses, since a checked-in expectation is
/// reviewable evidence (D15:3789).
///
/// # Errors
///
/// Returns the serialization failure of a value, which their values rule
/// out.
///
/// # Panics
///
/// Panics when an entry does not run or a case traces an unregistered
/// check, which the cases rule out.
pub(crate) fn cases() -> Result<Cases, serde_json::Error> {
    let constructed = committed();
    let identity = identity(constructed);
    let registry = CheckRegistry::revision_zero();
    let bodies = data_bodies();
    let mut files = bodies
        .iter()
        .map(|(path, bytes)| SuiteFile {
            path: path.clone(),
            bytes: bytes.clone(),
        })
        .collect::<Vec<_>>();
    let mut kept = BTreeMap::new();
    let mut listing = Vec::new();
    for case in all(constructed) {
        let mut entries = Map::new();
        for (side, entry) in case.sides() {
            let value = match entry {
                Side::Unpaired(reason) => json!({
                    "unpaired": { "reason": reason, "verificationOwner": "intlify_config" }
                }),
                Side::Paired(input) => {
                    let outcome = resolved(&case.common, input, &bodies, constructed, &identity)
                        .expect("each entry of a case runs")
                        .outcome;
                    let input = written(FIXTURES, case.id, input, &mut files, &mut kept);
                    let input = generate::write(&input)?;
                    let expected = expected_bytes(&outcome)?;
                    let input_path = format!("{FIXTURES}/{}.{side}-input.json", case.id);
                    let expected_path = format!("{FIXTURES}/{}.{side}-expected.json", case.id);
                    let value = json!({
                        "entryAdmissionInputFixture": reference(&input_path, &input),
                        "expectedOutcomeFixture": reference(&expected_path, &expected),
                    });
                    files.push(SuiteFile {
                        path: input_path,
                        bytes: input,
                    });
                    files.push(SuiteFile {
                        path: expected_path,
                        bytes: expected,
                    });
                    value
                }
            };
            entries.insert(side.to_owned(), value);
        }
        let common = written(FIXTURES, case.id, &case.common, &mut files, &mut kept);
        let common = generate::write(&common)?;
        let common_path = format!("{FIXTURES}/{}.common.json", case.id);
        let rules = case
            .checks
            .iter()
            .map(|check| {
                registry
                    .rows
                    .iter()
                    .find(|row| row.check_id == *check)
                    .map(|row| row.rule_id.clone())
                    .expect("a case traces a registered check")
            })
            .collect::<Vec<_>>();
        let manifest = generate::write(&json!({
            "suiteRevision": REVISION,
            "manifestKind": ManifestKind::Resolver.name(),
            "id": case.id,
            "resolverConstruction": {
                "caseId": CONSTRUCTED,
                "identityReference": identity,
            },
            "commonInvocationInputFixture": reference(&common_path, &common),
            "entries": entries,
            "traceability": {
                "decisionIds": case.decisions,
                "ruleIds": rules,
                "checkIds": case.checks,
            },
        }))?;
        let path = format!("{CASES}/{}.json", case.id);
        listing.push(json!({
            "id": case.id,
            "path": path,
            "digest": file_digest(&manifest).as_str(),
        }));
        files.push(SuiteFile {
            path: common_path,
            bytes: common,
        });
        files.push(SuiteFile {
            path,
            bytes: manifest,
        });
    }
    Ok(Cases { files, listing })
}

/// The bytes of an expected outcome: two-space indentation, as the suite's
/// other files have, but each row of a projection's arrays, each binding,
/// each terminal state, and each cause key on a line of its own, so review
/// reads one record a line.
fn expected_bytes(outcome: &Value) -> Result<Vec<u8>, serde_json::Error> {
    /// Whether the array at `path`, by member names from the root, holds
    /// one record a line.
    fn rows(path: &[String]) -> bool {
        let path = path.iter().map(String::as_str).collect::<Vec<_>>();
        matches!(
            path[..],
            ["resolvedProfileArtifactBindings"]
                | [
                    "resolutionEvidence",
                    "terminalStates" | "sharedUnavailableCauseKeys"
                ]
                | ["resolutionEvidence", "shared" | "entry", _]
        )
    }
    fn write(
        value: &Value,
        path: &mut Vec<String>,
        depth: usize,
        text: &mut String,
    ) -> Result<(), serde_json::Error> {
        let indent = |depth: usize| "  ".repeat(depth);
        match value {
            Value::Object(members) if !members.is_empty() => {
                text.push_str("{\n");
                for (index, (name, member)) in members.iter().enumerate() {
                    text.push_str(&indent(depth + 1));
                    text.push_str(&serde_json::to_string(name)?);
                    text.push_str(": ");
                    path.push(name.clone());
                    write(member, path, depth + 1, text)?;
                    path.pop();
                    if index + 1 < members.len() {
                        text.push(',');
                    }
                    text.push('\n');
                }
                text.push_str(&indent(depth));
                text.push('}');
            }
            Value::Array(items) if !items.is_empty() => {
                let one_line = rows(path);
                text.push_str("[\n");
                for (index, item) in items.iter().enumerate() {
                    text.push_str(&indent(depth + 1));
                    if one_line {
                        text.push_str(&serde_json::to_string(item)?);
                    } else {
                        write(item, path, depth + 1, text)?;
                    }
                    if index + 1 < items.len() {
                        text.push(',');
                    }
                    text.push('\n');
                }
                text.push_str(&indent(depth));
                text.push(']');
            }
            other => text.push_str(&serde_json::to_string(other)?),
        }
        Ok(())
    }
    let mut text = String::new();
    write(outcome, &mut Vec::new(), 0, &mut text)?;
    text.push('\n');
    Ok(text.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::construction_cases;
    use crate::conformance::resolution::{duties, paired, Resolved};
    use crate::construction::admission::DRAFT_7;
    use crate::construction::component::strict_json;
    use crate::construction::validator::schema::Schema;

    /// Each paired entry of `case`, by its side, resolved.
    fn run(case: &Case) -> Vec<(&'static str, Resolved)> {
        let constructed = committed();
        let identity = identity(constructed);
        let bodies = data_bodies();
        case.sides()
            .into_iter()
            .filter_map(|(side, entry)| match entry {
                Side::Paired(input) => Some((
                    side,
                    resolved(&case.common, input, &bodies, constructed, &identity).unwrap(),
                )),
                Side::Unpaired(_) => None,
            })
            .collect()
    }

    /// The Finding records of both projections of `outcome`.
    fn findings(outcome: &Value) -> Vec<&Value> {
        ["shared", "entry"]
            .iter()
            .flat_map(|domain| {
                outcome["resolutionEvidence"][domain]["findings"]
                    .as_array()
                    .into_iter()
                    .flatten()
            })
            .map(|row| &row["record"])
            .collect()
    }

    /// Every case constructs the resolver this crate commits, the one the
    /// suite's successful construction case expects.
    #[test]
    fn every_case_names_the_construction_case_s_identity() {
        assert_eq!(
            identity(committed()),
            construction_cases::identity().unwrap()
        );
        for (_, done) in all(committed()).iter().flat_map(run) {
            assert_eq!(
                done.outcome["resolverConstructionIdentity"],
                identity(committed())
            );
        }
    }

    /// Each entry runs, breaks no duty, and its outcome is in the expected
    /// outcome's form, as this crate's validator and the oracle both read
    /// it.
    #[test]
    fn each_entry_runs_and_its_outcome_keeps_the_duties_and_the_form() {
        let schema = crate::conformance::case::schema().unwrap();
        let root = json!({
            "$schema": DRAFT_7,
            "$ref": "#/definitions/expectedOutcomeFixture",
            "definitions": schema["definitions"],
        });
        let read = Schema::compile(&root).unwrap();
        let oracle = jsonschema::draft7::new(&root).unwrap();
        for case in all(committed()) {
            for (side, done) in run(&case) {
                assert!(
                    duties(&done.outcome).is_empty(),
                    "{}/{side}: {:?}",
                    case.id,
                    duties(&done.outcome)
                );
                assert!(read.validate(&done.outcome).valid, "{}/{side}", case.id);
                assert!(oracle.is_valid(&done.outcome), "{}/{side}", case.id);
            }
        }
    }

    /// Both entries of a paired case reach one Materialized Resolution Input
    /// Set and agree where 015 makes them agree; a file-only case pairs no
    /// programmatic entry and has no Shared evidence.
    #[test]
    fn paired_entries_reach_one_set_and_agree() {
        for case in all(committed()) {
            let done = run(&case);
            match &done[..] {
                [("file", file), ("programmatic", programmatic)] => {
                    assert!(file.materialized.is_some(), "{}", case.id);
                    assert!(
                        file.materialized == programmatic.materialized,
                        "{}",
                        case.id
                    );
                    assert!(
                        paired(&file.outcome, &programmatic.outcome, true),
                        "{}",
                        case.id
                    );
                }
                [("file", file)] => {
                    assert!(file.materialized.is_none(), "{}", case.id);
                    assert!(file.outcome["resolutionEvidence"]["shared"].is_null());
                }
                _ => panic!("{} pairs neither side", case.id),
            }
        }
    }

    /// A case traces each check whose Finding its outcomes report, and no
    /// other.
    #[test]
    fn each_case_traces_the_checks_its_outcomes_report() {
        for case in all(committed()) {
            let mut reported = run(&case)
                .iter()
                .flat_map(|(_, done)| {
                    findings(&done.outcome)
                        .into_iter()
                        .map(|finding| finding["key"]["check"].as_str().unwrap().to_owned())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            reported.sort();
            reported.dedup();
            assert_eq!(reported, case.checks, "{}", case.id);
        }
    }

    /// The storefront resolves to a checked profile with a binding for each
    /// member of its collection, in ascending unsigned byte order of the
    /// keys' canonical bytes (D15:3303).
    #[test]
    fn the_storefront_resolves_to_a_checked_profile_with_each_binding() {
        let cases = all(committed());
        let case = cases
            .iter()
            .find(|case| case.id == "015-storefront-checked")
            .unwrap();
        for (_, done) in run(case) {
            let outcome = &done.outcome;
            assert_eq!(outcome["outcome"], "checked");
            assert_eq!(outcome["resolutionEvidence"]["complete"], true);
            assert!(findings(outcome).is_empty());
            let bindings = outcome["resolvedProfileArtifactBindings"]
                .as_array()
                .unwrap();
            assert_eq!(bindings.len(), Storefront::new().members.len());
            let keys = bindings
                .iter()
                .map(|row| intlify_shared_json::encoding::encode(&row["key"]).unwrap())
                .collect::<Vec<_>>();
            assert!(keys.windows(2).all(|pair| pair[0] < pair[1]));
        }
    }

    /// A file-only case names the reason its file entry is file-only: a
    /// case of raw JSON syntax submits bytes that are no JSON (D15:3235).
    #[test]
    fn a_file_only_case_names_why_its_file_entry_is_file_only() {
        let cases = all(committed());
        let mut file_only = 0;
        for case in &cases {
            let (Side::Unpaired(reason), Side::Paired(file)) = (&case.programmatic, &case.file)
            else {
                continue;
            };
            file_only += 1;
            let bytes = &file["configuration-file"]["input"]["bytes"]["bytes"];
            let bytes = crate::conformance::fixture::decode(bytes.as_str().unwrap());
            match *reason {
                "raw-json-syntax" => {
                    assert!(
                        serde_json::from_slice::<Value>(&bytes).is_err(),
                        "{}",
                        case.id
                    );
                }
                other => panic!("{}: no file-only case of {other}", case.id),
            }
        }
        assert_eq!(file_only, 2);
    }

    /// Every file the cases write lies below the Resolver Cases' own
    /// directories, the copy of the locale data among the fixtures.
    #[test]
    fn every_file_lies_below_the_resolver_cases_directories() {
        let written = cases().unwrap();
        assert!(written.files.iter().all(|file| {
            file.path.starts_with(&format!("{CASES}/"))
                || file.path.starts_with(&format!("{FIXTURES}/"))
        }));
        assert!(written
            .files
            .iter()
            .any(|file| file.path.starts_with(&format!("{DATA}/"))));
    }

    /// Exhausting Entry before any Shared evidence keeps the terminal Finding
    /// and the reserve's cause, both without an owner; exhausting Shared
    /// keeps the terminal Finding last in Shared, beside a complete Entry
    /// (D15:3271-3303).
    #[test]
    fn the_exhaustion_cases_keep_their_reserved_records() {
        let cases = all(committed());
        let find = |id: &str| cases.iter().find(|case| case.id == id).unwrap();
        let last = |outcome: &Value, domain: &str, name: &str| {
            outcome["resolutionEvidence"][domain][name]
                .as_array()
                .unwrap()
                .last()
                .unwrap()
                .clone()
        };
        for (_, done) in run(find("015-reporting-entry-limit-exceeded")) {
            let evidence = &done.outcome["resolutionEvidence"];
            assert_eq!(evidence["complete"], false);
            assert_eq!(evidence["terminalStates"][0]["domain"], "entry");
            assert!(evidence["shared"].is_null());
            let finding = last(&done.outcome, "entry", "findings");
            let cause = last(&done.outcome, "entry", "sharedUnavailabilityCauses");
            assert!(finding["owner"].is_null() && cause["owner"].is_null());
            assert_eq!(cause["record"]["cause"], finding["record"]["key"]);
            assert_eq!(
                evidence["sharedUnavailableCauseKeys"]
                    .as_array()
                    .unwrap()
                    .last(),
                Some(&finding["record"]["key"])
            );
        }
        for (_, done) in run(find("015-reporting-shared-limit-exceeded")) {
            let evidence = &done.outcome["resolutionEvidence"];
            assert_eq!(evidence["complete"], false);
            assert_eq!(evidence["terminalStates"].as_array().unwrap().len(), 1);
            assert_eq!(evidence["terminalStates"][0]["domain"], "shared");
            let finding = last(&done.outcome, "shared", "findings");
            assert!(finding["owner"].is_null());
            assert_eq!(
                finding["record"]["key"]["check"],
                "reporting.diagnostic-domain.complete"
            );
        }
    }

    /// An expected outcome reads back as the value written, with each record
    /// of a projection on a line of its own and the rest as two-space
    /// indentation writes it.
    #[test]
    fn an_expected_outcome_is_written_one_record_a_line() {
        let plain = json!({ "a": [1, { "b": [] }], "c": {}, "d": "e" });
        let mut pretty = serde_json::to_string_pretty(&plain).unwrap();
        pretty.push('\n');
        assert_eq!(expected_bytes(&plain).unwrap(), pretty.into_bytes());
        let outcome = json!({
            "resolvedProfileArtifactBindings": [{ "key": { "kind": "a" } }],
            "resolutionEvidence": {
                "terminalStates": [{ "domain": "shared" }],
                "shared": { "findings": [{ "owner": null, "record": { "a": [1] } }, { "x": 2 }] },
                "entry": { "evidenceIndex": [{ "y": [3] }] },
            },
        });
        let bytes = expected_bytes(&outcome).unwrap();
        assert_eq!(strict_json(&bytes), Some(outcome));
        let text = String::from_utf8(bytes).unwrap();
        for line in [
            r#"    {"key":{"kind":"a"}}"#,
            r#"      {"domain":"shared"}"#,
            r#"        {"owner":null,"record":{"a":[1]}},"#,
            r#"        {"x":2}"#,
            r#"        {"y":[3]}"#,
        ] {
            assert!(text.lines().any(|own| own == line), "{line}\n{text}");
        }
    }
}
