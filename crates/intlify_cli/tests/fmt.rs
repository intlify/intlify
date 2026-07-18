// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::fs;

mod common;

use common::{json_stdout, read, run_in as run, run_stdin, temp_project_root, write};

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
}

#[test]
fn relative_stdin_filepath_is_reported_from_the_project_root() {
    let root = temp_project_root("stdin-nested-path");
    let cwd = root.join("packages/app");
    fs::create_dir_all(&cwd).expect("nested cwd should be created");

    let result = run_stdin(
        &[
            "fmt",
            "--stdin-filepath",
            "virtual/message.mf2",
            "--reporter=json",
        ],
        &cwd,
        unformatted_message(),
    );
    let output = json_stdout(&result);

    assert_eq!(result.exit_code, 0);
    assert_eq!(
        output["results"][0]["path"],
        "packages/app/virtual/message.mf2"
    );
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
}

#[test]
fn standalone_extension_lookup_accepts_ascii_case_variants() {
    let root = temp_project_root("mf2-case");
    let file = root.join("message.MF2");
    write(&file, unformatted_message());

    let result = run(&["fmt", "message.MF2"], &root);

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout, "message.MF2\n");
    assert_eq!(read(&file), formatted_message_with_lf());
}

#[test]
fn hard_link_aliases_reread_serially_in_logical_path_order() {
    let root = temp_project_root("hard-link-aliases");
    let first = root.join("a.mf2");
    let second = root.join("z.mf2");
    write(&second, unformatted_message());
    fs::hard_link(&second, &first).expect("hard link should be created");

    let result = run(&["fmt", "z.mf2", "a.mf2", "--reporter=json"], &root);
    let output = json_stdout(&result);

    assert_eq!(result.exit_code, 0);
    assert_eq!(output["summary"]["matchedFiles"], 2);
    assert_eq!(output["summary"]["formattedFiles"], 1);
    assert_eq!(output["summary"]["unchangedFiles"], 1);
    assert_eq!(output["results"][0]["path"], "a.mf2");
    assert_eq!(output["results"][0]["status"], "formatted");
    assert_eq!(output["results"][1]["path"], "z.mf2");
    assert_eq!(output["results"][1]["status"], "unchanged");
    assert_eq!(read(&second), formatted_message_with_lf());
}

#[cfg(unix)]
#[test]
fn catalog_alias_write_failure_discards_entries_and_blocks_later_aliases() {
    use std::os::unix::fs::PermissionsExt;

    let root = temp_project_root("catalog-write-failure");
    let first = root.join("a.json");
    let second = root.join("z.json");
    let source = r#"{"message":"{$value   :number}"}"#;
    write(&second, source);
    fs::hard_link(&second, &first).expect("hard link should be created");
    let mut permissions = fs::metadata(&first)
        .expect("catalog metadata")
        .permissions();
    permissions.set_mode(0o444);
    fs::set_permissions(&first, permissions).expect("catalog should become read-only");

    // Elevated users can bypass mode 0444, so this fixture cannot certify a
    // write failure in that environment.
    if fs::OpenOptions::new().write(true).open(&first).is_ok() {
        let mut permissions = fs::metadata(&first)
            .expect("writable catalog metadata")
            .permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&first, permissions)
            .expect("writable catalog permissions should be restored");
        return;
    }

    let result = run(&["fmt", "z.json", "a.json", "--reporter=json"], &root);

    let mut permissions = fs::metadata(&second)
        .expect("catalog metadata after failure")
        .permissions();
    permissions.set_mode(0o600);
    fs::set_permissions(&second, permissions).expect("catalog permissions should be restored");
    let output = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert_eq!(output["summary"]["matchedFiles"], 2);
    assert_eq!(output["summary"]["errorCount"], 2);
    assert_eq!(output["results"][0]["path"], "a.json");
    assert_eq!(output["results"][0]["status"], "error");
    assert!(output["results"][0]["entries"]
        .as_array()
        .expect("write failure entries")
        .is_empty());
    assert_eq!(
        output["results"][0]["errors"][0]["code"],
        "output_write_failed"
    );
    assert_eq!(output["results"][1]["path"], "z.json");
    assert!(output["results"][1]["entries"]
        .as_array()
        .expect("blocked entries")
        .is_empty());
    assert_eq!(
        output["results"][1]["errors"][0]["code"],
        "alias_processing_blocked"
    );
    assert_eq!(read(&second), source);
}

#[cfg(unix)]
#[test]
fn catalog_pre_write_failure_does_not_block_later_aliases() {
    let root = temp_project_root("catalog-pre-write-failure");
    let first = root.join("a.json");
    let second = root.join("z.json");
    write(&second, r#"{"message":"#);
    fs::hard_link(&second, &first).expect("hard link should be created");

    let result = run(&["fmt", "z.json", "a.json", "--reporter=json"], &root);
    let output = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert_eq!(output["summary"]["matchedFiles"], 2);
    assert_eq!(output["summary"]["errorCount"], 2);
    assert_eq!(output["results"][0]["path"], "a.json");
    assert_eq!(
        output["results"][0]["errors"][0]["code"],
        "resource_parse_failed"
    );
    assert_eq!(output["results"][1]["path"], "z.json");
    assert_eq!(
        output["results"][1]["errors"][0]["code"],
        "resource_parse_failed"
    );
}

#[cfg(unix)]
#[test]
fn broken_symlink_metadata_failure_is_a_file_result() {
    use std::os::unix::fs::symlink;

    let root = temp_project_root("broken-alias");
    symlink(root.join("missing.mf2"), root.join("broken.mf2")).expect("symlink should be created");

    let result = run(&["fmt", "broken.mf2", "--reporter=json"], &root);
    let output = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert!(output["errors"].as_array().expect("errors").is_empty());
    assert_eq!(output["results"][0]["path"], "broken.mf2");
    assert_eq!(output["results"][0]["status"], "error");
    assert_eq!(
        output["results"][0]["errors"][0]["code"],
        "input_read_failed"
    );
    assert_eq!(
        output["results"][0]["errors"][0]["details"]["reason"],
        "metadata_failed"
    );
    assert_eq!(
        output["results"][0]["errors"][0]["details"]["ioKind"],
        "not_found"
    );
}

#[test]
fn configured_catalog_inputs_are_formatted_in_place() {
    let root = temp_project_root("catalog-format");
    let catalog = root.join("locales/en.json");
    write(
        &catalog,
        r#"{"count":".input   {$count   :number}\n{{Value {$count   :number}}}"}"#,
    );
    write(
        &root.join("intlify.config.json"),
        r#"{"resources":{"catalogs":[{"include":["locales/**"]}]}}"#,
    );

    let result = run(&["fmt", "locales", "--reporter=json"], &root);
    let output = json_stdout(&result);

    assert_eq!(result.exit_code, 0);
    assert!(output["errors"].as_array().expect("errors").is_empty());
    assert_eq!(output["summary"]["formattedFiles"], 1);
    assert_eq!(output["results"][0]["path"], "locales/en.json");
    assert_eq!(output["results"][0]["status"], "formatted");
    assert_eq!(output["results"][0]["changed"], true);
    assert_eq!(
        output["results"][0]["entries"][0]["key"],
        serde_json::json!({ "path": "/count", "occurrence": 0 })
    );
    assert_eq!(
        read(&catalog),
        r#"{"count":".input {$count :number}\n{{Value {$count :number}}}"}"#
    );
}

#[test]
fn catalog_write_preserves_host_framing_and_is_idempotent() {
    let root = temp_project_root("catalog-preservation");
    let catalog = root.join("messages.json");
    let source = "\u{feff}{\r\n  \"count\" : \".input   {$count   :number}\\n{{Value {$count   :number}}}\",\r\n  \"metadata\": 1\r\n}\r\n";
    let expected = "\u{feff}{\r\n  \"count\" : \".input {$count :number}\\n{{Value {$count :number}}}\",\r\n  \"metadata\": 1\r\n}\r\n";
    write(&catalog, source);

    let first = run(&["fmt", "messages.json"], &root);

    assert_eq!(first.exit_code, 0);
    assert_eq!(first.stdout, "messages.json\n");
    assert!(first.stderr.is_empty());
    assert_eq!(read(&catalog), expected);

    let second = run(&["fmt", "messages.json", "--reporter=json"], &root);
    let output = json_stdout(&second);

    assert_eq!(second.exit_code, 0);
    assert_eq!(output["results"][0]["status"], "unchanged");
    assert_eq!(output["results"][0]["changed"], false);
    assert_eq!(read(&catalog), expected);
}

#[test]
fn catalog_formatting_uses_the_resolved_preserve_mode_with_trivia() {
    let root = temp_project_root("catalog-preserve-mode");
    let catalog = root.join("messages.json");
    write(
        &catalog,
        r#"{"message":".input {$count :number}\n\n.local $label={$count :number}\n{{{$label} items}}"}"#,
    );
    write(
        &root.join("intlify.config.json"),
        r#"{"fmt":{"mode":"preserve"}}"#,
    );

    let result = run(&["fmt", "messages.json", "--reporter=json"], &root);
    let output = json_stdout(&result);

    assert_eq!(result.exit_code, 0);
    assert_eq!(output["summary"]["mode"], "preserve");
    assert_eq!(output["results"][0]["status"], "formatted");
    assert_eq!(
        read(&catalog),
        r#"{"message":".input {$count :number}\n\n.local $label = {$count :number}\n{{{$label} items}}"}"#
    );
}

#[test]
fn bulk_discovery_without_catalog_policy_does_not_sniff_json() {
    let root = temp_project_root("catalog-bulk-no-policy");
    let standalone = root.join("message.mf2");
    let catalog = root.join("messages.json");
    write(&standalone, unformatted_message());
    let catalog_source = r#"{"message":"{$value   :number}"}"#;
    write(&catalog, catalog_source);

    let result = run(&["fmt", ".", "--reporter=json"], &root);
    let output = json_stdout(&result);

    assert_eq!(result.exit_code, 0);
    assert_eq!(output["summary"]["matchedFiles"], 1);
    assert_eq!(output["results"][0]["path"], "message.mf2");
    assert!(output["results"][0].get("entries").is_none());
    assert_eq!(read(&standalone), formatted_message_with_lf());
    assert_eq!(read(&catalog), catalog_source);
}

#[test]
fn mixed_standalone_and_catalog_results_use_exclusive_variants_in_path_order() {
    let root = temp_project_root("mixed-result-variants");
    write(&root.join("a.json"), r#"{"message":"{$value   :number}"}"#);
    write(&root.join("b.mf2"), unformatted_message());
    write(
        &root.join("intlify.config.json"),
        r#"{"resources":{"catalogs":[{"include":["a.json"]}]}}"#,
    );

    let result = run(&["fmt", ".", "--reporter=json"], &root);
    let output = json_stdout(&result);

    assert_eq!(result.exit_code, 0);
    assert_eq!(output["summary"]["matchedFiles"], 2);
    assert_eq!(output["summary"]["formattedFiles"], 2);
    assert_eq!(output["results"][0]["path"], "a.json");
    assert!(output["results"][0].get("entries").is_some());
    assert!(output["results"][0].get("diagnostics").is_none());
    assert_eq!(output["results"][1]["path"], "b.mf2");
    assert!(output["results"][1].get("entries").is_none());
    assert!(output["results"][1].get("diagnostics").is_some());
}

#[test]
fn direct_catalog_check_and_list_different_validate_without_writing() {
    let root = temp_project_root("catalog-check");
    let catalog = root.join("messages.JSON");
    let original = r#"{"count":".input   {$count   :number}\n{{Value {$count   :number}}}"}"#;
    write(&catalog, original);

    let check = run(
        &["fmt", "--check", "messages.JSON", "--reporter=json"],
        &root,
    );
    let output = json_stdout(&check);

    assert_eq!(check.exit_code, 1);
    assert_eq!(read(&catalog), original);
    assert_eq!(output["summary"]["differentFiles"], 1);
    assert_eq!(output["results"][0]["status"], "would_format");
    assert_eq!(output["results"][0]["changed"], true);
    assert!(output["results"][0].get("diagnostics").is_none());
    assert_eq!(output["results"][0]["entries"][0]["status"], "would_format");
    assert_eq!(output["results"][0]["entries"][0]["changed"], true);
    assert_eq!(output["results"][0]["entries"][0]["readOnly"], false);
    assert!(output["results"][0]["entries"][0]
        .get("displayKey")
        .is_none());
    assert!(check.stdout.contains(
        "\"results\":[{\"path\":\"messages.JSON\",\"status\":\"would_format\",\"changed\":true,\"entries\":[{\"key\":{\"path\":\"/count\",\"occurrence\":0},\"status\":\"would_format\",\"changed\":true,\"readOnly\":false,\"diagnostics\":[]}],\"errors\":[]}]"
    ));

    let list = run(
        &["fmt", "--check", "--list-different", "messages.JSON"],
        &root,
    );
    assert_eq!(list.exit_code, 1);
    assert_eq!(list.stdout, "messages.JSON\n");
    assert!(list.stderr.is_empty());
    assert_eq!(read(&catalog), original);
}

#[test]
fn catalog_diagnostics_are_nested_while_other_entries_format() {
    let source = r#"{"first":".input   {$count   :number}\n{{Value {$count   :number}}}","broken":"Hello {$name"}"#;

    let check_root = temp_project_root("catalog-diagnostic-check");
    let check_file = check_root.join("messages.json");
    write(&check_file, source);
    let check = run(
        &["fmt", "--check", "messages.json", "--reporter=json"],
        &check_root,
    );
    let check_output = json_stdout(&check);

    assert_eq!(check.exit_code, 1);
    assert_eq!(read(&check_file), source);
    assert_eq!(check_output["summary"]["differentFiles"], 1);
    assert_eq!(check_output["summary"]["diagnosticFiles"], 1);
    assert_eq!(check_output["results"][0]["status"], "would_format");
    assert!(check_output["results"][0].get("diagnostics").is_none());
    assert_eq!(
        check_output["results"][0]["entries"][0]["status"],
        "would_format"
    );
    assert_eq!(
        check_output["results"][0]["entries"][1]["status"],
        "diagnostic"
    );
    let original_diagnostic_start = check_output["results"][0]["entries"][1]["diagnostics"][0]
        ["span"]["start"]
        .as_u64()
        .expect("mapped diagnostic start");

    let write_root = temp_project_root("catalog-diagnostic-write");
    let write_file = write_root.join("messages.json");
    write(&write_file, source);
    let formatted = run(&["fmt", "messages.json", "--reporter=json"], &write_root);
    let formatted_output = json_stdout(&formatted);

    assert_eq!(formatted.exit_code, 1);
    assert_eq!(formatted_output["summary"]["formattedFiles"], 1);
    assert_eq!(formatted_output["summary"]["diagnosticFiles"], 1);
    assert_eq!(formatted_output["results"][0]["status"], "formatted");
    let candidate_diagnostic_start = formatted_output["results"][0]["entries"][1]["diagnostics"][0]
        ["span"]["start"]
        .as_u64()
        .expect("candidate-mapped diagnostic start");
    assert!(candidate_diagnostic_start < original_diagnostic_start);
    assert_eq!(
        read(&write_file),
        r#"{"first":".input {$count :number}\n{{Value {$count :number}}}","broken":"Hello {$name"}"#
    );
}

#[test]
fn catalog_stdin_formats_host_source_and_uses_nested_json_results() {
    let root = temp_project_root("catalog-stdin");
    let source = r#"{"count":".input   {$count   :number}\n{{Value {$count   :number}}}"}"#;
    let expected = r#"{"count":".input {$count :number}\n{{Value {$count :number}}}"}"#;

    let text = run_stdin(
        &["fmt", "--stdin-filepath", "virtual/messages.json"],
        &root,
        source,
    );
    assert_eq!(text.exit_code, 0);
    assert_eq!(text.stdout, expected);
    assert!(text.stderr.is_empty());

    let json = run_stdin(
        &[
            "fmt",
            "--stdin-filepath",
            "virtual/messages.json",
            "--reporter=json",
        ],
        &root,
        source,
    );
    let output = json_stdout(&json);
    assert_eq!(json.exit_code, 0);
    assert!(json.stderr.is_empty());
    assert_eq!(output["results"][0]["path"], "virtual/messages.json");
    assert_eq!(output["results"][0]["status"], "formatted");
    assert_eq!(output["results"][0]["entries"][0]["status"], "formatted");

    let check = run_stdin(
        &[
            "fmt",
            "--check",
            "--stdin-filepath",
            "virtual/messages.json",
        ],
        &root,
        source,
    );
    assert_eq!(check.exit_code, 1);
    assert_eq!(check.stdout, "virtual/messages.json\n");
    assert!(check.stderr.is_empty());
}

#[test]
fn catalog_stdin_operational_failure_emits_no_host_source() {
    let root = temp_project_root("catalog-stdin-error");

    let result = run_stdin(
        &["fmt", "--stdin-filepath", "virtual/messages.json"],
        &root,
        r#"{"message":"#,
    );

    assert_eq!(result.exit_code, 2);
    assert!(result.stdout.is_empty());
    assert!(result.stderr.contains("virtual/messages.json"));
}

#[test]
fn explicit_empty_catalog_policy_skips_file_and_passthroughs_stdin() {
    let root = temp_project_root("catalog-policy-empty");
    let source = r#"{"message":"{$value   :number}"}"#;
    let catalog = root.join("messages.json");
    write(&catalog, source);
    write(
        &root.join("intlify.config.json"),
        r#"{"resources":{"catalogs":[]}}"#,
    );

    let file = run(&["fmt", "messages.json", "--reporter=json"], &root);
    let file_output = json_stdout(&file);
    assert_eq!(file.exit_code, 0);
    assert_eq!(file_output["summary"]["matchedFiles"], 0);
    assert!(file_output["results"]
        .as_array()
        .expect("results")
        .is_empty());
    assert_eq!(read(&catalog), source);

    let stdin = run_stdin(
        &["fmt", "--stdin-filepath", "virtual/messages.json"],
        &root,
        source,
    );
    assert_eq!(stdin.exit_code, 0);
    assert_eq!(stdin.stdout, source);
    assert!(stdin.stderr.is_empty());

    let stdin_json = run_stdin(
        &[
            "fmt",
            "--stdin-filepath",
            "virtual/messages.json",
            "--reporter=json",
        ],
        &root,
        source,
    );
    let stdin_output = json_stdout(&stdin_json);
    assert_eq!(stdin_json.exit_code, 0);
    assert_eq!(stdin_output["summary"]["matchedFiles"], 0);
    assert!(stdin_output["results"]
        .as_array()
        .expect("stdin results")
        .is_empty());
}

#[test]
fn catalog_host_parse_failure_is_a_target_local_resource_error() {
    let root = temp_project_root("catalog-parse-error");
    write(&root.join("broken.json"), "{\n  \"message\":");

    let result = run(&["fmt", "broken.json", "--reporter=json"], &root);
    let output = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert!(output["errors"]
        .as_array()
        .expect("top-level errors")
        .is_empty());
    assert_eq!(output["results"][0]["status"], "error");
    assert_eq!(output["results"][0]["changed"], false);
    assert!(output["results"][0].get("diagnostics").is_none());
    assert!(output["results"][0]["entries"]
        .as_array()
        .expect("catalog entries")
        .is_empty());
    let error = &output["results"][0]["errors"][0];
    assert_eq!(error["code"], "resource_parse_failed");
    assert_eq!(error["details"]["format"], "json");
    assert_eq!(error["details"]["offset"], 14);
    assert_eq!(error["details"]["line"], 2);
    assert_eq!(error["details"]["column"], 12);
}

#[test]
fn catalog_operational_errors_override_diagnostics_without_discarding_other_results() {
    let root = temp_project_root("catalog-exit-priority");
    write(
        &root.join("a-diagnostic.json"),
        r#"{"message":"Hello {$name"}"#,
    );
    write(&root.join("z-error.json"), r#"{"message":"#);

    let result = run(
        &[
            "fmt",
            "a-diagnostic.json",
            "z-error.json",
            "--reporter=json",
        ],
        &root,
    );
    let output = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert_eq!(output["summary"]["status"], "error");
    assert_eq!(output["summary"]["matchedFiles"], 2);
    assert_eq!(output["summary"]["diagnosticFiles"], 1);
    assert!(output["summary"]["diagnosticCount"]
        .as_u64()
        .is_some_and(|count| count > 0));
    assert_eq!(output["summary"]["errorCount"], 1);
    assert_eq!(output["results"][0]["status"], "diagnostic");
    assert_eq!(output["results"][1]["status"], "error");
    assert_eq!(
        output["results"][1]["errors"][0]["code"],
        "resource_parse_failed"
    );
}

#[test]
fn catalog_utf8_failure_precedes_host_extraction() {
    let root = temp_project_root("catalog-invalid-utf8");
    fs::write(root.join("broken.json"), b"{\"message\":\"\xff\"}")
        .expect("invalid UTF-8 fixture should be written");

    let result = run(&["fmt", "broken.json", "--reporter=json"], &root);
    let output = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert!(output["errors"]
        .as_array()
        .expect("top-level errors")
        .is_empty());
    assert_eq!(output["results"][0]["status"], "error");
    assert!(output["results"][0]["entries"]
        .as_array()
        .expect("catalog entries")
        .is_empty());
    assert_eq!(
        output["results"][0]["errors"][0]["code"],
        "input_read_failed"
    );
    assert_eq!(
        output["results"][0]["errors"][0]["details"]["reason"],
        "invalid_utf8"
    );
}

#[test]
fn unsupported_catalog_entry_is_fail_complete() {
    let root = temp_project_root("catalog-entry-unsupported");
    write(&root.join("unsupported.json"), r#"{"message":"\uD800"}"#);

    let result = run(&["fmt", "unsupported.json", "--reporter=json"], &root);
    let output = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    let catalog = &output["results"][0];
    assert_eq!(catalog["status"], "error");
    assert!(catalog["entries"]
        .as_array()
        .expect("fail-complete entries")
        .is_empty());
    let error = &catalog["errors"][0];
    assert_eq!(error["code"], "resource_entry_unsupported");
    assert_eq!(error["details"]["format"], "json");
    assert_eq!(error["details"]["reason"], "message_text_unrepresentable");
    assert!(error["details"]["offset"].is_number());
    assert!(error["details"]["line"].is_number());
    assert!(error["details"]["column"].is_number());
    assert!(error["details"].get("entryKey").is_none());
}

#[test]
fn catalog_entry_identity_preserves_duplicate_occurrences_and_empty_catalogs() {
    let root = temp_project_root("catalog-identities");
    write(
        &root.join("duplicates.json"),
        r#"{"dup":"{$x   :number}","dup":"{$y   :number}","count":1}"#,
    );
    write(&root.join("empty.json"), "{}");

    let result = run(
        &["fmt", "duplicates.json", "empty.json", "--reporter=json"],
        &root,
    );
    let output = json_stdout(&result);

    assert_eq!(result.exit_code, 0);
    assert_eq!(output["summary"]["matchedFiles"], 2);
    assert_eq!(output["summary"]["formattedFiles"], 1);
    assert_eq!(output["summary"]["unchangedFiles"], 1);
    let entries = output["results"][0]["entries"]
        .as_array()
        .expect("duplicate entries");
    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries[0]["key"],
        serde_json::json!({ "path": "/dup", "occurrence": 0 })
    );
    assert_eq!(
        entries[1]["key"],
        serde_json::json!({ "path": "/dup", "occurrence": 1 })
    );
    assert_eq!(output["results"][1]["status"], "unchanged");
    assert!(output["results"][1]["entries"]
        .as_array()
        .expect("empty entries")
        .is_empty());
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
}

#[test]
fn ignored_file_is_not_read_after_classification() {
    let root = temp_project_root("ignore-before-read");
    fs::write(root.join("ignored.mf2"), [0xff]).expect("fixture should be written");
    write(
        &root.join("intlify.config.json"),
        r#"{"fmt":{"ignorePatterns":["ignored.mf2"]}}"#,
    );

    let result = run(&["fmt", "ignored.mf2", "--reporter=json"], &root);
    let output = json_stdout(&result);

    assert_eq!(result.exit_code, 0);
    assert_eq!(output["summary"]["matchedFiles"], 0);
    assert!(output["results"].as_array().expect("results").is_empty());
    assert!(output["errors"].as_array().expect("errors").is_empty());
}

#[cfg(unix)]
#[test]
fn ignored_directories_are_pruned_before_reading_entries() {
    use std::os::unix::fs::PermissionsExt;

    let root = temp_project_root("ignore-prune");
    let ignored = root.join("ignored");
    fs::create_dir_all(&ignored).expect("ignored directory should be created");
    fs::set_permissions(&ignored, fs::Permissions::from_mode(0o000))
        .expect("ignored directory permissions should be restricted");
    write(&root.join(".gitignore"), "ignored/\n");

    let result = run(&["fmt", ".", "--reporter=json"], &root);

    fs::set_permissions(&ignored, fs::Permissions::from_mode(0o700))
        .expect("ignored directory permissions should be restored");
    let json = json_stdout(&result);
    assert_eq!(result.exit_code, 0);
    assert_eq!(json["summary"]["matchedFiles"], 0);
    assert!(json["errors"].as_array().expect("errors").is_empty());
}

#[test]
fn stdin_ignore_uses_passthrough_or_zero_target_json() {
    let root = temp_project_root("stdin-ignore");
    write(
        &root.join("intlify.config.json"),
        r#"{"fmt":{"ignorePatterns":["virtual/**"]}}"#,
    );

    let original = format!("\u{feff}{}\r\n", unformatted_message());
    let text = run_stdin(
        &["fmt", "--stdin-filepath", "virtual/count.mf2"],
        &root,
        &original,
    );
    assert_eq!(text.exit_code, 0);
    assert_eq!(text.stdout.as_bytes(), original.as_bytes());
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
        &original,
    );
    let output = json_stdout(&json);
    assert_eq!(json.exit_code, 0);
    assert_eq!(output["summary"]["operation"], "stdin-check");
    assert_eq!(output["summary"]["matchedFiles"], 0);
    assert!(output["results"].as_array().expect("results").is_empty());
}

#[test]
fn ignored_stdin_still_rejects_invalid_utf8() {
    let root = temp_project_root("stdin-ignore-invalid-utf8");
    write(
        &root.join("intlify.config.json"),
        r#"{"fmt":{"ignorePatterns":["virtual/**"]}}"#,
    );

    let result = intlify_cli::run_with_stdin(
        [
            "fmt",
            "--stdin-filepath",
            "virtual/count.mf2",
            "--reporter=json",
        ],
        &*root,
        [0xff],
    );
    let output = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert_eq!(output["errors"][0]["code"], "input_read_failed");
    assert_eq!(output["errors"][0]["details"]["reason"], "invalid_utf8");
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
}
