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
//!
//! A unit that was read hands the declarations it found to
//! `intlify_authoring`, which decides what each message means and whether the
//! use site written with it supplies what it requires. A use of a shared
//! declaration is compared here, with the same comparison, because its
//! declaration may be written anywhere in the unit. A use whose declaration
//! could not be established is not a fact either; the declaration's own
//! diagnostics say why, so the use adds none.

use intlify_authoring::{
    compare_parameters, resolve_declarations_with_cancellation, AuthoringContext, AuthoringFailure,
    DeclarationFacts, DeclarationInput, DeclarationMetadata, Detail, Diagnostic, Exclusion,
    LimitKind, Location, MessageInput, Outcome, ReasonFamily, ReferenceFacts, Region,
    SourceSnapshot, UnitOutcome,
};

use crate::detail;
use crate::dom;
use crate::explicit::{self, Recognized, Source};
use crate::failure::ProducerFailure;
use crate::limits::{JsAuthoringLimits, JsLimitKind};
use crate::parse::{self, Reading};
use crate::profile::{JsAuthoringProfile, TEXT_CONTENT_USAGE};
use crate::report::Reporter;
use crate::unit::{check_usage_profile, AdmittedUnit};
use crate::workspace::JsAnalysisWorkspace;

/// What analyzing one unit established.
///
/// Nothing in it borrows the workspace. It stays valid after the workspace
/// moves on to another unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitAnalysis {
    source: SourceSnapshot,
    outcome: UnitOutcome,
    facts: UnitFacts,
    diagnostics: Box<[Diagnostic]>,
    work: UnitWork,
    outside_profile: u64,
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

    /// Return the unit's facts, when every occurrence in it was resolved.
    ///
    /// A blocked or failed unit returns `None` rather than the facts it did
    /// establish, so those cannot be consumed as though the unit had been
    /// covered.
    #[must_use]
    pub fn checked(&self) -> Option<&UnitFacts> {
        matches!(self.outcome, UnitOutcome::Checked).then_some(&self.facts)
    }

    /// Return the independently established facts, whatever the outcome.
    ///
    /// These support inspection only. They are not complete authoring input.
    #[must_use]
    pub const fn inspection_facts(&self) -> &UnitFacts {
        &self.facts
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

    /// Return how many literals were assigned to `textContent` of a receiver
    /// with no known DOM origin.
    ///
    /// Such an assignment is outside the profile, so it is neither
    /// recognized nor reported; this count is how coverage can be inspected.
    /// It is counted only when the profile admits a DOM global.
    #[must_use]
    pub const fn outside_profile(&self) -> u64 {
        self.outside_profile
    }
}

/// The facts one unit established, each list in canonical occurrence order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnitFacts {
    declarations: Box<[DeclarationFacts]>,
    references: Box<[ReferenceFacts]>,
    exclusions: Box<[Exclusion]>,
}

impl UnitFacts {
    /// Borrow the declarations.
    #[must_use]
    pub fn declarations(&self) -> &[DeclarationFacts] {
        &self.declarations
    }

    /// Borrow the references.
    #[must_use]
    pub fn references(&self) -> &[ReferenceFacts] {
        &self.references
    }

    /// Borrow the exclusions.
    #[must_use]
    pub fn exclusions(&self) -> &[Exclusion] {
        &self.exclusions
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
/// The unit is parsed once, under the grammar its snapshot names, and read
/// for the explicit forms the profile registers. The probe is asked around
/// parsing, after the semantic checks, while the tree is walked, and between
/// declarations. How often it is asked is not part of the contract, so it has
/// to be cheap and must not count on a number of calls.
pub fn analyze_unit<C>(
    context: &dyn AuthoringContext,
    profile: &JsAuthoringProfile,
    unit: &AdmittedUnit<'_>,
    limits: &JsAuthoringLimits,
    workspace: &mut JsAnalysisWorkspace,
    cancelled: &C,
) -> Result<UnitAnalysis, ProducerFailure>
where
    C: Fn() -> bool + ?Sized,
{
    check_usage_profile(context, profile)?;
    let source = unit.snapshot().clone();
    let mut reporter = Reporter::new(source.clone(), limits.authoring.diagnostics);
    let mut work = UnitWork {
        source_bytes: source.byte_length(),
        ..UnitWork::default()
    };
    let Some(text) = unit.text() else {
        // No range can address bytes that are not text, so the record
        // points at the unit as a whole.
        let location = Location::Unit(source.clone());
        return failed(reporter, detail::unit_not_text(), location, work);
    };

    work.parse_attempts = 1;
    let (arena, shared) = workspace.fresh();
    let parsed = match parse::read(
        arena,
        text,
        unit.grammar(),
        limits,
        source.unit(),
        cancelled,
    )? {
        Reading::Accepted(parsed) => parsed,
        Reading::Rejected(range) => {
            // The range was checked against the text, whose length is the
            // snapshot's, so it always lies inside the unit.
            let location = range
                .and_then(|range| Region::new(source.clone(), range).ok())
                .map_or_else(|| Location::Unit(source.clone()), Location::Region);
            return failed(reporter, detail::host_syntax_invalid(), location, work);
        }
    };
    work.ast_nodes = u64::from(parsed.stats.nodes);
    work.scopes = u64::from(parsed.stats.scopes);
    work.symbols = u64::from(parsed.stats.symbols);
    work.references = u64::from(parsed.stats.references);

    // With no intrinsic registered, no syntax is an authoring form, so there
    // is nothing to walk for.
    let bindings = profile.binding_set();
    let mut recognized = if bindings.is_empty() {
        Recognized::default()
    } else {
        explicit::recognize(&parsed, text, bindings, limits, &mut reporter, cancelled)?
    };
    let mut outside_profile = 0;
    if profile.admits_document() {
        let found = dom::recognize(
            &parsed,
            text,
            unit.grammar().is_script(),
            &recognized.intrinsics,
            limits,
            &mut reporter,
            cancelled,
        )?;
        outside_profile = found.outside_profile;
        merge(&mut recognized, found, limits, &reporter)?;
    }
    // The tree is not needed past this point, and nothing below borrows it.
    drop(parsed);

    let (facts, blocked_by_shared) = hand_over(
        context,
        recognized,
        limits,
        shared,
        &mut reporter,
        cancelled,
    )?;
    let outcome = if blocked_by_shared || reporter.blocks() {
        UnitOutcome::Blocked
    } else {
        UnitOutcome::Checked
    };
    Ok(UnitAnalysis {
        source,
        outcome,
        facts,
        diagnostics: reporter.into_diagnostics(),
        work,
        outside_profile,
    })
}

/// Add what automatic recognition found to the explicit forms.
///
/// The two recognizers never read the same source: a sink whose value is an
/// explicit form is left to the explicit recognizer. The overlap check runs
/// again over both, so a defect that let them disagree stops the analysis
/// instead of extracting one literal twice.
fn merge(
    recognized: &mut Recognized,
    found: dom::Found,
    limits: &JsAuthoringLimits,
    reporter: &Reporter,
) -> Result<(), ProducerFailure> {
    let offset = recognized.declarations.len();
    recognized.declarations.extend(found.declarations);
    if recognized.declarations.len() as u64 > limits.authoring.declarations {
        return Err(ProducerFailure::Authoring(AuthoringFailure::Limit(
            LimitKind::Declarations,
        )));
    }
    recognized
        .uses
        .extend(found.uses.into_iter().map(|mut used| {
            used.declaration += offset;
            used
        }));
    if recognized.uses.len() as u64 > limits.references {
        return Err(ProducerFailure::Limit(JsLimitKind::References));
    }
    if explicit::overlapping(&recognized.declarations) {
        return Err(ProducerFailure::DeclarationOverlap {
            unit: reporter.source().unit().clone(),
        });
    }
    Ok(())
}

/// Report a unit the author has to fix, establishing nothing.
fn failed(
    mut reporter: Reporter,
    detail: Detail,
    location: Location,
    work: UnitWork,
) -> Result<UnitAnalysis, ProducerFailure> {
    reporter.at_location(ReasonFamily::AuthoringInputInvalid, detail, location)?;
    Ok(UnitAnalysis {
        source: reporter.source().clone(),
        outcome: UnitOutcome::Failed,
        facts: UnitFacts::default(),
        diagnostics: reporter.into_diagnostics(),
        work,
        outside_profile: 0,
    })
}

/// Hand the declarations to the shared crate and settle every use.
///
/// Returns the established facts and whether the shared crate blocked any
/// declaration.
fn hand_over<C>(
    context: &dyn AuthoringContext,
    recognized: Recognized,
    limits: &JsAuthoringLimits,
    shared: &mut intlify_authoring::AnalysisWorkspace,
    reporter: &mut Reporter,
    cancelled: &C,
) -> Result<(UnitFacts, bool), ProducerFailure>
where
    C: Fn() -> bool + ?Sized,
{
    if recognized.declarations.is_empty()
        && recognized.uses.is_empty()
        && recognized.exclusions.is_empty()
    {
        return Ok((UnitFacts::default(), false));
    }
    let inputs: Vec<DeclarationInput<'_>> = recognized
        .declarations
        .iter()
        .map(|declared| {
            // Both `intent()` sources and `mf2` tags hold MF2, not displayed
            // text, so they reach the same parser as the same kind of input.
            // A literal at a proven sink is displayed text, and its braces
            // stay characters; only it takes a usage from where it is.
            let (message, usage) = match declared.source {
                Source::Authored => (MessageInput::Mf2(&declared.text), None),
                Source::Displayed => (
                    MessageInput::Literal(&declared.text),
                    Some(TEXT_CONTENT_USAGE),
                ),
            };
            DeclarationInput {
                occurrence: declared.occurrence.clone(),
                message,
                input_map: Some(&declared.input_map),
                metadata: DeclarationMetadata::default(),
                usage,
                parameters: declared.parameters.as_deref(),
            }
        })
        .collect();
    let result = resolve_declarations_with_cancellation(
        context,
        &inputs,
        &limits.authoring,
        shared,
        cancelled,
    )?;
    for diagnostic in result.diagnostics() {
        reporter.push(diagnostic.clone())?;
    }

    // The established facts, found by declaration. Facts come back in
    // canonical order, which is what makes the search valid.
    let established = result.inspection_facts();
    let facts_of = |index: usize| -> Option<&DeclarationFacts> {
        let occurrence = &recognized.declarations[index].occurrence;
        established
            .binary_search_by(|facts| facts.occurrence().canonical_cmp(occurrence))
            .ok()
            .map(|found| &established[found])
            .filter(|facts| facts.occurrence() == occurrence)
    };

    let mut references = Vec::new();
    for used in recognized.uses {
        if cancelled() {
            return Err(ProducerFailure::Cancelled);
        }
        let Some(target) = facts_of(used.declaration) else {
            continue;
        };
        if used.shared {
            let mut overflow = None;
            let matched = compare_parameters(
                &target.projection().parameters,
                &used.parameters,
                &used.occurrence,
                &mut |record| {
                    if overflow.is_none() {
                        overflow = reporter.push(record).err();
                    }
                },
            );
            if let Some(failure) = overflow {
                return Err(failure);
            }
            if !matched {
                continue;
            }
        }
        references.push(ReferenceFacts::new(
            used.occurrence,
            vec![target.occurrence().clone()],
            used.parameters,
        ));
    }
    references.sort_by(|left, right| left.occurrence().canonical_cmp(right.occurrence()));
    let mut exclusions = recognized.exclusions;
    exclusions.sort_by(|left, right| left.occurrence().canonical_cmp(right.occurrence()));

    Ok((
        UnitFacts {
            declarations: established.to_vec().into_boxed_slice(),
            references: references.into_boxed_slice(),
            exclusions: exclusions.into_boxed_slice(),
        },
        result.outcome() == Outcome::Blocked,
    ))
}

#[cfg(test)]
mod tests {
    use intlify_authoring::{AuthoringFailure, LimitKind};

    use super::*;
    use crate::grammar::Grammar;
    use crate::limits::tests::generous;
    use crate::test_support::{admit, at, context, profile, PRELUDE, UNIT};

    fn analyzed_with(
        profile: &JsAuthoringProfile,
        bytes: &[u8],
        limits: &JsAuthoringLimits,
    ) -> Result<UnitAnalysis, ProducerFailure> {
        let units = admit(&[(UNIT, Grammar::JsModule, bytes)]);
        analyze_unit(
            &context(),
            profile,
            &units[0],
            limits,
            &mut JsAnalysisWorkspace::new(),
            &|| false,
        )
    }

    fn analyzed(text: &str) -> UnitAnalysis {
        analyzed_with(&profile(), text.as_bytes(), &generous()).expect("the analysis runs")
    }

    fn details(analysis: &UnitAnalysis) -> Vec<&str> {
        analysis
            .diagnostics()
            .iter()
            .map(|record| {
                record
                    .detail()
                    .map_or(record.origin().code(), Detail::as_str)
            })
            .collect()
    }

    fn range_of(occurrence: &intlify_authoring::Occurrence) -> (u64, u64) {
        (occurrence.range().start(), occurrence.range().end())
    }

    #[test]
    fn a_checked_unit_hands_out_its_facts_through_both_accessors() {
        let text = format!("{PRELUDE}intent('Pay now')\nnoIntent(brand, 'Brand')\n");
        let analysis = analyzed(&text);
        assert_eq!(analysis.outcome(), UnitOutcome::Checked);
        assert_eq!(analysis.source().unit().as_str(), UNIT);
        assert!(analysis.diagnostics().is_empty());
        assert_eq!(analysis.checked(), Some(analysis.inspection_facts()));
        let facts = analysis.inspection_facts();
        assert_eq!(facts.declarations().len(), 1);
        assert_eq!(facts.references().len(), 1);
        assert_eq!(facts.exclusions().len(), 1);

        let work = analysis.work();
        assert_eq!(work.source_bytes, text.len() as u64);
        assert_eq!(work.parse_attempts, 1);
        assert!(work.ast_nodes > 0);
        assert_eq!(work.scopes, 1);
        // The three imports are the only declared names.
        assert_eq!(work.symbols, 3);
        assert_eq!(work.references, 3);
    }

    #[test]
    fn a_blocked_unit_keeps_its_facts_for_inspection_only() {
        let analysis = analyzed(&format!("{PRELUDE}intent('Pay now')\nintent(computed())\n"));
        assert_eq!(analysis.outcome(), UnitOutcome::Blocked);
        assert_eq!(details(&analysis), ["message-dynamic"]);
        assert_eq!(analysis.checked(), None);
        let facts = analysis.inspection_facts();
        assert_eq!(facts.declarations().len(), 1);
        assert_eq!(facts.references().len(), 1);
    }

    #[test]
    fn a_unit_that_is_not_text_fails_at_the_unit_as_a_whole_without_parsing() {
        let analysis =
            analyzed_with(&profile(), &[0x66, 0xff], &generous()).expect("the analysis runs");
        assert_eq!(analysis.outcome(), UnitOutcome::Failed);
        assert_eq!(details(&analysis), ["unit-not-text"]);
        assert!(matches!(
            analysis.diagnostics()[0].location(),
            Location::Unit(_)
        ));
        assert_eq!(analysis.inspection_facts(), &UnitFacts::default());
        assert_eq!(
            analysis.work(),
            UnitWork {
                source_bytes: 2,
                ..UnitWork::default()
            }
        );
    }

    #[test]
    fn a_unit_the_host_rejects_fails_where_the_host_says() {
        let text = "const = 1\n";
        let analysis = analyzed(text);
        assert_eq!(analysis.outcome(), UnitOutcome::Failed);
        assert_eq!(details(&analysis), ["host-syntax-invalid"]);
        let Location::Region(region) = analysis.diagnostics()[0].location() else {
            panic!("the rejection points into the unit");
        };
        assert_eq!(
            (region.range().start(), region.range().end()),
            at(text, "=")
        );
        assert_eq!(analysis.work().parse_attempts, 1);
        assert_eq!(analysis.work().ast_nodes, 0);
    }

    #[test]
    fn a_profile_that_registers_nothing_reads_no_form_but_still_reads_the_unit() {
        let text = format!("{PRELUDE}intent('Pay now')\nregister(intent)\n");
        let analysis = analyzed_with(&JsAuthoringProfile::new(), text.as_bytes(), &generous())
            .expect("the analysis runs");
        assert_eq!(analysis.outcome(), UnitOutcome::Checked);
        assert!(analysis.diagnostics().is_empty());
        assert_eq!(analysis.checked(), Some(&UnitFacts::default()));
        assert!(analysis.work().ast_nodes > 0);
    }

    #[test]
    fn facts_come_in_canonical_order_whatever_order_they_were_found_in() {
        // The shared use is settled after the walk and the inline one during
        // it, so they are found in the opposite of source order.
        let text =
            format!("{PRELUDE}const greeting = mf2`Hi`\nintent(greeting)\nintent('Pay now')\n");
        let analysis = analyzed(&text);
        assert_eq!(analysis.outcome(), UnitOutcome::Checked);
        let facts = analysis.inspection_facts();
        let declarations: Vec<_> = facts
            .declarations()
            .iter()
            .map(|facts| range_of(facts.occurrence()))
            .collect();
        assert_eq!(declarations, [at(&text, "mf2`Hi`"), at(&text, "'Pay now'")]);
        let references: Vec<_> = facts
            .references()
            .iter()
            .map(|reference| {
                (
                    range_of(reference.occurrence()),
                    range_of(&reference.declarations()[0]),
                )
            })
            .collect();
        assert_eq!(
            references,
            [
                (at(&text, "intent(greeting)"), at(&text, "mf2`Hi`")),
                (at(&text, "intent('Pay now')"), at(&text, "'Pay now'")),
            ]
        );
    }

    #[test]
    fn a_use_whose_declaration_is_blocked_is_no_fact_and_adds_no_record() {
        let text = format!(
            "{PRELUDE}const broken = mf2`Hello {{$name`\nintent(broken, {{ name }})\n\
             intent('Bye {{$name', {{ name }})\n"
        );
        let analysis = analyzed(&text);
        assert_eq!(analysis.outcome(), UnitOutcome::Blocked);
        // One record per broken message, from the shared crate, and nothing
        // about the uses: the declarations already say why.
        assert_eq!(analysis.diagnostics().len(), 2);
        assert!(analysis
            .diagnostics()
            .iter()
            .all(|record| record.stage() == intlify_authoring::Stage::MessageAnalysis));
        assert_eq!(analysis.inspection_facts(), &UnitFacts::default());
    }

    #[test]
    fn a_shared_uses_records_count_against_the_same_bound_as_every_other() {
        let text = format!("{PRELUDE}const greeting = mf2`Hello {{$name}}!`\nintent(greeting)\n");
        let mut limits = generous();
        limits.authoring.diagnostics = 1;
        let analysis = analyzed_with(&profile(), text.as_bytes(), &limits).unwrap();
        assert_eq!(analysis.outcome(), UnitOutcome::Blocked);
        assert_eq!(details(&analysis), ["parameter-missing"]);
        // The use's comparison reports into this unit's bounded collector;
        // with no room left the analysis stops rather than drop the record.
        limits.authoring.diagnostics = 0;
        assert_eq!(
            analyzed_with(&profile(), text.as_bytes(), &limits),
            Err(ProducerFailure::Authoring(AuthoringFailure::Limit(
                LimitKind::Diagnostics
            )))
        );
    }
}
