// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use intlify_contract::{
    decode_reference_artifact, encode_reference_artifact, LinkLimits, MessageSelector,
};
use serde_json::Value;

const SCHEMA: &str = include_str!("../schema/message-reference-artifact-v0.1.schema.json");
const CANONICAL: &[u8] = include_bytes!("../fixtures/reference/v0.1/canonical.json");
const ZERO_REFERENCES: &[u8] = include_bytes!("../fixtures/reference/v0.1/zero-references.json");
const NONCANONICAL_INPUT: &[u8] =
    include_bytes!("../fixtures/reference/v0.1/noncanonical-input.json");

#[test]
fn committed_reference_schema_is_draft_2020_12_and_closed() {
    let schema: Value = serde_json::from_str(SCHEMA).expect("schema must be valid JSON");
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["properties"]["kind"]["const"], "message-reference");
    assert_eq!(schema["properties"]["references"]["maxItems"], 1_000_000);
    assert_eq!(schema["$defs"]["reference"]["additionalProperties"], false);
    let exact_key = &schema["$defs"]["selector"]["oneOf"][0]["properties"]["key"];
    assert_eq!(exact_key["type"], "string");
    assert!(exact_key.get("minLength").is_none());
    assert_eq!(
        exact_key["$comment"],
        "An empty string is valid when the selected catalog-key domain defines a root key; the artifact codec enforces domain grammar."
    );
    assert_eq!(
        schema["$defs"]["selector"]["oneOf"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
}

#[test]
fn canonical_fixtures_are_stable_complete_documents_without_a_final_newline() {
    let limits = LinkLimits::default();
    for fixture in [CANONICAL, ZERO_REFERENCES] {
        let fixture = canonical_payload(fixture);
        assert_ne!(fixture.last(), Some(&b'\n'));
        let artifact = decode_reference_artifact(fixture, &limits).unwrap();
        assert_eq!(
            encode_reference_artifact(&artifact, &limits)
                .unwrap()
                .as_ref(),
            fixture
        );
    }
}

#[test]
fn canonical_fixture_preserves_every_reference_variant_payload_and_evidence() {
    let artifact =
        decode_reference_artifact(canonical_payload(CANONICAL), &LinkLimits::default()).unwrap();
    assert_eq!(artifact.references().len(), 2);
    assert!(matches!(
        artifact.references()[0].selector(),
        MessageSelector::Exact(_)
    ));
    assert!(artifact.references()[0].origin().is_some());
    assert!(matches!(
        artifact.references()[1].selector(),
        MessageSelector::Pattern(_)
    ));
    assert_eq!(
        artifact.references()[1].reason().unwrap().as_str(),
        "bounded set\n"
    );
}

#[test]
fn noncanonical_fixture_normalizes_only_at_the_writer_boundary() {
    let limits = LinkLimits::default();
    let noncanonical = decode_reference_artifact(NONCANONICAL_INPUT, &limits).unwrap();
    let canonical = decode_reference_artifact(canonical_payload(ZERO_REFERENCES), &limits).unwrap();
    assert_eq!(noncanonical, canonical);
    assert_eq!(
        encode_reference_artifact(&noncanonical, &limits)
            .unwrap()
            .as_ref(),
        canonical_payload(ZERO_REFERENCES)
    );
}

fn canonical_payload(fixture_file: &[u8]) -> &[u8] {
    fixture_file
        .strip_suffix(b"\n")
        .expect("canonical fixture files have one framing LF")
}
