// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Stateless, language-neutral message linking.
//!
//! This crate owns checked link policy, scope mapping and completeness, delivery
//! graphs, one immutable request boundary over `intlify_contract` artifacts,
//! deterministic selector resolution, findings, reachability, and bundle plans.
//! It intentionally performs no filesystem I/O, resource or source-language
//! parsing, CLI orchestration, exporting, or process-global scheduling.

mod error;
mod finding;
mod graph;
mod link;
mod location;
mod outcome;
mod plan;
mod policy;
mod request;
mod scope;
mod validation;

#[cfg(feature = "benchmark")]
#[doc(hidden)]
pub mod benchmark;

pub use error::{
    ArtifactContractSubject, ArtifactKind, ConfiguredRootIdentity, DeliveryEdgeEndpoint,
    InvalidRequestError, LinkOperationalError, ScopeEndpoint, ScopeUse, UnsupportedContractError,
};
pub use finding::{
    AmbiguousMessageDefinitionEvidence, AmbiguousMessageDefinitionFinding,
    AmbiguousMessageDefinitionSubject, DegradedAnalysisFinding, LinkFinding, LinkFindingKind,
    LinkFindingRecord, MissingTranslationEvidence, MissingTranslationFinding,
    MissingTranslationSubject, OrphanedTranslationEvidence, OrphanedTranslationFinding,
    OrphanedTranslationSubject, PartialCompletenessDegradation, PartialCompletenessEvidence,
    PartialCompletenessSubject, ResolutionFailure, UnboundedDynamicReferenceEvidence,
    UnboundedDynamicReferenceFinding, UnboundedDynamicReferenceSubject, UnresolvedMessageEvidence,
    UnresolvedMessageFinding, UnresolvedMessageSubject, UnusedMessageEvidence,
    UnusedMessageFinding, UnusedMessageSubject, WideSelectorDegradation, WideSelectorEvidence,
    WideSelectorSubject,
};
pub use graph::{DeliveryUnitEdge, DeliveryUnitGraph};
pub use link::link;
pub use location::DefinitionLocation;
pub use outcome::LinkOutcome;
pub use plan::{MessageBundlePlan, ResolvedMessage};
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
        DeliveryUnitGraph, LinkFinding, LinkFindingKind, LinkOperationalError, LinkOutcome,
        LinkPolicy, LinkRequest, MessageBundlePlan, ResolvedMessage,
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
        assert_send_sync::<LinkFinding>();
        assert_send_sync::<LinkOutcome>();
        assert_send_sync::<MessageBundlePlan>();
        assert_send_sync::<ResolvedMessage>();
    }

    #[test]
    fn finding_kind_precedence_is_explicit_and_contiguous() {
        let kinds = [
            LinkFindingKind::AmbiguousMessageDefinition,
            LinkFindingKind::UnresolvedMessage,
            LinkFindingKind::MissingTranslation,
            LinkFindingKind::OrphanedTranslation,
            LinkFindingKind::UnusedMessage,
            LinkFindingKind::UnboundedDynamicReference,
            LinkFindingKind::DegradedAnalysis,
        ];

        for (precedence, kind) in kinds.into_iter().enumerate() {
            assert_eq!(kind.precedence(), precedence as u8);
        }
    }
}
