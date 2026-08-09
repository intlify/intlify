// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! End-to-end coverage for project-wide message delivery.
//!
//! These tests start with representative JSON catalogs and TypeScript message
//! references. They exercise the public CLI result contract together with
//! durable registration instead of substituting prebuilt linker or exporter
//! values.

use std::fs;
use std::path::Path;
#[cfg(not(windows))]
use std::path::PathBuf;

use serde_json::{json, Value};

mod common;

use common::{json_stdout, run_in, temp_project_root, write};

#[cfg(not(windows))]
const MANIFEST_NAME: &str = ".intlify-output-manifest.json";

fn write_project(root: &Path, targets: &Value) {
    let config = json!({
        "resources": {
            "catalogs": [{
                "scope": "app",
                "include": ["locales/*.json"],
                "locale": {
                    "from": "path",
                    "pattern": "locales/{locale}.json"
                }
            }]
        },
        "messages": {
            "locales": ["en", "ja"],
            "coverageBaseline": { "app": "en" },
            "producers": {
                "js": {
                    "include": ["src/**/*.ts"],
                    "recognizers": {
                        "t": {
                            "kind": "lookup",
                            "scope": "app",
                            "domain": "json-pointer",
                            "keySyntax": "dot-path"
                        }
                    }
                }
            },
            "delivery": { "targets": targets }
        }
    });
    write(
        &root.join("intlify.config.json"),
        &serde_json::to_string_pretty(&config).unwrap(),
    );
    write(&root.join("locales/en.json"), r#"{"title":"Title"}"#);
    write(&root.join("locales/ja.json"), r#"{"title":"タイトル"}"#);
    write(&root.join("src/app.ts"), "t('title');\n");
}

fn target(name: &str, out: &str) -> Value {
    json!({
        "name": name,
        "exporter": "esm",
        "out": out,
        "eagerLocales": ["en"]
    })
}

fn run_emit(root: &Path, options: &[&str]) -> intlify_cli::CliRunResult {
    let mut arguments = vec![
        "messages",
        "emit",
        "--config",
        "intlify.config.json",
        "--reporter=json",
    ];
    arguments.extend_from_slice(options);
    run_in(&arguments, root)
}

fn run_emit_text(root: &Path, options: &[&str]) -> intlify_cli::CliRunResult {
    let mut arguments = vec!["messages", "emit", "--config", "intlify.config.json"];
    arguments.extend_from_slice(options);
    run_in(&arguments, root)
}

#[cfg(not(windows))]
fn artifact_path_from_manifest(output_root: &Path) -> PathBuf {
    let manifest: Value = serde_json::from_slice(
        &fs::read(output_root.join(MANIFEST_NAME)).expect("manifest should be readable"),
    )
    .expect("manifest should contain JSON");
    manifest["artifacts"][0]["path"]
        .as_array()
        .expect("artifact path should be an array")
        .iter()
        .fold(output_root.to_path_buf(), |path, segment| {
            path.join(segment.as_str().expect("path segment should be a string"))
        })
}

#[cfg(not(windows))]
#[test]
fn write_then_check_reports_durable_state_and_complete_differences() {
    let root = temp_project_root("messages-emit-write-check");
    write_project(&root, &json!([target("web", "generated/messages")]));

    let written = run_emit(&root, &[]);
    let written_json = json_stdout(&written);
    assert_eq!(written.exit_code, 0);
    assert!(written.stderr.is_empty());
    assert_eq!(written_json["command"], "messages.emit");
    assert_eq!(written_json["summary"]["status"], "success");
    assert_eq!(written_json["summary"]["operation"], "write");
    assert_eq!(written_json["summary"]["selectedTargets"], 1);
    assert_eq!(written_json["summary"]["preparedTargets"], 1);
    assert!(written_json["summary"]["preparedArtifacts"]
        .as_u64()
        .is_some_and(|count| count >= 3));
    assert_eq!(written_json["results"][0]["target"], "web");
    assert_eq!(written_json["results"][0]["status"], "written");
    assert_eq!(written_json["results"][0]["outputState"], "updated");
    assert!(root
        .join("generated/messages")
        .join(MANIFEST_NAME)
        .is_file());

    let unchanged = run_emit(&root, &[]);
    let unchanged_json = json_stdout(&unchanged);
    assert_eq!(unchanged.exit_code, 0);
    assert_eq!(unchanged_json["results"][0]["status"], "unchanged");
    assert_eq!(unchanged_json["results"][0]["outputState"], "unchanged");

    let matched = run_emit(&root, &["--check"]);
    let matched_json = json_stdout(&matched);
    assert_eq!(matched.exit_code, 0);
    assert_eq!(matched_json["summary"]["operation"], "check");
    assert_eq!(matched_json["summary"]["matchedTargets"], 1);
    assert_eq!(matched_json["results"][0]["status"], "matched");
    assert_eq!(matched_json["results"][0]["differences"], json!([]));

    let output_root = root.join("generated/messages");
    let artifact = artifact_path_from_manifest(&output_root);
    write(&artifact, "changed by the test\n");

    let different = run_emit(&root, &["--check"]);
    let different_json = json_stdout(&different);
    assert_eq!(different.exit_code, 1);
    assert_eq!(different_json["summary"]["status"], "failure");
    assert_eq!(different_json["summary"]["differentTargets"], 1);
    assert_eq!(different_json["summary"]["differenceCount"], 1);
    assert_eq!(different_json["results"][0]["status"], "different");
    assert_eq!(
        different_json["results"][0]["differences"][0]["kind"],
        "artifact_changed"
    );
    assert_eq!(
        different_json["results"][0]["differences"][0]["components"],
        json!(["payload"])
    );
}

#[cfg(windows)]
#[test]
fn write_reports_missing_durable_flush_without_publishing_output() {
    let root = temp_project_root("messages-emit-windows-durable-flush");
    write_project(&root, &json!([target("web", "generated/messages")]));

    let result = run_emit(&root, &[]);
    let json = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert!(result.stderr.is_empty());
    assert_eq!(json["command"], "messages.emit");
    assert_eq!(json["summary"]["status"], "error");
    assert_eq!(json["summary"]["operation"], "write");
    assert_eq!(json["summary"]["selectedTargets"], 1);
    assert_eq!(json["summary"]["errorTargets"], 1);
    assert_eq!(json["summary"]["errorCount"], 1);
    assert_eq!(json["results"][0]["target"], "web");
    assert_eq!(json["results"][0]["status"], "error");
    assert_eq!(json["results"][0]["outputState"], "unchanged");
    assert_eq!(
        json["results"][0]["errors"][0]["code"],
        "message_output_registration_failed"
    );
    assert_eq!(
        json["results"][0]["errors"][0]["details"]["kind"],
        "unsupported_capability"
    );
    assert_eq!(
        json["results"][0]["errors"][0]["details"]["evidence"]["capability"],
        "durable_flush"
    );
    assert!(!root.join("generated/messages").exists());
}

#[test]
fn check_of_missing_output_is_a_non_operational_difference() {
    let root = temp_project_root("messages-emit-output-missing");
    write_project(&root, &json!([target("web", "generated/messages")]));

    let result = run_emit(&root, &["--check"]);
    let json = json_stdout(&result);

    assert_eq!(result.exit_code, 1);
    assert_eq!(json["summary"]["status"], "failure");
    assert_eq!(json["summary"]["errorCount"], 0);
    assert_eq!(json["results"][0]["status"], "different");
    assert_eq!(json["results"][0]["outputState"], "unchanged");
    assert_eq!(
        json["results"][0]["differences"],
        json!([{ "kind": "output_missing" }])
    );
    assert!(!root.join("generated/messages").exists());
}

#[test]
fn blocking_link_findings_prevent_every_selected_target_transaction() {
    let root = temp_project_root("messages-emit-blocked");
    write_project(
        &root,
        &json!([
            target("browser", "generated/browser"),
            target("server", "generated/server")
        ]),
    );
    write(&root.join("src/app.ts"), "t('missing');\n");

    let result = run_emit(&root, &[]);
    let json = json_stdout(&result);

    assert_eq!(result.exit_code, 1);
    assert_eq!(json["summary"]["status"], "failure");
    assert_eq!(json["summary"]["blockedTargets"], 2);
    assert_eq!(json["summary"]["preparedTargets"], 0);
    assert_eq!(json["analysis"]["generationBlocked"], true);
    assert!(json["analysis"]["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|finding| finding["kind"] == "unresolved-message" && finding["blocking"] == true));
    assert_eq!(json["results"][0]["target"], "browser");
    assert_eq!(json["results"][1]["target"], "server");
    assert!(json["results"]
        .as_array()
        .unwrap()
        .iter()
        .all(|target| target["status"] == "blocked"));
    assert!(!root.join("generated").exists());
}

#[test]
fn message_validation_is_command_analysis_and_blocks_exporters() {
    let root = temp_project_root("messages-emit-validation");
    write_project(&root, &json!([target("web", "generated/messages")]));
    write(&root.join("locales/en.json"), r#"{"title":"Hello {$name"}"#);

    let result = run_emit(&root, &[]);
    let json = json_stdout(&result);

    assert_eq!(result.exit_code, 1);
    assert_eq!(json["summary"]["status"], "failure");
    assert_eq!(json["summary"]["diagnosticCount"], 1);
    assert_eq!(json["analysis"]["generationBlocked"], true);
    assert_eq!(json["analysis"]["messageValidation"]["truncated"], false);
    assert!(json["analysis"]["messageValidation"]["diagnostics"]
        .as_array()
        .is_some_and(|diagnostics| !diagnostics.is_empty()));
    assert_eq!(json["results"][0]["status"], "blocked");
    assert!(!root.join("generated/messages").exists());
}

#[test]
fn source_contract_failure_retains_analysis_but_has_no_target_results() {
    let root = temp_project_root("messages-emit-shared-source-error");
    write_project(&root, &json!([target("web", "generated/messages")]));
    let config_path = root.join("intlify.config.json");
    let mut config: Value =
        serde_json::from_slice(&fs::read(&config_path).unwrap()).expect("config should be JSON");
    config["messages"]["producers"]["artifacts"] = json!(["artifacts/external.json"]);
    write(
        &config_path,
        &serde_json::to_string_pretty(&config).unwrap(),
    );
    write(&root.join("artifacts/external.json"), "{");

    let result = run_emit(&root, &[]);
    let json = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert_eq!(json["summary"]["status"], "error");
    assert_eq!(json["summary"]["operation"], "write");
    assert_eq!(json["summary"]["selectedTargets"], 1);
    assert_eq!(json["summary"]["preparedTargets"], 0);
    assert_eq!(json["summary"]["errorCount"], 1);
    assert_eq!(json["analysis"]["generationBlocked"], true);
    assert_eq!(json["results"], json!([]));
    assert_eq!(json["errors"][0]["code"], "message_artifact_failed");
    assert_eq!(json["errors"][0]["details"]["kind"], "invalid_artifact");
    assert_eq!(
        json["errors"][0]["details"]["evidence"]["violation"]["code"],
        "invalid_json_syntax"
    );
    assert_eq!(
        json["errors"][0]["details"]["evidence"]["violation"]["location"],
        json!({ "kind": "root" })
    );
    assert!(!root.join("generated/messages").exists());
}

#[test]
fn one_target_failure_does_not_suppress_later_disjoint_targets() {
    let root = temp_project_root("messages-emit-target-continuation");
    write_project(
        &root,
        &json!([
            target("alpha", "generated/alpha"),
            target("omega", "generated/omega")
        ]),
    );
    write(&root.join("generated/alpha/unowned.txt"), "unowned\n");

    let result = run_emit(&root, &["--check"]);
    let json = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert_eq!(json["summary"]["status"], "error");
    assert_eq!(json["summary"]["selectedTargets"], 2);
    assert_eq!(json["summary"]["errorTargets"], 1);
    assert_eq!(json["summary"]["errorCount"], 1);
    assert_eq!(json["results"][0]["target"], "alpha");
    assert_eq!(json["results"][0]["status"], "error");
    assert_eq!(json["results"][1]["target"], "omega");
    assert_eq!(json["results"][1]["status"], "different");
    assert_eq!(json["results"][1]["outputState"], "unchanged");
    assert_eq!(
        json["results"][1]["differences"],
        json!([{ "kind": "output_missing" }])
    );
    assert!(root.join("generated/alpha/unowned.txt").is_file());
    assert!(!root.join("generated/omega").exists());
}

#[test]
fn explicit_target_selection_preserves_project_wide_analysis() {
    let root = temp_project_root("messages-emit-target-selection");
    write_project(
        &root,
        &json!([
            target("browser", "generated/browser"),
            target("server", "generated/server")
        ]),
    );

    let result = run_emit(&root, &["--target", "server", "--check"]);
    let json = json_stdout(&result);

    assert_eq!(result.exit_code, 1);
    assert_eq!(json["summary"]["status"], "failure");
    assert_eq!(json["summary"]["operation"], "check");
    assert_eq!(json["summary"]["selectedTargets"], 1);
    assert_eq!(json["results"].as_array().unwrap().len(), 1);
    assert_eq!(json["results"][0]["target"], "server");
    assert_eq!(json["results"][0]["status"], "different");
    assert_eq!(json["results"][0]["outputState"], "unchanged");
    assert_eq!(
        json["results"][0]["differences"],
        json!([{ "kind": "output_missing" }])
    );
    assert!(!root.join("generated/browser").exists());
    assert!(!root.join("generated/server").exists());
}

#[test]
fn text_reporter_renders_the_same_target_state_as_the_typed_json_result() {
    let root = temp_project_root("messages-emit-text-reporter");
    write_project(&root, &json!([target("web", "generated/messages")]));

    let text = run_emit_text(&root, &["--check"]);
    assert_eq!(text.exit_code, 1);
    assert_eq!(
        text.stdout,
        "messages emit: failure\nweb: different (unchanged)\n  difference: output_missing\n"
    );
    assert!(text.stderr.is_empty());

    let json = json_stdout(&run_emit(&root, &["--check"]));
    assert_eq!(json["summary"]["status"], "failure");
    assert_eq!(json["results"][0]["target"], "web");
    assert_eq!(json["results"][0]["status"], "different");
    assert_eq!(json["results"][0]["outputState"], "unchanged");
    assert_eq!(
        json["results"][0]["differences"],
        json!([{ "kind": "output_missing" }])
    );
}

#[test]
fn emit_requires_delivery_before_inventory_and_rejects_unknown_target() {
    let root = temp_project_root("messages-emit-required-delivery");
    write(
        &root.join("intlify.config.json"),
        r#"{
          "resources": {"catalogs": [{
            "scope": "app",
            "include": ["locales/*.json"],
            "locale": {"from": "path", "pattern": "locales/{locale}.json"}
          }]},
          "messages": {"locales": ["en"]}
        }"#,
    );

    let missing = run_emit(&root, &[]);
    let missing_json = json_stdout(&missing);
    assert_eq!(missing.exit_code, 2);
    assert_eq!(missing_json["command"], "messages.emit");
    assert_eq!(missing_json["summary"]["operation"], "write");
    assert_eq!(
        missing_json["errors"][0]["code"],
        "config_validation_failed"
    );
    assert_eq!(
        missing_json["errors"][0]["details"]["reason"],
        "invalid_message_delivery"
    );
    assert_eq!(
        missing_json["errors"][0]["details"]["pointer"],
        "/messages/delivery"
    );

    write_project(&root, &json!([target("web", "generated/messages")]));
    let unknown = run_emit(&root, &["--target", "server"]);
    let unknown_json = json_stdout(&unknown);
    assert_eq!(unknown.exit_code, 2);
    assert_eq!(unknown_json["errors"][0]["code"], "invalid_cli_argument");
    assert_eq!(
        unknown_json["errors"][0]["details"]["reason"],
        "unknown_target"
    );
    assert!(!root.join("generated/messages").exists());
}

#[test]
fn json_reporter_preserves_the_closed_envelope_and_result_field_order() {
    let root = temp_project_root("messages-emit-field-order");
    write_project(&root, &json!([target("web", "generated/messages")]));

    let result = run_emit(&root, &["--check"]);
    assert_eq!(result.exit_code, 1);
    let stdout = result.stdout.as_str();

    assert_order(
        stdout,
        &["\"summary\"", "\"analysis\"", "\"results\"", "\"errors\""],
    );
    assert_order(
        stdout,
        &[
            "\"target\"",
            "\"exporter\"",
            "\"out\"",
            "\"status\"",
            "\"outputState\"",
            "\"artifactCount\"",
            "\"payloadBytes\"",
            "\"differences\"",
            "\"diagnostics\"",
            "\"errors\"",
        ],
    );
}

fn assert_order(source: &str, fields: &[&str]) {
    let mut cursor = 0;
    for field in fields {
        let offset = source[cursor..]
            .find(field)
            .unwrap_or_else(|| panic!("missing field {field}"));
        cursor += offset + field.len();
    }
}
