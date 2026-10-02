// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Recognizing ordinary UI text assigned to a proven DOM receiver.
//!
//! The first profile admits one sink: a static literal assigned with `=` to
//! `textContent` of a receiver that starts at `document.querySelector()` or
//! `document.createElement()` with one static string, on the standard
//! `document`. The receiver is followed through `const` aliases in the same
//! function, and its evidence has to hold on every path that reaches the
//! assignment. Such a literal is displayed text: it becomes a `ui-literal`
//! declaration with the `text-content` usage, used where it is assigned.
//!
//! Nothing here is guessed. A receiver with no known origin, such as a
//! parameter or `this.el`, is outside the profile and only counted. A
//! receiver whose origin is known but whose evidence cannot be established
//! is reported at each literal assigned to it, because silently leaving that
//! text unlocalized would hide it. A proven sink assigned anything but a
//! literal is reported too. An `intent()` or `noIntent()` on the right is
//! explicit authoring, which that recognizer already read.

mod flow;
mod survey;

use std::collections::{BTreeMap, BTreeSet};

use intlify_authoring::{OccurrenceRole, ReasonFamily};
use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{ArrowFunctionExpression, Function, Program, Statement};
use oxc_ast_visit::{walk, Visit};
use oxc_semantic::{ScopeFlags, Scoping};

use self::flow::{Root, Verdict};
use self::survey::{key, Candidate, Key, Receiver, Survey, Tracking, Unestablished, Value};
use crate::binding::IntrinsicSymbols;
use crate::cooked::settle;
use crate::detail;
use crate::explicit::{Declared, Source, Used};
use crate::failure::ProducerFailure;
use crate::limits::{JsAuthoringLimits, JsLimitKind};
use crate::metadata::Annotation;
use crate::parse::Parsed;
use crate::report::Reporter;

/// What automatic recognition found in one unit.
#[derive(Debug, Default)]
pub(crate) struct Found {
    pub(crate) declarations: Vec<Declared>,
    /// Each use, with the index of its declaration in `declarations`.
    pub(crate) uses: Vec<Used>,
    /// Literal `textContent` assignments to receivers with no known origin.
    pub(crate) outside_profile: u64,
}

/// Recognize the proven `textContent` sinks of one accepted unit.
pub(crate) fn recognize<C>(
    parsed: &Parsed<'_>,
    text: &str,
    script: bool,
    intrinsics: &IntrinsicSymbols,
    limits: &JsAuthoringLimits,
    reporter: &mut Reporter,
    cancelled: &C,
) -> Result<Found, ProducerFailure>
where
    C: Fn() -> bool + ?Sized,
{
    let scoping = parsed.semantic.scoping();
    let survey = survey::survey(parsed.program, text, script, scoping, intrinsics, limits);
    if cancelled() {
        return Err(ProducerFailure::Cancelled);
    }
    let (roots, crowded) = roots(&survey, limits);
    let mut driver = Driver {
        roots: &roots,
        scoping,
        steps: limits.proof_steps,
        cancelled,
        verdicts: BTreeMap::new(),
        failure: None,
    };
    driver.visit_program(parsed.program);
    if let Some(failure) = driver.failure {
        return Err(failure);
    }
    let verdicts = driver.verdicts;

    let mut found = Found {
        outside_profile: survey.outside_profile,
        ..Found::default()
    };
    for candidate in &survey.candidates {
        let evidence = evidence(&survey, candidate, &crowded, &verdicts);
        classify(candidate, evidence, reporter, &mut found)?;
    }
    Ok(found)
}

/// Whether a candidate's receiver is proven where it is assigned.
#[derive(Debug, Clone, Copy)]
enum Evidence {
    Proven,
    Invalidated,
    Unestablished(Unestablished),
    Limit(JsLimitKind),
}

fn evidence(
    survey: &Survey,
    candidate: &Candidate,
    crowded: &BTreeSet<Key>,
    verdicts: &BTreeMap<Key, Verdict>,
) -> Evidence {
    match candidate.receiver {
        Receiver::Direct { root } if survey.is_dynamic(root) => {
            Evidence::Unestablished(Unestablished::DynamicScope)
        }
        Receiver::Direct { .. } => Evidence::Proven,
        Receiver::Tracked { origin } => {
            let root = survey.origins[origin].root;
            if survey.is_dynamic(root) {
                return Evidence::Unestablished(Unestablished::DynamicScope);
            }
            if crowded.contains(&root) {
                return Evidence::Limit(JsLimitKind::TrackedOrigins);
            }
            match verdicts.get(&root) {
                Some(verdict) if verdict.exhausted => Evidence::Limit(JsLimitKind::ProofSteps),
                Some(verdict) if verdict.valid.get(&key(candidate.assignment)) == Some(&true) => {
                    Evidence::Proven
                }
                // A sink no walk reached, such as one in a class static
                // block, is not proven either.
                _ => Evidence::Invalidated,
            }
        }
        Receiver::Unestablished(reason) => Evidence::Unestablished(reason),
        Receiver::AliasChain => Evidence::Limit(JsLimitKind::AliasChain),
    }
}

fn classify(
    candidate: &Candidate,
    evidence: Evidence,
    reporter: &mut Reporter,
    found: &mut Found,
) -> Result<(), ProducerFailure> {
    let unsupported = ReasonFamily::AuthoringFormUnsupported;
    let assignment = candidate.assignment;
    let Evidence::Proven = evidence else {
        // Only a literal the tracer could not prove is reported: it is UI
        // text that would otherwise be left unlocalized without a word.
        if !matches!(candidate.value, Value::Literal { .. }) || candidate.compound {
            return Ok(());
        }
        return match evidence {
            Evidence::Invalidated => reporter.at(
                unsupported,
                detail::receiver_evidence_invalidated(),
                assignment,
            ),
            Evidence::Unestablished(reason) => {
                reporter.at(unsupported, reason.detail(), assignment)
            }
            Evidence::Limit(kind) => reporter.at(
                ReasonFamily::AuthoringResourceLimit,
                detail::limit(kind),
                assignment,
            ),
            Evidence::Proven => Ok(()),
        };
    };
    if candidate.compound {
        return reporter.at(unsupported, detail::sink_compound_assignment(), assignment);
    }
    match &candidate.value {
        Value::Literal { span, cooked } => {
            let Some(cooked) = settle(cooked.clone(), *span, reporter)? else {
                return Ok(());
            };
            let occurrence = reporter.occurrence(*span, OccurrenceRole::UiLiteral)?;
            found.declarations.push(Declared {
                occurrence,
                source: Source::Displayed,
                text: cooked.text,
                input_map: cooked.input_map,
                // The assignment supplies no parameter, and a literal
                // requires none.
                parameters: Some(Vec::new()),
                annotation: Annotation::Absent,
            });
            let used = reporter.occurrence(assignment, OccurrenceRole::Reference)?;
            found.uses.push(Used {
                occurrence: used,
                declaration: found.declarations.len() - 1,
                shared: false,
                parameters: Vec::new(),
            });
            Ok(())
        }
        Value::Descriptor(span) => reporter.at(unsupported, detail::sink_descriptor(), *span),
        Value::Dynamic(span) => reporter.at(unsupported, detail::sink_value_dynamic(), *span),
    }
}

/// Prepare the walk of each function that has a followed sink.
///
/// Returns the prepared walks and the functions with more origins than one
/// function may track.
fn roots(survey: &Survey, limits: &JsAuthoringLimits) -> (BTreeMap<Key, Root>, BTreeSet<Key>) {
    let wanted: BTreeSet<Key> = survey
        .candidates
        .iter()
        .filter_map(|candidate| match candidate.receiver {
            Receiver::Tracked { origin } => Some(survey.origins[origin].root),
            _ => None,
        })
        .filter(|root| !survey.is_dynamic(*root))
        .collect();
    let mut roots = BTreeMap::new();
    let mut crowded = BTreeSet::new();
    for scope in wanted {
        let origins: Vec<usize> = survey
            .origins
            .iter()
            .enumerate()
            .filter(|(_, origin)| origin.root == scope)
            .map(|(index, _)| index)
            .collect();
        if origins.len() as u64 > limits.tracked_origins {
            crowded.insert(scope);
            continue;
        }
        let bit_of: BTreeMap<usize, usize> = origins
            .iter()
            .enumerate()
            .map(|(bit, &origin)| (origin, bit))
            .collect();
        let bits = |origins: &BTreeSet<usize>| -> Vec<usize> {
            origins
                .iter()
                .filter_map(|origin| bit_of.get(origin).copied())
                .collect()
        };
        let symbols = survey
            .bindings
            .iter()
            .filter_map(|(&symbol, tracking)| match *tracking {
                Tracking::Tracked { origin, root, .. } if root == scope => {
                    bit_of.get(&origin).map(|&bit| (symbol, bit))
                }
                _ => None,
            })
            .collect();
        let fresh = survey
            .bindings
            .iter()
            .filter_map(|(&symbol, tracking)| match *tracking {
                Tracking::Tracked {
                    origin,
                    root,
                    depth: 0,
                } if root == scope && survey.origins[origin].fresh => {
                    bit_of.get(&origin).map(|&bit| (symbol, bit))
                }
                _ => None,
            })
            .collect();
        let captures = survey
            .captures
            .iter()
            .map(|(&nested, origins)| (nested, bits(origins)))
            .filter(|(_, bits)| !bits.is_empty())
            .collect();
        let hoisted = survey.hoisted.get(&scope).map(&bits).unwrap_or_default();
        let sinks = survey
            .candidates
            .iter()
            .filter_map(|candidate| match candidate.receiver {
                Receiver::Tracked { origin } if survey.origins[origin].root == scope => bit_of
                    .get(&origin)
                    .map(|&bit| (key(candidate.assignment), bit)),
                _ => None,
            })
            .collect();
        roots.insert(
            scope,
            Root {
                width: origins.len(),
                symbols,
                fresh,
                captures,
                hoisted,
                exported: survey.exported.clone(),
                sinks,
            },
        );
    }
    (roots, crowded)
}

/// Walks the unit to reach each prepared function and analyze it.
struct Driver<'d, C: ?Sized> {
    roots: &'d BTreeMap<Key, Root>,
    scoping: &'d Scoping,
    steps: u64,
    cancelled: &'d C,
    verdicts: BTreeMap<Key, Verdict>,
    failure: Option<ProducerFailure>,
}

impl<C> Driver<'_, C>
where
    C: Fn() -> bool + ?Sized,
{
    fn run<'a>(&mut self, scope: Key, statements: &ArenaVec<'a, Statement<'a>>) {
        if self.failure.is_some() {
            return;
        }
        let Some(root) = self.roots.get(&scope) else {
            return;
        };
        match flow::analyze(statements, root, self.scoping, self.steps, self.cancelled) {
            Ok(verdict) => {
                self.verdicts.insert(scope, verdict);
            }
            Err(failure) => self.failure = Some(failure),
        }
    }
}

impl<'a, C> Visit<'a> for Driver<'_, C>
where
    C: Fn() -> bool + ?Sized,
{
    fn visit_program(&mut self, program: &Program<'a>) {
        self.run(key(program.span), &program.body);
        walk::walk_program(self, program);
    }

    fn visit_function(&mut self, function: &Function<'a>, flags: ScopeFlags) {
        if let Some(body) = &function.body {
            self.run(key(function.span), &body.statements);
        }
        walk::walk_function(self, function, flags);
    }

    fn visit_arrow_function_expression(&mut self, arrow: &ArrowFunctionExpression<'a>) {
        self.run(key(arrow.span), &arrow.body.statements);
        walk::walk_arrow_function_expression(self, arrow);
    }
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;

    use super::*;
    use crate::binding;
    use crate::grammar::Grammar;
    use crate::limits::tests::generous;
    use crate::test_support::{at, bindings, parse, reporter};

    /// Run automatic recognition over `text`, returning what it found and
    /// each record as a detail and a range.
    /// Each record, as its detail and range.
    type Records = Vec<(&'static str, (u64, u64))>;

    fn found(text: &str, limits: &JsAuthoringLimits) -> (Found, Records) {
        let allocator = Allocator::default();
        let parsed = parse(&allocator, text, Grammar::JsModule);
        let mut reporter = reporter(text);
        let intrinsics = binding::scan(parsed.program, &bindings(), &mut reporter).unwrap();
        let found = recognize(
            &parsed,
            text,
            false,
            &intrinsics,
            limits,
            &mut reporter,
            &|| false,
        )
        .expect("recognition runs");
        let records = reporter
            .into_diagnostics()
            .iter()
            .map(|record| {
                let intlify_authoring::Location::Region(region) = record.location() else {
                    panic!("a record points into the unit");
                };
                (
                    record.detail().expect("a detail").as_str(),
                    (region.range().start(), region.range().end()),
                )
            })
            .collect();
        (found, records)
    }

    #[test]
    fn a_proven_literal_is_declared_as_displayed_text_and_used_where_assigned() {
        let text = "const pay = document.querySelector('#pay')\npay.textContent = 'Pay\\x20now'\n\
                    element.textContent = 'Outside'\n";
        let (found, records) = found(text, &generous());
        assert_eq!(records, []);
        let [declared] = &found.declarations[..] else {
            panic!("one declaration");
        };
        assert_eq!(declared.source, Source::Displayed);
        assert_eq!(declared.text, "Pay now");
        assert_eq!(declared.occurrence.role(), OccurrenceRole::UiLiteral);
        let range = declared.occurrence.range();
        assert_eq!((range.start(), range.end()), at(text, "'Pay\\x20now'"));
        assert_eq!(declared.parameters.as_deref(), Some(&[][..]));
        let [used] = &found.uses[..] else {
            panic!("one use");
        };
        assert_eq!(used.declaration, 0);
        assert!(!used.shared && used.parameters.is_empty());
        let range = used.occurrence.range();
        assert_eq!(
            (range.start(), range.end()),
            at(text, "pay.textContent = 'Pay\\x20now'")
        );
        assert_eq!(found.outside_profile, 1);
    }

    #[test]
    fn a_function_with_more_origins_than_it_may_track_proves_nothing() {
        let text = "export function render() {\n  const a = document.querySelector('#a')\n  \
                    const b = document.querySelector('#b')\n  a.textContent = 'A'\n}\n";
        let mut limits = generous();
        limits.tracked_origins = 1;
        let (found, records) = found(text, &limits);
        assert!(found.declarations.is_empty());
        assert_eq!(
            records,
            [("tracked-origins", at(text, "a.textContent = 'A'"))]
        );
    }

    #[test]
    fn each_walk_is_prepared_with_only_its_own_origins_as_bits() {
        let text = "const top = document.createElement('p')\nconst again = top\n\
                    export function render() {\n  const pay = document.querySelector('#pay')\n  \
                    const alias = pay\n  pay.textContent = 'Pay'\n  const later = () => [pay, top]\n}\n\
                    top.textContent = 'Top'\n";
        let allocator = Allocator::default();
        let parsed = parse(&allocator, text, Grammar::JsModule);
        let scoping = parsed.semantic.scoping();
        let intrinsics = binding::scan(parsed.program, &bindings(), &mut reporter(text)).unwrap();
        let survey = survey::survey(
            parsed.program,
            text,
            false,
            scoping,
            &intrinsics,
            &generous(),
        );
        let (roots, crowded) = roots(&survey, &generous());
        assert!(crowded.is_empty());
        assert_eq!(roots.len(), 2);
        let program = (0, text.len() as u32);
        let render = roots
            .keys()
            .copied()
            .find(|scope| *scope != program)
            .unwrap();
        let walk = &roots[&render];
        // One origin, reached through two bindings.
        assert_eq!(walk.width, 1);
        assert_eq!(walk.symbols.len(), 2);
        assert!(walk.symbols.values().all(|&bit| bit == 0));
        // The arrow captures both origins, but only this function's is a
        // bit here.
        assert!(walk.captures.values().all(|bits| bits == &[0]));
        let sink = at(text, "pay.textContent = 'Pay'");
        assert_eq!(
            walk.sinks,
            BTreeMap::from([((sink.0 as u32, sink.1 as u32), 0)])
        );
        // A query finds an element again; only a made one starts over.
        assert!(walk.fresh.is_empty());
        let module = &roots[&program];
        assert_eq!(module.width, 1);
        assert_eq!(module.symbols.len(), 2);
        // Only the binding that runs the call starts over, not its alias.
        assert_eq!(module.fresh.len(), 1);
        assert!(module.fresh.values().all(|&bit| bit == 0));
    }

    #[test]
    fn evidence_follows_the_receiver_and_the_walk() {
        let survey = Survey {
            origins: vec![survey::Origin {
                root: (0, 10),
                fresh: false,
            }],
            dynamic: BTreeSet::from([(20, 30)]),
            ..Survey::default()
        };
        let candidate = |receiver| Candidate {
            assignment: oxc_span::Span::new(1, 5),
            receiver,
            value: Value::Dynamic(oxc_span::Span::new(4, 5)),
            compound: false,
        };
        let verdict = |valid, exhausted| Verdict {
            valid: BTreeMap::from([((1, 5), valid)]),
            exhausted,
        };
        let proven = |receiver, verdicts: &BTreeMap<Key, Verdict>| {
            evidence(&survey, &candidate(receiver), &BTreeSet::new(), verdicts)
        };
        let tracked = Receiver::Tracked { origin: 0 };
        let walked = BTreeMap::from([((0, 10), verdict(true, false))]);
        assert!(matches!(proven(tracked, &walked), Evidence::Proven));
        let invalid = BTreeMap::from([((0, 10), verdict(false, false))]);
        assert!(matches!(proven(tracked, &invalid), Evidence::Invalidated));
        let exhausted = BTreeMap::from([((0, 10), verdict(true, true))]);
        assert!(matches!(
            proven(tracked, &exhausted),
            Evidence::Limit(JsLimitKind::ProofSteps)
        ));
        assert!(matches!(
            proven(tracked, &BTreeMap::new()),
            Evidence::Invalidated
        ));
        assert!(matches!(
            evidence(
                &survey,
                &candidate(tracked),
                &BTreeSet::from([(0, 10)]),
                &walked
            ),
            Evidence::Limit(JsLimitKind::TrackedOrigins)
        ));
        assert!(matches!(
            proven(Receiver::Direct { root: (0, 10) }, &BTreeMap::new()),
            Evidence::Proven
        ));
        assert!(matches!(
            proven(Receiver::Direct { root: (20, 30) }, &BTreeMap::new()),
            Evidence::Unestablished(Unestablished::DynamicScope)
        ));
        assert!(matches!(
            proven(Receiver::AliasChain, &BTreeMap::new()),
            Evidence::Limit(JsLimitKind::AliasChain)
        ));
    }
}
