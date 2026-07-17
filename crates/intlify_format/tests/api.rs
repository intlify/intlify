// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use intlify_format::{
    check_format, check_parsed, check_snapshot, format_message, format_parsed, format_snapshot,
    FormatErrorCode, FormatMode, FormatOptions,
};
use ox_mf2_parser::{
    decode_snapshot, parse_message_to_snapshot, parse_source, CstTables, ParseOptions, ParseResult,
    SnapshotOptions, SnapshotSourceMetadata, SourceFileInput, SourceId, SourceStore,
};

fn parse_owned(source: &str, collect_trivia: bool) -> (SourceStore, ParseResult) {
    let mut sources = SourceStore::with_capacity(1);
    let source_id = sources.add(SourceFileInput {
        source,
        ..SourceFileInput::default()
    });
    let result = parse_source(
        &sources,
        source_id,
        ParseOptions {
            collect_trivia,
            ..ParseOptions::default()
        },
    )
    .expect("source parses");
    (sources, result)
}

#[test]
fn format_message_preserves_valid_input_for_foundation_phase() {
    let source = "Hello, {$name}!";
    let result = format_message(source, FormatOptions::default()).expect("format succeeds");

    assert!(!result.changed);
    assert_eq!(result.code, source);
}

#[test]
fn check_format_reports_unchanged_input() {
    let source = "Hello, {$name}!";
    let result = check_format(
        source,
        FormatOptions {
            mode: FormatMode::Preserve,
        },
    )
    .expect("check succeeds");

    assert!(!result.changed);
}

#[test]
fn standard_formatting_reports_changed_output() {
    let source = ".input   {$count   :number}\n{{Value {$count   :number}}}";
    let result = format_message(source, FormatOptions::default()).expect("format succeeds");

    assert!(result.changed);
    assert_eq!(
        result.code,
        ".input {$count :number}\n{{Value {$count :number}}}"
    );
}

#[test]
fn simple_message_pattern_text_preserves_crlf() {
    let source = "Hello\r\n{$name}";
    let result = format_message(source, FormatOptions::default()).expect("format succeeds");

    assert!(!result.changed);
    assert_eq!(result.code, source);
}

#[test]
fn parser_diagnostics_block_formatting() {
    let failure = format_message("Hello, {$name", FormatOptions::default())
        .expect_err("invalid source fails");

    assert!(!failure.diagnostics.is_empty());
    assert!(failure.errors.is_empty());
}

#[test]
fn parsed_output_and_check_match_source_and_snapshot_paths() {
    let source = ".input   {$count   :number}\n\n{{Value {$count   :number}}}";
    let (sources, result) = parse_owned(source, true);
    let snapshot = parse_message_to_snapshot(
        source,
        None,
        ParseOptions::default(),
        SnapshotOptions::default(),
    )
    .expect("snapshot writes");
    let snapshot = decode_snapshot(&snapshot.bytes).expect("snapshot decodes");

    for mode in [FormatMode::Standard, FormatMode::Preserve] {
        let options = FormatOptions { mode };
        let source_result = format_message(source, options).expect("source path succeeds");
        let parsed_result =
            format_parsed(&sources, &result, options).expect("parsed path succeeds");
        let snapshot_result =
            format_snapshot(source, snapshot, options).expect("snapshot path succeeds");

        assert_eq!(parsed_result, source_result);
        assert_eq!(snapshot_result, source_result);
        assert_eq!(
            check_parsed(&sources, &result, options).expect("parsed check succeeds"),
            check_format(source, options).expect("source check succeeds")
        );
        assert_eq!(
            check_snapshot(source, snapshot, options).expect("snapshot check succeeds"),
            check_format(source, options).expect("source check succeeds")
        );
    }
}

#[test]
fn parsed_diagnostics_match_source_backed_failure() {
    let source = "Hello, {$name";
    let (sources, result) = parse_owned(source, true);

    let parsed = format_parsed(&sources, &result, FormatOptions::default())
        .expect_err("parsed diagnostics fail");
    let source_backed =
        format_message(source, FormatOptions::default()).expect_err("source diagnostics fail");

    assert_eq!(parsed, source_backed);
    assert!(!parsed.diagnostics.is_empty());
    assert!(parsed.errors.is_empty());

    let (sources, result) = parse_owned(source, false);
    let preserve_failure = format_parsed(
        &sources,
        &result,
        FormatOptions {
            mode: FormatMode::Preserve,
        },
    )
    .expect_err("parser diagnostics precede preserve capability validation");
    assert!(!preserve_failure.diagnostics.is_empty());
    assert!(preserve_failure.errors.is_empty());
}

#[test]
fn parsed_path_uses_the_result_source_in_a_multi_source_store() {
    let source = ".input   {$count   :number}\n{{Value {$count}}}";
    let mut sources = SourceStore::with_capacity(2);
    sources.add(SourceFileInput {
        source: "unrelated",
        ..SourceFileInput::default()
    });
    let source_id = sources.add(SourceFileInput {
        source,
        ..SourceFileInput::default()
    });
    let result = parse_source(&sources, source_id, ParseOptions::default())
        .expect("second source should parse");

    let parsed = format_parsed(&sources, &result, FormatOptions::default())
        .expect("nonzero source id should format");
    let source_backed =
        format_message(source, FormatOptions::default()).expect("source path should format");

    assert_eq!(parsed, source_backed);
}

#[test]
fn parsed_preserve_mode_requires_collected_trivia() {
    let source = ".input   {$count   :number}\n{{Value {$count   :number}}}";
    let (sources, result) = parse_owned(source, false);

    let standard = format_parsed(&sources, &result, FormatOptions::default())
        .expect("standard mode accepts trivia-less artifacts");
    assert_eq!(
        standard,
        format_message(source, FormatOptions::default()).expect("source path succeeds")
    );

    let failure = format_parsed(
        &sources,
        &result,
        FormatOptions {
            mode: FormatMode::Preserve,
        },
    )
    .expect_err("preserve mode requires trivia");
    assert!(failure.diagnostics.is_empty());
    assert_eq!(failure.errors[0].code, FormatErrorCode::InternalError);
    assert_eq!(
        failure.errors[0].details.get("reason"),
        Some(&serde_json::json!("formatter_invariant_failed"))
    );
    assert_eq!(
        failure.errors[0].details.get("phase"),
        Some(&serde_json::json!("parsed_artifact_attachment"))
    );
    let check_failure = check_parsed(
        &sources,
        &result,
        FormatOptions {
            mode: FormatMode::Preserve,
        },
    )
    .expect_err("preserve check requires trivia");
    assert_eq!(check_failure, failure);
}

#[test]
fn parsed_attachment_rejects_detectable_owner_and_source_mismatches() {
    let source = ".input {$count :number}\n{{Value {$count}}}";
    let (_, result) = parse_owned(source, true);
    let mut mismatched_sources = SourceStore::with_capacity(1);
    mismatched_sources.add(SourceFileInput {
        source: "short",
        ..SourceFileInput::default()
    });

    let failure = format_parsed(&mismatched_sources, &result, FormatOptions::default())
        .expect_err("out-of-bounds attachment fails");
    assert_eq!(failure.errors[0].code, FormatErrorCode::InternalError);
    assert_eq!(
        failure.errors[0].details.get("phase"),
        Some(&serde_json::json!("parsed_artifact_attachment"))
    );

    let (sources, mut result) = parse_owned(source, true);
    result.source = SourceId::new(1);
    let failure = format_parsed(&sources, &result, FormatOptions::default())
        .expect_err("unresolved source id fails");
    assert_eq!(failure.errors[0].code, FormatErrorCode::InternalError);
    assert_eq!(
        failure.errors[0].details.get("reason"),
        Some(&serde_json::json!("formatter_invariant_failed"))
    );

    let (sources, mut result) = parse_owned(source, true);
    result.cst = CstTables::new();
    let failure = format_parsed(&sources, &result, FormatOptions::default())
        .expect_err("missing CST root fails");
    assert_eq!(failure.errors[0].code, FormatErrorCode::InternalError);

    let invalid_source = "Hello, {$name";
    let (sources, mut result) = parse_owned(invalid_source, true);
    result.diagnostics[0].location.column += 1;
    let failure = format_parsed(&sources, &result, FormatOptions::default())
        .expect_err("inconsistent materialized diagnostic fails");
    assert!(failure.diagnostics.is_empty());
    assert_eq!(failure.errors[0].code, FormatErrorCode::InternalError);
    assert_eq!(
        failure.errors[0].details.get("phase"),
        Some(&serde_json::json!("parsed_artifact_attachment"))
    );
}

#[test]
fn format_snapshot_formats_valid_standard_snapshot() {
    let source = ".input   {$count   :number}\n{{Value {$count   :number}}}";
    let snapshot = parse_message_to_snapshot(
        source,
        None,
        ParseOptions::default(),
        SnapshotOptions::default(),
    )
    .expect("snapshot writes");
    let view = decode_snapshot(&snapshot.bytes).expect("snapshot decodes");

    let result = format_snapshot(source, view, FormatOptions::default()).expect("format succeeds");

    assert!(result.changed);
    assert_eq!(
        result.code,
        ".input {$count :number}\n{{Value {$count :number}}}"
    );
}

#[test]
fn format_snapshot_formats_valid_preserve_snapshot() {
    let source = ".input   {$name   :string} {{Hello {$name}}}";
    let snapshot = parse_message_to_snapshot(
        source,
        None,
        ParseOptions::default(),
        SnapshotOptions::default(),
    )
    .expect("snapshot writes");
    let view = decode_snapshot(&snapshot.bytes).expect("snapshot decodes");

    let result = format_snapshot(
        source,
        view,
        FormatOptions {
            mode: FormatMode::Preserve,
        },
    )
    .expect("format succeeds");

    assert!(result.changed);
    assert_eq!(result.code, ".input {$name :string} {{Hello {$name}}}");
}

#[test]
fn format_snapshot_preserves_semantically_ragged_matcher_rows() {
    let source =
        ".match   $count  $gender\n0 {{One key}}\n10 female extra {{Three keys}}\n* * {{Two keys}}";
    let expected = ".match $count $gender\n0                  {{One key}}\n10  female  extra  {{Three keys}}\n*   *              {{Two keys}}";
    let snapshot = parse_message_to_snapshot(
        source,
        None,
        ParseOptions::default(),
        SnapshotOptions::default(),
    )
    .expect("snapshot writes");
    let view = decode_snapshot(&snapshot.bytes).expect("snapshot decodes");

    let result = format_snapshot(source, view, FormatOptions::default())
        .expect("semantic key arity does not block formatting");

    assert_eq!(result.code, expected);
}

#[test]
fn check_snapshot_accepts_trivia_less_standard_snapshot() {
    let source = ".input   {$count   :number}\n{{Value {$count   :number}}}";
    let snapshot = parse_message_to_snapshot(
        source,
        None,
        ParseOptions {
            collect_trivia: false,
            ..ParseOptions::default()
        },
        SnapshotOptions {
            include_trivia: false,
            ..SnapshotOptions::default()
        },
    )
    .expect("snapshot writes");
    let view = decode_snapshot(&snapshot.bytes).expect("snapshot decodes");

    let result = check_snapshot(source, view, FormatOptions::default()).expect("check succeeds");

    assert!(result.changed);
}

#[test]
fn snapshot_without_diagnostic_capability_is_rejected() {
    let source = ".input   {$count   :number}\n{{Value {$count   :number}}}";
    let snapshot = parse_message_to_snapshot(
        source,
        None,
        ParseOptions::default(),
        SnapshotOptions {
            include_diagnostics: false,
            ..SnapshotOptions::default()
        },
    )
    .expect("snapshot writes");
    let view = decode_snapshot(&snapshot.bytes).expect("snapshot decodes");

    let failure =
        format_snapshot(source, view, FormatOptions::default()).expect_err("missing proof fails");

    assert!(failure.diagnostics.is_empty());
    assert_eq!(failure.errors[0].code, FormatErrorCode::InvalidSnapshot);
    assert_eq!(
        failure.errors[0].details.get("reason"),
        Some(&serde_json::json!("missing_capability"))
    );
}

#[test]
fn preserve_snapshot_requires_trivia_capability() {
    let source = ".input   {$count   :number}\n{{Value {$count   :number}}}";
    let snapshot = parse_message_to_snapshot(
        source,
        None,
        ParseOptions {
            collect_trivia: false,
            ..ParseOptions::default()
        },
        SnapshotOptions {
            include_trivia: false,
            ..SnapshotOptions::default()
        },
    )
    .expect("snapshot writes");
    let view = decode_snapshot(&snapshot.bytes).expect("snapshot decodes");

    let failure = format_snapshot(
        source,
        view,
        FormatOptions {
            mode: FormatMode::Preserve,
        },
    )
    .expect_err("missing trivia fails");

    assert!(failure.diagnostics.is_empty());
    assert_eq!(failure.errors[0].code, FormatErrorCode::InvalidSnapshot);
    assert_eq!(
        failure.errors[0].details.get("reason"),
        Some(&serde_json::json!("missing_capability"))
    );
}

#[test]
fn snapshot_source_mismatch_is_operational_error() {
    let snapshot = parse_message_to_snapshot(
        "Hello {$name}",
        Some(SnapshotSourceMetadata::default()),
        ParseOptions::default(),
        SnapshotOptions {
            include_source_text: true,
            ..SnapshotOptions::default()
        },
    )
    .expect("snapshot writes");
    let view = decode_snapshot(&snapshot.bytes).expect("snapshot decodes");

    let failure = format_snapshot("Hello {$other}", view, FormatOptions::default())
        .expect_err("mismatch fails");

    assert!(failure.diagnostics.is_empty());
    assert_eq!(
        failure.errors[0].code,
        FormatErrorCode::SourceSnapshotMismatch
    );
}

#[test]
fn snapshot_parser_diagnostics_block_formatting() {
    let source = "Hello {$name";
    let snapshot = parse_message_to_snapshot(
        source,
        None,
        ParseOptions::default(),
        SnapshotOptions::default(),
    )
    .expect("snapshot writes");
    let view = decode_snapshot(&snapshot.bytes).expect("snapshot decodes");

    let failure =
        format_snapshot(source, view, FormatOptions::default()).expect_err("invalid source fails");

    assert!(!failure.diagnostics.is_empty());
    assert!(failure.errors.is_empty());
}

#[test]
fn error_codes_expose_stable_strings() {
    assert_eq!(
        FormatErrorCode::InvalidSnapshot.as_str(),
        "invalid_snapshot"
    );
    assert_eq!(
        FormatErrorCode::SourceSnapshotMismatch.as_str(),
        "source_snapshot_mismatch"
    );
    assert_eq!(
        FormatErrorCode::OutputWriteFailed.as_str(),
        "output_write_failed"
    );
}
