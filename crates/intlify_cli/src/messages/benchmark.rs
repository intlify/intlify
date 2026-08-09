// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Non-product observation surface for the dedicated messages benchmark.
//!
//! This module is available only with the non-default `benchmark` feature. It
//! invokes the ordinary project-link workflow and exposes completed immutable
//! observations. Callers cannot construct partial inventory or linker state.

use std::error::Error;
use std::fmt;
use std::path::Path;
use std::time::{Duration, Instant};

use intlify_contract::{
    encode_definition_artifact, encode_reference_artifact, LinkLimits, MessageDefinitionArtifact,
    MessageReferenceArtifact,
};
use intlify_export::ExportArtifactSet;
use intlify_linker::{LinkOutcome, LinkPolicy};
use intlify_resource::{HostFormatRegistry, ResolvedResources};

use super::config::ResolvedMessagesConfig;
use super::delivery::{DeliveryTargetInput, ResolvedDeliveryTargets};
use super::emit;
use super::observation::{observation_checksum, MessageBenchmarkObserver};
use super::orchestration::link_project_with_observer;
use super::reference::MessageLinkCache;
use super::registration::{PreparedOutput, WriteRegistrationStatus};
use crate::args::MessagesEmitArgs;
use crate::output::{CliRunResult, Reporter};

pub use super::observation::MessageBenchmarkStage;

/// One completed project-link workflow-stage occurrence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkMessageMeasurement {
    stage: MessageBenchmarkStage,
    identity: Box<str>,
    elapsed: Duration,
    checksum: u32,
    observation_complete: bool,
}

impl BenchmarkMessageMeasurement {
    /// Return the exact phase/cost boundary.
    #[must_use]
    pub const fn stage(&self) -> MessageBenchmarkStage {
        self.stage
    }

    /// Return the planned canonical occurrence identity.
    #[must_use]
    pub const fn identity(&self) -> &str {
        &self.identity
    }

    /// Return the unadjusted monotonic-clock duration.
    #[must_use]
    pub const fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Return the deterministic semantic checksum.
    #[must_use]
    pub const fn checksum(&self) -> u32 {
        self.checksum
    }
}

/// Complete ordinary outcome plus its benchmark-only observations.
#[derive(Debug)]
pub struct BenchmarkProjectLinkExecution {
    outcome: LinkOutcome,
    measurements: Box<[BenchmarkMessageMeasurement]>,
    definition_artifacts: Box<[MessageDefinitionArtifact]>,
    reference_artifacts: Box<[MessageReferenceArtifact]>,
}

impl BenchmarkProjectLinkExecution {
    /// Return the ordinary semantic link result.
    #[must_use]
    pub const fn outcome(&self) -> &LinkOutcome {
        &self.outcome
    }

    /// Return completed observations in execution order.
    #[must_use]
    pub const fn measurements(&self) -> &[BenchmarkMessageMeasurement] {
        &self.measurements
    }

    /// Return the number of admitted definition artifacts.
    #[must_use]
    pub const fn definition_artifact_count(&self) -> usize {
        self.definition_artifacts.len()
    }

    /// Return the number of admitted reference artifacts.
    #[must_use]
    pub const fn reference_artifact_count(&self) -> usize {
        self.reference_artifacts.len()
    }

    /// Return the ordinary checked definition artifacts retained by inventory.
    #[must_use]
    pub const fn definition_artifacts(&self) -> &[MessageDefinitionArtifact] {
        &self.definition_artifacts
    }

    /// Return the ordinary checked reference artifacts retained by inventory.
    #[must_use]
    pub const fn reference_artifacts(&self) -> &[MessageReferenceArtifact] {
        &self.reference_artifacts
    }

    /// Consume the observation into the ordinary outcome.
    #[must_use]
    pub fn into_outcome(self) -> LinkOutcome {
        self.outcome
    }
}

/// Opaque failure from the ordinary project workflow or observation contract.
#[derive(Debug)]
pub struct BenchmarkProjectLinkError {
    message: String,
}

impl fmt::Display for BenchmarkProjectLinkError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for BenchmarkProjectLinkError {}

/// Observe one ordinary complete in-process project-link workflow.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn benchmark_project_link(
    project_root: &Path,
    config_path: Option<&Path>,
    resources: &ResolvedResources,
    messages: &ResolvedMessagesConfig,
    registry: &HostFormatRegistry,
    limits: &LinkLimits,
) -> Result<BenchmarkProjectLinkExecution, BenchmarkProjectLinkError> {
    let mut recorder = BenchmarkRecorder::default();
    let cache = MessageLinkCache::default();
    recorder.begin(MessageBenchmarkStage::ProjectLinkE2e, "project-link");
    let execution = link_project_with_observer(
        project_root,
        config_path,
        resources,
        messages,
        registry,
        limits,
        Some(&cache),
        &mut recorder,
    )
    .map_err(|error| BenchmarkProjectLinkError {
        message: error.to_string(),
    })?;
    recorder.finish(MessageBenchmarkStage::ProjectLinkE2e, "project-link");

    let definition_artifacts = execution.definition_artifacts().to_vec().into_boxed_slice();
    let reference_artifacts = execution.reference_artifacts().to_vec().into_boxed_slice();
    let mut observation_fields = vec![
        b"definition-artifacts".to_vec(),
        count_bytes(definition_artifacts.len()),
    ];
    for artifact in &definition_artifacts {
        observation_fields.push(
            encode_definition_artifact(artifact, limits)
                .map_err(|error| BenchmarkProjectLinkError {
                    message: format!(
                        "failed to observe a request-admitted definition artifact: {error}"
                    ),
                })?
                .into_vec(),
        );
    }
    observation_fields.push(b"reference-artifacts".to_vec());
    observation_fields.push(count_bytes(reference_artifacts.len()));
    for artifact in &reference_artifacts {
        observation_fields.push(
            encode_reference_artifact(artifact, limits)
                .map_err(|error| BenchmarkProjectLinkError {
                    message: format!(
                        "failed to observe a request-admitted reference artifact: {error}"
                    ),
                })?
                .into_vec(),
        );
    }
    observation_fields.push(b"request".to_vec());
    observation_fields.push(
        recorder
            .completed_checksum(
                MessageBenchmarkStage::RequestValidationAndScopeMapping,
                "link-request",
            )?
            .to_be_bytes()
            .to_vec(),
    );
    observation_fields.push(b"link-outcome".to_vec());
    observation_fields.push(
        recorder
            .completed_checksum(
                MessageBenchmarkStage::FindingAndPlanMaterialization,
                "link-request",
            )?
            .to_be_bytes()
            .to_vec(),
    );
    recorder.observe(
        MessageBenchmarkStage::ProjectLinkE2e,
        "project-link",
        observation_checksum(
            MessageBenchmarkStage::ProjectLinkE2e.cost().as_bytes(),
            observation_fields.iter().map(Vec::as_slice),
        ),
    );
    let measurements = recorder.finish_recording()?;

    Ok(BenchmarkProjectLinkExecution {
        outcome: execution.into_outcome(),
        measurements: measurements.into_boxed_slice(),
        definition_artifacts,
        reference_artifacts,
    })
}

/// One checked registration path selected by a stateful benchmark fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkRegistrationMode {
    Write,
    Check,
}

/// Completed registration observations and the product state token.
#[derive(Debug)]
pub struct BenchmarkRegistrationExecution {
    measurements: Box<[BenchmarkMessageMeasurement]>,
    status: &'static str,
}

impl BenchmarkRegistrationExecution {
    #[must_use]
    pub const fn measurements(&self) -> &[BenchmarkMessageMeasurement] {
        &self.measurements
    }

    #[must_use]
    pub const fn status(&self) -> &'static str {
        self.status
    }
}

/// Completed command result plus E2E and JSON reporter observations.
#[derive(Debug)]
pub struct BenchmarkMessagesEmitExecution {
    result: CliRunResult,
    measurements: Box<[BenchmarkMessageMeasurement]>,
}

impl BenchmarkMessagesEmitExecution {
    #[must_use]
    pub const fn result(&self) -> &CliRunResult {
        &self.result
    }

    #[must_use]
    pub const fn measurements(&self) -> &[BenchmarkMessageMeasurement] {
        &self.measurements
    }
}

/// Run the ordinary output-registration path with benchmark-only observation.
#[doc(hidden)]
pub fn benchmark_output_registration(
    project_root: &Path,
    output_root: &str,
    policy: &LinkPolicy,
    artifacts: &ExportArtifactSet,
    mode: BenchmarkRegistrationMode,
) -> Result<BenchmarkRegistrationExecution, BenchmarkProjectLinkError> {
    let eager_locales = policy
        .production_locales()
        .first()
        .map(|locale| vec![locale.as_str().to_owned()])
        .unwrap_or_default();
    let input = [DeliveryTargetInput::new(
        "benchmark",
        "esm",
        output_root,
        &eager_locales,
        None,
    )];
    let targets = ResolvedDeliveryTargets::try_new(&input, policy).map_err(|error| {
        BenchmarkProjectLinkError {
            message: format!("registration fixture target is invalid: {error:?}"),
        }
    })?;
    let target = &targets.targets()[0];
    let mut recorder = BenchmarkRecorder::default();
    let prepared = PreparedOutput::try_new_with_observer(target, artifacts, &mut recorder)
        .map_err(|error| BenchmarkProjectLinkError {
            message: error.to_string(),
        })?;
    let status = match mode {
        BenchmarkRegistrationMode::Write => match prepared
            .write_with_observer(project_root, target.name().as_str(), &mut recorder)
            .map_err(|error| BenchmarkProjectLinkError {
                message: error.to_string(),
            })?
            .status()
        {
            WriteRegistrationStatus::Unchanged => "unchanged",
            WriteRegistrationStatus::Written => "written",
        },
        BenchmarkRegistrationMode::Check => {
            let comparison = prepared
                .check_with_observer(project_root, target.name().as_str(), &mut recorder)
                .map_err(|error| BenchmarkProjectLinkError {
                    message: error.to_string(),
                })?;
            if comparison.is_matched() {
                "matched"
            } else {
                "different"
            }
        }
    };
    Ok(BenchmarkRegistrationExecution {
        measurements: recorder.finish_recording()?.into_boxed_slice(),
        status,
    })
}

/// Run the ordinary JSON-reporting message-delivery command in process.
#[doc(hidden)]
pub fn benchmark_messages_emit(
    project_root: &Path,
    check: bool,
) -> Result<BenchmarkMessagesEmitExecution, BenchmarkProjectLinkError> {
    let mut recorder = BenchmarkRecorder::default();
    let e2e_stage = if check {
        MessageBenchmarkStage::MessagesEmitCheckE2e
    } else {
        MessageBenchmarkStage::MessagesEmitE2e
    };
    recorder.begin(e2e_stage, "messages-emit");
    let result = emit::run_with_observer(
        &MessagesEmitArgs {
            target: None,
            check,
        },
        Reporter::Json,
        Some("intlify.config.json"),
        project_root,
        &mut recorder,
    );
    recorder.finish(e2e_stage, "messages-emit");
    let result_fields = [
        result.exit_code.to_be_bytes().to_vec(),
        result.stdout.as_bytes().to_vec(),
        result.stderr.as_bytes().to_vec(),
    ];
    let checksum = observation_checksum(
        e2e_stage.cost().as_bytes(),
        result_fields.iter().map(Vec::as_slice),
    );
    recorder.observe(e2e_stage, "messages-emit", checksum);
    Ok(BenchmarkMessagesEmitExecution {
        result,
        measurements: recorder.finish_recording()?.into_boxed_slice(),
    })
}

#[derive(Debug)]
struct ActiveMeasurement {
    stage: MessageBenchmarkStage,
    identity: Box<str>,
    started: Instant,
    excluded: Duration,
}

#[derive(Debug, Default)]
struct BenchmarkRecorder {
    active: Vec<ActiveMeasurement>,
    completed: Vec<BenchmarkMessageMeasurement>,
    observation_started: Vec<Option<Instant>>,
    invariant_failed: bool,
    invariant_detail: Option<String>,
}

impl BenchmarkRecorder {
    fn completed_checksum(
        &self,
        stage: MessageBenchmarkStage,
        identity: &str,
    ) -> Result<u32, BenchmarkProjectLinkError> {
        self.completed
            .iter()
            .find(|measurement| {
                measurement.stage == stage
                    && measurement.identity.as_ref() == identity
                    && measurement.observation_complete
            })
            .map(|measurement| measurement.checksum)
            .ok_or_else(|| BenchmarkProjectLinkError {
                message: format!(
                    "message benchmark observation is missing for {}/{}",
                    stage.cost(),
                    identity
                ),
            })
    }

    fn finish_recording(
        self,
    ) -> Result<Vec<BenchmarkMessageMeasurement>, BenchmarkProjectLinkError> {
        if self.invariant_failed
            || !self.active.is_empty()
            || self
                .completed
                .iter()
                .any(|record| !record.observation_complete)
            || self.observation_started.len() != self.completed.len()
            || self.observation_started.iter().any(Option::is_some)
            || self.completed.iter().any(|record| {
                is_js_cache_child(record.stage)
                    && !self.completed.iter().any(|candidate| {
                        candidate.stage == MessageBenchmarkStage::CacheMissProduction
                            && candidate.identity == record.identity
                    })
            })
        {
            return Err(BenchmarkProjectLinkError {
                message: format!(
                    "message benchmark boundary invariant failed: {}",
                    self.invariant_detail
                        .as_deref()
                        .unwrap_or("the completed interval set is inconsistent")
                ),
            });
        }
        Ok(self.completed)
    }

    fn parent_allows(parent: MessageBenchmarkStage, child: MessageBenchmarkStage) -> bool {
        (matches!(
            parent,
            MessageBenchmarkStage::ProjectLinkE2e
                | MessageBenchmarkStage::MessagesEmitE2e
                | MessageBenchmarkStage::MessagesEmitCheckE2e
        ) && matches!(
            child,
            MessageBenchmarkStage::InventoryMetadataIo
                | MessageBenchmarkStage::DefinitionSnapshotRead
                | MessageBenchmarkStage::ReferenceSnapshotRead
                | MessageBenchmarkStage::ExternalArtifactRead
                | MessageBenchmarkStage::PreExtractionAdmission
                | MessageBenchmarkStage::ResourceHostParseAndEntryExtraction
                | MessageBenchmarkStage::DefinitionProjection
                | MessageBenchmarkStage::CacheMissProduction
                | MessageBenchmarkStage::ReferenceArtifactDecode
                | MessageBenchmarkStage::RequestValidationAndScopeMapping
                | MessageBenchmarkStage::SemanticIndexConstruction
                | MessageBenchmarkStage::CoverageBaselineSelection
                | MessageBenchmarkStage::TypedKeyModelConstruction
                | MessageBenchmarkStage::SelectorExpansionAndReferenceResolution
                | MessageBenchmarkStage::FallbackChainConstruction
                | MessageBenchmarkStage::LocaleAwareResolution
                | MessageBenchmarkStage::ReachabilityAndPlacement
                | MessageBenchmarkStage::FindingAndPlanMaterialization
                | MessageBenchmarkStage::LocaleFindingMaterialization
                | MessageBenchmarkStage::SelectedMessageParse
                | MessageBenchmarkStage::MessageSemanticValidation
                | MessageBenchmarkStage::PortableDiagnosticMapping
                | MessageBenchmarkStage::ArgumentSignatureDerivation
                | MessageBenchmarkStage::ValidatedExportBatchConstruction
                | MessageBenchmarkStage::LocaleAssetRendering
                | MessageBenchmarkStage::LoaderMapRendering
                | MessageBenchmarkStage::TypedKeyAccessorRendering
                | MessageBenchmarkStage::ExportArtifactSetConstruction
                | MessageBenchmarkStage::OutputCapabilityPreflight
                | MessageBenchmarkStage::OutputPathMappingAndOwnershipInspection
                | MessageBenchmarkStage::OutputStaging
                | MessageBenchmarkStage::OutputCommit
                | MessageBenchmarkStage::OutputCheckComparison
                | MessageBenchmarkStage::MessagesEmitJson
        )) || (parent == MessageBenchmarkStage::CacheMissProduction && is_js_cache_child(child))
            || (parent == MessageBenchmarkStage::SelectorExpansionAndReferenceResolution
                && matches!(
                    child,
                    MessageBenchmarkStage::FallbackChainConstruction
                        | MessageBenchmarkStage::LocaleAwareResolution
                ))
            || (parent == MessageBenchmarkStage::FindingAndPlanMaterialization
                && child == MessageBenchmarkStage::LocaleFindingMaterialization)
    }

    fn push_completed(
        &mut self,
        stage: MessageBenchmarkStage,
        identity: &str,
        elapsed: Duration,
        checksum: Option<u32>,
        observation_started: Option<Instant>,
    ) {
        if self.completed.iter().any(|measurement| {
            measurement.stage == stage && measurement.identity.as_ref() == identity
        }) {
            self.invariant_failed = true;
            self.invariant_detail
                .get_or_insert_with(|| format!("duplicate completion for {stage:?}/{identity}"));
            return;
        }
        self.completed.push(BenchmarkMessageMeasurement {
            stage,
            identity: identity.into(),
            elapsed,
            checksum: checksum.unwrap_or_default(),
            observation_complete: checksum.is_some(),
        });
        self.observation_started.push(observation_started);
    }

    fn exclude_observation_time(&mut self, elapsed: Duration) {
        for active in &mut self.active {
            active.excluded = active.excluded.saturating_add(elapsed);
        }
    }
}

impl MessageBenchmarkObserver for BenchmarkRecorder {
    fn enabled(&self) -> bool {
        true
    }

    fn begin(&mut self, stage: MessageBenchmarkStage, identity: &str) {
        if self
            .active
            .iter()
            .any(|active| active.stage == stage && active.identity.as_ref() == identity)
        {
            self.invariant_failed = true;
            self.invariant_detail
                .get_or_insert_with(|| format!("duplicate start for {stage:?}/{identity}"));
            return;
        }
        if let Some(parent) = self
            .active
            .last()
            .filter(|parent| !Self::parent_allows(parent.stage, stage))
        {
            self.invariant_failed = true;
            self.invariant_detail.get_or_insert_with(|| {
                format!("undeclared nesting from {:?} to {stage:?}", parent.stage)
            });
        }
        self.active.push(ActiveMeasurement {
            stage,
            identity: identity.into(),
            started: Instant::now(),
            excluded: Duration::ZERO,
        });
    }

    fn finish(&mut self, stage: MessageBenchmarkStage, identity: &str) {
        let Some(active) = self.active.pop() else {
            self.invariant_failed = true;
            self.invariant_detail
                .get_or_insert_with(|| format!("unmatched finish for {stage:?}/{identity}"));
            return;
        };
        if active.stage != stage || active.identity.as_ref() != identity {
            self.invariant_failed = true;
            self.invariant_detail.get_or_insert_with(|| {
                format!(
                    "crossing finish for {stage:?}/{identity} while {:?}/{} is active",
                    active.stage, active.identity
                )
            });
            return;
        }
        let finished = Instant::now();
        let elapsed = finished
            .saturating_duration_since(active.started)
            .saturating_sub(active.excluded);
        self.push_completed(stage, identity, elapsed, None, Some(finished));
    }

    fn abandon(&mut self, stage: MessageBenchmarkStage, identity: &str) {
        let Some(active) = self.active.pop() else {
            self.invariant_failed = true;
            return;
        };
        if active.stage != stage || active.identity.as_ref() != identity {
            self.invariant_failed = true;
        }
    }

    fn invalidate(&mut self) {
        self.invariant_failed = true;
        self.invariant_detail.get_or_insert_with(|| {
            "an observation codec rejected a completed product value".to_owned()
        });
    }

    fn observe(&mut self, stage: MessageBenchmarkStage, identity: &str, checksum: u32) {
        let Some(index) = self.completed.iter().rposition(|measurement| {
            measurement.stage == stage
                && measurement.identity.as_ref() == identity
                && !measurement.observation_complete
        }) else {
            self.invariant_failed = true;
            self.invariant_detail.get_or_insert_with(|| {
                format!("observation has no completed interval for {stage:?}/{identity}")
            });
            return;
        };
        let Some(observation_started) = self
            .observation_started
            .get_mut(index)
            .and_then(Option::take)
        else {
            self.invariant_failed = true;
            self.invariant_detail.get_or_insert_with(|| {
                format!("observation timer is missing for {stage:?}/{identity}")
            });
            return;
        };
        self.completed[index].checksum = checksum;
        self.completed[index].observation_complete = true;
        self.exclude_observation_time(observation_started.elapsed());
    }

    fn exclude_observation_overhead(&mut self, elapsed: Duration) {
        self.exclude_observation_time(elapsed);
    }

    fn record(
        &mut self,
        stage: MessageBenchmarkStage,
        identity: &str,
        elapsed: Duration,
        checksum: u32,
    ) {
        let observation_started = Instant::now();
        if let Some(parent) = self.active.last().filter(|parent| {
            let permitted = Self::parent_allows(parent.stage, stage)
                || (matches!(
                    parent.stage,
                    MessageBenchmarkStage::ProjectLinkE2e
                        | MessageBenchmarkStage::MessagesEmitE2e
                        | MessageBenchmarkStage::MessagesEmitCheckE2e
                ) && is_js_cache_child(stage));
            !permitted
        }) {
            self.invariant_failed = true;
            self.invariant_detail.get_or_insert_with(|| {
                format!(
                    "undeclared recorded nesting from {:?} into {stage:?}/{identity}",
                    parent.stage
                )
            });
        }
        self.push_completed(stage, identity, elapsed, Some(checksum), None);
        self.exclude_observation_time(observation_started.elapsed());
    }
}

fn count_bytes(count: usize) -> Vec<u8> {
    u64::try_from(count)
        .expect("supported benchmark observation counts fit u64")
        .to_be_bytes()
        .to_vec()
}

const fn is_js_cache_child(stage: MessageBenchmarkStage) -> bool {
    matches!(
        stage,
        MessageBenchmarkStage::SourceParse
            | MessageBenchmarkStage::RecognizerMatchAndStaticEvaluation
            | MessageBenchmarkStage::KeyAndProvenanceConstruction
            | MessageBenchmarkStage::ReferenceArtifactConstruction
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorder_accepts_zero_as_a_completed_checksum() {
        let mut recorder = BenchmarkRecorder::default();
        recorder.begin(MessageBenchmarkStage::InventoryMetadataIo, "inventory");
        recorder.finish(MessageBenchmarkStage::InventoryMetadataIo, "inventory");
        recorder.observe(MessageBenchmarkStage::InventoryMetadataIo, "inventory", 0);

        let completed = recorder.finish_recording().unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].checksum(), 0);
    }

    #[test]
    fn recorder_rejects_a_finished_boundary_without_observation() {
        let mut recorder = BenchmarkRecorder::default();
        recorder.begin(MessageBenchmarkStage::InventoryMetadataIo, "inventory");
        recorder.finish(MessageBenchmarkStage::InventoryMetadataIo, "inventory");

        assert!(recorder.finish_recording().is_err());
    }

    #[test]
    fn recorder_discards_an_abandoned_child_boundary() {
        let mut recorder = BenchmarkRecorder::default();
        recorder.begin(MessageBenchmarkStage::ProjectLinkE2e, "project-link");
        recorder.begin(
            MessageBenchmarkStage::ResourceHostParseAndEntryExtraction,
            "locales/en.json",
        );
        recorder.abandon(
            MessageBenchmarkStage::ResourceHostParseAndEntryExtraction,
            "locales/en.json",
        );
        recorder.finish(MessageBenchmarkStage::ProjectLinkE2e, "project-link");
        recorder.observe(MessageBenchmarkStage::ProjectLinkE2e, "project-link", 1);

        let completed = recorder.finish_recording().unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].stage(), MessageBenchmarkStage::ProjectLinkE2e);
    }

    #[test]
    fn recorder_rejects_an_explicitly_invalidated_observation() {
        let mut recorder = BenchmarkRecorder::default();
        recorder.invalidate();

        assert!(recorder.finish_recording().is_err());
    }

    #[test]
    fn recorder_excludes_observation_overhead_from_an_active_parent() {
        let mut recorder = BenchmarkRecorder::default();
        recorder.begin(MessageBenchmarkStage::ProjectLinkE2e, "project-link");
        recorder.exclude_observation_overhead(Duration::MAX);
        recorder.finish(MessageBenchmarkStage::ProjectLinkE2e, "project-link");
        recorder.observe(MessageBenchmarkStage::ProjectLinkE2e, "project-link", 1);

        let completed = recorder.finish_recording().unwrap();
        assert_eq!(completed[0].elapsed(), Duration::ZERO);
    }
}
