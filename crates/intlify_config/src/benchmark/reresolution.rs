// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's materialized re-resolution, which the `profile_resolve_e2e`
//! pair measures from a replay input through its replay result (D15:2773).
//!
//! Before any interval, a case's preparation constructs a resolver from the
//! committed input, as a complete invocation's does, and invokes it once on
//! what a binding submits for one file entry's invocation: the Materialized
//! Resolution Input Set beside that outcome, whose values were admitted then,
//! is the case's replay input (D15:2658, 2829). Each interval replays that
//! set on the same resolver, from the retained set through the identity
//! match to the replay result, and admits nothing at a pre-invocation
//! boundary (D15:2776). The storage the result borrows is made before the
//! start mark, and the result is copied out after the end mark (D15:2750,
//! 2829). A stale set or a resolver defect is a benchmark failure (D15:2831).
//!
//! The semantic checksum covers the Shared projection, and the profile view
//! with each binding's tuple and `C(member)` when the replay has them, in
//! the shared domain: never the Resolver Construction Identity, which Shared
//! Resolution Evidence does not hold, nor Entry evidence, which a replay has
//! none of (D15:175, 2660, 2776; plan 025 decision #316).

use crate::construction::resolver::ConstructedResolver;
use crate::construction::root::ConstructionRoot;
use crate::input_limits::ValueCounts;
use crate::locale::icu4x::Icu4xData;
use crate::locale::VersionedIdentity;
use crate::resolver::invocation::Resolver;
use crate::resolver::materialized::MaterializedInputs;
use crate::resolver::profile_resolution::ResolverInvariant;
use crate::resolver::profile_view::ProfileView;
use crate::resolver::replay::{ConstructionStale, Replay, ReplayStorage};
use crate::resolver::token::Specification;
use crate::snapshot::SnapshotCounts;

use super::evidence::outcome::{project, RetainedProjection};
use super::instrument::{Instrument, Interval};
use super::observation::{Digest, Frame, Observation};
use super::operation::OutputFailure;
use super::workflow::{frame_profile, materialized, retained_profile, Binding, RawInvocation};

/// A replay case's inputs: the resolver its preparation constructed once
/// from the committed input under `root`, what a binding submitted for the
/// invocation whose set it replays, and that set.
pub(in crate::benchmark) struct PreparedReplay {
    pub(in crate::benchmark) root: &'static ConstructionRoot,
    pub(in crate::benchmark) constructed: ConstructedResolver,
    pub(in crate::benchmark) raw: RawInvocation,
    pub(in crate::benchmark) set: MaterializedInputs<Icu4xData>,
}

impl PreparedReplay {
    /// The case replaying the set of `raw`'s invocation on `constructed`;
    /// `None` when that invocation is refused, finds a resolver defect, or
    /// materializes no value.
    pub(in crate::benchmark) fn new(
        root: &'static ConstructionRoot,
        constructed: ConstructedResolver,
        raw: RawInvocation,
    ) -> Option<Self> {
        let (_, set) = materialized(&Resolver::new(&constructed, root), &raw)?;
        Some(Self {
            root,
            constructed,
            raw,
            set: set?,
        })
    }

    /// One replay measured by `instrument`: the resolver and the storage are
    /// made before the start mark, and the result is copied out after the
    /// end mark, before the storage it borrows is released.
    pub(in crate::benchmark) fn interval<I: Instrument>(
        &self,
        instrument: &I,
    ) -> Result<Interval<Resolved>, I::Failure> {
        let resolver = Resolver::new(&self.constructed, self.root);
        let mut storage = ReplayStorage::default();
        let measured = instrument.interval((&resolver, &self.set, &mut storage), replay)?;
        Ok(Interval {
            quantity: measured.quantity,
            output: self.retain(measured.output),
        })
    }

    /// One untimed replay, retained as a timed one is.
    pub(in crate::benchmark) fn resolve(&self) -> Resolved {
        let resolver = Resolver::new(&self.constructed, self.root);
        let mut storage = ReplayStorage::default();
        self.retain(replay((&resolver, &self.set, &mut storage)))
    }

    fn retain(&self, replayed: Replayed<'_>) -> Resolved {
        let result = match replayed {
            Ok(Ok(replay)) => Ok(RetainedReplay {
                shared: RetainedProjection::of(&replay.shared),
                profile: replay.profile.as_ref().map(retained_profile),
            }),
            Ok(Err(stale)) => Err(ReplayFailure::Stale(stale)),
            Err(invariant) => Err(ReplayFailure::Invariant(Box::new(invariant))),
        };
        let supported = self.constructed.supported();
        Resolved {
            result,
            specification: supported.project_profile().specification.clone(),
        }
    }
}

/// What a replay returns.
type Replayed<'s> = Result<Result<Replay<'s, 's>, ConstructionStale>, ResolverInvariant>;

/// The replay of `set` on `resolver`, the result borrowing `storage`.
fn replay<'s>(
    (resolver, set, storage): (
        &'s Resolver<'_>,
        &'s MaterializedInputs<Icu4xData>,
        &'s mut ReplayStorage<'s>,
    ),
) -> Replayed<'s> {
    resolver.replay(set, storage)
}

/// Why a replay yields no result. Neither is an expected result: the set is
/// the resolver's own, and no input causes a resolver defect (D15:2831).
#[derive(Debug, PartialEq, Eq)]
pub(in crate::benchmark) enum ReplayFailure {
    Stale(ConstructionStale),
    Invariant(Box<ResolverInvariant>),
}

/// A replay result as the output retains it: its Shared projection, and its
/// profile view and bindings, each tuple with `C(member)` of its member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::benchmark) struct RetainedReplay {
    pub(in crate::benchmark) shared: RetainedProjection,
    pub(in crate::benchmark) profile: Option<(ProfileView, Vec<Binding>)>,
}

/// A replay's output, retained past the end mark, and the Profile
/// Specification the records belong to.
pub(in crate::benchmark) struct Resolved {
    pub(in crate::benchmark) result: Result<RetainedReplay, ReplayFailure>,
    pub(in crate::benchmark) specification: VersionedIdentity<String>,
}

/// What one replay counts in the units of the bounds that count it
/// (D15:2801): its set's marker-free value, as a Snapshot's root value is
/// counted (D15:1890), and its members with their `C(member)` bytes, all
/// admitted when the set was created; then its result's Shared records with
/// their accounting bytes, and the bindings beside its profile, none without
/// one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::benchmark) struct ReplayCounts {
    pub(in crate::benchmark) value: ValueCounts,
    pub(in crate::benchmark) members: u64,
    pub(in crate::benchmark) member_bytes: u64,
    pub(in crate::benchmark) shared_records: u64,
    pub(in crate::benchmark) shared_bytes: u64,
    pub(in crate::benchmark) bindings: u64,
}

impl ReplayCounts {
    /// The counts of `retained`, the replay of `set`; `None` when one is not
    /// representable.
    pub(in crate::benchmark) fn of(
        set: &MaterializedInputs<Icu4xData>,
        retained: &RetainedReplay,
    ) -> Option<Self> {
        let count = |value: usize| u64::try_from(value).ok();
        let members = set.common().collection.members();
        let member_bytes = members.iter().try_fold(0_u64, |total, member| {
            total.checked_add(count(member.canonical_body().len())?)
        })?;
        let records = &retained.shared.records;
        let shared_bytes = records
            .iter()
            .try_fold(0_u64, |total, (_, record)| total.checked_add(record.bytes))?;
        let bindings = retained
            .profile
            .as_ref()
            .map_or(0, |(_, bindings)| bindings.len());
        Some(Self {
            value: SnapshotCounts::of(set.value()).value(),
            members: count(members.len())?,
            member_bytes,
            shared_records: count(records.len())?,
            shared_bytes,
            bindings: count(bindings)?,
        })
    }
}

/// The replay result's shared domain, and no entry domain (plan 025
/// decision #316).
pub(in crate::benchmark) fn observe(resolved: &Resolved) -> Result<Observation, OutputFailure> {
    let retained = resolved
        .result
        .as_ref()
        .map_err(|_| OutputFailure::ReplayFailure)?;
    let specification = Specification {
        identity: &resolved.specification.identity,
        revision: &resolved.specification.revision,
    };
    let shared = frame(specification, &retained.shared, retained.profile.as_ref())?;
    Ok(Observation {
        shared,
        entry: None,
    })
}

/// The shared domain of a replay whose Shared projection is `shared` and
/// whose profile is `profile`, framed as an outcome's are, so that what an
/// invocation's outcome predicts of the replay of its set can be framed too
/// (D15:2660).
pub(in crate::benchmark) fn frame(
    specification: Specification<'_>,
    shared: &RetainedProjection,
    profile: Option<&(ProfileView, Vec<Binding>)>,
) -> Result<Digest, OutputFailure> {
    let mut frame = Frame::new("materialized-reresolution");
    project(&mut frame, specification, shared)?;
    frame_profile(&mut frame, profile)?;
    Ok(frame.finish())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::sync::Arc;

    use intlify_measurement::measurement::Aggregation;

    use super::*;
    use crate::artifacts::envelope::FormalMember;
    use crate::benchmark::cases::prepare::{prepare, Candidate};
    use crate::benchmark::cases::workflow_inputs::{
        programmatic, replayed, submission, ProgrammaticRecipe as P,
    };
    use crate::benchmark::cases::{
        declarations, EntryKind, EvidenceRecipe as R, Recipe, ReplayRecipe,
    };
    use crate::benchmark::operation::{Output as CaseOutput, Prepared};
    use crate::benchmark::quantity::Quantity;
    use crate::benchmark::workflow::RawEntry;
    use crate::construction::resolver::{build, committed};
    use crate::construction::{flow, identity};
    use crate::materialize::materialize_file;
    use crate::snapshot::{Marker, MarkerReason, Node};

    /// A set with the profile, and one semantically blocked after
    /// materialization, each through either entry.
    const RECIPES: [(R, P); 2] = [(R::Storefront, P::Storefront), (R::Blocked, P::Blocked)];

    fn root() -> &'static ConstructionRoot {
        ConstructionRoot::built_in().unwrap()
    }

    fn constructed() -> ConstructedResolver {
        build(root(), committed().input().clone()).unwrap()
    }

    /// The replay of the set of `recipe`'s file entry invocation.
    fn file(recipe: R) -> PreparedReplay {
        let constructed = constructed();
        let raw = submission(recipe, &constructed, root()).unwrap();
        PreparedReplay::new(root(), constructed, raw).unwrap()
    }

    fn specification(resolved: &Resolved) -> Specification<'_> {
        Specification {
            identity: &resolved.specification.identity,
            revision: &resolved.specification.revision,
        }
    }

    /// D15:2660, D26:1815; plan 025 decision #316: a replay holds and frames
    /// what the invocation whose set it replays predicts of it: that
    /// outcome's Shared projection and its profile view and bindings, with
    /// no entry domain.
    #[test]
    fn a_replay_frames_what_the_invocation_of_its_set_predicts() {
        for (recipe, _) in RECIPES {
            let prepared = file(recipe);
            let resolver = Resolver::new(&prepared.constructed, prepared.root);
            let (outcome, set) = materialized(&resolver, &prepared.raw).unwrap();
            assert!(set.is_some_and(|set| set == prepared.set), "{recipe:?}");
            assert_eq!(outcome.profile.is_some(), recipe.checked(), "{recipe:?}");
            let shared = outcome.shared.unwrap();
            let resolved = prepared.resolve();
            let retained = resolved.result.as_ref().unwrap();
            assert_eq!(retained.shared, shared, "{recipe:?}");
            assert_eq!(retained.profile, outcome.profile, "{recipe:?}");
            let asserted: serde_json::Value =
                serde_json::from_slice(&prepared.raw.specification).unwrap();
            let pinned = &asserted["project-profile"];
            assert_eq!(pinned["identity"], resolved.specification.identity.as_str());
            assert_eq!(pinned["revision"], resolved.specification.revision.as_str());
            let framed = frame(specification(&resolved), &shared, outcome.profile.as_ref());
            let expected = Observation {
                shared: framed.unwrap(),
                entry: None,
            };
            assert_eq!(observe(&resolved), Ok(expected), "{recipe:?}");
        }
    }

    /// D15:2660, 2791: the set of a programmatic entry's invocation, whose
    /// Snapshot is the value the file spells, replays as the file's set does.
    #[test]
    fn a_programmatic_entry_set_replays_as_the_file_set() {
        for (recipe, input) in RECIPES {
            let constructed = constructed();
            let raw = programmatic(input, &constructed, root()).unwrap();
            assert!(matches!(raw.entry, RawEntry::Programmatic { .. }));
            let replay = PreparedReplay::new(root(), constructed, raw).unwrap();
            let paired = observe(&file(recipe).resolve());
            assert_eq!(observe(&replay.resolve()), paired, "{recipe:?}");
        }
    }

    /// `recipe`'s declared case, as preparation makes it.
    fn declared(recipe: ReplayRecipe) -> Candidate {
        let declaration = declarations()
            .into_iter()
            .find(|case| case.fixture == Recipe::Replay(recipe));
        prepare(&declaration.unwrap()).unwrap()
    }

    fn held(candidate: &Candidate) -> &PreparedReplay {
        let Prepared::Replay(prepared) = &candidate.prepared else {
            unreachable!()
        };
        prepared
    }

    /// D15:2660, D26:1815; plan 025 decision #316: each declared replay
    /// frames what the invocation whose set it replays predicts of it: that
    /// outcome's Shared projection, and the profile view and bindings of the
    /// checked invocation of the example, the outcome's own but when an
    /// Entry-domain bound withheld them. That invocation is blocked, yet its
    /// replay has them, and frames what the example's does.
    #[test]
    fn each_declared_replay_frames_what_the_invocation_of_its_set_predicts() {
        let checked = declared(ReplayRecipe::Storefront);
        let example = held(&checked);
        let resolver = Resolver::new(&example.constructed, example.root);
        let (outcome, _) = materialized(&resolver, &example.raw).unwrap();
        let profile = outcome.profile;
        assert!(profile.is_some());
        for recipe in ReplayRecipe::ALL {
            let candidate = declared(recipe);
            let prepared = held(&candidate);
            let resolver = Resolver::new(&prepared.constructed, prepared.root);
            let (outcome, set) = materialized(&resolver, &prepared.raw).unwrap();
            assert!(set.is_some_and(|set| set == prepared.set), "{recipe:?}");
            let source = candidate.declaration.fixture.source();
            assert_eq!(source, *prepared.raw.file(), "{recipe:?}");
            let storefront = recipe == ReplayRecipe::Storefront;
            assert_eq!(outcome.profile.is_some(), storefront, "{recipe:?}");
            let shared = outcome.shared.unwrap();
            assert!(shared.terminal.is_none(), "{recipe:?}");
            let withheld = recipe == ReplayRecipe::EntryWithheld;
            let predicted = if withheld { &profile } else { &outcome.profile };
            let CaseOutput::Replay(resolved) = &candidate.output else {
                unreachable!()
            };
            let framed = frame(specification(resolved), &shared, predicted.as_ref());
            assert_eq!(framed, Ok(candidate.observation.shared), "{recipe:?}");
            assert_eq!(candidate.observation.entry, None);
            let same = candidate.observation == checked.observation;
            assert_eq!(same, recipe.profiled(), "{recipe:?}");
        }
    }

    /// D15:2660, 2791: the set of a programmatic entry's invocation of each
    /// declared recipe, the withheld one's included, replays as the file's
    /// set does.
    #[test]
    fn each_declared_recipe_replays_alike_through_either_entry() {
        for recipe in ReplayRecipe::ALL {
            let file = declared(recipe);
            let built = constructed();
            let raw = replayed(recipe, EntryKind::Programmatic, &built, root()).unwrap();
            assert!(matches!(raw.entry, RawEntry::Programmatic { .. }));
            let replay = PreparedReplay::new(root(), built, raw).unwrap();
            let observed = observe(&replay.resolve());
            assert_eq!(observed, Ok(file.observation), "{recipe:?}");
        }
    }

    /// D15:2658, 2831: a case needs the set of an invocation that materializes
    /// a value: a refused submission has no outcome, and a file with no value
    /// has no set beside its outcome.
    #[test]
    fn a_case_needs_an_invocation_that_materializes_a_set() {
        let built = constructed();
        let mut refused = submission(R::Storefront, &built, root()).unwrap();
        refused.reference.push('0');
        assert!(PreparedReplay::new(root(), built, refused).is_none());
        let built = constructed();
        let malformed = submission(R::Malformed, &built, root()).unwrap();
        assert!(PreparedReplay::new(root(), built, malformed).is_none());
    }

    /// D15:2776, 2829: the interval begins with the retained set, so what
    /// it is handed is the resolver, the set, and the storage the result
    /// borrows, never a submission, and its result is the untimed replay's.
    #[test]
    fn the_interval_is_handed_the_retained_set() {
        struct Recording(RefCell<Vec<&'static str>>);
        impl Instrument for Recording {
            type Failure = ();
            const AGGREGATION: Aggregation = Aggregation::BatchTotal;
            fn interval<Input, Output>(
                &self,
                input: Input,
                invoke: fn(Input) -> Output,
            ) -> Result<Interval<Output>, ()> {
                self.0.borrow_mut().push(std::any::type_name::<Input>());
                let output = invoke(input);
                Ok(Interval {
                    quantity: Quantity::new(0),
                    output,
                })
            }
            fn unavailable() {}
        }
        let prepared = file(R::Storefront);
        let recording = Recording(RefCell::new(Vec::new()));
        let measured = prepared.interval(&recording).unwrap();
        assert_eq!(observe(&measured.output), observe(&prepared.resolve()));
        let handed = recording.0.into_inner();
        assert_eq!(handed.len(), 1);
        let input = handed[0];
        assert!(input.contains("MaterializedInputs") && input.contains("ReplayStorage"));
        assert!(!input.contains("Submitted") && !input.contains("RawInvocation"));
    }

    /// D15:175, 2776; plan 025 decision #316: the codec frames, under its own
    /// name, the Shared projection, then the profile view and each binding's
    /// tuple and body behind their flag, and no Resolver Construction
    /// Identity; a replay without the profile frames the flag alone.
    #[test]
    fn a_replay_is_framed_as_the_codec_spells_it() {
        let resolved = file(R::Storefront).resolve();
        let retained = resolved.result.as_ref().unwrap();
        let (view, bindings) = retained.profile.as_ref().unwrap();
        let mut shared = Frame::new("materialized-reresolution");
        project(&mut shared, specification(&resolved), &retained.shared).unwrap();
        shared.flag(true);
        shared.json(&serde_json::to_value(view).unwrap());
        shared.uint(u64::try_from(bindings.len()).unwrap());
        for (tuple, body) in bindings {
            shared.text(tuple.kind.spelling());
            shared.text(tuple.identity.as_str());
            shared.text(tuple.revision.as_str());
            shared.text(tuple.specification_revision.as_str());
            shared.text(tuple.semantic_digest.as_str());
            shared.bytes(body);
        }
        let framed = Observation {
            shared: shared.finish(),
            entry: None,
        };
        assert_eq!(observe(&resolved), Ok(framed));

        let resolved = file(R::Blocked).resolve();
        let retained = resolved.result.as_ref().unwrap();
        assert!(retained.profile.is_none());
        let mut shared = Frame::new("materialized-reresolution");
        project(&mut shared, specification(&resolved), &retained.shared).unwrap();
        shared.flag(false);
        let framed = Observation {
            shared: shared.finish(),
            entry: None,
        };
        assert_eq!(observe(&resolved), Ok(framed));
    }

    /// Plan 025 decision #316: the shared domain tells apart the Shared
    /// projection, the profile, each binding, and its body.
    #[test]
    fn every_part_of_the_result_is_observed() {
        let resolved = file(R::Storefront).resolve();
        let observed = |edit: &dyn Fn(&mut RetainedReplay)| {
            let mut retained = resolved.result.as_ref().unwrap().clone();
            edit(&mut retained);
            observe(&Resolved {
                result: Ok(retained),
                specification: resolved.specification.clone(),
            })
            .unwrap()
        };
        let whole = observed(&|_| {});
        let edits: [&dyn Fn(&mut RetainedReplay); 5] = [
            &|retained| _ = retained.shared.records.pop(),
            &|retained| retained.shared.records[0].1.bytes += 1,
            &|retained| retained.profile = None,
            &|retained| _ = retained.profile.as_mut().unwrap().1.pop(),
            &|retained| retained.profile.as_mut().unwrap().1[0].1.push(0),
        ];
        for (index, edit) in edits.into_iter().enumerate() {
            assert_ne!(observed(edit).shared, whole.shared, "{index}");
        }
    }

    /// D15:2586, 2660, 2831: a set of another construction is stale and one
    /// holding a marker is a resolver defect; neither is a result, so each
    /// fails the observation.
    #[test]
    fn a_stale_set_and_a_resolver_defect_are_benchmark_failures() {
        let mut prepared = file(R::Storefront);
        let members = prepared.raw.members();
        let submitted = prepared.raw.submitted(&members);
        let resolver = Resolver::new(&prepared.constructed, prepared.root);
        let admitted = resolver
            .admit::<_, Icu4xData>(submitted, &prepared.raw.assertion)
            .unwrap();
        let mut other = flow::construct(root(), committed().input()).unwrap();
        other.registry_package.declared.revision = "1".to_owned();
        let selector = prepared.set.selector().clone();
        let stale = MaterializedInputs::new(
            identity::derive(root(), &other),
            prepared.set.value().clone(),
            selector.clone(),
            Arc::clone(&admitted.common),
        );
        let marked = MaterializedInputs::new(
            *prepared.set.construction_identity(),
            Node::Marker(Marker::Rejected(MarkerReason::UnsupportedType)),
            selector,
            Arc::clone(&admitted.common),
        );
        prepared.set = stale;
        let resolved = prepared.resolve();
        let failure = ReplayFailure::Stale(ConstructionStale);
        assert_eq!(resolved.result.as_ref().err(), Some(&failure));
        assert_eq!(observe(&resolved), Err(OutputFailure::ReplayFailure));
        prepared.set = marked;
        let resolved = prepared.resolve();
        let failure = ReplayFailure::Invariant(Box::new(ResolverInvariant::RetainedMarker));
        assert_eq!(resolved.result.as_ref().err(), Some(&failure));
        assert_eq!(observe(&resolved), Err(OutputFailure::ReplayFailure));
    }

    /// D15:1890, 2801: a replay counts its set's value as strict file entry
    /// counts the file, the set's members and their `C(member)` bytes, its
    /// Shared records with their bytes, and the bindings beside its profile,
    /// none without one.
    #[test]
    fn a_replay_counts_its_set_and_its_result() {
        let count = |len: usize| u64::try_from(len).unwrap();
        for (recipe, _) in RECIPES {
            let prepared = file(recipe);
            let resolved = prepared.resolve();
            let retained = resolved.result.as_ref().unwrap();
            let counts = ReplayCounts::of(&prepared.set, retained).unwrap();
            let limits = crate::materialize_tests::limits();
            let document = materialize_file(Arc::clone(prepared.raw.file()), limits).unwrap();
            assert_eq!(counts.value, document.counts().unwrap().value, "{recipe:?}");
            let bodies = prepared.raw.members.iter().map(|member| {
                let value = serde_json::from_slice(member).unwrap();
                FormalMember::new(value).unwrap().canonical_body().len()
            });
            let bytes = bodies.sum();
            assert_eq!(counts.members, count(prepared.raw.members.len()));
            assert_eq!(counts.member_bytes, count(bytes), "{recipe:?}");
            let records = &retained.shared.records;
            assert_eq!(counts.shared_records, count(records.len()));
            let total: u64 = records.iter().map(|(_, record)| record.bytes).sum();
            assert_eq!(counts.shared_bytes, total, "{recipe:?}");
            let bindings = retained.profile.as_ref().map_or(0, |(_, each)| each.len());
            assert_eq!(counts.bindings, count(bindings), "{recipe:?}");
            assert_eq!(counts.bindings > 0, recipe.checked(), "{recipe:?}");
        }
    }
}
