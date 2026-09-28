// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Where one unit's positions and records are made.
//!
//! Every range this crate reports comes from the host parser's spans, and
//! every record goes into one bounded collector per unit. The bound is
//! `intlify_authoring`'s own diagnostic limit, applied to this unit's records
//! and the shared crate's together, so a caller's one number bounds what an
//! analysis retains. It is enforced as each record arrives, and reaching it
//! stops the analysis rather than dropping records: a truncated list could
//! make a blocked unit look checked.

use intlify_authoring::{
    AuthoringFailure, ByteRange, Detail, Diagnostic, DiagnosticOrigin, LimitKind, Location,
    Occurrence, OccurrenceRole, ReasonFamily, Region, Severity, SourceSnapshot, Stage,
};
use oxc_span::Span;

use crate::failure::ProducerFailure;

/// Positions and records for one unit.
#[derive(Debug)]
pub(crate) struct Reporter {
    source: SourceSnapshot,
    budget: u64,
    diagnostics: Vec<Diagnostic>,
}

impl Reporter {
    /// Start reporting for one unit under a diagnostic budget.
    pub(crate) const fn new(source: SourceSnapshot, budget: u64) -> Self {
        Self {
            source,
            budget,
            diagnostics: Vec::new(),
        }
    }

    /// Borrow the unit's snapshot.
    pub(crate) const fn source(&self) -> &SourceSnapshot {
        &self.source
    }

    /// Return the range a parser span addresses.
    pub(crate) fn range(&self, span: Span) -> Result<ByteRange, ProducerFailure> {
        // A span is the parser's claim about the source it read. One that is
        // reversed or runs past the unit is the parser failing, not the author.
        ByteRange::new(u64::from(span.start), u64::from(span.end)).map_err(|_| self.invariant())
    }

    /// Return the occurrence a span plays `role` at.
    pub(crate) fn occurrence(
        &self,
        span: Span,
        role: OccurrenceRole,
    ) -> Result<Occurrence, ProducerFailure> {
        Occurrence::new(self.source.clone(), self.range(span)?, role).map_err(|_| self.invariant())
    }

    /// Record one host diagnostic at the source a span addresses.
    pub(crate) fn at(
        &mut self,
        family: ReasonFamily,
        detail: Detail,
        span: Span,
    ) -> Result<(), ProducerFailure> {
        let range = self.range(span)?;
        self.at_range(family, detail, range)
    }

    /// Record one host diagnostic at a range of the unit.
    pub(crate) fn at_range(
        &mut self,
        family: ReasonFamily,
        detail: Detail,
        range: ByteRange,
    ) -> Result<(), ProducerFailure> {
        let region = Region::new(self.source.clone(), range).map_err(|_| self.invariant())?;
        self.at_location(family, detail, Location::Region(region))
    }

    /// Record one host diagnostic at a location.
    pub(crate) fn at_location(
        &mut self,
        family: ReasonFamily,
        detail: Detail,
        location: Location,
    ) -> Result<(), ProducerFailure> {
        self.push(
            Diagnostic::new(
                Stage::HostDiscovery,
                DiagnosticOrigin::Authoring(family),
                Severity::Error,
                location,
            )
            .with_detail(detail),
        )
    }

    /// Retain one record, whoever made it.
    pub(crate) fn push(&mut self, diagnostic: Diagnostic) -> Result<(), ProducerFailure> {
        if self.diagnostics.len() as u64 >= self.budget {
            return Err(ProducerFailure::Authoring(AuthoringFailure::Limit(
                LimitKind::Diagnostics,
            )));
        }
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    /// Return whether any retained record blocks a checked result.
    pub(crate) fn blocks(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_blocking)
    }

    /// Give up the records, in 016's reporting order.
    pub(crate) fn into_diagnostics(mut self) -> Box<[Diagnostic]> {
        self.diagnostics.sort_by(Diagnostic::reporting_cmp);
        self.diagnostics.into_boxed_slice()
    }

    fn invariant(&self) -> ProducerFailure {
        ProducerFailure::ParserInvariant {
            unit: self.source.unit().clone(),
        }
    }
}
