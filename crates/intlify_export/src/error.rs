// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Closed operational and invariant failures from shared export preparation.

use ox_mf2_parser::SemanticInvariantError;

use crate::ExportMessageValidationFailure;

/// Complete failure from preparing one checked linker outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportPreparationError {
    MessageValidation(ExportMessageValidationFailure),
    SemanticModelConstruction(SemanticInvariantError),
    SemanticValidation(SemanticInvariantError),
    InternalInvariant(ExportPreparationInvariant),
}

impl core::fmt::Display for ExportPreparationError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MessageValidation(failure) => write!(
                formatter,
                "{} message validation diagnostics prevent export",
                failure.total_diagnostics()
            ),
            Self::SemanticModelConstruction(error) => {
                write!(formatter, "semantic model construction failed: {error}")
            }
            Self::SemanticValidation(error) => {
                write!(formatter, "semantic validation failed: {error}")
            }
            Self::InternalInvariant(invariant) => {
                write!(
                    formatter,
                    "export preparation invariant failed: {invariant:?}"
                )
            }
        }
    }
}

impl std::error::Error for ExportPreparationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::SemanticModelConstruction(error) | Self::SemanticValidation(error) => Some(error),
            Self::MessageValidation(_) | Self::InternalInvariant(_) => None,
        }
    }
}

/// Impossible contradiction selected by the preparation boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportPreparationInvariant {
    ParserFailure,
    DiagnosticCountOverflow,
    DiagnosticMapping(DiagnosticMappingInvariant),
    OutcomeContract(OutcomeContractInvariant),
    TypedOutput(TypedOutputInvariant),
}

/// Contradiction between a parser-owned diagnostic and portable mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticMappingInvariant {
    LabelCountExceeded,
    InvalidPrimarySpan,
    InvalidLabelSpan,
    ForeignLabelSource,
}

/// Contradiction in the checked bundle-plan and definition snapshot surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutcomeContractInvariant {
    DuplicatePlanCoordinate,
    NonCanonicalPlanOrder,
    DuplicateLogicalMessage,
    NonCanonicalMessageOrder,
    DefinitionSnapshotMismatch,
}

/// Contradiction in the checked model-to-signature relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypedOutputInvariant {
    ModelRelationMismatch,
    SignatureKeyMismatch,
    DuplicateArgument,
    NonCanonicalArgumentOrder,
}
