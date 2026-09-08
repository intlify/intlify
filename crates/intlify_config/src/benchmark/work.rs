// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Versioned workload facts for input/structural/selection and the single-locale
//! canonicalization slice. These are not physical metrics, formal Resource
//! Policy observations, or whole-profile locale-policy / target work.

use serde::{Deserialize, Serialize};

use crate::input_limits::ValueCounts;
use crate::locale::{CanonicalizationFailure, ProviderFailure, Spelling};
use crate::materialize::{InputCounts, MaterializationError};
use crate::structural::selection::SelectorByteObservation;

use super::operation::{Analysis, Operation, Output, Prepared};
use super::quantity::Quantity;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum WorkKind {
    RawFileBytes,
    ParserTokensVisited,
    LogicalValueNodes,
    MaximumValueDepth,
    CollectionEntries,
    TotalDecodedStringBytes,
    MaximumDecodedStringBytes,
    ProfileDeclarations,
    MaximumDeclaredProfileIdBytes,
    StructuralAnalysisUnits,
    RetainedAdmissionIssues,
    RetainedSchemaFragments,
    RetainedSchemaIssuesIncludingAlternatives,
    SelectorIdBytes,
    LocaleOccurrences,
    RawLocaleIdentifierBytes,
    CanonicalLocaleIdentifierBytes,
    RetainedCanonicalLocaleValues,
    LocaleCorrectionSuggestions,
}

impl WorkKind {
    const INPUT: [Self; 14] = [
        Self::RawFileBytes,
        Self::ParserTokensVisited,
        Self::LogicalValueNodes,
        Self::MaximumValueDepth,
        Self::CollectionEntries,
        Self::TotalDecodedStringBytes,
        Self::MaximumDecodedStringBytes,
        Self::ProfileDeclarations,
        Self::MaximumDeclaredProfileIdBytes,
        Self::StructuralAnalysisUnits,
        Self::RetainedAdmissionIssues,
        Self::RetainedSchemaFragments,
        Self::RetainedSchemaIssuesIncludingAlternatives,
        Self::SelectorIdBytes,
    ];

    const LOCALE: [Self; 5] = [
        Self::LocaleOccurrences,
        Self::RawLocaleIdentifierBytes,
        Self::CanonicalLocaleIdentifierBytes,
        Self::RetainedCanonicalLocaleValues,
        Self::LocaleCorrectionSuggestions,
    ];

    pub(super) fn for_operation(operation: Operation) -> &'static [Self] {
        if operation == Operation::LocaleCanonicalization {
            &Self::LOCALE
        } else {
            &Self::INPUT
        }
    }

    const fn unit(self) -> WorkUnit {
        match self {
            Self::RawFileBytes => WorkUnit::Octet,
            Self::TotalDecodedStringBytes
            | Self::MaximumDecodedStringBytes
            | Self::MaximumDeclaredProfileIdBytes
            | Self::SelectorIdBytes
            | Self::RawLocaleIdentifierBytes
            | Self::CanonicalLocaleIdentifierBytes => WorkUnit::Utf8Octet,
            Self::ParserTokensVisited => WorkUnit::ParserToken,
            Self::LogicalValueNodes => WorkUnit::LogicalValue,
            Self::MaximumValueDepth => WorkUnit::ValueLevel,
            Self::CollectionEntries => WorkUnit::CollectionEntry,
            Self::ProfileDeclarations => WorkUnit::ProfileDeclaration,
            Self::StructuralAnalysisUnits => WorkUnit::ApplicableKeywordSubject,
            Self::RetainedAdmissionIssues
            | Self::RetainedSchemaFragments
            | Self::RetainedSchemaIssuesIncludingAlternatives => WorkUnit::RetainedRecord,
            Self::LocaleOccurrences => WorkUnit::LocaleOccurrence,
            Self::RetainedCanonicalLocaleValues => WorkUnit::CanonicalLocaleValue,
            Self::LocaleCorrectionSuggestions => WorkUnit::CorrectionSuggestion,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum WorkUnit {
    Octet,
    Utf8Octet,
    ParserToken,
    LogicalValue,
    ValueLevel,
    CollectionEntry,
    ProfileDeclaration,
    ApplicableKeywordSubject,
    RetainedRecord,
    LocaleOccurrence,
    CanonicalLocaleValue,
    CorrectionSuggestion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum WorkStage {
    OperationResult,
    PreparedInput,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum UnavailableWork {
    RawInputNotComplete,
    ProfileContainerNotAdmitted,
    SchemaPrerequisiteUnavailable,
    LocaleNotCanonicalized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case", deny_unknown_fields)]
enum WorkValue {
    Exact { value: Quantity },
    AtLeast { value: Quantity },
    Unavailable { reason: UnavailableWork },
    NotApplicable {},
}

impl WorkValue {
    const fn exact(value: u64) -> Self {
        Self::Exact {
            value: Quantity::new(value),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkFact {
    kind: WorkKind,
    unit: WorkUnit,
    stage: WorkStage,
    observation: WorkValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct LogicalWork {
    profile_identity: String,
    profile_revision: String,
    operation: Operation,
    facts: Vec<WorkFact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkFailure {
    OperationMismatch,
    InvalidOrdinaryResult,
    UnrepresentableCounter,
}

impl LogicalWork {
    pub(super) fn observe(prepared: &Prepared, output: &Output) -> Result<Self, WorkFailure> {
        let mut work = Self {
            profile_identity: if prepared.operation() == Operation::LocaleCanonicalization {
                "intlify-config-minimum-single-locale-work"
            } else {
                "intlify-config-minimum-input-work"
            }
            .into(),
            profile_revision: "0".into(),
            operation: prepared.operation(),
            facts: WorkKind::for_operation(prepared.operation())
                .iter()
                .map(|&kind| WorkFact {
                    kind,
                    unit: kind.unit(),
                    stage: WorkStage::NotApplicable,
                    observation: WorkValue::NotApplicable {},
                })
                .collect(),
        };
        match (prepared, output) {
            (Prepared::Entry { .. }, Output::Entry(Ok(doc))) => {
                work.input(doc.counts(), WorkStage::OperationResult);
            }
            (Prepared::Entry { .. }, Output::Entry(Err(error))) => work.failed_input(error),
            (Prepared::Structural { .. }, Output::Structural(Ok(analysis))) => {
                work.input(analysis.benchmark_input_counts(), WorkStage::PreparedInput);
                work.structural(analysis, WorkStage::OperationResult);
            }
            (Prepared::Authoring(analysis), Output::Authoring(Ok(Some(_)))) => {
                if !analysis.is_complete() {
                    return Err(WorkFailure::InvalidOrdinaryResult);
                }
                work.input(analysis.benchmark_input_counts(), WorkStage::PreparedInput);
                work.structural(analysis, WorkStage::PreparedInput);
            }
            (Prepared::Select { analysis, selector }, Output::Select(Ok(_))) => {
                work.input(analysis.benchmark_input_counts(), WorkStage::PreparedInput);
                work.structural(analysis, WorkStage::PreparedInput);
                let observation = match selector.benchmark_id_bytes() {
                    SelectorByteObservation::NotApplicable => WorkValue::NotApplicable {},
                    SelectorByteObservation::Exact(value) => WorkValue::exact(value),
                    SelectorByteObservation::AtLeast(value) => WorkValue::AtLeast {
                        value: Quantity::new(value),
                    },
                };
                let stage = if observation == (WorkValue::NotApplicable {}) {
                    WorkStage::NotApplicable
                } else {
                    WorkStage::PreparedInput
                };
                work.set(WorkKind::SelectorIdBytes, stage, observation);
            }
            (Prepared::Structural { .. }, Output::Structural(Err(_)))
            | (Prepared::Authoring(_), Output::Authoring(_))
            | (Prepared::Select { .. }, Output::Select(Err(_))) => {
                return Err(WorkFailure::InvalidOrdinaryResult);
            }
            (Prepared::Locale { input, .. }, Output::Locale(result)) => {
                work.set(
                    WorkKind::LocaleOccurrences,
                    WorkStage::PreparedInput,
                    WorkValue::exact(1),
                );
                work.set(
                    WorkKind::RawLocaleIdentifierBytes,
                    WorkStage::PreparedInput,
                    WorkValue::exact(
                        u64::try_from(input.len())
                            .map_err(|_| WorkFailure::UnrepresentableCounter)?,
                    ),
                );
                let bytes = match result {
                    Ok(value) => WorkValue::exact(
                        u64::try_from(value.locale().as_str().len())
                            .map_err(|_| WorkFailure::UnrepresentableCounter)?,
                    ),
                    Err(CanonicalizationFailure::ByteLimit {
                        spelling: Spelling::Canonical,
                        actual,
                        ..
                    }) => WorkValue::exact(*actual),
                    Err(
                        CanonicalizationFailure::ByteLimit {
                            spelling: Spelling::Raw,
                            ..
                        }
                        | CanonicalizationFailure::Provider(ProviderFailure::InvalidIdentifier),
                    ) => WorkValue::Unavailable {
                        reason: UnavailableWork::LocaleNotCanonicalized,
                    },
                    _ => return Err(WorkFailure::InvalidOrdinaryResult),
                };
                work.set(
                    WorkKind::CanonicalLocaleIdentifierBytes,
                    WorkStage::OperationResult,
                    bytes,
                );
                work.set(
                    WorkKind::RetainedCanonicalLocaleValues,
                    WorkStage::OperationResult,
                    WorkValue::exact(u64::from(result.is_ok())),
                );
                work.set(
                    WorkKind::LocaleCorrectionSuggestions,
                    WorkStage::OperationResult,
                    WorkValue::exact(u64::from(
                        result
                            .as_ref()
                            .is_ok_and(|value| value.suggested_replacement().is_some()),
                    )),
                );
            }
            _ => return Err(WorkFailure::OperationMismatch),
        }
        Ok(work)
    }

    fn set(&mut self, kind: WorkKind, stage: WorkStage, observation: WorkValue) {
        let fact = self
            .facts
            .iter_mut()
            .find(|fact| fact.kind == kind)
            .expect("complete finite owner work vocabulary");
        fact.stage = stage;
        fact.observation = observation;
    }

    fn input(&mut self, counts: InputCounts, stage: WorkStage) {
        self.set(
            WorkKind::RawFileBytes,
            stage,
            WorkValue::exact(counts.file_bytes),
        );
        self.set(
            WorkKind::ParserTokensVisited,
            stage,
            WorkValue::exact(counts.parser_tokens),
        );
        self.values(Some(counts.value), stage);
    }

    fn values(&mut self, value: Option<ValueCounts>, stage: WorkStage) {
        let kinds = [
            WorkKind::LogicalValueNodes,
            WorkKind::MaximumValueDepth,
            WorkKind::CollectionEntries,
            WorkKind::TotalDecodedStringBytes,
            WorkKind::MaximumDecodedStringBytes,
        ];
        let observations = value.map(|value| {
            [
                value.nodes,
                value.depth,
                value.collection_entries,
                value.total_string_bytes,
                value.single_string_bytes,
            ]
        });
        for (index, kind) in kinds.into_iter().enumerate() {
            self.set(
                kind,
                stage,
                observations.map_or(
                    WorkValue::Unavailable {
                        reason: UnavailableWork::RawInputNotComplete,
                    },
                    |values| WorkValue::exact(values[index]),
                ),
            );
        }
    }

    fn failed_input(&mut self, error: &MaterializationError) {
        self.set(
            WorkKind::RawFileBytes,
            WorkStage::OperationResult,
            WorkValue::exact(error.progress.file_bytes),
        );
        self.set(
            WorkKind::ParserTokensVisited,
            WorkStage::OperationResult,
            WorkValue::exact(error.progress.parser_tokens_visited),
        );
        self.values(
            error.progress.complete_value.as_deref().copied(),
            WorkStage::OperationResult,
        );
    }

    fn structural(&mut self, analysis: &Analysis, stage: WorkStage) {
        let profiles = analysis.benchmark_profile_counts();
        for (kind, observation) in [
            (WorkKind::ProfileDeclarations, profiles.map(|value| value.0)),
            (
                WorkKind::MaximumDeclaredProfileIdBytes,
                profiles.map(|value| value.1),
            ),
        ] {
            self.set(
                kind,
                stage,
                observation.map_or(
                    WorkValue::Unavailable {
                        reason: UnavailableWork::ProfileContainerNotAdmitted,
                    },
                    WorkValue::exact,
                ),
            );
        }
        self.set(
            WorkKind::StructuralAnalysisUnits,
            stage,
            analysis.benchmark_structural_units().map_or(
                WorkValue::Unavailable {
                    reason: UnavailableWork::SchemaPrerequisiteUnavailable,
                },
                WorkValue::exact,
            ),
        );
        let (admission, fragments, issues) = analysis.benchmark_retained_record_counts();
        for (kind, value) in [
            (WorkKind::RetainedAdmissionIssues, admission),
            (WorkKind::RetainedSchemaFragments, fragments),
            (WorkKind::RetainedSchemaIssuesIncludingAlternatives, issues),
        ] {
            self.set(kind, stage, WorkValue::exact(value));
        }
    }

    /// Full equality includes vocabulary order, units, stages, unavailable state,
    /// and exact values. `expected` must come from independently checked fixture
    /// preparation, never from the decoded record being admitted.
    pub(super) fn matches_expected(&self, expected: &Self) -> bool {
        self == expected
    }
}

#[cfg(test)]
mod tests;
