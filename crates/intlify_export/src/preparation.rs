// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Complete validation gate and typed-output construction for one link outcome.

use std::collections::{BTreeMap, BTreeSet};

use intlify_contract::{CatalogKey, CatalogKeyDomain, Locale, MessagePayload};
use intlify_linker::export_preparation_handoff::{BaselineDefinitionView, ExportPreparationView};
use intlify_linker::{
    DefinitionLocation, LinkOutcome, MessageBundlePlan, ResolvedCatalogScopeId, ResolvedMessage,
};
use ox_mf2_parser::{build_semantic_model, parse_message, validate_semantics, SemanticModel};

use crate::diagnostic::{map_parser_diagnostic, map_semantic_diagnostic};
use crate::typed_output::derive_arguments;
use crate::{
    ExportMessageValidationFailure, ExportPreparationError, ExportPreparationInvariant,
    ExportValidationLimits, OutcomeContractInvariant, TypedOutputInvariant, ValidatedExportBatch,
    ValidatedMessageSignature, ValidatedTypedOutput,
};

/// Validate one complete checked outcome and prepare the shared exporter input.
pub fn prepare_export(
    outcome: &LinkOutcome,
    limits: ExportValidationLimits,
) -> Result<Option<ValidatedExportBatch<'_>>, ExportPreparationError> {
    let Some(view) = outcome.export_preparation_view() else {
        return Ok(None);
    };

    preflight_plans(view.plans()).map_err(internal_outcome)?;
    let (validation_set, baseline_locations) =
        collect_validation_set(view).map_err(internal_outcome)?;
    let (failure, semantic_models) = validate_messages(
        &validation_set,
        &baseline_locations,
        limits.diagnostic_retention(),
    )?;
    if let Some(failure) = failure {
        return Err(ExportPreparationError::MessageValidation(failure));
    }

    let typed_output = build_typed_output(view, &semantic_models).map_err(internal_typed)?;
    Ok(Some(ValidatedExportBatch::new(
        outcome,
        view.plans(),
        typed_output,
    )))
}

fn internal_outcome(invariant: OutcomeContractInvariant) -> ExportPreparationError {
    ExportPreparationError::InternalInvariant(ExportPreparationInvariant::OutcomeContract(
        invariant,
    ))
}

fn internal_typed(invariant: TypedOutputInvariant) -> ExportPreparationError {
    ExportPreparationError::InternalInvariant(ExportPreparationInvariant::TypedOutput(invariant))
}

fn preflight_plans(plans: &[MessageBundlePlan]) -> Result<(), OutcomeContractInvariant> {
    let mut coordinates = BTreeSet::new();
    for plan in plans {
        if !coordinates.insert((plan.delivery_unit(), plan.locale())) {
            return Err(OutcomeContractInvariant::DuplicatePlanCoordinate);
        }
    }
    if plans.windows(2).any(|pair| {
        (pair[0].delivery_unit(), pair[0].locale()) > (pair[1].delivery_unit(), pair[1].locale())
    }) {
        return Err(OutcomeContractInvariant::NonCanonicalPlanOrder);
    }

    for plan in plans {
        let mut identities = BTreeSet::new();
        for message in plan.messages() {
            if !identities.insert(message_identity(message)) {
                return Err(OutcomeContractInvariant::DuplicateLogicalMessage);
            }
        }
    }
    if plans.iter().any(|plan| {
        plan.messages()
            .windows(2)
            .any(|pair| message_identity(&pair[0]) > message_identity(&pair[1]))
    }) {
        return Err(OutcomeContractInvariant::NonCanonicalMessageOrder);
    }
    Ok(())
}

fn message_identity(
    message: &ResolvedMessage,
) -> (&ResolvedCatalogScopeId, CatalogKeyDomain, &CatalogKey) {
    (message.resolved_scope(), *message.domain(), message.key())
}

#[derive(Clone, Copy)]
struct ValidationDefinition<'a> {
    resolved_scope: &'a ResolvedCatalogScopeId,
    domain: CatalogKeyDomain,
    key: &'a CatalogKey,
    locale: &'a Locale,
    message: &'a MessagePayload,
}

impl ValidationDefinition<'_> {
    fn matches(self, other: Self) -> bool {
        self.resolved_scope == other.resolved_scope
            && self.domain == other.domain
            && self.key == other.key
            && self.locale == other.locale
            && self.message == other.message
    }
}

fn collect_validation_set(
    view: ExportPreparationView<'_>,
) -> Result<
    (
        BTreeMap<DefinitionLocation, ValidationDefinition<'_>>,
        BTreeSet<DefinitionLocation>,
    ),
    OutcomeContractInvariant,
> {
    let mut definitions = BTreeMap::new();
    for plan in view.plans() {
        for message in plan.messages() {
            insert_definition(
                &mut definitions,
                message.definition(),
                ValidationDefinition {
                    resolved_scope: message.resolved_scope(),
                    domain: *message.domain(),
                    key: message.key(),
                    locale: message.definition_locale(),
                    message: message.message(),
                },
            )?;
        }
    }

    let mut baseline_locations = BTreeSet::new();
    for baseline in view.typed_key_baselines() {
        for definition in baseline.definitions() {
            baseline_locations.insert(definition.location().clone());
            insert_definition(
                &mut definitions,
                definition.location(),
                baseline_validation_definition(definition),
            )?;
        }
    }
    Ok((definitions, baseline_locations))
}

fn baseline_validation_definition(
    definition: BaselineDefinitionView<'_>,
) -> ValidationDefinition<'_> {
    ValidationDefinition {
        resolved_scope: definition.resolved_scope(),
        domain: definition.key().domain(),
        key: definition.key(),
        locale: definition.locale(),
        message: definition.message(),
    }
}

fn insert_definition<'a>(
    definitions: &mut BTreeMap<DefinitionLocation, ValidationDefinition<'a>>,
    location: &DefinitionLocation,
    definition: ValidationDefinition<'a>,
) -> Result<(), OutcomeContractInvariant> {
    if let Some(previous) = definitions.get(location) {
        if !previous.matches(definition) {
            return Err(OutcomeContractInvariant::DefinitionSnapshotMismatch);
        }
        return Ok(());
    }
    definitions.insert(location.clone(), definition);
    Ok(())
}

fn validate_messages(
    definitions: &BTreeMap<DefinitionLocation, ValidationDefinition<'_>>,
    baseline_locations: &BTreeSet<DefinitionLocation>,
    retention: u32,
) -> Result<
    (
        Option<ExportMessageValidationFailure>,
        BTreeMap<DefinitionLocation, SemanticModel>,
    ),
    ExportPreparationError,
> {
    let retention = retention as usize;
    let mut retained = Vec::with_capacity(retention.min(definitions.len()));
    let mut total = 0u64;
    let mut baseline_models = BTreeMap::new();

    for (location, definition) in definitions {
        let parsed = parse_message(definition.message.as_str()).map_err(|_| {
            ExportPreparationError::InternalInvariant(ExportPreparationInvariant::ParserFailure)
        })?;
        let payload_len = u32::try_from(definition.message.as_str().len())
            .map_err(|_| internal_mapping_count())?;
        if !parsed.result().diagnostics.is_empty() {
            for diagnostic in &parsed.result().diagnostics {
                let mapped = map_parser_diagnostic(location, payload_len, diagnostic)
                    .map_err(internal_mapping)?;
                retain_diagnostic(mapped, retention, &mut retained, &mut total)?;
            }
            continue;
        }

        let model = build_semantic_model(parsed.sources(), parsed.result())
            .map_err(ExportPreparationError::SemanticModelConstruction)?;
        let diagnostics =
            validate_semantics(&model).map_err(ExportPreparationError::SemanticValidation)?;
        if diagnostics.is_empty() {
            if baseline_locations.contains(location) {
                baseline_models.insert(location.clone(), model);
            }
            continue;
        }
        for diagnostic in &diagnostics {
            let mapped = map_semantic_diagnostic(location, payload_len, diagnostic)
                .map_err(internal_mapping)?;
            retain_diagnostic(mapped, retention, &mut retained, &mut total)?;
        }
    }

    let failure = if total == 0 {
        None
    } else {
        Some(
            ExportMessageValidationFailure::checked(retained, total)
                .map_err(ExportPreparationError::InternalInvariant)?,
        )
    };
    Ok((failure, baseline_models))
}

fn internal_mapping(invariant: crate::DiagnosticMappingInvariant) -> ExportPreparationError {
    ExportPreparationError::InternalInvariant(ExportPreparationInvariant::DiagnosticMapping(
        invariant,
    ))
}

fn internal_mapping_count() -> ExportPreparationError {
    ExportPreparationError::InternalInvariant(ExportPreparationInvariant::DiagnosticCountOverflow)
}

fn retain_diagnostic(
    diagnostic: crate::MappedMessageDiagnostic,
    retention: usize,
    retained: &mut Vec<crate::MappedMessageDiagnostic>,
    total: &mut u64,
) -> Result<(), ExportPreparationError> {
    *total = total.checked_add(1).ok_or_else(internal_mapping_count)?;
    if retained.len() < retention {
        retained.push(diagnostic);
    }
    Ok(())
}

fn build_typed_output<'a>(
    view: ExportPreparationView<'a>,
    semantic_models: &BTreeMap<DefinitionLocation, SemanticModel>,
) -> Result<Vec<ValidatedTypedOutput<'a>>, TypedOutputInvariant> {
    preflight_model_relation(view)?;
    let mut output = Vec::with_capacity(view.typed_key_baselines().len());

    for baseline in view.typed_key_baselines() {
        let mut messages = Vec::with_capacity(baseline.model().keys().len());
        for (key, definition) in baseline.model().keys().iter().zip(baseline.definitions()) {
            if key != definition.key() {
                return Err(TypedOutputInvariant::SignatureKeyMismatch);
            }
            let model = semantic_models
                .get(definition.location())
                .ok_or(TypedOutputInvariant::ModelRelationMismatch)?;
            let arguments = derive_arguments(model);
            validate_arguments(&arguments)?;
            messages.push(ValidatedMessageSignature {
                key,
                arguments: arguments.into_boxed_slice(),
            });
        }
        if messages.len() != baseline.model().keys().len()
            || messages
                .iter()
                .zip(baseline.model().keys())
                .any(|(message, key)| message.key != key)
        {
            return Err(TypedOutputInvariant::SignatureKeyMismatch);
        }
        output.push(ValidatedTypedOutput {
            model: baseline.model(),
            messages: messages.into_boxed_slice(),
        });
    }
    Ok(output)
}

fn preflight_model_relation(view: ExportPreparationView<'_>) -> Result<(), TypedOutputInvariant> {
    if !view.typed_key_baseline_relation_is_complete() {
        return Err(TypedOutputInvariant::ModelRelationMismatch);
    }
    let mut previous_scope = None;
    for baseline in view.typed_key_baselines() {
        let scope = baseline.model().resolved_scope();
        if previous_scope.is_some_and(|previous| previous >= scope)
            || baseline
                .model()
                .keys()
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || baseline.model().keys().len() != baseline.definitions().len()
        {
            return Err(TypedOutputInvariant::ModelRelationMismatch);
        }
        previous_scope = Some(scope);
        for (key, definition) in baseline.model().keys().iter().zip(baseline.definitions()) {
            if definition.resolved_scope() != scope
                || definition.key() != key
                || definition.locale() != baseline.baseline_locale()
            {
                return Err(TypedOutputInvariant::ModelRelationMismatch);
            }
        }
    }
    Ok(())
}

fn validate_arguments(
    arguments: &[crate::MessageArgumentSignature],
) -> Result<(), TypedOutputInvariant> {
    if arguments
        .windows(2)
        .any(|pair| pair[0].name() == pair[1].name())
    {
        return Err(TypedOutputInvariant::DuplicateArgument);
    }
    if arguments
        .windows(2)
        .any(|pair| pair[0].name().as_bytes() > pair[1].name().as_bytes())
    {
        return Err(TypedOutputInvariant::NonCanonicalArgumentOrder);
    }
    Ok(())
}
