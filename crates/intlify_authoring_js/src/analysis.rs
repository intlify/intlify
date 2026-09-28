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
    compare_parameters, resolve_declarations_with_cancellation, AuthoringContext, DeclarationFacts,
    DeclarationInput, DeclarationMetadata, Detail, Diagnostic, Exclusion, Location, MessageInput,
    Outcome, ReasonFamily, ReferenceFacts, Region, SourceSnapshot, UnitOutcome,
};

use crate::detail;
use crate::explicit::{self, Recognized};
use crate::failure::ProducerFailure;
use crate::limits::JsAuthoringLimits;
use crate::parse::{self, Reading};
use crate::profile::JsAuthoringProfile;
use crate::report::Reporter;
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
    facts: UnitFacts,
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
    let recognized = if bindings.is_empty() {
        Recognized::default()
    } else {
        explicit::recognize(&parsed, text, bindings, limits, &mut reporter, cancelled)?
    };
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
    })
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
        .map(|declared| DeclarationInput {
            occurrence: declared.occurrence.clone(),
            // Both `intent()` sources and `mf2` tags hold MF2, not displayed
            // text, so they reach the same parser as the same kind of input.
            message: MessageInput::Mf2(&declared.text),
            input_map: Some(&declared.input_map),
            metadata: DeclarationMetadata::default(),
            usage: None,
            parameters: declared.parameters.as_deref(),
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
