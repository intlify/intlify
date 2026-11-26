// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Entry projection stage over one file entry's complete invocation
//! (D15:2772, 2776). The stage takes what the Shared projection stage left,
//! so before the interval the invocation's evaluation phases and the Shared
//! projection stage run on an evaluation of their own; the interval runs the
//! Entry projection stage: path admission, the complete invocation's Evidence
//! metadata work, and the Entry domain's slices (plan 025 decisions #253 and
//! #260). The slices borrow the evaluation, so after the end read they are
//! copied out. Neither the result nor its observation is a Profile or a
//! shared artifact.

use std::iter::once;

use crate::benchmark::observation::{Frame, Observation};
use crate::benchmark::operation::OutputFailure;
use crate::resolver::capability::EvidenceMetadataLimits;
use crate::resolver::dependency::EntryInputs;
use crate::resolver::evaluation::{Evaluation, Unit};
use crate::resolver::evidence::EvidenceProjection;
use crate::resolver::evidence_token::EvidenceTokens;
use crate::resolver::outcome::{entry_projection, EntryStage, SharedStage};
use crate::resolver::profile_resolution::{Invocation, Invoked, ResolverInvariant};
use crate::resolver::token::{Sha256, Specification, Tokens};
use crate::resolver::{decision_evidence, occurrence};

use super::{count, domain, observable, specification, Id, Retained};

/// The Entry projection stage: what every timed invocation of
/// `entry_evidence` runs.
pub(in crate::benchmark) fn entry<'e, 'a>(
    (stage, evaluation, invoked, invocation): (
        SharedStage<'e, 'a>,
        &'e Evaluation,
        &'e Invoked<'_, Id>,
        &'e Invocation<'a, Id>,
    ),
) -> Result<EntryStage<'e, 'a>, ResolverInvariant> {
    entry_projection(stage, evaluation, invoked, invocation, &Sha256)
}

/// What the complete invocation counts under the six `evidenceMetadata`
/// bounds the resolver applies itself, in their units (D15:1835-1840, 1852):
/// the largest path Evidence needs, by its segments and by the canonical
/// bytes of its view, since each path is checked on its own, and, over what
/// path admission leaves, the raw candidates, the token inputs, and the
/// comparisons and work units the tokens and projections take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::benchmark) struct InvocationWork {
    pub(in crate::benchmark) path_segments: u64,
    pub(in crate::benchmark) path_bytes: u64,
    /// `None` when a path a record cannot do without exceeds a path bound,
    /// which ends the count before what path admission leaves (plan 025
    /// decision #232).
    pub(in crate::benchmark) admitted: Option<AdmittedWork>,
}

/// What the complete invocation counts over what path admission leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::benchmark) struct AdmittedWork {
    pub(in crate::benchmark) subject_occurrences: u64,
    pub(in crate::benchmark) token_inputs: u64,
    pub(in crate::benchmark) collision_comparisons: u64,
    pub(in crate::benchmark) metadata_work_units: u64,
}

impl InvocationWork {
    /// Counted over `evaluation` and the entry dependency records `entry`
    /// under `specification`'s tokens and `limits`' path bounds, outside the
    /// interval, as `evidence_metadata::invocation` counts them.
    fn of(
        evaluation: &Evaluation,
        entry: &EntryInputs,
        specification: Specification<'_>,
        limits: &EvidenceMetadataLimits,
    ) -> Self {
        let candidates = evaluation.occurrence_candidates();
        let decisions = evaluation.decision_evidence();
        let sizes: Vec<_> = candidates
            .iter()
            .flat_map(|candidate| once(&candidate.primary).chain(&candidate.related))
            .chain(decisions.iter().map(|candidate| &candidate.evidence))
            .flat_map(|projection| {
                projection
                    .required_path()
                    .into_iter()
                    .chain(projection.entry_path())
            })
            .collect();
        Self {
            path_segments: sizes.iter().map(|size| size.segments).max().unwrap_or(0),
            path_bytes: sizes.iter().map(|size| size.bytes).max().unwrap_or(0),
            admitted: AdmittedWork::of(evaluation, entry, specification, limits),
        }
    }
}

impl AdmittedWork {
    fn of(
        evaluation: &Evaluation,
        entry: &EntryInputs,
        specification: Specification<'_>,
        limits: &EvidenceMetadataLimits,
    ) -> Option<Self> {
        let occurrences = evaluation
            .occurrence_candidates()
            .iter()
            .map(|candidate| candidate.admitted(limits))
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        let decisions = evaluation
            .decision_evidence()
            .iter()
            .map(|candidate| candidate.admitted(limits))
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        let units: Vec<&Unit> = evaluation.statuses().map(|(unit, _)| unit).collect();
        let subjects = Tokens::assign(
            specification,
            units.iter().map(|unit| &unit.subject),
            &Sha256,
        );
        let redacted = || {
            occurrence::redacted(&occurrences)
                .chain(decision_evidence::redacted(&decisions))
                .chain(entry.redacted())
        };
        let evidence = EvidenceTokens::assign(specification, redacted(), &Sha256);
        let projections: u64 = occurrences
            .iter()
            .flat_map(|candidate| once(&candidate.primary).chain(&candidate.related))
            .chain(decisions.iter().map(|candidate| &candidate.evidence))
            .map(EvidenceProjection::work)
            .sum();
        Some(Self {
            subject_occurrences: count(evaluation.candidates().count()),
            token_inputs: count(units.len()) + count(redacted().count()),
            collision_comparisons: subjects.comparisons() + evidence.comparisons(),
            metadata_work_units: projections
                + entry.work()
                + subjects.records()
                + evidence.records(),
        })
    }
}

/// The Entry projection stage's complete ordinary output, retained past the
/// end read, with the complete invocation's work and the Profile
/// Specification its subjects' records belong to.
pub(in crate::benchmark) struct Resolved {
    pub(in crate::benchmark) result: Result<Retained, ResolverInvariant>,
    pub(in crate::benchmark) invocation: InvocationWork,
    pub(in crate::benchmark) specification: Specification<'static>,
}

impl Resolved {
    /// The slices borrow the evaluation, so after the end read they are
    /// copied out, and the complete invocation's work counted. Both are
    /// observation, not work of the interval.
    pub(in crate::benchmark) fn retain(
        evaluation: &Evaluation,
        invoked: &Invoked<'_, Id>,
        invocation: &Invocation<'_, Id>,
        stage: Result<EntryStage<'_, '_>, ResolverInvariant>,
    ) -> Self {
        let specification = specification(invocation);
        Self {
            result: stage.map(|stage| Retained::of(stage.committed())),
            invocation: InvocationWork::of(
                evaluation,
                &invoked.entry,
                specification,
                &invocation.capability.bootstrap_only().evidence_metadata,
            ),
            specification,
        }
    }
}

/// A resolver defect is no blocked outcome: the case fails rather than
/// completing as an expected failure. Logical work uses this screen.
pub(in crate::benchmark) fn require_observable(
    resolved: &Resolved,
) -> Result<&Retained, OutputFailure> {
    observable(&resolved.result)
}

/// The Entry projection, all of it entry-specific Evidence, framed in the
/// entry checksum domain. The stage has no Shared semantic output, so its
/// shared domain frames nothing (D15:176, 2799; plan 025 interpretation
/// #259).
pub(in crate::benchmark) fn observe(resolved: &Resolved) -> Result<Observation, OutputFailure> {
    let retained = require_observable(resolved)?;
    let mut entry = Frame::new("entry-evidence");
    domain(&mut entry, resolved.specification, retained)?;
    Ok(Observation {
        shared: Frame::new("entry-evidence-shared").finish(),
        entry: Some(entry.finish()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::cases::prepare::prepare;
    use crate::benchmark::cases::{declarations, EvidenceRecipe as R, Recipe};
    use crate::benchmark::evidence::PreparedEvidence;
    use crate::benchmark::operation::{Operation, Prepared};
    use crate::construction::registry::check::RecordKind;
    use crate::resolver::evaluation::ExhaustedBound;
    use crate::resolver::evidence_metadata::fixtures::{unbounded, with};
    use crate::resolver::evidence_metadata::MetadataBound;
    use crate::resolver::limits::{BoundId, Group};
    use crate::resolver::outcome::outcome;
    use crate::resolver::outcome::tests::identity;

    fn prepared(recipe: R) -> PreparedEvidence {
        let declaration = declarations()
            .into_iter()
            .find(|case| {
                case.operation == Operation::EntryEvidence
                    && case.fixture == Recipe::Evidence(recipe)
                    && case.limit.is_none()
            })
            .unwrap();
        let Prepared::EntryEvidence(prepared) = prepare(&declaration).unwrap().prepared else {
            unreachable!()
        };
        *prepared
    }

    /// `prepared` with the capability's `evidenceMetadata` bounds at
    /// `metadata`.
    fn under(recipe: R, metadata: EvidenceMetadataLimits) -> Resolved {
        let mut prepared = prepared(recipe);
        prepared.capability = prepared.capability.with_evidence_metadata(metadata);
        prepared.resolve_entry().unwrap()
    }

    /// The example's file with its default locale twice: a duplicate member,
    /// so no value, and the duplicate's Logical input path the only path
    /// Evidence carries (D15:781, 3565).
    fn duplicate(metadata: EvidenceMetadataLimits) -> Resolved {
        let member = br#""defaultRequestedLocale":"en""#;
        let mut prepared = prepared(R::Storefront);
        let source = prepared.source.to_vec();
        let at = source
            .windows(member.len())
            .position(|window| window == member)
            .unwrap();
        let mut duplicated = source[..at + member.len()].to_vec();
        duplicated.push(b',');
        duplicated.extend_from_slice(&source[at..]);
        prepared.source = std::sync::Arc::from(duplicated);
        prepared.capability = prepared.capability.with_evidence_metadata(metadata);
        prepared.resolve_entry().unwrap()
    }

    fn kinds(retained: &Retained, kind: RecordKind) -> usize {
        retained
            .slices
            .iter()
            .flat_map(|slice| &slice.records)
            .filter(|record| record.kind == kind)
            .count()
    }

    /// Each recipe's Entry domain commits whole: the example's with no Entry
    /// Finding, the dense one's with an occurrence of each of its Shared
    /// Findings, and the malformed one's with the Finding that leaves it
    /// without a value and the cause that names it (D15:1166, 1177-1184).
    #[test]
    fn each_recipe_commits_the_entry_domain_whole() {
        for recipe in R::ENTRY {
            let retained = prepared(recipe).resolve_entry().unwrap().result.unwrap();
            assert!(retained.exhausted.is_none(), "{recipe:?}");
            let findings = kinds(&retained, RecordKind::EntryFinding);
            let occurrences = kinds(&retained, RecordKind::FindingOccurrence);
            let causes = kinds(&retained, RecordKind::SharedUnavailabilityCause);
            match recipe {
                R::Storefront => assert_eq!((findings, occurrences, causes), (0, 0, 0)),
                R::Blocked => assert!(findings == 0 && occurrences > 0 && causes == 0),
                R::Dense => assert!(occurrences >= 2 * crate::benchmark::cases::DENSE),
                R::Malformed => assert!(findings > 0 && occurrences > 0 && causes > 0),
                R::Inadmissible => assert!(findings == 2 && occurrences > 2 && causes > 0),
                R::Truncated => unreachable!("only the Shared domain's bound exhausts it"),
            }
        }
    }

    /// The stage commits exactly the records of the outcome's Entry
    /// projection, which finalization only orders (D15:2144).
    #[test]
    fn the_stage_commits_the_records_of_the_entry_projection() {
        for recipe in R::ENTRY {
            let prepared = prepared(recipe);
            let invocation = prepared.invocation();
            let mut evaluation = Evaluation::default();
            let mut set = None;
            let invoked = prepared
                .invoke(&mut evaluation, &invocation, &mut set)
                .unwrap();
            let stage = super::super::shared((&evaluation, &invoked, &invocation)).unwrap();
            let retained = Retained::of(
                entry((stage, &evaluation, &invoked, &invocation))
                    .unwrap()
                    .committed(),
            );
            let outcome = outcome(&evaluation, &invoked, identity(), &invocation, &Sha256).unwrap();
            let mut committed: Vec<_> = retained
                .slices
                .into_iter()
                .flat_map(|slice| slice.records)
                .map(|record| (record.kind.id(), record.view.to_string()))
                .collect();
            let mut projected: Vec<_> = outcome
                .evidence
                .entry
                .records
                .into_iter()
                .map(|(_, retained)| (retained.kind.id(), retained.view.to_string()))
                .collect();
            committed.sort();
            projected.sort();
            assert_eq!(committed, projected, "{recipe:?}");
        }
    }

    /// A Logical input path is a path Evidence needs: without a value the
    /// duplicate member's is the largest, and one under its size it is left
    /// absent, which leaves the Entry domain whole (D15:777).
    #[test]
    fn the_largest_path_of_a_file_without_a_value_is_its_logical_input_path() {
        let resolved = duplicate(unbounded());
        let work = resolved.invocation;
        assert!(work.path_segments > 0 && work.path_bytes > 0);
        let absent = duplicate(with(
            MetadataBound::LogicalPathSegments,
            work.path_segments - 1,
        ));
        assert!(absent.result.as_ref().unwrap().exhausted.is_none());
        assert_ne!(observe(&absent).unwrap(), observe(&resolved).unwrap());
    }

    /// Phase 1 projects the duplicate's Logical input path only within the
    /// path bounds, yet the path is still measured past them: one under
    /// either size, the largest path the complete invocation counts is the
    /// same as without the bound (D15:1852; plan 025 decision #299).
    #[test]
    fn a_logical_input_path_past_its_bound_is_still_measured() {
        let work = duplicate(unbounded()).invocation;
        let largest = (work.path_segments, work.path_bytes);
        for (bound, counted) in [
            (MetadataBound::LogicalPathSegments, work.path_segments),
            (MetadataBound::LogicalPathCanonicalBytes, work.path_bytes),
        ] {
            let tight = duplicate(with(bound, counted - 1)).invocation;
            assert_eq!(
                (tight.path_segments, tight.path_bytes),
                largest,
                "{bound:?}"
            );
        }
    }

    /// Each count of the complete invocation is what its bound counts: at
    /// the count nothing is exhausted, and one under it the Entry domain is,
    /// naming the bound, also over what path admission leaves once a path is
    /// past its bound. A count of 0 has no bound under it.
    #[test]
    fn the_invocation_work_is_what_its_bounds_count() {
        let segments = duplicate(unbounded()).invocation.path_segments;
        // The duplicate's path left absent by its bound, beside `limits`.
        let absent = |mut limits: EvidenceMetadataLimits| {
            limits.max_logical_path_segments =
                crate::input_limits::Bound::new(segments - 1).unwrap();
            duplicate(limits)
        };
        let inputs: [&dyn Fn(EvidenceMetadataLimits) -> Resolved; 4] = [
            &|limits| under(R::Storefront, limits),
            &|limits| under(R::Malformed, limits),
            &duplicate,
            &absent,
        ];
        for (input, resolve) in inputs.iter().enumerate() {
            let work = resolve(unbounded()).invocation.admitted.unwrap();
            if input == 3 {
                assert_ne!(Some(work), duplicate(unbounded()).invocation.admitted);
            }
            for (bound, counted) in [
                (MetadataBound::SubjectOccurrences, work.subject_occurrences),
                (MetadataBound::TokenInputs, work.token_inputs),
                (
                    MetadataBound::CollisionComparisons,
                    work.collision_comparisons,
                ),
                (MetadataBound::MetadataWorkUnits, work.metadata_work_units),
            ] {
                if counted == 0 {
                    assert_eq!(bound, MetadataBound::CollisionComparisons);
                    continue;
                }
                let exact = resolve(with(bound, counted)).result.unwrap();
                assert!(exact.exhausted.is_none(), "{input} {bound:?}");
                let over = resolve(with(bound, counted - 1)).result.unwrap();
                assert_eq!(
                    over.exhausted.map(|(_, bound)| bound),
                    Some(ExhaustedBound::Capability(bound.id())),
                    "{input} {bound:?}"
                );
            }
        }
    }

    /// The largest path is what each path bound counts: at its size the
    /// Entry projection is as without the bound, and one under it it is not,
    /// since that path is left absent or ends the count.
    #[test]
    fn the_largest_path_is_what_each_path_bound_counts() {
        let observed = |resolved: &Resolved| observe(resolved).unwrap();
        let unbounded = observed(&under(R::Storefront, unbounded()));
        let work = prepared(R::Storefront).resolve_entry().unwrap().invocation;
        for (bound, counted) in [
            (MetadataBound::LogicalPathSegments, work.path_segments),
            (MetadataBound::LogicalPathCanonicalBytes, work.path_bytes),
        ] {
            assert!(counted > 1, "{bound:?}");
            assert_eq!(
                observed(&under(R::Storefront, with(bound, counted))),
                unbounded
            );
            assert_ne!(
                observed(&under(R::Storefront, with(bound, counted - 1))),
                unbounded,
                "{bound:?}"
            );
        }
    }

    /// A path a record cannot do without past a path bound, a Finding
    /// occurrence's or Coverage Decision Evidence's, exhausts the Entry
    /// domain and ends the complete invocation's count before what path
    /// admission leaves, while the largest path is still the input's (plan
    /// 025 decision #232).
    #[test]
    fn a_required_path_past_its_bound_ends_the_invocation_count() {
        // The example's required paths are its Coverage Decision Evidence's,
        // and the blocked recipe's its Finding occurrences'.
        for recipe in [R::Storefront, R::Blocked] {
            let resolved = under(recipe, with(MetadataBound::LogicalPathSegments, 1));
            assert_eq!(resolved.invocation.admitted, None, "{recipe:?}");
            let unbounded = under(recipe, unbounded()).invocation;
            assert_eq!(
                (
                    resolved.invocation.path_segments,
                    resolved.invocation.path_bytes
                ),
                (unbounded.path_segments, unbounded.path_bytes)
            );
            assert_eq!(
                resolved.result.unwrap().exhausted.map(|(_, bound)| bound),
                Some(ExhaustedBound::Capability(
                    MetadataBound::LogicalPathSegments.id()
                )),
                "{recipe:?}"
            );
        }
    }

    /// An Entry domain a diagnostic bound exhausts is retained with its
    /// terminal cursor and the bound, and observed apart from the one
    /// committed whole (D15:2409).
    #[test]
    fn an_exhausted_entry_domain_is_retained_and_observed_with_its_bound() {
        let records = BoundId {
            group: Group::DiagnosticsEntry,
            name: "maxRecords",
        };
        let whole = prepared(R::Storefront).resolve_entry().unwrap();
        let mut tight = prepared(R::Storefront);
        tight.capability = super::super::tests::capability(Some((records, 1)), unbounded());
        let resolved = tight.resolve_entry().unwrap();
        let (_, bound) = resolved.result.as_ref().unwrap().exhausted.clone().unwrap();
        assert_eq!(bound, ExhaustedBound::Domain(records));
        assert_ne!(
            observe(&resolved).unwrap().entry,
            observe(&whole).unwrap().entry
        );
    }

    /// Each recipe's Entry projection has its own checksum, the same each
    /// time it is observed, and the shared domain frames nothing, the same
    /// for every recipe (D15:2799; plan 025 interpretation #259).
    #[test]
    fn every_recipe_has_its_own_entry_observation_and_an_empty_shared_domain() {
        let empty = Frame::new("entry-evidence-shared").finish();
        let mut seen = Vec::new();
        for recipe in R::ENTRY {
            let resolved = prepared(recipe).resolve_entry().unwrap();
            let observation = observe(&resolved).unwrap();
            assert_eq!(observation, observe(&resolved).unwrap());
            assert_eq!(observation.shared, empty);
            let entry = observation.entry.unwrap();
            assert!(!seen.contains(&entry), "{recipe:?}");
            seen.push(entry);
        }
    }
}
