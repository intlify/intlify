// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Consumer-neutral host resource extraction and validated write-back support.
//!
//! This crate owns resource configuration and membership resolution,
//! host-document identity, spans, resource limits, offset maps, and adapter
//! artifacts. It intentionally does not depend on the MF2 parser, formatter,
//! linter, or CLI: consumers compose extracted message entries with those
//! message-level cores.

mod adapter;
mod artifact;
mod binding;
mod config;
mod error;
mod glob;
mod identity;
mod limits;
mod offset_map;
mod registry;
mod span;

pub use artifact::{
    CandidateMessageAdmission, ExtractedCatalog, FormattedEntry, MessageEntry, RawReplacement,
    ValidatedWriteBack, WriteBackOutcome,
};
#[cfg(feature = "benchmark")]
#[doc(hidden)]
pub use artifact::{ProfiledWriteBack, WriteBackBenchmarkProfile};
pub use binding::{
    CatalogBindingValueError, CatalogScopeName, InvalidLocaleCapturePattern, LocaleBindingConfig,
    LocaleCaptureError, LocaleCapturePattern, ResolvedLocale,
};
pub use config::{
    CatalogAssignmentConflict, CatalogAssignmentOrigin, CatalogBindingConflict,
    CatalogBindingConflictField, CatalogConfig, CatalogDefinitionRef, CatalogLocaleAssignmentError,
    CatalogLocaleNotProduction, CatalogOverlayConfig, CatalogPolicyState, CatalogResolution,
    LayeredCatalogMatch, LayeredCatalogResolution, LinkerCatalogAssignmentError,
    LinkerCatalogResolution, ProjectRelativeResourcePath, ProjectRelativeResourcePathError,
    ResolvedCatalogBinding, ResolvedCatalogDefinition, ResolvedCatalogOverlay,
    ResolvedLinkerCatalogAssignment, ResolvedResources, ResourceConfigReason,
    ResourceConfigViolation, ResourcesConfig,
};
pub use error::{
    DeclaredFormat, DocumentUnsupportedFeature, EntryUnsupportedReason, FormatClassificationSource,
    InternalResourceErrorReason, ResourceError, ResourceErrorCode, ResourceErrorDetails,
    ResourceErrorSite, ResourcePhase,
};
pub use glob::{InvalidResourceGlob, ResourceGlob};
pub use identity::{CatalogKey, CatalogKeyDomain, EntryHandle, EntryKey, StructuralPathKey};
pub use limits::{
    preflight_host_bytes, ResourceLimit, MAX_ENTRIES, MAX_HOST_BYTES, MAX_IDENTITY_BYTES,
    MAX_MESSAGE_BYTES, MAX_NESTING_DEPTH, MAX_OFFSET_MAP_SEGMENTS, MAX_TOTAL_MESSAGE_BYTES,
};
pub use offset_map::{MessageOffsetMap, OffsetMapError};
pub use registry::{
    HostFormat, HostFormatClassification, HostFormatRegistry, KnownHostFormatId,
    ResolvedCatalogAssignment, ResolvedHostFormat,
};
pub use span::Utf8ByteSpan;

#[cfg(test)]
mod tests {
    use super::{
        CatalogKey, CatalogOverlayConfig, CatalogScopeName, EntryHandle, EntryKey,
        ExtractedCatalog, HostFormatRegistry, LocaleBindingConfig, LocaleCapturePattern,
        MessageOffsetMap, ResolvedCatalogBinding, ResolvedCatalogDefinition,
        ResolvedCatalogOverlay, ResolvedHostFormat, ResolvedLinkerCatalogAssignment,
        ResolvedLocale, ResolvedResources, ResourceError, ResourceErrorSite, ResourcesConfig,
        StructuralPathKey, Utf8ByteSpan, ValidatedWriteBack, WriteBackOutcome,
    };

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn public_core_values_are_send_and_sync() {
        assert_send_sync::<Utf8ByteSpan>();
        assert_send_sync::<StructuralPathKey>();
        assert_send_sync::<CatalogKey>();
        assert_send_sync::<EntryKey>();
        assert_send_sync::<EntryHandle>();
        assert_send_sync::<MessageOffsetMap>();
        assert_send_sync::<ResourceErrorSite>();
        assert_send_sync::<ResourceError>();
        assert_send_sync::<HostFormatRegistry>();
        assert_send_sync::<ResolvedHostFormat>();
        assert_send_sync::<ExtractedCatalog>();
        assert_send_sync::<ValidatedWriteBack>();
        assert_send_sync::<WriteBackOutcome>();
        assert_send_sync::<ResourcesConfig>();
        assert_send_sync::<ResolvedResources>();
        assert_send_sync::<CatalogOverlayConfig>();
        assert_send_sync::<ResolvedCatalogOverlay>();
        assert_send_sync::<CatalogScopeName>();
        assert_send_sync::<ResolvedLocale>();
        assert_send_sync::<LocaleCapturePattern>();
        assert_send_sync::<LocaleBindingConfig>();
        assert_send_sync::<ResolvedCatalogBinding>();
        assert_send_sync::<ResolvedCatalogDefinition>();
        assert_send_sync::<ResolvedLinkerCatalogAssignment>();
    }
}
