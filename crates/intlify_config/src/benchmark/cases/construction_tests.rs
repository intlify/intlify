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
    assert_eq!(cases.len(), 318);
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
        cases[315..]
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
    for declaration in &cases[304..] {
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
/// commit: one construction, observed once, with one identity (D15:2583).
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
