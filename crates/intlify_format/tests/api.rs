// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use intlify_format::{
    check_format, check_snapshot, format_message, format_snapshot, FormatErrorCode, FormatMode,
    FormatOptions,
};
use ox_mf2_parser::{
    decode_snapshot, parse_message_to_snapshot, ParseOptions, SnapshotOptions,
    SnapshotSourceMetadata,
};

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
        failure.errors[0].details.get("reason").map(String::as_str),
        Some("missing_capability")
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
        failure.errors[0].details.get("reason").map(String::as_str),
        Some("missing_capability")
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
