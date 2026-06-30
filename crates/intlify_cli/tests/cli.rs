// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::path::Path;

use serde_json::Value;

const CLI_VERSION: &str = intlify_cli::version::VERSION;

fn run(args: &[&str]) -> intlify_cli::CliRunResult {
    intlify_cli::run(args.iter().copied(), Path::new("."))
}

fn json_stdout(result: &intlify_cli::CliRunResult) -> Value {
    serde_json::from_str(result.stdout.trim_end()).expect("stdout should be JSON")
}

#[test]
fn manifest_declares_cli_crate_contract() {
    let manifest = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));

    assert!(manifest.contains("name = \"intlify_cli\""));
    assert!(manifest.contains("publish = false"));
    assert!(manifest.contains("[[bin]]"));
    assert!(manifest.contains("name = \"intlify\""));
    for cli_framework in ["argh", "bpaf", "clap", "lexopt", "pico-args"] {
        assert!(!manifest.contains(cli_framework));
    }
}

#[test]
fn no_args_prints_top_level_help() {
    let result = run(&[]);

    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.starts_with("Usage: intlify [options]"));
    assert!(!result.stdout.contains("Commands:"));
    assert!(result.stderr.is_empty());
}

#[test]
fn help_prints_top_level_help() {
    for flag in ["--help", "-h"] {
        let result = run(&[flag]);

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.starts_with("Usage: intlify [options]"));
        assert!(result.stderr.is_empty());
    }
}

#[test]
fn version_prints_version_only() {
    for flag in ["--version", "-V"] {
        let result = run(&[flag]);

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, format!("{CLI_VERSION}\n"));
        assert!(result.stderr.is_empty());
    }
}

#[test]
fn help_wins_over_version_and_invalid_args() {
    let result = run(&["--help", "--version", "--unknown"]);

    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.starts_with("Usage: intlify [options]"));
    assert!(result.stderr.is_empty());
}

#[test]
fn version_wins_over_invalid_args() {
    let result = run(&["--version", "--unknown"]);

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout, format!("{CLI_VERSION}\n"));
    assert!(result.stderr.is_empty());
}

#[test]
fn help_and_version_stay_human_readable_with_json_reporter() {
    let help = run(&["--reporter=json", "--help"]);
    let version = run(&["--reporter", "json", "--version"]);

    assert_eq!(help.exit_code, 0);
    assert!(help.stdout.starts_with("Usage: intlify [options]"));
    assert!(!help.stdout.contains("schemaVersion"));
    assert!(help.stderr.is_empty());
    assert_eq!(version.exit_code, 0);
    assert_eq!(version.stdout, format!("{CLI_VERSION}\n"));
    assert!(version.stderr.is_empty());
}

#[test]
fn reporter_json_without_command_prints_human_help() {
    let result = run(&["--reporter", "json"]);

    assert_eq!(result.exit_code, 0);
    assert!(result.stdout.starts_with("Usage: intlify [options]"));
    assert!(result.stderr.is_empty());
}

#[test]
fn reserved_command_text_reporter_writes_stderr() {
    let result = run(&["fmt"]);

    assert_eq!(result.exit_code, 2);
    assert!(result.stdout.is_empty());
    assert!(result.stderr.contains("reserved"));
}

#[test]
fn reserved_command_help_prints_placeholder_help() {
    for command in ["fmt", "lint", "check", "init"] {
        let result = run(&[command, "--help"]);

        assert_eq!(result.exit_code, 0);
        assert!(result
            .stdout
            .starts_with(&format!("Usage: intlify {command}")));
        assert!(result.stdout.contains("reserved but not available"));
        assert!(result.stderr.is_empty());
    }
}

#[test]
fn reserved_command_json_envelope_uses_stdout() {
    let result = run(&["fmt", "--reporter", "json"]);
    let json = json_stdout(&result);
    let project_root = intlify_cli::config::discover_project_root(Path::new("."));
    let project_root = intlify_cli::config::slash_normalize_path(&project_root);

    assert_eq!(result.exit_code, 2);
    assert!(result.stderr.is_empty());
    assert_eq!(json["schemaVersion"], "0");
    assert_eq!(json["command"], "fmt");
    assert_eq!(json["version"], CLI_VERSION);
    assert_eq!(json["projectRoot"], project_root);
    assert_eq!(json["summary"]["status"], "error");
    assert_eq!(json["results"], Value::Array(Vec::new()));
    assert_eq!(json["errors"][0]["kind"], "unsupported");
    assert_eq!(json["errors"][0]["code"], "command_not_ready");
    assert_eq!(json["errors"][0]["details"]["phase"], "3A");
    assert_eq!(json["errors"][0]["details"]["requiredPhase"], "3B");
}

#[test]
fn reserved_command_operands_do_not_change_placeholder_behavior() {
    let result = run(&["fmt", "file.mf2", "--reporter=json"]);
    let json = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert_eq!(json["command"], "fmt");
    assert_eq!(json["errors"][0]["code"], "command_not_ready");
}

#[test]
fn reserved_command_accepts_global_options_before_and_after_command() {
    let result = run(&["--config", "intlify.config.json", "fmt", "--reporter=json"]);
    let json = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert_eq!(json["command"], "fmt");
    assert_eq!(json["errors"][0]["code"], "command_not_ready");
}

#[test]
fn input_errors_use_json_when_reporter_json_is_present() {
    let result = run(&["--unknown", "--reporter", "json"]);
    let json = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert!(result.stderr.is_empty());
    assert_eq!(json["command"], "intlify");
    assert_eq!(json["errors"][0]["kind"], "input");
    assert_eq!(json["errors"][0]["code"], "unknown_cli_option");
    assert_eq!(json["errors"][0]["details"]["option"], "--unknown");
}

#[test]
fn unsupported_short_option_is_an_input_error() {
    let result = run(&["-x", "--reporter=json"]);
    let json = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert_eq!(json["errors"][0]["code"], "unknown_cli_option");
    assert_eq!(json["errors"][0]["details"]["option"], "-x");
}

#[test]
fn end_of_options_marker_is_not_special_cased() {
    let result = run(&["--", "--reporter=json"]);
    let json = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert_eq!(json["errors"][0]["kind"], "input");
    assert_eq!(json["errors"][0]["code"], "unknown_cli_option");
    assert_eq!(json["errors"][0]["details"]["option"], "--");
}

#[test]
fn duplicate_config_is_an_input_error() {
    let result = run(&["--config", "a.json", "--config=b.json", "--reporter=json"]);
    let json = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert_eq!(json["errors"][0]["code"], "duplicate_cli_option");
    assert_eq!(json["errors"][0]["details"]["option"], "--config");
}

#[test]
fn missing_config_value_is_an_input_error() {
    let result = run(&["--config", "--reporter=json"]);
    let json = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert_eq!(json["errors"][0]["code"], "missing_cli_option_value");
    assert_eq!(json["errors"][0]["details"]["option"], "--config");
}

#[test]
fn unsupported_reporter_is_reporter_error() {
    let result = run(&["--reporter", "xml"]);

    assert_eq!(result.exit_code, 2);
    assert!(result.stdout.is_empty());
    assert!(result.stderr.contains("Reporter is not supported"));
}

#[test]
fn unsupported_reporter_details_are_fixed_in_json() {
    let result = run(&["--reporter", "xml", "--reporter=json"]);
    let json = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert_eq!(json["errors"][0]["kind"], "reporter");
    assert_eq!(json["errors"][0]["code"], "reporter_not_supported");
    assert_eq!(json["errors"][0]["details"]["reporter"], "xml");
    assert_eq!(
        json["errors"][0]["details"]["supportedReporters"][0],
        "text"
    );
    assert_eq!(
        json["errors"][0]["details"]["supportedReporters"][1],
        "json"
    );
}

#[test]
fn missing_reporter_value_is_input_error() {
    let result = run(&["--reporter", "--reporter=json"]);
    let json = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert_eq!(json["errors"][0]["code"], "missing_cli_option_value");
    assert_eq!(json["errors"][0]["details"]["option"], "--reporter");
}

#[test]
fn unknown_command_is_reported_in_details() {
    let result = run(&["foo", "--reporter=json"]);
    let json = json_stdout(&result);

    assert_eq!(result.exit_code, 2);
    assert_eq!(json["command"], "intlify");
    assert_eq!(json["errors"][0]["kind"], "unsupported");
    assert_eq!(json["errors"][0]["code"], "unknown_command");
    assert_eq!(json["errors"][0]["details"]["command"], "foo");
}

#[test]
fn reserved_command_details_match_required_phases() {
    let cases = [
        ("fmt", "3B", false),
        ("lint", "3C", false),
        ("check", "3B+3C", true),
        ("init", "3B+3C", true),
    ];

    for (command, required_phase, requires_engines) in cases {
        let result = run(&[command, "--reporter=json"]);
        let json = json_stdout(&result);
        let details = &json["errors"][0]["details"];

        assert_eq!(result.exit_code, 2);
        assert_eq!(json["command"], command);
        assert_eq!(json["errors"][0]["code"], "command_not_ready");
        assert_eq!(details["phase"], "3A");
        assert_eq!(details["requiredPhase"], required_phase);
        if requires_engines {
            assert_eq!(details["requires"][0], "fmt");
            assert_eq!(details["requires"][1], "lint");
        } else {
            assert!(details.get("requires").is_none());
        }
    }
}

#[test]
fn json_envelope_field_order_is_deterministic() {
    let result = run(&["fmt", "--reporter=json"]);
    let output = result.stdout.as_str();

    let keys = [
        "\"schemaVersion\"",
        "\"command\"",
        "\"version\"",
        "\"projectRoot\"",
        "\"summary\"",
        "\"results\"",
        "\"errors\"",
    ];
    let positions = keys
        .iter()
        .map(|key| output.find(key).expect("key should exist"))
        .collect::<Vec<_>>();

    assert!(positions.windows(2).all(|window| window[0] < window[1]));
    assert!(output.ends_with('\n'));
    assert!(!output.contains('\n') || output.matches('\n').count() == 1);
}
