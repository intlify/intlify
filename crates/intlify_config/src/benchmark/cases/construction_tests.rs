// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use super::prepare::{prepare, Candidate};
use super::*;

use crate::benchmark::observation::Observation;
use crate::benchmark::operation::{Output, Prepared};
use crate::construction::committed;
use crate::construction::resolver;
use crate::construction::root::ConstructionRoot;

use ConstructionRecipe as R;

fn root() -> &'static ConstructionRoot {
    ConstructionRoot::built_in().unwrap()
}

fn prepared(operation: Operation, recipe: ConstructionRecipe) -> Candidate {
    let case = declarations()
        .into_iter()
        .find(|case| case.operation == operation && case.fixture == Recipe::Construction(recipe))
        .unwrap();
    prepare(&case).unwrap()
}

fn observed(operation: Operation, recipe: ConstructionRecipe) -> Observation {
    prepared(operation, recipe).observation
}

#[test]
fn construction_cases_keep_the_304_rows_and_append_after_them() {
    let cases = declarations();
    assert_eq!(cases.len(), 347);
    assert!(cases[..304].iter().all(|case| !matches!(
        case.operation,
        Operation::ConstructionAuthorityAdmission | Operation::ConstructionIdentity
    )));
    // Every recipe once, in its order, then the identity of each that
    // constructs.
    assert_eq!(
        cases[304..315]
            .iter()
            .map(|case| (case.operation, case.fixture))
            .collect::<Vec<_>>(),
        R::ALL
            .into_iter()
            .map(|recipe| (
                Operation::ConstructionAuthorityAdmission,
                Recipe::Construction(recipe)
            ))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        cases[315..318]
            .iter()
            .map(|case| (case.operation, case.fixture))
            .collect::<Vec<_>>(),
        [
            R::Committed,
            R::ProfileSpecificationLayout,
            R::UnmappedSchema
        ]
        .into_iter()
        .map(|recipe| (
            Operation::ConstructionIdentity,
            Recipe::Construction(recipe)
        ))
        .collect::<Vec<_>>()
    );
    for declaration in &cases[304..318] {
        assert_eq!(declaration.selector, Selector::Absent);
        assert!(declaration.limit.is_none());
        let Recipe::Construction(recipe) = declaration.fixture else {
            unreachable!()
        };
        // The case's source is what each component of its input declares.
        let source: serde_json::Value =
            serde_json::from_slice(&declaration.fixture.source()).unwrap();
        assert_eq!(source, recipe.source(), "{declaration:?}");
        let candidate = prepare(declaration).unwrap();
        assert!(candidate.input_limits.is_none(), "nothing is materialized");
        assert!(candidate.prepared.prerequisites_admitted());
        match (&candidate.prepared, &candidate.output) {
            (Prepared::Construction(prepared), Output::Construction(resolved)) => {
                assert_eq!(prepared.input, recipe.input(), "{declaration:?}");
                assert_eq!(
                    resolved
                        .constructed
                        .as_ref()
                        .err()
                        .map(|(refusal, _)| refusal.number()),
                    recipe.refused_at(),
                    "{declaration:?}"
                );
            }
            (Prepared::ConstructionIdentity(prepared), Output::ConstructionIdentity(identity)) => {
                assert_eq!(prepared.resolver.input(), &recipe.input());
                assert_eq!(*identity, resolver::committed().identity(root()));
            }
            _ => panic!("wrong owner boundary"),
        }
    }
}

/// Each refused recipe's input is the committed input with one change, and
/// the change is what its row refuses.
#[test]
fn each_refused_recipe_is_refused_at_its_own_row() {
    let rows = R::ALL
        .into_iter()
        .filter_map(ConstructionRecipe::refused_at)
        .collect::<Vec<_>>();
    assert_eq!(rows, [2, 8, 15, 25, 31, 39, 42, 43]);
    for recipe in R::ALL {
        let input = recipe.input();
        let changed = [
            input.profile_specification_schema != committed::input().profile_specification_schema,
            input.profile_specification != committed::input().profile_specification,
            input.configuration_schema_authority_set
                != committed::input().configuration_schema_authority_set,
            input.resolver_capability_specification
                != committed::input().resolver_capability_specification,
            input.registry_package != committed::input().registry_package,
        ]
        .into_iter()
        .filter(|changed| *changed)
        .count();
        assert_eq!(changed, usize::from(recipe != R::Committed), "{recipe:?}");
        let refused = resolver::build(root(), input)
            .err()
            .map(|(refusal, _)| refusal.number());
        assert_eq!(refused, recipe.refused_at(), "{recipe:?}");
    }
}

/// The inputs that construct differ only in bytes the identity does not
/// commit: one construction, observed once, with one identity (D15:2589).
#[test]
fn the_inputs_that_construct_are_one_construction() {
    let construction = Operation::ConstructionAuthorityAdmission;
    let identity = Operation::ConstructionIdentity;
    for recipe in [R::ProfileSpecificationLayout, R::UnmappedSchema] {
        assert_eq!(
            observed(construction, recipe),
            observed(construction, R::Committed)
        );
        assert_eq!(observed(identity, recipe), observed(identity, R::Committed));
        // The input context still binds the bytes each case is given.
        assert_ne!(
            super::context::observe(&prepared(construction, recipe)).unwrap(),
            super::context::observe(&prepared(construction, R::Committed)).unwrap()
        );
    }
    // Each refusal is observed apart from every other and from a
    // construction.
    let mut refusals = R::ALL
        .into_iter()
        .filter(|recipe| !recipe.constructs())
        .map(|recipe| observed(construction, recipe).shared.bytes())
        .collect::<Vec<_>>();
    refusals.push(observed(construction, R::Committed).shared.bytes());
    let count = refusals.len();
    refusals.sort_unstable();
    refusals.dedup();
    assert_eq!(refusals.len(), count);
}

/// The edges of the construction-root bounds follow the 318 rows, each the
/// committed recipe with its bound and edge, in the bounds' canonical order;
/// the two validation-work bounds' edges follow those 343 rows.
#[test]
fn bound_edges_keep_the_318_rows_and_append_after_them() {
    use super::construction_inputs::{bound, edge_refused_at, EDGES, VALIDATION_WORK_EDGES};

    let cases = declarations();
    let mut start = 318;
    for (bounds, kinds, count) in [(&EDGES[..], 13, 25), (&VALIDATION_WORK_EDGES[..], 2, 4)] {
        let edges = bounds
            .iter()
            .flat_map(|&(kind, first_over)| {
                [(kind, LimitEdge::Exact)]
                    .into_iter()
                    .chain(first_over.then_some((kind, LimitEdge::FirstOver)))
            })
            .collect::<Vec<_>>();
        assert_eq!(edges.len(), count);
        let mut order = Vec::new();
        for (declaration, &(kind, edge)) in cases[start..].iter().zip(&edges) {
            assert_eq!(
                declaration.operation,
                Operation::ConstructionAuthorityAdmission
            );
            assert_eq!(declaration.fixture, Recipe::Construction(R::Committed));
            assert_eq!(declaration.limit, Some((kind, edge)));
            assert_eq!(
                declaration.expected_kind == ExpectedKind::ConstructionConstructed,
                edge_refused_at(kind, edge).is_none()
            );
            order.push(bound(kind).unwrap());
        }
        // The bounds' canonical order, each once.
        order.dedup();
        let canonical = crate::construction::admission::ConstructionBoundId::ALL
            .into_iter()
            .filter(|bound| order.contains(bound))
            .collect::<Vec<_>>();
        assert_eq!(order, canonical);
        assert_eq!(order.len(), kinds);
        start += count;
    }
    assert_eq!(start, 343 + 4);
    assert_eq!(cases.len(), start);
}

/// Each edge's input is at its bound or one past it, a first-over input is
/// refused at the bounds' row naming that bound, and an exact one passes
/// that row.
#[test]
fn each_edge_is_at_its_bound_or_one_past_it() {
    use super::construction_inputs::bound;

    for declaration in &declarations()[318..] {
        let (kind, edge) = declaration.limit.unwrap();
        let bound = bound(kind).unwrap();
        let candidate = prepare(declaration).unwrap();
        let (Prepared::Construction(prepared), Output::Construction(resolved)) =
            (&candidate.prepared, &candidate.output)
        else {
            panic!("wrong owner boundary")
        };
        let over = u64::from(edge == LimitEdge::FirstOver);
        assert_eq!(
            prepared.totals().get(bound),
            Some(bound.value().get() + over),
            "{declaration:?}"
        );
        let refusal = resolved
            .constructed
            .as_ref()
            .err()
            .map(|(refusal, _)| *refusal);
        match edge {
            LimitEdge::FirstOver => {
                let refusal = refusal.unwrap();
                assert_eq!((refusal.number(), refusal.bound()), (1, Some(bound)));
            }
            LimitEdge::Exact => {
                assert_ne!(
                    refusal.map(crate::construction::component::Refusal::number),
                    Some(1),
                    "{declaration:?}"
                );
            }
        }
    }
}

/// The edges #134 leaves out: an earlier bound in the canonical order is
/// exceeded first, so the bound is never the one reported.
#[test]
fn the_edges_left_out_are_exceeded_after_an_earlier_bound() {
    use crate::construction::admission::ConstructionBoundId as B;
    use crate::construction::bounds;
    use crate::construction::component::Artifact;

    let first = |input: &crate::construction::flow::ConstructionInput| {
        resolver::build(root(), input.clone())
            .err()
            .and_then(|(refusal, _)| refusal.bound())
    };
    let unmapped = |schemas: Vec<Vec<u8>>| {
        let mut input = committed::input();
        let set = input.configuration_schema_authority_set.as_mut().unwrap();
        for (index, schema) in schemas.into_iter().enumerate() {
            set.members
                .insert((index + 1).to_string(), Artifact::read(schema));
        }
        input
    };
    // Every entry is a node and every artifact's root one more, so the
    // entries reach their bound only past the nodes'.
    let input = committed::input();
    let totals = bounds::totals(root(), &input);
    assert_eq!(
        totals.get(B::InputMaxNodes).unwrap() - totals.get(B::InputMaxCollectionEntries).unwrap(),
        12
    );
    let zeros = |count: usize| serde_json::to_vec(&vec![0; count]).unwrap();
    let entries = B::InputMaxCollectionEntries.value().get();
    let missing =
        usize::try_from(entries - totals.get(B::InputMaxCollectionEntries).unwrap()).unwrap();
    let input = unmapped(vec![zeros(missing / 2), zeros(missing - missing / 2)]);
    assert_eq!(
        bounds::totals(root(), &input).get(B::InputMaxCollectionEntries),
        Some(entries)
    );
    assert_eq!(first(&input), Some(B::InputMaxNodes));
    // A decoded string is never longer than its bytes, so the strings reach
    // their bound only past the artifacts' total bytes.
    let strings = B::InputMaxTotalStringBytes.value().get();
    assert_eq!(strings, B::InputMaxTotalArtifactBytes.value().get());
    assert!(totals.get(B::InputMaxTotalStringBytes) < totals.get(B::InputMaxTotalArtifactBytes));
    let half = usize::try_from(strings / 8).unwrap();
    let input = unmapped(
        (0..8)
            .map(|_| serde_json::to_vec(&"a".repeat(half)).unwrap())
            .collect(),
    );
    assert!(bounds::totals(root(), &input).get(B::InputMaxTotalStringBytes) >= Some(strings));
    assert_eq!(first(&input), Some(B::InputMaxTotalArtifactBytes));
    // Three registries, each within the input's single-artifact bound, which
    // comes first: their total stays under its bound, and one over the
    // single member's bound is one over the input's too.
    let single = B::InputMaxSingleArtifactBytes.value().get();
    assert_eq!(
        single,
        B::RegistryMaxSingleMemberArtifactBytes.value().get()
    );
    assert!(3 * single < B::RegistryMaxTotalArtifactBytes.value().get());
    let mut members = committed::REGISTRY_MEMBERS.map(<[u8]>::to_vec);
    members[4].resize(usize::try_from(single + 1).unwrap(), b' ');
    let mut input = committed::input();
    input.registry_package = Some(committed::package(
        committed::PACKAGE,
        members.each_ref().map(Vec::as_slice),
    ));
    assert_eq!(first(&input), Some(B::InputMaxSingleArtifactBytes));
}
