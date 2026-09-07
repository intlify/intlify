// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Workload facts for the active input/structural/selection slice. These are not
//! physical metrics, formal Resource Policy observations, or future locale/target
//! work. Later phases must extend the owner workload profile explicitly.

use serde::{Deserialize, Serialize};

use crate::input_limits::ValueCounts;
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
}

impl WorkKind {
    pub(super) const ALL: [Self; 14] = [
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

    const fn unit(self) -> WorkUnit {
        match self {
            Self::RawFileBytes => WorkUnit::Octet,
            Self::TotalDecodedStringBytes
            | Self::MaximumDecodedStringBytes
            | Self::MaximumDeclaredProfileIdBytes
            | Self::SelectorIdBytes => WorkUnit::Utf8Octet,
            Self::ParserTokensVisited => WorkUnit::ParserToken,
            Self::LogicalValueNodes => WorkUnit::LogicalValue,
            Self::MaximumValueDepth => WorkUnit::ValueLevel,
            Self::CollectionEntries => WorkUnit::CollectionEntry,
            Self::ProfileDeclarations => WorkUnit::ProfileDeclaration,
            Self::StructuralAnalysisUnits => WorkUnit::ApplicableKeywordSubject,
            Self::RetainedAdmissionIssues
            | Self::RetainedSchemaFragments
            | Self::RetainedSchemaIssuesIncludingAlternatives => WorkUnit::RetainedRecord,
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
}

impl LogicalWork {
    pub(super) fn observe(prepared: &Prepared, output: &Output) -> Result<Self, WorkFailure> {
        let mut work = Self {
            profile_identity: "intlify-config-minimum-input-work".into(),
            profile_revision: "0".into(),
            operation: prepared.operation(),
            facts: WorkKind::ALL
                .into_iter()
                .map(|kind| WorkFact {
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
