// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Running a Resolver Input Admission Case: what one boundary does with the
//! input its fixture materializes, in the form of the case's expected outcome
//! (D15:3109-3170, 3239-3267).
//!
//! The harness is the case's binding. It reads each part of the fixture as the
//! boundary reads it: a raw input as bytes or as a decoded value, a report in
//! its closed form, a candidate or a reference as text or as no text. A
//! Snapshot node outside the closed model, of an unknown variant, with a
//! member the model does not have, or with a number or a witness that is no
//! decimal, is submitted as a marker of no registered reason, which the
//! boundary refuses at its closed model (D15:3250); a locator candidate of an
//! unknown variant is no candidate, since a locator that is not admitted is
//! absence (D15:750). A boundary that reads capability bounds takes them from
//! the case's capability value, and the programmatic pair takes the case's
//! Snapshot: the runner admits both before any case runs, and a case whose
//! capability or Snapshot is refused does not run (D15:3162).
//!
//! An admitted input is written in its normalized form (D15:3256-3267), every
//! object's members in ascending unsigned byte order of their names.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::{json, Map, Value};

use crate::artifacts::collection::{read_collection, CollectionReport};
use crate::artifacts::vocabulary;
use crate::boundary::call_site::{
    self, AdmittedCallSite, CallSiteReport, SpanCandidate, StructuralEdge, SubmittedCallSite,
    SubmittedEdge, SubmittedMapping,
};
use crate::boundary::capability::{self, ImplementationCapability};
use crate::boundary::common::{collection_limits, vocabulary_limits};
use crate::boundary::construction_identity;
use crate::boundary::locator::{LocatorCandidate, LocatorVariant};
use crate::boundary::origin;
use crate::boundary::selector::{self, SelectorReport};
use crate::boundary::snapshot::{
    self, AdmittedSnapshot, SnapshotReport, SubmittedMarker, SubmittedNode,
};
use crate::boundary::source;
use crate::boundary::specification;
use crate::boundary::step::Refusal;
use crate::boundary::text::TextInput;
use crate::boundary::verification::{self, VerificationAssertion};
use crate::locale::icu4x::Icu4xData;
use crate::locale::VersionedIdentity;
use crate::resolver::origin::SafeOriginKind;
use crate::resolver::specification::SpecificationInput;
use crate::snapshot::{Marker, Node, BOOTSTRAP_LIMIT_EXCEEDED};
use crate::structural::selection::{InvalidSelectorType, SelectorInput, SelectorShape};
use intlify_shared_json::token::Token;

use super::construction::Constructed;
use super::fixture::{bytes, Bodies};

/// The selector's type tags, in 015's order (Decision 015-167).
const TAGS: [(InvalidSelectorType, &str); 5] = [
    (InvalidSelectorType::Null, "null"),
    (InvalidSelectorType::Boolean, "boolean"),
    (InvalidSelectorType::Number, "number"),
    (InvalidSelectorType::Array, "array"),
    (InvalidSelectorType::Object, "object"),
];

/// Whether the harness admits the parts of a case's fixture it admits before
/// the boundary: the capability value of a boundary that reads bounds, and
/// under it the programmatic pair's Snapshot. A refused part invalidates the
/// case rather than becoming its expected reason (D15:3162, 3252).
pub(crate) fn prepared(kind: &str, fixture: &Value, constructed: &Constructed) -> bool {
    if fixture.get("capability").is_none() {
        return true;
    }
    admitted_capability(fixture, constructed).is_some_and(|capability| {
        kind != "programmatic-entry-identity-revision"
            || entry_snapshot(&fixture["input"], &capability).is_some()
    })
}

/// What the boundary of `kind` does with `fixture`'s input, in the form of an
/// admission case's expected outcome. The runner prepared the case.
pub(crate) fn outcome(
    kind: &str,
    fixture: &Value,
    bodies: &Bodies,
    constructed: &Constructed,
) -> Value {
    let input = fixture.get("input");
    let capability =
        || admitted_capability(fixture, constructed).expect("the runner prepared the case");
    let raw = || input.map(|raw| bytes(raw, bodies));
    let result = match kind {
        "profile-resolution-specification-input" => {
            specification::admit(raw().as_deref(), &capability())
                .map(|admitted| json!({ "json": specification_view(&admitted) }))
                .map_err(Refusal::Specification)
        }
        "intent-surface-class-vocabulary-artifact" => {
            let limits = vocabulary_limits(&capability().bootstrap_only().input_boundary);
            vocabulary::read(raw().as_deref(), &limits)
                .map(|admitted| json!({ "json": value_view(admitted.artifact()) }))
                .map_err(Refusal::Vocabulary)
        }
        "locale-canonicalization-data-artifact" => data(input, bodies, &capability()),
        "submitted-profile-resolution-artifact-collection" => {
            collection(input, bodies, &capability())
        }
        "resource-limit-policy-verification-input" => {
            let assertion = &fixture["assertion"];
            let token = |name: &str| {
                Token::new(assertion[name].as_str().unwrap_or_default())
                    .expect("the Case Schema makes it a token")
            };
            let assertion = VerificationAssertion {
                identity: token("identity"),
                revision: token("revision"),
                semantic_digest: serde_json::from_value(assertion["semanticDigest"].clone())
                    .expect("the Case Schema makes it a digest"),
            };
            verification::admit(raw().as_deref(), &capability(), &assertion)
                .map(|admitted| json!({ "json": sorted(&admitted.normalized()) }))
                .map_err(Refusal::Verification)
        }
        "safe-origin-kind" => origin(fixture),
        "external-profile-selector-binding" => {
            selector::admit(selector_report(&fixture["input"]), &capability())
                .map(|admitted| json!({ "selector": selector_view(&admitted) }))
                .map_err(Refusal::Selector)
        }
        "file-source-identity-revision" => {
            let pair = &fixture["input"];
            let content = bytes(&pair["bytes"], bodies);
            source::admit(
                text(pair, "identity"),
                text(pair, "revision"),
                &content,
                &capability(),
            )
            .map(|admitted| pair_view(pair, admitted.identity().as_str(), admitted.revision()))
            .map_err(Refusal::Source)
        }
        "programmatic-entry-identity-revision" => {
            let pair = &fixture["input"];
            let capability = capability();
            let snapshot = entry_snapshot(pair, &capability).expect("the runner prepared the case");
            source::admit_entry(
                text(pair, "identity"),
                text(pair, "revision"),
                &snapshot,
                &capability,
            )
            .map(|admitted| pair_view(pair, admitted.identity().as_str(), admitted.revision()))
            .map_err(Refusal::EntryIdentity)
        }
        "programmatic-entry-snapshot" => {
            let report = input.map(snapshot_report);
            snapshot::admit(report.as_ref(), &capability())
                .map(|admitted| snapshot_view(&admitted))
                .map_err(Refusal::Snapshot)
        }
        "programmatic-call-site-evidence-input" => {
            call_site(&fixture["input"], bodies, &capability())
        }
        "implementation-capability" => {
            capability::admit(input.map(|raw| &raw["json"]), &constructed.capability)
                .map(|admitted| json!({ "json": admitted.normalized() }))
                .map_err(Refusal::Capability)
        }
        "resolver-construction-identity-reference" => {
            let reference = input.map(|raw| text_input(&raw["json"]));
            return match construction_identity::admit(reference, &constructed.identity) {
                Ok(admitted) => {
                    admitted_outcome(&json!({ "resolverConstructionIdentity": admitted.text() }))
                }
                Err(refusal) => refused_outcome(refusal.reason()),
            };
        }
        other => unreachable!("the Case Schema closes the boundary kinds, not {other}"),
    };
    match result {
        Ok(normalized) => admitted_outcome(&normalized),
        Err(refusal) => refused_outcome(refusal.reason()),
    }
}

fn admitted_outcome(normalized: &Value) -> Value {
    json!({ "admitted": true, "normalizedInput": normalized, "resolverInvoked": false })
}

fn refused_outcome(reason: &str) -> Value {
    json!({
        "admitted": false,
        "normalizedInput": null,
        "failureReason": reason,
        "resolverInvoked": false,
    })
}

/// The case's capability value, admitted against the constructed Capability
/// Specification.
fn admitted_capability(
    fixture: &Value,
    constructed: &Constructed,
) -> Option<ImplementationCapability> {
    capability::admit(
        Some(&fixture["capability"]["json"]),
        &constructed.capability,
    )
    .ok()
}

/// The programmatic pair's canonical Snapshot, admitted under the capability.
fn entry_snapshot(pair: &Value, capability: &ImplementationCapability) -> Option<AdmittedSnapshot> {
    snapshot::admit(Some(&snapshot_report(&pair["snapshot"])), capability).ok()
}

/// A member that is text, `None` when absent.
fn text<'a>(object: &'a Value, name: &str) -> Option<&'a str> {
    object
        .get(name)
        .map(|value| value.as_str().expect("the Case Schema makes it a string"))
}

/// A value read where a boundary expects text.
fn text_input(value: &Value) -> TextInput<'_> {
    value.as_str().map_or(TextInput::NotText, TextInput::Text)
}

/// `value` with every object's members in ascending unsigned byte order of
/// their names.
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

/// A serializable admitted value, sorted.
fn value_view(value: &impl serde::Serialize) -> Value {
    sorted(&serde_json::to_value(value).expect("an admitted value serializes"))
}

/// The admitted specification input: each present slot's assertion
/// (D15:3258).
fn specification_view(input: &SpecificationInput<String>) -> Value {
    let versioned = |identity: &VersionedIdentity<String>| json!({ "identity": identity.identity, "revision": identity.revision });
    let mut slots = Map::new();
    let mut slot = |name: &str, value: Option<Value>| {
        if let Some(value) = value {
            slots.insert(name.to_owned(), value);
        }
    };
    slot(
        "project-profile",
        input.project_profile.as_ref().map(versioned),
    );
    slot(
        "locale-canonicalization",
        input.locale_canonicalization.as_ref().map(versioned),
    );
    slot(
        "locale-negotiation",
        input.locale_negotiation.as_ref().map(versioned),
    );
    slot(
        "message-locale-fallback",
        input.message_locale_fallback.as_ref().map(versioned),
    );
    slot("coverage", input.coverage.as_ref().map(versioned));
    slot(
        "intent-surface-class-vocabulary",
        input
            .intent_surface_class_vocabulary
            .as_ref()
            .map(|assertion| {
                json!({
                    "identity": assertion.identity,
                    "revision": assertion.revision,
                    "semanticDigest": assertion.semantic_digest,
                })
            }),
    );
    slot(
        "delivery-graph",
        input.delivery_graph.as_ref().map(versioned),
    );
    slot(
        "delivery-placement",
        input.delivery_placement.as_ref().map(versioned),
    );
    sorted(&Value::Object(slots))
}

/// The locale canonicalization data boundary: the manifest's raw input and
/// the payloads the fixture supplies by address. The admitted value is the
/// manifest and the supplied payloads at the addresses it names, each kept as
/// its reference (D15:3258, 3267).
fn data(
    input: Option<&Value>,
    bodies: &Bodies,
    capability: &ImplementationCapability,
) -> Result<Value, Refusal> {
    use intlify_locale_icu4x::Role;
    let bounds = &capability.bootstrap_only().input_boundary;
    let manifest = input.map(|data| bytes(&data["manifest"], bodies));
    let references = input
        .and_then(|data| data["payloads"].as_object())
        .cloned()
        .unwrap_or_default();
    let payloads = references
        .iter()
        .map(|(address, raw)| (address.clone(), Arc::<[u8]>::from(bytes(raw, bodies))))
        .collect::<BTreeMap<_, _>>();
    let data = Icu4xData::read(
        manifest.as_deref(),
        &payloads,
        bounds.max_locale_data_manifest_bytes,
        bounds.max_depth,
    )
    .map_err(Refusal::Data)?;
    let mut supplied = Map::new();
    for role in [Role::Conformance, Role::Markers] {
        let address = data.manifest().address(role).as_str();
        if data.supplies(role) {
            supplied.insert(address.to_owned(), references[address].clone());
        }
    }
    Ok(json!({
        "json": sorted(&json!({
            "manifest": serde_json::to_value(data.manifest().manifest()).expect("a manifest serializes"),
            "payloads": Value::Object(supplied),
        }))
    }))
}

/// The collection boundary: the formal container's members, a malformed
/// container, or an unsafe host inspection.
fn collection(
    input: Option<&Value>,
    bodies: &Bodies,
    capability: &ImplementationCapability,
) -> Result<Value, Refusal> {
    let limits = collection_limits(&capability.bootstrap_only().input_boundary);
    let members = input
        .and_then(|report| report["members"].as_array())
        .map(|members| {
            members
                .iter()
                .map(|raw| bytes(raw, bodies))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let borrowed = members.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let report = input.map(|report| {
        if report.get("members").is_some() {
            CollectionReport::Formal(&borrowed)
        } else if report.get("container").is_some() {
            CollectionReport::MalformedContainer
        } else {
            CollectionReport::Unsafe
        }
    });
    let admitted = read_collection(report, &limits).map_err(Refusal::Collection)?;
    let members = admitted
        .members()
        .iter()
        .map(|member| json!({ "json": member.value() }))
        .collect::<Vec<_>>();
    Ok(json!({ "members": members }))
}

/// The origin boundary: the slot's Safe Origin Kind and the candidate beside
/// it, which may be no text.
fn origin(fixture: &Value) -> Result<Value, Refusal> {
    let slot = fixture["slot"]
        .as_str()
        .expect("the Case Schema makes the slot an ID");
    let slot = SafeOriginKind::ALL
        .into_iter()
        .find(|kind| kind.id() == slot)
        .expect("the Case Schema closes the slots");
    let candidate = fixture.get("candidate").map(text_input);
    origin::admit(slot, candidate)
        .map(|admitted| json!({ "safeOriginKind": admitted.id() }))
        .map_err(Refusal::Origin)
}

/// The selector's closed report (D15:3252).
fn selector_report(report: &Value) -> SelectorReport<'_> {
    match report["report"].as_str() {
        Some("absent") => SelectorReport::Absent,
        Some("string") => SelectorReport::String(
            report["value"]
                .as_str()
                .expect("the Case Schema makes it text"),
        ),
        Some("over-limit-string") => SelectorReport::OverLimitString,
        Some("invalid-type") => {
            let tag = report["type"].as_str();
            let kind = TAGS
                .into_iter()
                .find(|(_, name)| Some(*name) == tag)
                .expect("the Case Schema closes the tags")
                .0;
            SelectorReport::InvalidType(kind)
        }
        Some("unrepresentable") => SelectorReport::Unrepresentable,
        _ => SelectorReport::Unsafe,
    }
}

/// The admitted selector in its report's form.
fn selector_view(selector: &SelectorInput) -> Value {
    match (selector.shape(), selector.text()) {
        (SelectorShape::Absent, _) => json!({ "report": "absent" }),
        (SelectorShape::String, Some(value)) => json!({ "report": "string", "value": value }),
        (SelectorShape::String, None) => json!({ "report": "over-limit-string" }),
        (SelectorShape::Invalid(kind), _) => {
            let tag = TAGS
                .into_iter()
                .find(|(tagged, _)| *tagged == kind)
                .map(|(_, name)| name);
            json!({ "report": "invalid-type", "type": tag })
        }
    }
}

/// An admitted identity and revision pair with the fixture's mark (D15:3262).
fn pair_view(pair: &Value, identity: &str, revision: &str) -> Value {
    json!({ "identity": identity, "nonSecret": pair["nonSecret"], "revision": revision })
}

/// The Snapshot report a fixture's input holds: the formal Snapshot, or an
/// unsafe host inspection.
fn snapshot_report(input: &Value) -> SnapshotReport {
    match (input.get("snapshot"), input["digest"].as_str()) {
        (Some(root), Some(digest)) => SnapshotReport::Formal {
            root: submitted(root),
            digest: digest.to_owned(),
        },
        _ => SnapshotReport::Unsafe,
    }
}

/// A node outside the closed model: a marker of no registered reason, which
/// the boundary refuses at its closed model.
fn outside() -> SubmittedNode {
    SubmittedNode::Marker(SubmittedMarker {
        reason: String::new(),
        bound: None,
        witness: None,
    })
}

/// The submitted node a fixture's node is (D15:3250).
fn submitted(node: &Value) -> SubmittedNode {
    let Some(members) = node.as_object().filter(|members| members.len() == 1) else {
        return outside();
    };
    let (variant, value) = members.iter().next().expect("one member");
    match (variant.as_str(), value) {
        ("null", Value::Null) => SubmittedNode::Null,
        ("boolean", Value::Bool(value)) => SubmittedNode::Boolean(*value),
        ("number", Value::String(text)) if decimal(text) => text
            .parse()
            .map_or_else(|_| outside(), SubmittedNode::Number),
        ("string", Value::String(text)) => SubmittedNode::String(text.as_bytes().to_vec()),
        ("stringUtf16", _) => utf16(node).map_or_else(outside, SubmittedNode::String),
        ("array", Value::Array(elements)) => {
            SubmittedNode::Array(elements.iter().map(submitted).collect())
        }
        ("object", Value::Array(pairs)) => pairs
            .iter()
            .map(|pair| match pair.as_array().map(Vec::as_slice) {
                Some([Value::String(key), node]) => {
                    Some((key.as_bytes().to_vec(), submitted(node)))
                }
                Some([key, node]) => utf16(key).map(|key| (key, submitted(node))),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .map_or_else(outside, SubmittedNode::Object),
        ("marker", Value::Object(marker)) => marker_node(marker),
        _ => outside(),
    }
}

/// A marker as submitted: its reason, and a terminal marker's bound path and
/// decimal witness. A member the model does not have, or a witness that is no
/// decimal of a 64-bit count, is outside the model.
fn marker_node(marker: &Map<String, Value>) -> SubmittedNode {
    let known = marker
        .keys()
        .all(|name| matches!(name.as_str(), "reason" | "boundPath" | "witness"));
    let reason = marker.get("reason").and_then(Value::as_str);
    let bound = marker.get("boundPath").map(Value::as_str);
    let witness = marker.get("witness").map(|witness| {
        witness
            .as_str()
            .filter(|text| count(text))
            .and_then(|text| text.parse::<u64>().ok())
    });
    match (known, reason, bound, witness) {
        (true, Some(reason), None | Some(Some(_)), None | Some(Some(_))) => {
            SubmittedNode::Marker(SubmittedMarker {
                reason: reason.to_owned(),
                bound: bound.flatten().map(str::to_owned),
                witness: witness.flatten(),
            })
        }
        _ => outside(),
    }
}

/// Whether `text` is a decimal number: an optional minus, an integer without
/// a leading zero, an optional fraction, and an optional exponent (D15:3250).
fn decimal(text: &str) -> bool {
    let unsigned = text.strip_prefix('-').unwrap_or(text);
    let (mantissa, exponent) = unsigned
        .split_once(['e', 'E'])
        .map_or((unsigned, None), |(mantissa, exponent)| {
            (mantissa, Some(exponent))
        });
    let (integer, fraction) = mantissa
        .split_once('.')
        .map_or((mantissa, None), |(integer, fraction)| {
            (integer, Some(fraction))
        });
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    let exponent = exponent.map(|part| part.strip_prefix(['+', '-']).unwrap_or(part));
    digits(integer)
        && (integer == "0" || !integer.starts_with('0'))
        && fraction.is_none_or(digits)
        && exponent.is_none_or(digits)
}

/// Whether `text` is the decimal text of a count: digits without a leading
/// zero.
fn count(text: &str) -> bool {
    !text.is_empty()
        && text.bytes().all(|byte| byte.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'))
}

/// The string a `{ "stringUtf16": [<code unit>, ...] }` value holds, `None`
/// for any other value.
fn utf16(value: &Value) -> Option<Vec<u8>> {
    let members = value.as_object().filter(|members| members.len() == 1)?;
    code_units(members.get("stringUtf16")?.as_array()?)
}

/// The generalized UTF-8 of UTF-16 code units: a surrogate pair is its scalar
/// value, and a lone surrogate is encoded on its own, as a string that is not
/// a sequence of Unicode scalar values is submitted.
fn code_units(units: &[Value]) -> Option<Vec<u8>> {
    let units = units
        .iter()
        .map(|unit| unit.as_u64().and_then(|unit| u16::try_from(unit).ok()))
        .collect::<Option<Vec<_>>>()?;
    let mut bytes = Vec::new();
    for decoded in char::decode_utf16(units) {
        match decoded {
            Ok(character) => {
                let mut buffer = [0; 4];
                bytes.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
            }
            Err(error) => {
                let unit = u32::from(error.unpaired_surrogate());
                bytes.extend_from_slice(&[
                    0xE0 | u8::try_from(unit >> 12).expect("a surrogate's high bits fit"),
                    0x80 | u8::try_from((unit >> 6) & 0x3F).expect("six bits fit"),
                    0x80 | u8::try_from(unit & 0x3F).expect("six bits fit"),
                ]);
            }
        }
    }
    Some(bytes)
}

/// The admitted Snapshot: its canonical tree and its digest (D15:3250, 3263).
fn snapshot_view(snapshot: &AdmittedSnapshot) -> Value {
    json!({ "snapshot": node_view(snapshot.root()), "digest": snapshot.digest() })
}

/// A canonical node in the Snapshot form: keys in ascending byte order without
/// repetition, which the tree keeps, and each number as the shortest decimal
/// text that reads back as it.
fn node_view(node: &Node) -> Value {
    match node {
        Node::Null => json!({ "null": null }),
        Node::Boolean(value) => json!({ "boolean": value }),
        Node::Number(number) => json!({ "number": number.get().to_string() }),
        Node::String(text) => json!({ "string": text }),
        Node::Array(elements) => {
            json!({ "array": elements.iter().map(node_view).collect::<Vec<_>>() })
        }
        Node::Object(members) => json!({
            "object": members.iter().map(|(key, value)| json!([key, node_view(value)])).collect::<Vec<_>>(),
        }),
        Node::Marker(Marker::Rejected(reason)) => json!({ "marker": { "reason": reason.id() } }),
        Node::Marker(Marker::BootstrapLimitExceeded { bound, witness }) => json!({
            "marker": {
                "reason": BOOTSTRAP_LIMIT_EXCEEDED,
                "boundPath": bound.path(),
                "witness": witness.to_string(),
            },
        }),
    }
}

/// The call-site boundary: an unsafe host inspection, or the sidecar.
fn call_site(
    input: &Value,
    bodies: &Bodies,
    capability: &ImplementationCapability,
) -> Result<Value, Refusal> {
    let source = input
        .get("source")
        .map(|raw| bytes(raw, bodies))
        .unwrap_or_default();
    let report = if input.get("hostInspection").is_some() {
        CallSiteReport::Unsafe
    } else {
        CallSiteReport::Formal(SubmittedCallSite {
            identity: text(input, "identity"),
            marked: input["nonSecret"]
                .as_bool()
                .expect("the Case Schema makes the mark a boolean"),
            revision: text(input, "revision"),
            locator: input.get("locator").and_then(locator_candidate),
            source,
            mappings: input["mappings"]
                .as_array()
                .expect("the Case Schema makes the mappings a list")
                .iter()
                .map(|mapping| SubmittedMapping {
                    path: mapping["path"]
                        .as_array()
                        .expect("the Case Schema makes the path a list")
                        .iter()
                        .map(edge)
                        .collect(),
                    role: mapping["role"]
                        .as_str()
                        .expect("the Case Schema makes the role text")
                        .to_owned(),
                    span: SpanCandidate {
                        start: mapping["span"]["start"].as_f64().expect("a number"),
                        end: mapping["span"]["end"].as_f64().expect("a number"),
                    },
                })
                .collect(),
        })
    };
    let admitted = call_site::admit(Some(report), capability).map_err(Refusal::CallSite)?;
    let admitted = admitted.expect("a submitted sidecar is admitted as one");
    Ok(json!({ "json": call_site_view(&admitted) }))
}

/// A locator candidate: its variant, its payload, and its mark. A variant 015
/// does not name is no candidate, which the boundary would admit as absence.
fn locator_candidate(candidate: &Value) -> Option<LocatorCandidate<'_>> {
    let text = |name: &str| {
        candidate[name]
            .as_str()
            .expect("the Case Schema makes it text")
    };
    let variant = match text("variant") {
        "relative-path" => LocatorVariant::RelativePath,
        "safe-label" => LocatorVariant::SafeLabel,
        "safe-uri" => LocatorVariant::SafeUri,
        _ => return None,
    };
    Some(LocatorCandidate {
        variant,
        payload: text("payload"),
        non_secret: candidate["nonSecret"]
            .as_bool()
            .expect("the Case Schema makes the mark a boolean"),
    })
}

/// One edge of a mapping's path as submitted.
fn edge(edge: &Value) -> SubmittedEdge {
    if let Some(key) = edge.get("key").and_then(Value::as_str) {
        return SubmittedEdge::Key(key.as_bytes().to_vec());
    }
    if let Some(units) = edge.get("keyUtf16").and_then(Value::as_array) {
        return SubmittedEdge::Key(code_units(units).expect("the Case Schema makes code units"));
    }
    SubmittedEdge::Index(
        edge["index"]
            .as_f64()
            .expect("the Case Schema makes the index a number"),
    )
}

/// A span coordinate as submitted: an integer as an integer, so that a span
/// reads as the fixture wrote it, and any other number as it is.
#[expect(
    clippy::cast_possible_truncation,
    reason = "an integer within the exact range of binary64 converts exactly"
)]
fn number(value: f64) -> Value {
    const EXACT: f64 = 9_007_199_254_740_992.0;
    if value.fract() == 0.0 && value.abs() < EXACT {
        json!(value as i64)
    } else {
        json!(value)
    }
}

/// The admitted sidecar (D15:3267): its pair and mark, its locator or none,
/// its source as text, and its mappings in canonical order with each span as
/// submitted.
fn call_site_view(sidecar: &AdmittedCallSite) -> Value {
    let mappings = sidecar
        .mappings()
        .map(|(path, role, span)| {
            let path = path
                .iter()
                .map(|edge| match edge {
                    StructuralEdge::Key(key) => json!({ "key": key }),
                    StructuralEdge::Index(index) => json!({ "index": index }),
                })
                .collect::<Vec<_>>();
            let span = json!({ "start": number(span.start), "end": number(span.end) });
            json!({ "path": path, "role": role.id(), "span": span })
        })
        .collect::<Vec<_>>();
    let mut view = json!({
        "identity": sidecar.pair().identity().as_str(),
        "nonSecret": sidecar.marked(),
        "revision": sidecar.pair().revision(),
        "source": sidecar.source(),
        "mappings": mappings,
    });
    if let Some(locator) = sidecar.locator() {
        view["locator"] = locator.view();
    }
    sorted(&view)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::input_limits::Bound;

    fn marker(reason: &str, bound: Option<&str>, witness: Option<u64>) -> SubmittedNode {
        SubmittedNode::Marker(SubmittedMarker {
            reason: reason.to_owned(),
            bound: bound.map(str::to_owned),
            witness,
        })
    }

    #[test]
    fn a_number_is_read_only_from_decimal_text() {
        for text in [
            "0",
            "-0",
            "1",
            "-42",
            "0.5",
            "1234.5678",
            "1e21",
            "1E-7",
            "2.5e+3",
            "0e0",
        ] {
            assert!(decimal(text), "{text}");
        }
        for text in [
            "", "-", "01", "-01", "+1", ".5", "5.", "1.e3", "1e", "1e+", "e3", "NaN", "inf",
            "Infinity", "0x10", " 1", "1 ", "1_000", "--1", "1e3.5",
        ] {
            assert!(!decimal(text), "{text}");
        }
    }

    #[test]
    fn a_witness_is_read_only_from_the_decimal_text_of_a_count() {
        for text in ["0", "1", "18446744073709551615"] {
            assert!(count(text), "{text}");
        }
        for text in ["", "01", "+1", "-1", "1.0", "1e3", " 1"] {
            assert!(!count(text), "{text}");
        }
    }

    /// A surrogate pair is its scalar value, and a lone surrogate its own
    /// generalized UTF-8.
    #[test]
    fn code_units_are_their_generalized_utf8() {
        let units = |units: Value| code_units(units.as_array().unwrap());
        assert_eq!(units(json!([0x61, 0xe9])), Some("aé".as_bytes().to_vec()));
        assert_eq!(
            units(json!([0xd83d, 0xde00])),
            Some("\u{1f600}".as_bytes().to_vec())
        );
        assert_eq!(units(json!([0xd800])), Some(vec![0xed, 0xa0, 0x80]));
        assert_eq!(
            units(json!([0xdfff, 0x61])),
            Some(vec![0xed, 0xbf, 0xbf, 0x61])
        );
        assert_eq!(units(json!([65536])), None);
        assert_eq!(units(json!([-1])), None);
        assert_eq!(units(json!(["a"])), None);
        assert_eq!(
            utf16(&json!({ "stringUtf16": [0x61] })),
            Some(b"a".to_vec())
        );
        assert_eq!(utf16(&json!({ "stringUtf16": [0x61], "extra": 1 })), None);
        assert_eq!(utf16(&json!({ "stringUtf16": "a" })), None);
        assert_eq!(utf16(&json!("a")), None);
    }

    /// A node of the closed model is submitted as it is, and any other value
    /// as a marker of no registered reason (D15:3250).
    #[test]
    fn a_fixture_node_is_submitted_in_the_closed_model_or_outside_it() {
        let node = |value: Value| submitted(&value);
        assert!(node(json!({ "null": null })) == SubmittedNode::Null);
        assert!(node(json!({ "boolean": true })) == SubmittedNode::Boolean(true));
        assert!(node(json!({ "number": "-2.25" })) == SubmittedNode::Number(-2.25));
        assert!(node(json!({ "number": "-0" })) == SubmittedNode::Number(-0.0));
        assert!(node(json!({ "string": "é" })) == SubmittedNode::String("é".as_bytes().to_vec()));
        assert!(
            node(json!({ "stringUtf16": [0xd800] }))
                == SubmittedNode::String(vec![0xed, 0xa0, 0x80])
        );
        assert!(
            node(json!({ "array": [{ "null": null }, { "boolean": false }] }))
                == SubmittedNode::Array(vec![SubmittedNode::Null, SubmittedNode::Boolean(false)])
        );
        // Members keep their submitted order and repetition, and a key may be
        // code units.
        assert!(
            node(json!({ "object": [
                ["b", { "null": null }],
                [{ "stringUtf16": [0x61] }, { "boolean": false }],
                ["b", { "null": null }]
            ] }))
                == SubmittedNode::Object(vec![
                    (b"b".to_vec(), SubmittedNode::Null),
                    (b"a".to_vec(), SubmittedNode::Boolean(false)),
                    (b"b".to_vec(), SubmittedNode::Null),
                ])
        );
        // A member outside the model leaves its node a marker of no
        // registered reason inside the object.
        assert!(
            node(json!({ "object": [["a", { "unknown": 1 }]] }))
                == SubmittedNode::Object(vec![(b"a".to_vec(), outside())])
        );
        assert!(node(json!({ "marker": { "reason": "cycle" } })) == marker("cycle", None, None));
        assert!(
            node(json!({ "marker": {
                "reason": "bootstrap-limit-exceeded",
                "boundPath": "bootstrapOnly.snapshot.maxNodes",
                "witness": "2"
            } }))
                == marker(
                    "bootstrap-limit-exceeded",
                    Some("bootstrapOnly.snapshot.maxNodes"),
                    Some(2)
                )
        );
        // A marker keeps what it states, so the boundary decides whether its
        // members fit its reason.
        assert!(
            node(json!({ "marker": { "reason": "cycle", "witness": "0" } }))
                == marker("cycle", None, Some(0))
        );
        for value in [
            json!(null),
            json!([]),
            json!({}),
            json!({ "null": null, "boolean": true }),
            json!({ "unknown": null }),
            json!({ "null": false }),
            json!({ "boolean": "true" }),
            json!({ "number": 1 }),
            json!({ "number": "NaN" }),
            json!({ "number": "01" }),
            json!({ "string": 1 }),
            json!({ "stringUtf16": [65536] }),
            json!({ "stringUtf16": "a" }),
            json!({ "array": {} }),
            json!({ "object": {} }),
            json!({ "object": [["a"]] }),
            json!({ "object": [["a", { "null": null }, 1]] }),
            json!({ "object": [[1, { "null": null }]] }),
            json!({ "object": [[{ "stringUtf16": [97], "extra": 1 }, { "null": null }]] }),
            json!({ "marker": "cycle" }),
            json!({ "marker": {} }),
            json!({ "marker": { "reason": 1 } }),
            json!({ "marker": { "reason": "cycle", "bound": "p" } }),
            json!({ "marker": { "reason": "cycle", "boundPath": 1 } }),
            json!({ "marker": { "reason": "cycle", "witness": 2 } }),
            json!({ "marker": { "reason": "cycle", "witness": "02" } }),
            json!({ "marker": { "reason": "cycle", "witness": "18446744073709551616" } }),
        ] {
            assert!(node(value.clone()) == outside(), "{value}");
        }
    }

    #[test]
    fn every_admitted_selector_is_written_in_its_report_form() {
        let bound = Bound::new(8).unwrap();
        assert_eq!(
            selector_view(&SelectorInput::absent(bound)),
            json!({ "report": "absent" })
        );
        assert_eq!(
            selector_view(&SelectorInput::string("app", bound)),
            json!({ "report": "string", "value": "app" })
        );
        assert_eq!(
            selector_view(&SelectorInput::over_limit(bound)),
            json!({ "report": "over-limit-string" })
        );
        for (kind, tag) in TAGS {
            assert_eq!(
                selector_view(&SelectorInput::invalid_type(kind, bound)),
                json!({ "report": "invalid-type", "type": tag })
            );
            assert!(
                selector_report(&json!({ "report": "invalid-type", "type": tag }))
                    == SelectorReport::InvalidType(kind)
            );
        }
        for (report, read) in [
            (json!({ "report": "absent" }), SelectorReport::Absent),
            (
                json!({ "report": "string", "value": "array" }),
                SelectorReport::String("array"),
            ),
            (
                json!({ "report": "over-limit-string" }),
                SelectorReport::OverLimitString,
            ),
            (
                json!({ "report": "unrepresentable" }),
                SelectorReport::Unrepresentable,
            ),
            (json!({ "report": "unsafe" }), SelectorReport::Unsafe),
        ] {
            assert!(selector_report(&report) == read, "{report}");
        }
    }

    #[test]
    fn a_locator_candidate_of_an_unknown_variant_is_none() {
        for (variant, read) in [
            ("relative-path", LocatorVariant::RelativePath),
            ("safe-label", LocatorVariant::SafeLabel),
            ("safe-uri", LocatorVariant::SafeUri),
        ] {
            let candidate = json!({ "variant": variant, "payload": "p", "nonSecret": true });
            let read_candidate = locator_candidate(&candidate).unwrap();
            assert_eq!(read_candidate.variant, read);
            assert_eq!(read_candidate.payload, "p");
            assert!(read_candidate.non_secret);
        }
        let unknown = json!({ "variant": "file-uri", "payload": "p", "nonSecret": false });
        assert!(locator_candidate(&unknown).is_none());
    }

    #[test]
    fn an_edge_is_a_key_its_code_units_or_an_index() {
        assert!(matches!(edge(&json!({ "key": "a" })), SubmittedEdge::Key(key) if key == b"a"));
        assert!(matches!(
            edge(&json!({ "keyUtf16": [0xd800] })),
            SubmittedEdge::Key(key) if key == [0xed, 0xa0, 0x80]
        ));
        assert!(matches!(
            edge(&json!({ "index": 1.5 })),
            SubmittedEdge::Index(index) if index.to_bits() == 1.5_f64.to_bits()
        ));
    }

    #[test]
    fn an_integral_coordinate_is_written_as_an_integer() {
        assert_eq!(number(3.0).to_string(), "3");
        assert_eq!(number(-2.0).to_string(), "-2");
        assert_eq!(
            number(9_007_199_254_740_991.0).to_string(),
            "9007199254740991"
        );
        assert_eq!(number(1.5).to_string(), "1.5");
        assert_eq!(
            number(9_007_199_254_740_992.0).to_string(),
            "9007199254740992.0"
        );
    }

    #[test]
    fn members_are_sorted_by_their_unsigned_bytes_at_every_depth() {
        let value = json!({ "é": 1, "b": [{ "z": 1, "a": 2 }], "B": { "y": 1, "x": 2 } });
        assert_eq!(
            serde_json::to_string(&sorted(&value)).unwrap(),
            r#"{"B":{"x":2,"y":1},"b":[{"a":2,"z":1}],"é":1}"#
        );
    }

    #[test]
    fn a_value_that_is_no_string_is_no_text() {
        assert_eq!(text_input(&json!("a")), TextInput::Text("a"));
        for value in [json!(null), json!(1), json!(["a"]), json!({ "a": "a" })] {
            assert_eq!(text_input(&value), TextInput::NotText, "{value}");
        }
    }

    fn constructed() -> Constructed {
        crate::conformance::construction::constructed(
            &crate::construction::flow::testing::committed(),
        )
        .unwrap()
    }

    /// The capability value of the committed cases with the bound at
    /// `pointer` set to `value`.
    fn capability_with(pointer: &str, value: u64) -> Value {
        let mut capability = crate::conformance::admission_cases::capability();
        *capability.pointer_mut(pointer).unwrap() = json!(value.to_string());
        capability
    }

    /// The locale data boundary reads both of its bounds from the
    /// capability: the manifest's bytes and its depth.
    #[test]
    fn the_locale_data_boundary_reads_its_byte_and_depth_bounds() {
        const MANIFEST: &[u8] = include_bytes!("../../../intlify_locale_icu4x/data/manifest.json");
        let constructed = constructed();
        let bytes = u64::try_from(MANIFEST.len()).unwrap();
        let reason = |pointer: &str, value: u64| {
            let fixture = json!({
                "capability": { "json": capability_with(pointer, value) },
                "input": {
                    "manifest": { "bytes": crate::resolver::evidence_source::hex(MANIFEST) },
                    "payloads": {},
                },
            });
            outcome(
                "locale-canonicalization-data-artifact",
                &fixture,
                &Bodies::new(),
                &constructed,
            )["failureReason"]
                .clone()
        };
        let size = "/bootstrapOnly/inputBoundary/maxLocaleDataManifestBytes";
        let depth = "/bootstrapOnly/inputBoundary/maxDepth";
        assert_eq!(reason(size, bytes), Value::Null);
        assert_eq!(reason(size, bytes - 1), "over-bound-envelope");
        assert_eq!(reason(depth, 5), Value::Null);
        assert_eq!(reason(depth, 4), "over-bound-envelope");
    }

    /// An admitted Snapshot's markers are written in the Snapshot form: each
    /// rejected reason as its ID, and a terminal marker with its bound path
    /// and decimal witness (D15:3250).
    #[test]
    fn an_admitted_snapshot_s_markers_are_written_in_their_form() {
        const VECTORS: &[u8] = include_bytes!("../../fixtures/snapshot/vectors.json");
        let vectors: Value = serde_json::from_slice(VECTORS).unwrap();
        let vector = |id: &str| {
            vectors["snapshots"]
                .as_array()
                .unwrap()
                .iter()
                .find(|vector| vector["id"] == id)
                .unwrap()
                .clone()
        };
        let constructed = constructed();
        let terminal = vector("bootstrap-maxNodes");
        let mut marker = terminal["snapshot"]["marker"].clone();
        let bound = marker.as_object_mut().unwrap().remove("bound").unwrap();
        marker["boundPath"] = bound;
        // The vectors hold one Snapshot of each rejected reason's marker.
        let rejected = vectors["snapshots"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|vector| vector["id"].as_str().unwrap().starts_with("marker-"))
            .map(|vector| {
                (
                    vector["snapshot"].clone(),
                    vector["digest"].clone(),
                    crate::conformance::admission_cases::capability(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(rejected.len(), 9);
        for (snapshot, digest, capability) in rejected.into_iter().chain([(
            json!({ "marker": marker }),
            terminal["digest"].clone(),
            capability_with("/bootstrapOnly/snapshot/maxNodes", 1),
        )]) {
            let fixture = json!({
                "capability": { "json": capability },
                "input": { "snapshot": snapshot, "digest": digest },
            });
            let outcome = outcome(
                "programmatic-entry-snapshot",
                &fixture,
                &Bodies::new(),
                &constructed,
            );
            assert_eq!(outcome["normalizedInput"], fixture["input"], "{outcome}");
        }
    }
}
