// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Complete validation gate and typed-output construction for one link outcome.

use std::collections::{BTreeMap, BTreeSet};

use intlify_contract::{CatalogKey, CatalogKeyDomain, Locale, MessagePayload};
use intlify_linker::export_preparation_handoff::{BaselineDefinitionView, ExportPreparationView};
use intlify_linker::{
    DefinitionLocation, LinkOutcome, MessageBundlePlan, ResolvedCatalogScopeId, ResolvedMessage,
};
use ox_mf2_parser::{
    build_semantic_model, parse_message, validate_semantics, DeclarationKind, DiagnosticSeverity,
    SemanticDiagnostic, SemanticModel, StandaloneParseResult,
};

use crate::diagnostic::{map_parser_diagnostic, map_semantic_diagnostic};
use crate::observation::{
    finish_observation, observation_checksum, ExportBenchmarkObserver, ExportBenchmarkStage,
    NoopExportBenchmarkObserver,
};
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
    prepare_export_with_observer(outcome, limits, &mut NoopExportBenchmarkObserver)
}

pub(crate) fn prepare_export_with_observer<'a, O>(
    outcome: &'a LinkOutcome,
    limits: ExportValidationLimits,
    observer: &mut O,
) -> Result<Option<ValidatedExportBatch<'a>>, ExportPreparationError>
where
    O: ExportBenchmarkObserver,
{
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
        observer,
    )?;
    if let Some(failure) = failure {
        return Err(ExportPreparationError::MessageValidation(failure));
    }

    observer.begin(ExportBenchmarkStage::ArgumentSignatureDerivation);
    let typed_output = build_typed_output(view, &semantic_models).map_err(internal_typed)?;
    finish_observation(
        observer,
        ExportBenchmarkStage::ArgumentSignatureDerivation,
        || checksum_typed_output(view, &typed_output),
    );

    observer.begin(ExportBenchmarkStage::ValidatedExportBatchConstruction);
    let batch = ValidatedExportBatch::new(outcome, view.plans(), typed_output);
    finish_observation(
        observer,
        ExportBenchmarkStage::ValidatedExportBatchConstruction,
        || checksum_validated_batch(&batch),
    );
    Ok(Some(batch))
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
    observer: &mut impl ExportBenchmarkObserver,
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
    let mut parsed_definitions = BTreeMap::new();

    observer.begin(ExportBenchmarkStage::SelectedMessageParse);
    for (location, definition) in definitions {
        let parsed = parse_message(definition.message.as_str()).map_err(|_| {
            ExportPreparationError::InternalInvariant(ExportPreparationInvariant::ParserFailure)
        })?;
        let payload_len = u32::try_from(definition.message.as_str().len())
            .map_err(|_| internal_mapping_count())?;
        parsed_definitions.insert(
            location.clone(),
            ParsedDefinition {
                parsed,
                payload_len,
            },
        );
    }
    finish_observation(observer, ExportBenchmarkStage::SelectedMessageParse, || {
        checksum_parsed_definitions(definitions, &parsed_definitions)
    });

    let mut semantic_diagnostics = BTreeMap::new();
    let mut semantic_models = BTreeMap::new();
    observer.begin(ExportBenchmarkStage::MessageSemanticValidation);
    for (location, parsed) in &parsed_definitions {
        if !parsed.parsed.result().diagnostics.is_empty() {
            continue;
        }
        let model = build_semantic_model(parsed.parsed.sources(), parsed.parsed.result())
            .map_err(ExportPreparationError::SemanticModelConstruction)?;
        let diagnostics =
            validate_semantics(&model).map_err(ExportPreparationError::SemanticValidation)?;
        semantic_diagnostics.insert(location.clone(), diagnostics);
        semantic_models.insert(location.clone(), model);
    }
    finish_observation(
        observer,
        ExportBenchmarkStage::MessageSemanticValidation,
        || checksum_semantic_validation(&semantic_models, &semantic_diagnostics),
    );

    observer.begin(ExportBenchmarkStage::PortableDiagnosticMapping);
    for (location, parsed) in &parsed_definitions {
        if !parsed.parsed.result().diagnostics.is_empty() {
            for diagnostic in &parsed.parsed.result().diagnostics {
                let mapped = map_parser_diagnostic(location, parsed.payload_len, diagnostic)
                    .map_err(internal_mapping)?;
                retain_diagnostic(mapped, retention, &mut retained, &mut total)?;
            }
            continue;
        }
        for diagnostic in semantic_diagnostics
            .get(location)
            .ok_or_else(|| internal_typed(TypedOutputInvariant::ModelRelationMismatch))?
        {
            let mapped = map_semantic_diagnostic(location, parsed.payload_len, diagnostic)
                .map_err(internal_mapping)?;
            retain_diagnostic(mapped, retention, &mut retained, &mut total)?;
        }
    }

    let mapped_checksum = observer
        .enabled()
        .then(|| checksum_mapped_diagnostics(&retained, total));
    let failure = if total == 0 {
        None
    } else {
        Some(
            ExportMessageValidationFailure::checked(retained, total)
                .map_err(ExportPreparationError::InternalInvariant)?,
        )
    };
    finish_observation(
        observer,
        ExportBenchmarkStage::PortableDiagnosticMapping,
        || mapped_checksum.expect("enabled observer has a mapped-diagnostic checksum"),
    );

    semantic_models.retain(|location, _| {
        baseline_locations.contains(location)
            && semantic_diagnostics
                .get(location)
                .is_some_and(Vec::is_empty)
    });
    Ok((failure, semantic_models))
}

struct ParsedDefinition {
    parsed: StandaloneParseResult,
    payload_len: u32,
}

fn checksum_parsed_definitions(
    definitions: &BTreeMap<DefinitionLocation, ValidationDefinition<'_>>,
    parsed: &BTreeMap<DefinitionLocation, ParsedDefinition>,
) -> u32 {
    let mut fields = Vec::new();
    push_count(&mut fields, definitions.len());
    for (location, definition) in definitions {
        push_definition_location(&mut fields, location);
        push_resolved_scope(&mut fields, definition.resolved_scope);
        fields.push(definition.domain.as_str().as_bytes().to_vec());
        fields.push(definition.key.as_str().as_bytes().to_vec());
        fields.push(definition.locale.as_str().as_bytes().to_vec());
        fields.push(definition.message.as_str().as_bytes().to_vec());
        let parsed = parsed
            .get(location)
            .expect("every validation definition has a parsed result");
        fields.push(parsed.payload_len.to_be_bytes().to_vec());
        push_count(&mut fields, parsed.parsed.result().diagnostics.len());
        for diagnostic in &parsed.parsed.result().diagnostics {
            fields.push(diagnostic.code.json_code().as_bytes().to_vec());
            fields.push(severity_token(diagnostic.severity).as_bytes().to_vec());
            push_span(&mut fields, diagnostic.span.start, diagnostic.span.end);
            push_count(&mut fields, diagnostic.labels.len());
            for label in &diagnostic.labels {
                push_span(&mut fields, label.span.start, label.span.end);
            }
        }
    }
    observation_checksum(b"selected_message_parse", fields.iter().map(Vec::as_slice))
}

fn checksum_semantic_validation(
    models: &BTreeMap<DefinitionLocation, SemanticModel>,
    diagnostics: &BTreeMap<DefinitionLocation, Vec<SemanticDiagnostic>>,
) -> u32 {
    let mut fields = Vec::new();
    push_count(&mut fields, diagnostics.len());
    for (location, values) in diagnostics {
        push_definition_location(&mut fields, location);
        push_count(&mut fields, values.len());
        for diagnostic in values {
            fields.push(diagnostic.code().json_code().as_bytes().to_vec());
            fields.push(severity_token(diagnostic.severity()).as_bytes().to_vec());
            fields.push(diagnostic.span().start.to_be_bytes().to_vec());
            fields.push(diagnostic.span().end.to_be_bytes().to_vec());
            push_count(&mut fields, diagnostic.labels().len());
            for label in diagnostic.labels() {
                push_span(&mut fields, label.span.start, label.span.end);
            }
        }
        if let Some(model) = models.get(location) {
            fields.push(b"model-present".to_vec());
            push_semantic_model(&mut fields, model);
        } else {
            fields.push(b"model-absent".to_vec());
        }
    }
    observation_checksum(
        b"message_semantic_validation",
        fields.iter().map(Vec::as_slice),
    )
}

fn checksum_mapped_diagnostics(diagnostics: &[crate::MappedMessageDiagnostic], total: u64) -> u32 {
    let mut fields = vec![total.to_be_bytes().to_vec()];
    push_count(&mut fields, diagnostics.len());
    for diagnostic in diagnostics {
        push_definition_location(&mut fields, diagnostic.definition());
        fields.push(diagnostic.kind().category().as_bytes().to_vec());
        fields.push(diagnostic.kind().code().as_bytes().to_vec());
        fields.push(severity_token(diagnostic.severity()).as_bytes().to_vec());
        push_span(
            &mut fields,
            diagnostic.span().start(),
            diagnostic.span().end(),
        );
        push_count(&mut fields, diagnostic.labels().len());
        for label in diagnostic.labels() {
            push_span(&mut fields, label.span().start(), label.span().end());
        }
    }
    observation_checksum(
        b"portable_diagnostic_mapping",
        fields.iter().map(Vec::as_slice),
    )
}

fn checksum_typed_output(
    view: ExportPreparationView<'_>,
    output: &[ValidatedTypedOutput<'_>],
) -> u32 {
    let mut fields = Vec::new();
    push_count(&mut fields, output.len());
    for (typed, baseline) in output.iter().zip(view.typed_key_baselines()) {
        push_resolved_scope(&mut fields, typed.model().resolved_scope());
        push_count(&mut fields, typed.messages().len());
        for (message, definition) in typed.messages().iter().zip(baseline.definitions()) {
            push_catalog_key(&mut fields, message.key());
            push_definition_location(&mut fields, definition.location());
            push_count(&mut fields, message.arguments().len());
            for argument in message.arguments() {
                fields.push(argument.name().as_bytes().to_vec());
                push_optional_str(&mut fields, argument.input_annotation());
            }
        }
    }
    observation_checksum(
        b"argument_signature_derivation",
        fields.iter().map(Vec::as_slice),
    )
}

fn checksum_validated_batch(batch: &ValidatedExportBatch<'_>) -> u32 {
    let mut fields = Vec::new();
    push_count(&mut fields, batch.plans().len());
    for plan in batch.plans() {
        push_delivery_unit(&mut fields, plan.delivery_unit());
        fields.push(plan.locale().as_str().as_bytes().to_vec());
        push_count(&mut fields, plan.messages().len());
        for message in plan.messages() {
            push_resolved_scope(&mut fields, message.resolved_scope());
            push_catalog_key(&mut fields, message.key());
            fields.push(message.definition_locale().as_str().as_bytes().to_vec());
            fields.push(message.message().as_str().as_bytes().to_vec());
            push_definition_location(&mut fields, message.definition());
        }
    }
    push_count(&mut fields, batch.typed_output().len());
    for typed in batch.typed_output() {
        push_resolved_scope(&mut fields, typed.model().resolved_scope());
        push_count(&mut fields, typed.messages().len());
        for message in typed.messages() {
            push_catalog_key(&mut fields, message.key());
            push_count(&mut fields, message.arguments().len());
            for argument in message.arguments() {
                fields.push(argument.name().as_bytes().to_vec());
                push_optional_str(&mut fields, argument.input_annotation());
            }
        }
    }
    observation_checksum(
        b"validated_export_batch_construction",
        fields.iter().map(Vec::as_slice),
    )
}

fn push_semantic_model(fields: &mut Vec<Vec<u8>>, model: &SemanticModel) {
    push_count(fields, model.semantic_declarations().len());
    for declaration in model.semantic_declarations() {
        fields.push(
            match declaration.kind() {
                DeclarationKind::Input => b"input".as_slice(),
                DeclarationKind::Local => b"local".as_slice(),
            }
            .to_vec(),
        );
        fields.push(declaration.name().as_bytes().to_vec());
        push_optional_str(fields, declaration.input_annotation());
    }
    push_count(fields, model.semantic_references().len());
    for reference in model.semantic_references() {
        fields.push(reference.name().as_bytes().to_vec());
        fields.push(
            if reference.resolved_declaration().is_some() {
                b"resolved"
            } else {
                b"external"
            }
            .to_vec(),
        );
    }
}

fn push_resolved_scope(fields: &mut Vec<Vec<u8>>, scope: &ResolvedCatalogScopeId) {
    let scope = scope.as_catalog_scope();
    fields.push(scope.namespace().as_str().as_bytes().to_vec());
    fields.push(scope.name().as_str().as_bytes().to_vec());
}

fn push_catalog_key(fields: &mut Vec<Vec<u8>>, key: &CatalogKey) {
    fields.push(key.domain().as_str().as_bytes().to_vec());
    fields.push(key.as_str().as_bytes().to_vec());
}

fn push_definition_location(fields: &mut Vec<Vec<u8>>, location: &DefinitionLocation) {
    fields.push(location.source().namespace().as_str().as_bytes().to_vec());
    push_count(fields, location.source().path().segments().len());
    for segment in location.source().path().segments() {
        fields.push(segment.as_str().as_bytes().to_vec());
    }
    fields.push(
        location
            .entry()
            .structural_path()
            .as_str()
            .as_bytes()
            .to_vec(),
    );
    fields.push(location.entry().occurrence().to_be_bytes().to_vec());
}

fn push_delivery_unit(fields: &mut Vec<Vec<u8>>, delivery_unit: &intlify_contract::DeliveryUnitId) {
    push_count(fields, delivery_unit.segments().len());
    for segment in delivery_unit.segments() {
        fields.push(segment.as_str().as_bytes().to_vec());
    }
}

fn push_optional_str(fields: &mut Vec<Vec<u8>>, value: Option<&str>) {
    if let Some(value) = value {
        fields.push(b"present".to_vec());
        fields.push(value.as_bytes().to_vec());
    } else {
        fields.push(b"absent".to_vec());
    }
}

fn push_span(fields: &mut Vec<Vec<u8>>, start: u32, end: u32) {
    fields.push(start.to_be_bytes().to_vec());
    fields.push(end.to_be_bytes().to_vec());
}

fn push_count(fields: &mut Vec<Vec<u8>>, count: usize) {
    fields.push(
        u64::try_from(count)
            .expect("supported observation count fits u64")
            .to_be_bytes()
            .to_vec(),
    );
}

const fn severity_token(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => "error",
        DiagnosticSeverity::Warning => "warning",
        DiagnosticSeverity::Information => "information",
        DiagnosticSeverity::Hint => "hint",
    }
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
