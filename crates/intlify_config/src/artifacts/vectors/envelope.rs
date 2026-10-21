// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The envelope vectors an independent implementation checks. For each member
//! they record what it declares, the tuple it establishes, whether its
//! integrity holds, and its canonical bytes; then the members whose semantic
//! digests must agree or differ, transport spellings whose canonical bodies
//! must agree, members that must stay unequal, texts the boundary refuses, the
//! orders of members within one tuple and without one, and references with
//! their tuples and canonical bytes. The contents are test values, not
//! artifacts any project admits.

use serde_json::{json, Value};

use crate::artifacts::collection::{read_collection, CollectionFailure, CollectionLimits};
use crate::artifacts::envelope::{
    envelope_of, sealed, ArtifactKind, Declaration, ExactTuple, FormalMember, INTEGRITY_DOMAIN,
};
use crate::artifacts::SemanticDigest;
use crate::references::{PolicyReference, TargetProfileReference};

/// Limits no vector comes near.
const ROOM: CollectionLimits = CollectionLimits {
    members: u64::MAX,
    member_bytes: u64::MAX,
    total_bytes: u64::MAX,
    depth: u64::MAX,
};

fn target_profile() -> Value {
    let versioned =
        |identity: &str, revision: &str| json!({"identity": identity, "revision": revision});
    json!({
        "host": {
            "kind": "javascript-esm-dom-text",
            "grammar": versioned("ecmascript", "2025"),
            "moduleProfile": versioned("intlify-esm-module", "0"),
        },
        "mode": "runtime-backed",
        "adapter": versioned("intlify-vanilla-dom", "0"),
        "execution": {
            "specification": versioned("intlify-design-023", "0"),
            "profile": versioned("intlify-text-interpolation-minimum", "0"),
        },
        "mf2Specification": versioned("unicode-mf2", "48"),
        "codecs": {
            "bindingTable": versioned("intlify-binding-table", "0"),
            "localePayload": versioned("intlify-locale-payload", "0"),
            "descriptor": versioned("intlify-target-descriptor", "0"),
            "sourceMap": versioned("ecma-426-source-map", "3"),
        },
        "locale": {
            "selection": "direct-only",
            "services": "absent",
            "directions": [
                {"locale": "en", "direction": "LTR"},
                {"locale": "ja", "direction": "LTR"},
            ],
        },
        "delivery": {"unit": "main", "loading": "eager"},
        "limits": {
            "maxMessages": "1000",
            "maxUses": "10000",
            "maxLocales": "8",
            "maxFiles": "64",
            "maxFileBytes": "1048576",
            "maxOutputBytes": "8388608",
            "maxParameterBytes": "4096",
            "maxTextBytes": "65536",
        },
    })
}

fn approval() -> Value {
    envelope_of(
        ArtifactKind::ApprovalPolicy,
        "storefront-approval",
        "1",
        json!({"localizedApproval": "required"}),
    )
}

/// One sealed envelope of each admitted kind, in kind order.
fn admitted() -> Vec<(&'static str, Value)> {
    let routing = json!({
        "adapter": {"identity": "intlify-provider-fixture", "revision": "0"},
        "configuration": {
            "identity": "storefront-routing",
            "semanticDigest": SemanticDigest::from_hash([0x2a; 32]).as_str(),
        },
    });
    let section = json!({"configurationInput": {"maxConfigurationBytes": "1048576"}});
    vec![
        ("approval-policy", approval()),
        (
            "glossary-set",
            envelope_of(
                ArtifactKind::GlossarySet,
                "storefront-glossary",
                "1",
                json!({}),
            ),
        ),
        (
            "provider-routing-policy",
            envelope_of(
                ArtifactKind::ProviderRoutingPolicy,
                "storefront-routing",
                "1",
                routing,
            ),
        ),
        (
            "resource-limit-policy",
            envelope_of(
                ArtifactKind::ResourceLimitPolicy,
                "storefront-limits",
                "1",
                json!({"projectProfileResolution": section}),
            ),
        ),
        (
            "selection-policy",
            envelope_of(
                ArtifactKind::SelectionPolicy,
                "storefront-selection",
                "1",
                json!({"mode": "explicit"}),
            ),
        ),
        (
            "source-admission-policy",
            envelope_of(
                ArtifactKind::SourceAdmissionPolicy,
                "storefront-source-admission",
                "1",
                json!({"sourceApproval": "authenticated-sufficient"}),
            ),
        ),
        (
            "target-profile",
            envelope_of(
                ArtifactKind::TargetProfile,
                "storefront-web",
                "1",
                target_profile(),
            ),
        ),
        (
            "trust-policy",
            envelope_of(
                ArtifactKind::TrustPolicy,
                "storefront-trust",
                "1",
                json!({}),
            ),
        ),
    ]
}

/// `value` with the member at `pointer` set, or removed for `None`; sealed
/// again when `reseal`.
fn edited(value: &Value, pointer: &str, replacement: Option<Value>, reseal: bool) -> Value {
    let mut edited = value.clone();
    let (parent, name) = pointer.rsplit_once('/').expect("a pointer below the root");
    let object = edited
        .pointer_mut(parent)
        .and_then(Value::as_object_mut)
        .expect("an object to edit");
    match replacement {
        Some(replacement) => object.insert(name.into(), replacement),
        None => object.remove(name),
    };
    if reseal {
        sealed(edited)
    } else {
        edited
    }
}

/// Members that are envelopes of the approval policy in every respect but one.
fn variants() -> Vec<(&'static str, Value)> {
    let base = approval();
    let limits = |content: Value| {
        envelope_of(
            ArtifactKind::ResourceLimitPolicy,
            "storefront-limits",
            "1",
            content,
        )
    };
    let digest = |byte: u8| Some(json!(SemanticDigest::from_hash([byte; 32]).as_str()));
    vec![
        // The Resource Limit Policy's section is 015's to check, not 017's.
        ("resource-limit-policy-without-section", limits(json!({}))),
        (
            "resource-limit-policy-non-object-section",
            limits(json!({"projectProfileResolution": "default"})),
        ),
        (
            "resource-limit-policy-invalid-section",
            limits(
                json!({"projectProfileResolution": {"configurationInput": {"maxConfigurationBytes": "0"}}}),
            ),
        ),
        // Name and revision edits keep the semantic digest; content and
        // specification-revision edits change it.
        (
            "approval-policy-renamed",
            edited(
                &base,
                "/body/identity",
                Some(json!("storefront-approval-renamed")),
                true,
            ),
        ),
        (
            "approval-policy-revised",
            edited(&base, "/body/revision", Some(json!("2")), true),
        ),
        (
            "approval-policy-other-content",
            edited(
                &base,
                "/body/content/localizedApproval",
                Some(json!("not-required")),
                true,
            ),
        ),
        (
            "approval-policy-specification-revised",
            edited(&base, "/specification/revision", Some(json!("1")), true),
        ),
        // A failing integrity or content keeps the tuple.
        (
            "approval-policy-stale-integrity",
            edited(
                &base,
                "/body/content/localizedApproval",
                Some(json!("not-required")),
                false,
            ),
        ),
        (
            "approval-policy-invalid-content",
            edited(
                &base,
                "/body/content/localizedApproval",
                Some(json!("sometimes")),
                true,
            ),
        ),
        (
            "approval-policy-other-integrity-digest",
            edited(&base, "/integrityDigest", digest(0x07), false),
        ),
        (
            "approval-policy-another-integrity-digest",
            edited(&base, "/integrityDigest", digest(0xf0), false),
        ),
    ]
}

/// Members that establish no tuple, and members that are unsupported.
fn without_tuple() -> Vec<(&'static str, Value)> {
    let base = approval();
    let mut members = vec![
        ("missing-body", edited(&base, "/body", None, false)),
        (
            "missing-integrity-digest",
            edited(&base, "/integrityDigest", None, false),
        ),
        (
            "missing-schema-revision",
            edited(&base, "/schemaRevision", None, true),
        ),
        (
            "missing-specification",
            edited(&base, "/specification", None, true),
        ),
        (
            "missing-content",
            edited(&base, "/body/content", None, true),
        ),
        (
            "unknown-member",
            edited(&base, "/description", Some(json!("approval")), true),
        ),
        (
            "unknown-specification-member",
            edited(&base, "/specification/note", Some(json!("x")), true),
        ),
        (
            "unknown-body-member",
            edited(&base, "/body/note", Some(json!("x")), true),
        ),
        (
            "wrong-governing-specification",
            edited(
                &base,
                "/specification/identity",
                Some(json!("intlify-design-018")),
                true,
            ),
        ),
        (
            "malformed-identity",
            edited(&base, "/body/identity", Some(json!("Storefront")), true),
        ),
        (
            "malformed-specification-revision",
            edited(&base, "/specification/revision", Some(json!("")), true),
        ),
        (
            "non-object-content",
            edited(&base, "/body/content", Some(json!(["required"])), true),
        ),
        (
            "malformed-integrity-digest",
            edited(&base, "/integrityDigest", Some(json!("sha256:00")), false),
        ),
        ("non-object-member", json!(["approval-policy"])),
        ("no-kind", json!({"schemaRevision": "0"})),
        (
            "non-string-kind",
            edited(&base, "/kind", Some(json!(null)), true),
        ),
    ];
    members.extend([
        (
            "unsupported-kind",
            edited(&base, "/kind", Some(json!("localized-message")), true),
        ),
        (
            "vocabulary-kind",
            edited(
                &base,
                "/kind",
                Some(json!("intent-surface-class-vocabulary")),
                true,
            ),
        ),
        (
            "unsupported-schema-revision",
            edited(&base, "/schemaRevision", Some(json!("1")), true),
        ),
    ]);
    members
}

fn tuple_value(tuple: &ExactTuple) -> Value {
    json!({
        "kind": tuple.kind.spelling(),
        "identity": tuple.identity.as_str(),
        "revision": tuple.revision.as_str(),
        "specificationRevision": tuple.specification_revision.as_str(),
        "semanticDigest": tuple.semantic_digest.as_str(),
    })
}

fn framed(value: &Value) -> Result<FormalMember, String> {
    FormalMember::new(value.clone()).map_err(|failure| format!("{failure:?}"))
}

/// What one member declares, the tuple it establishes, whether its integrity
/// holds, and its canonical bytes, all read back through this crate.
fn record(id: &str, value: &Value) -> Result<Value, String> {
    let member = framed(value)?;
    let declaration = match member.declaration() {
        Declaration::NoKind => Value::Null,
        Declaration::Unsupported => json!("unsupported"),
        Declaration::Admitted(kind) => json!(kind.spelling()),
    };
    let envelope = member.envelope();
    Ok(json!({
        "id": id,
        "member": value,
        "declaration": declaration,
        "tuple": envelope.as_ref().map_or(Value::Null, |envelope| tuple_value(envelope.tuple())),
        "integrityHolds": envelope.as_ref().map_or(Value::Null, |envelope| json!(envelope.integrity_holds())),
        "canonicalBytes": member.canonical_bytes(),
    }))
}

/// The ids of `members`, ordered by their canonical bodies.
fn ordered(members: &[(&'static str, Value)]) -> Result<Vec<&'static str>, String> {
    let mut keyed = members
        .iter()
        .map(|(id, value)| Ok((framed(value)?.canonical_body().to_vec(), *id)))
        .collect::<Result<Vec<_>, String>>()?;
    keyed.sort();
    Ok(keyed.into_iter().map(|(_, id)| id).collect())
}

/// Transport spellings of the approval policy: compact, spaced, reordered, and
/// with one character escaped. They must frame to one canonical body.
fn spellings() -> Result<Vec<String>, String> {
    let base = approval();
    let compact = serde_json::to_string(&base).map_err(|error| error.to_string())?;
    let spaced = serde_json::to_string_pretty(&base).map_err(|error| error.to_string())?;
    let reordered = format!(
        "{{\"integrityDigest\":{},\"body\":{},\"specification\":{},\"schemaRevision\":\"0\",\"kind\":\"approval-policy\"}}",
        base["integrityDigest"], base["body"], base["specification"]
    );
    let escaped = compact.replacen(
        "\"revision\":\"1\"",
        &format!("\"revision\":\"{}u0031\"", '\\'),
        1,
    );
    let texts = vec![compact, spaced, reordered, escaped];
    let bytes: Vec<&[u8]> = texts.iter().map(String::as_bytes).collect();
    let collection =
        read_collection(Some(bytes.as_slice()), &ROOM).map_err(|failure| format!("{failure:?}"))?;
    let members = collection.members();
    if members
        .iter()
        .any(|member| member.canonical_body() != members[0].canonical_body())
    {
        return Err("the spellings frame differently".into());
    }
    Ok(texts)
}

/// Texts the boundary refuses because a JSON number makes them unrepresentable.
fn unrepresentable() -> Result<Vec<String>, String> {
    let base = serde_json::to_string(&approval()).map_err(|error| error.to_string())?;
    let texts = vec![
        base.replacen("\"1\"", "1", 1),
        base.replacen(
            "{\"localizedApproval\"",
            "{\"limit\":0,\"localizedApproval\"",
            1,
        ),
    ];
    for text in &texts {
        let bytes = [text.as_bytes()];
        if read_collection(Some(bytes.as_slice()), &ROOM) != Err(CollectionFailure::Unrepresentable)
        {
            return Err(format!("{text} was not refused"));
        }
    }
    Ok(texts)
}

fn references(members: &[(&'static str, Value)]) -> Result<Vec<Value>, String> {
    let mut references = Vec::new();
    for id in ["approval-policy", "target-profile"] {
        let value = &members
            .iter()
            .find(|(member, _)| *member == id)
            .ok_or(format!("no member {id}"))?
            .1;
        let member = framed(value)?;
        let tuple = member
            .envelope()
            .ok_or(format!("{id} establishes no tuple"))?
            .tuple()
            .clone();
        let reference = tuple_value(&tuple);
        let (named, bytes) = if tuple.kind == ArtifactKind::TargetProfile {
            let named: TargetProfileReference =
                serde_json::from_value(reference.clone()).map_err(|error| error.to_string())?;
            (named.tuple(), named.canonical_bytes())
        } else {
            let named: PolicyReference =
                serde_json::from_value(reference.clone()).map_err(|error| error.to_string())?;
            (named.tuple(), named.canonical_bytes())
        };
        if named != tuple {
            return Err(format!("the reference to {id} names another tuple"));
        }
        references.push(json!({
            "resolves": id,
            "reference": reference,
            "canonicalBytes": bytes,
        }));
    }
    Ok(references)
}

/// The complete vectors document.
pub fn document() -> Result<Value, String> {
    let admitted = admitted();
    let variants = variants();
    let without = without_tuple();
    let all: Vec<(&'static str, Value)> = admitted
        .iter()
        .chain(&variants)
        .chain(&without)
        .cloned()
        .collect();
    let members = all
        .iter()
        .map(|(id, value)| record(id, value))
        .collect::<Result<Vec<_>, String>>()?;
    let one_tuple: Vec<(&'static str, Value)> = all
        .iter()
        .filter(|(id, _)| {
            matches!(
                *id,
                "approval-policy"
                    | "approval-policy-other-integrity-digest"
                    | "approval-policy-another-integrity-digest"
            )
        })
        .cloned()
        .collect();
    let kinds: Vec<Value> = ArtifactKind::ALL
        .iter()
        .map(|kind| {
            json!({
                "kind": kind.spelling(),
                "governingSpecification": kind.governing_specification(),
                "semanticDomain": kind.semantic_domain(),
            })
        })
        .collect();
    Ok(json!({
        "note": "Members of a Submitted Profile Resolution Artifact Collection, read by intlify_config. \
                 An independent implementation decides from each member alone what it declares, \
                 whether it establishes an exact-reference tuple and which, whether its integrity \
                 holds, and its canonical bytes; checks the members whose semantic digests must agree \
                 or differ, the spellings that must frame to one canonical body, the members that \
                 must stay unequal, and the texts a JSON number makes unrepresentable; orders the \
                 members within one tuple and the members without a tuple by their canonical bodies; \
                 and checks that each reference names its member's tuple. The contents are test \
                 values.",
        "integrityDomain": INTEGRITY_DOMAIN,
        "kinds": kinds,
        "members": members,
        "sameSemanticDigest": [
            ["approval-policy", "approval-policy-renamed"],
            ["approval-policy", "approval-policy-revised"],
            ["approval-policy", "approval-policy-other-integrity-digest"],
        ],
        "differentSemanticDigest": [
            ["approval-policy", "approval-policy-other-content"],
            ["approval-policy", "approval-policy-specification-revised"],
            ["glossary-set", "trust-policy"],
            ["resource-limit-policy", "resource-limit-policy-without-section"],
        ],
        "spellings": spellings()?,
        "unequalBodies": [
            ["approval-policy", "approval-policy-other-integrity-digest"],
            ["approval-policy", "approval-policy-stale-integrity"],
        ],
        "unrepresentable": unrepresentable()?,
        "withinOneTuple": ordered(&one_tuple)?,
        "withoutTuple": ordered(&without)?,
        "references": references(&admitted)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMITTED: &str = include_str!("../../../fixtures/envelope/vectors.json");

    fn committed() -> Value {
        serde_json::from_str(COMMITTED).unwrap()
    }

    #[test]
    fn the_committed_vectors_are_the_ones_this_crate_writes() {
        // Compared as values: the repository formatter owns the file's layout.
        assert_eq!(committed(), document().unwrap());
    }

    #[test]
    fn every_member_reads_as_the_independent_implementation_decided() {
        let vectors = committed();
        for case in vectors["members"].as_array().unwrap() {
            let recorded = record(case["id"].as_str().unwrap(), &case["member"]).unwrap();
            assert_eq!(&recorded, case, "{}", case["id"]);
        }
    }

    #[test]
    fn the_vectors_cover_every_kind_and_both_sides_of_each_decision() {
        let vectors = committed();
        let members = vectors["members"].as_array().unwrap();
        for kind in ArtifactKind::ALL {
            assert!(
                members
                    .iter()
                    .any(|case| case["tuple"]["kind"] == kind.spelling()),
                "{}",
                kind.spelling()
            );
        }
        let count = |predicate: &dyn Fn(&Value) -> bool| {
            members.iter().filter(|case| predicate(case)).count()
        };
        assert!(count(&|case| case["tuple"].is_null() && case["declaration"].is_null()) > 0);
        assert!(count(&|case| case["declaration"] == "unsupported") > 0);
        assert!(
            count(&|case| case["tuple"].is_null()
                && case["declaration"].is_string()
                && case["declaration"] != "unsupported")
                > 0
        );
        assert!(count(&|case| case["integrityHolds"] == false) > 0);
    }
}
