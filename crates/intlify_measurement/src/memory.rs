// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Memory Observation Domain a memory case declares.
//!
//! Memory is not a Performance Surface. A memory case keeps the surface whose
//! work it encloses and states beside it what its observation counts: which
//! observer, which storage, which parts of the process, which threads, and
//! from which reuse state. Two memory cases whose domains differ do not
//! observe the same thing, so the domain is one of the case's dimensions, and
//! a memory case without one is never projected.
//!
//! The six parts are design 026's. What the storage classes, the coverage,
//! and the reuse state are called is the owner's registered spelling, so this
//! module checks that a domain does not contradict itself, not what its
//! spellings mean.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::identity::{Token, VersionedIdentity};

/// Whether an observation counts the storage of one part of the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Inclusion {
    Included,
    Excluded,
    // The part does not exist in this process, such as a JIT under ahead-of-time
    // compilation, which is not the same as a part that exists and is left out.
    NotApplicable,
}

/// The storage classes an observation counts, and those it leaves out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageClasses {
    #[schemars(length(min = 1))]
    pub included: Vec<Token>,
    pub excluded: Vec<Token>,
}

/// The parts of a process design 026 asks every memory observation to place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Components {
    pub harness: Inclusion,
    pub language_runtime: Inclusion,
    pub jit: Inclusion,
    pub shared_libraries: Inclusion,
    pub locale_data: Inclusion,
}

/// Which process and threads an observation covers, and whether it counts
/// workers and child processes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Coverage {
    pub process: Token,
    pub thread: Token,
    pub workers: Inclusion,
    pub child_processes: Inclusion,
}

/// Which allocations an observation attributes to the case, and the reuse
/// state it starts from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reuse {
    pub ownership: Token,
    pub state: Token,
}

/// The versioned descriptor of what one memory measurement observed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryObservationDomain {
    pub identity: VersionedIdentity,
    pub observer: Token,
    pub storage: StorageClasses,
    pub components: Components,
    pub coverage: Coverage,
    pub reuse: Reuse,
}

/// Why a Memory Observation Domain does not say what it observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainFailure {
    /// The domain counts no storage class at all.
    NothingIncluded,
    /// One storage class is listed twice on the same side.
    RepeatedStorageClass,
    /// One storage class is both counted and left out.
    ContradictoryStorageClass,
}

impl MemoryObservationDomain {
    /// Check that the domain says what it counts without contradicting itself.
    ///
    /// # Errors
    ///
    /// Returns the first failure, in the order the variants are declared.
    pub fn check(&self) -> Result<(), DomainFailure> {
        let StorageClasses { included, excluded } = &self.storage;
        if included.is_empty() {
            return Err(DomainFailure::NothingIncluded);
        }
        let counted = included.iter().collect::<BTreeSet<_>>();
        let left_out = excluded.iter().collect::<BTreeSet<_>>();
        if counted.len() != included.len() || left_out.len() != excluded.len() {
            return Err(DomainFailure::RepeatedStorageClass);
        }
        if !counted.is_disjoint(&left_out) {
            return Err(DomainFailure::ContradictoryStorageClass);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tokens(values: &[&'static str]) -> Vec<Token> {
        values.iter().copied().map(Token::literal).collect()
    }

    fn domain() -> MemoryObservationDomain {
        MemoryObservationDomain {
            identity: VersionedIdentity::literal("intlify-measurement-test-memory", "0"),
            observer: Token::literal("test-allocator"),
            storage: StorageClasses {
                included: tokens(&["invocation-scratch", "immutable-artifact"]),
                excluded: tokens(&["caller-acquired-inputs"]),
            },
            components: Components {
                harness: Inclusion::Excluded,
                language_runtime: Inclusion::Included,
                jit: Inclusion::NotApplicable,
                shared_libraries: Inclusion::Excluded,
                locale_data: Inclusion::Excluded,
            },
            coverage: Coverage {
                process: Token::literal("calling-process"),
                thread: Token::literal("calling-thread-only"),
                workers: Inclusion::NotApplicable,
                child_processes: Inclusion::NotApplicable,
            },
            reuse: Reuse {
                ownership: Token::literal("allocated-in-interval"),
                state: Token::literal("fresh"),
            },
        }
    }

    #[test]
    fn a_domain_spells_each_of_026s_six_parts() {
        assert_eq!(
            serde_json::to_value(domain()).unwrap(),
            json!({
                "identity": {"identity": "intlify-measurement-test-memory", "revision": "0"},
                "observer": "test-allocator",
                "storage": {
                    "included": ["invocation-scratch", "immutable-artifact"],
                    "excluded": ["caller-acquired-inputs"]
                },
                "components": {
                    "harness": "excluded",
                    "languageRuntime": "included",
                    "jit": "not-applicable",
                    "sharedLibraries": "excluded",
                    "localeData": "excluded"
                },
                "coverage": {
                    "process": "calling-process",
                    "thread": "calling-thread-only",
                    "workers": "not-applicable",
                    "childProcesses": "not-applicable"
                },
                "reuse": {"ownership": "allocated-in-interval", "state": "fresh"}
            })
        );
    }

    #[test]
    fn every_component_is_placed_rather_than_left_to_a_default() {
        let mut value = serde_json::to_value(domain()).unwrap();
        assert!(serde_json::from_value::<MemoryObservationDomain>(value.clone()).is_ok());
        for part in [
            "harness",
            "languageRuntime",
            "jit",
            "sharedLibraries",
            "localeData",
        ] {
            let mut missing = value.clone();
            missing["components"].as_object_mut().unwrap().remove(part);
            assert!(
                serde_json::from_value::<MemoryObservationDomain>(missing).is_err(),
                "{part}"
            );
        }
        value["components"]["harness"] = json!("partly");
        assert!(serde_json::from_value::<MemoryObservationDomain>(value).is_err());
    }

    #[test]
    fn a_domain_that_counts_something_without_contradiction_passes() {
        assert_eq!(domain().check(), Ok(()));
        let mut nothing_left_out = domain();
        nothing_left_out.storage.excluded.clear();
        assert_eq!(nothing_left_out.check(), Ok(()));
    }

    #[test]
    fn a_domain_that_counts_nothing_says_nothing() {
        let mut empty = domain();
        empty.storage.included.clear();
        assert_eq!(empty.check(), Err(DomainFailure::NothingIncluded));
    }

    #[test]
    fn a_storage_class_is_listed_once_on_one_side() {
        let mut repeated = domain();
        repeated
            .storage
            .included
            .push(Token::literal("invocation-scratch"));
        assert_eq!(repeated.check(), Err(DomainFailure::RepeatedStorageClass));
        let mut repeated = domain();
        repeated
            .storage
            .excluded
            .push(Token::literal("caller-acquired-inputs"));
        assert_eq!(repeated.check(), Err(DomainFailure::RepeatedStorageClass));
        let mut both = domain();
        both.storage
            .excluded
            .push(Token::literal("immutable-artifact"));
        assert_eq!(both.check(), Err(DomainFailure::ContradictoryStorageClass));
    }

    #[test]
    fn the_failures_are_reported_in_their_declared_order() {
        // Repeated on one side and contradictory across the sides: the
        // repetition is reported, because it is checked first.
        let mut both = domain();
        both.storage
            .included
            .push(Token::literal("immutable-artifact"));
        both.storage
            .excluded
            .push(Token::literal("immutable-artifact"));
        assert_eq!(both.check(), Err(DomainFailure::RepeatedStorageClass));
    }
}
