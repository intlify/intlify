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
    ConfiguredRoot, DynamicReferenceMode, InvalidRequestError, LinkOperationalError, LinkPolicy,
    PlacementPolicy,
};
use intlify_producer_js::{
    JsKeySyntax, JsRecognizerBinding, JsRecognizerCallKind, JsRecognizerSet,
};
use intlify_resource::{CatalogLocaleNotProduction, ResolvedResources, ResourceGlob};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::Serialize;
use serde_json::{Map, Value};

const CONFIG_EVIDENCE_BYTES: usize = 255;
const LOCALE_BYTES_LIMIT: usize = 255;
const SCOPE_BYTES_LIMIT: usize = 255;
const PRODUCTION_LOCALES_LIMIT: usize = 1_024;
const CONFIGURED_ROOTS_LIMIT: usize = 4_096;

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

/// Immutable linker policy and producer configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMessagesConfig {
    policy: LinkPolicy,
    producers: ResolvedMessageProducers,
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
        &["locales", "dynamicReferences", "roots", "producers"],
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
    let (mut roots, resolved_roots) = validate_roots(section.get("roots"), resources)?;
    let (producers, resolved_producers) = validate_producers(section.get("producers"), resources)?;

    let limits = LinkLimits::default();
    let policy = LinkPolicy::try_new(
        resolved_locales,
        resolved_roots,
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

    roots.sort_by(root_config_order);
    Ok(Some((
        MessagesConfig {
            locales,
            dynamic_references,
            roots,
            producers,
        },
        ResolvedMessagesConfig {
            policy,
            producers: resolved_producers,
        },
    )))
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

fn validate_roots(
    value: Option<&Value>,
    resources: &ResolvedResources,
) -> Result<(Vec<MessageRootConfig>, Vec<ConfiguredRoot>), MessagesConfigViolation> {
    let Some(value) = value else {
        return Ok((Vec::new(), Vec::new()));
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
                domain_pointer,
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
        resolved.push(root);
    }

    Ok((normalized, resolved))
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
) -> Result<(MessageProducersConfig, ResolvedMessageProducers), MessagesConfigViolation> {
    let Some(value) = value else {
        return Ok((
            MessageProducersConfig::default(),
            ResolvedMessageProducers::default(),
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

    let (js, resolved_js) = match object.get("js") {
        None => (None, None),
        Some(value) => {
            let (normalized, resolved) =
                validate_js_producer(value, "/messages/producers/js", resources)?;
            (Some(normalized), Some(resolved))
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
    ))
}

fn validate_js_producer(
    value: &Value,
    pointer: &str,
    resources: &ResolvedResources,
) -> Result<(MessageJsProducerConfig, ResolvedJsProducerConfig), MessagesConfigViolation> {
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
    for callee in recognizer_names {
        let binding_pointer = pointer_property(&recognizers_pointer, callee);
        let binding_value = &recognizer_values[callee];
        let (normalized, resolved) =
            validate_js_recognizer(callee, binding_value, &binding_pointer, resources)?;
        normalized_recognizers.insert(callee.clone(), normalized);
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
    let (reason, pointer) = if locale_failure {
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
        MessagesConfigError, MessagesConfigReason,
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

        let (_, roots) = validate_roots(
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
            roots,
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
                "producers": {}
            })),
            &resources,
        )
        .unwrap()
        .unwrap();

        assert_eq!(omitted, explicit);
    }

    #[test]
    fn rejects_shape_and_future_fields_before_locales() {
        let resource = app_resources();
        let shape = section_error(&Value::Null, &resource);
        assert_eq!(
            shape.reason(),
            MessagesConfigReason::InvalidMessagesSectionShape
        );
        assert_eq!(shape.pointer(), "/messages");
        assert!(shape.value().is_none());

        let unknown = section_error(
            &json!({ "locales": null, "fallback": null, "coverageBaseline": {} }),
            &resource,
        );
        assert_eq!(unknown.reason(), MessagesConfigReason::UnknownField);
        assert_eq!(unknown.pointer(), "/messages/coverageBaseline");
        assert_eq!(unknown.field(), Some("coverageBaseline"));

        for (field, placeholder) in [
            ("coverageBaseline", json!({})),
            ("fallback", Value::Null),
            ("delivery", json!({})),
        ] {
            let error = section_error(
                &json!({ "locales": ["en"], (field): placeholder }),
                &resource,
            );
            assert_eq!(error.reason(), MessagesConfigReason::UnknownField);
            assert_eq!(error.pointer(), format!("/messages/{field}"));
            assert_eq!(error.field(), Some(field));
        }
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
