// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The suite's Resolver Input Admission Cases of the boundary kinds: for each
//! kind of the boundary table, one case its boundary admits, and one for each
//! row of the kind's failure precedence, refused with that row's reason
//! (D15:3140-3170, 3239-3269).
//!
//! Every case is evaluated under the constructing construction case, which
//! its `resolverConstruction` names with the identity of the committed
//! construction vector (D15:3158). A failing case's fixture is its kind's
//! admitted fixture, changed only where its row looks, so that every row
//! before it holds. A boundary that reads capability bounds takes them from
//! the fixture's capability value: a fixture owner's, with each bound these
//! boundaries read raised past every input here, and a row over a bound
//! lowers that bound alone. A raw input that is strict UTF-8 JSON is a file
//! of its own beside the fixture, and any other raw input is its bytes in
//! lowercase hexadecimal (D15:3243-3248).
//!
//! An admitted case expects the normalized input this generator derives from
//! the fixture itself, and a refused case its row's reason, so no case's
//! expectation is what the boundary it checks computes. A refusal shows only
//! its first reason (D15:3160), so two rows of a kind that share a reason are
//! told apart only by the inputs made for them, which differ. Each case traces the
//! decisions of its boundary and, as its rule IDs, its row's
//! `015.input-admission.<kind>.<reason>` pair, the admitted case each pair of
//! its kind (D15:2174).

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::base64url::CONSTRUCTION_IDENTITY_PREFIX;
use crate::boundary::capability::ImplementationCapability;
use crate::boundary::source::{call_site_source_revision, file_source_revision};
use crate::construction::component::file_digest;
use crate::construction::generate;
use crate::resolver::evidence_source::hex;
use crate::resolver::limits::fixture_limits;
use crate::resolver::origin::SafeOriginKind;

use super::case::{
    ManifestKind, BOUNDARY_PRECEDENCE, COMMON_SET, COMMON_SLOTS, ENTRY_SET, REVISION,
};
use super::construction_cases::{self, Cases, CONSTRUCTED};
use super::fixture::written;
use super::index::SuiteFile;

/// Where the manifests lie below the suite root.
const CASES: &str = "cases/admission";

/// Where the fixtures, and the raw inputs they keep in files, lie below the
/// suite root.
const FIXTURES: &str = "fixtures/admission";

/// A bound past every input here, which the capability value sets for each
/// bound these boundaries read.
const ROOMY: &str = "1048576";

/// The committed vectors these cases take their inputs from, which
/// independent checkers verify: vocabularies, envelopes, Verification Input
/// boundary inputs, and canonical Snapshots.
const VOCABULARY_VECTORS: &[u8] = include_bytes!("../../fixtures/vocabulary/vectors.json");
const ENVELOPE_VECTORS: &[u8] = include_bytes!("../../fixtures/envelope/vectors.json");
const VERIFICATION_VECTORS: &[u8] = include_bytes!("../../fixtures/verification/vectors.json");
const SNAPSHOT_VECTORS: &[u8] = include_bytes!("../../fixtures/snapshot/vectors.json");

/// The committed locale canonicalization data manifest.
const MANIFEST: &[u8] = include_bytes!("../../../intlify_locale_icu4x/data/manifest.json");

/// Where the locale data cases' placeholder payloads lie below the suite
/// root.
const PAYLOADS: &str = "fixtures/admission/locale-data";

/// The decisions every case traces: the dedicated case types and closed
/// reasons, the singular reason and pinned precedence, the construction the
/// case names, and the boundaries' order.
const SHARED_DECISIONS: [&str; 4] = ["015-096", "015-112", "015-122", "015-144"];

/// A kind's cases before they are written: the fixture its boundary admits
/// and the normalized input that case expects, and for each row of the
/// kind's failure precedence, in order, the fixture that row refuses.
struct KindCases {
    admitted: Value,
    normalized: Value,
    refused: Vec<Value>,
    /// The files the fixtures reference beyond their raw inputs.
    files: Vec<SuiteFile>,
}

/// One boundary kind of the table: its name, the short name its case IDs
/// use, the decisions its cases trace beyond the shared ones, and its cases.
struct Kind {
    name: &'static str,
    short: &'static str,
    decisions: &'static [&'static str],
    cases: fn(&Context) -> KindCases,
}

/// What the kinds' cases are written from: the capability value, and the
/// identity of the construction the cases name.
struct Context {
    capability: Value,
    identity: String,
}

/// Each boundary kind, in the order of the failure precedence.
const KINDS: [Kind; 13] = [
    Kind {
        name: "profile-resolution-specification-input",
        short: "specification",
        decisions: &["015-087", "015-165"],
        cases: specification,
    },
    Kind {
        name: "intent-surface-class-vocabulary-artifact",
        short: "vocabulary",
        decisions: &["015-099", "015-138"],
        cases: vocabulary,
    },
    Kind {
        name: "locale-canonicalization-data-artifact",
        short: "locale-data",
        decisions: &["015-121"],
        cases: locale_data,
    },
    Kind {
        name: "submitted-profile-resolution-artifact-collection",
        short: "collection",
        decisions: &["015-040", "015-121"],
        cases: collection,
    },
    Kind {
        name: "resource-limit-policy-verification-input",
        short: "verification",
        decisions: &["015-120", "015-121"],
        cases: verification,
    },
    Kind {
        name: "safe-origin-kind",
        short: "origin",
        decisions: &["015-084", "015-166"],
        cases: origin,
    },
    Kind {
        name: "external-profile-selector-binding",
        short: "selector",
        decisions: &["015-167"],
        cases: selector,
    },
    Kind {
        name: "file-source-identity-revision",
        short: "file-source",
        decisions: &["015-080", "015-149"],
        cases: file_source,
    },
    Kind {
        name: "programmatic-entry-identity-revision",
        short: "entry-source",
        decisions: &["015-123", "015-149"],
        cases: entry_source,
    },
    Kind {
        name: "programmatic-entry-snapshot",
        short: "snapshot",
        decisions: &["015-146", "015-200"],
        cases: snapshot,
    },
    Kind {
        name: "programmatic-call-site-evidence-input",
        short: "call-site",
        decisions: &["015-123", "015-204"],
        cases: call_site,
    },
    Kind {
        name: "implementation-capability",
        short: "capability",
        decisions: &["015-101", "015-110"],
        cases: implementation_capability,
    },
    Kind {
        name: "resolver-construction-identity-reference",
        short: "construction-identity",
        decisions: &["015-117"],
        cases: construction_identity,
    },
];

/// The capability value of a case whose boundary reads bounds: a fixture
/// owner's, with every bound of the namespaces these boundaries read, and the
/// selector's, raised to [`ROOMY`].
pub(crate) fn capability() -> Value {
    let mut value = ImplementationCapability::fixture_owned(fixture_limits(1)).normalized();
    for namespace in [
        "inputBoundary",
        "snapshot",
        "evidenceMetadata",
        "localeData",
    ] {
        let bounds = value["bootstrapOnly"][namespace]
            .as_object_mut()
            .expect("the capability has each namespace");
        for bound in bounds.values_mut() {
            *bound = json!(ROOMY);
        }
    }
    value["policyComparable"]["configurationInput"]["maxProfileIdBytes"] = json!(ROOMY);
    value
}

/// The capability value with the bound at `pointer` set to `value`.
fn lowered(capability: &Value, pointer: &str, value: usize) -> Value {
    let mut capability = capability.clone();
    *capability
        .pointer_mut(pointer)
        .expect("the capability has the bound") = json!(value.to_string());
    capability
}

/// A raw input of `bytes`.
fn raw(bytes: &[u8]) -> Value {
    json!({ "bytes": hex(bytes) })
}

/// A raw input of `value`'s JSON text.
fn raw_json(value: &Value) -> Value {
    raw(&serde_json::to_vec(value).expect("a JSON value serializes"))
}

/// A fixture of a boundary that reads capability bounds: the capability
/// value and, unless absent, the input.
fn reads(capability: &Value, input: Option<Value>) -> Value {
    let mut fixture = json!({ "capability": { "json": capability } });
    if let Some(input) = input {
        fixture["input"] = input;
    }
    fixture
}

/// `value` with every object's members in ascending unsigned byte order of
/// their names, as an admitted input states its decoded value (D15:3258).
fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(members) => {
            let mut names = members.keys().collect::<Vec<_>>();
            names.sort();
            Value::Object(
                names
                    .into_iter()
                    .map(|name| (name.clone(), sorted(&members[name])))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

/// The Profile Resolution Specification Input: every slot revision `"0"`
/// supports, refused when absent, over its byte bound, not one JSON value,
/// not an object, with a repeated or an unknown member, or with a field
/// outside its grammar.
fn specification(context: &Context) -> KindCases {
    let input = json!({
        "project-profile": { "identity": "intlify-project-profile", "revision": "0" },
        "locale-canonicalization": { "identity": "intlify-locale-canonicalization", "revision": "0" },
        "locale-negotiation": { "identity": "intlify-locale-negotiation", "revision": "0" },
        "message-locale-fallback": { "identity": "intlify-message-locale-fallback", "revision": "0" },
        "coverage": { "identity": "intlify-localization-coverage", "revision": "0" },
        "intent-surface-class-vocabulary": {
            "identity": "storefront-surfaces",
            "revision": "1",
            "semanticDigest": format!("sha256:{}", "1".repeat(64)),
        },
        "delivery-graph": { "identity": "intlify-delivery-graph", "revision": "0" },
        "delivery-placement": { "identity": "intlify-delivery-placement", "revision": "0" },
    });
    let text = serde_json::to_string(&input).expect("a JSON value serializes");
    let capability = &context.capability;
    let with = |input: Value| reads(capability, Some(input));
    let edited = |edit: &dyn Fn(&mut Value)| {
        let mut value = input.clone();
        edit(&mut value);
        with(raw_json(&value))
    };
    let repeated = prefixed(
        &text,
        r#""coverage":{"identity":"intlify-localization-coverage","revision":"0"}"#,
    );
    KindCases {
        admitted: with(raw(text.as_bytes())),
        normalized: json!({ "json": sorted(&input) }),
        refused: vec![
            reads(capability, None),
            reads(
                &lowered(
                    capability,
                    "/bootstrapOnly/inputBoundary/maxSpecificationInputBytes",
                    text.len() - 1,
                ),
                Some(raw(text.as_bytes())),
            ),
            with(raw(b"{")),
            with(raw(b"[]")),
            with(raw(repeated.as_bytes())),
            edited(&|value| value["schemaRevision"] = json!("0")),
            edited(&|value| {
                value["coverage"]["identity"] = json!("Intlify-Localization-Coverage");
            }),
        ],
        files: Vec::new(),
    }
}

/// The Safe Origin Kind of the `configuration-file` slot: its own ID,
/// refused when absent, not a string, unregistered, a case or whitespace
/// variant, or another slot's ID (D15:3162).
fn origin(_: &Context) -> KindCases {
    let slot = SafeOriginKind::ConfigurationFile.id();
    let with = |candidate: Option<Value>| {
        let mut fixture = json!({ "slot": slot });
        if let Some(candidate) = candidate {
            fixture["candidate"] = candidate;
        }
        fixture
    };
    KindCases {
        admitted: with(Some(json!(slot))),
        normalized: json!({ "safeOriginKind": slot }),
        refused: vec![
            with(None),
            with(Some(Value::Null)),
            with(Some(json!("configuration -file"))),
            with(Some(json!(" Configuration-File\t"))),
            with(Some(json!(SafeOriginKind::ConfigurationProgrammatic.id()))),
        ],
        files: Vec::new(),
    }
}

/// The selector's closed report: a string within the capability's
/// `maxProfileIdBytes`, refused as an unsafe inspection, as unrepresentable,
/// or as a string past the bound (D15:3252; Decision 015-167).
fn selector(context: &Context) -> KindCases {
    let capability = &context.capability;
    let report = json!({ "report": "string", "value": "app" });
    KindCases {
        admitted: reads(capability, Some(report.clone())),
        normalized: json!({ "selector": report }),
        refused: vec![
            reads(capability, Some(json!({ "report": "unsafe" }))),
            reads(capability, Some(json!({ "report": "unrepresentable" }))),
            reads(
                &lowered(
                    capability,
                    "/policyComparable/configurationInput/maxProfileIdBytes",
                    2,
                ),
                Some(report),
            ),
        ],
        files: Vec::new(),
    }
}

/// The Resolver Construction Identity reference: the constructed identity,
/// refused when absent, when no text, when its text presents no identity,
/// with the lowest unused bit of its last character set, and when it presents
/// another identity, its first digest character the next of the alphabet.
fn construction_identity(context: &Context) -> KindCases {
    let identity = &context.identity;
    let with = |reference: Option<Value>| match reference {
        Some(reference) => json!({ "input": { "json": reference } }),
        None => json!({}),
    };
    let mut unused_bit = identity.clone().into_bytes();
    *unused_bit.last_mut().expect("an identity has text") += 1;
    let unused_bit = String::from_utf8(unused_bit).expect("the next character is ASCII");
    let digest = &identity[CONSTRUCTION_IDENTITY_PREFIX.len()..];
    let first = digest.as_bytes()[0];
    let other = format!(
        "{CONSTRUCTION_IDENTITY_PREFIX}{}{}",
        char::from(if first == b'Z' { b'a' } else { first + 1 }),
        &digest[1..]
    );
    KindCases {
        admitted: with(Some(json!(identity))),
        normalized: json!({ "resolverConstructionIdentity": identity }),
        refused: vec![
            with(None),
            with(Some(json!([identity]))),
            with(Some(json!(unused_bit))),
            with(Some(json!(other))),
        ],
        files: Vec::new(),
    }
}

/// `text`, a JSON object, with `member` in front of its first member.
fn prefixed(text: &str, member: &str) -> String {
    format!("{{{member},{}", &text[1..])
}

/// The member `id` of the array `array` of committed vectors.
fn vector(vectors: &[u8], array: &str, id: &str) -> Value {
    let vectors: Value = serde_json::from_slice(vectors).expect("committed vectors are JSON");
    vectors[array]
        .as_array()
        .expect("the vectors list them")
        .iter()
        .find(|vector| vector["id"] == id)
        .expect("the vectors have it")
        .clone()
}

/// A count a committed vector states as decimal text.
fn counted(text: &Value) -> usize {
    text.as_str()
        .and_then(|text| text.parse().ok())
        .expect("a vector states its counts as decimal text")
}

/// The Intent Surface-Class Vocabulary Artifact of the committed `fixture`
/// vocabulary vector, refused when absent, over its byte bound, cut short,
/// with more members than its count bound, with an identity outside its
/// grammar, or with a repeated or an unknown member.
fn vocabulary(context: &Context) -> KindCases {
    let artifact = vector(VOCABULARY_VECTORS, "vocabularies", "fixture")["artifact"].clone();
    let text = serde_json::to_string(&artifact).expect("a JSON value serializes");
    let members = artifact["members"]
        .as_array()
        .expect("a vocabulary lists its members")
        .len();
    let capability = &context.capability;
    let with = |bytes: &[u8]| reads(capability, Some(raw(bytes)));
    let mut renamed = artifact.clone();
    renamed["identity"] = json!("Intlify");
    let bound = |name: &str, value: usize| {
        lowered(
            capability,
            &format!("/bootstrapOnly/inputBoundary/{name}"),
            value,
        )
    };
    KindCases {
        admitted: with(text.as_bytes()),
        normalized: json!({ "json": sorted(&artifact) }),
        refused: vec![
            reads(capability, None),
            reads(
                &bound("maxVocabularyBytes", text.len() - 1),
                Some(raw(text.as_bytes())),
            ),
            with(&text.as_bytes()[..text.len() - 1]),
            reads(
                &bound("maxVocabularyMembers", members - 1),
                Some(raw(text.as_bytes())),
            ),
            reads(capability, Some(raw_json(&renamed))),
            with(prefixed(&text, r#""revision":"9""#).as_bytes()),
            with(prefixed(&text, r#""description":"x""#).as_bytes()),
        ],
        files: Vec::new(),
    }
}

/// The committed locale canonicalization data manifest, with a placeholder
/// payload at the address it names for its conformance role and another at
/// an address it does not name, since the boundary reads no payload's bytes;
/// refused when absent, over its byte bound, cut short, of another schema
/// revision, or with a repeated or an unknown member.
fn locale_data(context: &Context) -> KindCases {
    let text = std::str::from_utf8(MANIFEST).expect("the manifest is UTF-8");
    let manifest: Value = serde_json::from_slice(MANIFEST).expect("the manifest is JSON");
    let payload = |name: &str, bytes: &[u8]| {
        let path = format!("{PAYLOADS}/{name}");
        let reference = json!({ "path": path, "digest": file_digest(bytes).as_str() });
        let file = SuiteFile {
            path,
            bytes: bytes.to_vec(),
        };
        (reference, file)
    };
    let (named, named_file) = payload("conformance.json", b"{}\n");
    let (unnamed, unnamed_file) = payload("unnamed.json", b"[]\n");
    let payloads = json!({ "conformance.json": named, "unnamed.json": unnamed });
    let capability = &context.capability;
    let with = |capability: &Value, bytes: &[u8]| {
        reads(
            capability,
            Some(json!({ "manifest": raw(bytes), "payloads": payloads })),
        )
    };
    let mut revised = manifest.clone();
    revised["schemaRevision"] = json!("1");
    let admitted = json!({ "manifest": manifest, "payloads": { "conformance.json": named } });
    KindCases {
        admitted: with(capability, MANIFEST),
        normalized: json!({ "json": sorted(&admitted) }),
        refused: vec![
            reads(capability, None),
            with(
                &lowered(
                    capability,
                    "/bootstrapOnly/inputBoundary/maxLocaleDataManifestBytes",
                    MANIFEST.len() - 1,
                ),
                MANIFEST,
            ),
            with(capability, &MANIFEST[..MANIFEST.len() - 1]),
            with(
                capability,
                &serde_json::to_vec(&revised).expect("a JSON value serializes"),
            ),
            with(
                capability,
                prefixed(text, r#""kind":"locale-canonicalization-data""#).as_bytes(),
            ),
            with(capability, prefixed(text, r#""comment":"x""#).as_bytes()),
        ],
        files: vec![named_file, unnamed_file],
    }
}

/// A collection of the committed approval-policy envelope vector, refused
/// when absent, unsafe to inspect, not a formal container, with a member
/// over its byte bound, with a member cut short, or with a member that holds
/// a number.
fn collection(context: &Context) -> KindCases {
    let member = vector(ENVELOPE_VECTORS, "members", "approval-policy")["member"].clone();
    let text = serde_json::to_string(&member).expect("a JSON value serializes");
    let capability = &context.capability;
    let members = |capability: &Value, members: &[Value]| {
        reads(capability, Some(json!({ "members": members })))
    };
    let mut numbered = member.clone();
    numbered["schemaRevision"] = json!(0);
    KindCases {
        admitted: members(capability, &[raw(text.as_bytes())]),
        normalized: json!({ "members": [{ "json": member }] }),
        refused: vec![
            reads(capability, None),
            reads(capability, Some(json!({ "hostInspection": "unsafe" }))),
            reads(capability, Some(json!({ "container": "malformed" }))),
            members(
                &lowered(
                    capability,
                    "/bootstrapOnly/inputBoundary/maxCollectionMemberBytes",
                    text.len() - 1,
                ),
                &[raw(text.as_bytes())],
            ),
            members(capability, &[raw(&text.as_bytes()[..text.len() - 1])]),
            members(capability, &[raw_json(&numbered)]),
        ],
        files: Vec::new(),
    }
}

/// The committed Verification Input boundary vectors: the admitted input
/// with the assertion it makes about itself, refused when absent, over its
/// byte or pin bound, or as the vector of each other row, each with its
/// assertion.
fn verification(context: &Context) -> KindCases {
    let capability = &context.capability;
    let with = |capability: &Value, id: &str| {
        let vector = vector(VERIFICATION_VECTORS, "boundary", id);
        let text = vector["text"].as_str().expect("a vector has its text");
        json!({
            "assertion": vector["assertion"],
            "capability": { "json": capability },
            "input": raw(text.as_bytes()),
        })
    };
    let admitted = vector(VERIFICATION_VECTORS, "boundary", "admitted");
    let text = admitted["text"].as_str().expect("a vector has its text");
    let input: Value = serde_json::from_str(text).expect("the admitted input is JSON");
    let pins = input["pins"]
        .as_array()
        .expect("an input lists its pins")
        .len();
    let mut missing = with(capability, "admitted");
    missing
        .as_object_mut()
        .expect("a fixture is an object")
        .remove("input");
    let bound = |name: &str, value: usize| {
        lowered(
            capability,
            &format!("/bootstrapOnly/inputBoundary/{name}"),
            value,
        )
    };
    let mut refused = vec![
        missing,
        with(
            &bound("maxVerificationInputBytes", text.len() - 1),
            "admitted",
        ),
        with(capability, "syntax"),
        with(&bound("maxVerificationPins", pins - 1), "admitted"),
    ];
    for id in [
        "another-kind",
        "repeated-member",
        "unknown-member",
        "private-key",
        "trust-store",
        "stale-integrity",
        "identity-mismatch",
        "revision-mismatch",
        "digest-mismatch",
    ] {
        refused.push(with(capability, id));
    }
    KindCases {
        admitted: with(capability, "admitted"),
        normalized: json!({ "json": sorted(&input) }),
        refused,
        files: Vec::new(),
    }
}

/// The cases of an identity and revision boundary: the pair beside its
/// `content` member, with the mark `non_secret`; refused when the identity or
/// the revision is absent or over its bound, when the identity is
/// `unstable`, when the revision is presented in upper case, or when it is
/// `other`.
fn pair_cases(
    context: &Context,
    content: (&str, &Value),
    non_secret: bool,
    identity: &str,
    revision: &str,
    unstable: &str,
    other: &str,
) -> KindCases {
    let capability = &context.capability;
    let pair = |capability: &Value, identity: Option<&str>, revision: Option<&str>| {
        let mut input = json!({ "nonSecret": non_secret });
        input[content.0] = content.1.clone();
        if let Some(identity) = identity {
            input["identity"] = json!(identity);
        }
        if let Some(revision) = revision {
            input["revision"] = json!(revision);
        }
        json!({ "capability": { "json": capability }, "input": input })
    };
    let bound = |name: &str, value: &str| {
        lowered(
            capability,
            &format!("/bootstrapOnly/evidenceMetadata/{name}"),
            value.len() - 1,
        )
    };
    let upper = format!(
        "sha256:{}",
        revision["sha256:".len()..].to_ascii_uppercase()
    );
    KindCases {
        admitted: pair(capability, Some(identity), Some(revision)),
        normalized: json!({ "identity": identity, "nonSecret": non_secret, "revision": revision }),
        refused: vec![
            pair(capability, None, Some(revision)),
            pair(capability, Some(identity), None),
            pair(
                &bound("maxSourceIdentityBytes", identity),
                Some(identity),
                Some(revision),
            ),
            pair(
                &bound("maxSourceRevisionBytes", revision),
                Some(identity),
                Some(revision),
            ),
            pair(capability, Some(unstable), Some(revision)),
            pair(capability, Some(identity), Some(&upper)),
            pair(capability, Some(identity), Some(other)),
        ],
        files: Vec::new(),
    }
}

/// A configuration file's identity and the revision of its bytes, marked
/// non-secret; the other revision is that of other bytes.
fn file_source(context: &Context) -> KindCases {
    let content = b"{\"schemaVersion\":\"0\"}\n";
    pair_cases(
        context,
        ("bytes", &raw(content)),
        true,
        "intlify-file-source.v0:storefront/intlify.config.json",
        &file_source_revision(content),
        "intlify-file-source:storefront/intlify.config.json",
        &file_source_revision(b"{}"),
    )
}

/// A Programmatic Entry's identity, not marked non-secret, and the digest of
/// its canonical Snapshot, the committed `null` vector; the other revision is
/// the `false` vector's digest.
fn entry_source(context: &Context) -> KindCases {
    let entry = vector(SNAPSHOT_VECTORS, "snapshots", "null");
    let other = vector(SNAPSHOT_VECTORS, "snapshots", "false");
    let digest = entry["digest"].as_str().expect("a vector has its digest");
    pair_cases(
        context,
        (
            "snapshot",
            &json!({ "snapshot": entry["snapshot"], "digest": digest }),
        ),
        false,
        "intlify-programmatic-entry.v0:storefront",
        digest,
        "intlify-programmatic-entry:storefront",
        other["digest"].as_str().expect("a vector has its digest"),
    )
}

/// The committed `array-of-scalars` Snapshot vector, refused when absent,
/// unsafe to inspect, over its node bound, with a node of a variant 015 does
/// not name, with a number past the portable range, or with another
/// Snapshot's digest.
fn snapshot(context: &Context) -> KindCases {
    let vector = vector(SNAPSHOT_VECTORS, "snapshots", "array-of-scalars");
    let other = self::vector(SNAPSHOT_VECTORS, "snapshots", "null")["digest"].clone();
    let snapshot = json!({ "snapshot": vector["snapshot"], "digest": vector["digest"] });
    let capability = &context.capability;
    let with = |input: Value| reads(capability, Some(input));
    let changed = |pointer: &str, value: Value| {
        let mut changed = snapshot.clone();
        *changed
            .pointer_mut(pointer)
            .expect("the Snapshot has the node") = value;
        with(changed)
    };
    KindCases {
        admitted: with(snapshot.clone()),
        normalized: snapshot.clone(),
        refused: vec![
            reads(capability, None),
            with(json!({ "hostInspection": "unsafe" })),
            reads(
                &lowered(
                    capability,
                    "/bootstrapOnly/snapshot/maxNodes",
                    counted(&vector["counts"]["maxNodes"]) - 1,
                ),
                Some(snapshot.clone()),
            ),
            changed("/snapshot/array/0", json!({ "undefined": null })),
            changed("/snapshot/array/2", json!({ "number": "9007199254740992" })),
            changed("/digest", other),
        ],
        files: Vec::new(),
    }
}

/// A call-site sidecar over a configuration call's source, with a locator,
/// its mappings out of canonical order, and one key as code units; refused
/// when unsafe to inspect, over the source's byte bound or the mappings'
/// count bound, with a role 015 does not name, with a source that is not
/// UTF-8, with a repeated mapping, without its identity or revision, with an
/// identity without a versioned tag, with a revision out of its presentation,
/// or with the file source revision of the same bytes.
fn call_site(context: &Context) -> KindCases {
    let source = "defineConfig({ profiles: { app: {} } }) // \u{8a2d}\u{5b9a}\n";
    let bytes = source.as_bytes();
    let identity = "intlify-call-site.v0:storefront/app.ts";
    let revision = call_site_source_revision(bytes);
    let mapping = |path: Value, role: &str, start: u64, end: u64| json!({ "path": path, "role": role, "span": { "start": start, "end": end } });
    let profiles = json!({ "key": "profiles" });
    let mappings = vec![
        mapping(
            json!([profiles, { "keyUtf16": [97, 112, 112] }]),
            "member-value",
            32,
            34,
        ),
        mapping(json!([]), "node", 13, 38),
        mapping(json!([profiles]), "member-key", 15, 23),
        mapping(json!([profiles]), "member-value", 25, 36),
    ];
    let capability = &context.capability;
    let sidecar = |capability: &Value,
                   identity: Option<&str>,
                   revision: Option<&str>,
                   bytes: &[u8],
                   mappings: &[Value]| {
        let mut input = json!({
            "nonSecret": false,
            "locator": { "variant": "safe-label", "payload": "app.ts", "nonSecret": true },
            "source": raw(bytes),
            "mappings": mappings,
        });
        if let Some(identity) = identity {
            input["identity"] = json!(identity);
        }
        if let Some(revision) = revision {
            input["revision"] = json!(revision);
        }
        json!({ "capability": { "json": capability }, "input": input })
    };
    let complete = |identity: Option<&str>, revision: Option<&str>| {
        sidecar(capability, identity, revision, bytes, &mappings)
    };
    let bound = |name: &str, value: usize| {
        lowered(
            capability,
            &format!("/bootstrapOnly/evidenceMetadata/{name}"),
            value,
        )
    };
    let mut unnamed = mappings.clone();
    unnamed[2]["role"] = json!("value");
    let mut unreadable = bytes.to_vec();
    unreadable.push(0xff);
    let mut repeated = mappings.clone();
    repeated.push(mapping(json!([]), "node", 0, 1));
    let upper = format!(
        "sha256:{}",
        revision["sha256:".len()..].to_ascii_uppercase()
    );
    let canonical = json!([
        mapping(json!([]), "node", 13, 38),
        mapping(json!([profiles]), "member-key", 15, 23),
        mapping(json!([profiles]), "member-value", 25, 36),
        mapping(json!([profiles, { "key": "app" }]), "member-value", 32, 34),
    ]);
    let admitted = json!({
        "identity": identity,
        "nonSecret": false,
        "revision": revision,
        "locator": { "safe-label": "app.ts" },
        "source": source,
        "mappings": canonical,
    });
    let unsafe_host = json!({
        "capability": { "json": capability },
        "input": { "hostInspection": "unsafe" },
    });
    KindCases {
        admitted: complete(Some(identity), Some(&revision)),
        normalized: json!({ "json": sorted(&admitted) }),
        refused: vec![
            unsafe_host,
            sidecar(
                &bound("maxCallSiteSourceBytes", bytes.len() - 1),
                Some(identity),
                Some(&revision),
                bytes,
                &mappings,
            ),
            sidecar(
                &bound("maxCallSiteMappings", mappings.len() - 1),
                Some(identity),
                Some(&revision),
                bytes,
                &mappings,
            ),
            sidecar(capability, Some(identity), Some(&revision), bytes, &unnamed),
            sidecar(
                capability,
                Some(identity),
                Some(&revision),
                &unreadable,
                &mappings,
            ),
            sidecar(
                capability,
                Some(identity),
                Some(&revision),
                bytes,
                &repeated,
            ),
            complete(None, Some(&revision)),
            complete(Some(identity), None),
            complete(Some("intlify-call-site:storefront/app.ts"), Some(&revision)),
            complete(Some(identity), Some(&upper)),
            complete(Some(identity), Some(&file_source_revision(bytes))),
        ],
        files: Vec::new(),
    }
}

/// The capability value itself, refused when absent, with a member outside
/// its closed shape, for another Capability Specification's identity or
/// revision, with a bound 015 does not name, without one of its bounds, or
/// with a bound that is not canonical, not positive, not representable, or
/// below its bootstrap minimum.
fn implementation_capability(context: &Context) -> KindCases {
    let capability = &context.capability;
    let with = |value: Value| json!({ "input": { "json": value } });
    let changed = |change: &dyn Fn(&mut Value)| {
        let mut value = capability.clone();
        change(&mut value);
        with(value)
    };
    KindCases {
        admitted: with(capability.clone()),
        normalized: json!({ "json": capability }),
        refused: vec![
            json!({}),
            changed(&|value| value["profile"] = json!({})),
            changed(&|value| value["specification"]["identity"] = json!("another-capability")),
            changed(&|value| value["specification"]["revision"] = json!("1")),
            changed(&|value| value["bootstrapOnly"]["rawInput"]["maxUnknownBytes"] = json!("1")),
            changed(&|value| {
                value["bootstrapOnly"]["rawInput"]
                    .as_object_mut()
                    .expect("the capability has its raw input bounds")
                    .remove("maxParserTokens");
            }),
            changed(&|value| {
                value["policyComparable"]["configurationInput"]["maxNodes"] = json!("01");
            }),
            changed(&|value| value["bootstrapOnly"]["rawInput"]["maxFileBytes"] = json!("0")),
            changed(&|value| {
                value["bootstrapOnly"]["rawInput"]["maxFileBytes"] = json!(u64::MAX.to_string());
            }),
            changed(&|value| {
                value["bootstrapOnly"]["snapshot"]["maxEnvelopeCanonicalBytes"] = json!("4095");
            }),
        ],
        files: Vec::new(),
    }
}

/// A kind's failure precedence rows.
fn rows(kind: &str) -> &'static [(&'static str, &'static str)] {
    BOUNDARY_PRECEDENCE
        .iter()
        .find(|(name, _)| *name == kind)
        .map(|(_, rows)| *rows)
        .expect("every kind has its precedence")
}

/// A pair's pre-invocation rule ID (D15:2174).
pub(super) fn rule(kind: &str, reason: &str) -> String {
    format!("015.input-admission.{kind}.{reason}")
}

/// The ID of the case for a kind's row: `015-admission-`, the kind's short
/// name, and the row's check, followed by its reason when another row of the
/// kind takes the same check.
fn case_id(kind: &Kind, (check, reason): (&str, &str)) -> String {
    let shared = rows(kind.name)
        .iter()
        .filter(|(other, _)| *other == check)
        .count()
        > 1;
    if shared {
        format!("015-admission-{}-{check}-{reason}", kind.short)
    } else {
        format!("015-admission-{}-{check}", kind.short)
    }
}

/// What a refused case expects: no input and its row's reason.
fn refused(reason: &str) -> Value {
    json!({
        "admitted": false,
        "normalizedInput": null,
        "failureReason": reason,
        "resolverInvoked": false,
    })
}

/// One case before it is written.
struct Case {
    id: String,
    kind: &'static str,
    fixture: Value,
    expected: Value,
    decisions: Vec<&'static str>,
    rules: Vec<String>,
}

/// Every case: each kind's admitted one, and one for each of its rows, in
/// ascending ID order, then the two admission steps' cases in ascending ID
/// order, so that a step's raw input is the file a kind's case wrote; and
/// the files the fixtures reference beyond their raw inputs.
fn all() -> Result<(Vec<Case>, Vec<SuiteFile>), serde_json::Error> {
    let context = Context {
        capability: capability(),
        identity: construction_cases::identity()?,
    };
    let mut cases = Vec::new();
    let mut files = Vec::new();
    let mut kinds = BTreeMap::new();
    for kind in &KINDS {
        let built = (kind.cases)(&context);
        files.extend(built.files);
        let rows = rows(kind.name);
        kinds.insert(
            kind.name,
            Built {
                admitted: built.admitted.clone(),
                normalized: built.normalized.clone(),
                refused: rows.iter().copied().zip(built.refused.clone()).collect(),
            },
        );
        let decisions = SHARED_DECISIONS
            .iter()
            .chain(kind.decisions)
            .copied()
            .collect::<Vec<_>>();
        let mut pairs = Vec::new();
        for (_, reason) in rows {
            let pair = rule(kind.name, reason);
            if !pairs.contains(&pair) {
                pairs.push(pair);
            }
        }
        cases.push(Case {
            id: format!("015-admission-{}-admitted", kind.short),
            kind: kind.name,
            fixture: built.admitted,
            expected: json!({
                "admitted": true,
                "normalizedInput": built.normalized,
                "resolverInvoked": false,
            }),
            decisions: decisions.clone(),
            rules: pairs,
        });
        for (row, fixture) in rows.iter().zip(built.refused) {
            cases.push(Case {
                id: case_id(kind, *row),
                kind: kind.name,
                fixture,
                expected: refused(row.1),
                decisions: decisions.clone(),
                rules: vec![rule(kind.name, row.1)],
            });
        }
    }
    cases.sort_by(|left, right| left.id.cmp(&right.id));
    let mut steps = steps(&context, &kinds);
    steps.sort_by(|left, right| left.id.cmp(&right.id));
    cases.extend(steps);
    Ok((cases, files))
}

/// What the admission steps' cases take from a boundary kind's cases: the
/// fixture its boundary admits, the normalized input that case expects, and
/// each row with the fixture it refuses.
struct Built {
    admitted: Value,
    normalized: Value,
    refused: Vec<((&'static str, &'static str), Value)>,
}

impl Built {
    /// The input of the fixture the row of this check and reason refuses.
    fn input(&self, check: &str, reason: &str) -> Value {
        self.refused
            .iter()
            .find(|(row, _)| *row == (check, reason))
            .map(|(_, fixture)| fixture["input"].clone())
            .expect("the kind has the row")
    }
}

/// The decisions the common step's cases trace beyond the shared ones: the
/// step, and its case kind.
const COMMON_DECISIONS: [&str; 2] = ["015-168", "015-169"];

/// The decisions a file entry's step cases trace beyond the shared ones.
const FILE_DECISIONS: [&str; 2] = ["015-172", "015-173"];

/// The decisions a programmatic entry's step cases trace beyond the shared
/// ones.
const PROGRAMMATIC_DECISIONS: [&str; 2] = ["015-173", "015-201"];

/// A step case's failure: its slot, its boundary kind, and its reason.
type Failure = (SafeOriginKind, &'static str, &'static str);

/// The origin boundary's kind.
const ORIGIN: &str = "safe-origin-kind";

/// A step case: its ID's tail, its kind, its decisions, its fixture, and the
/// admitted set it expects, or its failures. An admitted case traces every
/// pair of its step's boundaries, and a refused one each pair it fails with.
fn step_case(
    tail: &str,
    kind: &'static str,
    decisions: &[&'static str],
    fixture: Value,
    expected: Result<Value, &[Failure]>,
    boundaries: &[&str],
) -> Case {
    let mut rules = Vec::new();
    let expected = match expected {
        Ok(normalized) => {
            for boundary in boundaries {
                for (_, reason) in rows(boundary) {
                    let pair = rule(boundary, reason);
                    if !rules.contains(&pair) {
                        rules.push(pair);
                    }
                }
            }
            json!({ "admitted": true, "normalizedInput": normalized, "resolverInvoked": false })
        }
        Err(failures) => {
            for (_, boundary, reason) in failures {
                let pair = rule(boundary, reason);
                if !rules.contains(&pair) {
                    rules.push(pair);
                }
            }
            let failures = failures
                .iter()
                .map(|(slot, boundary, reason)| {
                    json!({ "slot": slot.id(), "boundaryKind": boundary, "failureReason": reason })
                })
                .collect::<Vec<_>>();
            json!({
                "admitted": false,
                "normalizedInput": null,
                "failures": failures,
                "resolverInvoked": false,
            })
        }
    };
    Case {
        id: format!("015-admission-{tail}"),
        kind,
        fixture,
        expected,
        decisions: SHARED_DECISIONS.iter().chain(decisions).copied().collect(),
        rules,
    }
}

/// The common step's cases (D15:3164, 3166): every slot's admitted input; and
/// its failures in slot rank, under a refused capability, which leaves a
/// refused vocabulary unevaluated, and of one slot's origin before its
/// value.
fn common_steps(kinds: &BTreeMap<&str, Built>) -> Vec<Case> {
    use SafeOriginKind as Slot;
    let kind = |name: &str| &kinds[name];
    let mut fixture = json!({
        "assertion": kind("resource-limit-policy-verification-input").admitted["assertion"],
    });
    let mut normalized = Map::new();
    for (slot, name) in COMMON_SLOTS {
        fixture[slot.id()] =
            json!({ "candidate": slot.id(), "input": kind(name).admitted["input"] });
        normalized.insert(slot.id().to_owned(), kind(name).normalized.clone());
    }
    let with = |change: &dyn Fn(&mut Value)| {
        let mut changed = fixture.clone();
        change(&mut changed);
        changed
    };
    let vocabulary = "intent-surface-class-vocabulary-artifact";
    let verification = "resource-limit-policy-verification-input";
    let capability = "implementation-capability";
    let malformed = kind(vocabulary).input("strict-syntax", "malformed-envelope");
    let boundaries = COMMON_SLOTS.map(|(_, name)| name);
    let case = |tail: &str, fixture: Value, expected: Result<Value, &[Failure]>| {
        step_case(
            tail,
            COMMON_SET,
            &COMMON_DECISIONS,
            fixture,
            expected,
            &[&[ORIGIN][..], &boundaries[..]].concat(),
        )
    };
    vec![
        case(
            "common-admitted",
            fixture.clone(),
            Ok(Value::Object(normalized)),
        ),
        case(
            "common-rank-order",
            with(&|fixture| {
                fixture[Slot::ProfileResolutionSpecificationInput.id()]["candidate"] =
                    json!(" profile-resolution-specification-input");
                fixture[Slot::IntentSurfaceClassVocabulary.id()]["input"] = malformed.clone();
                remove(
                    fixture,
                    Slot::ProfileResolutionArtifactCollection,
                    "candidate",
                );
            }),
            Err(&[
                (
                    Slot::ProfileResolutionSpecificationInput,
                    ORIGIN,
                    "non-canonical-safe-origin-kind",
                ),
                (
                    Slot::IntentSurfaceClassVocabulary,
                    vocabulary,
                    "malformed-envelope",
                ),
                (
                    Slot::ProfileResolutionArtifactCollection,
                    ORIGIN,
                    "missing-safe-origin-kind",
                ),
            ]),
        ),
        case(
            "common-refused-capability",
            with(&|fixture| {
                fixture[Slot::ImplementationCapability.id()]["input"] =
                    kind(capability).input("closed-shape", "malformed-capability");
                fixture[Slot::LocaleCanonicalizationData.id()]["candidate"] = json!("x");
                fixture[Slot::IntentSurfaceClassVocabulary.id()]["input"] = malformed.clone();
            }),
            Err(&[
                (
                    Slot::LocaleCanonicalizationData,
                    ORIGIN,
                    "unknown-safe-origin-kind",
                ),
                (
                    Slot::ImplementationCapability,
                    capability,
                    "malformed-capability",
                ),
            ]),
        ),
        case(
            "common-origin-before-value",
            with(&|fixture| {
                remove(fixture, Slot::ResourceLimitPolicyVerification, "candidate");
                fixture[Slot::ResourceLimitPolicyVerification.id()]["input"] =
                    kind(verification).input("strict-syntax", "malformed-input");
            }),
            Err(&[
                (
                    Slot::ResourceLimitPolicyVerification,
                    ORIGIN,
                    "missing-safe-origin-kind",
                ),
                (
                    Slot::ResourceLimitPolicyVerification,
                    verification,
                    "malformed-input",
                ),
            ]),
        ),
    ]
}

/// Remove `member` from the fixture's slot.
fn remove(fixture: &mut Value, slot: SafeOriginKind, member: &str) {
    fixture[slot.id()]
        .as_object_mut()
        .expect("a slot is an object")
        .remove(member);
}

/// A file entry's step cases (D15:3164, 3168): the admitted set with its
/// `relative-path` locator, and with a locator that is not admitted, which is
/// absence; and its failures under a refused capability, none when both
/// origins hold and only the origin's when one does not, and in slot rank.
fn file_steps(context: &Context, kinds: &BTreeMap<&str, Built>) -> Vec<Case> {
    use SafeOriginKind as Slot;
    let source = "file-source-identity-revision";
    let selector = "external-profile-selector-binding";
    let pair = &kinds[source].admitted["input"];
    let mut input = pair.clone();
    input["locator"] =
        json!({ "variant": "relative-path", "payload": "intlify.config.json", "nonSecret": true });
    let fixture = json!({
        "capability": { "json": context.capability },
        "entry": {
            Slot::ConfigurationFile.id(): { "candidate": Slot::ConfigurationFile.id(), "input": input },
            Slot::ProfileSelectorCli.id(): {
                "candidate": Slot::ProfileSelectorCli.id(),
                "input": kinds[selector].admitted["input"],
            },
        },
    });
    let with = |change: &dyn Fn(&mut Value)| {
        let mut changed = fixture.clone();
        change(&mut changed);
        changed
    };
    let admitted = |locator: Value| {
        json!({
            Slot::ConfigurationFile.id(): {
                "bytes": { "bytes": pair["bytes"]["bytes"] },
                "identity": pair["identity"],
                "nonSecret": pair["nonSecret"],
                "revision": pair["revision"],
                "locator": locator,
            },
            Slot::ProfileSelectorCli.id(): kinds[selector].normalized,
        })
    };
    let refused = kinds["implementation-capability"].input("closed-shape", "malformed-capability");
    let file = Slot::ConfigurationFile.id();
    let cli = Slot::ProfileSelectorCli.id();
    let case = |tail: &str, fixture: Value, expected: Result<Value, &[Failure]>| {
        step_case(
            tail,
            ENTRY_SET,
            &FILE_DECISIONS,
            fixture,
            expected,
            &[ORIGIN, source, selector],
        )
    };
    vec![
        case(
            "entry-file-admitted",
            fixture.clone(),
            Ok(admitted(json!({ "relative-path": "intlify.config.json" }))),
        ),
        case(
            "entry-file-locator-absence",
            with(&|fixture| {
                fixture["entry"][file]["input"]["locator"]["payload"] =
                    json!("../intlify.config.json");
            }),
            Ok(admitted(Value::Null)),
        ),
        case(
            "entry-file-refused-capability",
            with(&|fixture| fixture["capability"] = refused.clone()),
            Err(&[]),
        ),
        case(
            "entry-file-refused-capability-origin",
            with(&|fixture| {
                fixture["capability"] = refused.clone();
                fixture["entry"][cli]["candidate"] = json!("x");
            }),
            Err(&[(Slot::ProfileSelectorCli, ORIGIN, "unknown-safe-origin-kind")]),
        ),
        case(
            "entry-file-rank-order",
            with(&|fixture| {
                fixture["entry"][file]["input"]["identity"] = kinds[source]
                    .input("value-rules", "unstable-source-identity")["identity"]
                    .clone();
                fixture["entry"][cli]["input"] =
                    kinds[selector].input("unsafe-host-inspection", "unsafe-host-inspection");
            }),
            Err(&[
                (Slot::ConfigurationFile, source, "unstable-source-identity"),
                (Slot::ProfileSelectorCli, selector, "unsafe-host-inspection"),
            ]),
        ),
    ]
}

/// A programmatic entry's step cases (D15:3164, 3168): the admitted set with
/// its call-site sidecar and its `safe-label` locator, and without the
/// sidecar; and its failures after a refused Snapshot, which leaves the
/// identity and revision unevaluated, and in the boundary table's order.
fn programmatic_steps(context: &Context, kinds: &BTreeMap<&str, Built>) -> Vec<Case> {
    use SafeOriginKind as Slot;
    let source = "programmatic-entry-identity-revision";
    let snapshot = "programmatic-entry-snapshot";
    let call_site = "programmatic-call-site-evidence-input";
    let selector = "external-profile-selector-binding";
    let pair = &kinds[source].admitted["input"];
    let fixture = json!({
        "capability": { "json": context.capability },
        "entry": {
            Slot::ConfigurationProgrammatic.id(): {
                "candidate": Slot::ConfigurationProgrammatic.id(),
                "input": {
                    "snapshot": pair["snapshot"],
                    "identity": pair["identity"],
                    "nonSecret": pair["nonSecret"],
                    "revision": pair["revision"],
                    "locator": { "variant": "safe-label", "payload": "app", "nonSecret": true },
                    "callSite": kinds[call_site].admitted["input"],
                },
            },
            Slot::ProfileSelectorProgrammatic.id(): {
                "candidate": Slot::ProfileSelectorProgrammatic.id(),
                "input": kinds[selector].admitted["input"],
            },
        },
    });
    let with = |change: &dyn Fn(&mut Value)| {
        let mut changed = fixture.clone();
        change(&mut changed);
        changed
    };
    let admitted = |call_site: Value| {
        json!({
            Slot::ConfigurationProgrammatic.id(): {
                "snapshot": pair["snapshot"]["snapshot"],
                "digest": pair["snapshot"]["digest"],
                "identity": pair["identity"],
                "nonSecret": pair["nonSecret"],
                "revision": pair["revision"],
                "locator": { "safe-label": "app" },
                "callSite": call_site,
            },
            Slot::ProfileSelectorProgrammatic.id(): kinds[selector].normalized,
        })
    };
    let entry = Slot::ConfigurationProgrammatic.id();
    let unstable =
        kinds[source].input("value-rules", "unstable-entry-identity")["identity"].clone();
    let case = |tail: &str, fixture: Value, expected: Result<Value, &[Failure]>| {
        step_case(
            tail,
            ENTRY_SET,
            &PROGRAMMATIC_DECISIONS,
            fixture,
            expected,
            &[ORIGIN, source, snapshot, call_site, selector],
        )
    };
    vec![
        case(
            "entry-programmatic-admitted",
            fixture.clone(),
            Ok(admitted(kinds[call_site].normalized.clone())),
        ),
        case(
            "entry-programmatic-no-call-site",
            with(&|fixture| {
                fixture["entry"][entry]["input"]
                    .as_object_mut()
                    .expect("the input is an object")
                    .remove("callSite");
            }),
            Ok(admitted(Value::Null)),
        ),
        case(
            "entry-programmatic-refused-snapshot",
            with(&|fixture| {
                fixture["entry"][entry]["input"]["snapshot"] =
                    kinds[snapshot].input("closed-shape", "malformed-snapshot");
                fixture["entry"][entry]["input"]["identity"] = unstable.clone();
            }),
            Err(&[(
                Slot::ConfigurationProgrammatic,
                snapshot,
                "malformed-snapshot",
            )]),
        ),
        case(
            "entry-programmatic-value-order",
            with(&|fixture| {
                fixture["entry"][entry]["input"]["identity"] = unstable.clone();
                fixture["entry"][entry]["input"]["callSite"] =
                    kinds[call_site].input("closed-shape", "invalid-utf8-source");
                fixture["entry"][Slot::ProfileSelectorProgrammatic.id()]["input"] =
                    kinds[selector].input("unsafe-host-inspection", "unsafe-host-inspection");
            }),
            Err(&[
                (
                    Slot::ConfigurationProgrammatic,
                    source,
                    "unstable-entry-identity",
                ),
                (
                    Slot::ConfigurationProgrammatic,
                    call_site,
                    "invalid-utf8-source",
                ),
                (
                    Slot::ProfileSelectorProgrammatic,
                    selector,
                    "unsafe-host-inspection",
                ),
            ]),
        ),
    ]
}

/// The two admission steps' cases.
fn steps(context: &Context, kinds: &BTreeMap<&str, Built>) -> Vec<Case> {
    let mut cases = common_steps(kinds);
    cases.extend(file_steps(context, kinds));
    cases.extend(programmatic_steps(context, kinds));
    cases
}

/// The suite's input admission cases of the boundary kinds, in ascending
/// unsigned byte order of their IDs: each manifest, fixture, and raw input
/// kept in a file, and the index entry of each manifest.
///
/// # Errors
///
/// Returns the serialization failure of a value, which their values rule
/// out.
pub(crate) fn cases() -> Result<Cases, serde_json::Error> {
    let identity = construction_cases::identity()?;
    let (all, mut files) = all()?;
    let mut listing = Vec::new();
    let mut kept = BTreeMap::new();
    for case in all {
        let fixture = written(FIXTURES, &case.id, &case.fixture, &mut files, &mut kept);
        let fixture = generate::write(&fixture)?;
        let fixture_path = format!("{FIXTURES}/{}.json", case.id);
        let manifest = generate::write(&json!({
            "suiteRevision": REVISION,
            "manifestKind": ManifestKind::ResolverInputAdmission.name(),
            "id": case.id,
            "resolverConstruction": {
                "caseId": CONSTRUCTED,
                "identityReference": identity,
            },
            "boundaryKind": case.kind,
            "inputFixture": {
                "path": fixture_path,
                "digest": file_digest(&fixture).as_str(),
            },
            "expected": case.expected,
            "verificationOwner": "intlify_config",
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
    listing.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    Ok(Cases { files, listing })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use super::*;
    use crate::conformance::admission;
    use crate::conformance::construction::{self, Constructed};
    use crate::conformance::fixture::Bodies;
    use crate::construction::flow::testing::committed;

    /// The committed file at `path` below the suite root.
    fn committed_file(path: &str) -> Option<Vec<u8>> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("conformance");
        std::fs::read(root.join(path)).ok()
    }

    fn constructed() -> Constructed {
        construction::constructed(&committed()).unwrap()
    }

    /// The kinds whose admitted value states its members in byte order.
    const SORTED: [&str; 5] = [
        "profile-resolution-specification-input",
        "intent-surface-class-vocabulary-artifact",
        "locale-canonicalization-data-artifact",
        "resource-limit-policy-verification-input",
        "programmatic-call-site-evidence-input",
    ];

    fn context() -> Context {
        Context {
            capability: capability(),
            identity: construction_cases::identity().unwrap(),
        }
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
        assert_eq!(cases.listing.len(), 105 + 13);
        assert!(cases
            .listing
            .windows(2)
            .all(|pair| pair[0]["id"].as_str() < pair[1]["id"].as_str()));
    }

    /// Each kind of the boundary table, in the order of the precedence, has
    /// its admitted case, which traces each of its pairs, and one case for
    /// each row, which traces its row's pair; every case traces the shared
    /// decisions and its kind's.
    #[test]
    fn each_kind_has_its_admitted_case_and_one_for_each_row() {
        assert_eq!(
            KINDS.map(|kind| kind.name),
            BOUNDARY_PRECEDENCE.map(|(name, _)| name)
        );
        let context = context();
        for kind in &KINDS {
            let built = (kind.cases)(&context);
            assert_eq!(built.refused.len(), rows(kind.name).len(), "{}", kind.name);
        }
        let (cases, _) = all().unwrap();
        let cases = cases
            .iter()
            .filter(|case| KINDS.iter().any(|kind| kind.name == case.kind))
            .collect::<Vec<_>>();
        assert_eq!(cases.len(), 13 + 92);
        let ids = cases
            .iter()
            .map(|case| case.id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), cases.len());
        assert!(cases.windows(2).all(|pair| pair[0].id < pair[1].id));
        for case in cases {
            let kind = KINDS.iter().find(|kind| kind.name == case.kind).unwrap();
            assert_eq!(case.decisions[..4], SHARED_DECISIONS, "{}", case.id);
            assert_eq!(case.decisions[4..], *kind.decisions, "{}", case.id);
            if case.expected["admitted"] == true {
                assert_eq!(case.id, format!("015-admission-{}-admitted", kind.short));
                let reasons = rows(kind.name)
                    .iter()
                    .map(|(_, reason)| *reason)
                    .collect::<BTreeSet<_>>();
                assert_eq!(case.rules.len(), reasons.len(), "{}", case.id);
                // A decoded value states its members in byte order
                // (D15:3258).
                if SORTED.contains(&case.kind) {
                    let normalized = &case.expected["normalizedInput"];
                    assert_eq!(
                        serde_json::to_string(normalized).unwrap(),
                        serde_json::to_string(&sorted(normalized)).unwrap(),
                        "{}",
                        case.id
                    );
                }
            } else {
                let reason = case.expected["failureReason"].as_str().unwrap();
                assert_eq!(case.rules, [rule(kind.name, reason)], "{}", case.id);
            }
        }
    }

    /// The two admission steps' 13 cases follow the kinds' (D15:3401): the
    /// common step, a file entry, and a programmatic entry each have an
    /// admitted case and a refused one. Each traces the shared decisions and
    /// its step's; a refused one traces the pair of each of its failures,
    /// which run in slot rank with an origin's first, and an admitted one
    /// every pair of its step's boundaries, those of its refused cases among
    /// them.
    #[test]
    fn each_step_has_its_admitted_and_refused_cases() {
        let (cases, _) = all().unwrap();
        let steps = &cases[13 + 92..];
        assert_eq!(steps.len(), 13);
        assert!(steps.windows(2).all(|pair| pair[0].id < pair[1].id));
        let rank = |slot: &Value| {
            SafeOriginKind::ALL
                .into_iter()
                .find(|kind| *slot == kind.id())
                .unwrap()
                .rank()
        };
        let mut seen = 0;
        for (prefix, kind, decisions) in [
            ("015-admission-common-", COMMON_SET, &COMMON_DECISIONS),
            ("015-admission-entry-file-", ENTRY_SET, &FILE_DECISIONS),
            (
                "015-admission-entry-programmatic-",
                ENTRY_SET,
                &PROGRAMMATIC_DECISIONS,
            ),
        ] {
            let own = steps
                .iter()
                .filter(|case| case.id.starts_with(prefix))
                .collect::<Vec<_>>();
            seen += own.len();
            let admitted = own
                .iter()
                .filter(|case| case.expected["admitted"] == true)
                .collect::<Vec<_>>();
            assert!(!admitted.is_empty(), "{prefix}");
            assert!(admitted.len() < own.len(), "{prefix}");
            for case in &own {
                assert_eq!(case.kind, kind, "{}", case.id);
                assert_eq!(case.decisions[..4], SHARED_DECISIONS, "{}", case.id);
                assert_eq!(case.decisions[4..], *decisions, "{}", case.id);
                if case.expected["admitted"] == true {
                    assert_eq!(case.rules, admitted[0].rules, "{}", case.id);
                    continue;
                }
                let failures = case.expected["failures"].as_array().unwrap();
                let mut pairs = Vec::new();
                for failure in failures {
                    let pair = rule(
                        failure["boundaryKind"].as_str().unwrap(),
                        failure["failureReason"].as_str().unwrap(),
                    );
                    if !pairs.contains(&pair) {
                        pairs.push(pair);
                    }
                }
                assert_eq!(case.rules, pairs, "{}", case.id);
                assert!(pairs.iter().all(|pair| admitted[0].rules.contains(pair)));
                let order = failures
                    .iter()
                    .map(|failure| (rank(&failure["slot"]), failure["boundaryKind"] != ORIGIN))
                    .collect::<Vec<_>>();
                assert!(
                    order.windows(2).all(|pair| pair[0] <= pair[1]),
                    "{}",
                    case.id
                );
            }
        }
        assert_eq!(seen, steps.len());
    }

    /// A step case that shows a boundary left unevaluated holds an input
    /// that boundary refuses (D15:3164): the common step's refused capability
    /// beside a vocabulary of a malformed envelope, and a programmatic entry's
    /// refused Snapshot beside an unstable identity, which the admitted
    /// case's canonical Snapshot shows.
    #[test]
    fn a_step_case_holds_what_its_unevaluated_boundary_refuses() {
        let constructed = constructed();
        let (cases, files) = all().unwrap();
        let bodies = files
            .iter()
            .map(|file| (file.path.clone(), file.bytes.clone()))
            .collect::<Bodies>();
        let fixture = |id: &str| {
            cases
                .iter()
                .find(|case| case.id == id)
                .unwrap()
                .fixture
                .clone()
        };
        let reason = |kind: &str, input: &Value| {
            let fixture = json!({ "capability": { "json": capability() }, "input": input });
            admission::outcome(kind, &fixture, &bodies, &constructed)["failureReason"].clone()
        };
        let common = fixture("015-admission-common-refused-capability");
        assert_eq!(
            reason(
                "intent-surface-class-vocabulary-artifact",
                &common[SafeOriginKind::IntentSurfaceClassVocabulary.id()]["input"]
            ),
            "malformed-envelope"
        );
        let programmatic = SafeOriginKind::ConfigurationProgrammatic.id();
        let refused = fixture("015-admission-entry-programmatic-refused-snapshot");
        let mut pair = fixture("015-admission-entry-programmatic-admitted")["entry"][programmatic]
            ["input"]
            .clone();
        pair["identity"] = refused["entry"][programmatic]["input"]["identity"].clone();
        let pair = json!({
            "identity": pair["identity"],
            "nonSecret": pair["nonSecret"],
            "revision": pair["revision"],
            "snapshot": pair["snapshot"],
        });
        assert_eq!(
            reason("programmatic-entry-identity-revision", &pair),
            "unstable-entry-identity"
        );
    }

    /// Each case's fixture, run through the harness as the runner reads it,
    /// does what the case expects, the admitted case under its prepared
    /// parts too.
    #[test]
    fn each_case_expects_what_its_boundary_does() {
        let constructed = constructed();
        let (cases, files) = all().unwrap();
        let bodies = files
            .iter()
            .map(|file| (file.path.clone(), file.bytes.clone()))
            .collect::<Bodies>();
        for case in &cases {
            assert!(
                admission::prepared(case.kind, &case.fixture, &constructed),
                "{}",
                case.id
            );
            assert_eq!(
                admission::outcome(case.kind, &case.fixture, &bodies, &constructed),
                case.expected,
                "{}",
                case.id
            );
        }
    }

    /// The file pair's and the entry pair's admitted cases hold different
    /// marks, so writing an admitted pair's mark from anything but its
    /// fixture fails one of them.
    #[test]
    fn the_two_pairs_hold_both_marks() {
        let (cases, _) = all().unwrap();
        let mark = |kind: &str| {
            let case = cases
                .iter()
                .find(|case| case.kind == kind && case.expected["admitted"] == true)
                .unwrap();
            case.fixture["input"]["nonSecret"].clone()
        };
        assert_ne!(
            mark("file-source-identity-revision"),
            mark("programmatic-entry-identity-revision")
        );
    }

    /// How many bounds `to` lowers from `from`, two capability values of one
    /// shape: each bound whose decimal text is smaller, every other value
    /// unchanged.
    fn lowered_bounds(from: &Value, to: &Value) -> usize {
        match (from, to) {
            (Value::Object(left), Value::Object(right)) => {
                assert!(left.keys().eq(right.keys()));
                left.iter()
                    .map(|(name, value)| lowered_bounds(value, &right[name]))
                    .sum()
            }
            (Value::String(left), Value::String(right)) if left != right => {
                assert!(right.parse::<u64>().unwrap() < left.parse::<u64>().unwrap());
                1
            }
            (left, right) => {
                assert_eq!(left, right);
                0
            }
        }
    }

    /// A refused case's fixture is its kind's admitted fixture with its input
    /// changed or one capability bound lowered, never both, so every check
    /// before its row holds; and no two fixtures of a kind are alike, so two
    /// rows that share a reason have inputs of their own.
    #[test]
    fn each_row_changes_the_input_or_lowers_one_bound() {
        let context = context();
        let rest = |fixture: &Value| {
            let mut rest = fixture.clone();
            rest.as_object_mut().unwrap().remove("capability");
            rest
        };
        for kind in &KINDS {
            let built = (kind.cases)(&context);
            for (row, fixture) in rows(kind.name).iter().zip(&built.refused) {
                let lowered = lowered_bounds(&built.admitted["capability"], &fixture["capability"]);
                let changed = rest(fixture) != rest(&built.admitted);
                assert!(lowered <= 1, "{} {row:?}", kind.name);
                assert_ne!(changed, lowered == 1, "{} {row:?}", kind.name);
            }
            let fixtures = std::iter::once(&built.admitted)
                .chain(&built.refused)
                .map(|fixture| serde_json::to_string(fixture).unwrap())
                .collect::<BTreeSet<_>>();
            assert_eq!(fixtures.len(), built.refused.len() + 1, "{}", kind.name);
        }
    }

    #[test]
    fn a_case_id_names_its_check_and_its_reason_when_the_check_repeats() {
        let kind = |name: &str| KINDS.iter().find(|kind| kind.name == name).unwrap();
        assert_eq!(
            case_id(
                kind("profile-resolution-specification-input"),
                ("strict-syntax", "malformed-input")
            ),
            "015-admission-specification-strict-syntax"
        );
        assert_eq!(
            case_id(
                kind("resource-limit-policy-verification-input"),
                ("reserved-members", "ambient-authority")
            ),
            "015-admission-verification-reserved-members-ambient-authority"
        );
    }

    /// The capability value is one the constructed Capability Specification
    /// admits, with each bound these boundaries read at [`ROOMY`].
    #[test]
    fn the_capability_value_is_admitted_and_roomy() {
        let value = capability();
        assert!(
            crate::boundary::capability::admit(Some(&value), &constructed().capability).is_ok()
        );
        for namespace in [
            "inputBoundary",
            "snapshot",
            "evidenceMetadata",
            "localeData",
        ] {
            for bound in value["bootstrapOnly"][namespace]
                .as_object()
                .unwrap()
                .values()
            {
                assert_eq!(bound, ROOMY, "{namespace}");
            }
        }
        assert_eq!(
            value["policyComparable"]["configurationInput"]["maxProfileIdBytes"],
            ROOMY
        );
        assert_eq!(
            lowered(&value, "/bootstrapOnly/snapshot/maxNodes", 4)["bootstrapOnly"]["snapshot"]
                ["maxNodes"],
            "4"
        );
    }
}
