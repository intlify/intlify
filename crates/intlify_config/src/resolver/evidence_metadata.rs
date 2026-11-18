// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's `evidenceMetadata` bounds that the resolver applies itself:
//! the two path bounds, under which every Logical input path, Portable Value
//! Locator, and Resolver Input Component Path that a Finding Occurrence or
//! Coverage Decision Evidence candidate's Evidence needs is admitted, and the
//! four counts of the resolver's own Evidence metadata work, which no adapter
//! applies (D15:724, 1834-1839, 1851; Decision 015-197).
//!
//! Both come once after the last evaluation phase, in two scopes, each before
//! its domain plans any reporting slice: first the Shared scope, the
//! candidates and subjects of the units that report into the Shared domain,
//! which the construction context and Materialized Resolution Input Set alone
//! determine, in the Shared projection stage ([`shared_scope`]), and then the
//! complete invocation, whose paths are checked before evidence token
//! generation, in the Entry projection stage ([`invocation`]) (D15:1851,
//! 2775; plan 025 decisions #230 and #253). A Logical
//! input path past either path bound is absent, as path absence is (D15:776).
//! A value locator or component path past one leaves its Evidence invalid
//! (D15:842), and the Finding Registry fixes each check's evidence kinds, so
//! no other form replaces it: the Entry domain, the only one whose records
//! carry Evidence, is exhausted instead (plan 025 decisions #232 and #236). So
//! it is when only a count of the complete invocation exceeds its bound, while
//! the Shared domain reports as it would within every bound, so that paired
//! entries keep equal Shared projections (D15:2659, 3228; plan 025 decision
//! #247). A count of the Shared scope past its bound exhausts every domain
//! (#229). Each domain's terminal Finding names the first bound in the table's
//! order that its scope exceeds, which is also the order the work is checked
//! in: the path bounds, then subject occurrences, token inputs, collision
//! comparisons, and metadata work units, so no token of a scope is generated
//! past its token inputs' bound.
//!
//! A subject occurrence is a raw candidate the evaluation recorded, before its
//! logical subject is deduplicated. A token input is the subject of a recorded
//! unit, one for each unit, or a redacted component of an admitted candidate's
//! Evidence or of an entry dependency record, whose tokens share Evidence's
//! colliding sets (D15:2194; plan 025 interpretation #245). A collision
//! comparison is one pair of a colliding set of subject or evidence token
//! records. A metadata work unit is one source identity, locator, Logical
//! input path or value locator, selector, or component path that an admitted
//! candidate's Evidence or an entry dependency record carries, or one token
//! record (D15:1851; plan 025 interpretation #239). The diagnostic domain's
//! subject token, which only the reserved terminal records name, is outside
//! the counts (plan 025 interpretation #237).

use std::iter::once;

use crate::input_limits::Bound;

use super::capability::{BootstrapBoundId, BootstrapGroup, EvidenceMetadataLimits};
use super::decision_evidence::{self, DecisionEvidence};
use super::dependency::EntryInputs;
use super::evaluation::{Evaluation, Unit};
use super::evidence_path::PathSize;
use super::evidence_token::EvidenceTokens;
use super::occurrence::{self, OccurrenceCandidate};
use super::reporting::ReportingDomain;
use super::tally::{total, Tally};
use super::token::{Digest, Specification, Tokens};

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

/// The Evidence candidates of an invocation as path admission leaves them,
/// with the tokens of their invocation.
#[derive(Debug)]
pub(crate) struct Admitted {
    /// The Finding Occurrence candidates, in the evaluation's order.
    pub(crate) occurrences: Vec<OccurrenceCandidate>,
    /// The Coverage Decision Evidence candidates, in the evaluation's order.
    pub(crate) decisions: Vec<DecisionEvidence>,
    /// The token of the subject of every unit the evaluation recorded.
    pub(crate) subjects: Tokens,
    /// The token of every redacted component of the admitted Evidence.
    pub(crate) evidence: EvidenceTokens,
}

/// The first bound in the table's order that each domain's scope exceeds,
/// if one does (plan 025 decision #247).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Exceeded {
    pub(crate) entry: Option<MetadataBound>,
    pub(crate) shared: Option<MetadataBound>,
}

impl Exceeded {
    /// The bound `domain`'s scope exceeds, if one does.
    pub(crate) const fn of(self, domain: ReportingDomain) -> Option<MetadataBound> {
        match domain {
            ReportingDomain::Entry => self.entry,
            ReportingDomain::Shared => self.shared,
        }
    }
}

/// What the resolver's own Evidence metadata work leaves to report.
#[derive(Debug)]
pub(crate) enum Admission {
    /// Every path and count is within its bound.
    Admitted(Admitted),
    /// The complete invocation exceeds `entry`, the first bound in the
    /// table's order it exceeds, while the Shared scope exceeds none: only
    /// the Entry domain is exhausted, and the Shared domain reports with
    /// `shared`, the tokens of its units' subjects.
    EntryExhausted {
        entry: MetadataBound,
        shared: Tokens,
    },
    /// The Shared scope exceeds `shared`, so every domain is exhausted, the
    /// Entry domain at `entry`, the first bound the complete invocation
    /// exceeds.
    Exhausted {
        entry: MetadataBound,
        shared: MetadataBound,
    },
}

impl Admission {
    /// The bound each domain's scope exceeds.
    pub(crate) const fn exceeded(&self) -> Exceeded {
        match self {
            Self::Admitted(_) => Exceeded {
                entry: None,
                shared: None,
            },
            Self::EntryExhausted { entry, .. } => Exceeded {
                entry: Some(*entry),
                shared: None,
            },
            Self::Exhausted { entry, shared } => Exceeded {
                entry: Some(*entry),
                shared: Some(*shared),
            },
        }
    }
}

/// What the Evidence metadata work of an invocation's Shared scope leaves:
/// the first bound in the table's order the scope exceeds, if one does, and
/// the tokens of its subjects once its token inputs are within their bound.
#[derive(Debug)]
pub(crate) enum SharedScope {
    /// Every count of the Shared scope is within its bound.
    Within(Tokens),
    /// A count of the Shared scope exceeds `bound`. `tokens` are its subject
    /// tokens when its token inputs were within their bound, which the counts
    /// of the complete invocation take in.
    Exceeded {
        bound: MetadataBound,
        tokens: Option<Tokens>,
    },
}

impl SharedScope {
    /// The first bound in the table's order the Shared scope exceeds, if one
    /// does.
    pub(crate) const fn exceeded(&self) -> Option<MetadataBound> {
        match self {
            Self::Within(_) => None,
            Self::Exceeded { bound, .. } => Some(*bound),
        }
    }
}

/// Whether `unit` reports its Findings and status into the Shared domain.
fn reports_into_shared(unit: &Unit) -> bool {
    ReportingDomain::of(unit.phase) == ReportingDomain::Shared
}

/// Checks the Evidence metadata work of `evaluation`'s Shared scope, the
/// candidates and subjects of the units that report into the Shared domain,
/// against the four counts of `limits` in the table's order, assigning under
/// `digest` the scope's subject tokens only once its token inputs are within
/// their bound. Those subjects have colliding sets of their own, so their
/// tokens are the invocation's (D15:1851, 2194; plan 025 decisions #171,
/// #247, and #253).
pub(crate) fn shared_scope(
    evaluation: &Evaluation,
    specification: Specification<'_>,
    limits: &EvidenceMetadataLimits,
    digest: &impl Digest,
) -> SharedScope {
    let units: Vec<&Unit> = evaluation
        .statuses()
        .map(|(unit, _)| unit)
        .filter(|unit| reports_into_shared(unit))
        .collect();
    let inputs = MetadataBound::SubjectOccurrences
        .holds(
            limits,
            Tally::of(
                evaluation
                    .candidates()
                    .filter(|(unit, _)| reports_into_shared(unit))
                    .count(),
            ),
        )
        .and_then(|()| MetadataBound::TokenInputs.holds(limits, Tally::of(units.len())));
    if let Err(bound) = inputs {
        return SharedScope::Exceeded {
            bound,
            tokens: None,
        };
    }
    let tokens = Tokens::assign(
        specification,
        units.iter().map(|unit| &unit.subject),
        digest,
    );
    let counted = MetadataBound::CollisionComparisons
        .holds(limits, Tally::exactly(tokens.comparisons()))
        .and_then(|()| {
            MetadataBound::MetadataWorkUnits.holds(limits, Tally::exactly(tokens.records()))
        });
    match counted {
        Ok(()) => SharedScope::Within(tokens),
        Err(bound) => SharedScope::Exceeded {
            bound,
            tokens: Some(tokens),
        },
    }
}

/// Admits the Evidence of `evaluation`'s candidates under the path bounds of
/// `limits`, and checks the Evidence metadata work of the complete invocation,
/// which holds that of its Shared scope, `scope`, and of the entry dependency
/// records `entry` names, against the four counts, assigning under `digest`
/// the tokens of the other units' subjects and of the admitted Evidence only
/// once the invocation's token inputs are within their bound (D15:1851, 2194;
/// plan 025 decisions #247 and #253, interpretation #245).
pub(crate) fn invocation(
    evaluation: &Evaluation,
    entry: &EntryInputs,
    specification: Specification<'_>,
    limits: &EvidenceMetadataLimits,
    digest: &impl Digest,
    scope: SharedScope,
) -> Admission {
    let (shared_units, entry_units): (Vec<&Unit>, Vec<&Unit>) = evaluation
        .statuses()
        .map(|(unit, _)| unit)
        .partition(|unit| reports_into_shared(unit));
    let invocation_inputs = both(
        all(evaluation
            .occurrence_candidates()
            .iter()
            .map(|candidate| candidate.admitted(limits))),
        all(evaluation
            .decision_evidence()
            .iter()
            .map(|candidate| candidate.admitted(limits))),
    )
    .and_then(|(occurrences, decisions)| {
        MetadataBound::SubjectOccurrences
            .holds(limits, Tally::of(evaluation.candidates().count()))?;
        let components = occurrence::redacted(&occurrences)
            .chain(decision_evidence::redacted(&decisions))
            .chain(entry.redacted())
            .count();
        MetadataBound::TokenInputs.holds(
            limits,
            Tally::of(shared_units.len())
                .plus(Tally::of(entry_units.len()))
                .plus(Tally::of(components)),
        )?;
        Ok((occurrences, decisions))
    });
    // The complete invocation holds the Shared scope, so past a bound of the
    // Shared scope it exceeds one no later in the table's order.
    let (exceeded, shared_tokens) = match scope {
        SharedScope::Within(tokens) => (None, tokens),
        SharedScope::Exceeded {
            bound,
            tokens: Some(tokens),
        } => (Some(bound), tokens),
        SharedScope::Exceeded {
            bound,
            tokens: None,
        } => {
            return Admission::Exhausted {
                entry: invocation_inputs.err().unwrap_or(bound),
                shared: bound,
            }
        }
    };
    let invocation = invocation_inputs.and_then(|(occurrences, decisions)| {
        let subjects = shared_tokens.clone().merge(Tokens::assign(
            specification,
            entry_units.iter().map(|unit| &unit.subject),
            digest,
        ));
        let evidence = EvidenceTokens::assign(
            specification,
            occurrence::redacted(&occurrences)
                .chain(decision_evidence::redacted(&decisions))
                .chain(entry.redacted()),
            digest,
        );
        MetadataBound::CollisionComparisons.holds(
            limits,
            Tally::exactly(subjects.comparisons()).plus(Tally::exactly(evidence.comparisons())),
        )?;
        let projections = occurrences
            .iter()
            .flat_map(|candidate| once(&candidate.primary).chain(&candidate.related))
            .chain(decisions.iter().map(|candidate| &candidate.evidence))
            .map(|projection| Tally::exactly(projection.work()))
            .chain(once(Tally::exactly(entry.work())));
        MetadataBound::MetadataWorkUnits.holds(
            limits,
            total(projections.chain([subjects.records(), evidence.records()].map(Tally::exactly))),
        )?;
        Ok(Admitted {
            occurrences,
            decisions,
            subjects,
            evidence,
        })
    });
    match (exceeded, invocation) {
        (Some(shared), invocation) => Admission::Exhausted {
            entry: invocation.err().unwrap_or(shared),
            shared,
        },
        (None, Err(entry)) => Admission::EntryExhausted {
            entry,
            shared: shared_tokens,
        },
        (None, Ok(admitted)) => Admission::Admitted(admitted),
    }
}

#[cfg(any(test, feature = "benchmark"))]
pub(crate) mod fixtures {
    use crate::input_limits::Bound;
    use crate::resolver::capability::{BootstrapLimits, EvidenceMetadataLimits};

    #[cfg(test)]
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
    #[cfg(test)]
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

/// Both scopes' Evidence metadata work, as the Shared and the Entry
/// projection stages check it in turn.
#[cfg(test)]
pub(crate) fn admit(
    evaluation: &Evaluation,
    entry: &EntryInputs,
    specification: Specification<'_>,
    limits: &EvidenceMetadataLimits,
    digest: &impl Digest,
) -> Admission {
    let scope = shared_scope(evaluation, specification, limits, digest);
    invocation(evaluation, entry, specification, limits, digest, scope)
}

#[cfg(test)]
mod tests {
    use std::iter::once;

    use serde_json::json;

    use crate::artifacts::count;
    use crate::boundary::source::{file_source_revision, read_pair, SourceLimits};
    use crate::input_limits::Bound;
    use crate::resolver::capability::{BootstrapGroup, BootstrapLimits, EvidenceMetadataLimits};
    use crate::resolver::dependency::fixtures::{file_entry, redacted_entry};
    use crate::resolver::dependency::EntryInputs;
    use crate::resolver::evaluation::{
        Blocking, Check, Code, Evaluation, Found, Observation, Phase, Subject, Unit,
    };
    use crate::resolver::evidence::{EvidenceProjection, Source};
    use crate::resolver::evidence_path::{PathSize, Step};
    use crate::resolver::evidence_token::EvidenceTokens;
    use crate::resolver::locales::fixtures::key;
    use crate::resolver::locales::LocaleSubject;
    use crate::resolver::origin::SafeOriginKind as O;
    use crate::resolver::profile_resolution::tests::Inputs;
    use crate::resolver::token::{Digest, Sha256, Specification, Tokens};
    use crate::resolver::{decision_evidence, occurrence};

    use super::fixtures::{unbounded, with};
    use super::{admit, all, both, path_holds, shared_scope, Exceeded, MetadataBound, SharedScope};
    use crate::resolver::reporting::ReportingDomain;

    const PROFILE: Specification<'static> = Specification {
        identity: "intlify-project-profile",
        revision: "0",
    };

    const ALL: [MetadataBound; 6] = [
        MetadataBound::LogicalPathSegments,
        MetadataBound::LogicalPathCanonicalBytes,
        MetadataBound::SubjectOccurrences,
        MetadataBound::TokenInputs,
        MetadataBound::CollisionComparisons,
        MetadataBound::MetadataWorkUnits,
    ];

    /// A digest under which every record collides.
    struct Constant;

    impl Digest for Constant {
        fn digest(&self, _: &[u8]) -> [u8; 32] {
            [7; 32]
        }
    }

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
            assert_eq!(
                admit(
                    evaluation,
                    file_entry(),
                    PROFILE,
                    &with(bound, largest),
                    &Sha256
                )
                .exceeded(),
                Exceeded::default()
            );
            // Only the Entry domain's records carry Evidence (plan 025
            // decision #247).
            assert_eq!(
                admit(
                    evaluation,
                    file_entry(),
                    PROFILE,
                    &with(bound, largest - 1),
                    &Sha256
                )
                .exceeded(),
                Exceeded {
                    entry: Some(bound),
                    shared: None,
                }
            );
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

    /// What each count of `evaluation` is under `digest`, as 015's table
    /// counts it, with nothing past a path bound.
    fn counts(
        evaluation: &Evaluation,
        entry: &EntryInputs,
        digest: &impl Digest,
    ) -> [(MetadataBound, u64); 4] {
        let subjects = Tokens::assign(
            PROFILE,
            evaluation.statuses().map(|(unit, _)| &unit.subject),
            digest,
        );
        let components: Vec<_> = occurrence::redacted(evaluation.occurrence_candidates())
            .chain(decision_evidence::redacted(evaluation.decision_evidence()))
            .chain(entry.redacted())
            .collect();
        let inputs = evaluation.statuses().count() + components.len();
        let evidence = EvidenceTokens::assign(PROFILE, components, digest);
        let projected: u64 = projections(evaluation)
            .into_iter()
            .map(EvidenceProjection::work)
            .sum::<u64>()
            + entry.work();
        [
            (
                MetadataBound::SubjectOccurrences,
                count(evaluation.candidates().count()),
            ),
            (MetadataBound::TokenInputs, count(inputs)),
            (
                MetadataBound::CollisionComparisons,
                subjects.comparisons() + evidence.comparisons(),
            ),
            (
                MetadataBound::MetadataWorkUnits,
                projected + subjects.records() + evidence.records(),
            ),
        ]
    }

    /// What each count of `evaluation`'s Shared scope is under `digest`: the
    /// candidates and subjects of the units that report into the Shared
    /// domain, and the comparisons and records of their tokens.
    fn shared_counts(evaluation: &Evaluation, digest: &impl Digest) -> [(MetadataBound, u64); 4] {
        let shared = |unit: &Unit| ReportingDomain::of(unit.phase) == ReportingDomain::Shared;
        let subjects: Vec<&Subject> = evaluation
            .statuses()
            .filter(|(unit, _)| shared(unit))
            .map(|(unit, _)| &unit.subject)
            .collect();
        let tokens = Tokens::assign(PROFILE, subjects.iter().copied(), digest);
        [
            (
                MetadataBound::SubjectOccurrences,
                count(
                    evaluation
                        .candidates()
                        .filter(|(unit, _)| shared(unit))
                        .count(),
                ),
            ),
            (MetadataBound::TokenInputs, count(subjects.len())),
            (MetadataBound::CollisionComparisons, tokens.comparisons()),
            (MetadataBound::MetadataWorkUnits, tokens.records()),
        ]
    }

    /// Each count passes at exactly what the complete invocation counts, and
    /// one below exhausts the Entry domain, naming the bound. Down to what
    /// the Shared scope counts only the Entry domain is exhausted, and one
    /// below that every domain is, both naming the bound, the counts checked
    /// after admission and in the table's order, under a digest that makes
    /// every record of a kind collide so the comparisons count too
    /// (D15:1836-1839, 1851; plan 025 decision #247, interpretation #239).
    fn each_count_passes_exactly_and_names_itself_one_over(evaluation: &Evaluation) {
        let redacted = redacted_entry();
        assert_eq!(
            admit(evaluation, &redacted, PROFILE, &unbounded(), &Sha256).exceeded(),
            Exceeded::default()
        );
        let exceeded = |bound, limit| {
            admit(
                evaluation,
                &redacted,
                PROFILE,
                &with(bound, limit),
                &Constant,
            )
            .exceeded()
        };
        let scopes: Vec<(MetadataBound, u64, u64)> = counts(evaluation, &redacted, &Constant)
            .into_iter()
            .zip(shared_counts(evaluation, &Constant))
            .map(|((bound, count), (_, shared))| (bound, count, shared))
            .collect();
        for &(bound, count, shared) in &scopes {
            assert!(
                count >= 2 && shared <= count,
                "{bound:?} counts {count}, {shared}"
            );
            assert_eq!(exceeded(bound, count), Exceeded::default(), "{bound:?}");
            let entry = Some(bound);
            assert_eq!(
                exceeded(bound, count - 1),
                Exceeded {
                    entry,
                    shared: (shared == count).then_some(bound),
                },
                "{bound:?}"
            );
            if (1..count).contains(&shared) {
                assert_eq!(
                    exceeded(bound, shared),
                    Exceeded {
                        entry,
                        shared: None,
                    },
                    "{bound:?}"
                );
            }
            if shared >= 2 {
                assert_eq!(
                    exceeded(bound, shared - 1),
                    Exceeded {
                        entry,
                        shared: entry,
                    },
                    "{bound:?}"
                );
            }
        }
        // Some count exhausts only the Entry domain, and some every domain.
        assert!(scopes
            .iter()
            .any(|(_, count, shared)| (1..*count).contains(shared)));
        assert!(scopes.iter().any(|(_, _, shared)| *shared >= 2));
    }

    /// Each domain names the first bound in the table's order that its own
    /// scope exceeds: past a value locator's path bound and a Shared-scope
    /// count, the Entry domain names the path bound and the Shared domain the
    /// count, whether the Shared scope exceeds its inputs or the work its
    /// tokens take (D15:1851; plan 025 decision #247).
    #[test]
    fn each_domain_names_the_first_bound_its_scope_exceeds() {
        failing().resolve(|evaluation, _| {
            let one = Bound::new(1).unwrap();
            for (shared, limits) in [
                (
                    MetadataBound::SubjectOccurrences,
                    EvidenceMetadataLimits {
                        max_logical_path_segments: one,
                        max_subject_occurrences: one,
                        ..unbounded()
                    },
                ),
                (
                    MetadataBound::MetadataWorkUnits,
                    EvidenceMetadataLimits {
                        max_logical_path_segments: one,
                        max_metadata_work_units: one,
                        ..unbounded()
                    },
                ),
            ] {
                assert_eq!(
                    admit(&evaluation, file_entry(), PROFILE, &limits, &Sha256).exceeded(),
                    Exceeded {
                        entry: Some(MetadataBound::LogicalPathSegments),
                        shared: Some(shared),
                    }
                );
            }
        });
    }

    /// The Shared projection stage counts the Shared scope alone, its inputs
    /// before any token: within every bound it keeps the tokens of the
    /// scope's subjects, past an input count it keeps none, since none were
    /// assigned, and past a count of its tokens it keeps them for the counts
    /// of the complete invocation, under a digest that makes every record of
    /// a kind collide so the comparisons count too (D15:1851; plan 025
    /// decision #253).
    #[test]
    fn the_shared_scope_keeps_its_tokens_once_its_inputs_are_within_their_bound() {
        failing().resolve(|evaluation, _| {
            let subjects = evaluation
                .statuses()
                .filter(|(unit, _)| ReportingDomain::of(unit.phase) == ReportingDomain::Shared)
                .map(|(unit, _)| &unit.subject);
            let tokens = Tokens::assign(PROFILE, subjects, &Constant);
            match shared_scope(&evaluation, PROFILE, &unbounded(), &Constant) {
                SharedScope::Within(within) => assert_eq!(within, tokens),
                scope @ SharedScope::Exceeded { .. } => panic!("{scope:?}"),
            }
            for (bound, counted) in shared_counts(&evaluation, &Constant) {
                assert!(counted > 1, "{bound:?}");
                let assigned = !matches!(
                    bound,
                    MetadataBound::SubjectOccurrences | MetadataBound::TokenInputs
                );
                match shared_scope(&evaluation, PROFILE, &with(bound, counted - 1), &Constant) {
                    SharedScope::Exceeded {
                        bound: named,
                        tokens: kept,
                    } => {
                        assert_eq!(named, bound);
                        assert_eq!(kept, assigned.then(|| tokens.clone()), "{bound:?}");
                    }
                    scope @ SharedScope::Within(_) => panic!("{bound:?}: {scope:?}"),
                }
            }
        });
    }

    /// A complete invocation counts the Coverage Decision Evidence of its
    /// decision table.
    #[test]
    fn each_count_of_a_complete_invocation_passes_exactly_and_names_itself_one_over() {
        Inputs::complete().resolve(|evaluation, _| {
            assert!(evaluation.occurrence_candidates().is_empty());
            assert!(!evaluation.decision_evidence().is_empty());
            each_count_passes_exactly_and_names_itself_one_over(&evaluation);
        });
    }

    /// A failing invocation counts the Evidence of its Findings' occurrences.
    #[test]
    fn each_count_of_a_failing_invocation_passes_exactly_and_names_itself_one_over() {
        failing().resolve(|evaluation, _| {
            assert!(!evaluation.occurrence_candidates().is_empty());
            each_count_passes_exactly_and_names_itself_one_over(&evaluation);
        });
    }

    /// Materialized Value Evidence of `steps` in a file whose owner marked
    /// its source identity when `marked`.
    fn located(marked: bool, steps: &[Step<'_>]) -> EvidenceProjection {
        let largest = Bound::new(u64::MAX - 1).unwrap();
        let limits = SourceLimits {
            identity_bytes: largest,
            revision_bytes: largest,
        };
        let pair = read_pair(
            Some("intlify-file-source.v0:storefront/intlify.config.json"),
            Some(&file_source_revision(b"")),
            b"",
            limits,
        )
        .unwrap();
        let source = Source {
            identity: pair.identity(),
            marked,
            locator: None,
        };
        EvidenceProjection::configuration_value(O::ConfigurationFile, source, steps, None).unwrap()
    }

    /// The requested locales `en` and `ja` each found twice, observed at a
    /// value of a file whose owner did not mark its source identity beside an
    /// unsafe member of one whose owner did: Evidence whose components are a
    /// source identity and a member, of two records.
    fn redacting() -> Evaluation {
        let mut evaluation = Evaluation::default();
        for locale in ["en", "ja"] {
            let unit = Unit {
                phase: Phase::LocaleResolution,
                subject: Subject::Locale(LocaleSubject::RequestedLocale(key(locale))),
                check: Check::UniqueLocale,
            };
            let found = Blocking {
                code: Code::LocaleDuplicate,
                reason: None,
                details: None,
            };
            let observation = Observation {
                primary: located(
                    false,
                    &[
                        Step::Safe("profiles"),
                        Step::Safe("web"),
                        Step::Safe("requestedLocales"),
                        Step::Index(0),
                    ],
                ),
                related: vec![located(
                    true,
                    &[
                        Step::Safe("profiles"),
                        Step::Unsafe,
                        Step::Safe("requestedLocales"),
                        Step::Index(1),
                    ],
                )],
            };
            evaluation
                .check_found(unit, Ok(()), |()| {
                    Err::<(), _>(Found::observed(found, observation))
                })
                .unwrap()
                .unwrap_err();
        }
        evaluation
    }

    /// Evidence that redacts a source identity and a member counts each
    /// component as a token input and each record as work, and the one pair
    /// of records when they collide (D15:1837-1839; plan 025 interpretation
    /// #239).
    #[test]
    fn each_count_of_redacted_evidence_passes_exactly_and_names_itself_one_over() {
        let evaluation = redacting();
        let components: Vec<_> = occurrence::redacted(evaluation.occurrence_candidates()).collect();
        assert_eq!(components.len(), 4);
        let evidence = EvidenceTokens::assign(PROFILE, components, &Constant);
        assert_eq!((evidence.records(), evidence.comparisons()), (2, 1));
        each_count_passes_exactly_and_names_itself_one_over(&evaluation);
    }

    /// A file entry's redacted source identity and selector are two token
    /// inputs, and their projections, the locator among them, and their two
    /// token records are work, whatever the rest of the invocation counts.
    /// They are Entry records, so past a bound only for them only the Entry
    /// domain is exhausted (D15:1837, 1839, 1851; plan 025 decision #247,
    /// interpretation #245).
    #[test]
    fn the_entry_dependencies_count_as_token_inputs_and_work() {
        let entry = redacted_entry();
        Inputs::complete().resolve(|evaluation, _| {
            let [_, (_, plain_inputs), _, (_, plain_work)] =
                counts(&evaluation, file_entry(), &Sha256);
            let [_, (_, inputs), _, (_, work)] = counts(&evaluation, &entry, &Sha256);
            assert_eq!(inputs, plain_inputs + 2);
            assert_eq!(work, plain_work + 1 + 2);
            for (bound, count) in [
                (MetadataBound::TokenInputs, inputs),
                (MetadataBound::MetadataWorkUnits, work),
            ] {
                let exceeded = |limit| {
                    admit(&evaluation, &entry, PROFILE, &with(bound, limit), &Sha256).exceeded()
                };
                assert_eq!(exceeded(count), Exceeded::default(), "{bound:?}");
                assert_eq!(
                    exceeded(count - 1),
                    Exceeded {
                        entry: Some(bound),
                        shared: None,
                    },
                    "{bound:?}"
                );
            }
        });
    }
}
