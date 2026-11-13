// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's `profile_resolver_construct` pair.
//!
//! The construction interval admits the complete construction authority and
//! builds the immutable resolver; it ends when the resolver exists. The
//! identity interval derives that resolver's Resolver Construction Identity,
//! which the construction interval excludes (D15:2764, 2775). A refused
//! construction is an expected result, not a harness failure: the refusal is
//! observed, and the input it hands back is released after the end read.

use serde::{Deserialize, Serialize};

use crate::construction::bounds;
use crate::construction::component::Refusal;
use crate::construction::flow::ConstructionInput;
use crate::construction::identity::{self, ConstructionIdentity};
use crate::construction::resolver::{self, ConstructedResolver};
use crate::construction::root::ConstructionRoot;

use super::observation::{Frame, Observation};
use super::operation::OutputFailure;
use super::quantity::Quantity;

/// A construction case: the input construction is given, read before the
/// interval, under the built-in root.
pub(super) struct PreparedConstruction {
    pub(super) root: &'static ConstructionRoot,
    pub(super) input: ConstructionInput,
}

/// An identity case: a resolver constructed under the built-in root before
/// the interval.
pub(super) struct PreparedIdentity {
    pub(super) root: &'static ConstructionRoot,
    pub(super) resolver: ConstructedResolver,
}

/// A construction's result: the resolver, or the refusal with the input it
/// hands back.
pub(super) type Constructed = Result<ConstructedResolver, (Refusal, ConstructionInput)>;

/// A construction's result, with the root it was constructed under.
pub(super) struct Resolved {
    pub(super) root: &'static ConstructionRoot,
    pub(super) constructed: Constructed,
}

/// Construct from the input, as the interval does.
#[expect(
    clippy::result_large_err,
    reason = "the refused input goes back as it was moved in, so refusing allocates nothing"
)]
pub(super) fn build((root, input): (&'static ConstructionRoot, ConstructionInput)) -> Constructed {
    resolver::build(root, input)
}

impl PreparedConstruction {
    /// Construct from a copy of the input, as each repetition does.
    pub(super) fn construct(&self) -> Resolved {
        Resolved {
            root: self.root,
            constructed: resolver::build(self.root, self.input.clone()),
        }
    }

    /// The input's totals in the construction-root bounds' units.
    pub(super) fn totals(&self) -> bounds::Totals {
        bounds::totals(self.root, &self.input)
    }
}

impl PreparedIdentity {
    /// Derive the identity once, outside any interval.
    pub(super) fn derive(&self) -> ConstructionIdentity {
        self.resolver.identity(self.root)
    }

    /// The bytes the identity is the digest of.
    pub(super) fn preimage(&self) -> Vec<u8> {
        identity::preimage(self.root, self.resolver.admitted())
    }
}

/// What a construction case is prepared from, for its descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct InputFacts {
    scope: String,
    root: String,
    given_components: Quantity,
    input_storage: String,
    preparation: String,
    ordinary_output: String,
}

/// How many of the five components an input gives.
fn given(input: &ConstructionInput) -> u64 {
    [
        input.profile_specification_schema.is_some(),
        input.profile_specification.is_some(),
        input.configuration_schema_authority_set.is_some(),
        input.resolver_capability_specification.is_some(),
        input.registry_package.is_some(),
    ]
    .into_iter()
    .map(u64::from)
    .sum()
}

impl InputFacts {
    pub(super) fn construction(prepared: &PreparedConstruction) -> Self {
        Self {
            scope: "complete-resolver-construction-input-set".into(),
            root: "built-in-resolver-construction-root-package".into(),
            given_components: Quantity::new(given(&prepared.input)),
            input_storage: "resident-input-copied-before-each-interval".into(),
            preparation: "artifacts-read-scanned-and-decoded-before-interval".into(),
            ordinary_output:
                "fresh-resolver-or-refusal-with-input-handed-back-released-after-end-read".into(),
        }
    }

    pub(super) fn identity(prepared: &PreparedIdentity) -> Self {
        Self {
            scope: "identity-of-one-constructed-resolver".into(),
            root: "built-in-resolver-construction-root-package".into(),
            given_components: Quantity::new(given(prepared.resolver.input())),
            input_storage: "resolver-constructed-once-before-intervals-and-reused".into(),
            preparation: "construction-before-interval".into(),
            ordinary_output: "identity-bytes-copied-out".into(),
        }
    }
}

/// The construction's observation. A resolver is observed as its identity
/// commits the admitted authorities: the identity's preimage. A refusal is
/// observed as its row, the reason it refuses with, and, at the bounds' row,
/// the bound it names.
pub(super) fn observe(resolved: &Resolved) -> Result<Observation, OutputFailure> {
    let mut shared = Frame::new("resolver-construction");
    match &resolved.constructed {
        Ok(resolver) => {
            shared.uint(0);
            shared.bytes(&identity::preimage(resolved.root, resolver.admitted()));
        }
        Err((refusal, _)) => {
            shared.uint(1);
            shared.uint(u64::try_from(refusal.number()).unwrap_or(u64::MAX));
            shared.json(
                &serde_json::to_value(refusal.reason())
                    .map_err(|_| OutputFailure::ObservationEncoding)?,
            );
            match refusal.bound() {
                Some(bound) => {
                    shared.uint(1);
                    shared.json(
                        &serde_json::to_value(bound)
                            .map_err(|_| OutputFailure::ObservationEncoding)?,
                    );
                }
                None => shared.uint(0),
            }
        }
    }
    Ok(Observation {
        shared: shared.finish(),
        entry: None,
    })
}

/// The identity's observation: its 32 bytes.
pub(super) fn observe_identity(identity: &ConstructionIdentity) -> Observation {
    let mut shared = Frame::new("resolver-construction-identity");
    shared.bytes(identity.bytes());
    Observation {
        shared: shared.finish(),
        entry: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::committed;

    fn root() -> &'static ConstructionRoot {
        ConstructionRoot::built_in().unwrap()
    }

    #[test]
    fn a_construction_case_constructs_from_a_copy_of_its_input() {
        let prepared = PreparedConstruction {
            root: root(),
            input: committed::input(),
        };
        let first = prepared.construct().constructed.unwrap();
        let second = prepared.construct().constructed.unwrap();
        assert_eq!(first.input(), &prepared.input);
        assert_eq!(first.identity(root()), second.identity(root()));
        assert_eq!(
            InputFacts::construction(&prepared).given_components,
            Quantity::new(5)
        );
    }

    #[test]
    fn a_refusal_is_observed_by_its_row_and_hands_its_input_back() {
        let mut input = committed::input();
        input.resolver_capability_specification = None;
        let prepared = PreparedConstruction {
            root: root(),
            input,
        };
        let refused = prepared.construct();
        let Err((refusal, returned)) = &refused.constructed else {
            panic!("the input without a capability is refused");
        };
        assert_eq!(refusal.number(), 25);
        assert_eq!(returned, &prepared.input);
        let constructed = PreparedConstruction {
            root: root(),
            input: committed::input(),
        }
        .construct();
        assert_ne!(observe(&refused).unwrap(), observe(&constructed).unwrap());
        assert_eq!(
            InputFacts::construction(&prepared).given_components,
            Quantity::new(4)
        );
    }

    #[test]
    fn a_constructed_resolver_is_observed_by_the_preimage_of_its_identity() {
        let resolver = resolver::committed();
        let prepared = PreparedIdentity {
            root: root(),
            resolver: resolver::build(root(), committed::input()).unwrap(),
        };
        assert_eq!(prepared.derive(), resolver.identity(root()));
        assert_eq!(
            prepared.preimage(),
            identity::preimage(root(), resolver.admitted())
        );
        let mut frame = Frame::new("resolver-construction");
        frame.uint(0);
        frame.bytes(&prepared.preimage());
        let observation = observe(&Resolved {
            root: root(),
            constructed: resolver::build(root(), committed::input()),
        });
        assert_eq!(observation.unwrap().shared, frame.finish());
        let mut frame = Frame::new("resolver-construction-identity");
        frame.bytes(prepared.derive().bytes());
        assert_eq!(observe_identity(&prepared.derive()).shared, frame.finish());
    }
}
