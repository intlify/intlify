// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Language-neutral message-analysis fixtures, run through the public API.
//!
//! Each expected result in the fixture file is written from design 016 and the
//! MF2 grammar rather than regenerated from this implementation, so a change in
//! behaviour shows up as a failing case instead of a quietly updated snapshot.

use intlify_authoring::{
    analyze_message, AnalysisWorkspace, AuthoringLimits, ByteRange, MessageFailure, MessageInput,
    Occurrence, OccurrenceRole, OwnerIdentity, OwnerKind, SourceSnapshot, VersionedIdentity,
};
use serde_json::Value;

const FIXTURE: &str = include_str!("../fixtures/phase1/message-analysis.json");

fn limits() -> AuthoringLimits {
    AuthoringLimits {
        declarations: 1024,
        message_text_bytes: 64 * 1024,
        emitted_mf2_bytes: 128 * 1024,
        extraction_segments: 4096,
        parameter_names: 64,
        parameter_name_bytes: 256,
        metadata_value_bytes: 4096,
        vocabulary_members: 256,
        projection_nodes: 4096,
        projection_depth: 32,
        diagnostics: 256,
    }
    .validate()
    .expect("satisfiable bounds")
}

fn occurrence() -> Occurrence {
    let source = SourceSnapshot::new(
        OwnerIdentity::new(OwnerKind::Application, "storefront").expect("checked project id"),
        "checkout",
        "1",
        VersionedIdentity::literal("intlify-fixture-grammar", "0"),
        4096,
        &format!("sha256:{}", "0".repeat(64)),
    )
    .expect("checked snapshot");
    Occurrence::new(
        source,
        ByteRange::new(0, 16).expect("forward range"),
        OccurrenceRole::UiLiteral,
    )
    .expect("range inside the snapshot")
}

/// Read a case's input text.
///
/// A case may spell its text as code points when the value contains a scalar
/// that JSON cannot carry directly, such as U+0000.
fn input_text(input: &Value) -> String {
    if let Some(text) = input["text"].as_str() {
        return text.to_owned();
    }
    input["textCodepoints"]
        .as_array()
        .expect("either text or textCodepoints")
        .iter()
        .map(|point| {
            let value: u32 = point
                .as_str()
                .expect("exact decimal code point")
                .parse()
                .expect("code point in range");
            char::from_u32(value).expect("Unicode scalar value")
        })
        .collect()
}

/// Build the message input a case describes.
fn select(text: &str, literal: bool) -> MessageInput<'_> {
    if literal {
        MessageInput::Literal(text)
    } else {
        MessageInput::Mf2(text)
    }
}

fn offsets(pair: &Value) -> (u64, u64) {
    let read = |index: usize| {
        pair[index]
            .as_str()
            .expect("exact decimal offset string")
            .parse::<u64>()
            .expect("offset in the u64 domain")
    };
    (read(0), read(1))
}

#[test]
fn every_fixture_case_matches_its_independent_expected_result() {
    let document: Value = serde_json::from_str(FIXTURE).expect("valid fixture document");
    assert_eq!(
        document["fixtureId"].as_str(),
        Some("intlify-authoring-phase1-message-analysis")
    );
    assert_eq!(document["fixtureRevision"].as_str(), Some("0"));

    let cases = document["cases"].as_array().expect("case list");
    assert!(!cases.is_empty(), "the fixture must exercise something");

    let limits = limits();
    let occurrence = occurrence();
    // One workspace across every case, so a reset defect in any case would be
    // visible in the next one.
    let mut workspace = AnalysisWorkspace::new();
    let mut seen = Vec::new();

    for case in cases {
        let id = case["id"].as_str().expect("case id");
        assert!(!seen.contains(&id), "duplicate case id {id}");
        seen.push(id);

        let text = input_text(&case["input"]);
        let input = match case["input"]["kind"].as_str().expect("input kind") {
            "literal" | "mf2" => select(&text, case["input"]["kind"].as_str() == Some("literal")),
            other => panic!("{id}: unknown input kind {other}"),
        };
        let expect = &case["expect"];
        let outcome = expect["outcome"].as_str().expect("expected outcome");

        let analyzed = analyze_message(input, &occurrence, &limits, &mut workspace);

        if outcome == "failed" {
            let failure = analyzed.expect_err(&format!("{id}: expected an operational failure"));
            match expect["failure"].as_str().expect("failure kind") {
                "unrepresentable-scalar" => {
                    let offset: u64 = expect["offset"]
                        .as_str()
                        .expect("offset string")
                        .parse()
                        .expect("offset in the u64 domain");
                    assert_eq!(
                        failure,
                        MessageFailure::UnrepresentableScalar { offset },
                        "{id}"
                    );
                }
                other => panic!("{id}: unknown failure kind {other}"),
            }
            continue;
        }

        let analysis =
            analyzed.unwrap_or_else(|error| panic!("{id}: unexpected failure {error:?}"));

        if let Some(expected) = expect["mf2Source"].as_str() {
            assert_eq!(analysis.mf2_source(), expected, "{id}: MF2 source");
        }

        // Every emitted byte is covered exactly once, in order.
        let mut covered = 0_u64;
        for segment in analysis.extraction_map() {
            assert_eq!(segment.extracted().start(), covered, "{id}: segment order");
            covered = segment.extracted().end();
        }
        assert_eq!(
            covered,
            analysis.mf2_source().len() as u64,
            "{id}: segments must cover the emitted message"
        );

        if let Some(expected) = expect["segments"].as_array() {
            let actual: Vec<(u64, u64, u64, u64)> = analysis
                .extraction_map()
                .iter()
                .map(|segment| {
                    (
                        segment.extracted().start(),
                        segment.extracted().end(),
                        segment.source().start(),
                        segment.source().end(),
                    )
                })
                .collect();
            let wanted: Vec<(u64, u64, u64, u64)> = expected
                .iter()
                .map(|segment| {
                    let (extracted_start, extracted_end) = offsets(&segment["extracted"]);
                    let (source_start, source_end) = offsets(&segment["source"]);
                    (extracted_start, extracted_end, source_start, source_end)
                })
                .collect();
            assert_eq!(actual, wanted, "{id}: extraction segments");
        }

        match outcome {
            "understood" => {
                let facts = analysis
                    .facts()
                    .unwrap_or_else(|| panic!("{id}: expected parser-validated facts"));
                assert!(
                    !analysis.is_blocked(),
                    "{id}: unexpected blocking diagnostic"
                );
                if let Some(expected) = expect["parameters"].as_array() {
                    let wanted: Vec<&str> = expected
                        .iter()
                        .map(|name| name.as_str().expect("parameter name"))
                        .collect();
                    assert_eq!(facts.parameters(), wanted, "{id}: external parameters");
                }
            }
            "blocked" => {
                assert!(
                    analysis.facts().is_none(),
                    "{id}: a blocked message must not carry facts"
                );
                assert!(
                    analysis.is_blocked(),
                    "{id}: expected a blocking diagnostic"
                );
                let codes: Vec<&str> = analysis
                    .diagnostics()
                    .iter()
                    .map(|record| record.origin().code())
                    .collect();
                for expected in expect["diagnosticCodes"]
                    .as_array()
                    .expect("expected diagnostic codes")
                {
                    let code = expected.as_str().expect("diagnostic code");
                    assert!(codes.contains(&code), "{id}: missing {code} in {codes:?}");
                }
                assert!(
                    codes.iter().all(|code| !code.starts_with("authoring-")),
                    "{id}: parser-owned codes must keep their owner: {codes:?}"
                );
            }
            other => panic!("{id}: unknown outcome {other}"),
        }
    }
}

#[test]
fn a_reused_workspace_agrees_with_fresh_ones_across_the_whole_fixture() {
    let document: Value = serde_json::from_str(FIXTURE).expect("valid fixture document");
    let cases = document["cases"].as_array().expect("case list");
    let limits = limits();
    let occurrence = occurrence();

    let inputs: Vec<(String, bool)> = cases
        .iter()
        .map(|case| {
            (
                input_text(&case["input"]),
                case["input"]["kind"].as_str() == Some("literal"),
            )
        })
        .collect();
    let fresh: Vec<_> = inputs
        .iter()
        .map(|(text, literal)| {
            let mut workspace = AnalysisWorkspace::new();
            analyze_message(select(text, *literal), &occurrence, &limits, &mut workspace)
        })
        .collect();

    let mut shared = AnalysisWorkspace::new();
    let reused: Vec<_> = inputs
        .iter()
        .map(|(text, literal)| {
            analyze_message(select(text, *literal), &occurrence, &limits, &mut shared)
        })
        .collect();

    assert_eq!(fresh, reused);

    // Reversing the order must not change any individual result either.
    let mut backwards = AnalysisWorkspace::new();
    let mut reversed: Vec<_> = inputs
        .iter()
        .rev()
        .map(|(text, literal)| {
            analyze_message(select(text, *literal), &occurrence, &limits, &mut backwards)
        })
        .collect();
    reversed.reverse();
    assert_eq!(fresh, reversed);
}
