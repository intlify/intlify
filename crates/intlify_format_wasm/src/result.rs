// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use intlify_format::{FormatCheckResult, FormatFailure, FormatResult, OperationalError};
use ox_mf2_parser::Diagnostic;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WasmSpan {
    start: u32,
    end: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WasmSourceLocation {
    line: u32,
    column: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WasmDiagnosticLabel {
    source_id: u32,
    span: WasmSpan,
    message: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WasmDiagnostic {
    root_id: u32,
    source_id: u32,
    severity: u32,
    code: u32,
    message: Option<String>,
    span: WasmSpan,
    location: Option<WasmSourceLocation>,
    labels: Vec<WasmDiagnosticLabel>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WasmOperationalErrorDetail {
    key: String,
    value_json: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WasmOperationalError {
    kind: String,
    code: String,
    message: String,
    path: Option<String>,
    details: Vec<WasmOperationalErrorDetail>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WasmNativeFormatResult {
    ok: bool,
    code: Option<String>,
    changed: Option<bool>,
    diagnostics: Vec<WasmDiagnostic>,
    errors: Vec<WasmOperationalError>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WasmNativeFormatCheckResult {
    ok: bool,
    changed: Option<bool>,
    diagnostics: Vec<WasmDiagnostic>,
    errors: Vec<WasmOperationalError>,
}

impl WasmNativeFormatResult {
    pub(crate) fn from_format_result(result: FormatResult) -> Self {
        match result {
            Ok(success) => Self {
                ok: true,
                code: Some(success.code),
                changed: Some(success.changed),
                diagnostics: Vec::new(),
                errors: Vec::new(),
            },
            Err(failure) => Self::from_failure(failure),
        }
    }

    pub(crate) fn from_operational_error(error: OperationalError) -> Self {
        Self::from_failure(FormatFailure::from_error(error))
    }

    fn from_failure(failure: FormatFailure) -> Self {
        Self {
            ok: false,
            code: None,
            changed: None,
            diagnostics: diagnostics_to_wasm(failure.diagnostics),
            errors: failure.errors.into_iter().map(error_to_wasm).collect(),
        }
    }
}

impl WasmNativeFormatCheckResult {
    pub(crate) fn from_check_result(result: FormatCheckResult) -> Self {
        match result {
            Ok(success) => Self {
                ok: true,
                changed: Some(success.changed),
                diagnostics: Vec::new(),
                errors: Vec::new(),
            },
            Err(failure) => Self::from_failure(failure),
        }
    }

    pub(crate) fn from_operational_error(error: OperationalError) -> Self {
        Self::from_failure(FormatFailure::from_error(error))
    }

    fn from_failure(failure: FormatFailure) -> Self {
        Self {
            ok: false,
            changed: None,
            diagnostics: diagnostics_to_wasm(failure.diagnostics),
            errors: failure.errors.into_iter().map(error_to_wasm).collect(),
        }
    }
}

fn diagnostics_to_wasm(diagnostics: Vec<Diagnostic>) -> Vec<WasmDiagnostic> {
    diagnostics.into_iter().map(diagnostic_to_wasm).collect()
}

fn diagnostic_to_wasm(diagnostic: Diagnostic) -> WasmDiagnostic {
    WasmDiagnostic {
        root_id: 0,
        source_id: diagnostic.source.raw(),
        severity: diagnostic.severity as u32,
        code: u32::from(diagnostic.code.as_u16()),
        message: Some(diagnostic.message.to_string()),
        span: WasmSpan {
            start: diagnostic.span.start,
            end: diagnostic.span.end,
        },
        location: Some(WasmSourceLocation {
            line: diagnostic.location.line,
            column: diagnostic.location.column,
        }),
        labels: diagnostic
            .labels
            .into_iter()
            .map(|label| WasmDiagnosticLabel {
                source_id: label.source.raw(),
                span: WasmSpan {
                    start: label.span.start,
                    end: label.span.end,
                },
                message: Some(label.message.to_string()),
            })
            .collect(),
    }
}

fn error_to_wasm(error: OperationalError) -> WasmOperationalError {
    WasmOperationalError {
        kind: error.kind.as_str().to_string(),
        code: error.code.as_str().to_string(),
        message: error.message,
        path: error.path,
        details: error
            .details
            .into_iter()
            .map(|(key, value)| WasmOperationalErrorDetail {
                key,
                value_json: value.to_string(),
            })
            .collect(),
    }
}
