// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Shared preparation and portable output contracts for platform exporters.
//!
//! This crate turns one checked linker outcome into an immutable borrowed
//! batch. It owns MF2 syntax and semantic validation plus language-neutral
//! argument signatures, then provides the common exporter, artifact, output
//! limit, and error boundaries. It performs no filesystem I/O, configuration
//! decoding, reporting, destination mapping, or process-global scheduling.

mod artifact;
mod diagnostic;
mod error;
mod esm;
mod export_error;
mod exporter;
mod limits;
mod preparation;
mod typed_output;
#[allow(dead_code)]
mod writer;

pub use artifact::{
    ExportArtifact, ExportArtifactFormatVersion, ExportArtifactKind, ExportArtifactMetadata,
    ExportArtifactPath, ExportArtifactPathSegment, ExportArtifactPayload,
    ExportArtifactRelationship, ExportArtifactRelationshipKind, ExportArtifactSet, ExportMediaType,
};
pub use diagnostic::{
    ExportMessageValidationFailure, MappedMessageDiagnostic, MappedMessageDiagnosticKind,
    MappedMessageLabel, MessageUtf8Span,
};
pub use error::{
    DiagnosticMappingInvariant, ExportPreparationError, ExportPreparationInvariant,
    OutcomeContractInvariant, TypedOutputInvariant,
};
pub use esm::{EagerLocales, EsmExporter, EsmExporterOptions, EsmExporterOptionsError};
pub use export_error::{
    ExportError, ExportErrorEvidence, ExportErrorKind, GenerationFailedEvidence,
    GenerationLocation, GenerationStage, InternalInvariantEvidence, InternalInvariantViolation,
    InvalidOutputEvidence, InvalidOutputViolation, OutputLimitCounter, OutputLimitExceededEvidence,
    OutputLimitObservation, UnsupportedBatchEvidence, UnsupportedBatchFeature,
    UnsupportedBatchLocation,
};
pub use exporter::PlatformExporter;
pub use limits::{ExportValidationLimitConfigurationError, ExportValidationLimits};
pub use preparation::prepare_export;
pub use typed_output::{
    MessageArgumentSignature, ValidatedExportBatch, ValidatedMessageSignature, ValidatedTypedOutput,
};

#[cfg(test)]
mod tests {
    use super::{
        EsmExporter, EsmExporterOptions, EsmExporterOptionsError, ExportArtifact,
        ExportArtifactFormatVersion, ExportArtifactKind, ExportArtifactMetadata,
        ExportArtifactPath, ExportArtifactPathSegment, ExportArtifactPayload,
        ExportArtifactRelationship, ExportArtifactSet, ExportError, ExportMessageValidationFailure,
        ExportPreparationError, ExportValidationLimits, MappedMessageDiagnostic,
        MessageArgumentSignature, PlatformExporter, ValidatedExportBatch,
        ValidatedMessageSignature, ValidatedTypedOutput,
    };

    fn assert_send_sync<T: Send + Sync>() {}
    fn assert_send<T: Send>() {}

    #[test]
    fn public_preparation_values_are_send_and_sync() {
        assert_send_sync::<ExportValidationLimits>();
        assert_send_sync::<ExportMessageValidationFailure>();
        assert_send_sync::<ExportPreparationError>();
        assert_send_sync::<MappedMessageDiagnostic>();
        assert_send_sync::<MessageArgumentSignature>();
        assert_send_sync::<ValidatedExportBatch<'static>>();
        assert_send_sync::<ValidatedTypedOutput<'static>>();
        assert_send_sync::<ValidatedMessageSignature<'static>>();
        assert_send_sync::<ExportArtifactSet>();
        assert_send_sync::<ExportArtifact>();
        assert_send_sync::<ExportArtifactPath>();
        assert_send_sync::<ExportArtifactPathSegment>();
        assert_send_sync::<ExportArtifactKind>();
        assert_send_sync::<ExportArtifactFormatVersion>();
        assert_send_sync::<ExportArtifactPayload>();
        assert_send_sync::<ExportArtifactMetadata>();
        assert_send_sync::<ExportArtifactRelationship>();
        assert_send_sync::<ExportError>();
        assert_send_sync::<EsmExporterOptions>();
        assert_send_sync::<EsmExporterOptionsError>();
        assert_send::<EsmExporter>();
        assert_send::<Box<dyn PlatformExporter>>();
    }
}
