// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Checked-in finite fixture expectations, never learned from measured samples.
//! Only this gate constructs the private fixture passed to normal collection.
//! The registry pins logical observations, not machine performance or 017 IDs.

use serde::{Deserialize, Serialize};

use crate::benchmark::observation::{Digest, Frame, Observation};
use crate::benchmark::operation::Prepared;
use crate::benchmark::work::LogicalWork;

use super::context::{self, ContextFailure};
use super::prepare::{prepare, Candidate, PreparationFailure};
use super::{declarations, Declaration};

const IDENTITY: &str = "intlify-config-minimum-fixture-expectations";
const REVISION: &str = "1";
const OBSERVATION_CODEC: &str = "intlify-config-minimum-observation/0";
const EXPECTATIONS: &str = include_str!("expectations-v1.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Document {
    identity: String,
    revision: String,
    observation_codec: String,
    rows: Vec<Row>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Row {
    declaration: Declaration,
    input_context: Digest,
    result: Observation,
    logical_work: Digest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::benchmark) enum RegistryFailure {
    Malformed,
    Identity,
    Revision,
    ObservationCodec,
    Inventory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::benchmark) enum FixtureFailure {
    UndeclaredCase,
    Preparation(PreparationFailure),
    Context(ContextFailure),
    InputContextMismatch,
    ObservationEncoding,
    ResultMismatch,
    LogicalWorkMismatch,
}

/// No Deserialize or external registry override: only the compiled owner
/// expectations may authorize fixtures used by ordinary benchmark collection.
pub(in crate::benchmark) struct Registry {
    rows: Vec<Row>,
}

/// The operation and its expected observations remain immutable and inseparable.
/// There is no public constructor, Deserialize, mutable getter, or Profile output.
pub(in crate::benchmark) struct AdmittedFixture {
    declaration: Declaration,
    input_context: Digest,
    prepared: Prepared,
    expected: Observation,
    expected_work: LogicalWork,
}

impl Registry {
    pub(in crate::benchmark) fn load() -> Result<Self, RegistryFailure> {
        let document =
            serde_json::from_str(EXPECTATIONS).map_err(|_| RegistryFailure::Malformed)?;
        Self::admit_document(document)
    }

    fn admit_document(document: Document) -> Result<Self, RegistryFailure> {
        if document.identity != IDENTITY {
            return Err(RegistryFailure::Identity);
        }
        if document.revision != REVISION {
            return Err(RegistryFailure::Revision);
        }
        if document.observation_codec != OBSERVATION_CODEC {
            return Err(RegistryFailure::ObservationCodec);
        }
        // Exact order and equality detect missing, duplicate, unknown, moved,
        // or modified declarations, rather than accepting a valid prefix.
        let declared = declarations();
        if document.rows.len() != declared.len()
            || !document
                .rows
                .iter()
                .zip(declared)
                .all(|(row, case)| row.declaration == case)
        {
            return Err(RegistryFailure::Inventory);
        }
        Ok(Self {
            rows: document.rows,
        })
    }

    pub(in crate::benchmark) fn prepare(
        &self,
        declaration: &Declaration,
    ) -> Result<AdmittedFixture, FixtureFailure> {
        // Check membership before allocating/preparing an operation.
        self.row(declaration)?;
        self.admit_candidate(prepare(declaration).map_err(FixtureFailure::Preparation)?)
    }

    fn row(&self, declaration: &Declaration) -> Result<&Row, FixtureFailure> {
        self.rows
            .iter()
            .find(|row| row.declaration == *declaration)
            .ok_or(FixtureFailure::UndeclaredCase)
    }

    fn admit_candidate(&self, candidate: Candidate) -> Result<AdmittedFixture, FixtureFailure> {
        let row = self.row(&candidate.declaration)?;
        let input_context = context::observe(&candidate).map_err(FixtureFailure::Context)?;
        if input_context != row.input_context {
            return Err(FixtureFailure::InputContextMismatch);
        }
        // Re-observe complete ordinary output, not cached candidate summaries.
        let result = candidate
            .output
            .observe()
            .map_err(|_| FixtureFailure::ObservationEncoding)?;
        if result != row.result || candidate.observation != row.result {
            return Err(FixtureFailure::ResultMismatch);
        }
        let work = LogicalWork::observe(&candidate.prepared, &candidate.output)
            .map_err(|_| FixtureFailure::ObservationEncoding)?;
        if work_digest(&work)? != row.logical_work
            || !work.matches_expected(&candidate.logical_work)
        {
            return Err(FixtureFailure::LogicalWorkMismatch);
        }
        Ok(AdmittedFixture {
            declaration: candidate.declaration,
            input_context,
            prepared: candidate.prepared,
            expected: row.result,
            expected_work: work,
        })
    }
}

impl AdmittedFixture {
    pub(in crate::benchmark) fn declaration(&self) -> &Declaration {
        &self.declaration
    }
    pub(in crate::benchmark) fn input_context(&self) -> Digest {
        self.input_context
    }
    pub(in crate::benchmark) fn prepared(&self) -> &Prepared {
        &self.prepared
    }
    pub(in crate::benchmark) fn expected(&self) -> Observation {
        self.expected
    }
    pub(in crate::benchmark) fn expected_work(&self) -> &LogicalWork {
        &self.expected_work
    }
}

fn work_digest(work: &LogicalWork) -> Result<Digest, FixtureFailure> {
    let mut frame = Frame::new("fixture-logical-work");
    frame.json(&serde_json::to_value(work).map_err(|_| FixtureFailure::ObservationEncoding)?);
    Ok(frame.finish())
}

#[cfg(test)]
mod tests;
