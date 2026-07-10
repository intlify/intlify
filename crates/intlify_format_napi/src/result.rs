// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use intlify_format::{FormatCheckResult, FormatFailure, FormatResult, OperationalError};
use napi_derive::napi;
use ox_mf2_parser::Diagnostic;

#[napi(object)]
pub struct JsSpan {
    pub start: u32,
    pub end: u32,
}

#[napi(object)]
pub struct JsSourceLocation {
    pub line: u32,
    pub column: u32,
}

#[napi(object)]
pub struct JsDiagnosticLabel {
    pub source_id: u32,
    pub span: JsSpan,
    pub message: Option<String>,
}

#[napi(object)]
pub struct JsDiagnostic {
    pub root_id: u32,
    pub source_id: u32,
    pub severity: u32,
    pub code: u32,
    pub message: Option<String>,
    pub span: JsSpan,
    pub location: Option<JsSourceLocation>,
    pub labels: Vec<JsDiagnosticLabel>,
}

#[napi(object)]
pub struct JsOperationalErrorDetail {
    pub key: String,
    pub value_json: String,
}

#[napi(object)]
pub struct JsOperationalError {
    pub kind: String,
    pub code: String,
    pub message: String,
    pub path: Option<String>,
    pub details: Vec<JsOperationalErrorDetail>,
}

#[napi(object)]
pub struct JsNativeFormatResult {
    pub ok: bool,
    pub code: Option<String>,
    pub changed: Option<bool>,
    pub diagnostics: Vec<JsDiagnostic>,
    pub errors: Vec<JsOperationalError>,
}

#[napi(object)]
pub struct JsNativeFormatCheckResult {
    pub ok: bool,
    pub changed: Option<bool>,
    pub diagnostics: Vec<JsDiagnostic>,
    pub errors: Vec<JsOperationalError>,
}

impl JsNativeFormatResult {
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
            diagnostics: diagnostics_to_js(failure.diagnostics),
            errors: failure.errors.into_iter().map(error_to_js).collect(),
        }
    }
}

impl JsNativeFormatCheckResult {
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
            diagnostics: diagnostics_to_js(failure.diagnostics),
            errors: failure.errors.into_iter().map(error_to_js).collect(),
        }
    }
}

fn diagnostics_to_js(diagnostics: Vec<Diagnostic>) -> Vec<JsDiagnostic> {
    diagnostics.into_iter().map(diagnostic_to_js).collect()
}

fn diagnostic_to_js(diagnostic: Diagnostic) -> JsDiagnostic {
    JsDiagnostic {
        root_id: 0,
        source_id: diagnostic.source.raw(),
        severity: diagnostic.severity as u32,
        code: u32::from(diagnostic.code.as_u16()),
        message: Some(diagnostic.message.to_string()),
        span: JsSpan {
            start: diagnostic.span.start,
            end: diagnostic.span.end,
        },
        location: Some(JsSourceLocation {
            line: diagnostic.location.line,
            column: diagnostic.location.column,
        }),
        labels: diagnostic
            .labels
            .into_iter()
            .map(|label| JsDiagnosticLabel {
                source_id: label.source.raw(),
                span: JsSpan {
                    start: label.span.start,
                    end: label.span.end,
                },
                message: Some(label.message.to_string()),
            })
            .collect(),
    }
}

fn error_to_js(error: OperationalError) -> JsOperationalError {
    JsOperationalError {
        kind: error.kind.as_str().to_string(),
        code: error.code.as_str().to_string(),
        message: error.message,
        path: error.path,
        details: error
            .details
            .into_iter()
            .map(|(key, value)| JsOperationalErrorDetail {
                key,
                value_json: value.to_string(),
            })
            .collect(),
    }
}
