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

use intlify_contract::LinkLimits;
use intlify_linker::{
    link, DeliveryUnitGraph, LinkOperationalError, LinkOutcome, LinkRequest, ScopeMappingTable,
};
use intlify_resource::{HostFormatRegistry, ResolvedResources};

use super::completeness::build_scope_completeness;
use super::config::ResolvedMessagesConfig;
use super::inventory::{
    produce_definition_inventory, DefinitionInventory, DefinitionInventoryError,
    DefinitionSourceFailure,
};
use super::reference::{
    produce_reference_inventory, MessageLinkCache, ReferenceInventory, ReferenceInventoryError,
    ReferenceSourceFailure,
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
    // Configuration-derived definition gates deliberately precede every
    // reference-source scan. They must never be downgraded into partial input.
    let definitions = produce_definition_inventory(
        project_root,
        config_path,
        resources,
        messages,
        registry,
        limits,
    )
    .map_err(ProjectLinkError::DefinitionInventory)?;

    let references = produce_reference_inventory(
        project_root,
        definitions.target_scopes(),
        messages.producers(),
        limits,
        cache,
    )
    .map_err(ProjectLinkError::ReferenceInventory)?;

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
    let outcome = link(&request).map_err(|error| ProjectLinkError::Linker {
        stage: ProjectLinkStage::SemanticLink,
        error,
    })?;

    Ok(ProjectLinkExecution {
        outcome,
        definitions,
        references,
    })
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
