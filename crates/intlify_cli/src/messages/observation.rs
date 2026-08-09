// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Internal stage-observation seam shared by ordinary and benchmark workflows.
//!
//! Product execution uses the no-op implementation. The non-default benchmark
//! feature supplies the only recording implementation, so measurement state
//! cannot alter checked project inputs or become a product API.

#[cfg(feature = "benchmark")]
use std::time::Duration;

/// One exact project-link workflow measurement boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(not(feature = "benchmark"), allow(dead_code))]
pub enum MessageBenchmarkStage {
    InventoryMetadataIo,
    DefinitionSnapshotRead,
    ReferenceSnapshotRead,
    ExternalArtifactRead,
    SourceParse,
    RecognizerMatchAndStaticEvaluation,
    KeyAndProvenanceConstruction,
    ReferenceArtifactConstruction,
    CacheMissProduction,
    CacheHitValidationAndAccess,
    ReferenceArtifactEncode,
    ReferenceArtifactDecode,
    DefinitionArtifactEncode,
    DefinitionArtifactDecode,
    PreExtractionAdmission,
    ResourceHostParseAndEntryExtraction,
    DefinitionProjection,
    RequestValidationAndScopeMapping,
    SemanticIndexConstruction,
    CoverageBaselineSelection,
    TypedKeyModelConstruction,
    SelectorExpansionAndReferenceResolution,
    FallbackChainConstruction,
    LocaleAwareResolution,
    ReachabilityAndPlacement,
    FindingAndPlanMaterialization,
    LocaleFindingMaterialization,
    LinkCorePeakLiveMemory,
    ProjectLinkE2e,
    SelectedMessageParse,
    MessageSemanticValidation,
    PortableDiagnosticMapping,
    ArgumentSignatureDerivation,
    ValidatedExportBatchConstruction,
    LocaleAssetRendering,
    LoaderMapRendering,
    TypedKeyAccessorRendering,
    ExportArtifactSetConstruction,
    OutputCapabilityPreflight,
    OutputPathMappingAndOwnershipInspection,
    OutputStaging,
    OutputCommit,
    OutputCheckComparison,
    MessagesEmitE2e,
    MessagesEmitCheckE2e,
    MessagesEmitJson,
}

impl MessageBenchmarkStage {
    /// Return the exact design-owned phase.
    #[must_use]
    #[cfg(feature = "benchmark")]
    pub const fn phase(self) -> &'static str {
        match self {
            Self::InventoryMetadataIo
            | Self::DefinitionSnapshotRead
            | Self::ReferenceSnapshotRead
            | Self::ExternalArtifactRead => "message_project_input_io",
            Self::SourceParse
            | Self::RecognizerMatchAndStaticEvaluation
            | Self::KeyAndProvenanceConstruction
            | Self::ReferenceArtifactConstruction => "message_reference_produce_js",
            Self::CacheMissProduction | Self::CacheHitValidationAndAccess => {
                "message_reference_produce_js_cache"
            }
            Self::ReferenceArtifactEncode
            | Self::ReferenceArtifactDecode
            | Self::DefinitionArtifactEncode
            | Self::DefinitionArtifactDecode => "message_artifact_codec",
            Self::PreExtractionAdmission | Self::DefinitionProjection => {
                "message_definition_project"
            }
            Self::ResourceHostParseAndEntryExtraction => "resource_extract",
            Self::RequestValidationAndScopeMapping
            | Self::SemanticIndexConstruction
            | Self::SelectorExpansionAndReferenceResolution
            | Self::ReachabilityAndPlacement
            | Self::FindingAndPlanMaterialization => "message_link_core",
            Self::CoverageBaselineSelection | Self::TypedKeyModelConstruction => {
                "message_typed_key_model"
            }
            Self::FallbackChainConstruction
            | Self::LocaleAwareResolution
            | Self::LocaleFindingMaterialization => "message_link_fallback",
            Self::LinkCorePeakLiveMemory => "message_link_peak_memory",
            Self::ProjectLinkE2e => "message_project_link_e2e",
            Self::SelectedMessageParse
            | Self::MessageSemanticValidation
            | Self::PortableDiagnosticMapping
            | Self::ArgumentSignatureDerivation
            | Self::ValidatedExportBatchConstruction => "message_export_prepare",
            Self::LocaleAssetRendering
            | Self::LoaderMapRendering
            | Self::TypedKeyAccessorRendering
            | Self::ExportArtifactSetConstruction => "message_export_esm",
            Self::OutputCapabilityPreflight
            | Self::OutputPathMappingAndOwnershipInspection
            | Self::OutputStaging
            | Self::OutputCommit
            | Self::OutputCheckComparison => "message_output_register",
            Self::MessagesEmitE2e => "messages_emit_e2e",
            Self::MessagesEmitCheckE2e => "messages_emit_check_e2e",
            Self::MessagesEmitJson => "messages_emit_json",
        }
    }

    /// Return the exact design-owned cost.
    #[must_use]
    pub const fn cost(self) -> &'static str {
        match self {
            Self::InventoryMetadataIo => "inventory_metadata_io",
            Self::DefinitionSnapshotRead => "definition_snapshot_read",
            Self::ReferenceSnapshotRead => "reference_snapshot_read",
            Self::ExternalArtifactRead => "external_artifact_read",
            Self::SourceParse => "source_parse",
            Self::RecognizerMatchAndStaticEvaluation => "recognizer_match_and_static_evaluation",
            Self::KeyAndProvenanceConstruction => "key_and_provenance_construction",
            Self::ReferenceArtifactConstruction => "reference_artifact_construction",
            Self::CacheMissProduction => "cache_miss_production",
            Self::CacheHitValidationAndAccess => "cache_hit_validation_and_access",
            Self::ReferenceArtifactEncode => "reference_artifact_encode",
            Self::ReferenceArtifactDecode => "reference_artifact_decode",
            Self::DefinitionArtifactEncode => "definition_artifact_encode",
            Self::DefinitionArtifactDecode => "definition_artifact_decode",
            Self::PreExtractionAdmission => "pre_extraction_admission",
            Self::ResourceHostParseAndEntryExtraction => "host_parse_and_entry_extraction",
            Self::DefinitionProjection => "definition_projection",
            Self::RequestValidationAndScopeMapping => "request_validation_and_scope_mapping",
            Self::SemanticIndexConstruction => "semantic_index_construction",
            Self::CoverageBaselineSelection => "coverage_baseline_selection",
            Self::TypedKeyModelConstruction => "typed_key_model_construction",
            Self::SelectorExpansionAndReferenceResolution => {
                "selector_expansion_and_reference_resolution"
            }
            Self::FallbackChainConstruction => "fallback_chain_construction",
            Self::LocaleAwareResolution => "locale_aware_resolution",
            Self::ReachabilityAndPlacement => "reachability_and_placement",
            Self::FindingAndPlanMaterialization => "finding_and_plan_materialization",
            Self::LocaleFindingMaterialization => "locale_finding_materialization",
            Self::LinkCorePeakLiveMemory => "link_core_peak_live_memory",
            Self::ProjectLinkE2e | Self::MessagesEmitE2e | Self::MessagesEmitCheckE2e => {
                "complete_workflow"
            }
            Self::SelectedMessageParse => "selected_message_parse",
            Self::MessageSemanticValidation => "message_semantic_validation",
            Self::PortableDiagnosticMapping => "portable_diagnostic_mapping",
            Self::ArgumentSignatureDerivation => "argument_signature_derivation",
            Self::ValidatedExportBatchConstruction => "validated_export_batch_construction",
            Self::LocaleAssetRendering => "locale_asset_rendering",
            Self::LoaderMapRendering => "loader_map_rendering",
            Self::TypedKeyAccessorRendering => "typed_key_accessor_rendering",
            Self::ExportArtifactSetConstruction => "export_artifact_set_construction",
            Self::OutputCapabilityPreflight => "capability_preflight",
            Self::OutputPathMappingAndOwnershipInspection => {
                "path_mapping_and_ownership_inspection"
            }
            Self::OutputStaging => "staging",
            Self::OutputCommit => "commit",
            Self::OutputCheckComparison => "check_comparison",
            Self::MessagesEmitJson => "json_reporter",
        }
    }

    /// Return the stable semantic boundary identifier.
    #[must_use]
    #[cfg(feature = "benchmark")]
    pub const fn boundary_id(self) -> &'static str {
        match self {
            Self::InventoryMetadataIo => "project_inventory_metadata_io",
            Self::DefinitionSnapshotRead => "project_definition_snapshot_read",
            Self::ReferenceSnapshotRead => "project_reference_snapshot_read",
            Self::ExternalArtifactRead => "project_external_artifact_read",
            Self::SourceParse => "js_source_parse",
            Self::RecognizerMatchAndStaticEvaluation => "js_recognizer_static_evaluation",
            Self::KeyAndProvenanceConstruction => "js_key_provenance_construction",
            Self::ReferenceArtifactConstruction => "js_reference_artifact_construction",
            Self::CacheMissProduction => "js_cache_miss_production",
            Self::CacheHitValidationAndAccess => "js_cache_hit_access",
            Self::ReferenceArtifactEncode => "reference_artifact_encode",
            Self::ReferenceArtifactDecode => "reference_artifact_decode",
            Self::DefinitionArtifactEncode => "definition_artifact_encode",
            Self::DefinitionArtifactDecode => "definition_artifact_decode",
            Self::PreExtractionAdmission => "definition_pre_extraction_admission",
            Self::ResourceHostParseAndEntryExtraction => "resource_host_parse_and_entry_extraction",
            Self::DefinitionProjection => "definition_projection",
            Self::RequestValidationAndScopeMapping => "link_request_validation_scope_mapping",
            Self::SemanticIndexConstruction => "link_semantic_index_construction",
            Self::CoverageBaselineSelection => "coverage_baseline_selection",
            Self::TypedKeyModelConstruction => "typed_key_model_construction",
            Self::SelectorExpansionAndReferenceResolution => "link_selector_resolution",
            Self::FallbackChainConstruction => "fallback_chain_construction",
            Self::LocaleAwareResolution => "locale_aware_resolution",
            Self::ReachabilityAndPlacement => "link_reachability_placement",
            Self::FindingAndPlanMaterialization => "link_finding_plan_materialization",
            Self::LocaleFindingMaterialization => "locale_finding_materialization",
            Self::LinkCorePeakLiveMemory => "link_core_peak_live_memory",
            Self::ProjectLinkE2e => "project_link_e2e",
            Self::SelectedMessageParse => "selected_message_parse",
            Self::MessageSemanticValidation => "message_semantic_validation",
            Self::PortableDiagnosticMapping => "portable_diagnostic_mapping",
            Self::ArgumentSignatureDerivation => "argument_signature_derivation",
            Self::ValidatedExportBatchConstruction => "validated_export_batch_construction",
            Self::LocaleAssetRendering => "locale_asset_rendering",
            Self::LoaderMapRendering => "loader_map_rendering",
            Self::TypedKeyAccessorRendering => "typed_key_accessor_rendering",
            Self::ExportArtifactSetConstruction => "export_artifact_set_construction",
            Self::OutputCapabilityPreflight => "capability_preflight",
            Self::OutputPathMappingAndOwnershipInspection => {
                "path_mapping_and_ownership_inspection"
            }
            Self::OutputStaging => "staging",
            Self::OutputCommit => "commit",
            Self::OutputCheckComparison => "check_comparison",
            Self::MessagesEmitE2e => "messages_emit_e2e",
            Self::MessagesEmitCheckE2e => "messages_emit_check_e2e",
            Self::MessagesEmitJson => "messages_emit_json",
        }
    }
}

/// Private observation callback used by the project workflow.
pub(crate) trait MessageBenchmarkObserver {
    fn enabled(&self) -> bool {
        false
    }

    fn begin(&mut self, _stage: MessageBenchmarkStage, _identity: &str) {}

    fn finish(&mut self, _stage: MessageBenchmarkStage, _identity: &str) {}

    fn abandon(&mut self, _stage: MessageBenchmarkStage, _identity: &str) {}

    fn invalidate(&mut self) {}

    fn observe(&mut self, _stage: MessageBenchmarkStage, _identity: &str, _checksum: u32) {}

    #[cfg(feature = "benchmark")]
    fn exclude_observation_overhead(&mut self, _elapsed: Duration) {}

    #[cfg(feature = "benchmark")]
    fn record(
        &mut self,
        _stage: MessageBenchmarkStage,
        _identity: &str,
        _elapsed: Duration,
        _checksum: u32,
    ) {
    }
}

/// Zero-overhead semantic observer selected by ordinary product execution.
pub(crate) struct NoopMessageBenchmarkObserver;

impl MessageBenchmarkObserver for NoopMessageBenchmarkObserver {}

/// Compute the messages benchmark's deterministic 32-bit observation checksum.
pub(crate) fn observation_checksum<'a>(
    domain: &[u8],
    fields: impl IntoIterator<Item = &'a [u8]>,
) -> u32 {
    let fields = fields.into_iter().collect::<Vec<_>>();
    let mut checksum = 2_166_136_261_u32;
    update_checksum(&mut checksum, domain);
    update_checksum(&mut checksum, &[0]);
    update_checksum_field(
        &mut checksum,
        0x01,
        &u64::try_from(fields.len())
            .expect("supported observation field counts fit u64")
            .to_be_bytes(),
    );
    for field in fields {
        update_checksum_field(&mut checksum, 0x02, field);
    }
    checksum
}

fn update_checksum_field(checksum: &mut u32, tag: u8, payload: &[u8]) {
    update_checksum(checksum, &[tag]);
    update_checksum(
        checksum,
        &u64::try_from(payload.len())
            .expect("supported observation fields fit u64")
            .to_be_bytes(),
    );
    update_checksum(checksum, payload);
}

fn update_checksum(checksum: &mut u32, bytes: &[u8]) {
    for byte in bytes {
        *checksum ^= u32::from(*byte);
        *checksum = checksum.wrapping_mul(16_777_619);
    }
}

#[cfg(test)]
mod tests {
    use super::observation_checksum;

    #[test]
    fn observation_sequences_support_more_than_255_fields() {
        let fields = (0..300_u16).map(u16::to_be_bytes).collect::<Vec<_>>();
        let first = observation_checksum(b"many-fields", fields.iter().map(<[u8; 2]>::as_slice));
        let second = observation_checksum(b"many-fields", fields.iter().map(<[u8; 2]>::as_slice));

        assert_eq!(first, second);
    }

    #[test]
    fn observation_sequence_boundaries_are_unambiguous() {
        assert_ne!(
            observation_checksum(b"fields", [b"ab".as_slice(), b"c".as_slice()]),
            observation_checksum(b"fields", [b"a".as_slice(), b"bc".as_slice()])
        );
    }
}
