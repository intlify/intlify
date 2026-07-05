// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn run(args: &[&str], cwd: &Path) -> intlify_cli::CliRunResult {
    intlify_cli::run(args.iter().copied(), cwd)
}

fn run_stdin(args: &[&str], cwd: &Path, stdin: &str) -> intlify_cli::CliRunResult {
    intlify_cli::run_with_stdin(args.iter().copied(), cwd, stdin.as_bytes())
}

fn json_stdout(result: &intlify_cli::CliRunResult) -> Value {
    serde_json::from_str(result.stdout.trim_end()).expect("stdout should be JSON")
}

fn temp_project_root(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "intlify-fmt-{name}-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(root.join(".git")).expect("temp project git marker should be created");
    root
}

fn write(path: &Path, source: &str) {
    fs::create_dir_all(path.parent().expect("fixture parent"))
        .expect("fixture parent should be created");
    fs::write(path, source).expect("fixture should be written");
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).expect("fixture should be readable")
}

fn unformatted_message() -> &'static str {
    ".input   {$count   :number}\n{{Value {$count   :number}}}"
}

fn formatted_message_with_lf() -> &'static str {
    ".input {$count :number}\n{{Value {$count :number}}}\n"
}

#[test]
fn write_mode_formats_files_in_place() {
    let root = temp_project_root("write");
    let file = root.join("messages/count.mf2");
    write(&file, unformatted_message());

    let result = run(&["fmt", "messages/count.mf2"], &root);

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout, "messages/count.mf2\n");
    assert!(result.stderr.is_empty());
    assert_eq!(read(&file), formatted_message_with_lf());

    let unchanged = run(&["fmt", "messages/count.mf2"], &root);
    assert_eq!(unchanged.exit_code, 0);
    assert!(unchanged.stdout.is_empty());
    assert!(unchanged.stderr.is_empty());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn check_and_list_different_report_paths_without_writing() {
    let root = temp_project_root("check");
    let file = root.join("messages/count.mf2");
    write(&file, unformatted_message());

    let check = run(&["fmt", "--check", "."], &root);
    assert_eq!(check.exit_code, 1);
    assert_eq!(check.stdout, "messages/count.mf2\n");
    assert!(check.stderr.is_empty());
    assert_eq!(read(&file), unformatted_message());

    let list = run(&["fmt", "--check", "--list-different", "."], &root);
    assert_eq!(list.exit_code, 1);
    assert_eq!(list.stdout, "messages/count.mf2\n");
    assert!(list.stderr.is_empty());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn stdin_formatting_and_check_use_virtual_path() {
    let root = temp_project_root("stdin");

    let format = run_stdin(
        &["fmt", "--stdin-filepath", "virtual/count.mf2"],
        &root,
        unformatted_message(),
    );
    assert_eq!(format.exit_code, 0);
    assert_eq!(format.stdout, formatted_message_with_lf());
    assert!(format.stderr.is_empty());

    let check = run_stdin(
        &["fmt", "--check", "--stdin-filepath", "virtual/count.mf2"],
        &root,
        unformatted_message(),
    );
    assert_eq!(check.exit_code, 1);
    assert_eq!(check.stdout, "virtual/count.mf2\n");
    assert!(check.stderr.is_empty());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn json_reporter_reports_check_differences() {
    let root = temp_project_root("json-check");
    write(&root.join("messages/count.mf2"), unformatted_message());

    let result = run(
        &["fmt", "--check", "messages/count.mf2", "--reporter=json"],
        &root,
    );
    let json = json_stdout(&result);

    assert_eq!(result.exit_code, 1);
    assert!(result.stderr.is_empty());
    assert_eq!(json["command"], "fmt");
    assert_eq!(json["summary"]["status"], "failure");
    assert_eq!(json["summary"]["operation"], "check");
    assert_eq!(json["summary"]["mode"], "standard");
    assert_eq!(json["summary"]["matchedFiles"], 1);
    assert_eq!(json["summary"]["differentFiles"], 1);
    assert_eq!(json["results"][0]["path"], "messages/count.mf2");
    assert_eq!(json["results"][0]["status"], "would_format");
    assert_eq!(json["results"][0]["changed"], true);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mode_config_and_cli_precedence_are_reported() {
    let root = temp_project_root("mode");
    write(&root.join("messages/count.mf2"), unformatted_message());
    write(
        &root.join("intlify.config.json"),
        r#"{"fmt":{"mode":"preserve"}}"#,
    );

    let config_mode = run(
        &["fmt", "--check", "messages/count.mf2", "--reporter=json"],
        &root,
    );
    let config_json = json_stdout(&config_mode);
    assert_eq!(config_json["summary"]["mode"], "preserve");

    let cli_mode = run(
        &[
            "fmt",
            "--check",
            "--mode",
            "standard",
            "messages/count.mf2",
            "--reporter=json",
        ],
        &root,
    );
    let cli_json = json_stdout(&cli_mode);
    assert_eq!(cli_json["summary"]["mode"], "standard");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn parser_diagnostics_are_file_results() {
    let root = temp_project_root("diagnostic");
    write(&root.join("broken.mf2"), "Hello {$name");

    let result = run(&["fmt", "broken.mf2", "--reporter=json"], &root);
    let json = json_stdout(&result);

    assert_eq!(result.exit_code, 1);
    assert_eq!(json["summary"]["status"], "failure");
    assert_eq!(json["summary"]["diagnosticFiles"], 1);
    assert!(json["summary"]["diagnosticCount"].as_u64().expect("count") > 0);
    assert_eq!(json["results"][0]["status"], "diagnostic");
    assert_eq!(json["results"][0]["changed"], false);
    assert!(json["errors"].as_array().expect("errors").is_empty());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn mixed_operational_errors_continue_processing_valid_targets() {
    let root = temp_project_root("mixed");
    let valid = root.join("valid.mf2");
    write(&valid, unformatted_message());
    write(&root.join("notes.txt"), "not mf2");

    let result = run(&["fmt", "valid.mf2", "notes.txt", "--reporter=json"], &root);
    let json = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert_eq!(json["summary"]["status"], "error");
    assert_eq!(json["summary"]["formattedFiles"], 1);
    assert_eq!(json["errors"][0]["code"], "unsupported_input_file");
    assert_eq!(json["errors"][0]["path"], "notes.txt");
    assert_eq!(json["results"][0]["path"], "valid.mf2");
    assert_eq!(json["results"][0]["status"], "formatted");
    assert_eq!(read(&valid), formatted_message_with_lf());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn discovery_excludes_hidden_and_output_dirs_but_accepts_explicit_hidden_file() {
    let root = temp_project_root("discovery");
    write(&root.join("a.mf2"), unformatted_message());
    write(&root.join(".hidden.mf2"), unformatted_message());
    write(
        &root.join("node_modules/pkg/skipped.mf2"),
        unformatted_message(),
    );

    let directory = run(&["fmt", "."], &root);
    assert_eq!(directory.exit_code, 0);
    assert_eq!(directory.stdout, "a.mf2\n");
    assert_eq!(read(&root.join(".hidden.mf2")), unformatted_message());

    let explicit_hidden = run(&["fmt", ".hidden.mf2"], &root);
    assert_eq!(explicit_hidden.exit_code, 0);
    assert_eq!(explicit_hidden.stdout, ".hidden.mf2\n");
    assert_eq!(read(&root.join(".hidden.mf2")), formatted_message_with_lf());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn end_of_options_treats_dash_prefixed_paths_as_operands() {
    let root = temp_project_root("end-of-options");
    let file = root.join("--dash.mf2");
    write(&file, unformatted_message());

    let result = run(&["fmt", "--", "--dash.mf2"], &root);

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout, "--dash.mf2\n");
    assert_eq!(read(&file), formatted_message_with_lf());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn unsupported_unmatched_and_no_target_globs_are_distinct() {
    let root = temp_project_root("input-errors");
    write(&root.join("notes.txt"), "not mf2");

    let unsupported = run(&["fmt", "notes.txt", "--reporter=json"], &root);
    let unsupported_json = json_stdout(&unsupported);
    assert_eq!(unsupported.exit_code, 2);
    assert_eq!(
        unsupported_json["errors"][0]["code"],
        "unsupported_input_file"
    );

    let unmatched = run(&["fmt", "missing/**/*.mf2", "--reporter=json"], &root);
    let unmatched_json = json_stdout(&unmatched);
    assert_eq!(unmatched.exit_code, 2);
    assert_eq!(unmatched_json["errors"][0]["code"], "unmatched_input");

    let no_targets = run(&["fmt", "*.txt", "--reporter=json"], &root);
    let no_targets_json = json_stdout(&no_targets);
    assert_eq!(no_targets.exit_code, 0);
    assert_eq!(no_targets_json["summary"]["matchedFiles"], 0);
    assert!(no_targets_json["errors"]
        .as_array()
        .expect("errors")
        .is_empty());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn ignore_sources_apply_in_precedence_order() {
    let root = temp_project_root("ignore");
    write(&root.join("ignored/keep.mf2"), unformatted_message());
    write(&root.join("ignored/skip.mf2"), unformatted_message());
    write(&root.join(".gitignore"), "ignored/**\n");
    write(&root.join("custom.ignore"), "!ignored/keep.mf2\n");
    write(
        &root.join("intlify.config.json"),
        r#"{"fmt":{"ignorePatterns":["ignored/keep.mf2","!ignored/skip.mf2"]}}"#,
    );

    let result = run(
        &[
            "fmt",
            "ignored/keep.mf2",
            "ignored/skip.mf2",
            "--ignore-path",
            "custom.ignore",
        ],
        &root,
    );

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout, "ignored/skip.mf2\n");
    assert_eq!(read(&root.join("ignored/keep.mf2")), unformatted_message());
    assert_eq!(
        read(&root.join("ignored/skip.mf2")),
        formatted_message_with_lf()
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn stdin_ignore_uses_passthrough_or_zero_target_json() {
    let root = temp_project_root("stdin-ignore");
    write(
        &root.join("intlify.config.json"),
        r#"{"fmt":{"ignorePatterns":["virtual/**"]}}"#,
    );

    let text = run_stdin(
        &["fmt", "--stdin-filepath", "virtual/count.mf2"],
        &root,
        unformatted_message(),
    );
    assert_eq!(text.exit_code, 0);
    assert_eq!(text.stdout, unformatted_message());
    assert!(text.stderr.is_empty());

    let json = run_stdin(
        &[
            "fmt",
            "--check",
            "--stdin-filepath",
            "virtual/count.mf2",
            "--reporter=json",
        ],
        &root,
        unformatted_message(),
    );
    let output = json_stdout(&json);
    assert_eq!(json.exit_code, 0);
    assert_eq!(output["summary"]["operation"], "stdin-check");
    assert_eq!(output["summary"]["matchedFiles"], 0);
    assert!(output["results"].as_array().expect("results").is_empty());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn ignore_path_setup_errors_are_operational_errors() {
    let root = temp_project_root("ignore-errors");
    write(&root.join("message.mf2"), unformatted_message());
    write(&root.join("bad.ignore"), "[\n");

    let missing = run(
        &[
            "fmt",
            "message.mf2",
            "--ignore-path",
            "missing.ignore",
            "--reporter=json",
        ],
        &root,
    );
    let missing_json = json_stdout(&missing);
    assert_eq!(missing.exit_code, 2);
    assert_eq!(missing_json["errors"][0]["code"], "ignore_file_read_failed");

    let invalid = run(
        &[
            "fmt",
            "message.mf2",
            "--ignore-path",
            "bad.ignore",
            "--reporter=json",
        ],
        &root,
    );
    let invalid_json = json_stdout(&invalid);
    assert_eq!(invalid.exit_code, 2);
    assert_eq!(invalid_json["errors"][0]["code"], "invalid_ignore_pattern");
    assert_eq!(
        invalid_json["errors"][0]["details"]["source"],
        "ignore-path"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn file_framing_removes_bom_and_normalizes_final_lf() {
    let root = temp_project_root("framing");
    let file = root.join("message.mf2");
    fs::write(&file, b"\xEF\xBB\xBFHello {$name}\r\n").expect("fixture should be written");

    let result = run(&["fmt", "message.mf2"], &root);

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout, "message.mf2\n");
    assert_eq!(
        fs::read(&file).expect("fixture should be readable"),
        b"Hello {$name}\n"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn invalid_cli_combinations_are_json_reported() {
    let root = temp_project_root("invalid-cli");

    let result = run(&["fmt", "--list-different", "--reporter=json"], &root);
    let json = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert_eq!(json["summary"]["status"], "error");
    assert_eq!(json["errors"][0]["code"], "invalid_cli_argument");
    assert_eq!(json["errors"][0]["details"]["option"], "--list-different");

    let mode = run(&["fmt", "--mode", "compact", "--reporter=json"], &root);
    let mode_json = json_stdout(&mode);
    assert_eq!(mode.exit_code, 2);
    assert_eq!(mode_json["errors"][0]["code"], "invalid_cli_argument");
    assert_eq!(mode_json["errors"][0]["details"]["option"], "--mode");
    let _ = fs::remove_dir_all(root);
}
