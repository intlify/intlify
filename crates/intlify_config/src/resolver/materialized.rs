// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Materialized Resolution Input Set: the finite immutable replay
//! snapshot an invocation returns beside its outcome once entry admission
//! materialized a complete JSON-compatible value (D15:206, 2646-2658).
//!
//! It holds exactly the replay inventory: the Resolver Construction Identity
//! reference the invocation was admitted under, the value as the canonical
//! tree of a marker-free Snapshot of it, whichever entry materialized it, the
//! normalized External Profile Selector Input, and the six values of the
//! Common Invocation Input Set (D15:206, 3561). It holds no entry source,
//! locator, Snapshot digest, or call-site sidecar, which only Entry
//! reproduction needs (D15:2660), and no Derived Admission State:
//! Configuration Schema Selection, the confirmed selector, the admitted
//! Resource Limit Policy, the Sets, the Structural Analysis Result, Findings,
//! and evaluation status are recomputed, never retained (D15:1668, 2658).
//! The resolver derives it; no caller supplies one (D15:3223).
//!
//! The common values are the admitted Resolver Invocation Input Set's,
//! shared through one `Arc` rather than copied, and the value is the set's
//! own copy, so the set refers to no storage a later invocation resets
//! (D15:2750).
//!
//! Two sets are equal slot by slot, by value: the collection as the multiset
//! of its members' `C(member)` bytes, so permuting the submitted members
//! keeps a set equal and changing a member's multiplicity does not, and
//! every other slot by its admitted value (D15:2590). Comparing neither
//! hashes nor copies a value. No `Debug`: the value and the selector carry
//! configuration.

use std::sync::Arc;

use crate::boundary::common::CommonInputs;
use crate::construction::identity::ConstructionIdentity;
use crate::snapshot::Node;
use crate::structural::selection::SelectorInput;

/// A Materialized Resolution Input Set, its canonicalization data a `D`.
pub(crate) struct MaterializedInputs<D> {
    construction_identity: ConstructionIdentity,
    value: Node,
    selector: SelectorInput,
    common: Arc<CommonInputs<D>>,
}

impl<D> MaterializedInputs<D> {
    /// The set of an invocation admitted under `construction_identity` with
    /// `selector` and `common`, whose Phase 1 materialized `value`.
    pub(crate) fn new(
        construction_identity: ConstructionIdentity,
        value: Node,
        selector: SelectorInput,
        common: Arc<CommonInputs<D>>,
    ) -> Self {
        Self {
            construction_identity,
            value,
            selector,
            common,
        }
    }

    /// The Resolver Construction Identity reference a replay must match.
    pub(crate) const fn construction_identity(&self) -> &ConstructionIdentity {
        &self.construction_identity
    }

    /// The complete JSON-compatible configuration value.
    pub(crate) const fn value(&self) -> &Node {
        &self.value
    }

    /// The normalized External Profile Selector Input.
    pub(crate) const fn selector(&self) -> &SelectorInput {
        &self.selector
    }

    /// The Common Invocation Input Set's six values.
    pub(crate) fn common(&self) -> &CommonInputs<D> {
        &self.common
    }
}

/// The common values compare as the Common Invocation Input Set does, the
/// collection as a multiset.
impl<D: Eq> PartialEq for MaterializedInputs<D> {
    fn eq(&self, other: &Self) -> bool {
        self.construction_identity == other.construction_identity
            && self.selector == other.selector
            && *self.common == *other.common
            && self.value == other.value
    }
}

impl<D: Eq> Eq for MaterializedInputs<D> {}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::artifacts::collection::FormalCollection;
    use crate::artifacts::vectors::vocabulary as vectors;
    use crate::artifacts::vocabulary::{FormalVocabulary, IntentSurfaceClassVocabularyArtifact};
    use crate::boundary::capability::ImplementationCapability;
    use crate::boundary::verification::VerificationInput;
    use crate::construction::flow::construct;
    use crate::construction::identity::derive;
    use crate::construction::resolver::committed;
    use crate::construction::root::ConstructionRoot;
    use crate::resolver::invocation::tests::{
        example, materialized, resolver, seal, PinnedFixture, Raw, ROOM,
    };
    use crate::resolver::specification::SpecificationInput;
    use crate::snapshot::from_json;

    /// The set of `raw`'s file, admitted and resolved on the committed
    /// resolver.
    fn of_file(raw: &Raw) -> MaterializedInputs<PinnedFixture> {
        let resolver = resolver();
        materialized(&resolver, &raw.file(&resolver, true)).unwrap()
    }

    /// D15:2660, 3233: a file, the same value written otherwise, and a
    /// Snapshot of it beside a sidecar or not, each marked or not, give equal
    /// sets, which hold the value's canonical tree.
    #[test]
    fn either_entry_of_one_value_gives_an_equal_set() {
        let resolver = resolver();
        let raw = example(|inputs| inputs);
        let set = of_file(&raw);
        assert!(*set.value() == from_json(&raw.config));
        let pretty = Arc::from(serde_json::to_vec_pretty(&raw.config).unwrap());
        let mut entries = vec![
            raw.file(&resolver, false),
            raw.file_of(&resolver, pretty, true),
        ];
        for (marked, mirror) in [(true, false), (false, true)] {
            entries.push(raw.programmatic(&resolver, marked, mirror));
        }
        for inputs in &entries {
            assert!(materialized(&resolver, inputs).unwrap() == set);
        }
    }

    /// D15:2590: the collection is compared as a multiset, so its members in
    /// another order give an equal set and one of them submitted twice does
    /// not.
    #[test]
    fn the_collection_is_compared_as_a_multiset() {
        let mut raw = example(|inputs| inputs);
        let set = of_file(&raw);
        assert!(raw.members.len() >= 3);
        raw.members.reverse();
        assert!(of_file(&raw) == set);
        raw.members.push(raw.members[1].clone());
        assert!(of_file(&raw) != set);
    }

    /// D15:2590: every other slot is compared by value, so changing the
    /// value, the selector, or any common value gives an unequal set, and so
    /// does another Resolver Construction Identity reference.
    #[test]
    fn changing_any_slot_gives_an_unequal_set() {
        let set = of_file(&example(|inputs| inputs));
        let edits: [fn(Raw) -> Raw; 7] = [
            |mut raw| {
                raw.config["profiles"]["web"]["defaultSourceLocale"] = json!("ja");
                raw
            },
            |mut raw| {
                raw.selected = "app".to_owned();
                raw
            },
            |mut raw| {
                let mut input: Value = serde_json::from_slice(&raw.specification).unwrap();
                input.as_object_mut().unwrap().remove("delivery-placement");
                raw.specification = serde_json::to_vec(&input).unwrap();
                raw
            },
            |mut raw| {
                let empty = IntentSurfaceClassVocabularyArtifact::sealed(
                    vectors::IDENTITY,
                    vectors::REVISION,
                    &[],
                );
                raw.vocabulary = serde_json::to_vec(&empty.unwrap()).unwrap();
                raw
            },
            |mut raw| {
                raw.provider.binding.admission.artifact.push('x');
                raw
            },
            |mut raw| {
                let mut input: Value = serde_json::from_slice(&raw.verification).unwrap();
                input["revision"] = json!("2");
                (raw.verification, raw.assertion) = seal(input);
                raw
            },
            |raw| raw.bounded("rawInput.maxFileBytes", ROOM - 1),
        ];
        for edit in edits {
            assert!(of_file(&edit(example(|inputs| inputs))) != set);
        }
        let mut other = of_file(&example(|inputs| inputs));
        assert!(other == set);
        let root = ConstructionRoot::built_in().unwrap();
        let mut admitted = construct(root, committed().input()).unwrap();
        admitted.registry_package.declared.revision = "1".to_owned();
        other.construction_identity = derive(root, &admitted);
        assert!(other != set);
    }

    /// D15:206, 3561: by type, a set holds exactly the replay inventory, the
    /// reference, the value, the selector, and the six common values, and so
    /// no Derived Admission State. Each pattern names every field, so a field
    /// added to either type fails to compile here.
    #[test]
    fn a_set_holds_only_the_replay_inventory() {
        let set = of_file(&example(|inputs| inputs));
        let MaterializedInputs {
            construction_identity,
            value,
            selector,
            common,
        } = &set;
        let _: (&ConstructionIdentity, &Node, &SelectorInput) =
            (construction_identity, value, selector);
        let CommonInputs {
            specification,
            vocabulary,
            data,
            collection,
            verification,
            capability,
        } = &**common;
        let _: (
            &SpecificationInput<String>,
            &FormalVocabulary,
            &PinnedFixture,
            &FormalCollection,
            &VerificationInput,
            &ImplementationCapability,
        ) = (
            specification,
            vocabulary,
            data,
            collection,
            verification,
            capability,
        );
        assert_eq!(*set.construction_identity(), *resolver().identity());
    }
}
