// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The envelope vectors an independent implementation checks. For each member
//! they record what it declares, the tuple it establishes, whether its
//! integrity holds, how its body admission ends, and its canonical bytes; then
//! the members whose semantic digests must agree or differ, transport
//! spellings whose canonical bodies must agree, members that must stay
//! unequal, texts the boundary refuses, the orders of members within one tuple
//! and without one, changes to an admitted content with whether the result is
//! still the kind's closed content, and references with their tuples and
//! canonical bytes. The contents are test values, not artifacts any project
//! admits.

use serde_json::{json, Value};

use crate::artifacts::collection::{read_collection, CollectionFailure, CollectionLimits};
use crate::artifacts::content::{self, sample};
use crate::artifacts::envelope::{
    envelope_of, sealed, ArtifactKind, BodyFailure, Declaration, Envelope, ExactTuple,
    FormalMember, INTEGRITY_DOMAIN,
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

fn approval() -> Value {
    envelope_of(
        ArtifactKind::ApprovalPolicy,
        "storefront-approval",
        "1",
        sample(ArtifactKind::ApprovalPolicy),
    )
}

/// One sealed envelope of each admitted kind, in kind order, with its sample
/// content.
fn admitted() -> Vec<(&'static str, Value)> {
    [
        (ArtifactKind::ApprovalPolicy, "storefront-approval"),
        (ArtifactKind::GlossarySet, "storefront-glossary"),
        (ArtifactKind::ProviderRoutingPolicy, "storefront-routing"),
        (ArtifactKind::ResourceLimitPolicy, "storefront-limits"),
        (ArtifactKind::SelectionPolicy, "storefront-selection"),
        (
            ArtifactKind::SourceAdmissionPolicy,
            "storefront-source-admission",
        ),
        (ArtifactKind::TargetProfile, "storefront-web"),
        (ArtifactKind::TrustPolicy, "storefront-trust"),
    ]
    .into_iter()
    .map(|(kind, identity)| {
        (
            kind.spelling(),
            envelope_of(kind, identity, "1", sample(kind)),
        )
    })
    .collect()
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

/// Members that are envelopes of the approval policy in every respect but one,
/// and members with two failing parts, which body admission meets in order.
fn variants() -> Vec<(&'static str, Value)> {
    let base = approval();
    let of = |kind: ArtifactKind, identity: &str| envelope_of(kind, identity, "1", sample(kind));
    let glossary = of(ArtifactKind::GlossarySet, "storefront-glossary");
    let target = of(ArtifactKind::TargetProfile, "storefront-web");
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
        // Body admission reads the integrity, then the specification
        // revision, and only then the content.
        (
            "approval-policy-revised-specification-invalid-content",
            edited(
                &edited(&base, "/specification/revision", Some(json!("1")), false),
                "/body/content/localizedApproval",
                Some(json!("sometimes")),
                true,
            ),
        ),
        (
            "approval-policy-stale-integrity-revised-specification",
            edited(&base, "/specification/revision", Some(json!("1")), false),
        ),
        (
            "glossary-set-specification-revised",
            edited(&glossary, "/specification/revision", Some(json!("1")), true),
        ),
        (
            "target-profile-invalid-content",
            edited(&target, "/body/content/mode", Some(json!("jit")), true),
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

/// How the body admission of a member with a tuple ends.
fn admission(envelope: &Envelope<'_>) -> &'static str {
    match envelope.admit() {
        Ok(_) => "admitted",
        Err(BodyFailure::IntegrityInvalid) => "integrity-invalid",
        Err(BodyFailure::SpecificationRevisionUnsupported) => "specification-revision-unsupported",
        Err(BodyFailure::ContentInvalid) => "content-invalid",
    }
}

/// What one member declares, the tuple it establishes, whether its integrity
/// holds, how its body admission ends, and its canonical bytes, all read back
/// through this crate.
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
        "admission": envelope.as_ref().map_or(Value::Null, |envelope| json!(admission(envelope))),
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

/// One change to the sample content of `kind`: the member at `pointer` set to
/// `value`, or removed for `None`, and whether the changed content is meant to
/// be the kind's closed content.
type ContentCase = (
    &'static str,
    ArtifactKind,
    &'static str,
    Option<Value>,
    bool,
);

/// Changes that keep or break each rule of the five closed contents once, and
/// objects that the contents of 018 and 022 pass on as. The unit tests of the
/// content module try every rule more ways.
fn content_cases() -> Vec<ContentCase> {
    let of = |kind| {
        move |id, pointer, value, admitted| -> ContentCase { (id, kind, pointer, value, admitted) }
    };
    let (approval, selection) = (
        of(ArtifactKind::ApprovalPolicy),
        of(ArtifactKind::SelectionPolicy),
    );
    let (source, routing) = (
        of(ArtifactKind::SourceAdmissionPolicy),
        of(ArtifactKind::ProviderRoutingPolicy),
    );
    let target = of(ArtifactKind::TargetProfile);
    let set = |value: Value| Some(value);
    let pair =
        |identity: &str, revision: &str| set(json!({"identity": identity, "revision": revision}));
    let entry = |locale: &str, direction: &str| json!({"locale": locale, "direction": direction});
    let directions = |entries: &[Value]| set(Value::Array(entries.to_vec()));
    let (fullwidth, mathematical) = ("\u{ff41}", "\u{1d41a}");
    vec![
        // The three one-member Policy contents.
        approval(
            "approval-not-required",
            "/localizedApproval",
            set(json!("not-required")),
            true,
        ),
        approval(
            "approval-other-spelling",
            "/localizedApproval",
            set(json!("Required")),
            false,
        ),
        approval("approval-missing", "/localizedApproval", None, false),
        approval(
            "approval-unknown-member",
            "/mode",
            set(json!("explicit")),
            false,
        ),
        selection(
            "selection-other-mode",
            "/mode",
            set(json!("implicit")),
            false,
        ),
        selection("selection-unknown-member", "/note", set(json!("x")), false),
        source(
            "source-admission-required",
            "/sourceApproval",
            set(json!("required")),
            true,
        ),
        source(
            "source-admission-other-value",
            "/sourceApproval",
            set(json!("authenticated")),
            false,
        ),
        // The Provider routing.
        routing(
            "routing-other-adapter",
            "/adapter",
            pair("intlify-provider-other", "3"),
            true,
        ),
        routing(
            "routing-adapter-not-a-pair",
            "/adapter",
            set(json!("intlify-provider")),
            false,
        ),
        routing(
            "routing-uppercase-digest",
            "/configuration/semanticDigest",
            set(json!(format!("sha256:{}", "2A".repeat(32)))),
            false,
        ),
        routing(
            "routing-malformed-configuration",
            "/configuration/identity",
            set(json!("Routing")),
            false,
        ),
        routing(
            "routing-configuration-unknown-member",
            "/configuration/note",
            set(json!("x")),
            false,
        ),
        routing(
            "routing-unknown-member",
            "/fallback",
            set(json!("none")),
            false,
        ),
        // The Target Profile's mode and each value 017 fixes.
        target("target-aot", "/mode", set(json!("aot")), true),
        target("target-other-mode", "/mode", set(json!("jit")), false),
        target(
            "target-other-host-kind",
            "/host/kind",
            set(json!("javascript-cjs-dom-text")),
            false,
        ),
        target(
            "target-other-execution-specification",
            "/execution/specification",
            pair("intlify-design-023", "1"),
            false,
        ),
        target(
            "target-other-execution-profile",
            "/execution/profile",
            pair("intlify-text-interpolation", "0"),
            false,
        ),
        target(
            "target-other-source-map",
            "/codecs/sourceMap",
            pair("ecma-426-source-map", "2"),
            false,
        ),
        target(
            "target-other-locale-selection",
            "/locale/selection",
            set(json!("fallback")),
            false,
        ),
        target(
            "target-other-locale-services",
            "/locale/services",
            set(json!("present")),
            false,
        ),
        target(
            "target-other-delivery-unit",
            "/delivery/unit",
            set(json!("secondary")),
            false,
        ),
        target(
            "target-other-delivery-loading",
            "/delivery/loading",
            set(json!("lazy")),
            false,
        ),
        // An identity 017 leaves open, whose support 024 decides, and objects
        // that are closed.
        target(
            "target-other-grammar",
            "/host/grammar",
            pair("ecmascript", "2022"),
            true,
        ),
        target(
            "target-grammar-not-a-pair",
            "/host/grammar",
            set(json!({"identity": "es"})),
            false,
        ),
        target(
            "target-malformed-module-profile",
            "/host/moduleProfile",
            pair("Intlify-ESM", "0"),
            false,
        ),
        target(
            "target-unknown-member",
            "/hydration",
            set(json!("none")),
            false,
        ),
        target("target-missing-delivery", "/delivery", None, false),
        target(
            "target-host-unknown-member",
            "/host/note",
            set(json!("x")),
            false,
        ),
        // Directions.
        target(
            "target-no-directions",
            "/locale/directions",
            directions(&[]),
            true,
        ),
        target(
            "target-every-direction",
            "/locale/directions",
            directions(&[
                entry("ar", "RTL"),
                entry("en", "LTR"),
                entry("zxx", "unknown"),
            ]),
            true,
        ),
        target(
            "target-directions-by-utf8-bytes",
            "/locale/directions",
            directions(&[entry(fullwidth, "LTR"), entry(mathematical, "LTR")]),
            true,
        ),
        target(
            "target-directions-by-utf16-units",
            "/locale/directions",
            directions(&[entry(mathematical, "LTR"), entry(fullwidth, "LTR")]),
            false,
        ),
        target(
            "target-directions-unsorted",
            "/locale/directions",
            directions(&[entry("ja", "LTR"), entry("en", "LTR")]),
            false,
        ),
        target(
            "target-directions-repeated",
            "/locale/directions",
            directions(&[entry("en", "LTR"), entry("en", "LTR")]),
            false,
        ),
        target(
            "target-directions-repeated-otherwise",
            "/locale/directions",
            directions(&[entry("en", "LTR"), entry("en", "RTL")]),
            false,
        ),
        target(
            "target-direction-empty-locale",
            "/locale/directions",
            directions(&[entry("", "LTR")]),
            false,
        ),
        target(
            "target-direction-lowercase",
            "/locale/directions",
            directions(&[entry("en", "ltr")]),
            false,
        ),
        // Limits.
        target(
            "target-limit-one",
            "/limits/maxLocales",
            set(json!("1")),
            true,
        ),
        target(
            "target-limit-maximum",
            "/limits/maxOutputBytes",
            set(json!("18446744073709551615")),
            true,
        ),
        target(
            "target-limit-zero",
            "/limits/maxMessages",
            set(json!("0")),
            false,
        ),
        target(
            "target-limit-leading-zero",
            "/limits/maxUses",
            set(json!("010000")),
            false,
        ),
        target(
            "target-limit-over",
            "/limits/maxFileBytes",
            set(json!("18446744073709551616")),
            false,
        ),
        target("target-limit-missing", "/limits/maxTextBytes", None, false),
        // Contents 018 and 022 admit pass on as any object.
        of(ArtifactKind::GlossarySet)(
            "glossary-any-object",
            "/terms",
            set(json!([{"source": "cart"}])),
            true,
        ),
        of(ArtifactKind::ResourceLimitPolicy)(
            "resource-limit-any-object",
            "/projectProfileResolution",
            set(json!("default")),
            true,
        ),
        of(ArtifactKind::TrustPolicy)(
            "trust-any-object",
            "/roots",
            set(json!({"publisher": "storefront"})),
            true,
        ),
    ]
}

/// Each content case with this crate's decision, which must be the one the
/// case means.
fn contents() -> Result<Vec<Value>, String> {
    content_cases()
        .into_iter()
        .map(|(id, kind, pointer, value, meant)| {
            let changed = edited(&sample(kind), pointer, value.clone(), false);
            let object = changed.as_object().ok_or(format!("{id} is no object"))?;
            let admitted = content::read(kind, object).is_some();
            if admitted != meant {
                return Err(format!("{id}: admitted {admitted}, meant {meant}"));
            }
            let mut case = json!({
                "id": id,
                "base": kind.spelling(),
                "pointer": pointer,
                "admitted": admitted,
            });
            if let Some(value) = value {
                case["value"] = value;
            }
            Ok(case)
        })
        .collect()
}

/// A reference to each admitted kind's member, read back through this crate.
fn references(members: &[(&'static str, Value)]) -> Result<Vec<Value>, String> {
    let mut references = Vec::new();
    for (id, value) in members {
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
                 holds, how its body admission ends, and its canonical bytes; checks the members \
                 whose semantic digests must agree or differ, the spellings that must frame to one \
                 canonical body, the members that must stay unequal, and the texts a JSON number \
                 makes unrepresentable; orders the members within one tuple and the members without \
                 a tuple by their canonical bodies; decides for each change to an admitted member's \
                 content whether the result is still the kind's closed content; and checks that each \
                 reference names its member's tuple. The contents are test values.",
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
        "contents": contents()?,
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
        for outcome in [
            "admitted",
            "integrity-invalid",
            "specification-revision-unsupported",
            "content-invalid",
        ] {
            assert!(count(&|case| case["admission"] == outcome) > 0, "{outcome}");
        }
    }

    #[test]
    fn the_content_cases_keep_and_break_each_closed_content() {
        let vectors = committed();
        let members = vectors["members"].as_array().unwrap();
        let cases = vectors["contents"].as_array().unwrap();
        for kind in ArtifactKind::ALL {
            // Every case changes the content of the kind's admitted member.
            let base = members
                .iter()
                .find(|case| case["id"] == kind.spelling())
                .unwrap();
            assert_eq!(base["admission"], "admitted", "{kind:?}");
            let side = |admitted: bool| {
                cases
                    .iter()
                    .any(|case| case["base"] == kind.spelling() && case["admitted"] == admitted)
            };
            // A content 018 or 022 admits is any object, so nothing breaks it.
            let external = matches!(
                kind,
                ArtifactKind::GlossarySet
                    | ArtifactKind::ResourceLimitPolicy
                    | ArtifactKind::TrustPolicy
            );
            assert_eq!(side(false), !external, "{kind:?}");
            assert!(side(true) || !external, "{kind:?}");
        }
        let references = vectors["references"].as_array().unwrap();
        let named: Vec<&Value> = references
            .iter()
            .map(|reference| &reference["reference"]["kind"])
            .collect();
        assert_eq!(named, ArtifactKind::ALL.map(ArtifactKind::spelling));
    }
}
