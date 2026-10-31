// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's two Memory Observation Domains, both at revision `"0"`.
//!
//! Each observes, on the calling thread only, the maximum net growth of the
//! bytes requested through the global allocator, by allocation `Layout`,
//! between the interval's start and end marks, relative to the start. The
//! inputs the caller acquired before the interval, the storage a resolver held
//! before an invocation interval, and the bytes of locale data supplied as
//! inputs are left out; the structures the interval builds, including those it
//! builds from locale data, are counted. Allocator metadata, resident set size,
//! static storage, other threads, and allocations that bypass the global
//! allocator are left out too.
//!
//! Each domain states the reuse state it observes: a freshly constructed or a
//! reused resolver. 015 requires it to agree with the record's Execution State,
//! so it is spelled as that state's `engineState` is.

use intlify_measurement::execution::Execution;
use intlify_measurement::memory::{
    Components, Coverage, Inclusion, MemoryObservationDomain, Reuse, StorageClasses,
};

use super::cases::ResolverState;
use super::shared::identity::{Token, VersionedIdentity};

/// The domain of a fresh resolver construction.
pub(super) const CONSTRUCTION: &str = "profile-resolver-construction";

/// The domain of one complete invocation.
pub(super) const INVOCATION: &str = "profile-resolver-invocation";

const REVISION: &str = "0";

fn tokens(values: &[&'static str]) -> Vec<Token> {
    values.iter().copied().map(Token::literal).collect()
}

fn domain(
    identity: &'static str,
    excluded: &[&'static str],
    state: &'static str,
) -> MemoryObservationDomain {
    MemoryObservationDomain {
        identity: VersionedIdentity::literal(identity, REVISION),
        observer: Token::literal("global-allocator-layout-requests"),
        storage: StorageClasses {
            included: tokens(&["structures-built-in-interval"]),
            excluded: tokens(excluded),
        },
        components: Components {
            // The marks enclose the production boundary alone.
            harness: Inclusion::Excluded,
            // What the standard library allocates for the work, through the
            // global allocator, is part of the work.
            language_runtime: Inclusion::Included,
            jit: Inclusion::NotApplicable,
            // Their own allocations bypass the global allocator.
            shared_libraries: Inclusion::Excluded,
            // The bytes supplied as inputs; what is built from them is counted
            // as a structure built in the interval.
            locale_data: Inclusion::Excluded,
        },
        coverage: Coverage {
            process: Token::literal("calling-process"),
            thread: Token::literal("calling-thread-only"),
            // Resolution is synchronous: it starts no worker and no process.
            workers: Inclusion::NotApplicable,
            child_processes: Inclusion::NotApplicable,
        },
        reuse: Reuse {
            ownership: Token::literal("requested-in-interval"),
            state: Token::literal(state),
        },
    }
}

/// The domain of constructing one resolver, which is always a fresh one.
pub(super) fn construction() -> MemoryObservationDomain {
    domain(
        CONSTRUCTION,
        &[
            "caller-acquired-inputs",
            "locale-data-supplied-as-inputs",
            "allocator-metadata",
            "resident-set-size",
            "static-storage",
            "allocations-bypassing-global-allocator",
        ],
        ResolverState::Fresh.engine_state(),
    )
}

/// The domain of one complete invocation of a resolver in the given state.
pub(super) fn invocation(resolver: ResolverState) -> MemoryObservationDomain {
    domain(
        INVOCATION,
        &[
            "caller-acquired-inputs",
            "resolver-storage-held-before-interval",
            "locale-data-supplied-as-inputs",
            "allocator-metadata",
            "resident-set-size",
            "static-storage",
            "allocations-bypassing-global-allocator",
        ],
        resolver.engine_state(),
    )
}

/// Whether a domain's reuse state is the engine state the case ran in.
pub(super) fn agrees(domain: &MemoryObservationDomain, execution: &Execution) -> bool {
    domain.reuse.state.as_str() == execution.engine_state
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::descriptor::{prepared_core, OutputBuffer};
    use crate::benchmark::operation::Operation;

    fn execution(engine_state: &str) -> Execution {
        Execution {
            engine_state: engine_state.into(),
            ..prepared_core(Operation::StructuralAnalysis, None)
        }
    }

    #[test]
    fn the_two_domains_are_015s_at_revision_zero() {
        assert_eq!(
            construction().identity,
            VersionedIdentity::literal("profile-resolver-construction", "0")
        );
        for resolver in [
            ResolverState::Fresh,
            ResolverState::Reused,
            ResolverState::ReusedAfterBlocked,
        ] {
            assert_eq!(
                invocation(resolver).identity,
                VersionedIdentity::literal("profile-resolver-invocation", "0")
            );
        }
    }

    #[test]
    fn both_domains_say_what_they_count_without_contradiction() {
        assert_eq!(construction().check(), Ok(()));
        assert_eq!(invocation(ResolverState::Reused).check(), Ok(()));
    }

    #[test]
    fn only_an_invocation_leaves_out_what_the_resolver_held_before() {
        let held = Token::literal("resolver-storage-held-before-interval");
        assert!(!construction().storage.excluded.contains(&held));
        assert!(invocation(ResolverState::Fresh)
            .storage
            .excluded
            .contains(&held));
        // Apart from that and their names, the two domains observe alike.
        let mut invocation = invocation(ResolverState::Fresh);
        invocation.identity = construction().identity;
        invocation.storage.excluded.retain(|class| *class != held);
        assert_eq!(invocation, construction());
    }

    #[test]
    fn each_domain_states_the_reuse_state_it_observes() {
        assert_eq!(construction().reuse.state.as_str(), "fresh");
        assert_eq!(
            invocation(ResolverState::Fresh).reuse.state.as_str(),
            "fresh"
        );
        assert_eq!(
            invocation(ResolverState::Reused).reuse.state.as_str(),
            "reused"
        );
        assert_eq!(
            invocation(ResolverState::ReusedAfterBlocked)
                .reuse
                .state
                .as_str(),
            "reused"
        );
        // A fresh and a reused invocation are different observations.
        assert_ne!(
            invocation(ResolverState::Fresh),
            invocation(ResolverState::Reused)
        );
    }

    #[test]
    fn a_domain_agrees_only_with_the_engine_state_it_states() {
        assert!(agrees(&construction(), &execution("fresh")));
        assert!(!agrees(&construction(), &execution("reused")));
        for (resolver, state, other) in [
            (ResolverState::Fresh, "fresh", "reused"),
            (ResolverState::Reused, "reused", "fresh"),
            (ResolverState::ReusedAfterBlocked, "reused", "fresh"),
        ] {
            assert!(agrees(&invocation(resolver), &execution(state)));
            assert!(!agrees(&invocation(resolver), &execution(other)));
        }
        // Only the engine state is read: another field changes nothing.
        let mut execution = execution("fresh");
        execution.output_buffer_state = OutputBuffer::NotApplicable {};
        assert!(agrees(&construction(), &execution));
    }
}
