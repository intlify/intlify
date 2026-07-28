// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::io::Cursor;

use intlify_contract::{
    decode_definition_artifact, decode_definition_artifact_from_reader, encode_definition_artifact,
    ArtifactContractError, ArtifactNamespace, ArtifactVersion, ArtifactViolationCode,
    ArtifactViolationLocation, CatalogKey, CatalogKeyDomain, CatalogScopeId, CatalogScopeName,
    DefinitionField, EntryReference, EntryStructuralPath, FingerprintAlgorithm, FingerprintDigest,
    InputFingerprint, LinkLimitCounter, LinkLimits, Locale, MessageDefinition,
    MessageDefinitionArtifact, MessagePayload, PortablePathSegment, PortableRelativePath,
    ProducerId, ProducerIdentity, ProducerRevision, SourceDocumentIdentity,
};
use serde_json::Value;

const SCHEMA: &str = include_str!("../schema/message-definition-artifact-v0.1.schema.json");
const CANONICAL_FILE: &[u8] = include_bytes!("../fixtures/definition/v0.1/canonical.json");
const ZERO_DEFINITIONS_FILE: &[u8] =
    include_bytes!("../fixtures/definition/v0.1/zero-definitions.json");
const NONCANONICAL_INPUT: &[u8] =
    include_bytes!("../fixtures/definition/v0.1/noncanonical-input.json");

fn path(segments: &[&str]) -> PortableRelativePath {
    PortableRelativePath::try_new(
        segments
            .iter()
            .map(|segment| PortablePathSegment::try_new(*segment).unwrap())
            .collect(),
    )
    .unwrap()
}

fn source(segments: &[&str]) -> SourceDocumentIdentity {
    SourceDocumentIdentity::new(ArtifactNamespace::Project, path(segments))
}

fn producer() -> ProducerIdentity {
    ProducerIdentity::new(
        ProducerId::try_new("dev.intlify/resource-definition").unwrap(),
        ProducerRevision::try_new("0.1.0").unwrap(),
    )
}

fn fingerprint() -> InputFingerprint {
    InputFingerprint::new(
        FingerprintAlgorithm::Blake3_256,
        FingerprintDigest::new(std::array::from_fn(|index| index as u8)),
    )
}

fn definition(
    key: &str,
    message: &str,
    structural_path: &str,
    occurrence: u32,
) -> MessageDefinition {
    MessageDefinition::try_new(
        CatalogScopeId::new(
            ArtifactNamespace::Project,
            CatalogScopeName::try_new("app").unwrap(),
        ),
        CatalogKeyDomain::JsonPointer,
        CatalogKey::try_new(CatalogKeyDomain::JsonPointer, key).unwrap(),
        Locale::try_new("en").unwrap(),
        MessagePayload::try_new(message).unwrap(),
        EntryReference::new(
            EntryStructuralPath::try_new(structural_path).unwrap(),
            occurrence,
        ),
    )
    .unwrap()
}

fn canonical_artifact() -> MessageDefinitionArtifact {
    MessageDefinitionArtifact::try_new(
        ArtifactVersion::DRAFT_V0_1,
        producer(),
        source(&["locales", "en.json"]),
        vec![path(&["resources", "en.json"])],
        fingerprint(),
        vec![
            definition("", "", "", 0),
            definition("/title", "Hello, {$name}\n", "/title", 0),
        ],
        &LinkLimits::default(),
    )
    .unwrap()
}

#[test]
fn canonical_writer_round_trips_exact_complete_definition_artifact() {
    let limits = LinkLimits::default();
    let artifact = canonical_artifact();
    let encoded = encode_definition_artifact(&artifact, &limits).unwrap();
    assert_eq!(encoded.as_ref(), canonical_payload(CANONICAL_FILE));

    let decoded = decode_definition_artifact(canonical_payload(CANONICAL_FILE), &limits).unwrap();
    assert_eq!(decoded, artifact);
    assert_eq!(decoded.definitions().len(), 2);
    assert_eq!(decoded.definitions()[0].key().as_str(), "");
    assert_eq!(decoded.definitions()[0].message().as_str(), "");
    assert_eq!(
        decoded.definitions()[1].message().as_str(),
        "Hello, {$name}\n"
    );
    let expected_decoded_bytes = decoded.producer().id().as_str().len()
        + decoded.producer().revision().as_str().len()
        + decoded
            .source()
            .path()
            .segments()
            .iter()
            .map(|segment| segment.as_str().len())
            .sum::<usize>()
        + decoded
            .logical_aliases()
            .iter()
            .flat_map(PortableRelativePath::segments)
            .map(|segment| segment.as_str().len())
            .sum::<usize>()
        + 32
        + decoded
            .definitions()
            .iter()
            .map(|definition| {
                definition.locale().as_str().len()
                    + definition.scope().name().as_str().len()
                    + definition.source().structural_path().as_str().len()
                    + definition.key().as_str().len()
                    + definition.message().as_str().len()
            })
            .sum::<usize>();
    assert_eq!(decoded.decoded_bytes(), expected_decoded_bytes as u64);
}

#[test]
fn decoder_accepts_nonsemantic_member_order_and_reader_chunking() {
    let limits = LinkLimits::default();
    let from_slice = decode_definition_artifact(NONCANONICAL_INPUT, &limits).unwrap();
    let from_reader =
        decode_definition_artifact_from_reader(&mut Cursor::new(NONCANONICAL_INPUT), &limits)
            .unwrap();
    assert_eq!(from_reader, from_slice);
    assert_eq!(from_slice.definitions(), []);
    assert_eq!(from_slice.logical_aliases(), []);
    assert_eq!(
        encode_definition_artifact(&from_slice, &limits)
            .unwrap()
            .as_ref(),
        canonical_payload(ZERO_DEFINITIONS_FILE)
    );
}

#[test]
fn direct_construction_rejects_duplicate_and_descending_aliases() {
    for (aliases, expected_code) in [
        (
            vec![path(&["locales", "en.json"])],
            ArtifactViolationCode::DuplicateValue,
        ),
        (
            vec![path(&["assets", "en.json"])],
            ArtifactViolationCode::NonCanonicalOrder,
        ),
    ] {
        let error = MessageDefinitionArtifact::try_new(
            ArtifactVersion::DRAFT_V0_1,
            producer(),
            source(&["locales", "en.json"]),
            aliases,
            fingerprint(),
            Vec::new(),
            &LinkLimits::default(),
        )
        .unwrap_err();
        let ArtifactContractError::InvalidArtifact(violation) = error else {
            panic!("expected an alias contract violation");
        };
        assert_eq!(violation.code(), expected_code);
        assert_eq!(
            violation.location(),
            &ArtifactViolationLocation::LogicalAlias {
                alias_ordinal: 0,
                segment_ordinal: None,
            }
        );
    }
}

#[test]
fn occurrence_sequences_are_contiguous_per_structural_path() {
    let error = MessageDefinitionArtifact::try_new(
        ArtifactVersion::DRAFT_V0_1,
        producer(),
        source(&["messages.json"]),
        Vec::new(),
        fingerprint(),
        vec![
            definition("/a", "first", "/same", 0),
            definition("/b", "other", "/other", 0),
            definition("/c", "skipped", "/same", 2),
        ],
        &LinkLimits::default(),
    )
    .unwrap_err();
    let ArtifactContractError::InvalidArtifact(violation) = error else {
        panic!("expected an occurrence contract violation");
    };
    assert_eq!(
        violation.code(),
        ArtifactViolationCode::DiscontinuousOccurrence
    );
    assert_eq!(
        violation.location(),
        &ArtifactViolationLocation::Definition {
            ordinal: 2,
            field: Some(DefinitionField::SourceOccurrence),
        }
    );
}

#[test]
fn decoded_accounting_and_structural_limits_match_direct_construction() {
    let limits = LinkLimits::default()
        .try_with_limit(LinkLimitCounter::MessageBytes, 5)
        .unwrap();
    let direct = MessageDefinitionArtifact::try_new(
        ArtifactVersion::DRAFT_V0_1,
        producer(),
        source(&["messages.json"]),
        Vec::new(),
        fingerprint(),
        vec![definition("/a", "123456", "/a", 0)],
        &limits,
    )
    .unwrap_err();

    let canonical = canonical_payload(CANONICAL_FILE);
    let wire = canonical
        .windows(b"Hello, {$name}\\n".len())
        .position(|window| window == b"Hello, {$name}\\n")
        .expect("canonical fixture contains its escaped message");
    let mut encoded = canonical.to_vec();
    encoded.splice(
        wire..wire + b"Hello, {$name}\\n".len(),
        b"123456".iter().copied(),
    );
    let decoded = decode_definition_artifact(&encoded, &limits).unwrap_err();
    assert_eq!(decoded, direct);
}

#[test]
fn message_payload_is_retained_without_mf2_parsing_or_formatting() {
    let exact_source = ".match {$count ->\r\n{{ intentionally unfinished";
    let artifact = MessageDefinitionArtifact::try_new(
        ArtifactVersion::DRAFT_V0_1,
        producer(),
        source(&["messages.json"]),
        Vec::new(),
        fingerprint(),
        vec![definition("", exact_source, "", 0)],
        &LinkLimits::default(),
    )
    .unwrap();
    let encoded = encode_definition_artifact(&artifact, &LinkLimits::default()).unwrap();
    let decoded =
        decode_definition_artifact(&encoded, &LinkLimits::default()).expect("payload stays opaque");
    assert_eq!(decoded.definitions()[0].message().as_str(), exact_source);
}

#[test]
fn committed_definition_schema_is_draft_2020_12_and_closed() {
    let schema: Value = serde_json::from_str(SCHEMA).expect("schema must be valid JSON");
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["properties"]["kind"]["const"], "message-definition");
    assert_eq!(schema["properties"]["logicalAliases"]["maxItems"], 4_096);
    assert_eq!(schema["properties"]["definitions"]["maxItems"], 100_000);
    assert_eq!(
        schema["$defs"]["inputFingerprint"]["properties"]["digest"]["pattern"],
        "^[0-9a-f]{64}$"
    );
    assert!(schema["$defs"]["definition"]["properties"]["key"]
        .get("minLength")
        .is_none());
    assert!(schema["$defs"]["definition"]["properties"]["message"]
        .get("minLength")
        .is_none());
}

#[test]
fn committed_schema_validates_all_definition_json_fixtures() {
    let schema: Value = serde_json::from_str(SCHEMA).expect("schema must be valid JSON");
    let validator = jsonschema::validator_for(&schema).expect("schema must compile");

    for (name, fixture) in [
        ("canonical.json", CANONICAL_FILE),
        ("zero-definitions.json", ZERO_DEFINITIONS_FILE),
        ("noncanonical-input.json", NONCANONICAL_INPUT),
    ] {
        let instance: Value =
            serde_json::from_slice(fixture).expect("fixture must be valid JSON syntax");
        let errors = validator
            .iter_errors(&instance)
            .map(|error| error.to_string())
            .collect::<Vec<_>>();
        assert!(errors.is_empty(), "{name} violates schema: {errors:#?}");
    }

    let mut uppercase_digest: Value = serde_json::from_slice(ZERO_DEFINITIONS_FILE).unwrap();
    uppercase_digest["inputFingerprint"]["digest"] = Value::String("A".repeat(64));
    assert!(
        !validator.is_valid(&uppercase_digest),
        "the schema must actively reject a malformed digest"
    );
}

fn canonical_payload(fixture_file: &[u8]) -> &[u8] {
    fixture_file
        .strip_suffix(b"\n")
        .expect("canonical fixture files have one framing LF")
}
