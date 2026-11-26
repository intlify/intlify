// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's outcome finalization stage over one file entry's complete
//! invocation (D15:2772, 2776). It takes what the Entry projection stage
//! left, so before the interval the invocation's evaluation phases and both
//! projection stages run on an evaluation of their own; the interval runs
//! outcome finalization: each domain's projection in canonical order,
//! reporting finalization's verification, the profile Phase 14 projects for
//! a checked outcome, and the mandatory reserve (plan 025 decisions #253 and
//! #262). The outcome borrows the evaluation, so after the end read it is
//! copied out. Neither the result nor its observation is a Profile or a
//! shared artifact.

use serde_json::Value;

use crate::artifacts::envelope::ExactTuple;
use crate::benchmark::observation::{Frame, Observation};
use crate::benchmark::operation::OutputFailure;
use crate::construction::identity::ConstructionIdentity;
use crate::resolver::evaluation::{Evaluation, Unit};
use crate::resolver::finalization::reserved_bytes;
use crate::resolver::outcome::{finalize, EntryStage, Outcome};
use crate::resolver::profile_resolution::{Invocation, Invoked, ResolverInvariant};
use crate::resolver::profile_view::ProfileView;
use crate::resolver::resolution_evidence::Projection;
use crate::resolver::token::Specification;

use super::{count, specification, unit, Id, PreparedEvidence, RetainedRecord};

/// An outcome finalization case's inputs: the invocation's, and the Resolver
/// Construction Identity of the committed resolver, which finalization puts
/// in the outcome (D15:1151, 1156).
pub(in crate::benchmark) struct PreparedOutcome {
    pub(in crate::benchmark) evidence: PreparedEvidence,
    pub(in crate::benchmark) identity: ConstructionIdentity,
}

impl PreparedOutcome {
    /// One untimed invocation of outcome finalization, after both projection
    /// stages, retained as a timed one is, or `None` on a resolver defect
    /// before the stage.
    pub(in crate::benchmark) fn resolve(&self) -> Option<Resolved> {
        let prepared = &self.evidence;
        let invocation = prepared.invocation();
        let mut evaluation = Evaluation::default();
        let mut set = None;
        let invoked = prepared.invoke(&mut evaluation, &invocation, &mut set)?;
        let stage = super::shared((&evaluation, &invoked, &invocation)).ok()?;
        let stage = super::entry::entry((stage, &evaluation, &invoked, &invocation)).ok()?;
        let outcome = finalization((stage, &evaluation, &invoked, self.identity, &invocation));
        Some(Resolved::retain(&invocation, outcome))
    }
}

/// Outcome finalization: what every timed invocation of
/// `outcome_finalization` runs.
pub(in crate::benchmark) fn finalization<'e, 's>(
    (stage, evaluation, invoked, identity, invocation): (
        EntryStage<'e, '_>,
        &'e Evaluation,
        &'e Invoked<'s, Id>,
        ConstructionIdentity,
        &'e Invocation<'_, Id>,
    ),
) -> Result<Outcome<'e, 's>, ResolverInvariant> {
    finalize(stage, evaluation, invoked, identity, invocation)
}

/// One domain's projection as the output retains it: its records in
/// canonical order with their Evidence record owners, and its terminal
/// Finding, state, and Shared-unavailability cause, when a bound exhausted
/// it (D15:2409).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::benchmark) struct RetainedProjection {
    pub(in crate::benchmark) records: Vec<(Unit, RetainedRecord)>,
    pub(in crate::benchmark) terminal: Option<(Value, Value, Option<Value>)>,
}

impl RetainedProjection {
    pub(in crate::benchmark) fn of(projection: &Projection<'_>) -> Self {
        Self {
            records: projection
                .records
                .iter()
                .map(|(owner, retained)| ((*owner).clone(), RetainedRecord::of(retained)))
                .collect(),
            terminal: projection.terminal.as_ref().map(|terminal| {
                (
                    terminal.finding.clone(),
                    terminal.state.clone(),
                    terminal.cause.clone(),
                )
            }),
        }
    }
}

/// The outcome as the output retains it: its Resolver Construction Identity,
/// both projections, a checked outcome's profile view and the tuples of its
/// Resolved Profile Artifact Bindings, and its mandatory reserve in
/// `maxCanonicalBytes`' unit (D15:1151-1167, 1848).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::benchmark) struct RetainedOutcome {
    pub(in crate::benchmark) identity: ConstructionIdentity,
    pub(in crate::benchmark) entry: RetainedProjection,
    pub(in crate::benchmark) shared: Option<RetainedProjection>,
    pub(in crate::benchmark) profile: Option<(ProfileView, Vec<ExactTuple>)>,
    pub(in crate::benchmark) reserve: u64,
}

impl RetainedOutcome {
    fn of(outcome: &Outcome<'_, '_>) -> Self {
        Self {
            identity: outcome.identity,
            entry: RetainedProjection::of(&outcome.evidence.entry),
            shared: outcome.evidence.shared.as_ref().map(RetainedProjection::of),
            profile: outcome.checked.as_ref().map(|checked| {
                (
                    checked.profile.clone(),
                    checked
                        .bindings
                        .iter()
                        .map(|(tuple, _)| tuple.clone())
                        .collect(),
                )
            }),
            reserve: reserved_bytes(outcome),
        }
    }
}

/// Outcome finalization's complete ordinary output, retained past the end
/// read, with the Profile Specification its subjects' records belong to.
pub(in crate::benchmark) struct Resolved {
    pub(in crate::benchmark) result: Result<RetainedOutcome, ResolverInvariant>,
    pub(in crate::benchmark) specification: Specification<'static>,
}

impl Resolved {
    /// The outcome borrows the evaluation, so after the end read it is copied
    /// out, which is observation, not work of the interval.
    pub(in crate::benchmark) fn retain(
        invocation: &Invocation<'_, Id>,
        outcome: Result<Outcome<'_, '_>, ResolverInvariant>,
    ) -> Self {
        Self {
            result: outcome.map(|outcome| RetainedOutcome::of(&outcome)),
            specification: specification(invocation),
        }
    }
}

/// A resolver defect is no blocked outcome: the case fails rather than
/// completing as an expected failure. Logical work uses this screen.
pub(in crate::benchmark) fn require_observable(
    resolved: &Resolved,
) -> Result<&RetainedOutcome, OutputFailure> {
    resolved
        .result
        .as_ref()
        .map_err(|_| OutputFailure::EvidenceMaterializationInvariant)
}

/// The outcome, its Shared semantic output and its entry-specific Evidence
/// framed in separate checksum domains: the Resolver Construction Identity,
/// the Shared projection or its absence, and a checked outcome's profile view
/// and bindings in the shared domain, the Entry projection in the entry
/// domain. Whether the outcome is checked and whether its bundle is complete
/// follow from the profile and the terminal states (D15:1153-1154,
/// 1176-1185, 2775, 2794; plan 025 interpretation #261).
pub(in crate::benchmark) fn observe(resolved: &Resolved) -> Result<Observation, OutputFailure> {
    let outcome = require_observable(resolved)?;
    let specification = resolved.specification;
    let mut shared = Frame::new("outcome-finalization");
    shared.text(&outcome.identity.text());
    shared.flag(outcome.shared.is_some());
    if let Some(projection) = &outcome.shared {
        project(&mut shared, specification, projection)?;
    }
    shared.flag(outcome.profile.is_some());
    if let Some((view, tuples)) = &outcome.profile {
        shared.json(&serde_json::to_value(view).map_err(|_| OutputFailure::ObservationEncoding)?);
        shared.uint(count(tuples.len()));
        for tuple in tuples {
            shared.text(tuple.kind.spelling());
            shared.text(tuple.identity.as_str());
            shared.text(tuple.revision.as_str());
            shared.text(tuple.specification_revision.as_str());
            shared.text(tuple.semantic_digest.as_str());
        }
    }
    let mut entry = Frame::new("outcome-finalization-entry");
    project(&mut entry, specification, &outcome.entry)?;
    Ok(Observation {
        shared: shared.finish(),
        entry: Some(entry.finish()),
    })
}

/// One domain's projection: each record's owner, record kind, view, and
/// accounting bytes in order, and its terminal Finding, state, and cause.
pub(in crate::benchmark) fn project(
    frame: &mut Frame,
    specification: Specification<'_>,
    projection: &RetainedProjection,
) -> Result<(), OutputFailure> {
    frame.uint(count(projection.records.len()));
    for (owner, record) in &projection.records {
        unit(frame, specification, owner)?;
        frame.text(record.kind.id());
        frame.json(&record.view);
        frame.uint(record.bytes);
    }
    frame.flag(projection.terminal.is_some());
    if let Some((finding, state, cause)) = &projection.terminal {
        frame.json(finding);
        frame.json(state);
        frame.flag(cause.is_some());
        if let Some(cause) = cause {
            frame.json(cause);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::cases::prepare::prepare;
    use crate::benchmark::cases::{declarations, EvidenceRecipe as R, Recipe};
    use crate::benchmark::operation::{Operation, Prepared};
    use crate::resolver::outcome::outcome;
    use crate::resolver::token::Sha256;

    fn prepared(recipe: R) -> PreparedOutcome {
        let declaration = declarations()
            .into_iter()
            .find(|case| {
                case.operation == Operation::OutcomeFinalization
                    && case.fixture == Recipe::Evidence(recipe)
            })
            .unwrap();
        let Prepared::OutcomeFinalization(prepared) = prepare(&declaration).unwrap().prepared
        else {
            unreachable!()
        };
        *prepared
    }

    fn retained(recipe: R) -> RetainedOutcome {
        prepared(recipe).resolve().unwrap().result.unwrap()
    }

    /// Only the example's outcome is checked. The blocked recipes' Shared
    /// projections are present and whole, the file without a value has none,
    /// and the truncated recipe's Shared domain is exhausted, so its bundle is
    /// incomplete (D15:1153-1154, 1164-1166, 1186).
    #[test]
    fn each_recipe_finalizes_its_expected_outcome() {
        for recipe in R::ALL {
            let outcome = retained(recipe);
            assert_eq!(outcome.profile.is_some(), recipe.checked(), "{recipe:?}");
            assert!(outcome.entry.terminal.is_none(), "{recipe:?}");
            match recipe {
                R::Malformed | R::Inadmissible => assert!(outcome.shared.is_none()),
                R::Truncated => {
                    assert!(outcome.shared.unwrap().terminal.is_some());
                }
                R::Storefront | R::Blocked | R::Dense => {
                    assert!(outcome.shared.unwrap().terminal.is_none(), "{recipe:?}");
                }
            }
        }
    }

    /// Finalization after both projection stages is the outcome the resolver
    /// builds for the invocation: the same projections in the same order, the
    /// same profile, and the same reserve.
    #[test]
    fn the_stage_finalizes_the_outcome_the_resolver_builds() {
        for recipe in R::ALL {
            let prepared = prepared(recipe);
            let inputs = &prepared.evidence;
            let invocation = inputs.invocation();
            let mut evaluation = Evaluation::default();
            let mut set = None;
            let invoked = inputs
                .invoke(&mut evaluation, &invocation, &mut set)
                .unwrap();
            let whole = outcome(
                &evaluation,
                &invoked,
                prepared.identity,
                &invocation,
                &Sha256,
            )
            .unwrap();
            assert_eq!(
                prepared.resolve().unwrap().result.unwrap(),
                RetainedOutcome::of(&whole),
                "{recipe:?}"
            );
        }
    }

    /// The observation tells every part of the outcome apart: the Shared
    /// projection's presence and terminal state and the profile and its
    /// bindings in the shared domain, and the Entry projection's records and
    /// terminal state in the entry domain.
    #[test]
    fn every_part_of_the_outcome_is_observed() {
        let resolved = prepared(R::Storefront).resolve().unwrap();
        let base = resolved.result.as_ref().unwrap().clone();
        let observed = |outcome: &RetainedOutcome| {
            observe(&Resolved {
                result: Ok(outcome.clone()),
                specification: resolved.specification,
            })
            .unwrap()
        };
        let whole = observed(&base);
        let truncated = retained(R::Truncated);
        let terminal = truncated.shared.as_ref().unwrap().terminal.clone();
        let mut changes: Vec<(&str, RetainedOutcome, bool)> = Vec::new();
        let mut change = |name, edit: &dyn Fn(&mut RetainedOutcome), shared| {
            let mut outcome = base.clone();
            edit(&mut outcome);
            changes.push((name, outcome, shared));
        };
        change("no shared", &|outcome| outcome.shared = None, true);
        change(
            "shared terminal",
            &|outcome| outcome.shared.as_mut().unwrap().terminal = terminal.clone(),
            true,
        );
        change("no profile", &|outcome| outcome.profile = None, true);
        change(
            "no binding",
            &|outcome| {
                outcome.profile.as_mut().unwrap().1.pop();
            },
            true,
        );
        change(
            "one entry record fewer",
            &|outcome| {
                outcome.entry.records.pop();
            },
            false,
        );
        change(
            "entry terminal",
            &|outcome| outcome.entry.terminal = terminal.clone(),
            false,
        );
        for (name, outcome, shared) in &changes {
            let changed = observed(outcome);
            assert_eq!(changed.shared != whole.shared, *shared, "{name}");
            assert_eq!(changed.entry != whole.entry, !*shared, "{name}");
        }
    }

    /// Each recipe's outcome has its own observation, the same each time it
    /// is observed.
    #[test]
    fn every_recipe_has_its_own_outcome_observation() {
        let mut seen = Vec::new();
        for recipe in R::ALL {
            let resolved = prepared(recipe).resolve().unwrap();
            let observation = observe(&resolved).unwrap();
            assert_eq!(observation, observe(&resolved).unwrap());
            assert!(!seen.contains(&observation), "{recipe:?}");
            seen.push(observation);
        }
    }
}
