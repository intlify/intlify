// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Non-product observation surface for the dedicated messages benchmark.
//!
//! The feature accepts only ordinary checked outcomes, options, batches, and
//! exporter instances. It exposes completed observations and never admits a
//! caller-authored plan or partially constructed artifact set.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use intlify_contract::{DeliveryUnitId, Locale};
use intlify_linker::LinkOutcome;

use crate::observation::{ExportBenchmarkObserver, ExportBenchmarkStage};
use crate::{
    EsmExporter, ExportArtifactPath, ExportArtifactSet, ExportError, ExportPreparationError,
    ExportValidationLimits, ValidatedExportBatch,
};

/// Stable benchmark-facing stage identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BenchmarkExportStage {
    SelectedMessageParse,
    MessageSemanticValidation,
    PortableDiagnosticMapping,
    ArgumentSignatureDerivation,
    ValidatedExportBatchConstruction,
    LocaleAssetRendering,
    LoaderMapRendering,
    TypedKeyAccessorRendering,
    ExportArtifactSetConstruction,
}

impl BenchmarkExportStage {
    /// Return the exact active benchmark phase.
    #[must_use]
    pub const fn phase(self) -> &'static str {
        match self {
            Self::SelectedMessageParse
            | Self::MessageSemanticValidation
            | Self::PortableDiagnosticMapping
            | Self::ArgumentSignatureDerivation
            | Self::ValidatedExportBatchConstruction => "message_export_prepare",
            Self::LocaleAssetRendering
            | Self::LoaderMapRendering
            | Self::TypedKeyAccessorRendering
            | Self::ExportArtifactSetConstruction => "message_export_esm",
        }
    }

    /// Return the exact active benchmark cost and boundary identifier.
    #[must_use]
    pub const fn cost(self) -> &'static str {
        match self {
            Self::SelectedMessageParse => "selected_message_parse",
            Self::MessageSemanticValidation => "message_semantic_validation",
            Self::PortableDiagnosticMapping => "portable_diagnostic_mapping",
            Self::ArgumentSignatureDerivation => "argument_signature_derivation",
            Self::ValidatedExportBatchConstruction => "validated_export_batch_construction",
            Self::LocaleAssetRendering => "locale_asset_rendering",
            Self::LoaderMapRendering => "loader_map_rendering",
            Self::TypedKeyAccessorRendering => "typed_key_accessor_rendering",
            Self::ExportArtifactSetConstruction => "export_artifact_set_construction",
        }
    }

    /// Return the stable interval-boundary identifier.
    #[must_use]
    pub const fn boundary_id(self) -> &'static str {
        self.cost()
    }
}

/// One completed non-overlapping export-stage measurement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkExportMeasurement {
    stage: BenchmarkExportStage,
    elapsed: Duration,
    checksum: u32,
}

impl BenchmarkExportMeasurement {
    /// Return the measured stage.
    #[must_use]
    pub const fn stage(&self) -> BenchmarkExportStage {
        self.stage
    }

    /// Return the unadjusted monotonic-clock duration.
    #[must_use]
    pub const fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Return the deterministic semantic observation checksum.
    #[must_use]
    pub const fn checksum(&self) -> u32 {
        self.checksum
    }
}

/// Locale classification supplied by the concrete exporter branch.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum BenchmarkLocaleBucket {
    Locale(Locale),
    Shared,
}

/// Delivery-unit classification supplied by the concrete exporter branch.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum BenchmarkDeliveryUnitBucket {
    Unit(DeliveryUnitId),
    Shared,
}

/// Exact path-to-semantic-bucket relation for one checked artifact.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct BenchmarkArtifactAssociation {
    path: ExportArtifactPath,
    locale: BenchmarkLocaleBucket,
    delivery_unit: BenchmarkDeliveryUnitBucket,
}

impl BenchmarkArtifactAssociation {
    #[must_use]
    pub const fn new(
        path: ExportArtifactPath,
        locale: BenchmarkLocaleBucket,
        delivery_unit: BenchmarkDeliveryUnitBucket,
    ) -> Self {
        Self {
            path,
            locale,
            delivery_unit,
        }
    }

    #[must_use]
    pub const fn path(&self) -> &ExportArtifactPath {
        &self.path
    }

    #[must_use]
    pub const fn locale(&self) -> &BenchmarkLocaleBucket {
        &self.locale
    }

    #[must_use]
    pub const fn delivery_unit(&self) -> &BenchmarkDeliveryUnitBucket {
        &self.delivery_unit
    }
}

/// Checked artifact set paired with exact exporter-supplied classifications.
#[derive(Debug)]
pub struct BenchmarkExportObservation {
    artifacts: ExportArtifactSet,
    associations: Box<[BenchmarkArtifactAssociation]>,
    entry_roots: Box<[ExportArtifactPath]>,
}

impl BenchmarkExportObservation {
    fn try_new(
        artifacts: ExportArtifactSet,
        mut associations: Vec<BenchmarkArtifactAssociation>,
        entry_roots: Vec<ExportArtifactPath>,
    ) -> Result<Self, ExportError> {
        associations.sort();
        let unique_entry_roots = entry_roots.iter().collect::<BTreeSet<_>>();
        if associations.len() != artifacts.artifacts().len()
            || associations
                .iter()
                .zip(artifacts.artifacts())
                .any(|(association, artifact)| association.path() != artifact.logical_path())
            || associations
                .windows(2)
                .any(|pair| pair[0].path() == pair[1].path())
            || unique_entry_roots.len() != entry_roots.len()
            || entry_roots.iter().any(|root| {
                artifacts
                    .artifacts()
                    .binary_search_by(|artifact| artifact.logical_path().cmp(root))
                    .is_err()
            })
        {
            return Err(crate::esm::artifact_assembly_error());
        }
        Ok(Self {
            artifacts,
            associations: associations.into_boxed_slice(),
            entry_roots: entry_roots.into_boxed_slice(),
        })
    }

    #[must_use]
    pub const fn artifacts(&self) -> &ExportArtifactSet {
        &self.artifacts
    }

    #[must_use]
    pub const fn associations(&self) -> &[BenchmarkArtifactAssociation] {
        &self.associations
    }

    #[must_use]
    pub const fn entry_roots(&self) -> &[ExportArtifactPath] {
        &self.entry_roots
    }
}

/// Successful preparation result and its exact five stage observations.
#[derive(Debug)]
pub struct BenchmarkPreparationExecution<'a> {
    batch: Option<ValidatedExportBatch<'a>>,
    measurements: Box<[BenchmarkExportMeasurement]>,
}

impl<'a> BenchmarkPreparationExecution<'a> {
    #[must_use]
    pub const fn batch(&self) -> Option<&ValidatedExportBatch<'a>> {
        self.batch.as_ref()
    }

    #[must_use]
    pub const fn measurements(&self) -> &[BenchmarkExportMeasurement] {
        &self.measurements
    }

    #[must_use]
    pub fn into_batch(self) -> Option<ValidatedExportBatch<'a>> {
        self.batch
    }
}

/// Successful ESM generation result and its exact four stage observations.
#[derive(Debug)]
pub struct BenchmarkEsmExecution {
    observation: BenchmarkExportObservation,
    measurements: Box<[BenchmarkExportMeasurement]>,
}

impl BenchmarkEsmExecution {
    #[must_use]
    pub const fn observation(&self) -> &BenchmarkExportObservation {
        &self.observation
    }

    #[must_use]
    pub const fn measurements(&self) -> &[BenchmarkExportMeasurement] {
        &self.measurements
    }

    #[must_use]
    pub fn into_observation(self) -> BenchmarkExportObservation {
        self.observation
    }
}

#[doc(hidden)]
pub fn benchmark_prepare_export(
    outcome: &LinkOutcome,
    limits: ExportValidationLimits,
) -> Result<BenchmarkPreparationExecution<'_>, ExportPreparationError> {
    let mut recorder = BenchmarkExportRecorder::default();
    let batch = crate::preparation::prepare_export_with_observer(outcome, limits, &mut recorder)?;
    let measurements = recorder.finish_preparation()?;
    Ok(BenchmarkPreparationExecution {
        batch,
        measurements: measurements.into_boxed_slice(),
    })
}

#[doc(hidden)]
pub fn benchmark_export_esm(
    exporter: &EsmExporter,
    batch: &ValidatedExportBatch<'_>,
) -> Result<BenchmarkEsmExecution, ExportError> {
    let mut recorder = BenchmarkExportRecorder::default();
    let artifacts = crate::esm::export_with_observer(exporter, batch, &mut recorder)?;
    let (measurements, associations, entry_roots) = recorder.finish_esm()?;
    Ok(BenchmarkEsmExecution {
        observation: BenchmarkExportObservation::try_new(artifacts, associations, entry_roots)?,
        measurements: measurements.into_boxed_slice(),
    })
}

#[derive(Debug)]
struct ActiveStage {
    stage: BenchmarkExportStage,
    started: Instant,
}

#[derive(Debug, Default)]
struct BenchmarkExportRecorder {
    active: Option<ActiveStage>,
    measurements: Vec<BenchmarkExportMeasurement>,
    associations: Vec<BenchmarkArtifactAssociation>,
    entry_roots: Vec<ExportArtifactPath>,
    invariant_failed: bool,
}

type FinishedEsmObservation = (
    Vec<BenchmarkExportMeasurement>,
    Vec<BenchmarkArtifactAssociation>,
    Vec<ExportArtifactPath>,
);

impl BenchmarkExportRecorder {
    fn finish_preparation(self) -> Result<Vec<BenchmarkExportMeasurement>, ExportPreparationError> {
        let expected = [
            BenchmarkExportStage::SelectedMessageParse,
            BenchmarkExportStage::MessageSemanticValidation,
            BenchmarkExportStage::PortableDiagnosticMapping,
            BenchmarkExportStage::ArgumentSignatureDerivation,
            BenchmarkExportStage::ValidatedExportBatchConstruction,
        ];
        if self.invariant_failed
            || self.active.is_some()
            || self.measurements.iter().map(|item| item.stage).ne(expected)
            || !self.associations.is_empty()
            || !self.entry_roots.is_empty()
        {
            return Err(ExportPreparationError::InternalInvariant(
                crate::ExportPreparationInvariant::TypedOutput(
                    crate::TypedOutputInvariant::ModelRelationMismatch,
                ),
            ));
        }
        Ok(self.measurements)
    }

    fn finish_esm(self) -> Result<FinishedEsmObservation, ExportError> {
        let expected = [
            BenchmarkExportStage::LocaleAssetRendering,
            BenchmarkExportStage::LoaderMapRendering,
            BenchmarkExportStage::TypedKeyAccessorRendering,
            BenchmarkExportStage::ExportArtifactSetConstruction,
        ];
        if self.invariant_failed
            || self.active.is_some()
            || self.measurements.iter().map(|item| item.stage).ne(expected)
        {
            return Err(crate::esm::artifact_assembly_error());
        }
        Ok((self.measurements, self.associations, self.entry_roots))
    }
}

impl ExportBenchmarkObserver for BenchmarkExportRecorder {
    fn begin(&mut self, stage: ExportBenchmarkStage) {
        if self.active.is_some() {
            self.invariant_failed = true;
            return;
        }
        self.active = Some(ActiveStage {
            stage: benchmark_stage(stage),
            started: Instant::now(),
        });
    }

    fn finish(&mut self, stage: ExportBenchmarkStage, checksum: u32) {
        let expected = benchmark_stage(stage);
        let Some(active) = self.active.take() else {
            self.invariant_failed = true;
            return;
        };
        if active.stage != expected {
            self.invariant_failed = true;
            return;
        }
        self.measurements.push(BenchmarkExportMeasurement {
            stage: expected,
            elapsed: active.started.elapsed(),
            checksum,
        });
    }

    fn associate(&mut self, association: BenchmarkArtifactAssociation, entry_root: bool) {
        if entry_root {
            self.entry_roots.push(association.path().clone());
        }
        self.associations.push(association);
    }
}

const fn benchmark_stage(stage: ExportBenchmarkStage) -> BenchmarkExportStage {
    match stage {
        ExportBenchmarkStage::SelectedMessageParse => BenchmarkExportStage::SelectedMessageParse,
        ExportBenchmarkStage::MessageSemanticValidation => {
            BenchmarkExportStage::MessageSemanticValidation
        }
        ExportBenchmarkStage::PortableDiagnosticMapping => {
            BenchmarkExportStage::PortableDiagnosticMapping
        }
        ExportBenchmarkStage::ArgumentSignatureDerivation => {
            BenchmarkExportStage::ArgumentSignatureDerivation
        }
        ExportBenchmarkStage::ValidatedExportBatchConstruction => {
            BenchmarkExportStage::ValidatedExportBatchConstruction
        }
        ExportBenchmarkStage::LocaleAssetRendering => BenchmarkExportStage::LocaleAssetRendering,
        ExportBenchmarkStage::LoaderMapRendering => BenchmarkExportStage::LoaderMapRendering,
        ExportBenchmarkStage::TypedKeyAccessorRendering => {
            BenchmarkExportStage::TypedKeyAccessorRendering
        }
        ExportBenchmarkStage::ExportArtifactSetConstruction => {
            BenchmarkExportStage::ExportArtifactSetConstruction
        }
    }
}
