// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Stateless, language-neutral message-link request validation.
//!
//! This crate owns checked link policy, scope mapping and completeness, delivery
//! graphs, and one immutable request boundary over `intlify_contract` artifacts.
//! It intentionally performs no filesystem I/O, resource or source-language
//! parsing, CLI orchestration, exporting, or process-global scheduling.

mod error;
mod graph;
mod policy;
mod request;
mod scope;
mod validation;

pub use error::{
    ArtifactContractSubject, ArtifactKind, ConfiguredRootIdentity, DeliveryEdgeEndpoint,
    InvalidRequestError, LinkOperationalError, ScopeEndpoint, ScopeUse, UnsupportedContractError,
};
pub use graph::{DeliveryUnitEdge, DeliveryUnitGraph};
pub use policy::{
    ConfiguredRoot, ConfiguredRootConstructionError, DynamicReferenceMode, LinkPolicy,
    PlacementPolicy, ResolvedConfiguredRoot, ResolvedLinkPolicy,
};
pub use request::LinkRequest;
pub use scope::{
    CompletenessContributor, CompletenessSide, InputCompleteness, PartialReason,
    ResolvedCatalogScopeId, ResolvedInputCompleteness, ResolvedScopeCompleteness,
    ResolvedScopeCompletenessTable, ScopeCompleteness, ScopeCompletenessConstructionError,
    ScopeCompletenessTable, ScopeMapping, ScopeMappingTable,
};

#[cfg(test)]
mod tests {
    use super::{
        DeliveryUnitGraph, LinkOperationalError, LinkPolicy, LinkRequest,
        ResolvedScopeCompletenessTable, ScopeCompletenessTable, ScopeMappingTable,
    };

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn public_request_foundation_values_are_send_and_sync() {
        assert_send_sync::<LinkPolicy>();
        assert_send_sync::<ScopeMappingTable>();
        assert_send_sync::<ScopeCompletenessTable>();
        assert_send_sync::<ResolvedScopeCompletenessTable>();
        assert_send_sync::<DeliveryUnitGraph>();
        assert_send_sync::<LinkRequest<'static>>();
        assert_send_sync::<LinkOperationalError>();
    }
}
