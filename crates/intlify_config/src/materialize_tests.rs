// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::sync::Arc;

use crate::input_limits::{
    Bound, CountRelation, InputBound, InputLimits, RawInputLimits, ValueLimits,
};
use crate::materialize::{
    materialize_file, ByteSpan, InputFailure, MaterializedDocument, NodeKind,
};

// Explicit test-owned capacity, not a product default or admitted capability.
fn limits() -> InputLimits {
    InputLimits {
        raw: RawInputLimits {
            max_file_bytes: Bound::new(1_000_000).unwrap(),
            max_parser_tokens: Bound::new(100_000).unwrap(),
        },
        value: ValueLimits {
            max_nodes: Bound::new(100_000).unwrap(),
            max_depth: Bound::new(50_000).unwrap(),
            max_collection_entries: Bound::new(100_000).unwrap(),
            max_total_string_bytes: Bound::new(1_000_000).unwrap(),
            max_single_string_bytes: Bound::new(100_000).unwrap(),
        },
    }
}

fn document(source: &str) -> MaterializedDocument {
    materialize_file(Arc::from(source.as_bytes()), limits()).unwrap()
}

fn failure(source: &[u8]) -> crate::materialize::MaterializationError {
    match materialize_file(Arc::from(source), limits()) {
        Ok(_) => panic!("invalid input was materialized"),
        Err(error) => error,
    }
}

#[test]
fn portable_numbers_use_binary64_values_and_preserve_raw_tokens() {
    for (source, expected) in [
        ("0", 0.0_f64),
        ("-0", 0.0),
        ("-0.0e+0", 0.0),
        ("1e0", 1.0),
        ("0.1", 0.1),
        ("9007199254740991", 9_007_199_254_740_991.0),
        ("-9007199254740991", -9_007_199_254_740_991.0),
        ("9007199254740991.1", 9_007_199_254_740_991.0),
        ("1e-999", 0.0),
        ("-1e-999", 0.0),
    ] {
        let doc = document(source);
        let root = doc.node(doc.root());
        let NodeKind::Number(number) = root.kind() else {
            panic!("number expected");
        };
        assert_eq!(number.get().to_bits(), expected.to_bits());
        assert_eq!(doc.raw_span(root.span()), source.as_bytes());
    }
    for source in [
        "9007199254740992",
        "-9007199254740992",
        "9007199254740991.5",
        "1e999",
        "-1e999",
        "18446744073709551615",
    ] {
        assert_eq!(
            failure(source.as_bytes()).reason,
            InputFailure::NonPortableNumber
        );
    }
}

#[test]
fn strict_json_rejects_non_json_syntax_without_a_partial_value() {
    for source in [
        "",
        " ",
        "[",
        "{",
        "[1,]",
        r#"{"x":1,}"#,
        "[1 2]",
        r#"{"x" 1}"#,
        r"{x:1}",
        "null null",
        "truefalse",
        "+1",
        ".1",
        "01",
        "-01",
        "1.",
        "1e",
        "1e+",
        "--1",
        "NaN",
        "Infinity",
        "undefined",
        "// comment\nnull",
        "/*x*/null",
        "\u{feff}null",
        "\u{a0}null",
        "\"unclosed",
        "\"line\nbreak\"",
        r#""\x00""#,
        r#""\uXX00""#,
        r#""\u000""#,
        "'single'",
        "[,,]",
    ] {
        assert_eq!(
            failure(source.as_bytes()).reason,
            InputFailure::Syntax,
            "{source:?}"
        );
    }
}

#[test]
fn invalid_utf8_and_unicode_scalar_errors_have_byte_spans() {
    let error = failure(&[b'"', 0xf0, 0x28, 0x8c, 0xbc, b'"']);
    assert_eq!(error.reason, InputFailure::InvalidUtf8);
    assert_eq!(error.span, Some(ByteSpan::new(1, 2)));
    let error = failure(&[b'"', 0xe3, 0x81]);
    assert_eq!(error.reason, InputFailure::InvalidUtf8);
    assert_eq!(error.span, Some(ByteSpan::new(1, 3)));
    for source in [
        r#""\ud800""#,
        r#""\udfff""#,
        r#""\ud800\u0041""#,
        r#"{"\ud800":0}"#,
        r#""\ud800a""#,
    ] {
        let error = failure(source.as_bytes());
        assert_eq!(error.reason, InputFailure::NonScalarString);
        assert!(error.span.is_some());
    }
}

#[test]
fn unicode_keys_are_decoded_before_scoped_duplicate_detection() {
    for source in [r#"{"x":0,"\u0078":1}"#, r#"{"😀":0,"\ud83d\ude00":1}"#] {
        let error = failure(source.as_bytes());
        assert_eq!(error.reason, InputFailure::DuplicateMember);
        let span = error.span.unwrap();
        let first = error.related_span.unwrap();
        assert!(span.start_byte() > first.start_byte());
        assert_eq!(
            source.as_bytes()[usize::try_from(span.start_byte()).unwrap()],
            b'"'
        );
        assert_eq!(
            source.as_bytes()[usize::try_from(first.start_byte()).unwrap()],
            b'"'
        );
    }
    let doc = document(r#"{"x":{"x":0},"X":1,"é":2,"e\u0301":3}"#);
    let NodeKind::Object(members) = doc.node(doc.root()).kind() else {
        panic!()
    };
    assert_eq!(members.len(), 4);
    assert_eq!(
        members.keys().map(String::as_str).collect::<Vec<_>>(),
        ["X", "e\u{301}", "x", "é"]
    );
}

#[test]
fn source_map_retains_key_value_container_and_eof_positions() {
    let source = "\r\n{\"日本語\": [ -0, \"\\u0061\" ] }\n";
    let doc = document(source);
    let root = doc.node(doc.root());
    let NodeKind::Object(members) = root.kind() else {
        panic!()
    };
    assert_eq!(
        doc.raw_span(root.span()),
        &source.as_bytes()[2..source.len() - 1]
    );
    let member = &members["日本語"];
    assert_eq!(doc.raw_span(member.key_span()), "\"日本語\"".as_bytes());
    let value = doc.node(member.value());
    assert_eq!(doc.raw_span(value.span()), b"[ -0, \"\\u0061\" ]");
    let NodeKind::Array(items) = value.kind() else {
        panic!()
    };
    assert_eq!(doc.raw_span(doc.node(items[0]).span()), b"-0");
    assert_eq!(doc.raw_span(doc.node(items[1]).span()), b"\"\\u0061\"");
    let NodeKind::String(text) = doc.node(items[1]).kind() else {
        panic!()
    };
    assert_eq!(text, "a");
    assert_eq!(failure(b"[1,").span, Some(ByteSpan::new(3, 3)));
}

#[test]
fn complete_logical_counts_are_independent_of_escaping_and_member_order() {
    for source in [
        r#"{"a":["é",null],"b":{"a":"é"}}"#,
        r#"{"b":{"\u0061":"\u00e9"},"\u0061":["\u00e9",null]}"#,
    ] {
        let counts = document(source).counts().value;
        assert_eq!(counts.nodes, 6);
        assert_eq!(counts.depth, 3);
        assert_eq!(counts.collection_entries, 5);
        assert_eq!(counts.total_string_bytes, 7);
        assert_eq!(counts.single_string_bytes, 2);
    }
}

#[test]
fn logical_limits_accept_exact_and_report_all_complete_overruns_in_fixed_order() {
    let source = br#"{"abc":["def",true]}"#;
    let mut exact = limits();
    exact.value = ValueLimits {
        max_nodes: Bound::new(4).unwrap(),
        max_depth: Bound::new(3).unwrap(),
        max_collection_entries: Bound::new(3).unwrap(),
        max_total_string_bytes: Bound::new(6).unwrap(),
        max_single_string_bytes: Bound::new(3).unwrap(),
    };
    assert!(materialize_file(Arc::from(source.as_slice()), exact).is_ok());
    let mut over = exact;
    over.value = ValueLimits {
        max_nodes: Bound::new(3).unwrap(),
        max_depth: Bound::new(2).unwrap(),
        max_collection_entries: Bound::new(2).unwrap(),
        max_total_string_bytes: Bound::new(5).unwrap(),
        max_single_string_bytes: Bound::new(2).unwrap(),
    };
    let error = materialize_file(Arc::from(source.as_slice()), over)
        .err()
        .unwrap();
    let InputFailure::ResourceLimits(violations) = error.reason else {
        panic!()
    };
    assert_eq!(
        violations.iter().map(|v| v.bound).collect::<Vec<_>>(),
        [
            InputBound::Nodes,
            InputBound::Depth,
            InputBound::CollectionEntries,
            InputBound::TotalStringBytes,
            InputBound::SingleStringBytes,
        ]
    );
    for violation in violations {
        assert_eq!(violation.actual, violation.limit.get() + 1);
        assert_eq!(violation.relation, CountRelation::Exact);
    }
}

#[test]
fn file_byte_and_token_limits_have_distinct_counting_domains() {
    let mut input_limits = limits();
    // {, key, :, [, true, ,, null, ], } = nine tokens, whitespace is not a token.
    let source = br#" {"x": [true, null]} "#;
    input_limits.raw.max_file_bytes = Bound::new(u64::try_from(source.len()).unwrap()).unwrap();
    input_limits.raw.max_parser_tokens = Bound::new(9).unwrap();
    let doc = materialize_file(Arc::from(source.as_slice()), input_limits).unwrap();
    assert_eq!(doc.counts().parser_tokens, 9);
    input_limits.raw.max_parser_tokens = Bound::new(8).unwrap();
    let error = materialize_file(Arc::from(source.as_slice()), input_limits)
        .err()
        .unwrap();
    let InputFailure::ResourceLimits(violations) = error.reason else {
        panic!()
    };
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].bound, InputBound::ParserTokens);
    assert_eq!(violations[0].actual, 9);
    assert_eq!(violations[0].relation, CountRelation::AtLeast);
    input_limits.raw.max_file_bytes = Bound::new(u64::try_from(source.len() - 1).unwrap()).unwrap();
    let error = materialize_file(Arc::from(source.as_slice()), input_limits)
        .err()
        .unwrap();
    let InputFailure::ResourceLimits(violations) = error.reason else {
        panic!()
    };
    assert_eq!(violations[0].bound, InputBound::FileBytes);
    assert_eq!(violations[0].relation, CountRelation::Exact);
}

#[test]
fn resource_bound_values_are_lossless_and_never_defaulted() {
    assert_eq!(Bound::parse("1").unwrap().get(), 1);
    assert_eq!(
        Bound::parse("18446744073709551614").unwrap().get(),
        u64::MAX - 1
    );
    for value in [
        "",
        "0",
        "01",
        "+1",
        "-1",
        " 1",
        "1 ",
        "1.0",
        "1e0",
        "١",
        "18446744073709551615",
        "18446744073709551616",
    ] {
        assert!(Bound::parse(value).is_none());
    }
    assert!(Bound::new(0).is_none());
    assert!(Bound::new(u64::MAX).is_none());
}

#[test]
fn owned_results_and_per_invocation_state_survive_failure_and_input_release() {
    let input: Arc<[u8]> = Arc::from(br#"{"x":"first"}"#.as_slice());
    let doc = materialize_file(Arc::clone(&input), limits()).unwrap();
    drop(input);
    for source in [b"[1,".as_slice(), br#"{"x":0,"x":1}"#, b"9007199254740992"] {
        let _ = failure(source);
    }
    let second = document("null");
    assert_eq!(second.counts().value.nodes, 1);
    assert_eq!(second.counts().parser_tokens, 1);
    assert_eq!(
        doc.raw_span(doc.node(doc.root()).span()),
        br#"{"x":"first"}"#
    );
}

#[test]
fn deeply_nested_input_and_failure_cleanup_do_not_recurse_on_the_rust_stack() {
    let depth = 20_000;
    let source = format!("{}null{}", "[".repeat(depth), "]".repeat(depth));
    let doc = document(&source);
    assert_eq!(doc.counts().value.depth, u64::try_from(depth + 1).unwrap());
    drop(doc);
    let invalid = format!("{}null", "[".repeat(depth));
    assert_eq!(failure(invalid.as_bytes()).reason, InputFailure::Syntax);
}

#[test]
fn error_debug_output_contains_no_rejected_key_or_value() {
    let error = failure(br#"{"private-token":0,"private-token":1}"#);
    let rendered = format!("{error:?}");
    assert!(!rendered.contains("private-token"));
    assert!(rendered.contains("DuplicateMember"));
}

fn assert_document_matches(doc: &MaterializedDocument, expected: &serde_json::Value) {
    use serde_json::Value;
    let mut pending = vec![(doc.root(), expected)];
    while let Some((id, expected)) = pending.pop() {
        let node = doc.node(id);
        match (node.kind(), expected) {
            (NodeKind::Null, Value::Null) => {}
            (NodeKind::Boolean(actual), Value::Bool(expected)) => assert_eq!(actual, expected),
            (NodeKind::Number(actual), Value::Number(expected)) => {
                let number = expected.as_f64().unwrap();
                let number = if number == 0.0 { 0.0 } else { number };
                assert_eq!(actual.get().to_bits(), number.to_bits());
            }
            (NodeKind::String(actual), Value::String(expected)) => assert_eq!(actual, expected),
            (NodeKind::Array(items), Value::Array(expected)) => {
                assert_eq!(items.len(), expected.len());
                pending.extend(items.iter().copied().zip(expected));
            }
            (NodeKind::Object(members), Value::Object(expected)) => {
                assert_eq!(members.len(), expected.len());
                for (name, member) in members {
                    let key: String =
                        serde_json::from_slice(doc.raw_span(member.key_span())).unwrap();
                    assert_eq!(&key, name);
                    let value_span = doc.node(member.value()).span();
                    assert!(member.key_span().start_byte() > node.span().start_byte());
                    assert!(member.key_span().end_byte() < value_span.start_byte());
                    assert!(value_span.end_byte() < node.span().end_byte());
                    pending.push((member.value(), &expected[name]));
                }
            }
            _ => panic!("materialized type differs from independent JSON decoder"),
        }
    }
}

#[test]
fn materialized_values_match_independent_decoder_over_finite_generated_corpus() {
    use serde_json::json;
    let atoms = [
        json!(null),
        json!(true),
        json!(false),
        json!(0),
        json!(1.5),
        json!(-2),
        json!(""),
        json!("é日本語😀"),
        json!("\0\n\r\t\u{08}\u{0c}\\\"/"),
        json!([]),
        json!({}),
    ];
    for (index, atom) in atoms.iter().enumerate() {
        for peer in &atoms {
            let value = json!({
                "": atom,
                "a/b~c": [peer, {"same": atom, "é": peer}],
                "e\u{301}": {"index": index, "same": [atom, peer]},
            });
            let compact = serde_json::to_string(&value).unwrap();
            let pretty = serde_json::to_string_pretty(&value).unwrap();
            let compact_doc = document(&compact);
            let pretty_doc = document(&pretty);
            assert_document_matches(&compact_doc, &value);
            assert_document_matches(&pretty_doc, &value);
            assert_eq!(compact_doc.counts().value, pretty_doc.counts().value);
            assert_eq!(
                compact_doc.counts().parser_tokens,
                pretty_doc.counts().parser_tokens
            );
        }
    }
}

#[test]
fn escaped_scalar_lengths_match_owned_decoding_at_unicode_boundaries() {
    for (escaped, expected) in [
        (r#""\u0000""#, "\0"),
        (r#""\u001f""#, "\u{1f}"),
        (r#""\u007f""#, "\u{7f}"),
        (r#""\u0080""#, "\u{80}"),
        (r#""\u07ff""#, "\u{7ff}"),
        (r#""\u0800""#, "\u{800}"),
        (r#""\uD7FF""#, "\u{d7ff}"),
        (r#""\ue000""#, "\u{e000}"),
        (r#""\uffff""#, "\u{ffff}"),
        (r#""\ud800\udc00""#, "\u{10000}"),
        (r#""\uDBFF\uDFFF""#, "\u{10ffff}"),
        (r#""\"\\\/\b\f\n\r\t""#, "\"\\/\u{08}\u{0c}\n\r\t"),
        (r#""\\ud800""#, "\\ud800"),
    ] {
        let doc = document(escaped);
        let NodeKind::String(actual) = doc.node(doc.root()).kind() else {
            panic!()
        };
        assert_eq!(actual, expected);
        assert_eq!(
            doc.counts().value.total_string_bytes,
            u64::try_from(expected.len()).unwrap()
        );
    }
}

#[test]
fn mutated_small_inputs_never_panic_or_admit_invalid_json() {
    for source in [
        br#"{"x":[0,"a",true,null]}"#.as_slice(),
        br#"["\u0061",{}]"#,
    ] {
        for offset in 0..source.len() {
            for byte in [
                0, b' ', b'"', b'\\', b'{', b'}', b'[', b']', b',', b':', b'0', b'x', 0xff,
            ] {
                let mut mutated = source.to_vec();
                mutated[offset] = byte;
                if let Ok(doc) = materialize_file(Arc::from(mutated.as_slice()), limits()) {
                    let expected: serde_json::Value = serde_json::from_slice(&mutated).unwrap();
                    assert_document_matches(&doc, &expected);
                }
            }
        }
    }
}

#[test]
fn logical_overruns_use_complete_totals_not_first_encountered_prefixes() {
    let mut input_limits = limits();
    input_limits.value.max_nodes = Bound::new(2).unwrap();
    input_limits.value.max_total_string_bytes = Bound::new(2).unwrap();
    let first = br#"{"a":["xx","yy"],"b":"zz"}"#;
    let second = br#"{"b":"zz","a":["xx","yy"]}"#;
    let mut reasons = Vec::new();
    for source in [first.as_slice(), second] {
        let error = materialize_file(Arc::from(source), input_limits)
            .err()
            .unwrap();
        assert_eq!(error.progress.complete_value.as_ref().unwrap().nodes, 5);
        let InputFailure::ResourceLimits(ref violations) = error.reason else {
            panic!()
        };
        assert_eq!(violations[0].actual, 5);
        assert_eq!(violations[1].actual, 8);
        assert!(violations
            .iter()
            .all(|v| v.relation == CountRelation::Exact));
        reasons.push(error.reason);
    }
    assert_eq!(reasons[0], reasons[1]);
}

#[test]
fn incomplete_raw_inputs_do_not_claim_complete_logical_counts() {
    let mut input_limits = limits();
    input_limits.raw.max_parser_tokens = Bound::new(1).unwrap();
    let error = materialize_file(Arc::from(b"[null]".as_slice()), input_limits)
        .err()
        .unwrap();
    assert_eq!(error.progress.parser_tokens_visited, 1);
    assert!(error.progress.complete_value.is_none());
    for source in [b"[1,".as_slice(), br#"{"x":1,"x":2}"#, b"null null"] {
        assert!(failure(source).progress.complete_value.is_none());
    }
}

#[test]
fn deep_object_indexes_also_cleanup_without_recursive_value_destruction() {
    let depth = 10_000;
    let source = format!("{}null{}", "{\"x\":".repeat(depth), "}".repeat(depth));
    let doc = document(&source);
    assert_eq!(doc.counts().value.depth, u64::try_from(depth + 1).unwrap());
    drop(doc);
    let invalid = format!("{}null", "{\"x\":".repeat(depth));
    assert_eq!(failure(invalid.as_bytes()).reason, InputFailure::Syntax);
}
