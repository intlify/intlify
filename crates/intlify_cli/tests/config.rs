// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use intlify_cli::config::{
    load_project_config, slash_normalize_path, ConfigSource, EmptyConfig, FormatterConfig,
    FormatterMode, LoadedProjectConfig, ProjectConfig,
};

fn temp_root(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "intlify-config-{name}-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("temp root should be created");
    root
}

fn write(path: &Path, source: &str) {
    fs::write(path, source).expect("fixture file should be written");
}

fn assert_default_config(loaded: &LoadedProjectConfig) {
    assert_eq!(
        loaded.config,
        ProjectConfig {
            fmt: FormatterConfig::default(),
            lint: EmptyConfig {}
        }
    );
}

#[test]
fn discovers_root_config_from_git_root() {
    let root = temp_root("discover");
    fs::create_dir(root.join(".git")).expect("git marker should be created");
    fs::create_dir(root.join("nested")).expect("nested directory should be created");
    write(&root.join("intlify.config.json"), r#"{"fmt":{},"lint":{}}"#);

    let loaded = load_project_config(&root.join("nested"), None).expect("config should load");

    assert_eq!(loaded.project_root, root);
    assert_eq!(loaded.source, ConfigSource::Discovered);
    assert_eq!(
        loaded.config_path,
        Some(loaded.project_root.join("intlify.config.json"))
    );
    assert_default_config(&loaded);
    let _ = fs::remove_dir_all(loaded.project_root);
}

#[test]
fn uses_default_config_when_root_config_is_absent() {
    let root = temp_root("default");

    let loaded = load_project_config(&root, None).expect("default config should load");

    assert_eq!(loaded.source, ConfigSource::Default);
    assert!(loaded.config_path.is_none());
    assert_default_config(&loaded);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn parses_json_config_and_ignores_schema_metadata() {
    let root = temp_root("json");
    let config_path = root.join("intlify.config.json");
    write(
        &config_path,
        r#"{
  "$schema": "./node_modules/@intlify/cli/schema/config.schema.json",
  "fmt": {},
  "lint": {}
}"#,
    );

    let loaded = load_project_config(&root, Some("intlify.config.json")).expect("json should load");

    assert_eq!(loaded.source, ConfigSource::Explicit);
    assert_eq!(loaded.config_path, Some(config_path));
    assert_default_config(&loaded);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn parses_jsonc_comments_and_trailing_commas() {
    let root = temp_root("jsonc");
    write(
        &root.join("intlify.config.jsonc"),
        r#"{
  // Formatter options are added in Phase 3B.
  "fmt": {},
  /*
   * Linter options are added in Phase 3C.
   */
  "lint": {},
}"#,
    );

    let loaded = load_project_config(&root, None).expect("jsonc should load");

    assert_eq!(loaded.source, ConfigSource::Discovered);
    assert_default_config(&loaded);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn rejects_json5_only_syntax_in_jsonc() {
    let root = temp_root("json5");
    write(
        &root.join("intlify.config.jsonc"),
        r#"{
  fmt: {},
  "lint": {}
}"#,
    );

    let error = load_project_config(&root, None).expect_err("json5-only syntax should fail");

    assert_eq!(error.code, "config_parse_failed");
    assert_eq!(error.path.as_deref(), Some("intlify.config.jsonc"));
    assert!(error.details.expect("parse details")["line"]
        .as_u64()
        .is_some());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn reports_root_json_jsonc_conflict() {
    let root = temp_root("conflict");
    write(&root.join("intlify.config.json"), r#"{"fmt":{},"lint":{}}"#);
    write(
        &root.join("intlify.config.jsonc"),
        r#"{"fmt":{},"lint":{}}"#,
    );

    let error = load_project_config(&root, None).expect_err("conflict should fail");

    assert_eq!(error.code, "config_conflict");
    assert!(error.path.is_none());
    let details = error.details.expect("conflict details");
    assert_eq!(details["paths"][0], "intlify.config.json");
    assert_eq!(details["paths"][1], "intlify.config.jsonc");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn explicit_config_bypasses_root_conflict() {
    let root = temp_root("explicit-bypass");
    write(
        &root.join("intlify.config.json"),
        r#"{"fmt":{"unknown":true},"lint":{}}"#,
    );
    write(
        &root.join("intlify.config.jsonc"),
        r#"{"fmt":{"unknown":true},"lint":{}}"#,
    );
    write(&root.join("fixture.json"), r#"{"fmt":{},"lint":{}}"#);

    let loaded =
        load_project_config(&root, Some("fixture.json")).expect("explicit config should load");

    assert_eq!(loaded.source, ConfigSource::Explicit);
    assert_eq!(loaded.config_path, Some(root.join("fixture.json")));
    assert_default_config(&loaded);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn explicit_missing_config_uses_not_found_before_extension_check() {
    let root = temp_root("missing");

    let error = load_project_config(&root, Some("missing.json5")).expect_err("missing should fail");

    assert_eq!(error.code, "config_not_found");
    assert_eq!(error.path.as_deref(), Some("missing.json5"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn explicit_config_outside_project_root_uses_absolute_error_path() {
    let root = temp_root("outside-root");
    let outside = temp_root("outside-config").join("missing.json");
    let outside_arg = outside.to_string_lossy().into_owned();

    let error = load_project_config(&root, Some(&outside_arg)).expect_err("missing should fail");

    assert_eq!(error.code, "config_not_found");
    assert_eq!(
        error.path.as_deref(),
        Some(slash_normalize_path(&outside).as_str())
    );
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(outside.parent().expect("outside parent"));
}

#[test]
fn existing_unsupported_extension_reports_supported_extensions() {
    let root = temp_root("extension");
    write(&root.join("config.json5"), r#"{"fmt":{},"lint":{}}"#);

    let error =
        load_project_config(&root, Some("config.json5")).expect_err("extension should fail");

    assert_eq!(error.code, "config_extension_unsupported");
    assert_eq!(error.path.as_deref(), Some("config.json5"));
    assert_eq!(
        error.details.expect("extension details")["supportedExtensions"][0],
        ".json"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn explicit_directory_path_reports_read_failed() {
    let root = temp_root("directory");
    fs::create_dir(root.join("config.json")).expect("directory config fixture should be created");

    let error = load_project_config(&root, Some("config.json")).expect_err("directory should fail");

    assert_eq!(error.code, "config_read_failed");
    assert_eq!(error.path.as_deref(), Some("config.json"));
    assert!(error.details.expect("read details")["ioKind"]
        .as_str()
        .is_some());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn validates_unknown_root_field_with_json_pointer() {
    let root = temp_root("unknown-root");
    write(
        &root.join("intlify.config.json"),
        r#"{"fmt":{},"lint":{},"unknown":true}"#,
    );

    let error = load_project_config(&root, None).expect_err("unknown root field should fail");

    assert_eq!(error.code, "config_validation_failed");
    let details = error.details.expect("validation details");
    assert_eq!(details["pointer"], "/unknown");
    assert_eq!(details["reason"], "unknown_field");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn validates_unknown_fmt_and_lint_fields() {
    for (fixture, pointer) in [
        (r#"{"fmt":{"semi":false},"lint":{}}"#, "/fmt/semi"),
        (r#"{"fmt":{},"lint":{"rule":true}}"#, "/lint/rule"),
    ] {
        let root = temp_root("unknown-section");
        write(&root.join("intlify.config.json"), fixture);

        let error =
            load_project_config(&root, None).expect_err("unknown section field should fail");
        let details = error.details.expect("validation details");

        assert_eq!(error.code, "config_validation_failed");
        assert_eq!(details["pointer"], pointer);
        assert_eq!(details["reason"], "unknown_field");
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn fmt_and_lint_sections_are_optional() {
    let root = temp_root("optional-sections");
    write(&root.join("intlify.config.json"), "{}");

    let loaded = load_project_config(&root, None).expect("empty config should use defaults");

    assert_default_config(&loaded);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn parses_formatter_config() {
    let root = temp_root("fmt-config");
    write(
        &root.join("intlify.config.json"),
        r#"{"fmt":{"mode":"preserve","ignorePatterns":["ignored/**","!ignored/keep.mf2"]}}"#,
    );

    let loaded = load_project_config(&root, None).expect("formatter config should load");

    assert_eq!(
        loaded.config.fmt,
        FormatterConfig {
            mode: FormatterMode::Preserve,
            ignore_patterns: vec!["ignored/**".to_owned(), "!ignored/keep.mf2".to_owned()]
        }
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn validates_fmt_and_lint_section_shapes() {
    for (fixture, pointer, reason) in [
        (r#"{"fmt":null}"#, "/fmt", "expected_object"),
        (r#"{"fmt":{},"lint":[]}"#, "/lint", "expected_object"),
    ] {
        let root = temp_root("required-sections");
        write(&root.join("intlify.config.json"), fixture);

        let error = load_project_config(&root, None).expect_err("invalid section should fail");
        let details = error.details.expect("validation details");

        assert_eq!(error.code, "config_validation_failed");
        assert_eq!(details["pointer"], pointer);
        assert_eq!(details["reason"], reason);
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn validates_formatter_mode_and_ignore_patterns() {
    for (fixture, pointer, reason) in [
        (
            r#"{"fmt":{"mode":"compact"}}"#,
            "/fmt/mode",
            "invalid_value",
        ),
        (
            r#"{"fmt":{"ignorePatterns":"ignored/**"}}"#,
            "/fmt/ignorePatterns",
            "expected_array",
        ),
        (
            r#"{"fmt":{"ignorePatterns":[1]}}"#,
            "/fmt/ignorePatterns/0",
            "expected_string",
        ),
        (
            r#"{"fmt":{"ignorePatterns":["["]}}"#,
            "/fmt/ignorePatterns/0",
            "invalid_ignore_pattern",
        ),
    ] {
        let root = temp_root("fmt-validation");
        write(&root.join("intlify.config.json"), fixture);

        let error = load_project_config(&root, None).expect_err("invalid fmt config should fail");
        let details = error.details.expect("validation details");

        assert_eq!(error.code, "config_validation_failed");
        assert_eq!(details["pointer"], pointer);
        assert_eq!(details["reason"], reason);
        let _ = fs::remove_dir_all(root);
    }
}

#[test]
fn parse_error_reports_line_and_column() {
    let root = temp_root("parse-position");
    write(
        &root.join("intlify.config.jsonc"),
        r#"{
  // Original file line is preserved.
  "fmt": {},
  "lint": {
    invalid
  }
}"#,
    );

    let error = load_project_config(&root, None).expect_err("parse should fail");
    let details = error.details.expect("parse details");

    assert_eq!(error.code, "config_parse_failed");
    assert!(details["line"].as_u64().expect("line") >= 5);
    assert!(details["column"].as_u64().expect("column") > 0);
    let _ = fs::remove_dir_all(root);
}
