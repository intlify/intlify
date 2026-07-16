// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::error::Error;
use std::fmt;
use std::sync::Arc;

use crate::{EntryKey, ResourceLimit, Utf8ByteSpan};

/// Resource pipeline phase attached to limits and internal failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourcePhase {
    /// Host-format resolution and registry dispatch.
    Registry,
    /// Original host extraction.
    Extract,
    /// Consumer mapping through a published offset map.
    Map,
    /// Re-escaping and complete candidate validation.
    ValidateWriteBack,
}

impl ResourcePhase {
    /// Return the stable machine-readable phase spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Registry => "registry",
            Self::Extract => "extract",
            Self::Map => "map",
            Self::ValidateWriteBack => "validate_write_back",
        }
    }
}

/// Stable resource operational-error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceErrorCode {
    /// An opted-in target cannot resolve to a shipped adapter.
    FormatUnsupported,
    /// A selected host adapter rejected the complete host syntax.
    ParseFailed,
    /// One candidate cannot satisfy message-entry representability.
    EntryUnsupported,
    /// A valid host document uses an unsupported document-level feature.
    DocumentUnsupported,
    /// A fixed resource representation limit was exceeded.
    LimitExceeded,
    /// A supposedly valid resource-layer invariant failed.
    Internal,
}

impl ResourceErrorCode {
    /// Return the stable shared operational-error code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FormatUnsupported => "resource_format_unsupported",
            Self::ParseFailed => "resource_parse_failed",
            Self::EntryUnsupported => "resource_entry_unsupported",
            Self::DocumentUnsupported => "resource_document_unsupported",
            Self::LimitExceeded => "resource_limit_exceeded",
            Self::Internal => "internal_error",
        }
    }
}

/// Source of a host-format classification decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormatClassificationSource {
    /// Top-level target extension or project assignment.
    Extension,
    /// A declaration inside an already selected composed host format.
    Embedded,
}

impl FormatClassificationSource {
    /// Return the stable CLI detail spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Extension => "extension",
            Self::Embedded => "embedded",
        }
    }
}

/// Presence-aware embedded format declaration evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeclaredFormat {
    /// No embedded declaration exists.
    Absent,
    /// A declaration exists without a value.
    Valueless,
    /// The exact decoded declaration value, without normalization.
    Value(Arc<str>),
}

/// Stable entry-representability reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryUnsupportedReason {
    /// Candidate message text cannot become a Unicode-scalar Rust string.
    MessageTextUnrepresentable,
    /// A structural path cannot be represented by the adapter contract.
    StructuralPathUnsupported,
    /// A semantic container contains unsupported inline structure.
    InlineContentUnsupported,
}

impl EntryUnsupportedReason {
    /// Return the stable CLI detail spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MessageTextUnrepresentable => "message_text_unrepresentable",
            Self::StructuralPathUnsupported => "structural_path_unsupported",
            Self::InlineContentUnsupported => "inline_content_unsupported",
        }
    }
}

/// Stable document-level unsupported feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocumentUnsupportedFeature {
    /// A Vue `src` block also has inline content.
    VueSrcWithInlineContent,
    /// A YAML stream contains more than one document.
    MultipleDocuments,
    /// A YAML version is outside the supported profile.
    YamlVersion,
    /// A non-Core YAML tag is used.
    CustomTags,
    /// A YAML alias is used.
    Aliases,
    /// A YAML merge key is used.
    MergeKeys,
    /// An unsupported XLIFF profile is selected.
    XliffVersion,
    /// XML contains a document type declaration.
    XmlDtd,
    /// XML declares an unsupported version.
    XmlVersion,
    /// XML declares an unsupported encoding.
    XmlEncoding,
}

impl DocumentUnsupportedFeature {
    /// Return the stable CLI detail spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VueSrcWithInlineContent => "vue_src_with_inline_content",
            Self::MultipleDocuments => "multiple_documents",
            Self::YamlVersion => "yaml_version",
            Self::CustomTags => "custom_tags",
            Self::Aliases => "aliases",
            Self::MergeKeys => "merge_keys",
            Self::XliffVersion => "xliff_version",
            Self::XmlDtd => "xml_dtd",
            Self::XmlVersion => "xml_version",
            Self::XmlEncoding => "xml_encoding",
        }
    }
}

/// Stable internal resource-layer invariant reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InternalResourceErrorReason {
    /// Candidate admission or write-back received an operation-invalid handle.
    InvalidEntryHandle,
    /// No unused nonzero process artifact identity remains.
    ArtifactIdentityExhausted,
    /// An adapter attempted to publish an invalid offset map.
    OffsetMapInvariantFailed,
    /// A consumer span could not be mapped through a published map.
    OffsetMapFailed,
    /// Re-escaping or candidate equivalence validation failed.
    WriteBackFailed,
    /// Another built-in adapter or registry invariant failed.
    AdapterInvariantFailed,
}

impl InternalResourceErrorReason {
    /// Return the stable CLI detail spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidEntryHandle => "resource_invalid_entry_handle",
            Self::ArtifactIdentityExhausted => "resource_artifact_identity_exhausted",
            Self::OffsetMapInvariantFailed => "resource_offset_map_invariant_failed",
            Self::OffsetMapFailed => "resource_offset_map_failed",
            Self::WriteBackFailed => "resource_write_back_failed",
            Self::AdapterInvariantFailed => "resource_adapter_invariant_failed",
        }
    }
}

/// Typed, CLI-neutral evidence associated with a resource failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceErrorDetails {
    /// Unsupported top-level or embedded format evidence.
    FormatUnsupported {
        /// How the format was classified.
        classification_source: FormatClassificationSource,
        /// Presence-aware embedded declaration evidence.
        declared_format: DeclaredFormat,
        /// Normalized known id, when recognized.
        format: Option<&'static str>,
        /// Exact retained top-level extension spelling.
        extension: Arc<str>,
        /// Already selected composed outer format, when applicable.
        outer_format: Option<&'static str>,
        /// ASCII-ordered shipped ids accepted at this boundary.
        supported_formats: Arc<[&'static str]>,
    },
    /// Complete host syntax rejection evidence.
    ParseFailed {
        /// Adapter layer that rejected the source.
        format: &'static str,
        /// Composed outer adapter, when applicable.
        outer_format: Option<&'static str>,
    },
    /// Candidate entry representability evidence.
    EntryUnsupported {
        /// Adapter layer that rejected the candidate.
        format: &'static str,
        /// Composed outer adapter, when applicable.
        outer_format: Option<&'static str>,
        /// Stable representability reason.
        reason: EntryUnsupportedReason,
    },
    /// Document-level unsupported feature evidence.
    DocumentUnsupported {
        /// Adapter layer that rejected the construct.
        format: &'static str,
        /// Composed outer adapter, when applicable.
        outer_format: Option<&'static str>,
        /// Stable unsupported feature.
        feature: DocumentUnsupportedFeature,
    },
    /// Fixed resource limit evidence.
    LimitExceeded {
        /// Resource representation that crossed its inclusive maximum.
        resource: ResourceLimit,
        /// Fixed inclusive maximum.
        limit: u128,
        /// Exact first observed value over the limit.
        actual: u128,
    },
    /// Resource-layer invariant evidence.
    Internal {
        /// Stable implementation reason.
        reason: InternalResourceErrorReason,
    },
}

/// Optional primary site in the caller-owned original host document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceErrorSite {
    span: Utf8ByteSpan,
    entry_key: Option<EntryKey>,
}

impl ResourceErrorSite {
    #[allow(dead_code)]
    pub(crate) const fn new(span: Utf8ByteSpan, entry_key: Option<EntryKey>) -> Self {
        Self { span, entry_key }
    }

    /// Return the complete original-host evidence span.
    #[must_use]
    pub const fn span(&self) -> Utf8ByteSpan {
        self.span
    }

    /// Return the containing constructible entry identity, when known.
    #[must_use]
    pub const fn entry_key(&self) -> Option<&EntryKey> {
        self.entry_key.as_ref()
    }
}

/// CLI-neutral resource operation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceError {
    code: ResourceErrorCode,
    phase: ResourcePhase,
    details: Box<ResourceErrorDetails>,
    site: Option<ResourceErrorSite>,
}

impl ResourceError {
    pub(crate) fn limit_exceeded(
        resource: ResourceLimit,
        actual: u128,
        phase: ResourcePhase,
        site: Option<ResourceErrorSite>,
    ) -> Self {
        Self {
            code: ResourceErrorCode::LimitExceeded,
            phase,
            details: Box::new(ResourceErrorDetails::LimitExceeded {
                resource,
                limit: resource.limit(),
                actual,
            }),
            site,
        }
    }

    pub(crate) fn internal(
        reason: InternalResourceErrorReason,
        phase: ResourcePhase,
        site: Option<ResourceErrorSite>,
    ) -> Self {
        Self {
            code: ResourceErrorCode::Internal,
            phase,
            details: Box::new(ResourceErrorDetails::Internal { reason }),
            site,
        }
    }

    /// Return the stable operational-error code.
    #[must_use]
    pub const fn code(&self) -> ResourceErrorCode {
        self.code
    }

    /// Return the pipeline phase that selected the failure.
    #[must_use]
    pub const fn phase(&self) -> ResourcePhase {
        self.phase
    }

    /// Return typed detail evidence.
    #[must_use]
    pub fn details(&self) -> &ResourceErrorDetails {
        self.details.as_ref()
    }

    /// Return the original-host site, when one exists.
    #[must_use]
    pub const fn site(&self) -> Option<&ResourceErrorSite> {
        self.site.as_ref()
    }
}

impl fmt::Display for ResourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())
    }
}

impl Error for ResourceError {}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{
        DocumentUnsupportedFeature, EntryUnsupportedReason, InternalResourceErrorReason,
        ResourceError, ResourceErrorCode, ResourceErrorDetails, ResourceErrorSite, ResourcePhase,
    };
    use crate::{EntryKey, ResourceLimit, StructuralPathKey, Utf8ByteSpan};

    #[test]
    fn stable_vocabulary_matches_the_design() {
        assert_eq!(
            ResourceErrorCode::FormatUnsupported.as_str(),
            "resource_format_unsupported"
        );
        assert_eq!(
            ResourceErrorCode::ParseFailed.as_str(),
            "resource_parse_failed"
        );
        assert_eq!(
            ResourceErrorCode::EntryUnsupported.as_str(),
            "resource_entry_unsupported"
        );
        assert_eq!(
            ResourceErrorCode::DocumentUnsupported.as_str(),
            "resource_document_unsupported"
        );
        assert_eq!(
            ResourceErrorCode::LimitExceeded.as_str(),
            "resource_limit_exceeded"
        );
        assert_eq!(ResourceErrorCode::Internal.as_str(), "internal_error");

        assert_eq!(ResourcePhase::Registry.as_str(), "registry");
        assert_eq!(ResourcePhase::Extract.as_str(), "extract");
        assert_eq!(ResourcePhase::Map.as_str(), "map");
        assert_eq!(
            ResourcePhase::ValidateWriteBack.as_str(),
            "validate_write_back"
        );

        assert_eq!(
            EntryUnsupportedReason::MessageTextUnrepresentable.as_str(),
            "message_text_unrepresentable"
        );
        assert_eq!(
            EntryUnsupportedReason::StructuralPathUnsupported.as_str(),
            "structural_path_unsupported"
        );
        assert_eq!(
            EntryUnsupportedReason::InlineContentUnsupported.as_str(),
            "inline_content_unsupported"
        );

        let features = [
            (
                DocumentUnsupportedFeature::VueSrcWithInlineContent,
                "vue_src_with_inline_content",
            ),
            (
                DocumentUnsupportedFeature::MultipleDocuments,
                "multiple_documents",
            ),
            (DocumentUnsupportedFeature::YamlVersion, "yaml_version"),
            (DocumentUnsupportedFeature::CustomTags, "custom_tags"),
            (DocumentUnsupportedFeature::Aliases, "aliases"),
            (DocumentUnsupportedFeature::MergeKeys, "merge_keys"),
            (DocumentUnsupportedFeature::XliffVersion, "xliff_version"),
            (DocumentUnsupportedFeature::XmlDtd, "xml_dtd"),
            (DocumentUnsupportedFeature::XmlVersion, "xml_version"),
            (DocumentUnsupportedFeature::XmlEncoding, "xml_encoding"),
        ];
        for (feature, expected) in features {
            assert_eq!(feature.as_str(), expected);
        }

        let reasons = [
            (
                InternalResourceErrorReason::InvalidEntryHandle,
                "resource_invalid_entry_handle",
            ),
            (
                InternalResourceErrorReason::ArtifactIdentityExhausted,
                "resource_artifact_identity_exhausted",
            ),
            (
                InternalResourceErrorReason::OffsetMapInvariantFailed,
                "resource_offset_map_invariant_failed",
            ),
            (
                InternalResourceErrorReason::OffsetMapFailed,
                "resource_offset_map_failed",
            ),
            (
                InternalResourceErrorReason::WriteBackFailed,
                "resource_write_back_failed",
            ),
            (
                InternalResourceErrorReason::AdapterInvariantFailed,
                "resource_adapter_invariant_failed",
            ),
        ];
        for (reason, expected) in reasons {
            assert_eq!(reason.as_str(), expected);
        }
    }

    #[test]
    fn retains_optional_original_host_site_and_entry_key() {
        let key = EntryKey::new(StructuralPathKey::from_shared(Arc::from("/greeting")), 0);
        let site = ResourceErrorSite::new(Utf8ByteSpan::new(12, 24), Some(key.clone()));
        let error = ResourceError::limit_exceeded(
            ResourceLimit::MessageBytes,
            1_048_577,
            ResourcePhase::Extract,
            Some(site),
        );

        assert_eq!(error.site().unwrap().span(), Utf8ByteSpan::new(12, 24));
        assert_eq!(error.site().unwrap().entry_key(), Some(&key));
    }

    #[test]
    fn document_wide_error_omits_site_and_entry_key() {
        let error = ResourceError::limit_exceeded(
            ResourceLimit::HostBytes,
            67_108_865,
            ResourcePhase::Extract,
            None,
        );

        assert_eq!(error.code(), ResourceErrorCode::LimitExceeded);
        assert_eq!(error.phase(), ResourcePhase::Extract);
        assert!(error.site().is_none());
    }

    #[test]
    fn typed_evidence_does_not_retain_source_or_dependency_debug_text() {
        let error = ResourceError::internal(
            InternalResourceErrorReason::AdapterInvariantFailed,
            ResourcePhase::Extract,
            None,
        );
        let debug = format!("{error:?}");

        assert!(!debug.contains("secret source"));
        assert!(!debug.contains("raw replacement"));
        assert!(!debug.contains("dependency error"));
        assert!(matches!(
            error.details(),
            ResourceErrorDetails::Internal {
                reason: InternalResourceErrorReason::AdapterInvariantFailed
            }
        ));
    }
}
