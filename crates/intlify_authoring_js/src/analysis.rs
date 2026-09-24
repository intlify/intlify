// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Analyzing one admitted unit.
//!
//! Units are analyzed independently. One call reads one unit, uses only its
//! own workspace, and starts no thread, so a caller can run units in any order
//! or on several workers and get the same results. Putting results in a
//! deterministic order is the caller's merge, not this call's.
//!
//! A unit the author has to fix is a failed unit, not a failed invocation. It
//! carries one diagnostic saying why, and it establishes nothing: a unit that
//! could not be read is not evidence that it contains no declarations.

use intlify_authoring::{
    Detail, Diagnostic, DiagnosticOrigin, Location, ReasonFamily, Region, Severity, SourceSnapshot,
    Stage, UnitOutcome,
};

use crate::detail;
use crate::failure::ProducerFailure;
use crate::limits::JsAuthoringLimits;
use crate::parse::{self, Reading};
use crate::unit::AdmittedUnit;
use crate::workspace::JsAnalysisWorkspace;

/// What analyzing one unit established.
///
/// Nothing in it borrows the workspace. It stays valid after the workspace
/// moves on to another unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitAnalysis {
    source: SourceSnapshot,
    outcome: UnitOutcome,
    diagnostics: Box<[Diagnostic]>,
    work: UnitWork,
}

impl UnitAnalysis {
    /// Borrow the analyzed unit's snapshot.
    #[must_use]
    pub const fn source(&self) -> &SourceSnapshot {
        &self.source
    }

    /// Return what happened to the unit.
    #[must_use]
    pub const fn outcome(&self) -> UnitOutcome {
        self.outcome
    }

    /// Borrow the diagnostics, in reporting order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Return the logical work the analysis did.
    #[must_use]
    pub const fn work(&self) -> UnitWork {
        self.work
    }

    fn failed(source: SourceSnapshot, location: Location, detail: Detail, work: UnitWork) -> Self {
        let diagnostic = Diagnostic::new(
            Stage::HostDiscovery,
            DiagnosticOrigin::Authoring(ReasonFamily::AuthoringInputInvalid),
            Severity::Error,
            location,
        )
        .with_detail(detail);
        Self {
            source,
            outcome: UnitOutcome::Failed,
            diagnostics: Box::new([diagnostic]),
            work,
        }
    }
}

/// The logical work one analysis did.
///
/// These count what was read, not how long it took, so the same unit gives
/// the same counts on every run and a reused workspace can be compared with a
/// fresh one exactly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UnitWork {
    /// Bytes in the unit.
    pub source_bytes: u64,
    /// Times the unit was parsed: once for text, never for a unit that is not.
    pub parse_attempts: u64,
    /// Syntax tree nodes of an accepted unit.
    pub ast_nodes: u64,
    /// Scopes of an accepted unit.
    pub scopes: u64,
    /// Symbols an accepted unit declares.
    pub symbols: u64,
    /// Identifier references in an accepted unit.
    pub references: u64,
}

/// Analyze one admitted unit.
///
/// The unit is parsed once, under the grammar its snapshot names. The probe
/// is asked before and after parsing and after the semantic checks. How often
/// it is asked is not part of the contract, so it has to be cheap and must not
/// count on a number of calls.
pub fn analyze_unit<C>(
    unit: &AdmittedUnit<'_>,
    limits: &JsAuthoringLimits,
    workspace: &mut JsAnalysisWorkspace,
    cancelled: &C,
) -> Result<UnitAnalysis, ProducerFailure>
where
    C: Fn() -> bool + ?Sized,
{
    let source = unit.snapshot().clone();
    let mut work = UnitWork {
        source_bytes: source.byte_length(),
        ..UnitWork::default()
    };
    let Some(text) = unit.text() else {
        // No range can address bytes that are not text, so the record
        // points at the unit as a whole.
        let location = Location::Unit(source.clone());
        return Ok(UnitAnalysis::failed(
            source,
            location,
            detail::unit_not_text(),
            work,
        ));
    };

    work.parse_attempts = 1;
    let reading = parse::read(
        workspace.fresh_arena(),
        text,
        unit.grammar(),
        limits,
        source.unit(),
        cancelled,
    )?;
    match reading {
        Reading::Accepted(stats) => {
            work.ast_nodes = u64::from(stats.nodes);
            work.scopes = u64::from(stats.scopes);
            work.symbols = u64::from(stats.symbols);
            work.references = u64::from(stats.references);
            Ok(UnitAnalysis {
                source,
                outcome: UnitOutcome::Checked,
                diagnostics: Box::new([]),
                work,
            })
        }
        Reading::Rejected(range) => {
            // The range was checked against the text, whose length is the
            // snapshot's, so it always lies inside the unit.
            let location = range
                .and_then(|range| Region::new(source.clone(), range).ok())
                .map_or_else(|| Location::Unit(source.clone()), Location::Region);
            Ok(UnitAnalysis::failed(
                source,
                location,
                detail::host_syntax_invalid(),
                work,
            ))
        }
    }
}
