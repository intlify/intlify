// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Executable `messages emit` project workflow and reporter adaptation.
//!
//! This module owns command-specific configuration activation, target
//! selection, shared link/export preparation, per-target export and output
//! registration, and the typed text/JSON result. The exporter remains pure and
//! receives no filesystem capability.

mod report;

use std::path::Path;

use intlify_contract::{ArtifactContractError, LinkLimits};
use intlify_export::{
    prepare_export, ExportArtifactSet, ExportPreparationError, ExportValidationLimits,
};
use intlify_linker::LinkOperationalError;
use intlify_producer_js::{JsProducerError, JsProducerFailure};
use intlify_resource::HostFormatRegistry;
use serde_json::{json, Map, Value};

use self::report::{
    CheckSummary, EmitAnalysis, EmitReport, EmitSummary, EmitTargetResult, MessageValidationDto,
    ReducedSummary, WriteSummary,
};
use super::delivery::{DeliveryTargetSelectionError, ResolvedDeliveryTarget};
use super::exporter_registry::BuiltInExporterRegistry;
use super::orchestration::{link_project, ProjectLinkError, ProjectLinkExecution};
use super::reference::{ReferenceInventoryError, ReferenceSourceFailure};
use super::registration::{
    output_state_token, reporter_error as registration_error, PreparedOutput,
    WriteRegistrationStatus,
};
use crate::args::MessagesEmitArgs;
use crate::config;
use crate::error::OperationalError;
use crate::output::{CliRunResult, Reporter};

const COMMAND: &str = "messages.emit";

pub(crate) fn run(
    arguments: &MessagesEmitArgs,
    reporter: Reporter,
    config_path: Option<&str>,
    cwd: &Path,
) -> CliRunResult {
    let project_root = config::discover_project_root(cwd);
    let loaded = match config::load_project_config(cwd, config_path) {
        Ok(loaded) => loaded,
        Err(error) => {
            return report::render_early_error(
                error.into(),
                arguments.check,
                reporter,
                &project_root,
            );
        }
    };
    let Some(messages) = loaded.resolved_messages.as_ref() else {
        return report::render_early_error(
            delivery_required_error(),
            arguments.check,
            reporter,
            &loaded.project_root,
        );
    };
    let Some(delivery) = messages.delivery() else {
        return report::render_early_error(
            delivery_required_error(),
            arguments.check,
            reporter,
            &loaded.project_root,
        );
    };
    let selected = match delivery.select(arguments.target.as_deref()) {
        Ok(selected) => selected,
        Err(error) => {
            return report::render_early_error(
                target_selection_error(&error, arguments.target.as_deref()),
                arguments.check,
                reporter,
                &loaded.project_root,
            );
        }
    };

    let selected_targets = selected.targets();
    let execution = match link_project(
        &loaded.project_root,
        loaded.config_path.as_deref(),
        &loaded.resolved_resources,
        messages,
        &HostFormatRegistry::new(),
        &LinkLimits::default(),
        None,
    ) {
        Ok(execution) => execution,
        Err(error) => {
            return report::render_early_error(
                project_link_error(&error),
                arguments.check,
                reporter,
                &loaded.project_root,
            );
        }
    };

    execute_from_linked_project(
        &execution,
        selected_targets,
        messages,
        arguments.check,
        reporter,
        &loaded.project_root,
    )
}

fn execute_from_linked_project(
    execution: &ProjectLinkExecution,
    selected_targets: &[ResolvedDeliveryTarget],
    messages: &super::config::ResolvedMessagesConfig,
    check: bool,
    reporter: Reporter,
    project_root: &Path,
) -> CliRunResult {
    let mut analysis = EmitAnalysis::from_findings(
        execution.outcome().findings(),
        execution.outcome().generation_blocked(),
    );
    let source_errors = source_errors(execution);
    if !source_errors.is_empty() {
        analysis.generation_blocked = true;
        return render_reduced_error(
            selected_targets,
            check,
            analysis,
            source_errors,
            reporter,
            project_root,
        );
    }

    let prepared = match prepare_export(execution.outcome(), ExportValidationLimits::default()) {
        Ok(Some(batch)) => batch,
        Ok(None) => {
            analysis.generation_blocked = true;
            return render_blocked(selected_targets, check, analysis, reporter, project_root);
        }
        Err(ExportPreparationError::MessageValidation(failure)) => {
            analysis.generation_blocked = true;
            analysis.message_validation = Some(MessageValidationDto::from_failure(&failure));
            return render_blocked(selected_targets, check, analysis, reporter, project_root);
        }
        Err(error) => {
            analysis.generation_blocked = true;
            return render_reduced_error(
                selected_targets,
                check,
                analysis,
                vec![report::preparation_error(&error)],
                reporter,
                project_root,
            );
        }
    };

    let registry = BuiltInExporterRegistry::new();
    let results = selected_targets
        .iter()
        .map(|target| {
            execute_target(
                target,
                messages.policy(),
                &prepared,
                &registry,
                check,
                project_root,
            )
        })
        .collect::<Vec<_>>();
    render_results(results, check, analysis, reporter, project_root)
}

fn execute_target(
    target: &ResolvedDeliveryTarget,
    policy: &intlify_linker::LinkPolicy,
    batch: &intlify_export::ValidatedExportBatch<'_>,
    registry: &BuiltInExporterRegistry,
    check: bool,
    project_root: &Path,
) -> EmitTargetResult {
    let mut result = target_result_shell(target, check);
    let Ok(exporter) = registry.create(target, policy) else {
        result.status = "error";
        result
            .errors
            .push(report::exporter_factory_error(target.out().as_str()));
        return result;
    };
    let artifacts = match exporter.export(batch) {
        Ok(artifacts) => artifacts,
        Err(error) => {
            result.status = "error";
            result
                .errors
                .push(report::export_error(&error, target.out().as_str()));
            return result;
        }
    };
    let (artifact_count, payload_bytes) = artifact_metrics(&artifacts);
    result.artifact_count = Some(artifact_count);
    result.payload_bytes = Some(payload_bytes);

    let prepared = match PreparedOutput::try_new(target, &artifacts) {
        Ok(prepared) => prepared,
        Err(error) => {
            result.status = "error";
            result.output_state = output_state_token(error.output_state());
            result
                .errors
                .push(registration_error(&error, target.out().as_str()));
            return result;
        }
    };

    if check {
        match prepared.check(project_root) {
            Ok(comparison) => {
                result.status = if comparison.is_matched() {
                    "matched"
                } else {
                    "different"
                };
                result.differences = Some(comparison.reporter_differences());
            }
            Err(error) => {
                result.status = "error";
                result.output_state = output_state_token(error.output_state());
                result
                    .errors
                    .push(registration_error(&error, target.out().as_str()));
            }
        }
    } else {
        match prepared.write(project_root) {
            Ok(outcome) => {
                result.status = match outcome.status() {
                    WriteRegistrationStatus::Unchanged => "unchanged",
                    WriteRegistrationStatus::Written => "written",
                };
                result.output_state = output_state_token(outcome.output_state());
            }
            Err(error) => {
                result.status = "error";
                result.output_state = output_state_token(error.output_state());
                result
                    .errors
                    .push(registration_error(&error, target.out().as_str()));
            }
        }
    }
    result
}

fn target_result_shell(target: &ResolvedDeliveryTarget, check: bool) -> EmitTargetResult {
    EmitTargetResult {
        target: target.name().as_str().to_owned(),
        exporter: target.exporter().as_str(),
        out: target.out().as_str().to_owned(),
        status: "blocked",
        output_state: "unchanged",
        artifact_count: None,
        payload_bytes: None,
        differences: check.then(Vec::new),
        diagnostics: Vec::new(),
        errors: Vec::new(),
    }
}

fn artifact_metrics(artifacts: &ExportArtifactSet) -> (u64, u64) {
    let artifact_count = u64::try_from(artifacts.artifacts().len())
        .expect("checked artifact count always fits in u64");
    let payload_bytes = artifacts.artifacts().iter().fold(0_u64, |total, artifact| {
        total
            .checked_add(
                u64::try_from(artifact.payload().as_bytes().len())
                    .expect("checked payload length always fits in u64"),
            )
            .expect("checked artifact-set payload budget fits in u64")
    });
    (artifact_count, payload_bytes)
}

fn render_blocked(
    selected_targets: &[ResolvedDeliveryTarget],
    check: bool,
    analysis: EmitAnalysis,
    reporter: Reporter,
    project_root: &Path,
) -> CliRunResult {
    let results = selected_targets
        .iter()
        .map(|target| target_result_shell(target, check))
        .collect();
    render_results(results, check, analysis, reporter, project_root)
}

fn render_results(
    results: Vec<EmitTargetResult>,
    check: bool,
    analysis: EmitAnalysis,
    reporter: Reporter,
    project_root: &Path,
) -> CliRunResult {
    let selected_targets = count(results.len());
    let prepared_targets = count(
        results
            .iter()
            .filter(|result| result.artifact_count.is_some())
            .count(),
    );
    let prepared_artifacts = checked_sum(results.iter().filter_map(|result| result.artifact_count));
    let prepared_payload_bytes =
        checked_sum(results.iter().filter_map(|result| result.payload_bytes));
    let blocked_targets = count(
        results
            .iter()
            .filter(|result| result.status == "blocked")
            .count(),
    );
    let error_targets = count(
        results
            .iter()
            .filter(|result| !result.errors.is_empty())
            .count(),
    );
    let error_count = checked_sum(results.iter().map(|result| count(result.errors.len())));
    let diagnostic_count = analysis
        .message_validation
        .as_ref()
        .map_or(0, |validation| validation.total_diagnostics);
    let finding_count = analysis.finding_count();
    let blocking_finding_count = analysis.blocking_finding_count();
    let has_difference = results.iter().any(|result| result.status == "different");
    let status = if error_count > 0 {
        "error"
    } else if analysis.generation_blocked || blocked_targets > 0 || has_difference {
        "failure"
    } else {
        "success"
    };
    let exit_code = if error_count > 0 {
        2
    } else {
        i32::from(status == "failure")
    };

    let summary = if check {
        EmitSummary::Check(CheckSummary {
            status,
            operation: "check",
            selected_targets,
            prepared_targets,
            prepared_artifacts,
            prepared_payload_bytes,
            blocked_targets,
            diagnostic_count,
            finding_count,
            blocking_finding_count,
            error_targets,
            error_count,
            matched_targets: count(
                results
                    .iter()
                    .filter(|result| result.status == "matched")
                    .count(),
            ),
            different_targets: count(
                results
                    .iter()
                    .filter(|result| result.status == "different")
                    .count(),
            ),
            difference_count: checked_sum(results.iter().filter_map(|result| {
                result
                    .differences
                    .as_ref()
                    .map(|differences| count(differences.len()))
            })),
        })
    } else {
        EmitSummary::Write(WriteSummary {
            status,
            operation: "write",
            selected_targets,
            prepared_targets,
            prepared_artifacts,
            prepared_payload_bytes,
            blocked_targets,
            diagnostic_count,
            finding_count,
            blocking_finding_count,
            error_targets,
            error_count,
            written_targets: count(
                results
                    .iter()
                    .filter(|result| result.status == "written")
                    .count(),
            ),
            unchanged_targets: count(
                results
                    .iter()
                    .filter(|result| result.status == "unchanged")
                    .count(),
            ),
        })
    };
    report::render(
        EmitReport {
            summary,
            analysis,
            results,
            errors: Vec::new(),
            exit_code,
        },
        reporter,
        project_root,
    )
}

fn render_reduced_error(
    selected_targets: &[ResolvedDeliveryTarget],
    check: bool,
    analysis: EmitAnalysis,
    errors: Vec<OperationalError>,
    reporter: Reporter,
    project_root: &Path,
) -> CliRunResult {
    let summary = EmitSummary::Reduced(ReducedSummary {
        status: "error",
        operation: if check { "check" } else { "write" },
        selected_targets: count(selected_targets.len()),
        prepared_targets: 0,
        prepared_artifacts: 0,
        prepared_payload_bytes: 0,
        diagnostic_count: 0,
        finding_count: analysis.finding_count(),
        blocking_finding_count: analysis.blocking_finding_count(),
        error_count: count(errors.len()),
    });
    report::render(
        EmitReport {
            summary,
            analysis,
            results: Vec::new(),
            errors,
            exit_code: 2,
        },
        reporter,
        project_root,
    )
}

fn source_errors(execution: &ProjectLinkExecution) -> Vec<OperationalError> {
    let mut errors = execution
        .definition_failures()
        .iter()
        .map(|failure| failure.error().clone())
        .collect::<Vec<_>>();
    errors.extend(
        execution
            .reference_failures()
            .iter()
            .map(reference_source_error),
    );
    errors
}

fn reference_source_error(failure: &ReferenceSourceFailure) -> OperationalError {
    match failure {
        ReferenceSourceFailure::Host { error, .. } => error.clone(),
        ReferenceSourceFailure::Identity { path, .. } => OperationalError {
            kind: "input",
            code: "input_path_unrepresentable",
            message: format!("A reference source path cannot be represented: {path}"),
            path: Some(path.clone()),
            details: Some(json!({
                "reason": "portable_path_invalid",
                "source": "project_inventory",
            })),
        },
        ReferenceSourceFailure::Producer { failure, .. } => producer_failure_error(failure),
        ReferenceSourceFailure::ExternalArtifact {
            host_primary,
            error,
            ..
        } => artifact_contract_error(error, &portable_path_string(host_primary)),
    }
}

fn producer_failure_error(failure: &JsProducerFailure) -> OperationalError {
    let path = portable_source_path(failure.source());
    let mut evidence = serde_json::Map::new();
    evidence.insert(
        "producer".to_owned(),
        Value::String("dev.intlify/js-reference".to_owned()),
    );
    evidence.insert(
        "stage".to_owned(),
        Value::String(failure.stage().as_str().to_owned()),
    );
    evidence.insert(
        "reason".to_owned(),
        Value::String(failure.reason().as_str().to_owned()),
    );
    evidence.insert("source".to_owned(), source_identity_value(failure.source()));
    if let Some(span) = failure.span() {
        evidence.insert(
            "span".to_owned(),
            json!({ "start": span.start(), "end": span.end() }),
        );
    }
    if let (Some(limit), Some(observed)) = (failure.limit(), failure.observed()) {
        evidence.insert("limit".to_owned(), Value::from(limit));
        evidence.insert("observed".to_owned(), Value::from(observed));
    }
    OperationalError {
        kind: "input",
        code: "message_artifact_failed",
        message: format!("JavaScript message-reference production failed: {path}"),
        path: Some(path),
        details: Some(json!({
            "kind": "producer_failed",
            "evidence": Value::Object(evidence),
        })),
    }
}

fn artifact_contract_error(error: &ArtifactContractError, path: &str) -> OperationalError {
    let (kind, evidence) = super::artifact_error::contract_error_parts(error);
    OperationalError {
        kind: "input",
        code: "message_artifact_failed",
        message: format!("Message reference artifact admission failed: {path}"),
        path: Some(path.to_owned()),
        details: Some(json!({ "kind": kind, "evidence": evidence })),
    }
}

fn project_link_error(error: &ProjectLinkError) -> OperationalError {
    match error {
        ProjectLinkError::DefinitionInventory(error) => error.operational_error().clone(),
        ProjectLinkError::ReferenceInventory(error) => reference_inventory_error(error),
        ProjectLinkError::Linker { error, .. } => linker_error(error),
    }
}

fn reference_inventory_error(error: &ReferenceInventoryError) -> OperationalError {
    match error {
        ReferenceInventoryError::Producer(JsProducerError::Failed(failure)) => {
            producer_failure_error(failure)
        }
        ReferenceInventoryError::Producer(JsProducerError::Cancelled) => OperationalError {
            kind: "internal",
            code: "internal_error",
            message: "JavaScript reference production was unexpectedly cancelled.".to_owned(),
            path: None,
            details: Some(json!({ "reason": "message_producer_invariant_failed" })),
        },
        ReferenceInventoryError::Producer(JsProducerError::InternalInvariant)
        | ReferenceInventoryError::InternalInvariant => OperationalError {
            kind: "internal",
            code: "internal_error",
            message: "Message reference inventory violated an invariant.".to_owned(),
            path: None,
            details: Some(json!({ "reason": "message_producer_invariant_failed" })),
        },
    }
}

fn linker_error(error: &LinkOperationalError) -> OperationalError {
    match error {
        LinkOperationalError::InternalInvariant => OperationalError {
            kind: "internal",
            code: "internal_error",
            message: "Message linker violated an invariant.".to_owned(),
            path: None,
            details: Some(json!({ "reason": "message_linker_invariant_failed" })),
        },
        LinkOperationalError::InvalidRequest(evidence) => link_boundary_error(
            error,
            "invalid_request",
            super::link_error::invalid_request_value(evidence),
        ),
        LinkOperationalError::UnsupportedContract(evidence) => link_boundary_error(
            error,
            "unsupported_contract",
            super::link_error::unsupported_contract_value(evidence),
        ),
        LinkOperationalError::Limit(evidence) => {
            link_boundary_error(error, "limit", super::link_error::limit_value(evidence))
        }
    }
}

fn link_boundary_error(
    error: &LinkOperationalError,
    kind: &'static str,
    evidence: Value,
) -> OperationalError {
    let details = Map::from_iter([
        ("kind".to_owned(), Value::String(kind.to_owned())),
        ("evidence".to_owned(), evidence),
    ]);
    OperationalError {
        kind: "input",
        code: "message_link_failed",
        message: error.to_string(),
        path: None,
        details: Some(Value::Object(details)),
    }
}

fn delivery_required_error() -> OperationalError {
    OperationalError {
        kind: "config",
        code: "config_validation_failed",
        message: "The messages emit command requires messages.delivery.targets.".to_owned(),
        path: None,
        details: Some(json!({
            "pointer": "/messages/delivery",
            "reason": "invalid_message_delivery",
        })),
    }
}

fn target_selection_error(
    error: &DeliveryTargetSelectionError,
    operand: Option<&str>,
) -> OperationalError {
    let reason = match error {
        DeliveryTargetSelectionError::InvalidOperand(_) => "invalid_target_name",
        DeliveryTargetSelectionError::UnknownTarget(_) => "unknown_target",
    };
    OperationalError {
        kind: "input",
        code: "invalid_cli_argument",
        message: "The selected delivery target is invalid or is not configured.".to_owned(),
        path: None,
        details: Some(json!({
            "reason": reason,
            "target": operand.unwrap_or_default(),
        })),
    }
}

fn source_identity_value(source: &intlify_contract::SourceDocumentIdentity) -> Value {
    json!({
        "namespace": { "kind": source.namespace().as_str() },
        "path": source.path().segments().iter().map(intlify_contract::PortablePathSegment::as_str).collect::<Vec<_>>(),
    })
}

fn portable_source_path(source: &intlify_contract::SourceDocumentIdentity) -> String {
    portable_path_string(source.path())
}

fn portable_path_string(path: &intlify_contract::PortableRelativePath) -> String {
    path.segments()
        .iter()
        .map(intlify_contract::PortablePathSegment::as_str)
        .collect::<Vec<_>>()
        .join("/")
}

fn count(value: usize) -> u64 {
    u64::try_from(value).expect("bounded command result count fits in u64")
}

fn checked_sum(values: impl IntoIterator<Item = u64>) -> u64 {
    values.into_iter().fold(0_u64, |total, value| {
        total
            .checked_add(value)
            .expect("bounded command result total fits in u64")
    })
}
