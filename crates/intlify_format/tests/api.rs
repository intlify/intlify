// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use intlify_format::{check_format, format_message, FormatErrorCode, FormatMode, FormatOptions};

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
fn parser_diagnostics_block_formatting() {
    let failure = format_message("Hello, {$name", FormatOptions::default())
        .expect_err("invalid source fails");

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
