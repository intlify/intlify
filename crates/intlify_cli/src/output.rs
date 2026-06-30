// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::path::Path;

use serde::Serialize;
use serde_json::json;

use crate::command::ReservedCommand;
use crate::config::slash_normalize_path;
use crate::error::{CliError, OperationalError};
use crate::schema::OUTPUT_SCHEMA_VERSION;
use crate::version::VERSION;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reporter {
    Text,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliRunResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CliRunResult {
    pub fn success(stdout: String) -> Self {
        Self {
            exit_code: 0,
            stdout,
            stderr: String::new(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    // Field order follows this struct definition; fixture tests rely on it for
    // stable single-line JSON output consumed by tools and agents.
    schema_version: &'static str,
    command: String,
    version: &'static str,
    project_root: String,
    summary: Summary,
    results: Vec<serde_json::Value>,
    errors: Vec<OperationalError>,
}

#[derive(Debug, Serialize)]
struct Summary {
    status: &'static str,
}

pub(crate) fn render_error(
    error: CliError,
    reporter: Reporter,
    command: &str,
    project_root: &Path,
) -> CliRunResult {
    render_operational_error(error.into(), reporter, command, project_root)
}

pub(crate) fn render_reserved_command(
    command: ReservedCommand,
    reporter: Reporter,
    project_root: &Path,
) -> CliRunResult {
    let message = format!(
        "The {} command is reserved but not available in this release.",
        command.name
    );
    let mut details = json!({
        "phase": "3A",
        "requiredPhase": command.required_phase
    });

    if !command.requires.is_empty() {
        details["requires"] = json!(command.requires);
    }

    render_operational_error(
        OperationalError {
            kind: "unsupported",
            code: "command_not_ready",
            message,
            path: None,
            details: Some(details),
        },
        reporter,
        command.name,
        project_root,
    )
}

fn render_operational_error(
    error: OperationalError,
    reporter: Reporter,
    command: &str,
    project_root: &Path,
) -> CliRunResult {
    match reporter {
        Reporter::Text => CliRunResult {
            exit_code: 2,
            stdout: String::new(),
            stderr: format!("error: {}\n", error.message),
        },
        Reporter::Json => CliRunResult {
            exit_code: 2,
            stdout: format!(
                "{}\n",
                serialize_envelope(command, project_root, vec![error])
            ),
            stderr: String::new(),
        },
    }
}

fn serialize_envelope(command: &str, project_root: &Path, errors: Vec<OperationalError>) -> String {
    let envelope = Envelope {
        schema_version: OUTPUT_SCHEMA_VERSION,
        command: command.to_owned(),
        version: VERSION,
        project_root: slash_normalize_path(project_root),
        summary: Summary { status: "error" },
        results: Vec::new(),
        errors,
    };

    serde_json::to_string(&envelope).expect("serializing CLI output envelope should not fail")
}
