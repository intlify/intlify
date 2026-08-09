// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Stable `messages emit` result DTOs and checked-domain projections.
//!
//! The execution module supplies only typed linker, exporter, and registration
//! values. This module preserves their canonical order while adapting them to
//! the command's machine-readable envelope and equivalent text presentation.

use std::fmt::Write as _;
use std::path::Path;

use intlify_contract::{
    ArtifactNamespace, CatalogScopeId, DeliveryUnitId, EntryReference, MessageSelector,
    PortablePathSegment, ReferenceArtifactIdentity, ReferenceArtifactSegment,
    ReferenceRecordIdentity, SourceDocumentIdentity, SourceOrigin,
};
use intlify_export::{
    ExportError, ExportErrorEvidence, ExportMessageValidationFailure, ExportPreparationError,
    ExportPreparationInvariant, GenerationLocation, GenerationStage, InternalInvariantViolation,
    InvalidOutputViolation, MappedMessageDiagnostic, OutputLimitCounter, OutputLimitObservation,
    UnsupportedBatchFeature, UnsupportedBatchLocation,
};
use intlify_linker::{
    CompletenessSide, DefinitionLocation, DegradedAnalysisFinding, DynamicReferenceMode,
    LinkFinding, LinkFindingRecord, PartialReason, ResolutionFailure, ResolvedCatalogScopeId,
};
use ox_mf2_parser::DiagnosticSeverity;
use ox_mf2_parser::SemanticInvariantErrorKind;
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::error::OperationalError;
use crate::output::{
    serialize_json_envelope, serialize_json_envelope_with_analysis, CliRunResult, Reporter,
};

use super::COMMAND;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EmitAnalysis {
    pub(super) generation_blocked: bool,
    pub(super) findings: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) message_validation: Option<MessageValidationDto>,
}

impl EmitAnalysis {
    pub(super) fn from_findings(findings: &[LinkFinding], generation_blocked: bool) -> Self {
        Self {
            generation_blocked,
            findings: findings.iter().map(finding_value).collect(),
            message_validation: None,
        }
    }

    pub(super) fn finding_count(&self) -> u64 {
        u64::try_from(self.findings.len()).expect("bounded finding count fits in u64")
    }

    pub(super) fn blocking_finding_count(&self) -> u64 {
        self.findings
            .iter()
            .filter(|finding| finding["blocking"] == Value::Bool(true))
            .count()
            .try_into()
            .expect("bounded finding count fits in u64")
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MessageValidationDto {
    diagnostics: Vec<Value>,
    pub(super) total_diagnostics: u64,
    truncated: bool,
}

impl MessageValidationDto {
    pub(super) fn from_failure(failure: &ExportMessageValidationFailure) -> Self {
        Self {
            diagnostics: failure.diagnostics().iter().map(diagnostic_value).collect(),
            total_diagnostics: failure.total_diagnostics(),
            truncated: failure.truncated(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EmitTargetResult {
    pub(super) target: String,
    pub(super) exporter: &'static str,
    pub(super) out: String,
    pub(super) status: &'static str,
    pub(super) output_state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) artifact_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) payload_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) differences: Option<Vec<Value>>,
    pub(super) diagnostics: Vec<Value>,
    pub(super) errors: Vec<OperationalError>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(super) enum EmitSummary {
    Write(WriteSummary),
    Check(CheckSummary),
    Reduced(ReducedSummary),
}

impl EmitSummary {
    fn status(&self) -> &'static str {
        match self {
            Self::Write(summary) => summary.status,
            Self::Check(summary) => summary.status,
            Self::Reduced(summary) => summary.status,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WriteSummary {
    pub(super) status: &'static str,
    pub(super) operation: &'static str,
    pub(super) selected_targets: u64,
    pub(super) prepared_targets: u64,
    pub(super) prepared_artifacts: u64,
    pub(super) prepared_payload_bytes: u64,
    pub(super) blocked_targets: u64,
    pub(super) diagnostic_count: u64,
    pub(super) finding_count: u64,
    pub(super) blocking_finding_count: u64,
    pub(super) error_targets: u64,
    pub(super) error_count: u64,
    pub(super) written_targets: u64,
    pub(super) unchanged_targets: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CheckSummary {
    pub(super) status: &'static str,
    pub(super) operation: &'static str,
    pub(super) selected_targets: u64,
    pub(super) prepared_targets: u64,
    pub(super) prepared_artifacts: u64,
    pub(super) prepared_payload_bytes: u64,
    pub(super) blocked_targets: u64,
    pub(super) diagnostic_count: u64,
    pub(super) finding_count: u64,
    pub(super) blocking_finding_count: u64,
    pub(super) error_targets: u64,
    pub(super) error_count: u64,
    pub(super) matched_targets: u64,
    pub(super) different_targets: u64,
    pub(super) difference_count: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReducedSummary {
    pub(super) status: &'static str,
    pub(super) operation: &'static str,
    pub(super) selected_targets: u64,
    pub(super) prepared_targets: u64,
    pub(super) prepared_artifacts: u64,
    pub(super) prepared_payload_bytes: u64,
    pub(super) diagnostic_count: u64,
    pub(super) finding_count: u64,
    pub(super) blocking_finding_count: u64,
    pub(super) error_count: u64,
}

pub(super) struct EmitReport {
    pub(super) summary: EmitSummary,
    pub(super) analysis: EmitAnalysis,
    pub(super) results: Vec<EmitTargetResult>,
    pub(super) errors: Vec<OperationalError>,
    pub(super) exit_code: i32,
}

#[cfg(feature = "benchmark")]
pub(super) fn benchmark_analysis_checksum(analysis: &EmitAnalysis) -> Result<u32, ()> {
    let mut fields = Vec::new();
    push_analysis(&mut fields, analysis)?;
    Ok(super::super::observation::observation_checksum(
        b"messages_emit_command_analysis",
        fields.iter().map(Vec::as_slice),
    ))
}

#[cfg(feature = "benchmark")]
pub(super) fn benchmark_observation_checksum(report: &EmitReport) -> Result<u32, ()> {
    let mut fields = Vec::new();
    push_text(&mut fields, "schemaVersion", "0");
    push_text(&mut fields, "command", COMMAND);
    push_text(&mut fields, "version", env!("CARGO_PKG_VERSION"));
    push_summary(&mut fields, &report.summary);
    push_analysis(&mut fields, &report.analysis)?;
    push_count(&mut fields, "results", report.results.len())?;
    for result in &report.results {
        fields.push(b"result".to_vec());
        push_text(&mut fields, "target", &result.target);
        push_text(&mut fields, "exporter", result.exporter);
        push_text(&mut fields, "out", &result.out);
        push_text(&mut fields, "status", result.status);
        push_text(&mut fields, "outputState", result.output_state);
        push_optional_u64(&mut fields, "artifactCount", result.artifact_count);
        push_optional_u64(&mut fields, "payloadBytes", result.payload_bytes);
        push_optional_values(&mut fields, "differences", result.differences.as_deref())?;
        push_values(&mut fields, "diagnostics", &result.diagnostics)?;
        push_errors(&mut fields, "errors", &result.errors)?;
    }
    push_errors(&mut fields, "errors", &report.errors)?;
    push_i32(&mut fields, "exitCode", report.exit_code);
    Ok(super::super::observation::observation_checksum(
        b"messages_emit_typed_result",
        fields.iter().map(Vec::as_slice),
    ))
}

#[cfg(feature = "benchmark")]
fn push_summary(fields: &mut Vec<Vec<u8>>, summary: &EmitSummary) {
    fields.push(b"summary".to_vec());
    match summary {
        EmitSummary::Write(summary) => {
            fields.push(b"write".to_vec());
            push_text(fields, "status", summary.status);
            push_text(fields, "operation", summary.operation);
            push_u64(fields, "selectedTargets", summary.selected_targets);
            push_u64(fields, "preparedTargets", summary.prepared_targets);
            push_u64(fields, "preparedArtifacts", summary.prepared_artifacts);
            push_u64(
                fields,
                "preparedPayloadBytes",
                summary.prepared_payload_bytes,
            );
            push_u64(fields, "blockedTargets", summary.blocked_targets);
            push_u64(fields, "diagnosticCount", summary.diagnostic_count);
            push_u64(fields, "findingCount", summary.finding_count);
            push_u64(
                fields,
                "blockingFindingCount",
                summary.blocking_finding_count,
            );
            push_u64(fields, "errorTargets", summary.error_targets);
            push_u64(fields, "errorCount", summary.error_count);
            push_u64(fields, "writtenTargets", summary.written_targets);
            push_u64(fields, "unchangedTargets", summary.unchanged_targets);
        }
        EmitSummary::Check(summary) => {
            fields.push(b"check".to_vec());
            push_text(fields, "status", summary.status);
            push_text(fields, "operation", summary.operation);
            push_u64(fields, "selectedTargets", summary.selected_targets);
            push_u64(fields, "preparedTargets", summary.prepared_targets);
            push_u64(fields, "preparedArtifacts", summary.prepared_artifacts);
            push_u64(
                fields,
                "preparedPayloadBytes",
                summary.prepared_payload_bytes,
            );
            push_u64(fields, "blockedTargets", summary.blocked_targets);
            push_u64(fields, "diagnosticCount", summary.diagnostic_count);
            push_u64(fields, "findingCount", summary.finding_count);
            push_u64(
                fields,
                "blockingFindingCount",
                summary.blocking_finding_count,
            );
            push_u64(fields, "errorTargets", summary.error_targets);
            push_u64(fields, "errorCount", summary.error_count);
            push_u64(fields, "matchedTargets", summary.matched_targets);
            push_u64(fields, "differentTargets", summary.different_targets);
            push_u64(fields, "differenceCount", summary.difference_count);
        }
        EmitSummary::Reduced(summary) => {
            fields.push(b"reduced".to_vec());
            push_text(fields, "status", summary.status);
            push_text(fields, "operation", summary.operation);
            push_u64(fields, "selectedTargets", summary.selected_targets);
            push_u64(fields, "preparedTargets", summary.prepared_targets);
            push_u64(fields, "preparedArtifacts", summary.prepared_artifacts);
            push_u64(
                fields,
                "preparedPayloadBytes",
                summary.prepared_payload_bytes,
            );
            push_u64(fields, "diagnosticCount", summary.diagnostic_count);
            push_u64(fields, "findingCount", summary.finding_count);
            push_u64(
                fields,
                "blockingFindingCount",
                summary.blocking_finding_count,
            );
            push_u64(fields, "errorCount", summary.error_count);
        }
    }
}

#[cfg(feature = "benchmark")]
fn push_analysis(fields: &mut Vec<Vec<u8>>, analysis: &EmitAnalysis) -> Result<(), ()> {
    fields.push(b"analysis".to_vec());
    push_bool(fields, "generationBlocked", analysis.generation_blocked);
    push_values(fields, "findings", &analysis.findings)?;
    fields.push(b"messageValidation".to_vec());
    if let Some(validation) = analysis.message_validation.as_ref() {
        fields.push(b"present".to_vec());
        push_values(fields, "diagnostics", &validation.diagnostics)?;
        push_u64(fields, "totalDiagnostics", validation.total_diagnostics);
        push_bool(fields, "truncated", validation.truncated);
    } else {
        fields.push(b"absent".to_vec());
    }
    Ok(())
}

#[cfg(feature = "benchmark")]
fn push_errors(
    fields: &mut Vec<Vec<u8>>,
    name: &str,
    errors: &[OperationalError],
) -> Result<(), ()> {
    push_count(fields, name, errors.len())?;
    for error in errors {
        fields.push(b"error".to_vec());
        push_text(fields, "kind", error.kind);
        push_text(fields, "code", error.code);
        fields.push(b"path".to_vec());
        match error.path.as_deref() {
            Some(path) if !Path::new(path).is_absolute() => {
                fields.push(b"relative".to_vec());
                fields.push(path.as_bytes().to_vec());
            }
            Some(_) => fields.push(b"absolute-redacted".to_vec()),
            None => fields.push(b"absent".to_vec()),
        }
        fields.push(b"details".to_vec());
        if let Some(details) = error.details.as_ref() {
            fields.push(b"present".to_vec());
            push_value(fields, details)?;
        } else {
            fields.push(b"absent".to_vec());
        }
    }
    Ok(())
}

#[cfg(feature = "benchmark")]
fn push_optional_values(
    fields: &mut Vec<Vec<u8>>,
    name: &str,
    values: Option<&[Value]>,
) -> Result<(), ()> {
    fields.push(name.as_bytes().to_vec());
    if let Some(values) = values {
        fields.push(b"present".to_vec());
        push_count(fields, "items", values.len())?;
        for value in values {
            push_value(fields, value)?;
        }
    } else {
        fields.push(b"absent".to_vec());
    }
    Ok(())
}

#[cfg(feature = "benchmark")]
fn push_values(fields: &mut Vec<Vec<u8>>, name: &str, values: &[Value]) -> Result<(), ()> {
    push_count(fields, name, values.len())?;
    for value in values {
        push_value(fields, value)?;
    }
    Ok(())
}

#[cfg(feature = "benchmark")]
fn push_value(fields: &mut Vec<Vec<u8>>, value: &Value) -> Result<(), ()> {
    match value {
        Value::Null => fields.push(b"null".to_vec()),
        Value::Bool(value) => {
            fields.push(b"bool".to_vec());
            fields.push(vec![u8::from(*value)]);
        }
        Value::Number(value) => {
            if let Some(value) = value.as_u64() {
                fields.push(b"u64".to_vec());
                fields.push(value.to_be_bytes().to_vec());
            } else if let Some(value) = value.as_i64() {
                fields.push(b"i64".to_vec());
                fields.push(value.to_be_bytes().to_vec());
            } else if let Some(value) = value.as_f64().filter(|value| value.is_finite()) {
                fields.push(b"f64".to_vec());
                fields.push(value.to_bits().to_be_bytes().to_vec());
            } else {
                return Err(());
            }
        }
        Value::String(value) => {
            fields.push(b"string".to_vec());
            fields.push(value.as_bytes().to_vec());
        }
        Value::Array(values) => {
            push_count(fields, "array", values.len())?;
            for value in values {
                push_value(fields, value)?;
            }
        }
        Value::Object(object) => {
            let members = object.iter().filter(|(name, _)| name.as_str() != "message");
            push_count(fields, "object", members.clone().count())?;
            for (name, value) in members {
                fields.push(name.as_bytes().to_vec());
                push_value(fields, value)?;
            }
        }
    }
    Ok(())
}

#[cfg(feature = "benchmark")]
fn push_optional_u64(fields: &mut Vec<Vec<u8>>, name: &str, value: Option<u64>) {
    fields.push(name.as_bytes().to_vec());
    if let Some(value) = value {
        fields.push(b"present".to_vec());
        fields.push(value.to_be_bytes().to_vec());
    } else {
        fields.push(b"absent".to_vec());
    }
}

#[cfg(feature = "benchmark")]
fn push_count(fields: &mut Vec<Vec<u8>>, name: &str, value: usize) -> Result<(), ()> {
    fields.push(name.as_bytes().to_vec());
    fields.push(u64::try_from(value).map_err(|_| ())?.to_be_bytes().to_vec());
    Ok(())
}

#[cfg(feature = "benchmark")]
fn push_text(fields: &mut Vec<Vec<u8>>, name: &str, value: &str) {
    fields.push(name.as_bytes().to_vec());
    fields.push(value.as_bytes().to_vec());
}

#[cfg(feature = "benchmark")]
fn push_u64(fields: &mut Vec<Vec<u8>>, name: &str, value: u64) {
    fields.push(name.as_bytes().to_vec());
    fields.push(value.to_be_bytes().to_vec());
}

#[cfg(feature = "benchmark")]
fn push_i32(fields: &mut Vec<Vec<u8>>, name: &str, value: i32) {
    fields.push(name.as_bytes().to_vec());
    fields.push(value.to_be_bytes().to_vec());
}

#[cfg(feature = "benchmark")]
fn push_bool(fields: &mut Vec<Vec<u8>>, name: &str, value: bool) {
    fields.push(name.as_bytes().to_vec());
    fields.push(vec![u8::from(value)]);
}

#[derive(Debug, Serialize)]
struct EarlyErrorSummary {
    status: &'static str,
    operation: &'static str,
}

/// Render a failure that occurs after command mode is known but before a
/// checked linker outcome exists. Analysis and unresolved counters stay absent.
pub(super) fn render_early_error(
    error: OperationalError,
    check: bool,
    reporter: Reporter,
    project_root: &Path,
) -> CliRunResult {
    match reporter {
        Reporter::Json => CliRunResult {
            exit_code: 2,
            stdout: format!(
                "{}\n",
                serialize_json_envelope(
                    COMMAND,
                    project_root,
                    EarlyErrorSummary {
                        status: "error",
                        operation: if check { "check" } else { "write" },
                    },
                    Vec::<Value>::new(),
                    vec![error],
                )
            ),
            stderr: String::new(),
        },
        Reporter::Text => CliRunResult {
            exit_code: 2,
            stdout: String::new(),
            stderr: format!("error: {}\n", error.message),
        },
    }
}

pub(super) fn render(report: EmitReport, reporter: Reporter, project_root: &Path) -> CliRunResult {
    match reporter {
        Reporter::Json => CliRunResult {
            exit_code: report.exit_code,
            stdout: format!(
                "{}\n",
                serialize_json_envelope_with_analysis(
                    COMMAND,
                    project_root,
                    report.summary,
                    report.analysis,
                    report.results,
                    report.errors,
                )
            ),
            stderr: String::new(),
        },
        Reporter::Text => render_text(&report),
    }
}

fn render_text(report: &EmitReport) -> CliRunResult {
    let mut stdout = format!("messages emit: {}\n", report.summary.status());
    for result in &report.results {
        writeln!(
            &mut stdout,
            "{}: {} ({})",
            result.target, result.status, result.output_state
        )
        .expect("writing to a String cannot fail");
        for difference in result.differences.as_deref().unwrap_or_default() {
            writeln!(
                &mut stdout,
                "  difference: {}",
                difference["kind"].as_str().unwrap_or("unknown")
            )
            .expect("writing to a String cannot fail");
        }
    }
    for finding in &report.analysis.findings {
        writeln!(
            &mut stdout,
            "finding: {}{}",
            finding["kind"].as_str().unwrap_or("unknown"),
            if finding["blocking"] == Value::Bool(true) {
                " (blocking)"
            } else {
                ""
            }
        )
        .expect("writing to a String cannot fail");
    }

    let mut stderr = String::new();
    for error in report.errors.iter().chain(
        report
            .results
            .iter()
            .flat_map(|result| result.errors.iter()),
    ) {
        writeln!(&mut stderr, "error: {}", error.message).expect("writing to a String cannot fail");
    }
    CliRunResult {
        exit_code: report.exit_code,
        stdout,
        stderr,
    }
}

pub(super) fn export_error(error: &ExportError, output_root: &str) -> OperationalError {
    if let ExportErrorEvidence::InternalInvariant(evidence) = error.evidence() {
        return OperationalError {
            kind: "internal",
            code: "internal_error",
            message: error.to_string(),
            path: Some(output_root.to_owned()),
            details: Some(json!({
                "reason": "message_exporter_invariant_failed",
                "invariant": exporter_invariant_token(evidence.invariant()),
            })),
        };
    }

    let (kind, evidence) = match error.evidence() {
        ExportErrorEvidence::UnsupportedBatch(evidence) => (
            "unsupported_batch",
            json!({
                "feature": unsupported_feature_token(evidence.feature()),
                "location": unsupported_location_value(evidence.location()),
            }),
        ),
        ExportErrorEvidence::GenerationFailed(evidence) => (
            "generation_failed",
            json!({
                "stage": generation_stage_token(evidence.stage()),
                "location": generation_location_value(evidence.location()),
            }),
        ),
        ExportErrorEvidence::OutputLimitExceeded(evidence) => {
            let observation = match evidence.observation() {
                OutputLimitObservation::Exact(value) => Value::from(value),
                OutputLimitObservation::ArithmeticOverflow => {
                    Value::String("arithmetic_overflow".to_owned())
                }
            };
            (
                "output_limit_exceeded",
                json!({
                    "counter": output_limit_counter_token(evidence.counter()),
                    "limit": evidence.counter().ceiling(),
                    "observed": observation,
                }),
            )
        }
        ExportErrorEvidence::InvalidOutput(evidence) => (
            "invalid_output",
            json!({ "violation": invalid_output_token(evidence.violation()) }),
        ),
        ExportErrorEvidence::InternalInvariant(_) => unreachable!("handled above"),
    };
    OperationalError {
        kind: "output",
        code: "message_export_failed",
        message: error.to_string(),
        path: Some(output_root.to_owned()),
        details: Some(json!({ "kind": kind, "evidence": evidence })),
    }
}

pub(super) fn exporter_factory_error(output_root: &str) -> OperationalError {
    OperationalError {
        kind: "internal",
        code: "internal_error",
        message: "Checked delivery target and exporter policy are inconsistent.".to_owned(),
        path: Some(output_root.to_owned()),
        details: Some(json!({
            "reason": "message_exporter_invariant_failed",
            "invariant": "capability_preflight_contract",
        })),
    }
}

pub(super) fn preparation_error(error: &ExportPreparationError) -> OperationalError {
    match error {
        ExportPreparationError::MessageValidation(_) => {
            unreachable!("message validation remains command analysis")
        }
        ExportPreparationError::SemanticModelConstruction(error) => semantic_error(
            error.kind(),
            "semantic_model_construction",
            "Semantic model construction failed during message export preparation.",
        ),
        ExportPreparationError::SemanticValidation(error) => semantic_error(
            error.kind(),
            "semantic_validation",
            "Semantic validation failed during message export preparation.",
        ),
        ExportPreparationError::InternalInvariant(invariant) => OperationalError {
            kind: "internal",
            code: "internal_error",
            message: "Message export preparation violated an invariant.".to_owned(),
            path: None,
            details: Some(json!({
                "reason": "message_export_preparation_invariant_failed",
                "invariant": preparation_invariant_token(*invariant),
            })),
        },
    }
}

fn semantic_error(
    kind: SemanticInvariantErrorKind,
    stage: &'static str,
    message: &'static str,
) -> OperationalError {
    match kind {
        SemanticInvariantErrorKind::ApiMisuse => OperationalError {
            kind: "internal",
            code: "internal_error",
            message: message.to_owned(),
            path: None,
            details: Some(json!({ "reason": "semantic_api_misuse" })),
        },
        SemanticInvariantErrorKind::InvariantViolation => OperationalError {
            kind: "internal",
            code: "internal_error",
            message: message.to_owned(),
            path: None,
            details: Some(json!({
                "reason": "semantic_invariant_failed",
                "stage": stage,
            })),
        },
    }
}

const fn preparation_invariant_token(invariant: ExportPreparationInvariant) -> &'static str {
    use intlify_export::{
        DiagnosticMappingInvariant, OutcomeContractInvariant, TypedOutputInvariant,
    };
    match invariant {
        ExportPreparationInvariant::ParserFailure => "parser_failure",
        ExportPreparationInvariant::DiagnosticCountOverflow => "diagnostic_count_overflow",
        ExportPreparationInvariant::DiagnosticMapping(
            DiagnosticMappingInvariant::LabelCountExceeded,
        ) => "diagnostic_label_count_exceeded",
        ExportPreparationInvariant::DiagnosticMapping(
            DiagnosticMappingInvariant::InvalidPrimarySpan,
        ) => "diagnostic_primary_span_invalid",
        ExportPreparationInvariant::DiagnosticMapping(
            DiagnosticMappingInvariant::InvalidLabelSpan,
        ) => "diagnostic_label_span_invalid",
        ExportPreparationInvariant::DiagnosticMapping(
            DiagnosticMappingInvariant::ForeignLabelSource,
        ) => "diagnostic_foreign_label_source",
        ExportPreparationInvariant::OutcomeContract(
            OutcomeContractInvariant::DuplicatePlanCoordinate,
        ) => "outcome_duplicate_plan_coordinate",
        ExportPreparationInvariant::OutcomeContract(
            OutcomeContractInvariant::NonCanonicalPlanOrder,
        ) => "outcome_noncanonical_plan_order",
        ExportPreparationInvariant::OutcomeContract(
            OutcomeContractInvariant::DuplicateLogicalMessage,
        ) => "outcome_duplicate_logical_message",
        ExportPreparationInvariant::OutcomeContract(
            OutcomeContractInvariant::NonCanonicalMessageOrder,
        ) => "outcome_noncanonical_message_order",
        ExportPreparationInvariant::OutcomeContract(
            OutcomeContractInvariant::DefinitionSnapshotMismatch,
        ) => "outcome_definition_snapshot_mismatch",
        ExportPreparationInvariant::TypedOutput(TypedOutputInvariant::ModelRelationMismatch) => {
            "typed_output_model_relation_mismatch"
        }
        ExportPreparationInvariant::TypedOutput(TypedOutputInvariant::SignatureKeyMismatch) => {
            "typed_output_signature_key_mismatch"
        }
        ExportPreparationInvariant::TypedOutput(TypedOutputInvariant::DuplicateArgument) => {
            "typed_output_duplicate_argument"
        }
        ExportPreparationInvariant::TypedOutput(
            TypedOutputInvariant::NonCanonicalArgumentOrder,
        ) => "typed_output_noncanonical_argument_order",
    }
}

const fn unsupported_feature_token(feature: UnsupportedBatchFeature) -> &'static str {
    match feature {
        UnsupportedBatchFeature::BatchComposition => "batch_composition",
        UnsupportedBatchFeature::DeliveryUnitPartitioning => "delivery_unit_partitioning",
        UnsupportedBatchFeature::LocalePartitioning => "locale_partitioning",
        UnsupportedBatchFeature::FallbackSemantics => "fallback_semantics",
        UnsupportedBatchFeature::MessageSemantics => "message_semantics",
    }
}

fn unsupported_location_value(location: &UnsupportedBatchLocation) -> Value {
    match location {
        UnsupportedBatchLocation::Batch => json!({ "kind": "batch" }),
        UnsupportedBatchLocation::Plan {
            delivery_unit,
            locale,
        } => json!({
            "kind": "plan",
            "deliveryUnit": delivery_unit_value(delivery_unit),
            "locale": locale.as_str(),
        }),
        UnsupportedBatchLocation::Definition {
            delivery_unit,
            locale,
            source,
            entry,
        } => json!({
            "kind": "definition",
            "deliveryUnit": delivery_unit_value(delivery_unit),
            "locale": locale.as_str(),
            "source": source_value(source),
            "entry": entry_value(entry),
        }),
    }
}

const fn generation_stage_token(stage: GenerationStage) -> &'static str {
    match stage {
        GenerationStage::MessageCompilation => "message_compilation",
        GenerationStage::PayloadEncoding => "payload_encoding",
        GenerationStage::MetadataGeneration => "metadata_generation",
    }
}

fn generation_location_value(location: &GenerationLocation) -> Value {
    match location {
        GenerationLocation::Batch => json!({ "kind": "batch" }),
        GenerationLocation::Plan {
            delivery_unit,
            locale,
        } => json!({
            "kind": "plan",
            "deliveryUnit": delivery_unit_value(delivery_unit),
            "locale": locale.as_str(),
        }),
        GenerationLocation::Definition {
            delivery_unit,
            locale,
            source,
            entry,
        } => json!({
            "kind": "definition",
            "deliveryUnit": delivery_unit_value(delivery_unit),
            "locale": locale.as_str(),
            "source": source_value(source),
            "entry": entry_value(entry),
        }),
    }
}

const fn output_limit_counter_token(counter: OutputLimitCounter) -> &'static str {
    match counter {
        OutputLimitCounter::ArtifactRecordsPerSet => "artifact_records_per_set",
        OutputLimitCounter::ArtifactPathSegmentsPerPath => "artifact_path_segments_per_path",
        OutputLimitCounter::ArtifactPathSegmentBytes => "artifact_path_segment_bytes",
        OutputLimitCounter::ArtifactPathBytesPerPath => "artifact_path_bytes_per_path",
        OutputLimitCounter::ArtifactPathBytesPerSet => "artifact_path_bytes_per_set",
        OutputLimitCounter::ArtifactKindBytes => "artifact_kind_bytes",
        OutputLimitCounter::MediaTypeComponentBytes => "media_type_component_bytes",
        OutputLimitCounter::MediaTypeBytes => "media_type_bytes",
        OutputLimitCounter::RelationshipRecordsPerArtifact => "relationship_records_per_artifact",
        OutputLimitCounter::RelationshipRecordsPerSet => "relationship_records_per_set",
        OutputLimitCounter::RelationshipTargetBytesPerSet => "relationship_target_bytes_per_set",
        OutputLimitCounter::PayloadBytesPerArtifact => "payload_bytes_per_artifact",
        OutputLimitCounter::PayloadBytesPerSet => "payload_bytes_per_set",
    }
}

const fn invalid_output_token(violation: InvalidOutputViolation) -> &'static str {
    match violation {
        InvalidOutputViolation::ArtifactEnvelope => "artifact_envelope",
        InvalidOutputViolation::ArtifactPath => "artifact_path",
        InvalidOutputViolation::DuplicateArtifactPath => "duplicate_artifact_path",
        InvalidOutputViolation::ArtifactKind => "artifact_kind",
        InvalidOutputViolation::FormatVersion => "format_version",
        InvalidOutputViolation::MetadataEnvelope => "metadata_envelope",
        InvalidOutputViolation::MediaType => "media_type",
        InvalidOutputViolation::RelationshipEnvelope => "relationship_envelope",
        InvalidOutputViolation::RelationshipKind => "relationship_kind",
        InvalidOutputViolation::RelationshipTarget => "relationship_target",
        InvalidOutputViolation::DuplicateRelationship => "duplicate_relationship",
        InvalidOutputViolation::ConflictingRelationshipClassification => {
            "conflicting_relationship_classification"
        }
        InvalidOutputViolation::RelationshipSelfEdge => "relationship_self_edge",
        InvalidOutputViolation::LazyTargetInEagerClosure => "lazy_target_in_eager_closure",
    }
}

const fn exporter_invariant_token(invariant: InternalInvariantViolation) -> &'static str {
    match invariant {
        InternalInvariantViolation::ValidatedBatchContract => "validated_batch_contract",
        InternalInvariantViolation::CapabilityPreflightContract => "capability_preflight_contract",
        InternalInvariantViolation::GenerationState => "generation_state",
        InternalInvariantViolation::OutputBudgetAccounting => "output_budget_accounting",
        InternalInvariantViolation::ArtifactAssemblyState => "artifact_assembly_state",
        InternalInvariantViolation::SharedConstructorState => "shared_constructor_state",
    }
}

fn finding_value(finding: &LinkFinding) -> Value {
    let (kind, subject, evidence) = match finding.record() {
        LinkFindingRecord::AmbiguousMessageDefinition(finding) => (
            "ambiguous-message-definition",
            json!({
                "scope": resolved_scope_value(finding.subject().resolved_scope()),
                "domain": finding.subject().domain().as_str(),
                "key": finding.subject().key().as_str(),
                "locale": finding.subject().locale().as_str(),
            }),
            json!({
                "definitions": finding.evidence().definitions().iter().map(definition_value).collect::<Vec<_>>()
            }),
        ),
        LinkFindingRecord::UnresolvedMessage(finding) => {
            let evidence = finding.evidence();
            let mut object = ordered_object([
                (
                    "deliveryUnit",
                    delivery_unit_value(evidence.delivery_unit()),
                ),
                ("scope", resolved_scope_value(evidence.resolved_scope())),
                (
                    "domain",
                    Value::String(evidence.domain().as_str().to_owned()),
                ),
                ("selector", selector_value(evidence.selector())),
            ]);
            insert_optional_string(
                &mut object,
                "reason",
                evidence.reason().map(intlify_contract::ReasonText::as_str),
            );
            insert_optional_value(&mut object, "origin", evidence.origin().map(origin_value));
            object.insert(
                "failures".to_owned(),
                Value::Array(
                    evidence
                        .failures()
                        .iter()
                        .map(resolution_failure_value)
                        .collect(),
                ),
            );
            (
                "unresolved-message",
                reference_record_value(finding.subject().reference()),
                Value::Object(object),
            )
        }
        LinkFindingRecord::MissingTranslation(finding) => {
            let subject = finding.subject();
            let evidence = finding.evidence();
            let mut evidence_object = ordered_object([
                (
                    "deliveryUnit",
                    delivery_unit_value(evidence.delivery_unit()),
                ),
                ("scope", resolved_scope_value(evidence.resolved_scope())),
                (
                    "domain",
                    Value::String(evidence.domain().as_str().to_owned()),
                ),
                ("probedLocales", locales_value(evidence.probed_locales())),
                (
                    "selectedLocale",
                    Value::String(evidence.selected_locale().as_str().to_owned()),
                ),
                ("definition", definition_value(evidence.definition())),
            ]);
            insert_optional_value(
                &mut evidence_object,
                "origin",
                evidence.origin().map(origin_value),
            );
            (
                "missing-translation",
                json!({
                    "reference": reference_record_value(subject.reference()),
                    "requestedLocale": subject.requested_locale().as_str(),
                    "key": subject.key().as_str(),
                }),
                Value::Object(evidence_object),
            )
        }
        LinkFindingRecord::OrphanedTranslation(finding) => (
            "orphaned-translation",
            scope_key_locale_value(
                finding.subject().resolved_scope(),
                finding.subject().domain().as_str(),
                finding.subject().key().as_str(),
                finding.subject().locale().as_str(),
            ),
            json!({
                "baselineLocale": finding.evidence().baseline_locale().as_str(),
                "definition": definition_value(finding.evidence().definition()),
            }),
        ),
        LinkFindingRecord::UnusedMessage(finding) => (
            "unused-message",
            scope_key_locale_value(
                finding.subject().resolved_scope(),
                finding.subject().domain().as_str(),
                finding.subject().key().as_str(),
                finding.subject().locale().as_str(),
            ),
            json!({ "definition": definition_value(finding.evidence().definition()) }),
        ),
        LinkFindingRecord::UnboundedDynamicReference(finding) => {
            let evidence = finding.evidence();
            let mut object = ordered_object([
                (
                    "deliveryUnit",
                    delivery_unit_value(evidence.delivery_unit()),
                ),
                ("scope", resolved_scope_value(evidence.resolved_scope())),
                (
                    "domain",
                    Value::String(evidence.domain().as_str().to_owned()),
                ),
            ]);
            insert_optional_string(
                &mut object,
                "reason",
                evidence.reason().map(intlify_contract::ReasonText::as_str),
            );
            insert_optional_value(&mut object, "origin", evidence.origin().map(origin_value));
            object.insert(
                "dynamicMode".to_owned(),
                Value::String(dynamic_mode_token(evidence.dynamic_mode()).to_owned()),
            );
            (
                "unbounded-dynamic-reference",
                reference_record_value(finding.subject().reference()),
                Value::Object(object),
            )
        }
        LinkFindingRecord::DegradedAnalysis(degraded) => match degraded {
            DegradedAnalysisFinding::WideSelector(finding) => {
                let evidence = finding.evidence();
                let mut object = ordered_object([
                    ("kind", Value::String("wide-selector".to_owned())),
                    (
                        "deliveryUnit",
                        delivery_unit_value(evidence.delivery_unit()),
                    ),
                    ("scope", resolved_scope_value(evidence.resolved_scope())),
                    (
                        "domain",
                        Value::String(evidence.domain().as_str().to_owned()),
                    ),
                ]);
                insert_optional_string(
                    &mut object,
                    "reason",
                    evidence.reason().map(intlify_contract::ReasonText::as_str),
                );
                insert_optional_value(&mut object, "origin", evidence.origin().map(origin_value));
                (
                    "degraded-analysis",
                    reference_record_value(finding.subject().reference()),
                    Value::Object(object),
                )
            }
            DegradedAnalysisFinding::PartialCompleteness(finding) => (
                "degraded-analysis",
                json!({
                    "scope": resolved_scope_value(finding.subject().resolved_scope()),
                    "side": completeness_side_token(finding.subject().side()),
                }),
                json!({
                    "kind": "partial-completeness",
                    "contributors": finding.evidence().contributors().iter().map(|contributor| json!({
                        "scope": scope_value(contributor.scope()),
                        "reason": partial_reason_token(contributor.reason()),
                    })).collect::<Vec<_>>(),
                }),
            ),
        },
    };

    json!({
        "kind": kind,
        "blocking": finding.blocking(),
        "subject": subject,
        "evidence": evidence,
    })
}

fn diagnostic_value(diagnostic: &MappedMessageDiagnostic) -> Value {
    let definition = diagnostic.definition();
    json!({
        "source": source_value(definition.source()),
        "entry": entry_value(definition.entry()),
        "category": diagnostic.kind().category(),
        "code": diagnostic.kind().code(),
        "severity": severity_token(diagnostic.severity()),
        "message": diagnostic.message(),
        "span": {
            "start": diagnostic.span().start(),
            "end": diagnostic.span().end(),
        },
        "labels": diagnostic.labels().iter().map(|label| json!({
            "span": {
                "start": label.span().start(),
                "end": label.span().end(),
            },
            "message": label.message(),
        })).collect::<Vec<_>>(),
    })
}

fn scope_key_locale_value(
    scope: &ResolvedCatalogScopeId,
    domain: &str,
    key: &str,
    locale: &str,
) -> Value {
    json!({
        "scope": resolved_scope_value(scope),
        "domain": domain,
        "key": key,
        "locale": locale,
    })
}

fn resolution_failure_value(failure: &ResolutionFailure) -> Value {
    match failure {
        ResolutionFailure::Selector(failure) => json!({
            "kind": "selector",
            "requestedLocale": failure.requested_locale().as_str(),
            "probedLocales": locales_value(failure.probed_locales()),
        }),
        ResolutionFailure::Message(failure) => json!({
            "kind": "message",
            "key": failure.key().as_str(),
            "requestedLocale": failure.requested_locale().as_str(),
            "probedLocales": locales_value(failure.probed_locales()),
        }),
    }
}

fn selector_value(selector: &MessageSelector) -> Value {
    match selector {
        MessageSelector::Exact(key) => json!({ "kind": "exact", "key": key.as_str() }),
        MessageSelector::Prefix(prefix) => {
            json!({ "kind": "prefix", "prefix": prefix.as_str() })
        }
        MessageSelector::Pattern(pattern) => {
            json!({ "kind": "pattern", "pattern": pattern.as_str() })
        }
        MessageSelector::AllInScope => json!({ "kind": "all-in-scope" }),
        MessageSelector::UnboundedDynamic => json!({ "kind": "unbounded-dynamic" }),
    }
}

fn reference_record_value(identity: &ReferenceRecordIdentity) -> Value {
    json!({
        "artifact": reference_artifact_value(identity.artifact()),
        "ordinal": identity.ordinal(),
    })
}

fn reference_artifact_value(identity: &ReferenceArtifactIdentity) -> Value {
    json!({
        "namespace": namespace_value(identity.namespace()),
        "segments": identity.segments().iter().map(ReferenceArtifactSegment::as_str).collect::<Vec<_>>(),
    })
}

fn definition_value(definition: &DefinitionLocation) -> Value {
    json!({
        "source": source_value(definition.source()),
        "entry": entry_value(definition.entry()),
    })
}

fn source_value(source: &SourceDocumentIdentity) -> Value {
    json!({
        "namespace": namespace_value(source.namespace()),
        "path": source.path().segments().iter().map(PortablePathSegment::as_str).collect::<Vec<_>>(),
    })
}

fn entry_value(entry: &EntryReference) -> Value {
    json!({
        "structuralPath": entry.structural_path().as_str(),
        "occurrence": entry.occurrence(),
    })
}

fn origin_value(origin: &SourceOrigin) -> Value {
    json!({
        "source": source_value(origin.source()),
        "span": {
            "start": origin.span().start(),
            "end": origin.span().end(),
        }
    })
}

fn resolved_scope_value(scope: &ResolvedCatalogScopeId) -> Value {
    scope_value(scope.as_catalog_scope())
}

fn scope_value(scope: &CatalogScopeId) -> Value {
    json!({
        "namespace": namespace_value(scope.namespace()),
        "name": scope.name().as_str(),
    })
}

fn namespace_value(namespace: ArtifactNamespace) -> Value {
    json!({ "kind": namespace.as_str() })
}

fn delivery_unit_value(delivery_unit: &DeliveryUnitId) -> Value {
    Value::Array(
        delivery_unit
            .segments()
            .iter()
            .map(|segment| Value::String(segment.as_str().to_owned()))
            .collect(),
    )
}

fn locales_value(locales: &[intlify_contract::Locale]) -> Value {
    Value::Array(
        locales
            .iter()
            .map(|locale| Value::String(locale.as_str().to_owned()))
            .collect(),
    )
}

fn ordered_object<const N: usize>(fields: [(&str, Value); N]) -> Map<String, Value> {
    fields
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect()
}

fn insert_optional_string(object: &mut Map<String, Value>, field: &str, value: Option<&str>) {
    if let Some(value) = value {
        object.insert(field.to_owned(), Value::String(value.to_owned()));
    }
}

fn insert_optional_value(object: &mut Map<String, Value>, field: &str, value: Option<Value>) {
    if let Some(value) = value {
        object.insert(field.to_owned(), value);
    }
}

const fn dynamic_mode_token(mode: DynamicReferenceMode) -> &'static str {
    match mode {
        DynamicReferenceMode::Compat => "compat",
        DynamicReferenceMode::Strict => "strict",
    }
}

const fn completeness_side_token(side: CompletenessSide) -> &'static str {
    match side {
        CompletenessSide::Definitions => "definitions",
        CompletenessSide::References => "references",
    }
}

const fn partial_reason_token(reason: PartialReason) -> &'static str {
    match reason {
        PartialReason::OpenEditorWorld => "open-editor-world",
        PartialReason::SourceOmitted => "source-omitted",
        PartialReason::SourceFailed => "source-failed",
        PartialReason::ProducerOmitted => "producer-omitted",
        PartialReason::ProducerFailed => "producer-failed",
        PartialReason::ExternalArtifactUnverified => "external-artifact-unverified",
    }
}

const fn severity_token(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => "error",
        DiagnosticSeverity::Warning => "warning",
        DiagnosticSeverity::Information => "information",
        DiagnosticSeverity::Hint => "hint",
    }
}

#[cfg(all(test, feature = "benchmark"))]
mod benchmark_tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn observation_checksum_ignores_render_only_project_root() {
        let first_report = report("web");
        let second_report = report("web");
        let first_checksum = benchmark_observation_checksum(&first_report).unwrap();
        let second_checksum = benchmark_observation_checksum(&second_report).unwrap();
        let first_root = tempdir().unwrap();
        let second_root = tempdir().unwrap();

        let first_rendered = render(first_report, Reporter::Json, first_root.path());
        let second_rendered = render(second_report, Reporter::Json, second_root.path());

        assert_ne!(first_rendered.stdout, second_rendered.stdout);
        assert_eq!(first_checksum, second_checksum);
    }

    #[test]
    fn observation_checksum_changes_with_typed_result_fields() {
        let first_checksum = benchmark_observation_checksum(&report("web")).unwrap();
        let second_checksum = benchmark_observation_checksum(&report("server")).unwrap();

        assert_ne!(first_checksum, second_checksum);
    }

    #[test]
    fn observation_checksum_ignores_human_error_messages() {
        let mut first_report = report("web");
        first_report.errors.push(error("first human explanation"));
        let mut second_report = report("web");
        second_report
            .errors
            .push(error("rewritten human explanation"));

        assert_eq!(
            benchmark_observation_checksum(&first_report).unwrap(),
            benchmark_observation_checksum(&second_report).unwrap()
        );
    }

    #[test]
    fn observation_checksum_redacts_absolute_error_paths() {
        let first_root = tempdir().unwrap();
        let second_root = tempdir().unwrap();
        let mut first_report = report("web");
        let mut first_error = error("failure");
        first_error.path = Some(
            first_root
                .path()
                .join("catalog.json")
                .to_string_lossy()
                .into_owned(),
        );
        first_report.errors.push(first_error);
        let mut second_report = report("web");
        let mut second_error = error("failure");
        second_error.path = Some(
            second_root
                .path()
                .join("catalog.json")
                .to_string_lossy()
                .into_owned(),
        );
        second_report.errors.push(second_error);

        assert_eq!(
            benchmark_observation_checksum(&first_report).unwrap(),
            benchmark_observation_checksum(&second_report).unwrap()
        );
    }

    fn report(target: &str) -> EmitReport {
        EmitReport {
            summary: EmitSummary::Write(WriteSummary {
                status: "success",
                operation: "write",
                selected_targets: 1,
                prepared_targets: 1,
                prepared_artifacts: 1,
                prepared_payload_bytes: 4,
                blocked_targets: 0,
                diagnostic_count: 0,
                finding_count: 0,
                blocking_finding_count: 0,
                error_targets: 0,
                error_count: 0,
                written_targets: 1,
                unchanged_targets: 0,
            }),
            analysis: EmitAnalysis {
                generation_blocked: false,
                findings: Vec::new(),
                message_validation: None,
            },
            results: vec![EmitTargetResult {
                target: target.to_owned(),
                exporter: "esm",
                out: "generated/messages".to_owned(),
                status: "written",
                output_state: "updated",
                artifact_count: Some(1),
                payload_bytes: Some(4),
                differences: None,
                diagnostics: Vec::new(),
                errors: Vec::new(),
            }],
            errors: Vec::new(),
            exit_code: 0,
        }
    }

    fn error(message: &str) -> OperationalError {
        OperationalError {
            kind: "generation_failed",
            code: "message_export_generation_failed",
            message: message.to_owned(),
            path: Some("catalog.json".to_owned()),
            details: Some(json!({ "stage": "artifact-construction" })),
        }
    }
}
