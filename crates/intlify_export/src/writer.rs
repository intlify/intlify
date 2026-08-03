// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Bounded byte generation shared by built-in platform renderers.
//!
//! Writers check every append against both the per-artifact and invocation-wide
//! payload ceilings before mutating their buffer. The budget counts only
//! successfully finished payloads and performs no I/O or encoding policy.

use crate::export_error::OutputLimitAccumulator;
use crate::{ExportArtifactPayload, ExportError, OutputLimitCounter};

/// Set-scoped payload accounting for one exporter invocation.
#[derive(Debug, Default)]
pub(crate) struct ExportPayloadBudget {
    committed: u64,
}

impl ExportPayloadBudget {
    pub(crate) const fn new() -> Self {
        Self { committed: 0 }
    }

    pub(crate) fn writer(&mut self) -> BoundedExportWriter<'_> {
        BoundedExportWriter {
            budget: self,
            bytes: Vec::new(),
        }
    }
}

/// One mutable byte buffer governed by an [`ExportPayloadBudget`].
#[derive(Debug)]
pub(crate) struct BoundedExportWriter<'a> {
    budget: &'a mut ExportPayloadBudget,
    bytes: Vec<u8>,
}

impl BoundedExportWriter<'_> {
    pub(crate) fn try_extend_from_slice(&mut self, bytes: &[u8]) -> Result<(), ExportError> {
        checked_output_lengths(
            PendingOutputLengths {
                current_artifact: self.bytes.len(),
                appended: bytes.len(),
            },
            self.budget.committed,
        )?;
        self.bytes.reserve(bytes.len());
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    pub(crate) fn try_write_str(&mut self, value: &str) -> Result<(), ExportError> {
        self.try_extend_from_slice(value.as_bytes())
    }

    pub(crate) fn finish(self) -> Result<ExportArtifactPayload, ExportError> {
        let (_, total) = checked_output_lengths(
            PendingOutputLengths {
                current_artifact: 0,
                appended: self.bytes.len(),
            },
            self.budget.committed,
        )?;
        self.budget.committed = total;
        Ok(ExportArtifactPayload::from_checked(
            self.bytes.into_boxed_slice(),
        ))
    }
}

#[derive(Debug, Clone, Copy)]
struct PendingOutputLengths {
    current_artifact: usize,
    appended: usize,
}

fn checked_output_lengths(
    pending: PendingOutputLengths,
    committed_set: u64,
) -> Result<(usize, u64), ExportError> {
    let mut limits = OutputLimitAccumulator::new();
    let mut artifact_total = 0;
    limits.add_usize(
        OutputLimitCounter::PayloadBytesPerArtifact,
        &mut artifact_total,
        pending.current_artifact,
    );
    limits.add_usize(
        OutputLimitCounter::PayloadBytesPerArtifact,
        &mut artifact_total,
        pending.appended,
    );

    let mut set_total = committed_set;
    limits.add_usize(
        OutputLimitCounter::PayloadBytesPerSet,
        &mut set_total,
        pending.current_artifact,
    );
    limits.add_usize(
        OutputLimitCounter::PayloadBytesPerSet,
        &mut set_total,
        pending.appended,
    );
    let next_artifact = pending.current_artifact.checked_add(pending.appended);
    if next_artifact.is_none() {
        limits.observe_overflow(OutputLimitCounter::PayloadBytesPerArtifact);
    }
    if let Some(error) = limits.finish() {
        return Err(error);
    }
    let Some(next_artifact) = next_artifact else {
        return Err(crate::ExportError::internal_invariant(
            crate::InternalInvariantEvidence::new(
                crate::InternalInvariantViolation::OutputBudgetAccounting,
            ),
        ));
    };
    Ok((next_artifact, set_total))
}

#[cfg(test)]
mod tests {
    use super::{checked_output_lengths, ExportPayloadBudget, PendingOutputLengths};
    use crate::{ExportErrorEvidence, OutputLimitCounter, OutputLimitObservation};

    fn limit(error: &crate::ExportError) -> (OutputLimitCounter, OutputLimitObservation) {
        let ExportErrorEvidence::OutputLimitExceeded(evidence) = error.evidence() else {
            panic!("expected output limit evidence");
        };
        (evidence.counter(), evidence.observation())
    }

    #[test]
    fn numeric_boundary_check_accepts_exact_payload_ceilings_without_allocating_them() {
        let per_artifact = OutputLimitCounter::PayloadBytesPerArtifact.ceiling() as usize;
        let per_set = OutputLimitCounter::PayloadBytesPerSet.ceiling();
        assert_eq!(
            checked_output_lengths(
                PendingOutputLengths {
                    current_artifact: per_artifact - 1,
                    appended: 1,
                },
                per_set - per_artifact as u64,
            ),
            Ok((per_artifact, per_set))
        );
    }

    #[test]
    fn per_artifact_payload_limit_precedes_the_set_limit() {
        let per_artifact = OutputLimitCounter::PayloadBytesPerArtifact.ceiling() as usize;
        let error = checked_output_lengths(
            PendingOutputLengths {
                current_artifact: per_artifact,
                appended: 1,
            },
            OutputLimitCounter::PayloadBytesPerSet.ceiling(),
        )
        .unwrap_err();
        assert_eq!(
            limit(&error),
            (
                OutputLimitCounter::PayloadBytesPerArtifact,
                OutputLimitObservation::Exact(per_artifact as u64 + 1),
            )
        );
    }

    #[test]
    fn small_writers_commit_only_finished_exact_bytes() {
        let mut budget = ExportPayloadBudget::new();
        {
            let mut abandoned = budget.writer();
            abandoned.try_write_str("discarded").unwrap();
        }
        let payload = {
            let mut writer = budget.writer();
            writer.try_write_str("export").unwrap();
            writer.try_extend_from_slice(&[0, 0xff]).unwrap();
            writer.finish().unwrap()
        };
        assert_eq!(payload.as_bytes(), b"export\0\xff");
        assert_eq!(budget.committed, 8);
    }
}
