// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's `evidenceMetadata` bounds that the resolver applies itself,
//! in the order of the capability's table (D15:1834-1839, 1851; Decision
//! 015-197), and path admission under the first two: every Logical input
//! path, Portable Value Locator, and Resolver Input Component Path that a
//! Finding Occurrence or Coverage Decision Evidence candidate's Evidence needs
//! is measured before token generation, each token written without a
//! collision ordinal (D15:724; plan 025 interpretations #234 and #238).
//!
//! A Logical input path past either path bound is absent, as path absence is
//! (D15:776). A value locator or component path past one leaves its Evidence
//! invalid (D15:842), and the Finding Registry fixes each check's evidence
//! kinds, so no other form replaces it: the Entry domain, the only one whose
//! records carry Evidence, is exhausted instead (plan 025 decisions #232,
//! #236, and #247). Where several paths exceed a bound, the one named is the
//! first in the table's order.

use crate::input_limits::Bound;

use super::capability::{BootstrapBoundId, BootstrapGroup, EvidenceMetadataLimits};
use super::evidence_path::PathSize;
use super::tally::Tally;

/// One `evidenceMetadata` bound the resolver applies itself, in the order of
/// 015's table, which `Ord` follows (D15:1834-1839).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum MetadataBound {
    LogicalPathSegments,
    LogicalPathCanonicalBytes,
    SubjectOccurrences,
    TokenInputs,
    CollisionComparisons,
    MetadataWorkUnits,
}

impl MetadataBound {
    /// Where the bound sits in `bootstrapOnly`, whose path the terminal
    /// Finding names (plan 025 decision #231).
    pub(crate) const fn id(self) -> BootstrapBoundId {
        let name = match self {
            Self::LogicalPathSegments => "maxLogicalPathSegments",
            Self::LogicalPathCanonicalBytes => "maxLogicalPathCanonicalBytes",
            Self::SubjectOccurrences => "maxSubjectOccurrences",
            Self::TokenInputs => "maxTokenInputs",
            Self::CollisionComparisons => "maxCollisionComparisons",
            Self::MetadataWorkUnits => "maxMetadataWorkUnits",
        };
        BootstrapBoundId {
            group: BootstrapGroup::EvidenceMetadata,
            name,
        }
    }

    /// The bound's value in `limits`.
    const fn limit(self, limits: &EvidenceMetadataLimits) -> Bound {
        match self {
            Self::LogicalPathSegments => limits.max_logical_path_segments,
            Self::LogicalPathCanonicalBytes => limits.max_logical_path_canonical_bytes,
            Self::SubjectOccurrences => limits.max_subject_occurrences,
            Self::TokenInputs => limits.max_token_inputs,
            Self::CollisionComparisons => limits.max_collision_comparisons,
            Self::MetadataWorkUnits => limits.max_metadata_work_units,
        }
    }

    /// `Err` with the bound when `count` exceeds its value in `limits`.
    fn holds(self, limits: &EvidenceMetadataLimits, count: Tally) -> Result<(), Self> {
        if count.exceeds(self.limit(limits)) {
            Err(self)
        } else {
            Ok(())
        }
    }
}

/// `Err` with the first path bound, in the table's order, that a path of
/// `size` exceeds in `limits` (D15:1834, 1835).
pub(crate) fn path_holds(
    limits: &EvidenceMetadataLimits,
    size: PathSize,
) -> Result<(), MetadataBound> {
    MetadataBound::LogicalPathSegments.holds(limits, Tally::exactly(size.segments))?;
    MetadataBound::LogicalPathCanonicalBytes.holds(limits, Tally::exactly(size.bytes))
}

/// Both values, or the first bound in the table's order that either result
/// names.
pub(crate) fn both<A, B>(
    left: Result<A, MetadataBound>,
    right: Result<B, MetadataBound>,
) -> Result<(A, B), MetadataBound> {
    match (left, right) {
        (Ok(left), Ok(right)) => Ok((left, right)),
        (Err(left), Err(right)) => Err(left.min(right)),
        (Err(bound), Ok(_)) | (Ok(_), Err(bound)) => Err(bound),
    }
}

/// Every value, in order, or the first bound in the table's order that any
/// result names. Every result is visited, since a later one may name an
/// earlier bound.
pub(crate) fn all<T>(
    results: impl IntoIterator<Item = Result<T, MetadataBound>>,
) -> Result<Vec<T>, MetadataBound> {
    let mut values = Ok(Vec::new());
    for result in results {
        values = both(values, result).map(|(mut values, value)| {
            values.push(value);
            values
        });
    }
    values
}

#[cfg(test)]
pub(crate) mod fixtures {
    use crate::input_limits::Bound;
    use crate::resolver::capability::{BootstrapLimits, EvidenceMetadataLimits};

    use super::MetadataBound;

    /// The capability's minimum vector, but every bound the resolver applies
    /// itself at its largest value.
    pub(crate) fn unbounded() -> EvidenceMetadataLimits {
        let largest = Bound::new(u64::MAX - 1).unwrap();
        EvidenceMetadataLimits {
            max_logical_path_segments: largest,
            max_logical_path_canonical_bytes: largest,
            max_subject_occurrences: largest,
            max_token_inputs: largest,
            max_collision_comparisons: largest,
            max_metadata_work_units: largest,
            ..BootstrapLimits::MINIMUM.evidence_metadata
        }
    }

    /// `unbounded`, but `bound` at `limit`.
    pub(crate) fn with(bound: MetadataBound, limit: u64) -> EvidenceMetadataLimits {
        let mut limits = unbounded();
        let limit = Bound::new(limit).unwrap();
        match bound {
            MetadataBound::LogicalPathSegments => limits.max_logical_path_segments = limit,
            MetadataBound::LogicalPathCanonicalBytes => {
                limits.max_logical_path_canonical_bytes = limit;
            }
            MetadataBound::SubjectOccurrences => limits.max_subject_occurrences = limit,
            MetadataBound::TokenInputs => limits.max_token_inputs = limit,
            MetadataBound::CollisionComparisons => limits.max_collision_comparisons = limit,
            MetadataBound::MetadataWorkUnits => limits.max_metadata_work_units = limit,
        }
        limits
    }
}

#[cfg(test)]
mod tests {
    use std::iter::once;

    use serde_json::json;

    use crate::input_limits::Bound;
    use crate::resolver::capability::{BootstrapGroup, BootstrapLimits, EvidenceMetadataLimits};
    use crate::resolver::evaluation::Evaluation;
    use crate::resolver::evidence::EvidenceProjection;
    use crate::resolver::evidence_path::PathSize;
    use crate::resolver::profile_resolution::tests::Inputs;

    use super::fixtures::{unbounded, with};
    use super::{all, both, path_holds, MetadataBound};

    const ALL: [MetadataBound; 6] = [
        MetadataBound::LogicalPathSegments,
        MetadataBound::LogicalPathCanonicalBytes,
        MetadataBound::SubjectOccurrences,
        MetadataBound::TokenInputs,
        MetadataBound::CollisionComparisons,
        MetadataBound::MetadataWorkUnits,
    ];

    /// `complete`'s profile, whose two targets each request a locale outside
    /// the project's that their Target Profiles have no direction for, and
    /// whose coverage rule names a surface class outside the vocabulary, as
    /// the reporting plan's tests build it.
    fn failing() -> Inputs {
        let mut inputs = Inputs::complete();
        for target in ["browser", "server"] {
            inputs.declaration()["targetProfiles"][target]["requestedLocales"] =
                json!(["ja", "en", "fr"]);
        }
        inputs.declaration()["coverage"]["rules"] =
            json!([{"intentSurfaceClasses": ["unknown"], "mode": "fallback-allowed"}]);
        inputs
    }

    /// Each bound is the capability's `evidenceMetadata` bound of its name,
    /// the last six of the group in their order, and reads that bound's value.
    #[test]
    fn each_bound_is_the_capabilitys_bound_of_its_name_in_the_tables_order() {
        let mut next = 0;
        let mut bound = || {
            next += 1;
            Bound::new(next).unwrap()
        };
        let evidence = EvidenceMetadataLimits {
            max_source_identity_bytes: bound(),
            max_source_revision_bytes: bound(),
            max_locator_bytes: bound(),
            max_call_site_source_bytes: bound(),
            max_call_site_mappings: bound(),
            max_logical_path_segments: bound(),
            max_logical_path_canonical_bytes: bound(),
            max_subject_occurrences: bound(),
            max_token_inputs: bound(),
            max_collision_comparisons: bound(),
            max_metadata_work_units: bound(),
        };
        let capability = BootstrapLimits {
            evidence_metadata: evidence,
            ..BootstrapLimits::MINIMUM
        };
        let group: Vec<_> = capability
            .bounds()
            .into_iter()
            .filter(|(id, _)| id.group == BootstrapGroup::EvidenceMetadata)
            .collect();
        let resolver = &group[group.len() - ALL.len()..];
        for (bound, (id, value)) in ALL.into_iter().zip(resolver) {
            assert_eq!(bound.id(), *id);
            assert_eq!(bound.limit(&evidence), *value, "{}", id.name);
        }
        assert!(ALL.windows(2).all(|pair| pair[0] < pair[1]));
    }

    /// A path passes at each bound, and the first bound it exceeds in the
    /// table's order is the one named.
    #[test]
    fn a_path_passes_at_each_bound_and_names_the_first_it_exceeds() {
        let limits = EvidenceMetadataLimits {
            max_logical_path_segments: Bound::new(3).unwrap(),
            max_logical_path_canonical_bytes: Bound::new(50).unwrap(),
            ..unbounded()
        };
        let size = |segments, bytes| PathSize { segments, bytes };
        assert_eq!(path_holds(&limits, size(3, 50)), Ok(()));
        assert_eq!(
            path_holds(&limits, size(4, 50)),
            Err(MetadataBound::LogicalPathSegments)
        );
        assert_eq!(
            path_holds(&limits, size(3, 51)),
            Err(MetadataBound::LogicalPathCanonicalBytes)
        );
        assert_eq!(
            path_holds(&limits, size(4, 51)),
            Err(MetadataBound::LogicalPathSegments)
        );
    }

    /// Joined results name the first bound in the table's order, whichever
    /// result names it.
    #[test]
    fn joined_results_name_the_first_bound_in_the_tables_order() {
        use MetadataBound::{LogicalPathCanonicalBytes as Bytes, LogicalPathSegments as Segments};
        assert_eq!(all([Ok(1), Ok(2)]), Ok(vec![1, 2]));
        assert_eq!(all([Ok(1), Err(Bytes), Err(Segments)]), Err(Segments));
        assert_eq!(all([Err(Segments), Ok(1), Err(Bytes)]), Err(Segments));
        assert_eq!(all::<u8>([]), Ok(Vec::new()));
        assert_eq!(both::<u8, u8>(Err(Bytes), Ok(1)), Err(Bytes));
        assert_eq!(both::<u8, u8>(Ok(1), Err(Bytes)), Err(Bytes));
        assert_eq!(both::<u8, u8>(Err(Bytes), Err(Segments)), Err(Segments));
    }

    /// Every projection of `evaluation`'s candidates, occurrences first.
    fn projections(evaluation: &Evaluation) -> Vec<&EvidenceProjection> {
        evaluation
            .occurrence_candidates()
            .iter()
            .flat_map(|candidate| once(&candidate.primary).chain(&candidate.related))
            .chain(
                evaluation
                    .decision_evidence()
                    .iter()
                    .map(|candidate| &candidate.evidence),
            )
            .collect()
    }

    /// The candidates of `evaluation` as path admission under `limits` leaves
    /// them, by how many there are of each, or the first bound in the
    /// table's order that one exceeds.
    fn admitted(
        evaluation: &Evaluation,
        limits: &EvidenceMetadataLimits,
    ) -> Result<(usize, usize), MetadataBound> {
        both(
            all(evaluation
                .occurrence_candidates()
                .iter()
                .map(|candidate| candidate.admitted(limits))),
            all(evaluation
                .decision_evidence()
                .iter()
                .map(|candidate| candidate.admitted(limits))),
        )
        .map(|(occurrences, decisions)| (occurrences.len(), decisions.len()))
    }

    /// The value locators and component paths that the candidates' Evidence
    /// needs pass at the largest segments and bytes among them, and one below
    /// names that path bound, the segments first, while a Logical input path
    /// past a bound only leaves its Evidence without it (D15:776, 842, 1851;
    /// plan 025 decisions #232 and #236).
    fn required_paths_pass_at_their_largest_size_and_name_it_one_below(evaluation: &Evaluation) {
        let sizes: Vec<PathSize> = projections(evaluation)
            .into_iter()
            .filter_map(EvidenceProjection::required_path)
            .collect();
        assert!(!sizes.is_empty());
        let segments = sizes.iter().map(|size| size.segments).max().unwrap();
        let bytes = sizes.iter().map(|size| size.bytes).max().unwrap();
        let candidates = (
            evaluation.occurrence_candidates().len(),
            evaluation.decision_evidence().len(),
        );
        for (bound, largest) in [
            (MetadataBound::LogicalPathSegments, segments),
            (MetadataBound::LogicalPathCanonicalBytes, bytes),
        ] {
            assert!(largest >= 2, "{bound:?}");
            assert_eq!(admitted(evaluation, &with(bound, largest)), Ok(candidates));
            assert_eq!(admitted(evaluation, &with(bound, largest - 1)), Err(bound));
        }
        let tight = EvidenceMetadataLimits {
            max_logical_path_segments: Bound::new(segments - 1).unwrap(),
            max_logical_path_canonical_bytes: Bound::new(bytes - 1).unwrap(),
            ..unbounded()
        };
        assert_eq!(
            admitted(evaluation, &tight),
            Err(MetadataBound::LogicalPathSegments)
        );
    }

    /// A failing invocation's occurrences need value locators.
    #[test]
    fn the_required_paths_of_a_failing_invocation_pass_at_their_largest_size() {
        failing().resolve(|evaluation, _| {
            assert!(!evaluation.occurrence_candidates().is_empty());
            required_paths_pass_at_their_largest_size_and_name_it_one_below(&evaluation);
        });
    }

    /// A complete invocation's Coverage Decision Evidence needs them.
    #[test]
    fn the_required_paths_of_a_complete_invocation_pass_at_their_largest_size() {
        Inputs::complete().resolve(|evaluation, _| {
            assert!(!evaluation.decision_evidence().is_empty());
            required_paths_pass_at_their_largest_size_and_name_it_one_below(&evaluation);
        });
    }
}
