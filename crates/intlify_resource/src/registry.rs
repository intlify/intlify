// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::fmt;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::Serialize;

use crate::adapter::{HostAdapter, JsonAdapter};
use crate::artifact::{extract_resolved, ExtractedCatalog};
use crate::config::CatalogDefinitionRef;
use crate::{DeclaredFormat, FormatClassificationSource, ResourceError, ResourcePhase};

const SUPPORTED_DIRECT_EXTENSIONS: &[&str] = &[".json"];
const SUPPORTED_FORMATS: &[&str] = &["json"];

/// Canonical format ids fixed by the resource design, including deferred tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KnownHostFormatId {
    /// RFC 8259 JSON catalogs.
    Json,
    /// Vue single-file components with inline `<i18n>` blocks.
    Vue,
    /// YAML 1.2 catalogs, including `.yaml` and `.yml` spellings.
    Yaml,
    /// JSON with comments and trailing commas.
    Jsonc,
    /// Standard JSON5 catalogs.
    Json5,
    /// XLIFF catalogs, including `.xlf` and `.xliff` spellings.
    Xliff,
}

impl KnownHostFormatId {
    /// Return the canonical lowercase registry id.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Vue => "vue",
            Self::Yaml => "yaml",
            Self::Jsonc => "jsonc",
            Self::Json5 => "json5",
            Self::Xliff => "xliff",
        }
    }

    pub(crate) const fn from_shipped(format: HostFormat) -> Self {
        match format {
            HostFormat::Json => Self::Json,
        }
    }
}

/// Host adapters shipped in this release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[schemars(rename_all = "lowercase")]
pub enum HostFormat {
    /// The built-in RFC 8259 JSON adapter.
    Json,
}

impl HostFormat {
    /// Return the canonical lowercase registry id.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
        }
    }
}

/// Extension or explicit-assignment classification before registry resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostFormatClassification {
    /// The adapter ships in this release.
    Shipped(HostFormat),
    /// The id is known but its adapter has not shipped.
    KnownButUnshipped(KnownHostFormatId),
    /// No known id is associated with the extension.
    UnrecognizedExtension,
}

impl HostFormatClassification {
    pub(crate) const fn known_id(self) -> Option<KnownHostFormatId> {
        match self {
            Self::Shipped(format) => Some(KnownHostFormatId::from_shipped(format)),
            Self::KnownButUnshipped(format) => Some(format),
            Self::UnrecognizedExtension => None,
        }
    }
}

/// Validated catalog-membership output consumed by the registry.
///
/// The resource resolver introduced in Milestone 9 is the only production
/// constructor. Keeping the fields private prevents consumers from bypassing
/// overlap and membership resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCatalogAssignment {
    classification: HostFormatClassification,
    retained_extension: Arc<str>,
    surviving_definitions: Arc<[CatalogDefinitionRef]>,
    assigning_definitions: Arc<[CatalogDefinitionRef]>,
}

impl ResolvedCatalogAssignment {
    pub(crate) fn new(
        classification: HostFormatClassification,
        retained_extension: Arc<str>,
        surviving_definitions: Arc<[CatalogDefinitionRef]>,
        assigning_definitions: Arc<[CatalogDefinitionRef]>,
    ) -> Self {
        Self {
            classification,
            retained_extension,
            surviving_definitions,
            assigning_definitions,
        }
    }

    /// Return the one resolved host-format classification.
    #[must_use]
    pub const fn classification(&self) -> HostFormatClassification {
        self.classification
    }

    /// Return the exact extension spelling retained from the logical path.
    #[must_use]
    pub fn retained_extension(&self) -> &str {
        &self.retained_extension
    }

    /// Return every definition that included and did not exclude the path.
    #[must_use]
    pub fn surviving_definitions(&self) -> &[CatalogDefinitionRef] {
        &self.surviving_definitions
    }

    /// Return the surviving definitions that contributed the resolved format.
    #[must_use]
    pub fn assigning_definitions(&self) -> &[CatalogDefinitionRef] {
        &self.assigning_definitions
    }

    fn retained_extension_arc(&self) -> &Arc<str> {
        &self.retained_extension
    }
}

/// Registry-issued shipped format context.
#[derive(Clone)]
pub struct ResolvedHostFormat {
    format: HostFormat,
    retained_extension: Arc<str>,
    adapter: Arc<dyn HostAdapter>,
}

impl ResolvedHostFormat {
    /// Return the shipped workflow classification.
    #[must_use]
    pub const fn format(&self) -> HostFormat {
        self.format
    }

    #[allow(dead_code)]
    pub(crate) fn retained_extension(&self) -> &Arc<str> {
        &self.retained_extension
    }

    pub(crate) fn adapter(&self) -> &Arc<dyn HostAdapter> {
        &self.adapter
    }
}

impl fmt::Debug for ResolvedHostFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResolvedHostFormat")
            .field("format", &self.format)
            .field("retained_extension", &self.retained_extension)
            .finish_non_exhaustive()
    }
}

/// Built-in, per-release host format registry.
#[derive(Clone)]
pub struct HostFormatRegistry {
    json: Arc<dyn HostAdapter>,
}

impl HostFormatRegistry {
    /// Construct the registry containing every adapter shipped in this release.
    #[must_use]
    pub fn new() -> Self {
        Self {
            json: Arc::new(JsonAdapter),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_json_adapter(adapter: Arc<dyn HostAdapter>) -> Self {
        Self { json: adapter }
    }

    /// Resolve one validated project catalog assignment.
    pub fn resolve_format(
        &self,
        assignment: &ResolvedCatalogAssignment,
    ) -> Result<ResolvedHostFormat, ResourceError> {
        match assignment.classification() {
            HostFormatClassification::Shipped(format) => {
                Ok(self.resolved(format, Arc::clone(assignment.retained_extension_arc())))
            }
            HostFormatClassification::KnownButUnshipped(format) => Err(self
                .unsupported_assignment_error(
                    Some(format.as_str()),
                    Arc::clone(assignment.retained_extension_arc()),
                )),
            HostFormatClassification::UnrecognizedExtension => Err(self
                .unsupported_assignment_error(
                    None,
                    Arc::clone(assignment.retained_extension_arc()),
                )),
        }
    }

    /// Resolve an already opted-in direct-file extension without sniffing.
    #[must_use]
    pub fn resolve_direct_extension(&self, extension: &str) -> Option<ResolvedHostFormat> {
        match classify_extension(extension) {
            HostFormatClassification::Shipped(format) => {
                Some(self.resolved(format, Arc::from(extension)))
            }
            HostFormatClassification::KnownButUnshipped(_)
            | HostFormatClassification::UnrecognizedExtension => None,
        }
    }

    /// Return canonical lowercase direct-file extensions in ASCII order.
    #[must_use]
    pub const fn supported_direct_extensions(&self) -> &'static [&'static str] {
        SUPPORTED_DIRECT_EXTENSIONS
    }

    /// Extract a complete immutable catalog artifact through the selected adapter.
    pub fn extract(
        &self,
        resolved: ResolvedHostFormat,
        source: Arc<str>,
    ) -> Result<ExtractedCatalog, ResourceError> {
        let _ = self;
        extract_resolved(resolved, source, ResourcePhase::Extract)
    }

    fn resolved(&self, format: HostFormat, retained_extension: Arc<str>) -> ResolvedHostFormat {
        let adapter = match format {
            HostFormat::Json => Arc::clone(&self.json),
        };
        ResolvedHostFormat {
            format,
            retained_extension,
            adapter,
        }
    }

    fn unsupported_assignment_error(
        &self,
        format: Option<&'static str>,
        extension: Arc<str>,
    ) -> ResourceError {
        let _ = self;
        ResourceError::format_unsupported(
            FormatClassificationSource::Extension,
            DeclaredFormat::Absent,
            format,
            extension,
            None,
            Arc::from(SUPPORTED_FORMATS),
            ResourcePhase::Registry,
            None,
        )
    }
}

impl Default for HostFormatRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for HostFormatRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HostFormatRegistry")
            .field("supported_formats", &SUPPORTED_FORMATS)
            .finish()
    }
}

#[allow(dead_code)]
pub(crate) struct ClassifiedExtension {
    pub(crate) classification: HostFormatClassification,
    pub(crate) retained_extension: Arc<str>,
}

#[allow(dead_code)]
pub(crate) fn classify_logical_path(path: &str) -> ClassifiedExtension {
    let basename = path.rsplit('/').next().unwrap_or_default();
    let extension = basename
        .char_indices()
        .rev()
        .find_map(|(index, character)| (character == '.').then_some(index))
        .filter(|index| *index != 0 && *index + 1 < basename.len())
        .map_or("", |index| &basename[index..]);

    ClassifiedExtension {
        classification: classify_extension(extension),
        retained_extension: Arc::from(extension),
    }
}

fn classify_extension(extension: &str) -> HostFormatClassification {
    if extension.eq_ignore_ascii_case(".json") {
        HostFormatClassification::Shipped(HostFormat::Json)
    } else if extension.eq_ignore_ascii_case(".vue") {
        HostFormatClassification::KnownButUnshipped(KnownHostFormatId::Vue)
    } else if extension.eq_ignore_ascii_case(".yaml") || extension.eq_ignore_ascii_case(".yml") {
        HostFormatClassification::KnownButUnshipped(KnownHostFormatId::Yaml)
    } else if extension.eq_ignore_ascii_case(".jsonc") {
        HostFormatClassification::KnownButUnshipped(KnownHostFormatId::Jsonc)
    } else if extension.eq_ignore_ascii_case(".json5") {
        HostFormatClassification::KnownButUnshipped(KnownHostFormatId::Json5)
    } else if extension.eq_ignore_ascii_case(".xlf") || extension.eq_ignore_ascii_case(".xliff") {
        HostFormatClassification::KnownButUnshipped(KnownHostFormatId::Xliff)
    } else {
        HostFormatClassification::UnrecognizedExtension
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{
        classify_logical_path, HostFormat, HostFormatClassification, HostFormatRegistry,
        KnownHostFormatId, ResolvedCatalogAssignment,
    };
    use crate::{
        FormatClassificationSource, ResourceErrorCode, ResourceErrorDetails, ResourcePhase,
    };

    #[test]
    fn classifies_final_basename_extension_lexically() {
        let cases = [
            (
                "messages.json",
                ".json",
                HostFormatClassification::Shipped(HostFormat::Json),
            ),
            (
                "locales/messages.JSON",
                ".JSON",
                HostFormatClassification::Shipped(HostFormat::Json),
            ),
            (
                ".config.json",
                ".json",
                HostFormatClassification::Shipped(HostFormat::Json),
            ),
            (
                "messages..json",
                ".json",
                HostFormatClassification::Shipped(HostFormat::Json),
            ),
            (".json", "", HostFormatClassification::UnrecognizedExtension),
            (
                "messages.",
                "",
                HostFormatClassification::UnrecognizedExtension,
            ),
            (
                "dir.with.dot/messages",
                "",
                HostFormatClassification::UnrecognizedExtension,
            ),
            (
                "dir\\messages.json",
                ".json",
                HostFormatClassification::Shipped(HostFormat::Json),
            ),
        ];

        for (path, extension, classification) in cases {
            let classified = classify_logical_path(path);
            assert_eq!(classified.retained_extension.as_ref(), extension);
            assert_eq!(classified.classification, classification);
        }
    }

    #[test]
    fn recognizes_known_but_unshipped_extensions() {
        let cases = [
            ("a.vue", KnownHostFormatId::Vue),
            ("a.YML", KnownHostFormatId::Yaml),
            ("a.yaml", KnownHostFormatId::Yaml),
            ("a.jsonc", KnownHostFormatId::Jsonc),
            ("a.JSON5", KnownHostFormatId::Json5),
            ("a.xlf", KnownHostFormatId::Xliff),
            ("a.XLIFF", KnownHostFormatId::Xliff),
        ];

        for (path, expected) in cases {
            assert_eq!(
                classify_logical_path(path).classification,
                HostFormatClassification::KnownButUnshipped(expected)
            );
        }
    }

    #[test]
    fn direct_resolution_accepts_only_shipped_extension_and_retains_spelling() {
        let registry = HostFormatRegistry::new();
        let resolved = registry.resolve_direct_extension(".JSON").unwrap();

        assert_eq!(resolved.format(), HostFormat::Json);
        assert_eq!(resolved.retained_extension().as_ref(), ".JSON");
        assert!(registry.resolve_direct_extension("").is_none());
        assert!(registry.resolve_direct_extension(".yaml").is_none());
        assert!(registry.resolve_direct_extension(".unknown").is_none());
        assert_eq!(registry.supported_direct_extensions(), &[".json"]);
    }

    #[test]
    fn assignment_resolution_owns_complete_unsupported_format_evidence() {
        let registry = HostFormatRegistry::new();
        let assignment = ResolvedCatalogAssignment::new(
            HostFormatClassification::KnownButUnshipped(KnownHostFormatId::Yaml),
            Arc::from(".YML"),
            Arc::from([]),
            Arc::from([]),
        );
        let error = registry.resolve_format(&assignment).unwrap_err();

        assert_eq!(error.code(), ResourceErrorCode::FormatUnsupported);
        assert_eq!(error.phase(), ResourcePhase::Registry);
        assert!(error.site().is_none());
        assert!(matches!(
            error.details(),
            ResourceErrorDetails::FormatUnsupported {
                classification_source: FormatClassificationSource::Extension,
                format: Some("yaml"),
                extension,
                supported_formats,
                ..
            } if extension.as_ref() == ".YML" && supported_formats.as_ref() == ["json"]
        ));
    }
}
