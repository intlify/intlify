// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Shared validation and typed-output preparation for platform exporters.
//!
//! This crate turns one checked linker outcome into an immutable borrowed
//! batch. It owns MF2 syntax and semantic validation plus language-neutral
//! argument signatures, but performs no platform rendering, filesystem I/O,
//! configuration decoding, reporting, or process-global scheduling.

mod diagnostic;
mod error;
mod limits;
mod preparation;
mod typed_output;

pub use diagnostic::{
    ExportMessageValidationFailure, MappedMessageDiagnostic, MappedMessageDiagnosticKind,
    MappedMessageLabel, MessageUtf8Span,
};
pub use error::{
    DiagnosticMappingInvariant, ExportPreparationError, ExportPreparationInvariant,
    OutcomeContractInvariant, TypedOutputInvariant,
};
pub use limits::{ExportValidationLimitConfigurationError, ExportValidationLimits};
pub use preparation::prepare_export;
pub use typed_output::{
    MessageArgumentSignature, ValidatedExportBatch, ValidatedMessageSignature, ValidatedTypedOutput,
};

#[cfg(test)]
mod tests {
    use super::{
        ExportMessageValidationFailure, ExportPreparationError, ExportValidationLimits,
        MappedMessageDiagnostic, MessageArgumentSignature, ValidatedExportBatch,
        ValidatedMessageSignature, ValidatedTypedOutput,
    };

    fn assert_send_sync<T: Send + Sync>() {}

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
    }
}
