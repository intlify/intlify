// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use intlify_contract::{
    ArtifactNamespace, ArtifactVersion, CatalogKey, CatalogKeyDomain, CatalogScopeId,
    CatalogScopeName, EntryReference, EntryStructuralPath, FingerprintAlgorithm, FingerprintDigest,
    InputFingerprint, LinkLimits, Locale, MessageDefinition, MessageDefinitionArtifact,
    MessagePayload, MessageReference, MessageReferenceArtifact, MessageSelector,
    PortablePathSegment, PortableRelativePath, ProducerId, ProducerIdentity, ProducerRevision,
    ReferenceArtifactIdentity, ReferenceArtifactSegment, SourceDocumentIdentity,
};
#[cfg(feature = "benchmark")]
use intlify_export::benchmark::benchmark_prepare_export;
use intlify_export::{
    prepare_export, ExportArtifact, ExportArtifactFormatVersion, ExportArtifactKind,
    ExportArtifactMetadata, ExportArtifactPath, ExportArtifactPayload, ExportArtifactSet,
    ExportPreparationError, ExportValidationLimits, MappedMessageDiagnosticKind, PlatformExporter,
    ValidatedExportBatch,
};
use intlify_linker::{
    link, CoverageBaseline, DeliveryUnitGraph, DynamicReferenceMode, InputCompleteness,
    LinkOutcome, LinkPolicy, LinkRequest, PlacementPolicy, ScopeCompleteness,
    ScopeCompletenessTable, ScopeMappingTable,
};

const DOMAIN: CatalogKeyDomain = CatalogKeyDomain::JsonPointer;

fn scope() -> CatalogScopeId {
    CatalogScopeId::new(
        ArtifactNamespace::Project,
        CatalogScopeName::try_new("app").unwrap(),
    )
}

fn locale(value: &str) -> Locale {
    Locale::try_new(value).unwrap()
}

fn key(value: &str) -> CatalogKey {
    CatalogKey::try_new(DOMAIN, value).unwrap()
}

fn producer() -> ProducerIdentity {
    ProducerIdentity::new(
        ProducerId::try_new("dev.intlify/export-test").unwrap(),
        ProducerRevision::try_new("test").unwrap(),
    )
}

fn definition(key_value: &str, message: &str, occurrence: u32) -> MessageDefinition {
    MessageDefinition::try_new(
        scope(),
        DOMAIN,
        key(key_value),
        locale("en"),
        MessagePayload::try_new(message).unwrap(),
        EntryReference::new(EntryStructuralPath::try_new(key_value).unwrap(), occurrence),
    )
    .unwrap()
}

fn definition_artifact(
    source: &str,
    definitions: Vec<MessageDefinition>,
) -> MessageDefinitionArtifact {
    let path =
        PortableRelativePath::try_new(vec![PortablePathSegment::try_new(source).unwrap()]).unwrap();
    MessageDefinitionArtifact::try_new(
        ArtifactVersion::DRAFT_V0_1,
        producer(),
        SourceDocumentIdentity::new(ArtifactNamespace::Project, path),
        Vec::new(),
        InputFingerprint::new(
            FingerprintAlgorithm::Blake3_256,
            FingerprintDigest::new([0; 32]),
        ),
        definitions,
        &LinkLimits::default(),
    )
    .unwrap()
}

fn reference_artifact(keys: &[&str]) -> MessageReferenceArtifact {
    let references = keys
        .iter()
        .map(|value| {
            MessageReference::try_new(
                scope(),
                DOMAIN,
                MessageSelector::Exact(key(value)),
                None,
                None,
            )
            .unwrap()
        })
        .collect();
    MessageReferenceArtifact::try_new(
        ArtifactVersion::DRAFT_V0_1,
        producer(),
        ReferenceArtifactIdentity::try_new(
            ArtifactNamespace::Project,
            vec![ReferenceArtifactSegment::try_new("app").unwrap()],
        )
        .unwrap(),
        intlify_contract::DeliveryUnitId::main(),
        references,
        &LinkLimits::default(),
    )
    .unwrap()
}

// Keep the complete artifact fixtures alive for the borrowed request and link call.
#[allow(clippy::needless_pass_by_value)]
fn linked_outcome(
    definitions: Vec<MessageDefinitionArtifact>,
    reference_keys: &[&str],
    include_baseline: bool,
    include_main_unit: bool,
) -> LinkOutcome {
    let app = scope();
    let limits = LinkLimits::default();
    let references = (!reference_keys.is_empty())
        .then(|| reference_artifact(reference_keys))
        .into_iter()
        .collect::<Vec<_>>();
    let policy = LinkPolicy::try_new(
        vec![locale("en")],
        Vec::new(),
        Vec::new(),
        include_baseline
            .then(|| CoverageBaseline::new(app.clone(), locale("en")))
            .into_iter()
            .collect(),
        DynamicReferenceMode::Compat,
        PlacementPolicy::Duplicate,
        &limits,
    )
    .unwrap();
    let mappings = ScopeMappingTable::empty(std::slice::from_ref(&app), &limits).unwrap();
    let completeness = ScopeCompletenessTable::try_new(
        std::slice::from_ref(&app),
        vec![ScopeCompleteness::try_new(
            app.clone(),
            InputCompleteness::Closed,
            InputCompleteness::Closed,
        )
        .unwrap()],
        &limits,
    )
    .unwrap();
    let graph = if include_main_unit {
        DeliveryUnitGraph::single_main(&limits).unwrap()
    } else {
        DeliveryUnitGraph::try_new(Vec::new(), Vec::new(), &limits).unwrap()
    };
    let request = LinkRequest::try_new(
        &references,
        &definitions,
        &policy,
        &mappings,
        &completeness,
        &graph,
        &limits,
    )
    .unwrap();
    link(&request).unwrap()
}

struct ThirdPartyExporter {
    calls: std::cell::Cell<u8>,
}

impl PlatformExporter for ThirdPartyExporter {
    fn export(
        &self,
        _batch: &ValidatedExportBatch<'_>,
    ) -> Result<ExportArtifactSet, intlify_export::ExportError> {
        let call = self.calls.get() + 1;
        self.calls.set(call);
        ExportArtifactSet::try_new(vec![ExportArtifact::new(
            ExportArtifactPath::try_new(["custom.bundle"])?,
            ExportArtifactKind::try_new("com.example/custom-bundle")?,
            ExportArtifactFormatVersion::DRAFT_V0_1,
            ExportArtifactPayload::try_new(vec![call])?,
            ExportArtifactMetadata::empty(),
        )])
    }
}

#[test]
fn blocked_outcomes_skip_export_validation() {
    let outcome = linked_outcome(Vec::new(), &["/missing"], false, true);
    assert!(outcome.bundle_plans().is_none());
    assert!(prepare_export(&outcome, ExportValidationLimits::default())
        .unwrap()
        .is_none());
}

#[cfg(feature = "benchmark")]
#[test]
fn blocked_outcomes_produce_an_empty_benchmark_execution() {
    let outcome = linked_outcome(Vec::new(), &["/missing"], false, true);
    let execution = benchmark_prepare_export(&outcome, ExportValidationLimits::default()).unwrap();

    assert!(execution.batch().is_none());
    assert!(execution.measurements().is_empty());
}

#[test]
fn successful_empty_outcome_produces_an_empty_checked_batch() {
    let outcome = linked_outcome(Vec::new(), &[], false, false);
    let batch = prepare_export(&outcome, ExportValidationLimits::default())
        .unwrap()
        .unwrap();
    assert!(batch.plans().is_empty());
    assert!(batch.typed_output().is_empty());
}

#[test]
fn non_sync_third_party_exporter_uses_the_object_safe_common_contract() {
    let outcome = linked_outcome(Vec::new(), &[], false, false);
    let batch = prepare_export(&outcome, ExportValidationLimits::default())
        .unwrap()
        .unwrap();
    let exporter: Box<dyn PlatformExporter> = Box::new(ThirdPartyExporter {
        calls: std::cell::Cell::new(0),
    });

    let set = exporter.export(&batch).unwrap();
    assert_eq!(set.artifacts().len(), 1);
    assert_eq!(set.artifacts()[0].payload().as_bytes(), &[1]);
    assert_eq!(
        set.artifacts()[0].kind().as_str(),
        "com.example/custom-bundle"
    );
}

#[test]
fn delivery_without_a_coverage_baseline_has_no_typed_output() {
    let outcome = linked_outcome(
        vec![definition_artifact(
            "en.json",
            vec![definition("/hello", "Hello", 0)],
        )],
        &["/hello"],
        false,
        true,
    );
    let batch = prepare_export(&outcome, ExportValidationLimits::default())
        .unwrap()
        .unwrap();
    assert_eq!(batch.plans().len(), 1);
    assert!(batch.typed_output().is_empty());
}

#[test]
fn plan_slice_uses_the_outcome_lifetime_not_the_batch_borrow() {
    let outcome = linked_outcome(
        vec![definition_artifact(
            "en.json",
            vec![definition("/hello", "Hello", 0)],
        )],
        &["/hello"],
        false,
        true,
    );
    let plans = {
        let batch = prepare_export(&outcome, ExportValidationLimits::default())
            .unwrap()
            .unwrap();
        batch.plans()
    };

    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].messages()[0].key().as_str(), "/hello");
}

#[test]
fn valid_baseline_derives_canonical_required_arguments() {
    let message = ".input {$count :number}\n.input {$plain}\n.input {$unused :string}\n.local $label = {$external}\n{{{$count} {$label} {$external} {$名前}}}";
    let outcome = linked_outcome(
        vec![definition_artifact(
            "en.json",
            vec![
                definition("/hello", message, 0),
                definition("/plain", "Hello", 0),
            ],
        )],
        &["/hello", "/plain"],
        true,
        true,
    );
    let batch = prepare_export(&outcome, ExportValidationLimits::default())
        .unwrap()
        .unwrap();

    assert_eq!(batch.plans().len(), 1);
    assert_eq!(batch.typed_output().len(), 1);
    let signatures = batch.typed_output()[0].messages();
    assert_eq!(signatures.len(), 2);
    assert_eq!(signatures[0].key().as_str(), "/hello");
    assert_eq!(
        signatures[0]
            .arguments()
            .iter()
            .map(|argument| (argument.name(), argument.input_annotation()))
            .collect::<Vec<_>>(),
        [
            ("count", Some("number")),
            ("external", None),
            ("plain", None),
            ("unused", Some("string")),
            ("名前", None),
        ]
    );
    assert_eq!(signatures[1].key().as_str(), "/plain");
    assert!(signatures[1].arguments().is_empty());

    let repeated = prepare_export(&outcome, ExportValidationLimits::default())
        .unwrap()
        .unwrap();
    assert_eq!(
        repeated.typed_output()[0].messages()[0]
            .arguments()
            .iter()
            .map(|argument| (argument.name(), argument.input_annotation()))
            .collect::<Vec<_>>(),
        [
            ("count", Some("number")),
            ("external", None),
            ("plain", None),
            ("unused", Some("string")),
            ("名前", None),
        ]
    );
}

#[test]
fn empty_plan_set_still_prepares_admitted_typed_output() {
    let outcome = linked_outcome(
        vec![definition_artifact(
            "en.json",
            vec![definition("/unused", "Hello", 0)],
        )],
        &[],
        true,
        false,
    );
    assert!(matches!(outcome.bundle_plans(), Some(plans) if plans.is_empty()));

    let batch = prepare_export(&outcome, ExportValidationLimits::default())
        .unwrap()
        .unwrap();
    assert!(batch.plans().is_empty());
    assert_eq!(batch.typed_output().len(), 1);
    assert_eq!(batch.typed_output()[0].messages().len(), 1);
}

#[test]
fn parser_and_semantic_diagnostics_keep_their_owned_categories() {
    let parser_outcome = linked_outcome(
        vec![definition_artifact(
            "parser.json",
            vec![definition("/parser", "Hello {$name", 0)],
        )],
        &["/parser"],
        true,
        true,
    );
    let ExportPreparationError::MessageValidation(parser_failure) =
        prepare_export(&parser_outcome, ExportValidationLimits::default()).unwrap_err()
    else {
        panic!("expected parser diagnostics");
    };
    assert!(parser_failure.total_diagnostics() > 0);
    assert!(parser_failure
        .diagnostics()
        .iter()
        .all(|diagnostic| diagnostic.kind().category() == "parser"));

    let semantic_outcome = linked_outcome(
        vec![definition_artifact(
            "semantic.json",
            vec![definition(
                "/semantic",
                ".input {$x :string}\n.input {$x :number}\n{{ok}}",
                0,
            )],
        )],
        &["/semantic"],
        true,
        true,
    );
    let ExportPreparationError::MessageValidation(semantic_failure) =
        prepare_export(&semantic_outcome, ExportValidationLimits::default()).unwrap_err()
    else {
        panic!("expected semantic diagnostics");
    };
    assert_eq!(semantic_failure.total_diagnostics(), 1);
    assert!(matches!(
        semantic_failure.diagnostics()[0].kind(),
        MappedMessageDiagnosticKind::Semantic(_)
    ));
    assert_eq!(
        semantic_failure.diagnostics()[0].kind().code(),
        "duplicate-declaration"
    );
}

#[test]
fn validation_deduplicates_roles_by_location_but_not_equal_payload_text() {
    let delivery_only = linked_outcome(
        vec![definition_artifact(
            "single.json",
            vec![definition("/one", "Hello {$name", 0)],
        )],
        &["/one"],
        false,
        true,
    );
    let ExportPreparationError::MessageValidation(delivery_only_failure) =
        prepare_export(&delivery_only, ExportValidationLimits::default()).unwrap_err()
    else {
        panic!("expected one invalid delivery definition");
    };

    let delivery_and_baseline = linked_outcome(
        vec![definition_artifact(
            "single.json",
            vec![definition("/one", "Hello {$name", 0)],
        )],
        &["/one"],
        true,
        true,
    );
    let ExportPreparationError::MessageValidation(overlap_failure) =
        prepare_export(&delivery_and_baseline, ExportValidationLimits::default()).unwrap_err()
    else {
        panic!("expected one overlapping invalid definition");
    };
    assert_eq!(overlap_failure, delivery_only_failure);

    let duplicate_text = linked_outcome(
        vec![definition_artifact(
            "double.json",
            vec![
                definition("/one", "Hello {$name", 0),
                definition("/two", "Hello {$name", 0),
            ],
        )],
        &["/one", "/two"],
        true,
        true,
    );
    let ExportPreparationError::MessageValidation(double_failure) =
        prepare_export(&duplicate_text, ExportValidationLimits::default()).unwrap_err()
    else {
        panic!("expected two invalid definitions");
    };
    assert_eq!(
        double_failure.total_diagnostics(),
        overlap_failure.total_diagnostics() * 2
    );
}

#[test]
fn zero_retention_still_counts_the_complete_validation_set() {
    let outcome = linked_outcome(
        vec![definition_artifact(
            "invalid.json",
            vec![
                definition("/one", "Hello {$name", 0),
                definition("/two", "Hello {$name", 0),
            ],
        )],
        &["/one", "/two"],
        true,
        true,
    );
    let limits = ExportValidationLimits::default()
        .try_with_diagnostic_retention(0)
        .unwrap();
    let ExportPreparationError::MessageValidation(failure) =
        prepare_export(&outcome, limits).unwrap_err()
    else {
        panic!("expected bounded diagnostics");
    };
    assert!(failure.diagnostics().is_empty());
    assert!(failure.total_diagnostics() >= 2);
    assert!(failure.truncated());
}

#[test]
fn retained_diagnostics_are_the_canonical_definition_location_prefix() {
    let outcome = linked_outcome(
        vec![
            definition_artifact("z.json", vec![definition("/z", "Hello {$name", 0)]),
            definition_artifact("a.json", vec![definition("/a", "Hello {$name", 0)]),
        ],
        &["/z", "/a"],
        true,
        true,
    );
    let limits = ExportValidationLimits::default()
        .try_with_diagnostic_retention(1)
        .unwrap();
    let ExportPreparationError::MessageValidation(failure) =
        prepare_export(&outcome, limits).unwrap_err()
    else {
        panic!("expected bounded diagnostics");
    };
    assert_eq!(failure.diagnostics().len(), 1);
    assert!(failure.total_diagnostics() >= 2);
    assert!(failure.truncated());
    assert_eq!(
        failure.diagnostics()[0]
            .definition()
            .source()
            .path()
            .segments()[0]
            .as_str(),
        "a.json"
    );
}
