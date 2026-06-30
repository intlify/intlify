// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use serde::Serialize;
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliError {
    pub(crate) kind: &'static str,
    pub(crate) code: &'static str,
    pub(crate) message: String,
    pub(crate) details: Option<Value>,
}

impl CliError {
    pub(crate) fn unknown_option(option: &str) -> Self {
        Self {
            kind: "input",
            code: "unknown_cli_option",
            message: format!("Unknown CLI option: {option}"),
            details: Some(json!({ "option": option })),
        }
    }

    pub(crate) fn missing_option_value(option: &'static str) -> Self {
        Self {
            kind: "input",
            code: "missing_cli_option_value",
            message: format!("Missing value for CLI option: {option}"),
            details: Some(json!({ "option": option })),
        }
    }

    pub(crate) fn duplicate_option(option: &'static str) -> Self {
        Self {
            kind: "input",
            code: "duplicate_cli_option",
            message: format!("Duplicate CLI option: {option}"),
            details: Some(json!({ "option": option })),
        }
    }

    pub(crate) fn reporter_not_supported(reporter: &str) -> Self {
        Self {
            kind: "reporter",
            code: "reporter_not_supported",
            message: format!("Reporter is not supported: {reporter}"),
            details: Some(json!({
                "reporter": reporter,
                "supportedReporters": ["text", "json"]
            })),
        }
    }

    pub(crate) fn unknown_command(command: &str) -> Self {
        Self {
            kind: "unsupported",
            code: "unknown_command",
            message: format!("Unknown command: {command}"),
            details: Some(json!({ "command": command })),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct OperationalError {
    pub(crate) kind: &'static str,
    pub(crate) code: &'static str,
    pub(crate) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) details: Option<Value>,
}

impl From<CliError> for OperationalError {
    fn from(error: CliError) -> Self {
        Self {
            kind: error.kind,
            code: error.code,
            message: error.message,
            path: None,
            details: error.details,
        }
    }
}
