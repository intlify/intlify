// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Shared, language-neutral contracts for intlify message artifacts and linking.
//!
//! The crate root owns the public contract facade. It re-exports checked
//! identities, catalog-key selectors, portable source evidence, construction
//! errors, and immutable resource limits while keeping their implementation
//! modules private.
//!
//! This crate defines values exchanged across artifact producers, codecs, and
//! linker consumers. It intentionally does not perform resource extraction, MF2
//! or JavaScript parsing, CLI orchestration, linking, or exporting.

mod error;
mod fingerprint;
mod identity;
mod key;
mod limits;
mod locale;
mod path;
mod source;

pub use error::{
    ValueConstructionError, ValueGrammar, ValueLimit, ValueLimitKind, ValueRangeError,
};
pub use identity::{
    ArtifactNamespace, ArtifactVersion, CatalogScopeId, CatalogScopeName, DeliveryUnitId,
    DeliveryUnitSegment, ProducerId, ProducerIdentity, ProducerRevision, ReferenceArtifactIdentity,
    ReferenceArtifactSegment,
};
pub use key::{
    CatalogKey, CatalogKeyDomain, CatalogKeyPattern, CatalogKeyPrefix, CatalogKeyToken,
    MessageSelector, PatternToken,
};
pub use limits::{
    ArtifactLimitEvidence, LinkLimitConfigurationError, LinkLimitCounter, LinkLimitEvidence,
    LinkLimitEvidenceConstructionError, LinkLimitObservation, LinkLimitSubject, LinkLimits,
};
pub use locale::Locale;
pub use path::{PortablePathSegment, PortableRelativePath, SourceDocumentIdentity};
pub use source::{
    EntryReference, EntryStructuralPath, MessagePayload, ReasonText, SourceOrigin, SourceUtf8Span,
};

#[cfg(test)]
mod tests {
    use super::{
        ArtifactLimitEvidence, ArtifactNamespace, ArtifactVersion, CatalogKey, CatalogKeyDomain,
        CatalogKeyPattern, CatalogKeyPrefix, CatalogKeyToken, CatalogScopeId, CatalogScopeName,
        DeliveryUnitId, DeliveryUnitSegment, EntryReference, EntryStructuralPath, LinkLimitCounter,
        LinkLimitEvidence, LinkLimitObservation, LinkLimitSubject, LinkLimits, Locale,
        MessagePayload, MessageSelector, PatternToken, PortablePathSegment, PortableRelativePath,
        ProducerId, ProducerIdentity, ProducerRevision, ReasonText, ReferenceArtifactIdentity,
        ReferenceArtifactSegment, SourceDocumentIdentity, SourceOrigin, SourceUtf8Span,
    };

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn public_core_values_are_send_and_sync() {
        assert_send_sync::<ArtifactVersion>();
        assert_send_sync::<ArtifactNamespace>();
        assert_send_sync::<ProducerId>();
        assert_send_sync::<ProducerRevision>();
        assert_send_sync::<ProducerIdentity>();
        assert_send_sync::<CatalogScopeName>();
        assert_send_sync::<CatalogScopeId>();
        assert_send_sync::<Locale>();
        assert_send_sync::<ReferenceArtifactSegment>();
        assert_send_sync::<ReferenceArtifactIdentity>();
        assert_send_sync::<DeliveryUnitSegment>();
        assert_send_sync::<DeliveryUnitId>();
        assert_send_sync::<PortablePathSegment>();
        assert_send_sync::<PortableRelativePath>();
        assert_send_sync::<SourceDocumentIdentity>();
        assert_send_sync::<SourceUtf8Span>();
        assert_send_sync::<SourceOrigin>();
        assert_send_sync::<EntryStructuralPath>();
        assert_send_sync::<EntryReference>();
        assert_send_sync::<MessagePayload>();
        assert_send_sync::<ReasonText>();
        assert_send_sync::<CatalogKeyDomain>();
        assert_send_sync::<CatalogKeyToken>();
        assert_send_sync::<CatalogKey>();
        assert_send_sync::<CatalogKeyPrefix>();
        assert_send_sync::<PatternToken>();
        assert_send_sync::<CatalogKeyPattern>();
        assert_send_sync::<MessageSelector>();
        assert_send_sync::<LinkLimitCounter>();
        assert_send_sync::<LinkLimitObservation>();
        assert_send_sync::<LinkLimitSubject>();
        assert_send_sync::<ArtifactLimitEvidence>();
        assert_send_sync::<LinkLimitEvidence>();
        assert_send_sync::<LinkLimits>();
    }
}
