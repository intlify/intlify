// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The adapter through which the common pipeline reads one owner's run.
//!
//! The pipeline never acquires anything and never decides what an owner's
//! document means. It asks the owner four things: what was planned, whether a
//! submitted document is the one this run issued, how each planned case turned
//! out, and what the measured cases project to. Everything else — the record
//! shapes, the admission rules, the reason vocabulary — is 026's.
//!
//! An owner answers about its own run. It cannot answer about another run, and
//! a rejection here is a statement about a submitted document rather than a
//! second chance to capture one.

use std::fmt::Debug;

use intlify_shared_json::quantity::Quantity;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::identity::{OwnerRecordIdentity, RecordIdentity, VersionedIdentity};
use crate::measurement::{EvidenceBody, ExpectedOwner};
use crate::plan::{CaseProjection, RunPlanRecord};
use crate::reason::InvocationFailure;
use crate::record::Reference;

/// One closed fragment an owner contributes to a common record.
pub trait Fragment:
    Serialize + for<'de> Deserialize<'de> + JsonSchema + Clone + PartialEq + Debug
{
}

impl<T> Fragment for T where
    T: Serialize + for<'de> Deserialize<'de> + JsonSchema + Clone + PartialEq + Debug
{
}

/// The execution state an owner's descriptors record.
///
/// A report row states which execution state each measured case ran in, so the
/// pipeline has to read it out of the owner's descriptors without knowing what
/// else they contain.
pub trait ObservedDescriptors {
    /// Borrow the execution state these descriptors observed.
    fn execution(&self) -> &crate::execution::Execution;
}

/// Why a submitted owner document is not this run's result.
///
/// The cases stay apart because they become different common causes. A
/// document that cannot be read at all is not a document that was read and
/// found to describe another run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    /// The bytes are not a readable document of this owner's result schema.
    Unreadable,
    /// The document declares a result schema this run does not produce.
    UnsupportedCodec,
    /// The document's own checksum does not match its content.
    Integrity(OwnerRecordIdentity),
    /// The document was read, but it does not bind to this run.
    Binding(Option<OwnerRecordIdentity>),
}

/// How one planned case turned out in the owner's run.
///
/// A case that produced no measurement is still a case. None of these is a
/// measured result, and the pipeline never promotes one into evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseOutcome {
    /// Samples were captured for this case.
    Measured,
    /// The invocation failed for a named reason; the run is incomplete.
    Failed(InvocationFailure),
    /// The output did not match the case's independently established
    /// expectation, which invalidates the run rather than losing a sample.
    SemanticMismatch,
    /// The owner's own record is inconsistent for this case.
    Invalid,
}

/// The Evidence body one owner's run projects to.
pub type OwnerEvidenceBody<O> = EvidenceBody<
    <O as OwnerRun>::Projection,
    <O as OwnerRun>::Descriptors,
    <O as OwnerRun>::Observation,
>;

/// One owner's executed run, as the common pipeline reads it.
pub trait OwnerRun {
    /// The owner's complete Measurement Case projection.
    type Projection: CaseProjection + 'static;
    /// The owner's per-case observed descriptors.
    type Descriptors: Fragment + ObservedDescriptors + 'static;
    /// The owner's complete semantic observation of a sample's output.
    type Observation: Fragment + 'static;
    /// One owner document this run has admitted as its own result.
    type Admitted;

    /// Borrow the Plan issued before this run began.
    fn plan(&self) -> &RunPlanRecord;

    /// Borrow the complete case projections, in the planned order.
    fn projections(&self) -> &[Self::Projection];

    /// Name the owner result this run's Plan was issued for.
    fn expected(&self) -> ExpectedOwner;

    /// Name the tool that produces this owner's records.
    fn producing_tool(&self) -> VersionedIdentity;

    /// Serialize this run's own owner document.
    fn encode(&self) -> Result<Vec<u8>, OwnerFailure>;

    /// Admit one submitted document as this run's result, or say why not.
    ///
    /// Revalidation never executes another timed run or learns a new expected
    /// value: the retained run is the authority a submission is checked against.
    fn admit(&self, bytes: &[u8]) -> Result<Self::Admitted, Rejection>;

    /// Borrow an admitted document's own instance identity.
    fn result_identity<'a>(&self, admitted: &'a Self::Admitted) -> &'a OwnerRecordIdentity;

    /// Report how every planned case turned out, in the planned order.
    fn outcomes(&self, admitted: &Self::Admitted) -> Vec<CaseOutcome>;

    /// Project the measured cases of an admitted document into Evidence.
    ///
    /// `None` means no case in this run is projection-eligible, which is not
    /// the same as an empty Evidence Set: the pipeline records no Evidence at
    /// all rather than an evidence record that claims nothing.
    fn evidence(
        &self,
        admitted: &Self::Admitted,
        parent: &RecordIdentity,
    ) -> Result<Option<OwnerEvidenceBody<Self>>, OwnerFailure>;

    /// Reference one attempt inside an admitted document.
    fn attempt_reference(
        &self,
        admitted: &Self::Admitted,
        ordinal: Quantity,
    ) -> Result<Reference, OwnerFailure>;
}

/// The owner could not answer, which is operational rather than a verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnerFailure;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rejection_keeps_unreadable_apart_from_read_and_rejected() {
        // These become different common causes, so collapsing them would
        // report a corrupt document as one that describes another run.
        let identity = OwnerRecordIdentity::fresh("intlify-measurement-test-result-v1").unwrap();
        assert_ne!(Rejection::Unreadable, Rejection::UnsupportedCodec);
        assert_ne!(
            Rejection::Integrity(identity.clone()),
            Rejection::Binding(Some(identity.clone()))
        );
        // A binding rejection can name the document it read; an unreadable one
        // has nothing to name.
        assert_ne!(Rejection::Binding(None), Rejection::Binding(Some(identity)));
    }

    #[test]
    fn a_case_outcome_never_carries_a_measured_result_for_a_failure() {
        assert_ne!(
            CaseOutcome::Measured,
            CaseOutcome::Failed(InvocationFailure::ClockFailure)
        );
        // A semantic mismatch is not a lost sample: it invalidates the run.
        assert_ne!(CaseOutcome::SemanticMismatch, CaseOutcome::Invalid);
        assert_ne!(
            CaseOutcome::Failed(InvocationFailure::ClockFailure),
            CaseOutcome::Failed(InvocationFailure::MeasurementOverflow)
        );
    }
}
