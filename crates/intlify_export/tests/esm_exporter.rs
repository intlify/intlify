// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use intlify_contract::{
    ArtifactNamespace, ArtifactVersion, CatalogKey, CatalogKeyDomain, CatalogScopeId,
    CatalogScopeName, DeliveryUnitId, DeliveryUnitSegment, EntryReference, EntryStructuralPath,
    FingerprintAlgorithm, FingerprintDigest, InputFingerprint, LinkLimits, Locale,
    MessageDefinition, MessageDefinitionArtifact, MessagePayload, MessageReference,
    MessageReferenceArtifact, MessageSelector, PortablePathSegment, PortableRelativePath,
    ProducerId, ProducerIdentity, ProducerRevision, ReferenceArtifactIdentity,
    ReferenceArtifactSegment, SourceDocumentIdentity,
};
#[cfg(feature = "benchmark")]
use intlify_export::benchmark::{
    benchmark_export_esm, benchmark_prepare_export, BenchmarkArtifactAssociation,
    BenchmarkDeliveryUnitBucket, BenchmarkExportStage, BenchmarkLocaleBucket,
};
use intlify_export::{
    prepare_export, EsmExporter, EsmExporterOptions, ExportArtifact, ExportArtifactFormatVersion,
    ExportArtifactRelationshipKind, ExportErrorEvidence, ExportValidationLimits,
    InternalInvariantViolation, PlatformExporter, UnsupportedBatchFeature,
};
#[cfg(feature = "benchmark")]
use intlify_export::{ExportArtifactPath, ExportArtifactSet, PayloadFingerprint};
use intlify_linker::{
    link, CoverageBaseline, DeliveryUnitGraph, DynamicReferenceMode, InputCompleteness,
    LinkOutcome, LinkPolicy, LinkRequest, LocaleFallback, PlacementPolicy, ScopeCompleteness,
    ScopeCompletenessTable, ScopeMappingTable,
};

const DOMAIN: CatalogKeyDomain = CatalogKeyDomain::JsonPointer;

fn scope(name: &str) -> CatalogScopeId {
    CatalogScopeId::new(
        ArtifactNamespace::Project,
        CatalogScopeName::try_new(name).unwrap(),
    )
}

fn locale(value: &str) -> Locale {
    Locale::try_new(value).unwrap()
}

fn key(value: &str) -> CatalogKey {
    CatalogKey::try_new(DOMAIN, value).unwrap()
}

fn domain_key(domain: CatalogKeyDomain, value: &str) -> CatalogKey {
    CatalogKey::try_new(domain, value).unwrap()
}

fn producer() -> ProducerIdentity {
    ProducerIdentity::new(
        ProducerId::try_new("dev.intlify/esm-export-test").unwrap(),
        ProducerRevision::try_new("test").unwrap(),
    )
}

fn definition(
    scope: CatalogScopeId,
    key_value: &str,
    locale_value: &str,
    message: &str,
    occurrence: u32,
) -> MessageDefinition {
    MessageDefinition::try_new(
        scope,
        DOMAIN,
        key(key_value),
        locale(locale_value),
        MessagePayload::try_new(message).unwrap(),
        EntryReference::new(EntryStructuralPath::try_new("message").unwrap(), occurrence),
    )
    .unwrap()
}

fn definition_in_domain(
    scope: CatalogScopeId,
    domain: CatalogKeyDomain,
    key_value: &str,
    locale_value: &str,
    message: &str,
    occurrence: u32,
) -> MessageDefinition {
    MessageDefinition::try_new(
        scope,
        domain,
        domain_key(domain, key_value),
        locale(locale_value),
        MessagePayload::try_new(message).unwrap(),
        EntryReference::new(EntryStructuralPath::try_new("message").unwrap(), occurrence),
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

fn reference_artifact(
    identity: &str,
    delivery_unit: DeliveryUnitId,
    references: Vec<(CatalogScopeId, &str)>,
) -> MessageReferenceArtifact {
    let references = references
        .into_iter()
        .map(|(scope, key_value)| {
            MessageReference::try_new(
                scope,
                DOMAIN,
                MessageSelector::Exact(key(key_value)),
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
            vec![ReferenceArtifactSegment::try_new(identity).unwrap()],
        )
        .unwrap(),
        delivery_unit,
        references,
        &LinkLimits::default(),
    )
    .unwrap()
}

fn reference_artifact_in_domains(
    identity: &str,
    delivery_unit: DeliveryUnitId,
    references: Vec<(CatalogScopeId, CatalogKeyDomain, &str)>,
) -> MessageReferenceArtifact {
    let references = references
        .into_iter()
        .map(|(scope, domain, key_value)| {
            MessageReference::try_new(
                scope,
                domain,
                MessageSelector::Exact(domain_key(domain, key_value)),
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
            vec![ReferenceArtifactSegment::try_new(identity).unwrap()],
        )
        .unwrap(),
        delivery_unit,
        references,
        &LinkLimits::default(),
    )
    .unwrap()
}

fn policy(
    production_locales: &[&str],
    fallbacks: Vec<LocaleFallback>,
    baselines: Vec<(CatalogScopeId, &str)>,
) -> LinkPolicy {
    LinkPolicy::try_new(
        production_locales
            .iter()
            .map(|value| locale(value))
            .collect(),
        fallbacks,
        Vec::new(),
        baselines
            .into_iter()
            .map(|(scope, locale_value)| CoverageBaseline::new(scope, locale(locale_value)))
            .collect(),
        DynamicReferenceMode::Compat,
        PlacementPolicy::Duplicate,
        &LinkLimits::default(),
    )
    .unwrap()
}

// Keep the complete artifact fixtures alive for the borrowed request and link call.
#[allow(clippy::needless_pass_by_value)]
fn linked_outcome(
    scopes: &[CatalogScopeId],
    definitions: Vec<MessageDefinitionArtifact>,
    references: Vec<MessageReferenceArtifact>,
    policy: &LinkPolicy,
    graph: &DeliveryUnitGraph,
) -> LinkOutcome {
    let limits = LinkLimits::default();
    let mappings = ScopeMappingTable::empty(scopes, &limits).unwrap();
    let completeness = ScopeCompletenessTable::try_new(
        scopes,
        scopes
            .iter()
            .map(|scope| {
                ScopeCompleteness::try_new(
                    scope.clone(),
                    InputCompleteness::Closed,
                    InputCompleteness::Closed,
                )
                .unwrap()
            })
            .collect(),
        &limits,
    )
    .unwrap();
    let request = LinkRequest::try_new(
        &references,
        &definitions,
        policy,
        &mappings,
        &completeness,
        graph,
        &limits,
    )
    .unwrap();
    link(&request).unwrap()
}

fn export(
    outcome: &LinkOutcome,
    policy: &LinkPolicy,
    eager: &[&str],
) -> intlify_export::ExportArtifactSet {
    let batch = prepare_export(outcome, ExportValidationLimits::default())
        .unwrap()
        .unwrap();
    let options =
        EsmExporterOptions::try_new(policy, eager.iter().map(|value| locale(value)).collect())
            .unwrap();
    EsmExporter::new(options).export(&batch).unwrap()
}

fn artifact<'a>(
    artifacts: &'a [ExportArtifact],
    kind: &str,
    payload_fragment: &str,
) -> &'a ExportArtifact {
    artifacts
        .iter()
        .find(|artifact| {
            artifact.kind().as_str() == kind
                && std::str::from_utf8(artifact.payload().as_bytes())
                    .unwrap()
                    .contains(payload_fragment)
        })
        .unwrap_or_else(|| {
            panic!("no artifact of kind {kind} contains the fragment {payload_fragment:?}")
        })
}

fn source(artifact: &ExportArtifact) -> &str {
    std::str::from_utf8(artifact.payload().as_bytes()).unwrap()
}

fn logical_path(artifact: &ExportArtifact) -> String {
    artifact
        .logical_path()
        .segments()
        .iter()
        .map(intlify_export::ExportArtifactPathSegment::as_str)
        .collect::<Vec<_>>()
        .join("/")
}

#[test]
fn fallback_materialization_emits_canonical_locale_loader_and_accessor_artifacts() {
    let app = scope("app");
    let policy = policy(
        &["fr", "en"],
        vec![LocaleFallback::new(locale("fr"), vec![locale("en")])],
        vec![(app.clone(), "en")],
    );
    let outcome = linked_outcome(
        std::slice::from_ref(&app),
        vec![definition_artifact(
            "messages.json",
            vec![
                definition(app.clone(), "/hello", "en", "Hello", 0),
                definition(
                    app.clone(),
                    "/total",
                    "en",
                    ".input {$count :number}\n{{Total: {$count}}}",
                    1,
                ),
            ],
        )],
        vec![reference_artifact(
            "app",
            DeliveryUnitId::main(),
            vec![(app.clone(), "/hello"), (app.clone(), "/total")],
        )],
        &policy,
        &DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap(),
    );

    #[cfg(feature = "benchmark")]
    {
        let preparation =
            benchmark_prepare_export(&outcome, ExportValidationLimits::default()).unwrap();
        assert_eq!(preparation.measurements().len(), 5);
        let exporter =
            EsmExporter::new(EsmExporterOptions::try_new(&policy, vec![locale("en")]).unwrap());
        let execution = benchmark_export_esm(&exporter, preparation.batch().unwrap()).unwrap();
        assert_eq!(execution.measurements().len(), 4);
        assert_eq!(
            execution.observation().associations().len(),
            execution.observation().artifacts().artifacts().len()
        );
        assert_eq!(execution.observation().entry_roots().len(), 2);
        assert!(matches!(
            execution.observation().associations()[0].locale(),
            BenchmarkLocaleBucket::Shared | BenchmarkLocaleBucket::Locale(_)
        ));
        assert!(execution
            .observation()
            .associations()
            .iter()
            .any(|association| matches!(
                association.delivery_unit(),
                BenchmarkDeliveryUnitBucket::Shared
            )));
    }

    let set = export(&outcome, &policy, &["en"]);
    assert_eq!(set.artifacts().len(), 4);
    let locale_en = artifact(
        set.artifacts(),
        "dev.intlify/esm-module",
        "export const locale = \"en\";",
    );
    let locale_fr = artifact(
        set.artifacts(),
        "dev.intlify/esm-module",
        "export const locale = \"fr\";",
    );
    let loader = artifact(
        set.artifacts(),
        "dev.intlify/loader-map",
        "export function loadLocale",
    );
    let accessor = artifact(
        set.artifacts(),
        "dev.intlify/typescript-accessor",
        "export const scope = [\"project\", \"app\"] as const;",
    );

    assert_eq!(
        source(locale_en),
        include_str!("../fixtures/esm/locale-multiple.mjs")
    );
    assert_eq!(
        source(locale_fr),
        "export const formatVersion = [0, 1];\nexport const deliveryUnit = [\"main\"];\nexport const locale = \"fr\";\n\nexport const messages = [\n  [[\"project\", \"app\"], \"json-pointer\", \"/hello\", \"en\", \"Hello\"],\n  [[\"project\", \"app\"], \"json-pointer\", \"/total\", \"en\", \".input {$count :number}\\n{{Total: {$count}}}\"]\n];\n"
    );

    let en_specifier = format!("./{}", logical_path(locale_en));
    let fr_specifier = format!("./{}", logical_path(locale_fr));
    // The golden keeps readable placeholders because opaque hashed specifier
    // framing is asserted independently by esm::path tests.
    let normalized_loader = source(loader)
        .replace(&en_specifier, "./locales/locale-en.mjs")
        .replace(&fr_specifier, "./locales/locale-fr.mjs");
    assert_eq!(
        normalized_loader,
        include_str!("../fixtures/esm/loader-multiple.mjs")
    );
    assert_eq!(
        source(accessor),
        include_str!("../fixtures/esm/accessor-app.ts")
    );

    for artifact in [locale_en, locale_fr, loader] {
        assert_eq!(
            artifact.format_version(),
            ExportArtifactFormatVersion::DRAFT_V0_1
        );
        assert_eq!(
            artifact.metadata().media_type().unwrap().as_str(),
            "text/javascript"
        );
    }
    assert_eq!(
        accessor.format_version(),
        ExportArtifactFormatVersion::DRAFT_V0_1
    );
    assert_eq!(
        accessor.metadata().media_type().unwrap().as_str(),
        "text/typescript"
    );

    assert_eq!(loader.logical_path().segments()[0].as_str(), "loader.mjs");
    assert_eq!(loader.metadata().relationships().len(), 2);
    let relationship_by_target = loader
        .metadata()
        .relationships()
        .iter()
        .map(|relationship| (relationship.target(), relationship.kind()))
        .collect::<Vec<_>>();
    assert!(relationship_by_target.contains(&(
        locale_en.logical_path(),
        ExportArtifactRelationshipKind::EagerLoad
    )));
    assert!(relationship_by_target.contains(&(
        locale_fr.logical_path(),
        ExportArtifactRelationshipKind::LazyLoad
    )));
    assert!(locale_en.metadata().relationships().is_empty());
    assert!(locale_fr.metadata().relationships().is_empty());
    assert!(accessor.metadata().relationships().is_empty());
}

#[test]
fn empty_message_plan_and_empty_eager_set_still_emit_all_lazy_locale_output() {
    let app = scope("app");
    let policy = policy(&["en"], Vec::new(), Vec::new());
    let outcome = linked_outcome(
        std::slice::from_ref(&app),
        Vec::new(),
        Vec::new(),
        &policy,
        &DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap(),
    );

    let first = export(&outcome, &policy, &[]);
    let second = export(&outcome, &policy, &[]);
    assert_eq!(first, second);
    assert_eq!(first.artifacts().len(), 2);
    let locale_artifact = artifact(
        first.artifacts(),
        "dev.intlify/esm-module",
        "export const locale = \"en\";",
    );
    assert_eq!(
        source(locale_artifact),
        "export const formatVersion = [0, 1];\nexport const deliveryUnit = [\"main\"];\nexport const locale = \"en\";\n\nexport const messages = [];\n"
    );
    let loader = artifact(
        first.artifacts(),
        "dev.intlify/loader-map",
        "export function loadLocale",
    );
    assert!(!source(loader).starts_with("import"));
    assert_eq!(loader.metadata().relationships().len(), 1);
    assert_eq!(
        loader.metadata().relationships()[0].kind(),
        ExportArtifactRelationshipKind::LazyLoad
    );
}

#[test]
fn loader_preserves_fallback_priority_and_assigns_eager_aliases_canonically() {
    let app = scope("app");
    let policy = policy(
        &["fr", "en", "de", "ja"],
        vec![
            LocaleFallback::new(locale("fr"), vec![locale("de"), locale("en")]),
            LocaleFallback::new(locale("ja"), vec![locale("en")]),
        ],
        Vec::new(),
    );
    let outcome = linked_outcome(
        std::slice::from_ref(&app),
        Vec::new(),
        Vec::new(),
        &policy,
        &DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap(),
    );
    let set = export(&outcome, &policy, &["fr", "de"]);
    let locale_de = artifact(
        set.artifacts(),
        "dev.intlify/esm-module",
        "export const locale = \"de\";",
    );
    let locale_fr = artifact(
        set.artifacts(),
        "dev.intlify/esm-module",
        "export const locale = \"fr\";",
    );
    let loader = artifact(
        set.artifacts(),
        "dev.intlify/loader-map",
        "export function loadLocale",
    );
    let loader_source = source(loader);

    assert!(loader_source.starts_with(&format!(
        "import * as eager0 from \"./{}\";\nimport * as eager1 from \"./{}\";\n",
        logical_path(locale_de),
        logical_path(locale_fr)
    )));
    assert!(loader_source.contains(
        "export const fallbacks = [\n  [\"fr\", [\"de\", \"en\"]],\n  [\"ja\", [\"en\"]]\n];"
    ));
    assert!(loader_source.contains("case \"de\":\n      return Promise.resolve(eager0);"));
    assert!(loader_source.contains("case \"fr\":\n      return Promise.resolve(eager1);"));
}

#[test]
fn root_key_unicode_arguments_and_scalar_escaping_are_preserved() {
    let unusual_scope = scope("a\"\\\n\u{2028}");
    let policy = policy(&["ja"], Vec::new(), vec![(unusual_scope.clone(), "ja")]);
    let outcome = linked_outcome(
        std::slice::from_ref(&unusual_scope),
        vec![definition_artifact(
            "ja.json",
            vec![definition(
                unusual_scope.clone(),
                "",
                "ja",
                ".input {$名前}\n{{値 {$名前}}}",
                0,
            )],
        )],
        vec![reference_artifact(
            "root",
            DeliveryUnitId::main(),
            vec![(unusual_scope.clone(), "")],
        )],
        &policy,
        &DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap(),
    );
    let set = export(&outcome, &policy, &[]);
    let accessor = artifact(
        set.artifacts(),
        "dev.intlify/typescript-accessor",
        "export type MessageKey",
    );
    assert_eq!(
        source(accessor),
        "export const formatVersion = [0, 1] as const;\nexport const scope = [\"project\", \"a\\\"\\\\\\n\\u2028\"] as const;\n\nexport type MessageKey =\n  | readonly [\"json-pointer\", \"\"];\n\nexport type MessageArguments<K extends MessageKey> = K extends readonly [\"json-pointer\", \"\"]\n  ? Readonly<{\n      \"名前\": unknown;\n    }>\n  : never;\n\ntype MessageCall =\n  | [key: readonly [\"json-pointer\", \"\"], args: MessageArguments<readonly [\"json-pointer\", \"\"]>];\n\nexport interface MessageRuntime<Result> {\n  format(messageScope: typeof scope, ...call: MessageCall): Result;\n}\n\nexport function createMessageAccessor<Result>(runtime: MessageRuntime<Result>) {\n  return function t(...call: MessageCall): Result {\n    return runtime.format(scope, ...call);\n  };\n}\n"
    );
}

#[test]
fn equal_canonical_key_strings_remain_distinct_across_catalog_domains() {
    let json_scope = scope("json");
    let yaml_scope = scope("yaml");
    let policy = policy(
        &["en"],
        Vec::new(),
        vec![(json_scope.clone(), "en"), (yaml_scope.clone(), "en")],
    );
    let outcome = linked_outcome(
        &[json_scope.clone(), yaml_scope.clone()],
        vec![definition_artifact(
            "en.json",
            vec![
                definition_in_domain(
                    json_scope.clone(),
                    CatalogKeyDomain::JsonPointer,
                    "",
                    "en",
                    "JSON root",
                    0,
                ),
                definition_in_domain(
                    yaml_scope.clone(),
                    CatalogKeyDomain::YamlTypedPath,
                    "",
                    "en",
                    "YAML root",
                    1,
                ),
            ],
        )],
        vec![reference_artifact_in_domains(
            "domains",
            DeliveryUnitId::main(),
            vec![
                (json_scope.clone(), CatalogKeyDomain::JsonPointer, ""),
                (yaml_scope.clone(), CatalogKeyDomain::YamlTypedPath, ""),
            ],
        )],
        &policy,
        &DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap(),
    );
    let set = export(&outcome, &policy, &[]);
    let accessors = set
        .artifacts()
        .iter()
        .filter(|artifact| artifact.kind().as_str() == "dev.intlify/typescript-accessor")
        .map(source)
        .collect::<Vec<_>>();
    assert_eq!(accessors.len(), 2);
    assert!(accessors.iter().any(|source| {
        source.contains("export const scope = [\"project\", \"json\"] as const;")
            && source.contains("readonly [\"json-pointer\", \"\"]")
    }));
    assert!(accessors.iter().any(|source| {
        source.contains("export const scope = [\"project\", \"yaml\"] as const;")
            && source.contains("readonly [\"yaml-typed-path\", \"\"]")
    }));
}

#[test]
fn every_admitted_scope_model_receives_one_independent_accessor() {
    let admin = scope("admin");
    let app = scope("app");
    let policy = policy(
        &["en"],
        Vec::new(),
        vec![(app.clone(), "en"), (admin.clone(), "en")],
    );
    let outcome = linked_outcome(
        &[admin.clone(), app.clone()],
        vec![definition_artifact(
            "en.json",
            vec![
                definition(admin.clone(), "/admin", "en", "Admin", 0),
                definition(app.clone(), "/app", "en", "App", 1),
            ],
        )],
        Vec::new(),
        &policy,
        &DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap(),
    );
    let set = export(&outcome, &policy, &[]);
    let accessors = set
        .artifacts()
        .iter()
        .filter(|artifact| artifact.kind().as_str() == "dev.intlify/typescript-accessor")
        .collect::<Vec<_>>();
    assert_eq!(accessors.len(), 2);
    assert!(accessors
        .iter()
        .any(|artifact| source(artifact).contains("[\"project\", \"admin\"]")));
    assert!(accessors
        .iter()
        .any(|artifact| source(artifact).contains("[\"project\", \"app\"]")));
    assert_eq!(
        set.artifacts()
            .iter()
            .find(|artifact| artifact.kind().as_str() == "dev.intlify/loader-map")
            .unwrap()
            .metadata()
            .relationships()
            .len(),
        1
    );
}

#[test]
fn empty_admitted_model_keeps_the_exact_uncallable_accessor_surface() {
    let app = scope("app");
    let policy = policy(&["en"], Vec::new(), vec![(app.clone(), "en")]);
    let outcome = linked_outcome(
        std::slice::from_ref(&app),
        Vec::new(),
        Vec::new(),
        &policy,
        &DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap(),
    );
    let set = export(&outcome, &policy, &[]);
    let accessor = artifact(
        set.artifacts(),
        "dev.intlify/typescript-accessor",
        "export type MessageKey",
    );
    assert_eq!(
        source(accessor),
        "export const formatVersion = [0, 1] as const;\nexport const scope = [\"project\", \"app\"] as const;\n\nexport type MessageKey = never;\n\nexport type MessageArguments<K extends MessageKey> = never;\n\ntype MessageCall = [key: never, args: never];\n\nexport interface MessageRuntime<Result> {\n  format(messageScope: typeof scope, ...call: MessageCall): Result;\n}\n\nexport function createMessageAccessor<Result>(runtime: MessageRuntime<Result>) {\n  return function t(...call: MessageCall): Result {\n    return runtime.format(scope, ...call);\n  };\n}\n"
    );
}

#[test]
fn custom_delivery_unit_is_unsupported_without_flattening() {
    let app = scope("app");
    let policy = policy(&["en"], Vec::new(), Vec::new());
    let custom_unit =
        DeliveryUnitId::try_new(vec![DeliveryUnitSegment::try_new("chunk").unwrap()]).unwrap();
    let graph = DeliveryUnitGraph::try_new(
        vec![custom_unit.clone()],
        Vec::new(),
        &LinkLimits::default(),
    )
    .unwrap();
    let outcome = linked_outcome(
        std::slice::from_ref(&app),
        Vec::new(),
        Vec::new(),
        &policy,
        &graph,
    );
    let batch = prepare_export(&outcome, ExportValidationLimits::default())
        .unwrap()
        .unwrap();
    let exporter = EsmExporter::new(EsmExporterOptions::try_new(&policy, Vec::new()).unwrap());
    let error = exporter.export(&batch).unwrap_err();
    let ExportErrorEvidence::UnsupportedBatch(evidence) = error.evidence() else {
        panic!("expected unsupported batch evidence")
    };
    assert_eq!(
        evidence.feature(),
        UnsupportedBatchFeature::DeliveryUnitPartitioning
    );
}

#[test]
fn generic_empty_plan_batch_is_a_builtin_capability_contract_mismatch() {
    let app = scope("app");
    let policy = policy(&["en"], Vec::new(), Vec::new());
    let graph = DeliveryUnitGraph::try_new(Vec::new(), Vec::new(), &LinkLimits::default()).unwrap();
    let outcome = linked_outcome(
        std::slice::from_ref(&app),
        Vec::new(),
        Vec::new(),
        &policy,
        &graph,
    );
    let batch = prepare_export(&outcome, ExportValidationLimits::default())
        .unwrap()
        .unwrap();
    assert!(batch.plans().is_empty());
    let exporter = EsmExporter::new(EsmExporterOptions::try_new(&policy, Vec::new()).unwrap());
    let error = exporter.export(&batch).unwrap_err();
    let ExportErrorEvidence::InternalInvariant(evidence) = error.evidence() else {
        panic!("expected internal invariant evidence")
    };
    assert_eq!(
        evidence.invariant(),
        InternalInvariantViolation::CapabilityPreflightContract
    );
}

#[test]
fn maximum_builtin_cardinality_emits_all_locale_and_scope_artifacts() {
    let production_values = (0..1_024)
        .map(|index| format!("locale-{index:04}"))
        .collect::<Vec<_>>();
    let scopes = (0..4_096)
        .map(|index| scope(&format!("scope-{index:04}")))
        .collect::<Vec<_>>();
    let baseline = locale(&production_values[0]);
    let policy = LinkPolicy::try_new(
        production_values
            .iter()
            .map(|value| locale(value))
            .collect(),
        Vec::new(),
        Vec::new(),
        scopes
            .iter()
            .map(|scope| CoverageBaseline::new(scope.clone(), baseline.clone()))
            .collect(),
        DynamicReferenceMode::Compat,
        PlacementPolicy::Duplicate,
        &LinkLimits::default(),
    )
    .unwrap();
    let outcome = linked_outcome(
        &scopes,
        Vec::new(),
        Vec::new(),
        &policy,
        &DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap(),
    );

    let set = export(&outcome, &policy, &[]);
    assert_eq!(set.artifacts().len(), 5_121);
    assert_eq!(
        set.artifacts()
            .iter()
            .filter(|artifact| artifact.kind().as_str() == "dev.intlify/esm-module")
            .count(),
        1_024
    );
    assert_eq!(
        set.artifacts()
            .iter()
            .filter(|artifact| artifact.kind().as_str() == "dev.intlify/typescript-accessor")
            .count(),
        4_096
    );
    let loader = set
        .artifacts()
        .iter()
        .find(|artifact| artifact.kind().as_str() == "dev.intlify/loader-map")
        .unwrap();
    assert_eq!(loader.metadata().relationships().len(), 1_024);
}

#[cfg(feature = "benchmark")]
#[test]
fn benchmark_observations_ignore_input_enumeration_and_track_semantic_output() {
    let (baseline_outcome, baseline_policy) = benchmark_outcome("First message", false);
    let baseline = benchmark_snapshot(&baseline_outcome, &baseline_policy);

    let (reordered_outcome, reordered_policy) = benchmark_outcome("First message", true);
    let reordered = benchmark_snapshot(&reordered_outcome, &reordered_policy);
    assert_eq!(baseline, reordered);

    let (changed_outcome, changed_policy) = benchmark_outcome("Changed message", false);
    let changed = benchmark_snapshot(&changed_outcome, &changed_policy);
    assert_eq!(
        baseline
            .preparation
            .iter()
            .map(|(stage, _)| *stage)
            .collect::<Vec<_>>(),
        changed
            .preparation
            .iter()
            .map(|(stage, _)| *stage)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        baseline
            .export
            .iter()
            .map(|(stage, _)| *stage)
            .collect::<Vec<_>>(),
        changed
            .export
            .iter()
            .map(|(stage, _)| *stage)
            .collect::<Vec<_>>()
    );
    assert!(
        baseline
            .preparation
            .iter()
            .zip(&changed.preparation)
            .any(|(left, right)| left.1 != right.1),
        "a semantic message change must affect a preparation checksum"
    );
    assert!(
        baseline
            .export
            .iter()
            .zip(&changed.export)
            .any(|(left, right)| left.1 != right.1),
        "a semantic message change must affect an exporter checksum"
    );
    assert_ne!(
        artifact_fingerprints(&baseline.artifacts),
        artifact_fingerprints(&changed.artifacts)
    );
}

#[cfg(feature = "benchmark")]
#[derive(Debug, PartialEq, Eq)]
struct BenchmarkSnapshot {
    preparation: Vec<(BenchmarkExportStage, u32)>,
    export: Vec<(BenchmarkExportStage, u32)>,
    artifacts: ExportArtifactSet,
    associations: Vec<BenchmarkArtifactAssociation>,
    entry_roots: Vec<ExportArtifactPath>,
}

#[cfg(feature = "benchmark")]
fn benchmark_snapshot(outcome: &LinkOutcome, policy: &LinkPolicy) -> BenchmarkSnapshot {
    let preparation = benchmark_prepare_export(outcome, ExportValidationLimits::default()).unwrap();
    let preparation_measurements = preparation
        .measurements()
        .iter()
        .map(|measurement| (measurement.stage(), measurement.checksum()))
        .collect();
    let exporter =
        EsmExporter::new(EsmExporterOptions::try_new(policy, vec![locale("en")]).unwrap());
    let execution = benchmark_export_esm(&exporter, preparation.batch().unwrap()).unwrap();
    let export_measurements = execution
        .measurements()
        .iter()
        .map(|measurement| (measurement.stage(), measurement.checksum()))
        .collect();
    let observation = execution.into_observation();

    BenchmarkSnapshot {
        preparation: preparation_measurements,
        export: export_measurements,
        artifacts: observation.artifacts().clone(),
        associations: observation.associations().to_vec(),
        entry_roots: observation.entry_roots().to_vec(),
    }
}

#[cfg(feature = "benchmark")]
fn benchmark_outcome(message: &str, reverse_artifacts: bool) -> (LinkOutcome, LinkPolicy) {
    let app = scope("app");
    let policy = policy(&["en"], Vec::new(), vec![(app.clone(), "en")]);
    let mut definitions = vec![
        definition_artifact(
            "a.json",
            vec![definition(app.clone(), "/first", "en", message, 0)],
        ),
        definition_artifact(
            "b.json",
            vec![definition(
                app.clone(),
                "/second",
                "en",
                "Second message",
                0,
            )],
        ),
    ];
    if reverse_artifacts {
        definitions.reverse();
    }
    let outcome = linked_outcome(
        std::slice::from_ref(&app),
        definitions,
        vec![reference_artifact(
            "app",
            DeliveryUnitId::main(),
            vec![(app.clone(), "/first"), (app.clone(), "/second")],
        )],
        &policy,
        &DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap(),
    );
    (outcome, policy)
}

#[cfg(feature = "benchmark")]
fn artifact_fingerprints(artifacts: &ExportArtifactSet) -> Vec<PayloadFingerprint> {
    artifacts
        .artifacts()
        .iter()
        .map(|artifact| artifact.payload().fingerprint())
        .collect()
}
