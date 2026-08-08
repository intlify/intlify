// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Raw and resolved models for the unified `messages` configuration section.
//!
//! This module owns bounded section-local validation, cross-checks against the
//! resource-owned scope/locale inventory, and checked construction of linker
//! policy, configured roots, JS recognizers, and external artifact paths. It
//! performs no filesystem discovery, artifact decoding, production, or linking.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::Arc;

use intlify_contract::{
    ArtifactNamespace, CatalogKey, CatalogKeyDomain, CatalogKeyPattern, CatalogKeyPrefix,
    CatalogScopeId, CatalogScopeName as ContractCatalogScopeName, LinkLimitCounter, LinkLimits,
    Locale, MessageSelector, PortablePathSegment, PortableRelativePath, ReasonText,
    ValueConstructionError,
};
use intlify_linker::{
    ConfiguredRoot, CoverageBaseline, DynamicReferenceMode, InvalidRequestError,
    LinkOperationalError, LinkPolicy, LocaleFallback, PlacementPolicy,
};
use intlify_producer_js::{
    JsKeySyntax, JsRecognizerBinding, JsRecognizerCallKind, JsRecognizerSet,
};
use intlify_resource::{CatalogLocaleNotProduction, ResolvedResources, ResourceGlob};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::Serialize;
use serde_json::{Map, Value};

use super::delivery::{
    BuiltInExporterId, CheckedEagerLocales, DeliveryOutputRoot, DeliveryOutputRootError,
    DeliveryOutputRootLimit, DeliveryPlacement, DeliveryTargetInput, DeliveryTargetName,
    DeliveryTargetNameError, DeliveryTargetsError, EagerLocalesError, ResolvedDeliveryTargets,
};

const CONFIG_EVIDENCE_BYTES: usize = 255;
const LOCALE_BYTES_LIMIT: usize = 255;
const SCOPE_BYTES_LIMIT: usize = 255;
const PRODUCTION_LOCALES_LIMIT: usize = 1_024;
const CONFIGURED_ROOTS_LIMIT: usize = 4_096;
const COVERAGE_BASELINES_LIMIT: usize = 4_096;
const COVERAGE_BASELINE_BYTES_TOTAL_LIMIT: u64 = 2_088_960;
const FALLBACK_SOURCES_LIMIT: usize = 1_024;
const FALLBACK_TARGETS_PER_SOURCE_LIMIT: usize = 64;

type ValidatedFallbacks = (BTreeMap<String, Vec<String>>, Vec<LocaleFallback>);
type ValidatedRoots = (
    Vec<MessageRootConfig>,
    Vec<ConfiguredRoot>,
    Vec<ResolvedMessageDomainDeclaration>,
);
type ValidatedProducers = (
    MessageProducersConfig,
    ResolvedMessageProducers,
    Vec<ResolvedMessageDomainDeclaration>,
);
type ValidatedJsProducer = (
    MessageJsProducerConfig,
    ResolvedJsProducerConfig,
    Vec<ResolvedMessageDomainDeclaration>,
);

/// Stable message-configuration validation reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessagesConfigReason {
    /// A present `messages` value is not an object.
    InvalidMessagesSectionShape,
    /// One object contains a field outside the supported schema.
    UnknownField,
    /// Production locale policy is absent, malformed, empty, duplicate, or over limit.
    InvalidMessageLocales,
    /// The optional dynamic-reference token is malformed or unsupported.
    InvalidMessageDynamicReferences,
    /// Configured roots are malformed, duplicate, or reference an unavailable scope.
    InvalidMessageRoots,
    /// Built-in or external producer configuration is malformed.
    InvalidMessageProducers,
    /// Coverage-baseline mapping is malformed or contradicts scope/locale policy.
    InvalidMessageCoverageBaseline,
    /// Locale fallback mapping is malformed or contradicts production locale policy.
    InvalidMessageFallback,
    /// Delivery target shape, identity, exporter, output, or options are malformed.
    InvalidMessageDelivery,
    /// Two checked delivery targets own equal or nested portable output roots.
    DeliveryOutputConflict,
}

impl MessagesConfigReason {
    /// Return the stable machine-readable reason string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidMessagesSectionShape => "invalid_messages_section_shape",
            Self::UnknownField => "unknown_field",
            Self::InvalidMessageLocales => "invalid_message_locales",
            Self::InvalidMessageDynamicReferences => "invalid_message_dynamic_references",
            Self::InvalidMessageRoots => "invalid_message_roots",
            Self::InvalidMessageProducers => "invalid_message_producers",
            Self::InvalidMessageCoverageBaseline => "invalid_message_coverage_baseline",
            Self::InvalidMessageFallback => "invalid_message_fallback",
            Self::InvalidMessageDelivery => "invalid_message_delivery",
            Self::DeliveryOutputConflict => "delivery_output_conflict",
        }
    }
}

/// Bounded path-independent evidence for one messages violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessagesConfigViolation {
    reason: MessagesConfigReason,
    pointer: Arc<str>,
    field: Option<Arc<str>>,
    value: Option<Value>,
    first_pointer: Option<Arc<str>>,
    limit: Option<u64>,
    observed: Option<u64>,
    first_target: Option<Arc<str>>,
    second_target: Option<Arc<str>>,
    first_out_pointer: Option<Arc<str>>,
    second_out_pointer: Option<Arc<str>>,
}

impl MessagesConfigViolation {
    /// Return the stable validation reason.
    #[must_use]
    pub const fn reason(&self) -> MessagesConfigReason {
        self.reason
    }

    /// Return the narrowest applicable RFC 6901 pointer.
    #[must_use]
    pub fn pointer(&self) -> &str {
        &self.pointer
    }

    /// Return an exact unknown member name when applicable.
    #[must_use]
    pub fn field(&self) -> Option<&str> {
        self.field.as_deref()
    }

    /// Return bounded rejected scalar evidence when applicable.
    #[must_use]
    pub const fn value(&self) -> Option<&Value> {
        self.value.as_ref()
    }

    /// Return the earlier equal occurrence pointer for a semantic duplicate.
    #[must_use]
    pub fn first_pointer(&self) -> Option<&str> {
        self.first_pointer.as_deref()
    }

    /// Return an inclusive count or byte ceiling when applicable.
    #[must_use]
    pub const fn limit(&self) -> Option<u64> {
        self.limit
    }

    /// Return the exact submitted observation that exceeded the ceiling.
    #[must_use]
    pub const fn observed(&self) -> Option<u64> {
        self.observed
    }

    /// Return the canonical first target for an output-root conflict.
    #[must_use]
    pub fn first_target(&self) -> Option<&str> {
        self.first_target.as_deref()
    }

    /// Return the canonical second target for an output-root conflict.
    #[must_use]
    pub fn second_target(&self) -> Option<&str> {
        self.second_target.as_deref()
    }

    /// Return the submitted first target's exact `/out` pointer.
    #[must_use]
    pub fn first_out_pointer(&self) -> Option<&str> {
        self.first_out_pointer.as_deref()
    }

    /// Return the submitted second target's exact `/out` pointer.
    #[must_use]
    pub fn second_out_pointer(&self) -> Option<&str> {
        self.second_out_pointer.as_deref()
    }

    fn invalid(
        reason: MessagesConfigReason,
        pointer: impl Into<Arc<str>>,
        rejected: Option<&Value>,
        evidence_bytes: usize,
    ) -> Self {
        Self {
            reason,
            pointer: pointer.into(),
            field: None,
            value: rejected.and_then(|value| scalar_evidence(value, evidence_bytes)),
            first_pointer: None,
            limit: None,
            observed: None,
            first_target: None,
            second_target: None,
            first_out_pointer: None,
            second_out_pointer: None,
        }
    }

    fn unknown(pointer: String, field: &str, rejected: &Value) -> Self {
        let mut violation = Self::invalid(
            MessagesConfigReason::UnknownField,
            pointer,
            Some(rejected),
            CONFIG_EVIDENCE_BYTES,
        );
        violation.field = Some(Arc::from(field));
        violation
    }

    fn duplicate(reason: MessagesConfigReason, pointer: String, first_pointer: String) -> Self {
        let mut violation = Self::invalid(reason, pointer, None, CONFIG_EVIDENCE_BYTES);
        violation.first_pointer = Some(Arc::from(first_pointer));
        violation
    }

    fn with_limit(
        reason: MessagesConfigReason,
        pointer: impl Into<Arc<str>>,
        limit: u64,
        observed: u64,
    ) -> Self {
        Self {
            reason,
            pointer: pointer.into(),
            field: None,
            value: None,
            first_pointer: None,
            limit: Some(limit),
            observed: Some(observed),
            first_target: None,
            second_target: None,
            first_out_pointer: None,
            second_out_pointer: None,
        }
    }

    fn output_conflict(
        first_target: &str,
        second_target: &str,
        first_target_index: usize,
        second_target_index: usize,
    ) -> Self {
        Self {
            reason: MessagesConfigReason::DeliveryOutputConflict,
            pointer: Arc::from("/messages/delivery/targets"),
            field: None,
            value: None,
            first_pointer: None,
            limit: None,
            observed: None,
            first_target: Some(Arc::from(first_target)),
            second_target: Some(Arc::from(second_target)),
            first_out_pointer: Some(Arc::from(format!(
                "/messages/delivery/targets/{first_target_index}/out"
            ))),
            second_out_pointer: Some(Arc::from(format!(
                "/messages/delivery/targets/{second_target_index}/out"
            ))),
        }
    }
}

impl fmt::Display for MessagesConfigViolation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} at {}", self.reason.as_str(), self.pointer)
    }
}

impl std::error::Error for MessagesConfigViolation {}

/// Section-local or coordinated resource/messages validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessagesConfigError {
    /// The raw `messages` section violates its supported contract.
    Section(MessagesConfigViolation),
    /// One statically known fixed catalog locale is outside `messages.locales`.
    CatalogLocaleNotProduction(CatalogLocaleNotProduction),
}

impl From<MessagesConfigViolation> for MessagesConfigError {
    fn from(value: MessagesConfigViolation) -> Self {
        Self::Section(value)
    }
}

impl fmt::Display for MessagesConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Section(violation) => violation.fmt(formatter),
            Self::CatalogLocaleNotProduction(violation) => write!(
                formatter,
                "catalog locale is not in the production set at definition {}",
                violation.definition_index()
            ),
        }
    }
}

impl std::error::Error for MessagesConfigError {}

/// Normalized exact dynamic-reference mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[schemars(rename_all = "lowercase")]
pub enum MessageDynamicReferenceMode {
    /// Retain conservative reachability and report non-blocking degradation.
    #[default]
    Compat,
    /// Treat unbounded dynamic evidence as blocking.
    Strict,
}

impl MessageDynamicReferenceMode {
    const fn resolved(self) -> DynamicReferenceMode {
        match self {
            Self::Compat => DynamicReferenceMode::Compat,
            Self::Strict => DynamicReferenceMode::Strict,
        }
    }
}

/// Normalized catalog-key domain token used by roots and recognizers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, JsonSchema)]
pub enum MessageCatalogKeyDomain {
    /// RFC 6901 JSON Pointer.
    #[serde(rename = "json-pointer")]
    #[schemars(rename = "json-pointer")]
    JsonPointer,
    /// YAML Core-Schema typed path.
    #[serde(rename = "yaml-typed-path")]
    #[schemars(rename = "yaml-typed-path")]
    YamlTypedPath,
    /// XLIFF 1.2 hierarchy.
    #[serde(rename = "xliff-1.2")]
    #[schemars(rename = "xliff-1.2")]
    Xliff12,
    /// XLIFF 2.x hierarchy.
    #[serde(rename = "xliff-2")]
    #[schemars(rename = "xliff-2")]
    Xliff2,
}

impl MessageCatalogKeyDomain {
    const fn resolved(self) -> CatalogKeyDomain {
        match self {
            Self::JsonPointer => CatalogKeyDomain::JsonPointer,
            Self::YamlTypedPath => CatalogKeyDomain::YamlTypedPath,
            Self::Xliff12 => CatalogKeyDomain::Xliff12,
            Self::Xliff2 => CatalogKeyDomain::Xliff2,
        }
    }
}

/// Normalized configured-root selector.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
#[schemars(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum MessageSelectorConfig {
    /// One exact canonical key.
    Exact {
        /// Canonical key spelling.
        key: String,
    },
    /// One canonical structural prefix.
    Prefix {
        /// Canonical prefix spelling.
        #[schemars(length(min = 1))]
        prefix: String,
    },
    /// One bounded canonical structural pattern.
    Pattern {
        /// Canonical pattern spelling.
        #[schemars(length(min = 1))]
        pattern: String,
    },
    /// Every key in the configured scope-domain pair.
    AllInScope,
}

/// One normalized configured root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageRootConfig {
    #[schemars(length(min = 1))]
    scope: String,
    domain: MessageCatalogKeyDomain,
    selector: MessageSelectorConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(default, schema_with = "nonempty_string_schema")]
    reason: Option<String>,
}

/// Normalized built-in JS recognizer call kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[schemars(rename_all = "lowercase")]
pub enum MessageJsCallKind {
    /// One ordinary message lookup.
    Lookup,
    /// One explicit finite set declaration.
    Set,
}

impl MessageJsCallKind {
    const fn resolved(self) -> JsRecognizerCallKind {
        match self {
            Self::Lookup => JsRecognizerCallKind::Lookup,
            Self::Set => JsRecognizerCallKind::Set,
        }
    }
}

/// Normalized JS source-facing key syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(rename_all = "kebab-case")]
pub enum MessageJsKeySyntax {
    /// The source string is already canonical.
    Canonical,
    /// The source string is one literal JSON Pointer segment.
    Literal,
    /// The source string uses the fixed dot-path grammar.
    DotPath,
}

impl MessageJsKeySyntax {
    const fn resolved(self) -> JsKeySyntax {
        match self {
            Self::Canonical => JsKeySyntax::Canonical,
            Self::Literal => JsKeySyntax::Literal,
            Self::DotPath => JsKeySyntax::DotPath,
        }
    }
}

/// One normalized built-in JS recognizer binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageJsRecognizerConfig {
    kind: MessageJsCallKind,
    #[schemars(length(min = 1))]
    scope: String,
    domain: MessageCatalogKeyDomain,
    key_syntax: MessageJsKeySyntax,
}

/// Normalized built-in JS producer configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageJsProducerConfig {
    #[schemars(
        length(min = 1),
        schema_with = "resource_glob_array_schema",
        description = "Non-empty JS/TS source membership patterns."
    )]
    include: Vec<ResourceGlob>,
    #[schemars(schema_with = "recognizers_schema")]
    recognizers: BTreeMap<String, MessageJsRecognizerConfig>,
}

/// Normalized producer declarations.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageProducersConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(default, schema_with = "js_producer_schema")]
    js: Option<MessageJsProducerConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(length(min = 1))]
    artifacts: Vec<String>,
}

/// Normalized delivery configuration admitted for executable message output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageDeliveryConfig {
    #[schemars(length(min = 1, max = 64))]
    targets: Vec<MessageDeliveryTargetConfig>,
}

impl MessageDeliveryConfig {
    /// Return targets in canonical checked-name order.
    #[must_use]
    pub fn targets(&self) -> &[MessageDeliveryTargetConfig] {
        &self.targets
    }
}

/// One normalized checked delivery target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageDeliveryTargetConfig {
    #[schemars(schema_with = "delivery_target_name_schema")]
    name: String,
    #[schemars(schema_with = "delivery_exporter_schema")]
    exporter: String,
    #[schemars(schema_with = "delivery_output_root_schema")]
    out: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(default, schema_with = "eager_locales_schema")]
    eager_locales: Vec<String>,
    #[serde(
        default = "duplicate_placement",
        skip_serializing_if = "is_duplicate_placement"
    )]
    #[schemars(
        default = "duplicate_placement",
        schema_with = "delivery_placement_schema"
    )]
    placement: String,
}

impl MessageDeliveryTargetConfig {
    /// Return the exact checked target identity.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the exact built-in exporter identifier.
    #[must_use]
    pub fn exporter(&self) -> &str {
        &self.exporter
    }

    /// Return the checked project-root-relative output root.
    #[must_use]
    pub fn out(&self) -> &str {
        &self.out
    }

    /// Return eager locales in canonical locale order.
    #[must_use]
    pub fn eager_locales(&self) -> &[String] {
        &self.eager_locales
    }

    /// Return the normalized target-wide placement token.
    #[must_use]
    pub fn placement(&self) -> &str {
        &self.placement
    }
}

/// Validated normalized `messages` section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessagesConfig {
    #[schemars(schema_with = "production_locales_schema")]
    locales: Vec<String>,
    #[serde(default)]
    dynamic_references: MessageDynamicReferenceMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(length(max = 4096))]
    roots: Vec<MessageRootConfig>,
    #[serde(default, skip_serializing_if = "is_default_producers")]
    producers: MessageProducersConfig,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[schemars(default, schema_with = "coverage_baseline_schema")]
    coverage_baseline: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[schemars(default, schema_with = "fallback_schema")]
    fallback: BTreeMap<String, Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(default, schema_with = "delivery_schema")]
    delivery: Option<MessageDeliveryConfig>,
}

impl MessagesConfig {
    /// Return canonical exact production-locale spellings.
    #[must_use]
    pub fn locales(&self) -> &[String] {
        &self.locales
    }

    /// Return the normalized dynamic-reference mode.
    #[must_use]
    pub const fn dynamic_references(&self) -> MessageDynamicReferenceMode {
        self.dynamic_references
    }

    /// Return configured roots in canonical semantic order.
    #[must_use]
    pub fn roots(&self) -> &[MessageRootConfig] {
        &self.roots
    }

    /// Return normalized producer declarations.
    #[must_use]
    pub const fn producers(&self) -> &MessageProducersConfig {
        &self.producers
    }

    /// Return canonical declared-scope coverage-baseline spellings.
    #[must_use]
    pub const fn coverage_baseline(&self) -> &BTreeMap<String, String> {
        &self.coverage_baseline
    }

    /// Return canonical fallback sources with semantic target order preserved.
    #[must_use]
    pub const fn fallback(&self) -> &BTreeMap<String, Vec<String>> {
        &self.fallback
    }

    /// Return normalized delivery targets when output delivery is configured.
    #[must_use]
    pub const fn delivery(&self) -> Option<&MessageDeliveryConfig> {
        self.delivery.as_ref()
    }
}

fn is_default_producers(value: &MessageProducersConfig) -> bool {
    value == &MessageProducersConfig::default()
}

fn resource_glob_array_schema(generator: &mut SchemaGenerator) -> Schema {
    Vec::<String>::json_schema(generator)
}

fn production_locales_schema(_: &mut SchemaGenerator) -> Schema {
    schemars::json_schema!({
        "type": "array",
        "minItems": 1,
        "maxItems": 1024,
        "items": {
            "type": "string",
            "minLength": 1
        }
    })
}

fn nonempty_string_schema(_: &mut SchemaGenerator) -> Schema {
    schemars::json_schema!({
        "type": "string",
        "minLength": 1
    })
}

fn js_producer_schema(generator: &mut SchemaGenerator) -> Schema {
    MessageJsProducerConfig::json_schema(generator)
}

fn recognizers_schema(generator: &mut SchemaGenerator) -> Schema {
    let mut schema = BTreeMap::<String, MessageJsRecognizerConfig>::json_schema(generator);
    schema.insert("minProperties".to_owned(), Value::from(1));
    schema
}

fn coverage_baseline_schema(_: &mut SchemaGenerator) -> Schema {
    schemars::json_schema!({
        "type": "object",
        "minProperties": 1,
        "maxProperties": 4096,
        "propertyNames": {
            "type": "string",
            "minLength": 1,
            "maxLength": 255
        },
        "additionalProperties": {
            "type": "string",
            "minLength": 1,
            "maxLength": 255
        }
    })
}

fn fallback_schema(_: &mut SchemaGenerator) -> Schema {
    schemars::json_schema!({
        "type": "object",
        "maxProperties": 1024,
        "propertyNames": {
            "type": "string",
            "minLength": 1,
            "maxLength": 255
        },
        "additionalProperties": {
            "type": "array",
            "minItems": 1,
            "maxItems": 64,
            "uniqueItems": true,
            "items": {
                "type": "string",
                "minLength": 1,
                "maxLength": 255
            }
        }
    })
}

fn delivery_target_name_schema(_: &mut SchemaGenerator) -> Schema {
    schemars::json_schema!({
        "type": "string",
        "minLength": 1,
        "maxLength": 255,
        "pattern": "^[a-z0-9][a-z0-9._-]{0,254}$"
    })
}

fn delivery_schema(generator: &mut SchemaGenerator) -> Schema {
    MessageDeliveryConfig::json_schema(generator)
}

fn delivery_exporter_schema(_: &mut SchemaGenerator) -> Schema {
    schemars::json_schema!({
        "type": "string",
        "enum": ["esm"]
    })
}

fn delivery_output_root_schema(_: &mut SchemaGenerator) -> Schema {
    schemars::json_schema!({
        "type": "string",
        "minLength": 1,
        "maxLength": 4159
    })
}

fn eager_locales_schema(_: &mut SchemaGenerator) -> Schema {
    schemars::json_schema!({
        "type": "array",
        "maxItems": 1024,
        "uniqueItems": true,
        "items": {
            "type": "string",
            "minLength": 1,
            "maxLength": 255
        }
    })
}

fn duplicate_placement() -> String {
    "duplicate".to_owned()
}

fn is_duplicate_placement(value: &String) -> bool {
    value == "duplicate"
}

fn delivery_placement_schema(_: &mut SchemaGenerator) -> Schema {
    schemars::json_schema!({
        "type": "string",
        "enum": ["duplicate"]
    })
}

/// Immutable resolved built-in JS producer inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedJsProducerConfig {
    include: Arc<[ResourceGlob]>,
    recognizers: JsRecognizerSet,
}

impl ResolvedJsProducerConfig {
    /// Return canonical checked source membership patterns.
    #[must_use]
    pub fn include(&self) -> &[ResourceGlob] {
        &self.include
    }

    /// Return canonical checked JS recognizers.
    #[must_use]
    pub const fn recognizers(&self) -> &JsRecognizerSet {
        &self.recognizers
    }
}

/// Immutable resolved producer inputs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedMessageProducers {
    js: Option<ResolvedJsProducerConfig>,
    artifacts: Arc<[PortableRelativePath]>,
}

impl ResolvedMessageProducers {
    /// Return the built-in JS producer when configured.
    #[must_use]
    pub const fn js(&self) -> Option<&ResolvedJsProducerConfig> {
        self.js.as_ref()
    }

    /// Return canonical exact external artifact paths.
    #[must_use]
    pub fn artifacts(&self) -> &[PortableRelativePath] {
        &self.artifacts
    }
}

/// One checked scope/domain declaration retained with its configuration pointer.
///
/// Definition production uses these declarations only after the complete
/// catalog extraction attempt. Recognizers precede configured roots, preserving
/// the domain-admission order independently of policy canonicalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedMessageDomainDeclaration {
    scope: CatalogScopeId,
    domain: CatalogKeyDomain,
    pointer: Arc<str>,
}

impl ResolvedMessageDomainDeclaration {
    /// Return the exact declared project scope.
    #[must_use]
    pub(crate) const fn scope(&self) -> &CatalogScopeId {
        &self.scope
    }

    /// Return the declared catalog-key comparison domain.
    #[must_use]
    pub(crate) const fn domain(&self) -> CatalogKeyDomain {
        self.domain
    }

    /// Return the narrowest configuration pointer for the domain declaration.
    #[must_use]
    pub(crate) fn pointer(&self) -> &str {
        &self.pointer
    }
}

/// Immutable linker policy and producer configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMessagesConfig {
    policy: LinkPolicy,
    producers: ResolvedMessageProducers,
    domain_declarations: Arc<[ResolvedMessageDomainDeclaration]>,
    delivery: Option<ResolvedDeliveryTargets>,
}

impl ResolvedMessagesConfig {
    /// Return the canonical checked linker policy.
    #[must_use]
    pub const fn policy(&self) -> &LinkPolicy {
        &self.policy
    }

    /// Return canonical checked producer inputs.
    #[must_use]
    pub const fn producers(&self) -> &ResolvedMessageProducers {
        &self.producers
    }

    /// Return recognizer declarations followed by roots in admission order.
    #[must_use]
    pub(crate) fn domain_declarations(&self) -> &[ResolvedMessageDomainDeclaration] {
        &self.domain_declarations
    }

    /// Return canonical checked delivery targets when configured.
    #[must_use]
    pub(crate) const fn delivery(&self) -> Option<&ResolvedDeliveryTargets> {
        self.delivery.as_ref()
    }

    /// Return whether an exact resource-resolved locale is in the production set.
    #[must_use]
    pub fn contains_production_locale(&self, locale: &str) -> bool {
        self.policy
            .production_locales()
            .binary_search_by(|candidate| candidate.as_str().as_bytes().cmp(locale.as_bytes()))
            .is_ok()
    }
}

/// Validate an optional raw `messages` section after resources have resolved.
///
/// Omission remains `None`; no policy or producer is synthesized. A present
/// section is validated to completion before any filesystem discovery can
/// begin.
pub fn validate_messages_config(
    value: Option<&Value>,
    resources: &ResolvedResources,
) -> Result<Option<(MessagesConfig, ResolvedMessagesConfig)>, MessagesConfigError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let Some(section) = value.as_object() else {
        return Err(MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessagesSectionShape,
            "/messages",
            Some(value),
            CONFIG_EVIDENCE_BYTES,
        )
        .into());
    };

    if let Some((field, rejected)) = first_unknown_field(
        section,
        &[
            "locales",
            "dynamicReferences",
            "roots",
            "producers",
            "coverageBaseline",
            "fallback",
            "delivery",
        ],
    ) {
        return Err(MessagesConfigViolation::unknown(
            pointer_property("/messages", field),
            field,
            rejected,
        )
        .into());
    }

    let (locales, resolved_locales) = validate_locales(section.get("locales"))?;
    let dynamic_references = validate_dynamic_references(section.get("dynamicReferences"))?;
    let (mut roots, resolved_roots, root_domains) =
        validate_roots(section.get("roots"), resources)?;
    let (producers, resolved_producers, mut domain_declarations) =
        validate_producers(section.get("producers"), resources)?;
    domain_declarations.extend(root_domains);
    let (coverage_baseline, resolved_coverage_baselines) = validate_coverage_baseline(
        section.get("coverageBaseline"),
        resources,
        &resolved_locales,
    )?;
    let (fallback, resolved_fallbacks) =
        validate_fallback(section.get("fallback"), &resolved_locales)?;

    let limits = LinkLimits::default();
    let policy = LinkPolicy::try_new(
        resolved_locales,
        resolved_fallbacks,
        resolved_roots,
        resolved_coverage_baselines,
        dynamic_references.resolved(),
        PlacementPolicy::Duplicate,
        &limits,
    )
    .map_err(policy_config_violation)?;

    let production = policy
        .production_locales()
        .iter()
        .map(Locale::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    if let Some(violation) =
        resources.first_fixed_locale_not_production(|locale| production.contains(locale))
    {
        return Err(MessagesConfigError::CatalogLocaleNotProduction(violation));
    }

    let (delivery, resolved_delivery) = validate_delivery(section.get("delivery"), &policy)?;

    roots.sort_by(root_config_order);
    Ok(Some((
        MessagesConfig {
            locales,
            dynamic_references,
            roots,
            producers,
            coverage_baseline,
            fallback,
            delivery,
        },
        ResolvedMessagesConfig {
            policy,
            producers: resolved_producers,
            domain_declarations: Arc::from(domain_declarations),
            delivery: resolved_delivery,
        },
    )))
}

fn validate_delivery(
    value: Option<&Value>,
    policy: &LinkPolicy,
) -> Result<
    (
        Option<MessageDeliveryConfig>,
        Option<ResolvedDeliveryTargets>,
    ),
    MessagesConfigViolation,
> {
    let Some(value) = value else {
        return Ok((None, None));
    };
    let pointer = "/messages/delivery";
    let object = value.as_object().ok_or_else(|| {
        MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageDelivery,
            pointer,
            Some(value),
            CONFIG_EVIDENCE_BYTES,
        )
    })?;
    if let Some((field, rejected)) = first_unknown_field(object, &["targets"]) {
        return Err(MessagesConfigViolation::unknown(
            pointer_property(pointer, field),
            field,
            rejected,
        ));
    }

    let targets_pointer = pointer_property(pointer, "targets");
    let targets_value = object.get("targets").ok_or_else(|| {
        MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageDelivery,
            targets_pointer.clone(),
            None,
            CONFIG_EVIDENCE_BYTES,
        )
    })?;
    let targets = targets_value.as_array().ok_or_else(|| {
        MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageDelivery,
            targets_pointer.clone(),
            Some(targets_value),
            CONFIG_EVIDENCE_BYTES,
        )
    })?;
    if targets.is_empty() {
        return Err(MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageDelivery,
            targets_pointer,
            Some(targets_value),
            CONFIG_EVIDENCE_BYTES,
        ));
    }
    if targets.len() > 64 {
        return Err(MessagesConfigViolation::with_limit(
            MessagesConfigReason::InvalidMessageDelivery,
            targets_pointer,
            64,
            usize_to_u64(targets.len()),
        ));
    }

    let mut target_objects = Vec::with_capacity(targets.len());
    for (target_index, target) in targets.iter().enumerate() {
        let target_pointer = pointer_index(&targets_pointer, target_index);
        let target_object = target.as_object().ok_or_else(|| {
            MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageDelivery,
                target_pointer.clone(),
                Some(target),
                CONFIG_EVIDENCE_BYTES,
            )
        })?;
        if let Some((field, rejected)) = first_unknown_field(
            target_object,
            &["name", "exporter", "out", "eagerLocales", "placement"],
        ) {
            return Err(MessagesConfigViolation::unknown(
                pointer_property(&target_pointer, field),
                field,
                rejected,
            ));
        }
        target_objects.push(target_object);
    }

    let names = decode_required_delivery_strings(&target_objects, &targets_pointer, "name")?;
    validate_delivery_names(&names, &targets_pointer)?;

    let exporters =
        decode_required_delivery_strings(&target_objects, &targets_pointer, "exporter")?;
    for (target_index, exporter) in exporters.iter().enumerate() {
        if BuiltInExporterId::try_new(exporter).is_err() {
            return Err(delivery_string_violation(
                pointer_property(&pointer_index(&targets_pointer, target_index), "exporter"),
                exporter,
            ));
        }
    }

    let outputs = decode_required_delivery_strings(&target_objects, &targets_pointer, "out")?;
    for (target_index, output) in outputs.iter().enumerate() {
        if let Err(error) = DeliveryOutputRoot::try_new(output) {
            return Err(delivery_output_violation(
                &targets_pointer,
                target_index,
                output,
                error,
            ));
        }
    }

    let mut eager_locales = Vec::with_capacity(target_objects.len());
    for (target_index, target) in target_objects.iter().enumerate() {
        let eager_pointer = pointer_property(
            &pointer_index(&targets_pointer, target_index),
            "eagerLocales",
        );
        let Some(value) = target.get("eagerLocales") else {
            eager_locales.push(Vec::new());
            continue;
        };
        let locales = value.as_array().ok_or_else(|| {
            MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageDelivery,
                eager_pointer.clone(),
                Some(value),
                CONFIG_EVIDENCE_BYTES,
            )
        })?;
        if locales.len() > 1_024 {
            return Err(MessagesConfigViolation::with_limit(
                MessagesConfigReason::InvalidMessageDelivery,
                eager_pointer,
                1_024,
                usize_to_u64(locales.len()),
            ));
        }
        let mut spellings = Vec::with_capacity(locales.len());
        for (locale_index, locale) in locales.iter().enumerate() {
            let locale_pointer = pointer_index(&eager_pointer, locale_index);
            let spelling = locale.as_str().ok_or_else(|| {
                MessagesConfigViolation::invalid(
                    MessagesConfigReason::InvalidMessageDelivery,
                    locale_pointer,
                    Some(locale),
                    LOCALE_BYTES_LIMIT,
                )
            })?;
            spellings.push(spelling.to_owned());
        }
        eager_locales.push(spellings);
    }
    for (target_index, locales) in eager_locales.iter().enumerate() {
        if let Err(error) = CheckedEagerLocales::try_new(locales, policy) {
            return Err(delivery_eager_locales_violation(
                &targets_pointer,
                target_index,
                locales,
                &error,
            ));
        }
    }

    let mut placements = Vec::with_capacity(target_objects.len());
    for (target_index, target) in target_objects.iter().enumerate() {
        let placement_pointer =
            pointer_property(&pointer_index(&targets_pointer, target_index), "placement");
        let placement = target
            .get("placement")
            .map(|value| {
                value.as_str().map(str::to_owned).ok_or_else(|| {
                    MessagesConfigViolation::invalid(
                        MessagesConfigReason::InvalidMessageDelivery,
                        placement_pointer,
                        Some(value),
                        CONFIG_EVIDENCE_BYTES,
                    )
                })
            })
            .transpose()?;
        placements.push(placement);
    }
    for (target_index, placement) in placements.iter().enumerate() {
        if DeliveryPlacement::try_new(placement.as_deref()).is_err() {
            return Err(delivery_string_violation(
                pointer_property(&pointer_index(&targets_pointer, target_index), "placement"),
                placement
                    .as_deref()
                    .expect("unsupported placement must be present"),
            ));
        }
    }

    let inputs = names
        .iter()
        .zip(&exporters)
        .zip(&outputs)
        .zip(&eager_locales)
        .zip(&placements)
        .map(|((((name, exporter), out), eager_locales), placement)| {
            DeliveryTargetInput::new(name, exporter, out, eager_locales, placement.as_deref())
        })
        .collect::<Vec<_>>();
    let resolved = ResolvedDeliveryTargets::try_new(&inputs, policy)
        .map_err(|error| delivery_targets_violation(error, &targets_pointer, &inputs))?;
    let normalized = MessageDeliveryConfig {
        targets: resolved
            .targets()
            .iter()
            .map(|target| MessageDeliveryTargetConfig {
                name: target.name().as_str().to_owned(),
                exporter: target.exporter().as_str().to_owned(),
                out: target.out().as_str().to_owned(),
                eager_locales: target
                    .eager_locales()
                    .locales()
                    .iter()
                    .map(|locale| locale.as_str().to_owned())
                    .collect(),
                placement: target.placement().as_str().to_owned(),
            })
            .collect(),
    };
    Ok((Some(normalized), Some(resolved)))
}

fn decode_required_delivery_strings(
    targets: &[&Map<String, Value>],
    targets_pointer: &str,
    field: &str,
) -> Result<Vec<String>, MessagesConfigViolation> {
    targets
        .iter()
        .enumerate()
        .map(|(target_index, target)| {
            let pointer = pointer_property(&pointer_index(targets_pointer, target_index), field);
            let value = target.get(field).ok_or_else(|| {
                MessagesConfigViolation::invalid(
                    MessagesConfigReason::InvalidMessageDelivery,
                    pointer.clone(),
                    None,
                    CONFIG_EVIDENCE_BYTES,
                )
            })?;
            value.as_str().map(str::to_owned).ok_or_else(|| {
                MessagesConfigViolation::invalid(
                    MessagesConfigReason::InvalidMessageDelivery,
                    pointer,
                    Some(value),
                    CONFIG_EVIDENCE_BYTES,
                )
            })
        })
        .collect()
}

fn validate_delivery_names(
    names: &[String],
    targets_pointer: &str,
) -> Result<(), MessagesConfigViolation> {
    let mut checked = Vec::with_capacity(names.len());
    for (target_index, name) in names.iter().enumerate() {
        let pointer = pointer_property(&pointer_index(targets_pointer, target_index), "name");
        let name = DeliveryTargetName::try_new(name).map_err(|error| match error {
            DeliveryTargetNameError::InvalidGrammar => delivery_string_violation(pointer, name),
            DeliveryTargetNameError::ByteLimitExceeded { limit, observed } => {
                MessagesConfigViolation::with_limit(
                    MessagesConfigReason::InvalidMessageDelivery,
                    pointer,
                    limit,
                    observed,
                )
            }
        })?;
        checked.push(name);
    }

    let mut first_occurrences = BTreeMap::new();
    for (target_index, name) in checked.into_iter().enumerate() {
        if let Some(first_target_index) = first_occurrences.insert(name, target_index) {
            return Err(MessagesConfigViolation::duplicate(
                MessagesConfigReason::InvalidMessageDelivery,
                pointer_property(&pointer_index(targets_pointer, target_index), "name"),
                pointer_property(&pointer_index(targets_pointer, first_target_index), "name"),
            ));
        }
    }
    Ok(())
}

fn delivery_output_violation(
    targets_pointer: &str,
    target_index: usize,
    output: &str,
    error: DeliveryOutputRootError,
) -> MessagesConfigViolation {
    let pointer = pointer_property(&pointer_index(targets_pointer, target_index), "out");
    match error {
        DeliveryOutputRootError::InvalidGrammar => delivery_string_violation(pointer, output),
        DeliveryOutputRootError::LimitExceeded {
            counter:
                DeliveryOutputRootLimit::Segments
                | DeliveryOutputRootLimit::SegmentBytes
                | DeliveryOutputRootLimit::DecodedBytes,
            limit,
            observed,
        } => MessagesConfigViolation::with_limit(
            MessagesConfigReason::InvalidMessageDelivery,
            pointer,
            limit,
            observed,
        ),
    }
}

fn delivery_eager_locales_violation(
    targets_pointer: &str,
    target_index: usize,
    locales: &[String],
    error: &EagerLocalesError,
) -> MessagesConfigViolation {
    let pointer = pointer_property(
        &pointer_index(targets_pointer, target_index),
        "eagerLocales",
    );
    match error {
        EagerLocalesError::CountLimitExceeded { limit, observed } => {
            MessagesConfigViolation::with_limit(
                MessagesConfigReason::InvalidMessageDelivery,
                pointer,
                *limit,
                *observed,
            )
        }
        EagerLocalesError::LocaleByteLimitExceeded {
            occurrence,
            limit,
            observed,
        } => MessagesConfigViolation::with_limit(
            MessagesConfigReason::InvalidMessageDelivery,
            pointer_index(&pointer, *occurrence),
            *limit,
            *observed,
        ),
        EagerLocalesError::InvalidLocale { occurrence }
        | EagerLocalesError::LocaleNotProduction { occurrence, .. } => {
            delivery_string_violation(pointer_index(&pointer, *occurrence), &locales[*occurrence])
        }
        EagerLocalesError::DuplicateLocale {
            occurrence,
            first_occurrence,
            ..
        } => MessagesConfigViolation::duplicate(
            MessagesConfigReason::InvalidMessageDelivery,
            pointer_index(&pointer, *occurrence),
            pointer_index(&pointer, *first_occurrence),
        ),
    }
}

fn delivery_targets_violation(
    error: DeliveryTargetsError,
    targets_pointer: &str,
    inputs: &[DeliveryTargetInput<'_>],
) -> MessagesConfigViolation {
    let reason = MessagesConfigReason::InvalidMessageDelivery;
    match error {
        DeliveryTargetsError::TargetCount {
            minimum: _,
            maximum,
            observed,
        } => MessagesConfigViolation::with_limit(reason, targets_pointer, maximum, observed),
        DeliveryTargetsError::InvalidTargetName {
            target_index,
            error,
        } => {
            let pointer = pointer_property(&pointer_index(targets_pointer, target_index), "name");
            match error {
                DeliveryTargetNameError::InvalidGrammar => {
                    delivery_string_violation(pointer, inputs[target_index].name())
                }
                DeliveryTargetNameError::ByteLimitExceeded { limit, observed } => {
                    MessagesConfigViolation::with_limit(reason, pointer, limit, observed)
                }
            }
        }
        DeliveryTargetsError::DuplicateTargetName {
            first_target_index,
            second_target_index,
            ..
        } => MessagesConfigViolation::duplicate(
            reason,
            pointer_property(&pointer_index(targets_pointer, second_target_index), "name"),
            pointer_property(&pointer_index(targets_pointer, first_target_index), "name"),
        ),
        DeliveryTargetsError::UnsupportedExporter { target_index } => delivery_string_violation(
            pointer_property(&pointer_index(targets_pointer, target_index), "exporter"),
            inputs[target_index].exporter(),
        ),
        DeliveryTargetsError::InvalidOutputRoot {
            target_index,
            error,
        } => {
            let pointer = pointer_property(&pointer_index(targets_pointer, target_index), "out");
            match error {
                DeliveryOutputRootError::InvalidGrammar => {
                    delivery_string_violation(pointer, inputs[target_index].out())
                }
                DeliveryOutputRootError::LimitExceeded {
                    counter:
                        DeliveryOutputRootLimit::Segments
                        | DeliveryOutputRootLimit::SegmentBytes
                        | DeliveryOutputRootLimit::DecodedBytes,
                    limit,
                    observed,
                } => MessagesConfigViolation::with_limit(reason, pointer, limit, observed),
            }
        }
        DeliveryTargetsError::InvalidEagerLocales {
            target_index,
            error,
        } => {
            let pointer = pointer_property(
                &pointer_index(targets_pointer, target_index),
                "eagerLocales",
            );
            match error {
                EagerLocalesError::CountLimitExceeded { limit, observed } => {
                    MessagesConfigViolation::with_limit(reason, pointer, limit, observed)
                }
                EagerLocalesError::LocaleByteLimitExceeded {
                    occurrence,
                    limit,
                    observed,
                } => MessagesConfigViolation::with_limit(
                    reason,
                    pointer_index(&pointer, occurrence),
                    limit,
                    observed,
                ),
                EagerLocalesError::InvalidLocale { occurrence }
                | EagerLocalesError::LocaleNotProduction { occurrence, .. } => {
                    delivery_string_violation(
                        pointer_index(&pointer, occurrence),
                        &inputs[target_index].eager_locales()[occurrence],
                    )
                }
                EagerLocalesError::DuplicateLocale {
                    occurrence,
                    first_occurrence,
                    ..
                } => MessagesConfigViolation::duplicate(
                    reason,
                    pointer_index(&pointer, occurrence),
                    pointer_index(&pointer, first_occurrence),
                ),
            }
        }
        DeliveryTargetsError::UnsupportedPlacement { target_index } => delivery_string_violation(
            pointer_property(&pointer_index(targets_pointer, target_index), "placement"),
            inputs[target_index]
                .placement()
                .expect("unsupported placement must be present"),
        ),
        DeliveryTargetsError::OutputRootConflict {
            first_name,
            second_name,
            first_target_index,
            second_target_index,
        } => MessagesConfigViolation::output_conflict(
            first_name.as_str(),
            second_name.as_str(),
            first_target_index,
            second_target_index,
        ),
    }
}

fn delivery_string_violation(pointer: String, value: &str) -> MessagesConfigViolation {
    MessagesConfigViolation::invalid(
        MessagesConfigReason::InvalidMessageDelivery,
        pointer,
        Some(&Value::String(value.to_owned())),
        CONFIG_EVIDENCE_BYTES,
    )
}

fn validate_locales(
    value: Option<&Value>,
) -> Result<(Vec<String>, Vec<Locale>), MessagesConfigViolation> {
    let pointer = "/messages/locales";
    let value = value.ok_or_else(|| {
        MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageLocales,
            pointer,
            None,
            CONFIG_EVIDENCE_BYTES,
        )
    })?;
    let values = value.as_array().ok_or_else(|| {
        MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageLocales,
            pointer,
            Some(value),
            CONFIG_EVIDENCE_BYTES,
        )
    })?;
    if values.is_empty() {
        return Err(MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageLocales,
            pointer,
            Some(value),
            CONFIG_EVIDENCE_BYTES,
        ));
    }
    if values.len() > PRODUCTION_LOCALES_LIMIT {
        return Err(MessagesConfigViolation::with_limit(
            MessagesConfigReason::InvalidMessageLocales,
            pointer,
            PRODUCTION_LOCALES_LIMIT as u64,
            values.len() as u64,
        ));
    }

    let mut checked = Vec::with_capacity(values.len());
    let mut first_occurrences = HashMap::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let entry_pointer = pointer_index(pointer, index);
        let locale = value.as_str().ok_or_else(|| {
            MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageLocales,
                entry_pointer.clone(),
                Some(value),
                LOCALE_BYTES_LIMIT,
            )
        })?;
        if locale.len() > LOCALE_BYTES_LIMIT {
            return Err(MessagesConfigViolation::with_limit(
                MessagesConfigReason::InvalidMessageLocales,
                entry_pointer,
                LOCALE_BYTES_LIMIT as u64,
                locale.len() as u64,
            ));
        }
        let resolved = Locale::try_new(locale).map_err(|_| {
            MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageLocales,
                entry_pointer.clone(),
                Some(value),
                LOCALE_BYTES_LIMIT,
            )
        })?;
        if let Some(first_index) = first_occurrences.insert(locale.to_owned(), index) {
            return Err(MessagesConfigViolation::duplicate(
                MessagesConfigReason::InvalidMessageLocales,
                entry_pointer,
                pointer_index(pointer, first_index),
            ));
        }
        checked.push((locale.to_owned(), resolved));
    }

    checked.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    let (normalized, resolved): (Vec<_>, Vec<_>) = checked.into_iter().unzip();
    Ok((normalized, resolved))
}

fn validate_dynamic_references(
    value: Option<&Value>,
) -> Result<MessageDynamicReferenceMode, MessagesConfigViolation> {
    let Some(value) = value else {
        return Ok(MessageDynamicReferenceMode::Compat);
    };
    let pointer = "/messages/dynamicReferences";
    let token = value.as_str().ok_or_else(|| {
        MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageDynamicReferences,
            pointer,
            Some(value),
            CONFIG_EVIDENCE_BYTES,
        )
    })?;
    match token {
        "compat" => Ok(MessageDynamicReferenceMode::Compat),
        "strict" => Ok(MessageDynamicReferenceMode::Strict),
        _ => Err(MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageDynamicReferences,
            pointer,
            Some(value),
            CONFIG_EVIDENCE_BYTES,
        )),
    }
}

fn validate_coverage_baseline(
    value: Option<&Value>,
    resources: &ResolvedResources,
    production_locales: &[Locale],
) -> Result<(BTreeMap<String, String>, Vec<CoverageBaseline>), MessagesConfigViolation> {
    let Some(value) = value else {
        return Ok((BTreeMap::new(), Vec::new()));
    };
    let pointer = "/messages/coverageBaseline";
    let object = value.as_object().ok_or_else(|| {
        MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageCoverageBaseline,
            pointer,
            Some(value),
            CONFIG_EVIDENCE_BYTES,
        )
    })?;
    if object.is_empty() {
        return Err(MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageCoverageBaseline,
            pointer,
            Some(value),
            CONFIG_EVIDENCE_BYTES,
        ));
    }
    if object.len() > COVERAGE_BASELINES_LIMIT {
        return Err(MessagesConfigViolation::with_limit(
            MessagesConfigReason::InvalidMessageCoverageBaseline,
            pointer,
            COVERAGE_BASELINES_LIMIT as u64,
            object.len() as u64,
        ));
    }

    let mut members = object.iter().collect::<Vec<_>>();
    members.sort_by(|(left, _), (right, _)| left.as_bytes().cmp(right.as_bytes()));

    let mut checked_scopes = Vec::with_capacity(members.len());
    for (scope, _) in &members {
        if scope.len() > SCOPE_BYTES_LIMIT {
            return Err(MessagesConfigViolation::with_limit(
                MessagesConfigReason::InvalidMessageCoverageBaseline,
                pointer,
                SCOPE_BYTES_LIMIT as u64,
                scope.len() as u64,
            ));
        }
        let scope_name = ContractCatalogScopeName::try_new(scope.as_str()).map_err(|_| {
            MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageCoverageBaseline,
                pointer_property(pointer, scope),
                None,
                CONFIG_EVIDENCE_BYTES,
            )
        })?;
        checked_scopes.push(CatalogScopeId::new(ArtifactNamespace::Project, scope_name));
    }

    let mut checked_locales = Vec::with_capacity(members.len());
    for (scope, value) in &members {
        let member_pointer = pointer_property(pointer, scope);
        let locale = value.as_str().ok_or_else(|| {
            MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageCoverageBaseline,
                member_pointer.clone(),
                Some(value),
                LOCALE_BYTES_LIMIT,
            )
        })?;
        if locale.len() > LOCALE_BYTES_LIMIT {
            return Err(MessagesConfigViolation::with_limit(
                MessagesConfigReason::InvalidMessageCoverageBaseline,
                member_pointer,
                LOCALE_BYTES_LIMIT as u64,
                locale.len() as u64,
            ));
        }
        checked_locales.push(Locale::try_new(locale).map_err(|_| {
            MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageCoverageBaseline,
                member_pointer,
                Some(value),
                LOCALE_BYTES_LIMIT,
            )
        })?);
    }

    let mut total = 0_u64;
    for ((scope, _), locale) in members.iter().zip(&checked_locales) {
        total = total
            .checked_add(scope.len() as u64)
            .and_then(|value| value.checked_add(locale.as_str().len() as u64))
            .expect("bounded coverage-baseline members cannot overflow u64");
        if total > COVERAGE_BASELINE_BYTES_TOTAL_LIMIT {
            return Err(MessagesConfigViolation::with_limit(
                MessagesConfigReason::InvalidMessageCoverageBaseline,
                pointer,
                COVERAGE_BASELINE_BYTES_TOTAL_LIMIT,
                total,
            ));
        }
    }

    for (scope, _) in &members {
        if resources
            .linker_scopes()
            .binary_search_by(|candidate| candidate.as_str().as_bytes().cmp(scope.as_bytes()))
            .is_err()
        {
            return Err(MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageCoverageBaseline,
                pointer_property(pointer, scope),
                None,
                CONFIG_EVIDENCE_BYTES,
            ));
        }
    }
    for ((scope, value), locale) in members.iter().zip(&checked_locales) {
        if production_locales.binary_search(locale).is_err() {
            return Err(MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageCoverageBaseline,
                pointer_property(pointer, scope),
                Some(value),
                LOCALE_BYTES_LIMIT,
            ));
        }
    }

    let mut normalized = BTreeMap::new();
    let mut resolved = Vec::with_capacity(members.len());
    for (((scope, _), scope_id), locale) in
        members.into_iter().zip(checked_scopes).zip(checked_locales)
    {
        normalized.insert(scope.to_owned(), locale.as_str().to_owned());
        resolved.push(CoverageBaseline::new(scope_id, locale));
    }
    Ok((normalized, resolved))
}

fn validate_fallback(
    value: Option<&Value>,
    production_locales: &[Locale],
) -> Result<ValidatedFallbacks, MessagesConfigViolation> {
    let Some(value) = value else {
        return Ok((BTreeMap::new(), Vec::new()));
    };
    let pointer = "/messages/fallback";
    let object = value.as_object().ok_or_else(|| {
        MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageFallback,
            pointer,
            Some(value),
            CONFIG_EVIDENCE_BYTES,
        )
    })?;
    if object.len() > FALLBACK_SOURCES_LIMIT {
        return Err(MessagesConfigViolation::with_limit(
            MessagesConfigReason::InvalidMessageFallback,
            pointer,
            FALLBACK_SOURCES_LIMIT as u64,
            object.len() as u64,
        ));
    }

    // Decode every source before inspecting members so source-policy failures
    // have stable precedence over malformed target arrays.
    let mut sources = Vec::with_capacity(object.len());
    for (source, targets) in object {
        let source_pointer = pointer_property(pointer, source);
        if source.len() > LOCALE_BYTES_LIMIT {
            return Err(MessagesConfigViolation::with_limit(
                MessagesConfigReason::InvalidMessageFallback,
                pointer,
                LOCALE_BYTES_LIMIT as u64,
                source.len() as u64,
            ));
        }
        let locale = Locale::try_new(source.as_str()).map_err(|_| {
            MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageFallback,
                source_pointer,
                None,
                LOCALE_BYTES_LIMIT,
            )
        })?;
        sources.push((source, locale, targets));
    }
    for (source, locale, _) in &sources {
        if production_locales.binary_search(locale).is_err() {
            return Err(MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageFallback,
                pointer_property(pointer, source),
                None,
                LOCALE_BYTES_LIMIT,
            ));
        }
    }

    sources.sort_by(|left, right| left.1.cmp(&right.1));
    let mut arrays = Vec::with_capacity(sources.len());
    for (source, source_locale, value) in sources {
        let source_pointer = pointer_property(pointer, source);
        let values = value.as_array().ok_or_else(|| {
            MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageFallback,
                source_pointer.clone(),
                Some(value),
                CONFIG_EVIDENCE_BYTES,
            )
        })?;
        arrays.push((source, source_locale, values));
    }
    for (source, _, values) in &arrays {
        if values.len() > FALLBACK_TARGETS_PER_SOURCE_LIMIT {
            return Err(MessagesConfigViolation::with_limit(
                MessagesConfigReason::InvalidMessageFallback,
                pointer_property(pointer, source),
                FALLBACK_TARGETS_PER_SOURCE_LIMIT as u64,
                values.len() as u64,
            ));
        }
    }

    let mut decoded = Vec::with_capacity(arrays.len());
    for (source, source_locale, values) in arrays {
        let source_pointer = pointer_property(pointer, source);
        let mut targets = Vec::with_capacity(values.len());
        for (index, value) in values.iter().enumerate() {
            let target_pointer = pointer_index(&source_pointer, index);
            let target = value.as_str().ok_or_else(|| {
                MessagesConfigViolation::invalid(
                    MessagesConfigReason::InvalidMessageFallback,
                    target_pointer.clone(),
                    Some(value),
                    LOCALE_BYTES_LIMIT,
                )
            })?;
            if target.len() > LOCALE_BYTES_LIMIT {
                return Err(MessagesConfigViolation::with_limit(
                    MessagesConfigReason::InvalidMessageFallback,
                    target_pointer,
                    LOCALE_BYTES_LIMIT as u64,
                    target.len() as u64,
                ));
            }
            let locale = Locale::try_new(target).map_err(|_| {
                MessagesConfigViolation::invalid(
                    MessagesConfigReason::InvalidMessageFallback,
                    target_pointer,
                    Some(value),
                    LOCALE_BYTES_LIMIT,
                )
            })?;
            targets.push((target.to_owned(), locale));
        }
        decoded.push((source.to_owned(), source_locale, targets));
    }

    for (source, _, targets) in &decoded {
        let source_pointer = pointer_property(pointer, source);
        for (index, (target_spelling, target)) in targets.iter().enumerate() {
            if production_locales.binary_search(target).is_err() {
                let rejected = Value::String(target_spelling.clone());
                return Err(MessagesConfigViolation::invalid(
                    MessagesConfigReason::InvalidMessageFallback,
                    pointer_index(&source_pointer, index),
                    Some(&rejected),
                    LOCALE_BYTES_LIMIT,
                ));
            }
        }
    }
    if let Some((source, _, _)) = decoded.iter().find(|(_, _, targets)| targets.is_empty()) {
        return Err(MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageFallback,
            pointer_property(pointer, source),
            None,
            CONFIG_EVIDENCE_BYTES,
        ));
    }
    for (source, source_locale, targets) in &decoded {
        if let Some((index, (target_spelling, _))) = targets
            .iter()
            .enumerate()
            .find(|(_, (_, target))| target == source_locale)
        {
            let rejected = Value::String(target_spelling.clone());
            return Err(MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageFallback,
                pointer_index(&pointer_property(pointer, source), index),
                Some(&rejected),
                LOCALE_BYTES_LIMIT,
            ));
        }
    }
    for (source, _, targets) in &decoded {
        let source_pointer = pointer_property(pointer, source);
        let mut first_occurrences = HashMap::with_capacity(targets.len());
        for (index, (_, target)) in targets.iter().enumerate() {
            if let Some(first_index) = first_occurrences.insert(target.clone(), index) {
                return Err(MessagesConfigViolation::duplicate(
                    MessagesConfigReason::InvalidMessageFallback,
                    pointer_index(&source_pointer, index),
                    pointer_index(&source_pointer, first_index),
                ));
            }
        }
    }

    let mut normalized = BTreeMap::new();
    let mut resolved = Vec::with_capacity(decoded.len());
    for (source, source_locale, targets) in decoded {
        let (target_spellings, target_locales): (Vec<_>, Vec<_>) = targets.into_iter().unzip();
        normalized.insert(source, target_spellings);
        resolved.push(LocaleFallback::new(source_locale, target_locales));
    }
    Ok((normalized, resolved))
}

fn validate_roots(
    value: Option<&Value>,
    resources: &ResolvedResources,
) -> Result<ValidatedRoots, MessagesConfigViolation> {
    let Some(value) = value else {
        return Ok((Vec::new(), Vec::new(), Vec::new()));
    };
    let pointer = "/messages/roots";
    let values = value.as_array().ok_or_else(|| {
        MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageRoots,
            pointer,
            Some(value),
            CONFIG_EVIDENCE_BYTES,
        )
    })?;
    if values.len() > CONFIGURED_ROOTS_LIMIT {
        return Err(MessagesConfigViolation::with_limit(
            MessagesConfigReason::InvalidMessageRoots,
            pointer,
            CONFIGURED_ROOTS_LIMIT as u64,
            values.len() as u64,
        ));
    }

    let mut normalized = Vec::with_capacity(values.len());
    let mut resolved = Vec::with_capacity(values.len());
    let mut declarations = Vec::with_capacity(values.len());
    let mut first_occurrences = HashMap::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let root_pointer = pointer_index(pointer, index);
        let object = value.as_object().ok_or_else(|| {
            MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageRoots,
                root_pointer.clone(),
                Some(value),
                CONFIG_EVIDENCE_BYTES,
            )
        })?;
        if let Some((field, rejected)) =
            first_unknown_field(object, &["scope", "domain", "selector", "reason"])
        {
            return Err(MessagesConfigViolation::unknown(
                pointer_property(&root_pointer, field),
                field,
                rejected,
            ));
        }

        let scope_pointer = pointer_property(&root_pointer, "scope");
        let (scope, scope_id) = validate_linker_scope(
            object,
            &scope_pointer,
            MessagesConfigReason::InvalidMessageRoots,
            resources,
        )?;

        let domain_pointer = pointer_property(&root_pointer, "domain");
        let domain_token = required_string(
            object,
            "domain",
            &domain_pointer,
            MessagesConfigReason::InvalidMessageRoots,
            CONFIG_EVIDENCE_BYTES,
        )?;
        let domain = parse_domain(domain_token).ok_or_else(|| {
            MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageRoots,
                domain_pointer.clone(),
                object.get("domain"),
                CONFIG_EVIDENCE_BYTES,
            )
        })?;

        let selector_pointer = pointer_property(&root_pointer, "selector");
        let selector_value = object.get("selector").ok_or_else(|| {
            MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageRoots,
                selector_pointer.clone(),
                None,
                CONFIG_EVIDENCE_BYTES,
            )
        })?;
        let (selector, selector_config) =
            validate_root_selector(selector_value, &selector_pointer, domain)?;

        let reason_pointer = pointer_property(&root_pointer, "reason");
        let (reason, normalized_reason) = match object.get("reason") {
            None => (None, None),
            Some(value) => {
                let text = value.as_str().ok_or_else(|| {
                    MessagesConfigViolation::invalid(
                        MessagesConfigReason::InvalidMessageRoots,
                        reason_pointer.clone(),
                        Some(value),
                        CONFIG_EVIDENCE_BYTES,
                    )
                })?;
                let checked = ReasonText::try_new(text).map_err(|error| {
                    construction_violation(
                        MessagesConfigReason::InvalidMessageRoots,
                        reason_pointer.clone(),
                        value,
                        &error,
                        LinkLimitCounter::ReasonBytes.protocol_ceiling() as usize,
                    )
                })?;
                (Some(checked), Some(text.to_owned()))
            }
        };

        let identity = (scope.to_owned(), domain, selector_config.clone());
        if let Some(first_index) = first_occurrences.insert(identity, index) {
            return Err(MessagesConfigViolation::duplicate(
                MessagesConfigReason::InvalidMessageRoots,
                root_pointer,
                pointer_index(pointer, first_index),
            ));
        }
        let root = ConfiguredRoot::try_new(scope_id, domain.resolved(), selector, reason).map_err(
            |_| {
                MessagesConfigViolation::invalid(
                    MessagesConfigReason::InvalidMessageRoots,
                    selector_pointer,
                    Some(selector_value),
                    CONFIG_EVIDENCE_BYTES,
                )
            },
        )?;
        normalized.push(MessageRootConfig {
            scope: scope.to_owned(),
            domain,
            selector: selector_config,
            reason: normalized_reason,
        });
        declarations.push(ResolvedMessageDomainDeclaration {
            scope: root.scope().clone(),
            domain: root.domain(),
            pointer: Arc::from(domain_pointer),
        });
        resolved.push(root);
    }

    Ok((normalized, resolved, declarations))
}

fn validate_root_selector(
    value: &Value,
    pointer: &str,
    domain: MessageCatalogKeyDomain,
) -> Result<(MessageSelector, MessageSelectorConfig), MessagesConfigViolation> {
    let object = value.as_object().ok_or_else(|| {
        MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageRoots,
            pointer,
            Some(value),
            CONFIG_EVIDENCE_BYTES,
        )
    })?;
    if let Some((field, rejected)) =
        first_unknown_field(object, &["kind", "key", "prefix", "pattern"])
    {
        return Err(MessagesConfigViolation::unknown(
            pointer_property(pointer, field),
            field,
            rejected,
        ));
    }
    let kind_pointer = pointer_property(pointer, "kind");
    let kind = required_string(
        object,
        "kind",
        &kind_pointer,
        MessagesConfigReason::InvalidMessageRoots,
        CONFIG_EVIDENCE_BYTES,
    )?;
    let resolved_domain = domain.resolved();

    match kind {
        "exact" => {
            let payload_pointer = pointer_property(pointer, "key");
            let payload = required_string(
                object,
                "key",
                &payload_pointer,
                MessagesConfigReason::InvalidMessageRoots,
                LinkLimitCounter::CatalogKeyBytes.protocol_ceiling() as usize,
            )?;
            reject_selector_payloads(object, pointer, &["prefix", "pattern"])?;
            let key = CatalogKey::try_new(resolved_domain, payload).map_err(|error| {
                construction_violation(
                    MessagesConfigReason::InvalidMessageRoots,
                    payload_pointer,
                    object.get("key").expect("required payload is present"),
                    &error,
                    LinkLimitCounter::CatalogKeyBytes.protocol_ceiling() as usize,
                )
            })?;
            Ok((
                MessageSelector::Exact(key),
                MessageSelectorConfig::Exact {
                    key: payload.to_owned(),
                },
            ))
        }
        "prefix" => {
            let payload_pointer = pointer_property(pointer, "prefix");
            let payload = required_string(
                object,
                "prefix",
                &payload_pointer,
                MessagesConfigReason::InvalidMessageRoots,
                LinkLimitCounter::SelectorPathBytes.protocol_ceiling() as usize,
            )?;
            reject_selector_payloads(object, pointer, &["key", "pattern"])?;
            let prefix = CatalogKeyPrefix::try_new(resolved_domain, payload).map_err(|error| {
                construction_violation(
                    MessagesConfigReason::InvalidMessageRoots,
                    payload_pointer,
                    object.get("prefix").expect("required payload is present"),
                    &error,
                    LinkLimitCounter::SelectorPathBytes.protocol_ceiling() as usize,
                )
            })?;
            Ok((
                MessageSelector::Prefix(prefix),
                MessageSelectorConfig::Prefix {
                    prefix: payload.to_owned(),
                },
            ))
        }
        "pattern" => {
            let payload_pointer = pointer_property(pointer, "pattern");
            let payload = required_string(
                object,
                "pattern",
                &payload_pointer,
                MessagesConfigReason::InvalidMessageRoots,
                LinkLimitCounter::SelectorPatternBytes.protocol_ceiling() as usize,
            )?;
            reject_selector_payloads(object, pointer, &["key", "prefix"])?;
            let pattern =
                CatalogKeyPattern::try_new(resolved_domain, payload).map_err(|error| {
                    construction_violation(
                        MessagesConfigReason::InvalidMessageRoots,
                        payload_pointer,
                        object.get("pattern").expect("required payload is present"),
                        &error,
                        LinkLimitCounter::SelectorPatternBytes.protocol_ceiling() as usize,
                    )
                })?;
            Ok((
                MessageSelector::Pattern(pattern),
                MessageSelectorConfig::Pattern {
                    pattern: payload.to_owned(),
                },
            ))
        }
        "all-in-scope" => {
            reject_selector_payloads(object, pointer, &["key", "prefix", "pattern"])?;
            Ok((
                MessageSelector::AllInScope,
                MessageSelectorConfig::AllInScope,
            ))
        }
        _ => Err(MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageRoots,
            kind_pointer,
            object.get("kind"),
            CONFIG_EVIDENCE_BYTES,
        )),
    }
}

fn reject_selector_payloads(
    object: &Map<String, Value>,
    pointer: &str,
    fields: &[&str],
) -> Result<(), MessagesConfigViolation> {
    for field in fields {
        if let Some(value) = object.get(*field) {
            return Err(MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageRoots,
                pointer_property(pointer, field),
                Some(value),
                CONFIG_EVIDENCE_BYTES,
            ));
        }
    }
    Ok(())
}

fn root_config_order(left: &MessageRootConfig, right: &MessageRootConfig) -> std::cmp::Ordering {
    left.scope
        .as_bytes()
        .cmp(right.scope.as_bytes())
        .then_with(|| left.domain.cmp(&right.domain))
        .then_with(|| left.selector.cmp(&right.selector))
}

fn validate_producers(
    value: Option<&Value>,
    resources: &ResolvedResources,
) -> Result<ValidatedProducers, MessagesConfigViolation> {
    let Some(value) = value else {
        return Ok((
            MessageProducersConfig::default(),
            ResolvedMessageProducers::default(),
            Vec::new(),
        ));
    };
    let pointer = "/messages/producers";
    let object = value.as_object().ok_or_else(|| {
        MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageProducers,
            pointer,
            Some(value),
            CONFIG_EVIDENCE_BYTES,
        )
    })?;
    if let Some((field, rejected)) = first_unknown_field(object, &["js", "artifacts"]) {
        return Err(MessagesConfigViolation::unknown(
            pointer_property(pointer, field),
            field,
            rejected,
        ));
    }

    let (js, resolved_js, declarations) = match object.get("js") {
        None => (None, None, Vec::new()),
        Some(value) => {
            let (normalized, resolved, declarations) =
                validate_js_producer(value, "/messages/producers/js", resources)?;
            (Some(normalized), Some(resolved), declarations)
        }
    };
    let (artifacts, resolved_artifacts) = match object.get("artifacts") {
        None => (Vec::new(), Arc::from([])),
        Some(value) => validate_external_artifacts(value)?,
    };

    Ok((
        MessageProducersConfig { js, artifacts },
        ResolvedMessageProducers {
            js: resolved_js,
            artifacts: resolved_artifacts,
        },
        declarations,
    ))
}

fn validate_js_producer(
    value: &Value,
    pointer: &str,
    resources: &ResolvedResources,
) -> Result<ValidatedJsProducer, MessagesConfigViolation> {
    let object = value.as_object().ok_or_else(|| {
        MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageProducers,
            pointer,
            Some(value),
            CONFIG_EVIDENCE_BYTES,
        )
    })?;
    if let Some((field, rejected)) = first_unknown_field(object, &["include", "recognizers"]) {
        return Err(MessagesConfigViolation::unknown(
            pointer_property(pointer, field),
            field,
            rejected,
        ));
    }

    let include_pointer = pointer_property(pointer, "include");
    let include_value = object.get("include").ok_or_else(|| {
        MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageProducers,
            include_pointer.clone(),
            None,
            CONFIG_EVIDENCE_BYTES,
        )
    })?;
    let include_values = include_value.as_array().ok_or_else(|| {
        MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageProducers,
            include_pointer.clone(),
            Some(include_value),
            CONFIG_EVIDENCE_BYTES,
        )
    })?;
    if include_values.is_empty() {
        return Err(MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageProducers,
            include_pointer,
            Some(include_value),
            CONFIG_EVIDENCE_BYTES,
        ));
    }

    let mut include = Vec::with_capacity(include_values.len());
    let mut include_first = HashMap::with_capacity(include_values.len());
    for (index, value) in include_values.iter().enumerate() {
        let entry_pointer = pointer_index(&include_pointer, index);
        let pattern = value.as_str().ok_or_else(|| {
            MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageProducers,
                entry_pointer.clone(),
                Some(value),
                CONFIG_EVIDENCE_BYTES,
            )
        })?;
        let glob = ResourceGlob::parse(pattern).map_err(|_| {
            MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageProducers,
                entry_pointer.clone(),
                Some(value),
                CONFIG_EVIDENCE_BYTES,
            )
        })?;
        if let Some(first_index) = include_first.insert(pattern.to_owned(), index) {
            return Err(MessagesConfigViolation::duplicate(
                MessagesConfigReason::InvalidMessageProducers,
                entry_pointer,
                pointer_index(&include_pointer, first_index),
            ));
        }
        include.push(glob);
    }
    include.sort_by(|left, right| left.source().as_bytes().cmp(right.source().as_bytes()));

    let recognizers_pointer = pointer_property(pointer, "recognizers");
    let recognizers_value = object.get("recognizers").ok_or_else(|| {
        MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageProducers,
            recognizers_pointer.clone(),
            None,
            CONFIG_EVIDENCE_BYTES,
        )
    })?;
    let recognizer_values = recognizers_value.as_object().ok_or_else(|| {
        MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageProducers,
            recognizers_pointer.clone(),
            Some(recognizers_value),
            CONFIG_EVIDENCE_BYTES,
        )
    })?;
    if recognizer_values.is_empty() {
        return Err(MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageProducers,
            recognizers_pointer,
            Some(recognizers_value),
            CONFIG_EVIDENCE_BYTES,
        ));
    }

    let mut recognizer_names = recognizer_values.keys().collect::<Vec<_>>();
    recognizer_names.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    let mut normalized_recognizers = BTreeMap::new();
    let mut resolved_recognizers = Vec::with_capacity(recognizer_names.len());
    let mut declarations = Vec::with_capacity(recognizer_names.len());
    for callee in recognizer_names {
        let binding_pointer = pointer_property(&recognizers_pointer, callee);
        let binding_value = &recognizer_values[callee];
        let (normalized, resolved) =
            validate_js_recognizer(callee, binding_value, &binding_pointer, resources)?;
        normalized_recognizers.insert(callee.clone(), normalized);
        declarations.push(ResolvedMessageDomainDeclaration {
            scope: resolved.scope().clone(),
            domain: resolved.domain(),
            pointer: Arc::from(pointer_property(&binding_pointer, "domain")),
        });
        resolved_recognizers.push(resolved);
    }
    let recognizers = JsRecognizerSet::try_new(resolved_recognizers).map_err(|_| {
        MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageProducers,
            recognizers_pointer,
            Some(recognizers_value),
            CONFIG_EVIDENCE_BYTES,
        )
    })?;

    Ok((
        MessageJsProducerConfig {
            include: include.clone(),
            recognizers: normalized_recognizers,
        },
        ResolvedJsProducerConfig {
            include: Arc::from(include),
            recognizers,
        },
        declarations,
    ))
}

fn validate_js_recognizer(
    callee: &str,
    value: &Value,
    pointer: &str,
    resources: &ResolvedResources,
) -> Result<(MessageJsRecognizerConfig, JsRecognizerBinding), MessagesConfigViolation> {
    let object = value.as_object().ok_or_else(|| {
        MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageProducers,
            pointer,
            Some(value),
            CONFIG_EVIDENCE_BYTES,
        )
    })?;
    if let Some((field, rejected)) =
        first_unknown_field(object, &["kind", "scope", "domain", "keySyntax"])
    {
        return Err(MessagesConfigViolation::unknown(
            pointer_property(pointer, field),
            field,
            rejected,
        ));
    }

    let kind_pointer = pointer_property(pointer, "kind");
    let kind = match required_string(
        object,
        "kind",
        &kind_pointer,
        MessagesConfigReason::InvalidMessageProducers,
        CONFIG_EVIDENCE_BYTES,
    )? {
        "lookup" => MessageJsCallKind::Lookup,
        "set" => MessageJsCallKind::Set,
        _ => {
            return Err(MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageProducers,
                kind_pointer,
                object.get("kind"),
                CONFIG_EVIDENCE_BYTES,
            ));
        }
    };

    let scope_pointer = pointer_property(pointer, "scope");
    let (scope, scope_id) = validate_linker_scope(
        object,
        &scope_pointer,
        MessagesConfigReason::InvalidMessageProducers,
        resources,
    )?;

    let domain_pointer = pointer_property(pointer, "domain");
    let domain = parse_domain(required_string(
        object,
        "domain",
        &domain_pointer,
        MessagesConfigReason::InvalidMessageProducers,
        CONFIG_EVIDENCE_BYTES,
    )?)
    .ok_or_else(|| {
        MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageProducers,
            domain_pointer,
            object.get("domain"),
            CONFIG_EVIDENCE_BYTES,
        )
    })?;

    let syntax_pointer = pointer_property(pointer, "keySyntax");
    let key_syntax = match required_string(
        object,
        "keySyntax",
        &syntax_pointer,
        MessagesConfigReason::InvalidMessageProducers,
        CONFIG_EVIDENCE_BYTES,
    )? {
        "canonical" => MessageJsKeySyntax::Canonical,
        "literal" => MessageJsKeySyntax::Literal,
        "dot-path" => MessageJsKeySyntax::DotPath,
        _ => {
            return Err(MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageProducers,
                syntax_pointer,
                object.get("keySyntax"),
                CONFIG_EVIDENCE_BYTES,
            ));
        }
    };
    if !matches!(key_syntax, MessageJsKeySyntax::Canonical)
        && domain != MessageCatalogKeyDomain::JsonPointer
    {
        return Err(MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageProducers,
            syntax_pointer,
            object.get("keySyntax"),
            CONFIG_EVIDENCE_BYTES,
        ));
    }

    let resolved = JsRecognizerBinding::try_new(
        callee,
        kind.resolved(),
        scope_id,
        domain.resolved(),
        key_syntax.resolved(),
    )
    .map_err(|error| {
        let (limit, observed) = match error {
            intlify_producer_js::JsRecognizerConfigurationError::CalleeBytes {
                limit,
                observed,
            }
            | intlify_producer_js::JsRecognizerConfigurationError::CalleeSegments {
                limit,
                observed,
            } => (Some(limit), Some(observed)),
            _ => (None, None),
        };
        if let (Some(limit), Some(observed)) = (limit, observed) {
            MessagesConfigViolation::with_limit(
                MessagesConfigReason::InvalidMessageProducers,
                pointer,
                limit,
                observed,
            )
        } else {
            MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageProducers,
                pointer,
                None,
                CONFIG_EVIDENCE_BYTES,
            )
        }
    })?;

    Ok((
        MessageJsRecognizerConfig {
            kind,
            scope: scope.to_owned(),
            domain,
            key_syntax,
        },
        resolved,
    ))
}

fn validate_external_artifacts(
    value: &Value,
) -> Result<(Vec<String>, Arc<[PortableRelativePath]>), MessagesConfigViolation> {
    let pointer = "/messages/producers/artifacts";
    let values = value.as_array().ok_or_else(|| {
        MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageProducers,
            pointer,
            Some(value),
            CONFIG_EVIDENCE_BYTES,
        )
    })?;
    if values.is_empty() {
        return Err(MessagesConfigViolation::invalid(
            MessagesConfigReason::InvalidMessageProducers,
            pointer,
            Some(value),
            CONFIG_EVIDENCE_BYTES,
        ));
    }
    let count_limit = LinkLimitCounter::ReferenceArtifacts.protocol_ceiling() as usize;
    if values.len() > count_limit {
        return Err(MessagesConfigViolation::with_limit(
            MessagesConfigReason::InvalidMessageProducers,
            pointer,
            count_limit as u64,
            values.len() as u64,
        ));
    }

    let mut paths = Vec::with_capacity(values.len());
    let mut first_occurrences = HashMap::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let entry_pointer = pointer_index(pointer, index);
        let source = value.as_str().ok_or_else(|| {
            MessagesConfigViolation::invalid(
                MessagesConfigReason::InvalidMessageProducers,
                entry_pointer.clone(),
                Some(value),
                CONFIG_EVIDENCE_BYTES,
            )
        })?;
        let path = parse_external_artifact_path(source).map_err(|error| {
            construction_violation(
                MessagesConfigReason::InvalidMessageProducers,
                entry_pointer.clone(),
                value,
                &error,
                CONFIG_EVIDENCE_BYTES,
            )
        })?;
        if let Some(first_index) = first_occurrences.insert(source.to_owned(), index) {
            return Err(MessagesConfigViolation::duplicate(
                MessagesConfigReason::InvalidMessageProducers,
                entry_pointer,
                pointer_index(pointer, first_index),
            ));
        }
        paths.push((source.to_owned(), path));
    }
    paths.sort_by(|left, right| left.1.cmp(&right.1));
    let (normalized, resolved): (Vec<_>, Vec<_>) = paths.into_iter().unzip();
    Ok((normalized, Arc::from(resolved)))
}

fn parse_external_artifact_path(
    source: &str,
) -> Result<PortableRelativePath, ValueConstructionError> {
    if source.is_empty()
        || source.starts_with('/')
        || source.starts_with("\\\\")
        || has_windows_drive_prefix(source)
        || source.ends_with('/')
        || source
            .chars()
            .any(|character| matches!(character, '*' | '?' | '[' | ']' | '{' | '}' | '\\'))
    {
        return Err(ValueConstructionError::Grammar(
            intlify_contract::ValueGrammar::Invalid,
        ));
    }
    let segments = source
        .split('/')
        .map(PortablePathSegment::try_new)
        .collect::<Result<Vec<_>, _>>()?;
    PortableRelativePath::try_new(segments)
}

fn parse_domain(value: &str) -> Option<MessageCatalogKeyDomain> {
    match value {
        "json-pointer" => Some(MessageCatalogKeyDomain::JsonPointer),
        "yaml-typed-path" => Some(MessageCatalogKeyDomain::YamlTypedPath),
        "xliff-1.2" => Some(MessageCatalogKeyDomain::Xliff12),
        "xliff-2" => Some(MessageCatalogKeyDomain::Xliff2),
        _ => None,
    }
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    pointer: &str,
    reason: MessagesConfigReason,
    evidence_bytes: usize,
) -> Result<&'a str, MessagesConfigViolation> {
    let value = object
        .get(field)
        .ok_or_else(|| MessagesConfigViolation::invalid(reason, pointer, None, evidence_bytes))?;
    value.as_str().ok_or_else(|| {
        MessagesConfigViolation::invalid(reason, pointer, Some(value), evidence_bytes)
    })
}

fn validate_linker_scope<'a>(
    object: &'a Map<String, Value>,
    pointer: &str,
    reason: MessagesConfigReason,
    resources: &ResolvedResources,
) -> Result<(&'a str, CatalogScopeId), MessagesConfigViolation> {
    let scope = required_string(object, "scope", pointer, reason, SCOPE_BYTES_LIMIT)?;
    if scope.len() > SCOPE_BYTES_LIMIT {
        return Err(MessagesConfigViolation::with_limit(
            reason,
            pointer,
            SCOPE_BYTES_LIMIT as u64,
            scope.len() as u64,
        ));
    }
    let scope_name = ContractCatalogScopeName::try_new(scope).map_err(|_| {
        MessagesConfigViolation::invalid(reason, pointer, object.get("scope"), SCOPE_BYTES_LIMIT)
    })?;
    if resources
        .linker_scopes()
        .binary_search_by(|candidate| candidate.as_str().as_bytes().cmp(scope.as_bytes()))
        .is_err()
    {
        return Err(MessagesConfigViolation::invalid(
            reason,
            pointer,
            object.get("scope"),
            SCOPE_BYTES_LIMIT,
        ));
    }
    Ok((
        scope,
        CatalogScopeId::new(ArtifactNamespace::Project, scope_name),
    ))
}

fn policy_config_violation(error: LinkOperationalError) -> MessagesConfigViolation {
    let coverage_baseline_failure = match &error {
        LinkOperationalError::InvalidRequest(
            InvalidRequestError::DuplicateCoverageBaseline(_)
            | InvalidRequestError::CoverageBaselineLocaleNotProduction { .. }
            | InvalidRequestError::ResolvedCoverageBaselineConflict { .. },
        ) => true,
        LinkOperationalError::Limit(evidence) => matches!(
            evidence.counter(),
            LinkLimitCounter::CoverageBaselines | LinkLimitCounter::CoverageBaselineBytesTotal
        ),
        LinkOperationalError::InvalidRequest(_)
        | LinkOperationalError::UnsupportedContract(_)
        | LinkOperationalError::InternalInvariant => false,
    };
    let fallback_failure = match &error {
        LinkOperationalError::InvalidRequest(
            InvalidRequestError::DuplicateFallbackSource(_)
            | InvalidRequestError::FallbackSourceNotProduction(_)
            | InvalidRequestError::EmptyFallbackSequence(_)
            | InvalidRequestError::FallbackTargetNotProduction { .. }
            | InvalidRequestError::FallbackSelfReference(_)
            | InvalidRequestError::DuplicateFallbackTarget { .. },
        ) => true,
        LinkOperationalError::Limit(evidence) => matches!(
            evidence.counter(),
            LinkLimitCounter::FallbackSources | LinkLimitCounter::FallbackTargetsPerSource
        ),
        LinkOperationalError::InvalidRequest(_)
        | LinkOperationalError::UnsupportedContract(_)
        | LinkOperationalError::InternalInvariant => false,
    };
    let locale_failure = match error {
        LinkOperationalError::InvalidRequest(
            InvalidRequestError::EmptyProductionLocales
            | InvalidRequestError::DuplicateProductionLocale(_),
        ) => true,
        LinkOperationalError::Limit(evidence) => matches!(
            evidence.counter(),
            LinkLimitCounter::ProductionLocales | LinkLimitCounter::LocaleBytes
        ),
        LinkOperationalError::InvalidRequest(_)
        | LinkOperationalError::UnsupportedContract(_)
        | LinkOperationalError::InternalInvariant => false,
    };
    let (reason, pointer) = if coverage_baseline_failure {
        (
            MessagesConfigReason::InvalidMessageCoverageBaseline,
            "/messages/coverageBaseline",
        )
    } else if fallback_failure {
        (
            MessagesConfigReason::InvalidMessageFallback,
            "/messages/fallback",
        )
    } else if locale_failure {
        (
            MessagesConfigReason::InvalidMessageLocales,
            "/messages/locales",
        )
    } else {
        (MessagesConfigReason::InvalidMessageRoots, "/messages/roots")
    };
    MessagesConfigViolation::invalid(reason, pointer, None, CONFIG_EVIDENCE_BYTES)
}

fn construction_violation(
    reason: MessagesConfigReason,
    pointer: String,
    value: &Value,
    error: &ValueConstructionError,
    evidence_bytes: usize,
) -> MessagesConfigViolation {
    match error {
        ValueConstructionError::StructuralLimit(limit) => {
            MessagesConfigViolation::with_limit(reason, pointer, limit.limit(), limit.attempted())
        }
        ValueConstructionError::FieldLimit {
            limit, attempted, ..
        } => MessagesConfigViolation::with_limit(reason, pointer, *limit, *attempted),
        ValueConstructionError::Grammar(_) | ValueConstructionError::Range(_) => {
            MessagesConfigViolation::invalid(reason, pointer, Some(value), evidence_bytes)
        }
    }
}

fn first_unknown_field<'a>(
    object: &'a Map<String, Value>,
    known: &[&str],
) -> Option<(&'a str, &'a Value)> {
    object
        .iter()
        .filter(|(field, _)| !known.contains(&field.as_str()))
        .min_by(|(left, _), (right, _)| left.as_bytes().cmp(right.as_bytes()))
        .map(|(field, value)| (field.as_str(), value))
}

fn scalar_evidence(value: &Value, byte_limit: usize) -> Option<Value> {
    match value {
        Value::String(value) if value.len() <= byte_limit => Some(Value::String(value.clone())),
        Value::Bool(_) | Value::Number(_) if value.to_string().len() <= byte_limit => {
            Some(value.clone())
        }
        Value::Null
        | Value::String(_)
        | Value::Bool(_)
        | Value::Number(_)
        | Value::Array(_)
        | Value::Object(_) => None,
    }
}

fn pointer_property(base: &str, property: &str) -> String {
    let mut pointer = String::with_capacity(base.len() + property.len() + 1);
    pointer.push_str(base);
    pointer.push('/');
    pointer.push_str(&property.replace('~', "~0").replace('/', "~1"));
    pointer
}

fn pointer_index(base: &str, index: usize) -> String {
    format!("{base}/{index}")
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn has_windows_drive_prefix(source: &str) -> bool {
    let bytes = source.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

#[cfg(test)]
mod tests {
    use intlify_contract::{LinkLimitCounter, LinkLimits, Locale};
    use intlify_linker::{
        DynamicReferenceMode, InvalidRequestError, LinkOperationalError, LinkPolicy,
        PlacementPolicy,
    };
    use intlify_resource::ResourcesConfig;
    use serde_json::{json, Value};

    use super::{
        policy_config_violation, validate_messages_config, validate_roots, MessageCatalogKeyDomain,
        MessagesConfigError, MessagesConfigReason, COVERAGE_BASELINES_LIMIT,
        FALLBACK_SOURCES_LIMIT, FALLBACK_TARGETS_PER_SOURCE_LIMIT, SCOPE_BYTES_LIMIT,
    };

    fn resources(value: &Value) -> intlify_resource::ResolvedResources {
        ResourcesConfig::validate(Some(value)).unwrap().resolve()
    }

    fn app_resources() -> intlify_resource::ResolvedResources {
        resources(&json!({
            "catalogs": [
                {
                    "include": ["locales/*.json"],
                    "scope": "app",
                    "locale": { "from": "path", "pattern": "locales/{locale}.json" }
                },
                {
                    "include": ["vendor/*.json"],
                    "scope": "vendor",
                    "locale": { "from": "fixed", "value": "en" }
                },
                {
                    "include": ["drafts/*.json"]
                }
            ]
        }))
    }

    fn section_error(
        value: &Value,
        resources: &intlify_resource::ResolvedResources,
    ) -> super::MessagesConfigViolation {
        match validate_messages_config(Some(value), resources).unwrap_err() {
            MessagesConfigError::Section(violation) => violation,
            MessagesConfigError::CatalogLocaleNotProduction(_) => {
                panic!("section-local error expected")
            }
        }
    }

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn resolved_message_configuration_is_send_and_sync() {
        assert_send_sync::<super::MessagesConfig>();
        assert_send_sync::<super::ResolvedMessagesConfig>();
        assert_send_sync::<super::ResolvedMessageProducers>();
        assert_send_sync::<super::ResolvedJsProducerConfig>();
    }

    #[test]
    fn omission_constructs_no_policy_or_producer() {
        assert!(validate_messages_config(None, &app_resources())
            .unwrap()
            .is_none());
    }

    #[test]
    fn resolves_minimal_policy_with_canonical_locales_and_defaults() {
        let (normalized, resolved) =
            validate_messages_config(Some(&json!({ "locales": ["ja", "en"] })), &app_resources())
                .unwrap()
                .unwrap();

        assert_eq!(normalized.locales, ["en", "ja"]);
        assert_eq!(
            resolved.policy().dynamic_references(),
            DynamicReferenceMode::Compat
        );
        assert!(resolved.policy().configured_roots().is_empty());
        assert!(resolved.policy().coverage_baselines().is_empty());
        assert!(normalized.coverage_baseline().is_empty());
        assert!(resolved.policy().fallbacks().is_empty());
        assert!(normalized.fallback().is_empty());
        assert!(resolved.producers().js().is_none());
        assert!(resolved.producers().artifacts().is_empty());
    }

    #[test]
    fn policy_failures_are_attributed_to_the_owning_config_field() {
        let empty_locales = policy_config_violation(LinkOperationalError::InvalidRequest(
            InvalidRequestError::EmptyProductionLocales,
        ));
        assert_eq!(
            empty_locales.reason(),
            MessagesConfigReason::InvalidMessageLocales
        );
        assert_eq!(empty_locales.pointer(), "/messages/locales");

        let locale_limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::ProductionLocales, 0)
            .unwrap();
        let locale_limit = LinkPolicy::try_new(
            vec![Locale::try_new("en").unwrap()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &locale_limits,
        )
        .unwrap_err();
        let locale_limit = policy_config_violation(locale_limit);
        assert_eq!(
            locale_limit.reason(),
            MessagesConfigReason::InvalidMessageLocales
        );
        assert_eq!(locale_limit.pointer(), "/messages/locales");

        let (_, roots, _) = validate_roots(
            Some(&json!([{
                "scope": "app",
                "domain": "json-pointer",
                "selector": { "kind": "all-in-scope" }
            }])),
            &app_resources(),
        )
        .unwrap();
        let root_limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::ConfiguredRoots, 0)
            .unwrap();
        let root_failure = LinkPolicy::try_new(
            vec![Locale::try_new("en").unwrap()],
            Vec::new(),
            roots,
            Vec::new(),
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &root_limits,
        )
        .unwrap_err();
        let root_failure = policy_config_violation(root_failure);
        assert_eq!(
            root_failure.reason(),
            MessagesConfigReason::InvalidMessageRoots
        );
        assert_eq!(root_failure.pointer(), "/messages/roots");
    }

    #[test]
    fn explicit_defaults_equal_omission() {
        let resources = app_resources();
        let omitted = validate_messages_config(Some(&json!({ "locales": ["en"] })), &resources)
            .unwrap()
            .unwrap();
        let explicit = validate_messages_config(
            Some(&json!({
                "locales": ["en"],
                "dynamicReferences": "compat",
                "roots": [],
                "producers": {},
                "fallback": {}
            })),
            &resources,
        )
        .unwrap()
        .unwrap();

        assert_eq!(omitted, explicit);
    }

    #[test]
    fn delivery_is_canonical_and_conflicts_retain_submitted_out_pointers() {
        let resources = app_resources();
        let (normalized, resolved) = validate_messages_config(
            Some(&json!({
                "locales": ["ja", "en"],
                "delivery": {
                    "targets": [
                        {
                            "name": "web",
                            "exporter": "esm",
                            "out": "generated/web",
                            "eagerLocales": ["ja", "en"]
                        },
                        {
                            "name": "native",
                            "exporter": "esm",
                            "out": "generated/native",
                            "placement": "duplicate"
                        }
                    ]
                }
            })),
            &resources,
        )
        .unwrap()
        .unwrap();

        let targets = normalized.delivery().unwrap().targets();
        assert_eq!(targets[0].name(), "native");
        assert_eq!(targets[1].name(), "web");
        assert_eq!(targets[1].eager_locales(), ["en", "ja"]);
        assert_eq!(targets[0].placement(), "duplicate");
        assert_eq!(resolved.delivery().unwrap().targets().len(), 2);

        let conflict = section_error(
            &json!({
                "locales": ["en"],
                "delivery": {
                    "targets": [
                        { "name": "z", "exporter": "esm", "out": "dist" },
                        { "name": "a", "exporter": "esm", "out": "dist/web" }
                    ]
                }
            }),
            &resources,
        );
        assert_eq!(
            conflict.reason(),
            MessagesConfigReason::DeliveryOutputConflict
        );
        assert_eq!(conflict.pointer(), "/messages/delivery/targets");
        assert_eq!(conflict.first_target(), Some("a"));
        assert_eq!(conflict.second_target(), Some("z"));
        assert_eq!(
            conflict.first_out_pointer(),
            Some("/messages/delivery/targets/1/out")
        );
        assert_eq!(
            conflict.second_out_pointer(),
            Some("/messages/delivery/targets/0/out")
        );
        assert!(conflict.value().is_none());
    }

    #[test]
    fn delivery_validation_uses_field_family_precedence_and_narrow_evidence() {
        let resources = app_resources();
        let error = section_error(
            &json!({
                "locales": ["en"],
                "delivery": {
                    "targets": [
                        { "name": "ok", "exporter": null, "out": "dist/ok" },
                        { "name": "Bad", "exporter": "unknown", "out": "../bad" }
                    ]
                }
            }),
            &resources,
        );
        assert_eq!(error.reason(), MessagesConfigReason::InvalidMessageDelivery);
        assert_eq!(error.pointer(), "/messages/delivery/targets/1/name");
        assert_eq!(error.value(), Some(&Value::String("Bad".to_owned())));

        let exporter = section_error(
            &json!({
                "locales": ["en"],
                "delivery": {
                    "targets": [
                        { "name": "ok", "exporter": null, "out": "dist/ok" },
                        { "name": "also-ok", "exporter": "unknown", "out": "../bad" }
                    ]
                }
            }),
            &resources,
        );
        assert_eq!(exporter.pointer(), "/messages/delivery/targets/0/exporter");
        assert!(exporter.value().is_none());

        let duplicate = section_error(
            &json!({
                "locales": ["en"],
                "delivery": {
                    "targets": [
                        { "name": "web", "exporter": "esm", "out": "dist/a" },
                        { "name": "web", "exporter": "esm", "out": "dist/b" }
                    ]
                }
            }),
            &resources,
        );
        assert_eq!(duplicate.pointer(), "/messages/delivery/targets/1/name");
        assert_eq!(
            duplicate.first_pointer(),
            Some("/messages/delivery/targets/0/name")
        );
    }

    #[test]
    fn rejects_shape_and_unknown_fields_before_locales() {
        let resource = app_resources();
        let shape = section_error(&Value::Null, &resource);
        assert_eq!(
            shape.reason(),
            MessagesConfigReason::InvalidMessagesSectionShape
        );
        assert_eq!(shape.pointer(), "/messages");
        assert!(shape.value().is_none());

        let unknown = section_error(&json!({ "locales": null, "exporter": null }), &resource);
        assert_eq!(unknown.reason(), MessagesConfigReason::UnknownField);
        assert_eq!(unknown.pointer(), "/messages/exporter");
        assert_eq!(unknown.field(), Some("exporter"));

        let error = section_error(&json!({ "locales": ["en"], "exporter": {} }), &resource);
        assert_eq!(error.reason(), MessagesConfigReason::UnknownField);
        assert_eq!(error.pointer(), "/messages/exporter");
        assert_eq!(error.field(), Some("exporter"));

        let delivery = section_error(
            &json!({ "locales": ["en"], "fallback": null, "delivery": null }),
            &resource,
        );
        assert_eq!(
            delivery.reason(),
            MessagesConfigReason::InvalidMessageFallback
        );
        assert_eq!(delivery.pointer(), "/messages/fallback");
    }

    #[test]
    fn validates_and_canonicalizes_coverage_baseline_mapping() {
        let resources = app_resources();
        let (normalized, resolved) = validate_messages_config(
            Some(&json!({
                "locales": ["ja", "en"],
                "coverageBaseline": {
                    "vendor": "en",
                    "app": "ja"
                }
            })),
            &resources,
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            normalized
                .coverage_baseline()
                .iter()
                .map(|(scope, locale)| (scope.as_str(), locale.as_str()))
                .collect::<Vec<_>>(),
            [("app", "ja"), ("vendor", "en")]
        );
        assert_eq!(
            resolved
                .policy()
                .coverage_baselines()
                .iter()
                .map(|value| (value.scope().name().as_str(), value.locale().as_str()))
                .collect::<Vec<_>>(),
            [("app", "ja"), ("vendor", "en")]
        );
    }

    #[test]
    fn validates_fallbacks_with_canonical_sources_and_ordered_targets() {
        let resources = app_resources();
        let (normalized, resolved) = validate_messages_config(
            Some(&json!({
                "locales": ["ja", "en-US", "en"],
                "fallback": {
                    "ja": ["en"],
                    "en-US": ["en", "ja"]
                }
            })),
            &resources,
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            normalized
                .fallback()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["en-US", "ja"]
        );
        assert_eq!(normalized.fallback()["en-US"], ["en", "ja"]);
        assert_eq!(normalized.fallback()["ja"], ["en"]);
        assert_eq!(resolved.policy().fallbacks()[0].source().as_str(), "en-US");
        assert_eq!(
            resolved.policy().fallbacks()[0]
                .targets()
                .iter()
                .map(Locale::as_str)
                .collect::<Vec<_>>(),
            ["en", "ja"]
        );
    }

    #[test]
    fn omitted_and_explicitly_empty_fallbacks_have_the_same_checked_identity() {
        let resources = app_resources();
        let omitted =
            validate_messages_config(Some(&json!({ "locales": ["ja", "en"] })), &resources)
                .unwrap()
                .unwrap();
        let explicit = validate_messages_config(
            Some(&json!({ "locales": ["ja", "en"], "fallback": {} })),
            &resources,
        )
        .unwrap()
        .unwrap();

        assert_eq!(omitted, explicit);
        assert!(omitted.1.policy().fallbacks().is_empty());
    }

    #[test]
    fn fallback_rejects_shape_counts_and_relations_with_precise_pointers() {
        let resources = app_resources();
        let error = section_error(&json!({ "locales": ["en"], "fallback": [] }), &resources);
        assert_eq!(error.reason(), MessagesConfigReason::InvalidMessageFallback);
        assert_eq!(error.pointer(), "/messages/fallback");

        let first_overlong_source = format!("z{}", "z".repeat(255));
        let second_overlong_source = format!("a{}", "a".repeat(256));
        let error = section_error(
            &json!({
                "locales": ["en"],
                "fallback": {
                    (first_overlong_source): ["en"],
                    (second_overlong_source): ["en"]
                }
            }),
            &resources,
        );
        assert_eq!(error.pointer(), "/messages/fallback");
        assert_eq!(error.observed(), Some(256));

        let mut sources = serde_json::Map::new();
        for index in 0..=FALLBACK_SOURCES_LIMIT {
            sources.insert(format!("locale-{index:04}"), json!(["en"]));
        }
        let error = section_error(
            &json!({
                "locales": ["en"],
                "fallback": Value::Object(sources)
            }),
            &resources,
        );
        assert_eq!(error.limit(), Some(FALLBACK_SOURCES_LIMIT as u64));
        assert_eq!(error.observed(), Some((FALLBACK_SOURCES_LIMIT + 1) as u64));

        let oversized_targets = vec!["en"; FALLBACK_TARGETS_PER_SOURCE_LIMIT + 1];
        let error = section_error(
            &json!({
                "locales": ["en", "a", "z"],
                "fallback": {
                    "a": oversized_targets,
                    "z": null
                }
            }),
            &resources,
        );
        assert_eq!(error.pointer(), "/messages/fallback/z");

        let error = section_error(
            &json!({ "locales": ["en"], "fallback": { "ja": [] } }),
            &resources,
        );
        assert_eq!(error.pointer(), "/messages/fallback/ja");

        let error = section_error(
            &json!({ "locales": ["ja", "en"], "fallback": { "ja": [] } }),
            &resources,
        );
        assert_eq!(error.pointer(), "/messages/fallback/ja");

        let error = section_error(
            &json!({ "locales": ["ja", "en"], "fallback": { "ja": ["fr"] } }),
            &resources,
        );
        assert_eq!(error.pointer(), "/messages/fallback/ja/0");
        assert_eq!(error.value(), Some(&json!("fr")));

        let error = section_error(
            &json!({ "locales": ["ja", "en"], "fallback": { "ja": ["ja"] } }),
            &resources,
        );
        assert_eq!(error.pointer(), "/messages/fallback/ja/0");
        assert_eq!(error.value(), Some(&json!("ja")));

        let duplicate = section_error(
            &json!({ "locales": ["ja", "en"], "fallback": { "ja": ["en", "en"] } }),
            &resources,
        );
        assert_eq!(duplicate.pointer(), "/messages/fallback/ja/1");
        assert_eq!(duplicate.first_pointer(), Some("/messages/fallback/ja/0"));

        let too_many_targets = vec!["en"; FALLBACK_TARGETS_PER_SOURCE_LIMIT + 1];
        let error = section_error(
            &json!({
                "locales": ["ja", "en"],
                "fallback": { "ja": too_many_targets }
            }),
            &resources,
        );
        assert_eq!(
            error.limit(),
            Some(FALLBACK_TARGETS_PER_SOURCE_LIMIT as u64)
        );
        assert_eq!(
            error.observed(),
            Some((FALLBACK_TARGETS_PER_SOURCE_LIMIT + 1) as u64)
        );
    }

    #[test]
    fn coverage_baseline_rejects_shape_empty_and_count_before_members() {
        let resources = app_resources();
        for value in [Value::Null, json!([]), json!({})] {
            let error = section_error(
                &json!({ "locales": ["en"], "coverageBaseline": value }),
                &resources,
            );
            assert_eq!(
                error.reason(),
                MessagesConfigReason::InvalidMessageCoverageBaseline
            );
            assert_eq!(error.pointer(), "/messages/coverageBaseline");
        }

        let mut members = serde_json::Map::new();
        for index in 0..=COVERAGE_BASELINES_LIMIT {
            members.insert(format!("scope-{index:04}"), Value::String("en".to_owned()));
        }
        let error = section_error(
            &json!({
                "locales": ["en"],
                "coverageBaseline": Value::Object(members)
            }),
            &resources,
        );
        assert_eq!(
            error.reason(),
            MessagesConfigReason::InvalidMessageCoverageBaseline
        );
        assert_eq!(error.pointer(), "/messages/coverageBaseline");
        assert_eq!(error.limit(), Some(COVERAGE_BASELINES_LIMIT as u64));
        assert_eq!(
            error.observed(),
            Some((COVERAGE_BASELINES_LIMIT + 1) as u64)
        );
    }

    #[test]
    fn coverage_baseline_uses_bounded_member_evidence_and_fixed_membership_order() {
        let resources = app_resources();
        let overlong_scope = "s".repeat(SCOPE_BYTES_LIMIT + 1);
        let error = section_error(
            &json!({
                "locales": ["en"],
                "coverageBaseline": { (overlong_scope): "en" }
            }),
            &resources,
        );
        assert_eq!(error.pointer(), "/messages/coverageBaseline");
        assert_eq!(error.limit(), Some(SCOPE_BYTES_LIMIT as u64));
        assert_eq!(error.observed(), Some((SCOPE_BYTES_LIMIT + 1) as u64));
        assert!(error.value().is_none());

        let error = section_error(
            &json!({
                "locales": ["en"],
                "coverageBaseline": { "missing~scope/name": "en" }
            }),
            &resources,
        );
        assert_eq!(
            error.pointer(),
            "/messages/coverageBaseline/missing~0scope~1name"
        );
        assert!(error.value().is_none());

        let error = section_error(
            &json!({
                "locales": ["en"],
                "coverageBaseline": { "app": "ja" }
            }),
            &resources,
        );
        assert_eq!(error.pointer(), "/messages/coverageBaseline/app");
        assert_eq!(error.value(), Some(&json!("ja")));
    }

    #[test]
    fn validates_locale_counts_bytes_duplicates_and_canonical_order() {
        let resource = resources(&json!({}));
        for value in [
            json!({}),
            json!({ "locales": null }),
            json!({ "locales": [] }),
        ] {
            let error = section_error(&value, &resource);
            assert_eq!(error.reason(), MessagesConfigReason::InvalidMessageLocales);
            assert_eq!(error.pointer(), "/messages/locales");
        }

        let too_many = section_error(&json!({ "locales": vec!["en"; 1_025] }), &resource);
        assert_eq!(too_many.limit(), Some(1_024));
        assert_eq!(too_many.observed(), Some(1_025));

        let overlong = section_error(&json!({ "locales": ["a".repeat(256)] }), &resource);
        assert_eq!(overlong.pointer(), "/messages/locales/0");
        assert_eq!(overlong.limit(), Some(255));
        assert!(overlong.value().is_none());

        let duplicate = section_error(&json!({ "locales": ["en", "ja", "en"] }), &resource);
        assert_eq!(duplicate.pointer(), "/messages/locales/2");
        assert_eq!(duplicate.first_pointer(), Some("/messages/locales/0"));
        assert!(duplicate.value().is_none());
    }

    #[test]
    fn validates_every_supported_root_selector_and_canonicalizes_roots() {
        let (normalized, resolved) = validate_messages_config(
            Some(&json!({
                "locales": ["en"],
                "dynamicReferences": "strict",
                "roots": [
                    {
                        "scope": "vendor",
                        "domain": "json-pointer",
                        "selector": { "kind": "all-in-scope" }
                    },
                    {
                        "scope": "app",
                        "domain": "json-pointer",
                        "selector": { "kind": "pattern", "pattern": "/errors/*" }
                    },
                    {
                        "scope": "app",
                        "domain": "json-pointer",
                        "selector": { "kind": "prefix", "prefix": "/legal" }
                    },
                    {
                        "scope": "app",
                        "domain": "json-pointer",
                        "selector": { "kind": "exact", "key": "" },
                        "reason": "root message"
                    }
                ]
            })),
            &app_resources(),
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            resolved.policy().dynamic_references(),
            DynamicReferenceMode::Strict
        );
        assert_eq!(resolved.policy().configured_roots().len(), 4);
        assert_eq!(normalized.roots[0].scope, "app");
        assert_eq!(
            normalized.roots[0].domain,
            MessageCatalogKeyDomain::JsonPointer
        );
        assert_eq!(normalized.roots.last().unwrap().scope, "vendor");
    }

    #[test]
    fn rejects_invalid_root_scope_selector_and_duplicate_identity() {
        let resource = app_resources();
        let unknown_scope = section_error(
            &json!({
                "locales": ["en"],
                "roots": [{
                    "scope": "missing",
                    "domain": "json-pointer",
                    "selector": { "kind": "all-in-scope" }
                }]
            }),
            &resource,
        );
        assert_eq!(unknown_scope.pointer(), "/messages/roots/0/scope");

        let unbounded = section_error(
            &json!({
                "locales": ["en"],
                "roots": [{
                    "scope": "app",
                    "domain": "json-pointer",
                    "selector": { "kind": "unbounded-dynamic" }
                }]
            }),
            &resource,
        );
        assert_eq!(unbounded.pointer(), "/messages/roots/0/selector/kind");

        let duplicate = section_error(
            &json!({
                "locales": ["en"],
                "roots": [
                    {
                        "scope": "app",
                        "domain": "json-pointer",
                        "selector": { "kind": "exact", "key": "/a" },
                        "reason": "one"
                    },
                    {
                        "scope": "app",
                        "domain": "json-pointer",
                        "selector": { "kind": "exact", "key": "/a" },
                        "reason": "two"
                    }
                ]
            }),
            &resource,
        );
        assert_eq!(duplicate.pointer(), "/messages/roots/1");
        assert_eq!(duplicate.first_pointer(), Some("/messages/roots/0"));
    }

    #[test]
    fn resolves_js_producer_globs_and_recognizers_canonically() {
        let (normalized, resolved) = validate_messages_config(
            Some(&json!({
                "locales": ["en"],
                "producers": {
                    "js": {
                        "include": ["src/**/*.ts", "src/**/*.js"],
                        "recognizers": {
                            "useMessageSet": {
                                "kind": "set",
                                "scope": "app",
                                "domain": "json-pointer",
                                "keySyntax": "canonical"
                            },
                            "i18n.t": {
                                "kind": "lookup",
                                "scope": "app",
                                "domain": "json-pointer",
                                "keySyntax": "dot-path"
                            }
                        }
                    }
                }
            })),
            &app_resources(),
        )
        .unwrap()
        .unwrap();

        let js = normalized.producers.js.as_ref().unwrap();
        assert_eq!(js.include[0].source(), "src/**/*.js");
        assert_eq!(
            js.recognizers
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["i18n.t", "useMessageSet"]
        );
        let resolved_js = resolved.producers().js().unwrap();
        assert_eq!(resolved_js.recognizers().bindings()[0].callee(), "i18n.t");
    }

    #[test]
    fn rejects_missing_empty_or_incompatible_js_configuration() {
        let resource = app_resources();
        for (value, pointer) in [
            (
                json!({
                    "locales": ["en"],
                    "producers": { "js": { "recognizers": {} } }
                }),
                "/messages/producers/js/include",
            ),
            (
                json!({
                    "locales": ["en"],
                    "producers": { "js": { "include": [], "recognizers": {} } }
                }),
                "/messages/producers/js/include",
            ),
            (
                json!({
                    "locales": ["en"],
                    "producers": {
                        "js": {
                            "include": ["src/**/*.ts"],
                            "recognizers": {}
                        }
                    }
                }),
                "/messages/producers/js/recognizers",
            ),
        ] {
            assert_eq!(section_error(&value, &resource).pointer(), pointer);
        }

        let incompatible = section_error(
            &json!({
                "locales": ["en"],
                "producers": {
                    "js": {
                        "include": ["src/**/*.ts"],
                        "recognizers": {
                            "t": {
                                "kind": "lookup",
                                "scope": "app",
                                "domain": "yaml-typed-path",
                                "keySyntax": "dot-path"
                            }
                        }
                    }
                }
            }),
            &resource,
        );
        assert_eq!(
            incompatible.pointer(),
            "/messages/producers/js/recognizers/t/keySyntax"
        );
    }

    #[test]
    fn validates_js_include_and_recognizer_identity_evidence() {
        let resource = app_resources();
        let duplicate_include = section_error(
            &json!({
                "locales": ["en"],
                "producers": {
                    "js": {
                        "include": ["src/**/*.ts", "src/**/*.ts"],
                        "recognizers": {
                            "t": {
                                "kind": "lookup",
                                "scope": "app",
                                "domain": "json-pointer",
                                "keySyntax": "canonical"
                            }
                        }
                    }
                }
            }),
            &resource,
        );
        assert_eq!(
            duplicate_include.pointer(),
            "/messages/producers/js/include/1"
        );
        assert_eq!(
            duplicate_include.first_pointer(),
            Some("/messages/producers/js/include/0")
        );

        let invalid_include = section_error(
            &json!({
                "locales": ["en"],
                "producers": {
                    "js": {
                        "include": ["src/[name].ts"],
                        "recognizers": {
                            "t": {
                                "kind": "lookup",
                                "scope": "app",
                                "domain": "json-pointer",
                                "keySyntax": "canonical"
                            }
                        }
                    }
                }
            }),
            &resource,
        );
        assert_eq!(
            invalid_include.pointer(),
            "/messages/producers/js/include/0"
        );

        let valid_callee = "a".repeat(255);
        validate_messages_config(
            Some(&json!({
                "locales": ["en"],
                "producers": {
                    "js": {
                        "include": ["src/**/*.ts"],
                        "recognizers": {
                            (valid_callee): {
                                "kind": "lookup",
                                "scope": "app",
                                "domain": "json-pointer",
                                "keySyntax": "canonical"
                            }
                        }
                    }
                }
            })),
            &resource,
        )
        .unwrap();

        let invalid_callee = "a".repeat(256);
        let overlong = section_error(
            &json!({
                "locales": ["en"],
                "producers": {
                    "js": {
                        "include": ["src/**/*.ts"],
                        "recognizers": {
                            (invalid_callee): {
                                "kind": "lookup",
                                "scope": "app",
                                "domain": "json-pointer",
                                "keySyntax": "canonical"
                            }
                        }
                    }
                }
            }),
            &resource,
        );
        assert_eq!(overlong.limit(), Some(255));
        assert_eq!(overlong.observed(), Some(256));
        assert!(overlong.value().is_none());

        let reserved = section_error(
            &json!({
                "locales": ["en"],
                "producers": {
                    "js": {
                        "include": ["src/**/*.ts"],
                        "recognizers": {
                            "import.t": {
                                "kind": "lookup",
                                "scope": "app",
                                "domain": "json-pointer",
                                "keySyntax": "canonical"
                            }
                        }
                    }
                }
            }),
            &resource,
        );
        assert_eq!(
            reserved.pointer(),
            "/messages/producers/js/recognizers/import.t"
        );
    }

    #[test]
    fn validates_external_artifact_paths_duplicates_and_canonical_order() {
        let (normalized, resolved) = validate_messages_config(
            Some(&json!({
                "locales": ["en"],
                "producers": {
                    "artifacts": ["z/ref.json", "a/ref.json"]
                }
            })),
            &app_resources(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(normalized.producers.artifacts, ["a/ref.json", "z/ref.json"]);
        assert_eq!(resolved.producers().artifacts().len(), 2);

        for invalid in [
            "",
            "/absolute.json",
            "C:/absolute.json",
            "a/../ref.json",
            "a/*.json",
            r"a\ref.json",
        ] {
            let error = section_error(
                &json!({
                    "locales": ["en"],
                    "producers": { "artifacts": [invalid] }
                }),
                &app_resources(),
            );
            assert_eq!(error.pointer(), "/messages/producers/artifacts/0");
        }

        let duplicate = section_error(
            &json!({
                "locales": ["en"],
                "producers": { "artifacts": ["a/ref.json", "a/ref.json"] }
            }),
            &app_resources(),
        );
        assert_eq!(duplicate.pointer(), "/messages/producers/artifacts/1");
        assert_eq!(
            duplicate.first_pointer(),
            Some("/messages/producers/artifacts/0")
        );

        let boundary = "a".repeat(4_096);
        validate_messages_config(
            Some(&json!({
                "locales": ["en"],
                "producers": { "artifacts": [boundary] }
            })),
            &app_resources(),
        )
        .unwrap();

        let first_over = "a".repeat(4_097);
        let overlong = section_error(
            &json!({
                "locales": ["en"],
                "producers": { "artifacts": [first_over] }
            }),
            &app_resources(),
        );
        assert_eq!(overlong.limit(), Some(4_096));
        assert_eq!(overlong.observed(), Some(4_097));
        assert!(overlong.value().is_none());
    }

    #[test]
    fn rejects_fixed_catalog_locale_outside_the_production_set() {
        let error = validate_messages_config(Some(&json!({ "locales": ["ja"] })), &app_resources())
            .unwrap_err();
        let MessagesConfigError::CatalogLocaleNotProduction(violation) = error else {
            panic!("fixed locale cross-section violation expected");
        };
        assert_eq!(violation.definition_index(), 1);
        assert_eq!(violation.scope().as_str(), "vendor");
        assert_eq!(violation.locale().as_str(), "en");
    }

    #[test]
    fn admits_path_captures_against_the_resolved_production_set() {
        let resources = resources(&json!({
            "catalogs": [{
                "include": ["locales/*.json"],
                "scope": "app",
                "locale": { "from": "path", "pattern": "locales/{locale}.json" }
            }]
        }));
        let (_, messages) =
            validate_messages_config(Some(&json!({ "locales": ["ja"] })), &resources)
                .unwrap()
                .unwrap();
        let path =
            intlify_resource::ProjectRelativeResourcePath::try_from("locales/en.json").unwrap();
        let intlify_resource::LinkerCatalogResolution::Matched(assignment) =
            resources.resolve_linker_path(&path).unwrap()
        else {
            panic!("path-bound catalog should resolve");
        };

        let violation = assignment
            .validate_production_locale(|locale| messages.contains_production_locale(locale))
            .unwrap_err();
        assert_eq!(violation.definition_index(), 0);
        assert_eq!(violation.scope().as_str(), "app");
        assert_eq!(violation.locale().as_str(), "en");
    }
}
