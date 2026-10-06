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

    /// Return the range of each retained record that points inside the unit.
    pub(crate) fn ranges(&self) -> Vec<(u64, u64)> {
        self.diagnostics
            .iter()
            .filter_map(|diagnostic| {
                let range = match diagnostic.location() {
                    Location::Occurrence(occurrence) => occurrence.range(),
                    Location::Region(region) => region.range(),
                    Location::Unit(_) => return None,
                };
                Some((range.start(), range.end()))
            })
            .collect()
    }

    /// Borrow the retained records, in the order they arrived.
    #[cfg(feature = "benchmark")]
    pub(crate) fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
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

#[cfg(test)]
mod tests {
    use intlify_authoring::{DiagnosticOrigin, Location, Severity, Stage};

    use super::*;
    use crate::detail;
    use crate::grammar::Grammar;
    use crate::test_support::{snapshot, token, UNIT};

    const TEXT: &str = "intent('Pay now')\n";

    fn reporter(budget: u64) -> Reporter {
        Reporter::new(snapshot(UNIT, Grammar::JsModule, TEXT.as_bytes()), budget)
    }

    fn span(start: u32, end: u32) -> Span {
        Span::new(start, end)
    }

    /// One record reduced to its stage, code, detail, severity and range.
    type Reduced<'d> = (
        Stage,
        &'d str,
        Option<&'d str>,
        Severity,
        Option<(u64, u64)>,
    );

    /// One record reduced to where it points and why.
    fn reduced(diagnostic: &Diagnostic) -> Reduced<'_> {
        let range = match diagnostic.location() {
            Location::Region(region) => Some((region.range().start(), region.range().end())),
            Location::Occurrence(occurrence) => {
                Some((occurrence.range().start(), occurrence.range().end()))
            }
            Location::Unit(_) => None,
        };
        (
            diagnostic.stage(),
            diagnostic.origin().code(),
            diagnostic.detail().map(Detail::as_str),
            diagnostic.severity(),
            range,
        )
    }

    #[test]
    fn a_span_inside_the_unit_is_its_range_and_occurrence() {
        let reporter = reporter(1);
        assert_eq!(reporter.source().unit(), &token(UNIT));
        let range = reporter.range(span(7, 16)).unwrap();
        assert_eq!((range.start(), range.end()), (7, 16));
        let occurrence = reporter
            .occurrence(span(7, 16), OccurrenceRole::IntentLiteral)
            .unwrap();
        assert_eq!(occurrence.role(), OccurrenceRole::IntentLiteral);
        assert_eq!(occurrence.range(), range);
        assert_eq!(occurrence.source(), reporter.source());
        // The whole unit, and an empty range at its end, are both inside it.
        assert!(reporter.range(span(0, 18)).is_ok());
        assert!(reporter
            .occurrence(span(18, 18), OccurrenceRole::Reference)
            .is_ok());
    }

    #[test]
    fn a_span_the_unit_cannot_hold_is_the_parser_failing() {
        let reporter = reporter(1);
        let invariant = ProducerFailure::ParserInvariant { unit: token(UNIT) };
        assert_eq!(reporter.range(span(9, 7)), Err(invariant.clone()));
        assert_eq!(
            reporter.occurrence(span(9, 7), OccurrenceRole::Reference),
            Err(invariant.clone())
        );
        assert_eq!(
            reporter.occurrence(span(7, 19), OccurrenceRole::Reference),
            Err(invariant.clone()),
            "one byte past the unit"
        );
        let mut reporter = reporter;
        assert_eq!(
            reporter.at(
                ReasonFamily::AuthoringFormUnsupported,
                detail::intent_arguments(),
                span(17, 19)
            ),
            Err(invariant)
        );
        assert!(reporter.into_diagnostics().is_empty());
    }

    #[test]
    fn a_host_record_names_its_stage_family_detail_and_place() {
        let mut reporter = reporter(3);
        reporter
            .at(
                ReasonFamily::AuthoringSourceDynamic,
                detail::message_dynamic(),
                span(7, 16),
            )
            .unwrap();
        reporter
            .at_range(
                ReasonFamily::AuthoringFormUnsupported,
                detail::surrogate_escape(),
                ByteRange::new(8, 11).unwrap(),
            )
            .unwrap();
        reporter
            .at_location(
                ReasonFamily::AuthoringInputInvalid,
                detail::unit_not_text(),
                Location::Unit(reporter.source().clone()),
            )
            .unwrap();
        let diagnostics = reporter.into_diagnostics();
        let reduced: Vec<_> = diagnostics.iter().map(reduced).collect();
        assert_eq!(
            reduced,
            [
                (
                    Stage::HostDiscovery,
                    "authoring-input-invalid",
                    Some("unit-not-text"),
                    Severity::Error,
                    None
                ),
                (
                    Stage::HostDiscovery,
                    "authoring-source-dynamic",
                    Some("message-dynamic"),
                    Severity::Error,
                    Some((7, 16))
                ),
                (
                    Stage::HostDiscovery,
                    "authoring-form-unsupported",
                    Some("surrogate-escape"),
                    Severity::Error,
                    Some((8, 11))
                ),
            ]
        );
        assert!(diagnostics
            .iter()
            .all(|record| matches!(record.origin(), DiagnosticOrigin::Authoring(_))));
    }

    #[test]
    fn records_are_retained_up_to_the_budget_exactly() {
        let mut reporter = reporter(2);
        let record = || {
            Diagnostic::new(
                Stage::HostDiscovery,
                DiagnosticOrigin::Authoring(ReasonFamily::AuthoringFormUnsupported),
                Severity::Error,
                Location::Unit(snapshot(UNIT, Grammar::JsModule, TEXT.as_bytes())),
            )
        };
        assert_eq!(reporter.push(record()), Ok(()));
        assert_eq!(reporter.push(record()), Ok(()));
        // The record over the bound stops the analysis instead of being
        // dropped, and what was retained stays as it was.
        assert_eq!(
            reporter.push(record()),
            Err(ProducerFailure::Authoring(AuthoringFailure::Limit(
                LimitKind::Diagnostics
            )))
        );
        assert_eq!(reporter.into_diagnostics().len(), 2);

        let mut none = self::reporter(0);
        assert!(none.push(record()).is_err());
    }

    #[test]
    fn only_an_error_blocks_a_checked_result() {
        let mut reporter = reporter(2);
        assert!(!reporter.blocks());
        let at = |severity| {
            Diagnostic::new(
                Stage::HostDiscovery,
                DiagnosticOrigin::Authoring(ReasonFamily::AuthoringFormUnsupported),
                severity,
                Location::Unit(snapshot(UNIT, Grammar::JsModule, TEXT.as_bytes())),
            )
        };
        reporter.push(at(Severity::Warning)).unwrap();
        assert!(!reporter.blocks());
        reporter.push(at(Severity::Error)).unwrap();
        assert!(reporter.blocks());
    }

    #[test]
    fn records_come_out_in_reporting_order_whatever_order_they_arrived_in() {
        let mut reporter = reporter(3);
        let unsupported = ReasonFamily::AuthoringFormUnsupported;
        reporter
            .at(unsupported, detail::parameter_spread(), span(10, 12))
            .unwrap();
        reporter
            .at(unsupported, detail::parameter_key(), span(2, 4))
            .unwrap();
        reporter
            .at(unsupported, detail::intent_arguments(), span(2, 4))
            .unwrap();
        let order: Vec<_> = reporter
            .into_diagnostics()
            .iter()
            .map(|record| (reduced(record).4, record.detail().map(Detail::as_str)))
            .collect();
        assert_eq!(
            order,
            [
                (Some((2, 4)), Some("intent-arguments")),
                (Some((2, 4)), Some("parameter-key")),
                (Some((10, 12)), Some("parameter-spread")),
            ]
        );
    }
}
