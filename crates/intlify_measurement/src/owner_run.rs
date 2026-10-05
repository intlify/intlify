// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The scaffolding every observing owner runs on.
//!
//! An owner run fixes its fixtures and expectations, issues its Plan, captures
//! every case on one clock, seals what it captured, and later re-admits the
//! records it wrote only when they are what it captured. None of that depends
//! on which operations are measured, so it lives here once, behind the
//! non-default `owner-run` feature.
//!
//! What an owner supplies is what only it knows, through [`Owner`]: the
//! spellings it registers, its fixed cases and how each is prepared, the one
//! operation each case invokes and how its output is observed, and the case
//! projection and descriptors that say what was measured. Its native checksum
//! keeps the owner's framing: the algorithm is shared, the bytes it frames
//! are not.

pub mod capture;
pub mod context;
pub mod observation;
pub mod run;
pub mod smoke;

mod adapter;
#[cfg(test)]
mod test_owner;

use crate::acquisition::Clock;
use crate::environment::{ClockObservation, Environment, InventoryFailure};
use crate::identity::{RecordIdentity, VersionedIdentity};
use crate::owner::{Fragment, ObservedDescriptors};
use crate::plan::CaseProjection;

use capture::{Capture, CaptureFailure, Capturing, Observed};
use context::{native_environment, ContextObservation, SamplingPolicy};
use observation::Framing;
use run::{Expected, PreparationFailure};

/// A versioned identity an owner registers as a literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Versioned {
    pub identity: &'static str,
    pub revision: &'static str,
}

impl Versioned {
    /// Register one identity at one revision.
    #[must_use]
    pub const fn new(identity: &'static str, revision: &'static str) -> Self {
        Self { identity, revision }
    }

    /// Present it as a checked versioned identity.
    ///
    /// # Panics
    ///
    /// Panics when the literal is outside the identity grammar. A registered
    /// spelling is an implementation constant, so an invalid one is a defect.
    #[must_use]
    pub fn versioned(self) -> VersionedIdentity {
        VersionedIdentity::literal(self.identity, self.revision)
    }
}

/// Every spelling one owner registers for its run and its records.
///
/// They are listed rather than derived from a prefix, so each one an owner
/// produces can be found by searching for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Labels {
    /// The owner identity every checksum method names.
    pub owner: &'static str,
    /// The framing every native checksum of this owner is computed under.
    pub framing: Framing,
    /// The codec of the owner's own plan inside its result.
    pub plan_codec: &'static str,
    /// The codec of the owner's result document.
    pub result_codec: &'static str,
    /// The instance identity domain of the owner's result document.
    pub result_domain: &'static str,
    /// The instance identity domain of the runner that issued the Plan.
    pub runner_domain: &'static str,
    /// The schema of the build observation the Plan is bound to.
    pub build_schema: &'static str,
    /// What exactly this owner measures.
    pub subject: &'static str,
    /// The measurement profile the run is issued under.
    pub profile: Versioned,
    /// The harness that produced the observations.
    pub harness: Versioned,
    /// The projection from the owner's records into 026's.
    pub projection: Versioned,
    /// Why the managed-runtime environment fields do not apply.
    pub native_rule: Versioned,
    /// Why no memory observer applies to a duration-only run.
    pub memory_rule: Versioned,
    /// The instrumentation the harness installs.
    pub instrumentation: Versioned,
    /// The concurrency the harness runs cases under.
    pub concurrency: Versioned,
}

/// What the owner's own crate says about itself.
///
/// These are read where the owner was compiled, so an owner writes them with
/// `env!` and `cfg!` in its own crate rather than letting this one answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Package {
    pub name: &'static str,
    pub version: &'static str,
    pub assertions: bool,
}

/// One fixed case an owner declares.
pub trait Case: Copy {
    /// The fixture's name, unique within the operation it measures.
    fn name(&self) -> &'static str;
    /// The path the fixture is declared to take.
    fn path(&self) -> Expected;
}

/// One observing owner, as the shared run drives it.
///
/// Every function is called outside measured intervals except the operation
/// [`Owner::capture`] hands to [`Capturing::capture`].
pub trait Owner: Sized + 'static {
    /// Every spelling this owner registers.
    const LABELS: Labels;
    /// The owner's own crate, as it described itself.
    const PACKAGE: Package;

    /// One fixed case.
    type Fixture: Case;
    /// Everything one case needs, prepared before any interval opens.
    type Prepared;
    /// The owner's complete Measurement Case projection.
    type Projection: CaseProjection + 'static;
    /// The owner's per-case observed descriptors.
    type Descriptors: Fragment + ObservedDescriptors + 'static;
    /// The owner's logical work vector.
    type Work: Fragment + 'static;

    /// The complete inventory, in the order it is attempted.
    fn fixtures() -> &'static [Self::Fixture];

    /// Prepare one case completely and establish its expectation.
    fn prepare(fixture: Self::Fixture) -> Result<Self::Prepared, PreparationFailure>;

    /// Borrow what a prepared case is expected to produce.
    fn expected(prepared: &Self::Prepared) -> &Observed<Self::Work>;

    /// Project one prepared case.
    fn project(prepared: &Self::Prepared) -> Self::Projection;

    /// Record how one prepared case was measured, on the acquired clock.
    fn descriptors(prepared: &Self::Prepared, clock: ClockObservation) -> Self::Descriptors;

    /// Capture one prepared case.
    fn capture<C: Clock>(
        capturing: &Capturing<'_, C>,
        prepared: &Self::Prepared,
    ) -> Result<Capture<Self::Work>, CaptureFailure>;

    /// The sampling this owner's profile performs.
    #[must_use]
    fn sampling() -> SamplingPolicy {
        SamplingPolicy::smoke()
    }

    /// Project this owner's observations into 026's field inventory.
    fn environment(
        context: &ContextObservation,
        parent: &RecordIdentity,
    ) -> Result<Environment, InventoryFailure> {
        native_environment(&Self::LABELS, context, parent)
    }
}
