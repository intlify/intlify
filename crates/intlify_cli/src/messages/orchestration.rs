// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Reusable built-in project inventory and semantic-link workflow.
//!
//! This module composes the resource-owned definition inventory, reference
//! producers and external artifacts, execution-derived completeness, the fixed
//! empty scope mapping, and the fixed one-node `main` delivery graph. It returns
//! an owned linker outcome plus retained source-local operational evidence. It
//! owns no executable command, reporter DTO, lint rule, editor session, export,
//! formatting, or filesystem mutation.

use std::error::Error;
use std::fmt;
use std::path::Path;

use intlify_contract::{
    encode_definition_artifact, encode_reference_artifact, CatalogScopeId, LinkLimits,
    MessageSelector,
};
#[cfg(feature = "benchmark")]
use intlify_linker::benchmark::{benchmark_link, BenchmarkLinkStage};
use intlify_linker::{
    link, DeliveryUnitGraph, DynamicReferenceMode, InputCompleteness, LinkOperationalError,
    LinkOutcome, LinkRequest, PartialReason, PlacementPolicy, ScopeMappingTable,
};
use intlify_resource::{HostFormatRegistry, ResolvedResources};

use super::completeness::build_scope_completeness;
use super::config::ResolvedMessagesConfig;
use super::inventory::{
    produce_definition_inventory_with_observer, DefinitionInventory, DefinitionInventoryError,
    DefinitionSourceFailure,
};
use super::observation::{
    observation_checksum, MessageBenchmarkObserver, MessageBenchmarkStage,
    NoopMessageBenchmarkObserver,
};
use super::reference::{
    produce_reference_inventory_with_observer, MessageLinkCache, ReferenceInventory,
    ReferenceInventoryError, ReferenceSourceFailure,
};

/// One successful complete project-link invocation.
#[derive(Debug)]
pub(crate) struct ProjectLinkExecution {
    outcome: LinkOutcome,
    definitions: DefinitionInventory,
    references: ReferenceInventory,
}

impl ProjectLinkExecution {
    /// Return the deterministic semantic result.
    pub(crate) const fn outcome(&self) -> &LinkOutcome {
        &self.outcome
    }

    /// Return source-local definition failures retained as partial evidence.
    pub(crate) fn definition_failures(&self) -> &[DefinitionSourceFailure] {
        self.definitions.failures()
    }

    /// Return source-local reference failures retained as partial evidence.
    pub(crate) fn reference_failures(&self) -> &[ReferenceSourceFailure] {
        self.references.failures()
    }

    /// Return definition artifacts admitted by this invocation.
    pub(crate) fn definition_artifacts(&self) -> &[intlify_contract::MessageDefinitionArtifact] {
        self.definitions.artifacts()
    }

    /// Return reference artifacts admitted by this invocation.
    pub(crate) fn reference_artifacts(&self) -> &[intlify_contract::MessageReferenceArtifact] {
        self.references.artifacts()
    }

    /// Consume the workflow result into its ordinary semantic outcome.
    #[cfg(feature = "benchmark")]
    pub(crate) fn into_outcome(self) -> LinkOutcome {
        self.outcome
    }
}

/// Linker-owned stage that rejected an otherwise composed workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectLinkStage {
    /// Empty built-in scope mapping construction.
    ScopeMapping,
    /// Execution-derived completeness construction.
    Completeness,
    /// Fixed one-node delivery graph construction.
    DeliveryGraph,
    /// Complete immutable request admission.
    RequestAdmission,
    /// Semantic indexing, resolution, finding, or plan construction.
    SemanticLink,
}

/// Fail-complete project-link error retaining its owning typed boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProjectLinkError {
    /// Definition inventory setup or configuration contradiction.
    DefinitionInventory(DefinitionInventoryError),
    /// Fatal reference producer or inventory invariant.
    ReferenceInventory(ReferenceInventoryError),
    /// Checked linker construction or execution failure.
    Linker {
        stage: ProjectLinkStage,
        error: LinkOperationalError,
    },
}

impl fmt::Display for ProjectLinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DefinitionInventory(error) => {
                write!(
                    formatter,
                    "definition inventory failed: {:?}",
                    error.operational_error()
                )
            }
            Self::ReferenceInventory(error) => {
                write!(formatter, "reference inventory failed: {error}")
            }
            Self::Linker { stage, error } => {
                write!(formatter, "project link failed during {stage:?}: {error}")
            }
        }
    }
}

impl Error for ProjectLinkError {}

/// Run the built-in single-unit project workflow without a user-facing command.
#[allow(clippy::too_many_arguments)]
pub(crate) fn link_project(
    project_root: &Path,
    config_path: Option<&Path>,
    resources: &ResolvedResources,
    messages: &ResolvedMessagesConfig,
    registry: &HostFormatRegistry,
    limits: &LinkLimits,
    cache: Option<&MessageLinkCache>,
) -> Result<ProjectLinkExecution, ProjectLinkError> {
    link_project_with_observer(
        project_root,
        config_path,
        resources,
        messages,
        registry,
        limits,
        cache,
        &mut NoopMessageBenchmarkObserver,
    )
}

/// Run the ordinary workflow with the private benchmark observation seam.
#[allow(clippy::too_many_arguments)]
pub(crate) fn link_project_with_observer<O>(
    project_root: &Path,
    config_path: Option<&Path>,
    resources: &ResolvedResources,
    messages: &ResolvedMessagesConfig,
    registry: &HostFormatRegistry,
    limits: &LinkLimits,
    cache: Option<&MessageLinkCache>,
    observer: &mut O,
) -> Result<ProjectLinkExecution, ProjectLinkError>
where
    O: MessageBenchmarkObserver + ?Sized,
{
    // Configuration-derived definition gates deliberately precede every
    // reference-source scan. They must never be downgraded into partial input.
    let definitions = produce_definition_inventory_with_observer(
        project_root,
        config_path,
        resources,
        messages,
        registry,
        limits,
        observer,
    )
    .map_err(ProjectLinkError::DefinitionInventory)?;

    let references = produce_reference_inventory_with_observer(
        project_root,
        definitions.target_scopes(),
        messages.producers(),
        limits,
        cache,
        observer,
    )
    .map_err(ProjectLinkError::ReferenceInventory)?;

    observer.begin(
        MessageBenchmarkStage::RequestValidationAndScopeMapping,
        "link-request",
    );
    let scope_mappings =
        ScopeMappingTable::empty(definitions.target_scopes(), limits).map_err(|error| {
            ProjectLinkError::Linker {
                stage: ProjectLinkStage::ScopeMapping,
                error,
            }
        })?;
    let completeness = build_scope_completeness(
        definitions.target_scopes(),
        definitions.failures(),
        references.failures(),
        limits,
    )
    .map_err(|error| ProjectLinkError::Linker {
        stage: ProjectLinkStage::Completeness,
        error,
    })?;
    let delivery_graph =
        DeliveryUnitGraph::single_main(limits).map_err(|error| ProjectLinkError::Linker {
            stage: ProjectLinkStage::DeliveryGraph,
            error,
        })?;
    let request = LinkRequest::try_new(
        references.artifacts(),
        definitions.artifacts(),
        messages.policy(),
        &scope_mappings,
        &completeness,
        &delivery_graph,
        limits,
    )
    .map_err(|error| ProjectLinkError::Linker {
        stage: ProjectLinkStage::RequestAdmission,
        error,
    })?;
    observer.finish(
        MessageBenchmarkStage::RequestValidationAndScopeMapping,
        "link-request",
    );
    if observer.enabled() {
        let observation_fields = request_observation(&request);
        observer.observe(
            MessageBenchmarkStage::RequestValidationAndScopeMapping,
            "link-request",
            observation_checksum(
                MessageBenchmarkStage::RequestValidationAndScopeMapping
                    .cost()
                    .as_bytes(),
                observation_fields.iter().map(Vec::as_slice),
            ),
        );
    }
    let outcome = run_semantic_link(&request, observer)?;

    Ok(ProjectLinkExecution {
        outcome,
        definitions,
        references,
    })
}

fn request_observation(request: &LinkRequest<'_>) -> Vec<Vec<u8>> {
    let mut fields = Vec::new();

    fields.push(b"reference-artifacts".to_vec());
    fields.push(count_bytes(request.reference_artifacts().len()));
    fields.extend(request.reference_artifacts().iter().map(|artifact| {
        encode_reference_artifact(artifact, request.limits())
            .expect("a request-admitted reference artifact re-encodes canonically")
            .into_vec()
    }));

    fields.push(b"definition-artifacts".to_vec());
    fields.push(count_bytes(request.definition_artifacts().len()));
    fields.extend(request.definition_artifacts().iter().map(|artifact| {
        encode_definition_artifact(artifact, request.limits())
            .expect("a request-admitted definition artifact re-encodes canonically")
            .into_vec()
    }));

    fields.push(b"policy".to_vec());
    fields.push(count_bytes(request.policy().production_locales().len()));
    fields.extend(
        request
            .policy()
            .production_locales()
            .iter()
            .map(|locale| locale.as_str().as_bytes().to_vec()),
    );
    fields.push(count_bytes(request.policy().configured_roots().len()));
    for root in request.policy().configured_roots() {
        push_scope(&mut fields, root.scope());
        fields.push(root.domain().as_str().as_bytes().to_vec());
        push_selector(&mut fields, root.selector());
        push_optional_text(
            &mut fields,
            root.reason().map(intlify_contract::ReasonText::as_str),
        );
    }
    fields.push(
        match request.policy().dynamic_references() {
            DynamicReferenceMode::Compat => b"compat".as_slice(),
            DynamicReferenceMode::Strict => b"strict".as_slice(),
        }
        .to_vec(),
    );
    fields.push(
        match request.policy().placement() {
            PlacementPolicy::Duplicate => b"duplicate".as_slice(),
        }
        .to_vec(),
    );

    fields.push(b"scope-mappings".to_vec());
    fields.push(count_bytes(request.scope_mappings().mappings().len()));
    for mapping in request.scope_mappings().mappings() {
        push_scope(&mut fields, mapping.source());
        push_scope(&mut fields, mapping.target());
    }

    fields.push(b"scope-completeness".to_vec());
    fields.push(count_bytes(request.scope_completeness().entries().len()));
    for entry in request.scope_completeness().entries() {
        push_scope(&mut fields, entry.scope());
        fields.push(completeness_token(entry.definitions()).to_vec());
        fields.push(completeness_token(entry.references()).to_vec());
    }

    fields.push(b"delivery-graph".to_vec());
    fields.push(count_bytes(request.delivery_graph().nodes().len()));
    for node in request.delivery_graph().nodes() {
        push_delivery_unit(&mut fields, node);
    }
    fields.push(count_bytes(request.delivery_graph().edges().len()));
    for edge in request.delivery_graph().edges() {
        push_delivery_unit(&mut fields, edge.parent());
        push_delivery_unit(&mut fields, edge.child());
    }
    fields.push(count_bytes(request.delivery_graph().roots().len()));
    for root in request.delivery_graph().roots() {
        push_delivery_unit(&mut fields, root);
    }
    fields.push(b"admitted".to_vec());
    fields
}

fn count_bytes(count: usize) -> Vec<u8> {
    u64::try_from(count)
        .expect("supported benchmark observation counts fit u64")
        .to_be_bytes()
        .to_vec()
}

fn push_scope(fields: &mut Vec<Vec<u8>>, scope: &CatalogScopeId) {
    fields.push(scope.namespace().as_str().as_bytes().to_vec());
    fields.push(scope.name().as_str().as_bytes().to_vec());
}

fn push_delivery_unit(fields: &mut Vec<Vec<u8>>, unit: &intlify_contract::DeliveryUnitId) {
    fields.push(count_bytes(unit.segments().len()));
    fields.extend(
        unit.segments()
            .iter()
            .map(|segment| segment.as_str().as_bytes().to_vec()),
    );
}

fn push_selector(fields: &mut Vec<Vec<u8>>, selector: &MessageSelector) {
    fields.push(selector.kind_str().as_bytes().to_vec());
    match selector {
        MessageSelector::Exact(key) => fields.push(key.as_str().as_bytes().to_vec()),
        MessageSelector::Prefix(prefix) => fields.push(prefix.as_str().as_bytes().to_vec()),
        MessageSelector::Pattern(pattern) => fields.push(pattern.as_str().as_bytes().to_vec()),
        MessageSelector::AllInScope | MessageSelector::UnboundedDynamic => {}
    }
}

fn push_optional_text(fields: &mut Vec<Vec<u8>>, value: Option<&str>) {
    match value {
        Some(value) => {
            fields.push(b"present".to_vec());
            fields.push(value.as_bytes().to_vec());
        }
        None => fields.push(b"absent".to_vec()),
    }
}

const fn completeness_token(completeness: InputCompleteness) -> &'static [u8] {
    match completeness {
        InputCompleteness::Closed => b"closed",
        InputCompleteness::Partial(reason) => match reason {
            PartialReason::OpenEditorWorld => b"partial:open-editor-world",
            PartialReason::SourceOmitted => b"partial:source-omitted",
            PartialReason::SourceFailed => b"partial:source-failed",
            PartialReason::ProducerOmitted => b"partial:producer-omitted",
            PartialReason::ProducerFailed => b"partial:producer-failed",
            PartialReason::ExternalArtifactUnverified => b"partial:external-artifact-unverified",
        },
    }
}

fn run_semantic_link<O>(
    request: &LinkRequest<'_>,
    observer: &mut O,
) -> Result<LinkOutcome, ProjectLinkError>
where
    O: MessageBenchmarkObserver + ?Sized,
{
    #[cfg(not(feature = "benchmark"))]
    let _ = &observer;

    #[cfg(feature = "benchmark")]
    if observer.enabled() {
        let execution = benchmark_link(request).map_err(|error| ProjectLinkError::Linker {
            stage: ProjectLinkStage::SemanticLink,
            error,
        })?;
        observer.exclude_observation_overhead(execution.observation_overhead());
        for measurement in execution.stages() {
            observer.record(
                message_stage_for_link(measurement.stage()),
                "link-request",
                measurement.elapsed(),
                measurement.checksum(),
            );
        }
        return Ok(execution.into_outcome());
    }

    link(request).map_err(|error| ProjectLinkError::Linker {
        stage: ProjectLinkStage::SemanticLink,
        error,
    })
}

#[cfg(feature = "benchmark")]
const fn message_stage_for_link(stage: BenchmarkLinkStage) -> MessageBenchmarkStage {
    match stage {
        BenchmarkLinkStage::SemanticIndexConstruction => {
            MessageBenchmarkStage::SemanticIndexConstruction
        }
        BenchmarkLinkStage::SelectorExpansionAndReferenceResolution => {
            MessageBenchmarkStage::SelectorExpansionAndReferenceResolution
        }
        BenchmarkLinkStage::ReachabilityAndPlacement => {
            MessageBenchmarkStage::ReachabilityAndPlacement
        }
        BenchmarkLinkStage::FindingAndPlanMaterialization => {
            MessageBenchmarkStage::FindingAndPlanMaterialization
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::ops::Deref;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use intlify_contract::{LinkLimits, MessageSelector};
    use intlify_linker::{DynamicReferenceMode, LinkFindingKind};
    use intlify_resource::{HostFormatRegistry, ResourcesConfig};
    use serde_json::{json, Value};

    use super::{
        link_project, MessageLinkCache, ProjectLinkError, ProjectLinkExecution, ProjectLinkStage,
    };
    use crate::messages::validate_messages_config;

    const EXTERNAL_REFERENCES: &str = concat!(
        r#"{"kind":"message-reference","version":{"major":0,"minor":1},"#,
        r#""producer":{"id":"dev.example/reference-producer","revision":"1"},"#,
        r#""identity":{"namespace":{"kind":"project"},"segments":["external","project"]},"#,
        r#""deliveryUnit":["main"],"references":["#,
        r#"{"scope":{"namespace":{"kind":"project"},"name":"app"},"domain":"json-pointer","#,
        r#""selector":{"kind":"prefix","prefix":"/prefix"}},"#,
        r#"{"scope":{"namespace":{"kind":"project"},"name":"vendor"},"domain":"json-pointer","#,
        r#""selector":{"kind":"all-in-scope"}}]}"#
    );

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new(name: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after the epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "intlify-project-link-{name}-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("temporary project root should be created");
            Self(path)
        }
    }

    impl Deref for TempRoot {
        type Target = Path;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write(root: &Path, relative: &str, source: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().expect("fixture parent"))
            .expect("fixture parent should be created");
        fs::write(path, source).expect("fixture should be written");
    }

    fn resources_value() -> Value {
        json!({
            "catalogs": [
                {
                    "scope": "app",
                    "include": ["locales/*.json"],
                    "locale": {
                        "from": "path",
                        "pattern": "locales/{locale}.json"
                    }
                },
                {
                    "scope": "app",
                    "include": ["aliases/*.json"],
                    "locale": { "from": "fixed", "value": "en" }
                },
                {
                    "scope": "app",
                    "include": ["duplicates/*.json"],
                    "locale": { "from": "fixed", "value": "en" }
                },
                {
                    "scope": "vendor",
                    "include": ["vendor/*.json"],
                    "locale": {
                        "from": "path",
                        "pattern": "vendor/{locale}.json"
                    }
                }
            ]
        })
    }

    fn messages_value(dynamic_references: &str) -> Value {
        json!({
            "locales": ["en", "ja"],
            "dynamicReferences": dynamic_references,
            "roots": [
                {
                    "scope": "app",
                    "domain": "json-pointer",
                    "selector": { "kind": "prefix", "prefix": "/prefix" }
                }
            ],
            "producers": {
                "js": {
                    "include": ["src/**/*.ts"],
                    "recognizers": {
                        "dynamic": {
                            "kind": "lookup",
                            "scope": "vendor",
                            "domain": "json-pointer",
                            "keySyntax": "canonical"
                        },
                        "set": {
                            "kind": "set",
                            "scope": "app",
                            "domain": "json-pointer",
                            "keySyntax": "canonical"
                        },
                        "t": {
                            "kind": "lookup",
                            "scope": "app",
                            "domain": "json-pointer",
                            "keySyntax": "canonical"
                        }
                    }
                },
                "artifacts": ["artifacts/external.json"]
            }
        })
    }

    fn resolved(
        messages_value: &Value,
    ) -> (
        intlify_resource::ResolvedResources,
        crate::messages::ResolvedMessagesConfig,
    ) {
        let resources = ResourcesConfig::validate(Some(&resources_value()))
            .unwrap()
            .resolve();
        let messages = validate_messages_config(Some(messages_value), &resources)
            .unwrap()
            .unwrap()
            .1;
        (resources, messages)
    }

    fn create_representative_project(root: &Path, reverse_creation: bool) {
        let mut files = vec![
            (
                "locales/en.json",
                r#"{"title":"Title","items":{"one":"One","two":"Two"},"prefix":{"child":"Child"},"unused":"Unused","ambiguous":"First"}"#,
            ),
            (
                "locales/ja.json",
                r#"{"title":"タイトル","items":{"one":"一","two":"二"},"prefix":{"child":"子"},"unused":"未使用","ambiguous":"第一"}"#,
            ),
            ("duplicates/en.json", r#"{"ambiguous":"Second"}"#),
            ("vendor/en.json", r#"{"shared":"Shared"}"#),
            ("vendor/ja.json", r#"{"shared":"共有"}"#),
            (
                "src/z.ts",
                "t('/title'); set('/items/*'); t('/missing'); t('/ambiguous'); dynamic(dynamicKey);",
            ),
            ("src/empty.ts", "export const answer = 42;"),
            ("artifacts/external.json", EXTERNAL_REFERENCES),
        ];
        if reverse_creation {
            files.reverse();
        }
        for (path, source) in files {
            write(root, path, source);
        }
        fs::create_dir_all(root.join("aliases")).unwrap();
        fs::hard_link(root.join("locales/en.json"), root.join("aliases/en.json"))
            .expect("definition hard-link alias should be created");
        fs::hard_link(root.join("src/z.ts"), root.join("src/a.ts"))
            .expect("reference hard-link alias should be created");
    }

    fn execute(
        root: &Path,
        dynamic_references: &str,
        cache: Option<&MessageLinkCache>,
    ) -> ProjectLinkExecution {
        execute_messages(root, &messages_value(dynamic_references), cache)
    }

    fn execute_messages(
        root: &Path,
        messages_value: &Value,
        cache: Option<&MessageLinkCache>,
    ) -> ProjectLinkExecution {
        let (resources, messages) = resolved(messages_value);
        link_project(
            root,
            None,
            &resources,
            &messages,
            &HostFormatRegistry::new(),
            &LinkLimits::default(),
            cache,
        )
        .unwrap()
    }

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn project_link_results_and_errors_are_thread_transferable() {
        assert_send_sync::<super::ProjectLinkExecution>();
        assert_send_sync::<super::ProjectLinkError>();
    }

    #[test]
    fn representative_project_is_deterministic_across_cache_and_creation_order() {
        let first_root = TempRoot::new("representative-first");
        let permuted_root = TempRoot::new("representative-permuted");
        create_representative_project(&first_root, false);
        create_representative_project(&permuted_root, true);
        let cache = MessageLinkCache::default();

        let miss = execute(&first_root, "strict", Some(&cache));
        let hit = execute(&first_root, "strict", Some(&cache));
        let permuted = execute(&permuted_root, "strict", Some(&cache));

        assert_eq!(miss.outcome(), hit.outcome());
        assert_eq!(hit.outcome(), permuted.outcome());
        assert_eq!(miss.definition_artifacts(), hit.definition_artifacts());
        assert_eq!(hit.definition_artifacts(), permuted.definition_artifacts());
        assert_eq!(miss.reference_artifacts(), hit.reference_artifacts());
        assert_eq!(hit.reference_artifacts(), permuted.reference_artifacts());
        assert!(miss.definition_failures().is_empty());
        assert!(miss.reference_failures().is_empty());

        // The hard-link catalog alias remains evidence but does not duplicate
        // the physical source or its definition set.
        assert_eq!(miss.definition_artifacts().len(), 5);
        assert_eq!(
            miss.definition_artifacts()
                .iter()
                .map(|artifact| artifact.logical_aliases().len())
                .sum::<usize>(),
            1
        );
        // Two JS physical groups plus one external artifact participate.
        assert_eq!(miss.reference_artifacts().len(), 3);

        let selectors = miss
            .reference_artifacts()
            .iter()
            .flat_map(intlify_contract::MessageReferenceArtifact::references)
            .map(intlify_contract::MessageReference::selector)
            .collect::<Vec<_>>();
        assert!(selectors
            .iter()
            .any(|selector| matches!(selector, MessageSelector::Exact(_))));
        assert!(selectors
            .iter()
            .any(|selector| matches!(selector, MessageSelector::Prefix(_))));
        assert!(selectors
            .iter()
            .any(|selector| matches!(selector, MessageSelector::Pattern(_))));
        assert!(selectors
            .iter()
            .any(|selector| matches!(selector, MessageSelector::AllInScope)));
        assert!(selectors
            .iter()
            .any(|selector| matches!(selector, MessageSelector::UnboundedDynamic)));

        let finding_kinds = miss
            .outcome()
            .findings()
            .iter()
            .map(intlify_linker::LinkFinding::kind)
            .collect::<Vec<_>>();
        for expected in [
            LinkFindingKind::AmbiguousMessageDefinition,
            LinkFindingKind::UnresolvedMessage,
            LinkFindingKind::UnusedMessage,
            LinkFindingKind::UnboundedDynamicReference,
            LinkFindingKind::DegradedAnalysis,
        ] {
            assert!(
                finding_kinds.contains(&expected),
                "representative outcome is missing {expected:?}"
            );
        }
        assert!(miss.outcome().generation_blocked());

        let stats = cache.stats();
        assert!(stats.js_hits > 0);
        assert!(stats.external_hits > 0);
    }

    #[cfg(feature = "benchmark")]
    #[test]
    fn benchmark_project_path_preserves_the_ordinary_outcome_and_stage_contract() {
        use crate::messages::benchmark::{benchmark_project_link, MessageBenchmarkStage};

        let root = TempRoot::new("representative-benchmark");
        create_representative_project(&root, false);
        let (resources, messages) = resolved(&messages_value("strict"));
        let ordinary = execute(&root, "strict", None);
        let observed = benchmark_project_link(
            &root,
            None,
            &resources,
            &messages,
            &HostFormatRegistry::new(),
            &LinkLimits::default(),
        )
        .unwrap();
        let repeated = benchmark_project_link(
            &root,
            None,
            &resources,
            &messages,
            &HostFormatRegistry::new(),
            &LinkLimits::default(),
        )
        .unwrap();

        assert_eq!(observed.outcome(), ordinary.outcome());
        assert_eq!(repeated.outcome(), ordinary.outcome());
        assert_eq!(
            observed.definition_artifact_count(),
            ordinary.definition_artifacts().len()
        );
        assert_eq!(
            observed.reference_artifact_count(),
            ordinary.reference_artifacts().len()
        );
        assert!(observed
            .measurements()
            .iter()
            .all(|measurement| !measurement
                .stage()
                .boundary_id()
                .rsplit_once("_v")
                .is_some_and(|(_, suffix)| {
                    !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
                })));
        assert_eq!(
            observed
                .measurements()
                .iter()
                .map(|measurement| {
                    (
                        measurement.stage(),
                        measurement.identity(),
                        measurement.checksum(),
                    )
                })
                .collect::<Vec<_>>(),
            repeated
                .measurements()
                .iter()
                .map(|measurement| {
                    (
                        measurement.stage(),
                        measurement.identity(),
                        measurement.checksum(),
                    )
                })
                .collect::<Vec<_>>()
        );
        let stages = observed
            .measurements()
            .iter()
            .map(crate::messages::benchmark::BenchmarkMessageMeasurement::stage)
            .collect::<std::collections::BTreeSet<_>>();
        for required in [
            MessageBenchmarkStage::InventoryMetadataIo,
            MessageBenchmarkStage::DefinitionSnapshotRead,
            MessageBenchmarkStage::ReferenceSnapshotRead,
            MessageBenchmarkStage::ExternalArtifactRead,
            MessageBenchmarkStage::SourceParse,
            MessageBenchmarkStage::RecognizerMatchAndStaticEvaluation,
            MessageBenchmarkStage::KeyAndProvenanceConstruction,
            MessageBenchmarkStage::ReferenceArtifactConstruction,
            MessageBenchmarkStage::CacheMissProduction,
            MessageBenchmarkStage::ReferenceArtifactDecode,
            MessageBenchmarkStage::PreExtractionAdmission,
            MessageBenchmarkStage::ResourceHostParseAndEntryExtraction,
            MessageBenchmarkStage::DefinitionProjection,
            MessageBenchmarkStage::RequestValidationAndScopeMapping,
            MessageBenchmarkStage::SemanticIndexConstruction,
            MessageBenchmarkStage::SelectorExpansionAndReferenceResolution,
            MessageBenchmarkStage::ReachabilityAndPlacement,
            MessageBenchmarkStage::FindingAndPlanMaterialization,
            MessageBenchmarkStage::ProjectLinkE2e,
        ] {
            assert!(stages.contains(&required), "missing {required:?}");
        }
        assert_eq!(
            observed
                .measurements()
                .iter()
                .filter(|measurement| {
                    measurement.stage() == MessageBenchmarkStage::InventoryMetadataIo
                })
                .map(crate::messages::benchmark::BenchmarkMessageMeasurement::identity)
                .collect::<Vec<_>>(),
            [
                "definition-inventory",
                "reference-js-inventory",
                "reference-external-inventory"
            ]
        );
    }

    #[test]
    fn source_failure_returns_an_outcome_with_partial_reference_evidence() {
        let root = TempRoot::new("partial");
        write(&root, "locales/en.json", r#"{"title":"Title"}"#);
        write(&root, "locales/ja.json", r#"{"title":"タイトル"}"#);
        write(&root, "src/broken.ts", "const =");
        write(&root, "artifacts/external.json", EXTERNAL_REFERENCES);

        let execution = execute(&root, "compat", None);

        assert!(execution.definition_failures().is_empty());
        assert_eq!(execution.reference_failures().len(), 1);
        assert!(execution
            .outcome()
            .findings()
            .iter()
            .any(|finding| finding.kind() == LinkFindingKind::DegradedAnalysis));
        assert!(execution.outcome().generation_blocked());
    }

    #[test]
    fn strict_and_compat_dynamic_reference_modes_keep_distinct_semantics() {
        let root = TempRoot::new("dynamic-mode");
        write(&root, "locales/en.json", r#"{"title":"Title"}"#);
        write(&root, "locales/ja.json", r#"{"title":"タイトル"}"#);
        write(&root, "src/app.ts", "t(dynamicKey)");
        let dynamic_messages = |mode| {
            json!({
                "locales": ["en", "ja"],
                "dynamicReferences": mode,
                "producers": {
                    "js": {
                        "include": ["src/**/*.ts"],
                        "recognizers": {
                            "t": {
                                "kind": "lookup",
                                "scope": "app",
                                "domain": "json-pointer",
                                "keySyntax": "canonical"
                            }
                        }
                    }
                }
            })
        };

        let compat_config = dynamic_messages("compat");
        let strict_config = dynamic_messages("strict");
        let compat = execute_messages(&root, &compat_config, None);
        let strict = execute_messages(&root, &strict_config, None);

        assert_eq!(
            compat
                .outcome()
                .findings()
                .iter()
                .filter(|finding| { finding.kind() == LinkFindingKind::UnboundedDynamicReference })
                .count(),
            1
        );
        assert!(!compat.outcome().generation_blocked());
        assert!(strict.outcome().generation_blocked());
        assert_eq!(
            resolved(&compat_config).1.policy().dynamic_references(),
            DynamicReferenceMode::Compat
        );
        assert_eq!(
            resolved(&strict_config).1.policy().dynamic_references(),
            DynamicReferenceMode::Strict
        );
    }

    #[test]
    fn definition_configuration_gate_stops_before_reference_inventory() {
        let root = TempRoot::new("definition-gate");
        write(&root, "locales/en.json", r#"{"title":"Title"}"#);
        write(&root, "src/app.ts", "t('/title')");
        let messages_value = json!({
            "locales": ["en", "ja"],
            "producers": {
                "js": {
                    "include": ["src/**/*.ts"],
                    "recognizers": {
                        "t": {
                            "kind": "lookup",
                            "scope": "app",
                            "domain": "yaml-typed-path",
                            "keySyntax": "canonical"
                        }
                    }
                }
            }
        });
        let (resources, messages) = resolved(&messages_value);
        let cache = MessageLinkCache::default();

        let error = link_project(
            &root,
            None,
            &resources,
            &messages,
            &HostFormatRegistry::new(),
            &LinkLimits::default(),
            Some(&cache),
        )
        .unwrap_err();

        assert!(matches!(error, ProjectLinkError::DefinitionInventory(_)));
        let stats = cache.stats();
        assert_eq!(stats.js_loads, 0);
        assert_eq!(stats.external_loads, 0);
    }

    #[test]
    fn duplicate_external_identity_retains_request_admission_ownership() {
        let root = TempRoot::new("duplicate-reference-identity");
        write(&root, "artifacts/a.json", EXTERNAL_REFERENCES);
        write(&root, "artifacts/b.json", EXTERNAL_REFERENCES);
        let messages_value = json!({
            "locales": ["en", "ja"],
            "producers": {
                "artifacts": ["artifacts/a.json", "artifacts/b.json"]
            }
        });
        let (resources, messages) = resolved(&messages_value);

        let error = link_project(
            &root,
            None,
            &resources,
            &messages,
            &HostFormatRegistry::new(),
            &LinkLimits::default(),
            None,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ProjectLinkError::Linker {
                stage: ProjectLinkStage::RequestAdmission,
                ..
            }
        ));
    }
}
