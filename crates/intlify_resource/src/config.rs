// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Resource catalog configuration validation and canonical resolution.
//!
//! This module owns project and overlay schema models, stable validation
//! evidence, include/exclude overlap semantics, format assignments, and the
//! resolved linker scope/locale registry. It classifies logical paths but does
//! not enumerate the filesystem or extract host documents.

use std::borrow::Cow;
use std::fmt;
use std::sync::Arc;

use schemars::consts::meta_schemas;
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::binding::{
    CatalogScopeName, LocaleBindingConfig, LocaleCaptureError, LocaleCapturePattern, ResolvedLocale,
};
use crate::glob::ResourceGlob;
use crate::registry::{
    classify_logical_path, HostFormat, HostFormatClassification, ResolvedCatalogAssignment,
    SUPPORTED_FORMATS,
};

/// Stable resource-configuration validation reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceConfigReason {
    /// A present project or overlay section is not an object.
    InvalidSectionShape,
    /// An object contains a field outside its fixed schema.
    UnknownField,
    /// A present `catalogs` value is not an array.
    InvalidCatalogsShape,
    /// One `catalogs` array element is not an object.
    InvalidCatalogDefinitionShape,
    /// `include` is missing, is not an array, or is empty.
    InvalidCatalogIncludeShape,
    /// A present `exclude` value is not an array.
    InvalidCatalogExcludeShape,
    /// One include or exclude entry is not a valid resource glob string.
    InvalidCatalogGlob,
    /// A present format is not an exact shipped host-format id.
    InvalidCatalogFormat,
    /// Exactly one member of the coordinated `scope` / `locale` pair is present.
    InvalidCatalogScopeLocalePair,
    /// A paired catalog scope is not an admitted exact resource scope.
    InvalidCatalogScope,
    /// A paired locale binding has an invalid shape, discriminator, or payload.
    InvalidCatalogLocaleBinding,
    /// A path locale binding has an invalid capture-pattern spelling.
    InvalidLocaleCapturePattern,
}

impl ResourceConfigReason {
    /// Return the stable machine-readable reason string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidSectionShape => "invalid_section_shape",
            Self::UnknownField => "unknown_field",
            Self::InvalidCatalogsShape => "invalid_catalogs_shape",
            Self::InvalidCatalogDefinitionShape => "invalid_catalog_definition_shape",
            Self::InvalidCatalogIncludeShape => "invalid_catalog_include_shape",
            Self::InvalidCatalogExcludeShape => "invalid_catalog_exclude_shape",
            Self::InvalidCatalogGlob => "invalid_catalog_glob",
            Self::InvalidCatalogFormat => "invalid_catalog_format",
            Self::InvalidCatalogScopeLocalePair => "invalid_catalog_scope_locale_pair",
            Self::InvalidCatalogScope => "invalid_catalog_scope",
            Self::InvalidCatalogLocaleBinding => "invalid_catalog_locale_binding",
            Self::InvalidLocaleCapturePattern => "invalid_locale_capture_pattern",
        }
    }
}

/// Path-independent evidence returned by resource configuration validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceConfigViolation {
    reason: ResourceConfigReason,
    pointer: Arc<str>,
    field: Option<Arc<str>>,
    value: Option<Value>,
}

impl ResourceConfigViolation {
    /// Return the stable validation reason.
    #[must_use]
    pub const fn reason(&self) -> ResourceConfigReason {
        self.reason
    }

    /// Return the RFC 6901 pointer within the normalized validation input.
    #[must_use]
    pub fn pointer(&self) -> &str {
        &self.pointer
    }

    /// Return the exact unknown field when the reason is `unknown_field`.
    #[must_use]
    pub fn field(&self) -> Option<&str> {
        self.field.as_deref()
    }

    /// Return rejected scalar evidence; arrays, objects, and missing values omit it.
    #[must_use]
    pub const fn value(&self) -> Option<&Value> {
        self.value.as_ref()
    }

    fn new(
        reason: ResourceConfigReason,
        pointer: String,
        rejected: Option<&Value>,
        field: Option<&str>,
    ) -> Self {
        Self {
            reason,
            pointer: Arc::from(pointer),
            field: field.map(Arc::from),
            value: rejected.and_then(scalar_evidence),
        }
    }
}

impl fmt::Display for ResourceConfigViolation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} at {}", self.reason.as_str(), self.pointer)
    }
}

impl std::error::Error for ResourceConfigViolation {}

/// Validated project resource configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct ResourcesConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        default,
        schema_with = "catalogs_schema",
        description = "Project-relative resource catalog definitions. Omission preserves absent policy; an empty array disables catalogs."
    )]
    catalogs: Option<Vec<CatalogConfig>>,
}

impl ResourcesConfig {
    /// Validate an omitted or exact present `resources` value.
    pub fn validate(value: Option<&Value>) -> Result<Self, ResourceConfigViolation> {
        let Some(value) = value else {
            return Ok(Self::default());
        };
        let catalogs = validate_catalog_section(value, "/resources", true)?;
        Ok(Self { catalogs })
    }

    /// Return catalog definitions while preserving absent versus present-empty policy.
    #[must_use]
    pub fn catalogs(&self) -> Option<&[CatalogConfig]> {
        self.catalogs.as_deref()
    }

    /// Compile this validated configuration into immutable lookup state.
    #[must_use]
    pub fn resolve(self) -> ResolvedResources {
        let policy = match self.catalogs {
            None => ResolvedCatalogPolicy::Absent,
            Some(catalogs) if catalogs.is_empty() => ResolvedCatalogPolicy::Empty,
            Some(catalogs) => ResolvedCatalogPolicy::Configured(resolve_definitions(catalogs)),
        };
        let linker_scopes = resolved_linker_scopes(&policy);
        ResolvedResources {
            policy,
            linker_scopes,
        }
    }
}

/// One validated resource catalog definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogConfig {
    include: Vec<ResourceGlob>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    exclude: Vec<ResourceGlob>,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<HostFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<CatalogScopeName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    locale: Option<LocaleBindingConfig>,
}

impl JsonSchema for CatalogConfig {
    fn schema_name() -> Cow<'static, str> {
        "CatalogConfig".into()
    }

    fn schema_id() -> Cow<'static, str> {
        concat!(module_path!(), "::CatalogConfig").into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let mut schema = CatalogConfigSchemaDefinition::json_schema(generator);
        schema.insert(
            "description".to_owned(),
            Value::String("One validated resource catalog definition.".to_owned()),
        );
        // The committed CLI schema is explicitly draft-07, while callers using
        // `schema_for!` receive Schemars' draft-2020-12 default. Preserve the
        // same mutual-presence constraint in both dialects.
        let dependency_keyword =
            if generator.settings().meta_schema.as_deref() == Some(meta_schemas::DRAFT07) {
                "dependencies"
            } else {
                "dependentRequired"
            };
        schema.insert(
            dependency_keyword.to_owned(),
            serde_json::json!({
                "scope": ["locale"],
                "locale": ["scope"]
            }),
        );
        schema
    }
}

impl CatalogConfig {
    /// Iterate validated include pattern spellings.
    pub fn include(&self) -> impl ExactSizeIterator<Item = &str> {
        self.include.iter().map(ResourceGlob::source)
    }

    /// Iterate validated exclude pattern spellings.
    pub fn exclude(&self) -> impl ExactSizeIterator<Item = &str> {
        self.exclude.iter().map(ResourceGlob::source)
    }

    /// Return the optional explicit shipped host format.
    #[must_use]
    pub const fn format(&self) -> Option<HostFormat> {
        self.format
    }

    /// Return the exact linker-participating scope when this definition is coordinated.
    #[must_use]
    pub const fn scope(&self) -> Option<&CatalogScopeName> {
        self.scope.as_ref()
    }

    /// Return the checked locale binding when this definition is coordinated.
    #[must_use]
    pub const fn locale(&self) -> Option<&LocaleBindingConfig> {
        self.locale.as_ref()
    }

    /// Return whether this definition contributes to linker catalog inventory.
    #[must_use]
    pub const fn is_linker_participating(&self) -> bool {
        self.scope.is_some()
    }
}

/// Validated additive editor catalog overlay without a `resources` wrapper.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct CatalogOverlayConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(default, schema_with = "overlay_catalogs_schema")]
    catalogs: Vec<CatalogConfig>,
}

impl CatalogOverlayConfig {
    /// Validate one normalized overlay object.
    pub fn validate(value: &Value) -> Result<Self, ResourceConfigViolation> {
        let catalogs = validate_catalog_section(value, "", false)?.unwrap_or_default();
        Ok(Self { catalogs })
    }

    /// Return the normalized overlay definitions.
    #[must_use]
    pub fn catalogs(&self) -> &[CatalogConfig] {
        &self.catalogs
    }

    /// Compile this validated overlay into immutable lookup state.
    #[must_use]
    pub fn resolve(self) -> ResolvedCatalogOverlay {
        ResolvedCatalogOverlay {
            definitions: resolve_definitions(self.catalogs),
        }
    }
}

fn catalogs_schema(generator: &mut SchemaGenerator) -> Schema {
    Vec::<CatalogConfig>::json_schema(generator)
}

#[derive(JsonSchema)]
#[schemars(deny_unknown_fields)]
#[allow(dead_code)]
struct CatalogConfigSchemaDefinition {
    #[schemars(
        length(min = 1),
        schema_with = "resource_glob_array_schema",
        description = "Non-empty project-relative resource membership patterns."
    )]
    include: Vec<String>,
    #[schemars(
        default,
        schema_with = "resource_glob_array_schema",
        description = "Project-relative patterns removed from this definition's include set."
    )]
    exclude: Vec<String>,
    #[schemars(
        default,
        schema_with = "host_format_schema",
        description = "Optional shipped host-format override."
    )]
    format: Option<HostFormat>,
    #[schemars(
        default,
        schema_with = "catalog_scope_schema",
        description = "Optional exact linker-participating scope. Must be paired with locale."
    )]
    scope: Option<String>,
    #[schemars(
        default,
        schema_with = "locale_binding_schema",
        description = "Optional path or fixed locale binding. Must be paired with scope."
    )]
    locale: Option<Value>,
}

fn overlay_catalogs_schema(generator: &mut SchemaGenerator) -> Schema {
    Vec::<CatalogOverlaySchemaDefinition>::json_schema(generator)
}

#[derive(JsonSchema)]
#[schemars(deny_unknown_fields)]
#[allow(dead_code)]
struct CatalogOverlaySchemaDefinition {
    #[schemars(length(min = 1), schema_with = "resource_glob_array_schema")]
    include: Vec<String>,
    #[schemars(default, schema_with = "resource_glob_array_schema")]
    exclude: Vec<String>,
    #[schemars(default, schema_with = "host_format_schema")]
    format: Option<HostFormat>,
}

fn resource_glob_array_schema(generator: &mut SchemaGenerator) -> Schema {
    Vec::<String>::json_schema(generator)
}

fn host_format_schema(_: &mut SchemaGenerator) -> Schema {
    schemars::json_schema!({
        "type": "string",
        "enum": SUPPORTED_FORMATS,
    })
}

fn catalog_scope_schema(_: &mut SchemaGenerator) -> Schema {
    schemars::json_schema!({
        "type": "string",
        "minLength": 1,
        "maxLength": 255,
    })
}

fn locale_binding_schema(_: &mut SchemaGenerator) -> Schema {
    schemars::json_schema!({
        "oneOf": [
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["from", "pattern"],
                "properties": {
                    "from": { "const": "path" },
                    "pattern": { "type": "string" }
                }
            },
            {
                "type": "object",
                "additionalProperties": false,
                "required": ["from", "value"],
                "properties": {
                    "from": { "const": "fixed" },
                    "value": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 255
                    }
                }
            }
        ]
    })
}

fn validate_catalog_section(
    value: &Value,
    base_pointer: &str,
    allow_linker_bindings: bool,
) -> Result<Option<Vec<CatalogConfig>>, ResourceConfigViolation> {
    let Some(section) = value.as_object() else {
        return Err(ResourceConfigViolation::new(
            ResourceConfigReason::InvalidSectionShape,
            base_pointer.to_owned(),
            Some(value),
            None,
        ));
    };

    if let Some((field, rejected)) = first_unknown_field(section, &["catalogs"]) {
        return Err(ResourceConfigViolation::new(
            ResourceConfigReason::UnknownField,
            pointer_property(base_pointer, field),
            Some(rejected),
            Some(field),
        ));
    }

    let Some(catalogs_value) = section.get("catalogs") else {
        return Ok(None);
    };
    let catalogs_pointer = pointer_property(base_pointer, "catalogs");
    let Some(catalogs) = catalogs_value.as_array() else {
        return Err(ResourceConfigViolation::new(
            ResourceConfigReason::InvalidCatalogsShape,
            catalogs_pointer,
            Some(catalogs_value),
            None,
        ));
    };

    catalogs
        .iter()
        .enumerate()
        .map(|(index, catalog)| {
            validate_catalog_definition(
                catalog,
                &pointer_index(&catalogs_pointer, index),
                allow_linker_bindings,
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn validate_catalog_definition(
    value: &Value,
    pointer: &str,
    allow_linker_bindings: bool,
) -> Result<CatalogConfig, ResourceConfigViolation> {
    let Some(definition) = value.as_object() else {
        return Err(ResourceConfigViolation::new(
            ResourceConfigReason::InvalidCatalogDefinitionShape,
            pointer.to_owned(),
            Some(value),
            None,
        ));
    };

    let known_fields: &[&str] = if allow_linker_bindings {
        &["include", "exclude", "format", "scope", "locale"]
    } else {
        &["include", "exclude", "format"]
    };
    if let Some((field, rejected)) = first_unknown_field(definition, known_fields) {
        return Err(ResourceConfigViolation::new(
            ResourceConfigReason::UnknownField,
            pointer_property(pointer, field),
            Some(rejected),
            Some(field),
        ));
    }

    let include_pointer = pointer_property(pointer, "include");
    let include_value = definition.get("include").ok_or_else(|| {
        ResourceConfigViolation::new(
            ResourceConfigReason::InvalidCatalogIncludeShape,
            include_pointer.clone(),
            None,
            None,
        )
    })?;
    let include_array = include_value.as_array().ok_or_else(|| {
        ResourceConfigViolation::new(
            ResourceConfigReason::InvalidCatalogIncludeShape,
            include_pointer.clone(),
            Some(include_value),
            None,
        )
    })?;
    if include_array.is_empty() {
        return Err(ResourceConfigViolation::new(
            ResourceConfigReason::InvalidCatalogIncludeShape,
            include_pointer,
            Some(include_value),
            None,
        ));
    }
    let include = validate_glob_array(include_array, &include_pointer)?;

    let exclude_pointer = pointer_property(pointer, "exclude");
    let exclude = match definition.get("exclude") {
        None => Vec::new(),
        Some(exclude_value) => {
            let Some(exclude_array) = exclude_value.as_array() else {
                return Err(ResourceConfigViolation::new(
                    ResourceConfigReason::InvalidCatalogExcludeShape,
                    exclude_pointer,
                    Some(exclude_value),
                    None,
                ));
            };
            validate_glob_array(exclude_array, &exclude_pointer)?
        }
    };

    let format_pointer = pointer_property(pointer, "format");
    let format = match definition.get("format") {
        None => None,
        Some(format_value) => {
            let Some(format) = format_value.as_str() else {
                return Err(ResourceConfigViolation::new(
                    ResourceConfigReason::InvalidCatalogFormat,
                    format_pointer,
                    Some(format_value),
                    None,
                ));
            };
            if format != "json" {
                return Err(ResourceConfigViolation::new(
                    ResourceConfigReason::InvalidCatalogFormat,
                    format_pointer,
                    Some(format_value),
                    None,
                ));
            }
            Some(HostFormat::Json)
        }
    };

    let has_scope = definition.contains_key("scope");
    let has_locale = definition.contains_key("locale");
    if has_scope != has_locale {
        return Err(ResourceConfigViolation::new(
            ResourceConfigReason::InvalidCatalogScopeLocalePair,
            pointer.to_owned(),
            None,
            None,
        ));
    }

    let (scope, locale) = if has_scope {
        let scope_pointer = pointer_property(pointer, "scope");
        let scope_value = definition
            .get("scope")
            .expect("presence was checked before interpretation");
        let scope_string = scope_value.as_str().ok_or_else(|| {
            ResourceConfigViolation::new(
                ResourceConfigReason::InvalidCatalogScope,
                scope_pointer.clone(),
                Some(scope_value),
                None,
            )
        })?;
        let scope = CatalogScopeName::try_new(scope_string).map_err(|_| {
            ResourceConfigViolation::new(
                ResourceConfigReason::InvalidCatalogScope,
                scope_pointer,
                Some(scope_value),
                None,
            )
        })?;
        let locale = validate_locale_binding(
            definition
                .get("locale")
                .expect("paired presence was checked before interpretation"),
            &pointer_property(pointer, "locale"),
        )?;
        (Some(scope), Some(locale))
    } else {
        (None, None)
    };

    Ok(CatalogConfig {
        include,
        exclude,
        format,
        scope,
        locale,
    })
}

fn validate_locale_binding(
    value: &Value,
    pointer: &str,
) -> Result<LocaleBindingConfig, ResourceConfigViolation> {
    let Some(binding) = value.as_object() else {
        return Err(ResourceConfigViolation::new(
            ResourceConfigReason::InvalidCatalogLocaleBinding,
            pointer.to_owned(),
            Some(value),
            None,
        ));
    };

    if let Some((field, rejected)) = first_unknown_field(binding, &["from", "pattern", "value"]) {
        return Err(ResourceConfigViolation::new(
            ResourceConfigReason::UnknownField,
            pointer_property(pointer, field),
            Some(rejected),
            Some(field),
        ));
    }

    let from_pointer = pointer_property(pointer, "from");
    let from_value = binding.get("from").ok_or_else(|| {
        ResourceConfigViolation::new(
            ResourceConfigReason::InvalidCatalogLocaleBinding,
            from_pointer.clone(),
            None,
            None,
        )
    })?;
    let discriminator = from_value.as_str().ok_or_else(|| {
        ResourceConfigViolation::new(
            ResourceConfigReason::InvalidCatalogLocaleBinding,
            from_pointer.clone(),
            Some(from_value),
            None,
        )
    })?;

    match discriminator {
        "path" => {
            let pattern_pointer = pointer_property(pointer, "pattern");
            let pattern_value = binding.get("pattern").ok_or_else(|| {
                ResourceConfigViolation::new(
                    ResourceConfigReason::InvalidCatalogLocaleBinding,
                    pattern_pointer.clone(),
                    None,
                    None,
                )
            })?;
            if let Some(value) = binding.get("value") {
                return Err(ResourceConfigViolation::new(
                    ResourceConfigReason::InvalidCatalogLocaleBinding,
                    pointer_property(pointer, "value"),
                    Some(value),
                    None,
                ));
            }
            let pattern = pattern_value.as_str().ok_or_else(|| {
                ResourceConfigViolation::new(
                    ResourceConfigReason::InvalidCatalogLocaleBinding,
                    pattern_pointer.clone(),
                    Some(pattern_value),
                    None,
                )
            })?;
            let pattern = LocaleCapturePattern::parse(pattern).map_err(|_| {
                ResourceConfigViolation::new(
                    ResourceConfigReason::InvalidLocaleCapturePattern,
                    pattern_pointer,
                    Some(pattern_value),
                    None,
                )
            })?;
            Ok(LocaleBindingConfig::Path { pattern })
        }
        "fixed" => {
            let value_pointer = pointer_property(pointer, "value");
            let fixed_value = binding.get("value").ok_or_else(|| {
                ResourceConfigViolation::new(
                    ResourceConfigReason::InvalidCatalogLocaleBinding,
                    value_pointer.clone(),
                    None,
                    None,
                )
            })?;
            if let Some(pattern) = binding.get("pattern") {
                return Err(ResourceConfigViolation::new(
                    ResourceConfigReason::InvalidCatalogLocaleBinding,
                    pointer_property(pointer, "pattern"),
                    Some(pattern),
                    None,
                ));
            }
            let fixed = fixed_value.as_str().ok_or_else(|| {
                ResourceConfigViolation::new(
                    ResourceConfigReason::InvalidCatalogLocaleBinding,
                    value_pointer.clone(),
                    Some(fixed_value),
                    None,
                )
            })?;
            let value = ResolvedLocale::try_new(fixed).map_err(|_| {
                ResourceConfigViolation::new(
                    ResourceConfigReason::InvalidCatalogLocaleBinding,
                    value_pointer,
                    Some(fixed_value),
                    None,
                )
            })?;
            Ok(LocaleBindingConfig::Fixed { value })
        }
        _ => Err(ResourceConfigViolation::new(
            ResourceConfigReason::InvalidCatalogLocaleBinding,
            from_pointer,
            Some(from_value),
            None,
        )),
    }
}

fn validate_glob_array(
    values: &[Value],
    pointer: &str,
) -> Result<Vec<ResourceGlob>, ResourceConfigViolation> {
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let entry_pointer = pointer_index(pointer, index);
            let pattern = value.as_str().ok_or_else(|| {
                ResourceConfigViolation::new(
                    ResourceConfigReason::InvalidCatalogGlob,
                    entry_pointer.clone(),
                    Some(value),
                    None,
                )
            })?;
            ResourceGlob::parse(pattern).map_err(|_| {
                ResourceConfigViolation::new(
                    ResourceConfigReason::InvalidCatalogGlob,
                    entry_pointer,
                    Some(value),
                    None,
                )
            })
        })
        .collect()
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

fn scalar_evidence(value: &Value) -> Option<Value> {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => Some(value.clone()),
        Value::Array(_) | Value::Object(_) => None,
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

/// Validation failure when constructing a project-relative resource path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectRelativeResourcePathError {
    /// The supplied path has no segments.
    Empty,
    /// The supplied path is absolute, drive-prefixed, or UNC-prefixed.
    NotRelative,
    /// The supplied path contains an empty, `.` or `..` segment.
    InvalidSegment,
}

impl ProjectRelativeResourcePathError {
    /// Return a stable internal reason useful to consumer assertions.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::NotRelative => "not_relative",
            Self::InvalidSegment => "invalid_segment",
        }
    }
}

impl fmt::Display for ProjectRelativeResourcePathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::error::Error for ProjectRelativeResourcePathError {}

/// Exact slash-normalized Unicode path lexically relative to the project root.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectRelativeResourcePath(Arc<str>);

impl ProjectRelativeResourcePath {
    /// Validate and retain a project-relative logical path.
    pub fn new(path: impl Into<Arc<str>>) -> Result<Self, ProjectRelativeResourcePathError> {
        let path = path.into();
        validate_project_relative_path(&path)?;
        Ok(Self(path))
    }

    /// Return the exact retained slash-normalized spelling.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for ProjectRelativeResourcePath {
    type Error = ProjectRelativeResourcePathError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(Arc::<str>::from(value))
    }
}

fn validate_project_relative_path(path: &str) -> Result<(), ProjectRelativeResourcePathError> {
    if path.is_empty() {
        return Err(ProjectRelativeResourcePathError::Empty);
    }
    if path.starts_with('/') || path.starts_with("\\\\") || has_windows_drive_prefix(path) {
        return Err(ProjectRelativeResourcePathError::NotRelative);
    }
    if path
        .split('/')
        .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
    {
        return Err(ProjectRelativeResourcePathError::InvalidSegment);
    }
    Ok(())
}

fn has_windows_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// Observable project catalog-policy state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CatalogPolicyState {
    /// Neither `resources` nor `resources.catalogs` established project policy.
    Absent,
    /// An explicit empty `resources.catalogs` array disables project catalogs.
    Empty,
    /// At least one validated project catalog definition is compiled.
    Configured,
}

/// Immutable compiled project resource configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedResources {
    policy: ResolvedCatalogPolicy,
    linker_scopes: Arc<[CatalogScopeName]>,
}

impl Default for ResolvedResources {
    fn default() -> Self {
        ResourcesConfig::default().resolve()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ResolvedCatalogPolicy {
    Absent,
    Empty,
    Configured(Arc<[ResolvedCatalogDefinition]>),
}

/// Immutable compiled editor overlay.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedCatalogOverlay {
    definitions: Arc<[ResolvedCatalogDefinition]>,
}

/// Read-only linker participation attached to one compiled catalog definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCatalogBinding {
    scope: CatalogScopeName,
    locale: LocaleBindingConfig,
}

impl ResolvedCatalogBinding {
    /// Return the exact project-local scope.
    #[must_use]
    pub const fn scope(&self) -> &CatalogScopeName {
        &self.scope
    }

    /// Return the checked locale assignment strategy.
    #[must_use]
    pub const fn locale(&self) -> &LocaleBindingConfig {
        &self.locale
    }
}

/// One immutable compiled project or overlay catalog definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCatalogDefinition {
    index: usize,
    include: Arc<[ResourceGlob]>,
    exclude: Arc<[ResourceGlob]>,
    format: Option<HostFormat>,
    binding: Option<ResolvedCatalogBinding>,
}

impl ResolvedCatalogDefinition {
    /// Return the zero-based source-array index.
    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }

    /// Iterate exact validated include patterns.
    pub fn include(&self) -> impl ExactSizeIterator<Item = &ResourceGlob> {
        self.include.iter()
    }

    /// Iterate exact validated exclude patterns.
    pub fn exclude(&self) -> impl ExactSizeIterator<Item = &ResourceGlob> {
        self.exclude.iter()
    }

    /// Return the optional explicit shipped host format.
    #[must_use]
    pub const fn format(&self) -> Option<HostFormat> {
        self.format
    }

    /// Return linker participation, or `None` for an entry-level-only definition.
    #[must_use]
    pub const fn binding(&self) -> Option<&ResolvedCatalogBinding> {
        self.binding.as_ref()
    }
}

fn resolve_definitions(catalogs: Vec<CatalogConfig>) -> Arc<[ResolvedCatalogDefinition]> {
    catalogs
        .into_iter()
        .enumerate()
        .map(|(index, catalog)| {
            let binding = catalog
                .scope
                .zip(catalog.locale)
                .map(|(scope, locale)| ResolvedCatalogBinding { scope, locale });
            ResolvedCatalogDefinition {
                index,
                include: Arc::from(catalog.include),
                exclude: Arc::from(catalog.exclude),
                format: catalog.format,
                binding,
            }
        })
        .collect::<Vec<_>>()
        .into()
}

fn resolved_linker_scopes(policy: &ResolvedCatalogPolicy) -> Arc<[CatalogScopeName]> {
    let ResolvedCatalogPolicy::Configured(definitions) = policy else {
        return Arc::from([]);
    };
    let mut scopes = definitions
        .iter()
        .filter_map(|definition| {
            definition
                .binding
                .as_ref()
                .map(|binding| binding.scope.clone())
        })
        .collect::<Vec<_>>();
    scopes.sort_unstable();
    scopes.dedup();
    Arc::from(scopes)
}

/// Project-only membership result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogResolution {
    /// Project catalog policy is absent for every path.
    PolicyAbsent,
    /// Project catalogs are explicitly disabled for every path.
    PolicyEmpty,
    /// No configured definition includes the path.
    Unmatched,
    /// Definitions include the path, but every including definition excludes it.
    Excluded,
    /// At least one including definition survives its own excludes.
    Matched(ResolvedCatalogAssignment),
}

/// Project catalog resolution with linker participation made explicit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkerCatalogResolution {
    /// Project catalog policy is absent for every path.
    PolicyAbsent,
    /// Project catalogs are explicitly disabled for every path.
    PolicyEmpty,
    /// No configured definition includes the path.
    Unmatched,
    /// Definitions include the path, but every including definition excludes it.
    Excluded,
    /// The path is a resource input, but no surviving definition is linker-participating.
    EntryLevelOnly(ResolvedCatalogAssignment),
    /// At least one surviving definition supplies one coherent scope and locale.
    Matched(ResolvedLinkerCatalogAssignment),
}

/// One resolved catalog assignment admitted for linker definition inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLinkerCatalogAssignment {
    catalog: ResolvedCatalogAssignment,
    scope: CatalogScopeName,
    locale: ResolvedLocale,
    participating_definitions: Arc<[CatalogDefinitionRef]>,
}

impl ResolvedLinkerCatalogAssignment {
    /// Borrow the ordinary resource format and membership assignment.
    #[must_use]
    pub const fn catalog(&self) -> &ResolvedCatalogAssignment {
        &self.catalog
    }

    /// Return the one exact coherent linker scope.
    #[must_use]
    pub const fn scope(&self) -> &CatalogScopeName {
        &self.scope
    }

    /// Return the one exact coherent locale resolved for this logical path.
    #[must_use]
    pub const fn locale(&self) -> &ResolvedLocale {
        &self.locale
    }

    /// Return surviving definitions that contributed linker bindings.
    #[must_use]
    pub fn participating_definitions(&self) -> &[CatalogDefinitionRef] {
        &self.participating_definitions
    }

    /// Require this concrete exact locale to belong to a production set.
    ///
    /// The caller supplies the already resolved messages policy membership
    /// check. A failure retains the first contributing definition so the host
    /// integration can attach its path-specific pointer and target evidence.
    pub fn validate_production_locale(
        &self,
        mut is_production: impl FnMut(&str) -> bool,
    ) -> Result<(), CatalogLocaleNotProduction> {
        if is_production(self.locale.as_str()) {
            return Ok(());
        }
        let definition = self
            .participating_definitions
            .first()
            .expect("a linker assignment has at least one participating definition");
        Err(CatalogLocaleNotProduction {
            definition_index: definition.definition_index(),
            scope: self.scope.clone(),
            locale: self.locale.clone(),
        })
    }
}

/// Source layer attached to catalog definition evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CatalogAssignmentOrigin {
    /// A project `resources.catalogs` definition supplied the assignment.
    Project,
    /// An additive editor overlay definition supplied the assignment.
    Overlay,
}

/// Read-only source-qualified definition identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CatalogDefinitionRef {
    origin: CatalogAssignmentOrigin,
    definition_index: usize,
}

impl CatalogDefinitionRef {
    /// Return the project or overlay source layer.
    #[must_use]
    pub const fn origin(self) -> CatalogAssignmentOrigin {
        self.origin
    }

    /// Return the zero-based definition index within that layer.
    #[must_use]
    pub const fn definition_index(self) -> usize {
        self.definition_index
    }

    const fn new(origin: CatalogAssignmentOrigin, definition_index: usize) -> Self {
        Self {
            origin,
            definition_index,
        }
    }
}

/// Deterministic two-definition format assignment conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogAssignmentConflict {
    assignment: CatalogDefinitionRef,
    conflicting_assignment: CatalogDefinitionRef,
}

/// Binding component selected when two linker-participating definitions disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CatalogBindingConflictField {
    /// The exact project-local scope differs.
    Scope,
    /// The exact resolved locale differs.
    Locale,
}

/// Deterministic same-path linker binding conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogBindingConflict {
    field: CatalogBindingConflictField,
    assignment: CatalogDefinitionRef,
    conflicting_assignment: CatalogDefinitionRef,
}

impl CatalogBindingConflict {
    /// Return the first unequal field under scope-before-locale precedence.
    #[must_use]
    pub const fn field(self) -> CatalogBindingConflictField {
        self.field
    }

    /// Return the later decisive conflicting definition.
    #[must_use]
    pub const fn assignment(self) -> CatalogDefinitionRef {
        self.assignment
    }

    /// Return the earliest definition that established the expected binding.
    #[must_use]
    pub const fn conflicting_assignment(self) -> CatalogDefinitionRef {
        self.conflicting_assignment
    }
}

/// One path capture failure tied to its catalog definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogLocaleAssignmentError {
    definition: CatalogDefinitionRef,
    error: LocaleCaptureError,
}

/// One resolved catalog locale outside the production locale set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogLocaleNotProduction {
    definition_index: usize,
    scope: CatalogScopeName,
    locale: ResolvedLocale,
}

impl CatalogLocaleNotProduction {
    /// Return the project catalog definition index.
    #[must_use]
    pub const fn definition_index(&self) -> usize {
        self.definition_index
    }

    /// Return the exact resource-owned scope.
    #[must_use]
    pub const fn scope(&self) -> &CatalogScopeName {
        &self.scope
    }

    /// Return the exact resource-owned resolved locale.
    #[must_use]
    pub const fn locale(&self) -> &ResolvedLocale {
        &self.locale
    }
}

impl CatalogLocaleAssignmentError {
    /// Return the definition whose binding could not resolve this path.
    #[must_use]
    pub const fn definition(self) -> CatalogDefinitionRef {
        self.definition
    }

    /// Return the resource-owned capture or locale-value failure.
    #[must_use]
    pub const fn error(self) -> LocaleCaptureError {
        self.error
    }
}

/// Failure while resolving one linker-participating project catalog path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkerCatalogAssignmentError {
    /// Ordinary host-format assignment failed before locale binding.
    Format(CatalogAssignmentConflict),
    /// One checked path binding could not resolve the concrete path.
    Locale(CatalogLocaleAssignmentError),
    /// Surviving definitions assigned unequal scope or locale identities.
    Binding(CatalogBindingConflict),
}

impl fmt::Display for LinkerCatalogAssignmentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(_) => formatter.write_str("catalog format assignment conflicts"),
            Self::Locale(error) => error.error.fmt(formatter),
            Self::Binding(_) => formatter.write_str("catalog scope or locale binding conflicts"),
        }
    }
}

impl std::error::Error for LinkerCatalogAssignmentError {}

impl CatalogAssignmentConflict {
    /// Return the later decisive conflicting definition.
    #[must_use]
    pub const fn assignment(self) -> CatalogDefinitionRef {
        self.assignment
    }

    /// Return the earliest definition that assigned a different format.
    #[must_use]
    pub const fn conflicting_assignment(self) -> CatalogDefinitionRef {
        self.conflicting_assignment
    }
}

/// Fallback-combined project and overlay resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayeredCatalogResolution {
    /// Neither the authoritative project layer nor an eligible overlay matched.
    Unmatched,
    /// One layer supplied a complete read-only assignment.
    Matched(LayeredCatalogMatch),
}

/// Read-only source and assignment for one layered match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayeredCatalogMatch {
    origin: CatalogAssignmentOrigin,
    assignment: ResolvedCatalogAssignment,
}

impl LayeredCatalogMatch {
    /// Return which layer supplied this match.
    #[must_use]
    pub const fn origin(&self) -> CatalogAssignmentOrigin {
        self.origin
    }

    /// Borrow the resolved catalog assignment.
    #[must_use]
    pub const fn assignment(&self) -> &ResolvedCatalogAssignment {
        &self.assignment
    }
}

impl ResolvedResources {
    /// Return the preserved project policy state.
    #[must_use]
    pub const fn policy_state(&self) -> CatalogPolicyState {
        match self.policy {
            ResolvedCatalogPolicy::Absent => CatalogPolicyState::Absent,
            ResolvedCatalogPolicy::Empty => CatalogPolicyState::Empty,
            ResolvedCatalogPolicy::Configured(_) => CatalogPolicyState::Configured,
        }
    }

    /// Return every configured definition in source-array order.
    #[must_use]
    pub fn definitions(&self) -> &[ResolvedCatalogDefinition] {
        match &self.policy {
            ResolvedCatalogPolicy::Configured(definitions) => definitions,
            ResolvedCatalogPolicy::Absent | ResolvedCatalogPolicy::Empty => &[],
        }
    }

    /// Return the canonical exact registry of linker-participating scopes.
    #[must_use]
    pub fn linker_scopes(&self) -> &[CatalogScopeName] {
        &self.linker_scopes
    }

    /// Select the first fixed binding outside a caller-provided production set.
    ///
    /// Definitions are visited in source-array order. Path bindings are
    /// intentionally deferred until one concrete assignment captures a locale.
    pub fn first_fixed_locale_not_production(
        &self,
        mut is_production: impl FnMut(&str) -> bool,
    ) -> Option<CatalogLocaleNotProduction> {
        self.definitions().iter().find_map(|definition| {
            let binding = definition.binding.as_ref()?;
            let locale = binding.locale.fixed_locale()?;
            (!is_production(locale.as_str())).then(|| CatalogLocaleNotProduction {
                definition_index: definition.index,
                scope: binding.scope.clone(),
                locale: locale.clone(),
            })
        })
    }

    /// Resolve one validated project-relative logical path.
    pub fn resolve_path(
        &self,
        path: &ProjectRelativeResourcePath,
    ) -> Result<CatalogResolution, CatalogAssignmentConflict> {
        match &self.policy {
            ResolvedCatalogPolicy::Absent => Ok(CatalogResolution::PolicyAbsent),
            ResolvedCatalogPolicy::Empty => Ok(CatalogResolution::PolicyEmpty),
            ResolvedCatalogPolicy::Configured(definitions) => {
                match resolve_layer(definitions, CatalogAssignmentOrigin::Project, path)? {
                    LayerResolution::Unmatched => Ok(CatalogResolution::Unmatched),
                    LayerResolution::Excluded => Ok(CatalogResolution::Excluded),
                    LayerResolution::Matched(assignment) => {
                        Ok(CatalogResolution::Matched(assignment))
                    }
                }
            }
        }
    }

    /// Resolve one path and require coherent scope/locale bindings for linker use.
    pub fn resolve_linker_path(
        &self,
        path: &ProjectRelativeResourcePath,
    ) -> Result<LinkerCatalogResolution, LinkerCatalogAssignmentError> {
        let resolution = self
            .resolve_path(path)
            .map_err(LinkerCatalogAssignmentError::Format)?;
        let CatalogResolution::Matched(catalog) = resolution else {
            return Ok(match resolution {
                CatalogResolution::PolicyAbsent => LinkerCatalogResolution::PolicyAbsent,
                CatalogResolution::PolicyEmpty => LinkerCatalogResolution::PolicyEmpty,
                CatalogResolution::Unmatched => LinkerCatalogResolution::Unmatched,
                CatalogResolution::Excluded => LinkerCatalogResolution::Excluded,
                CatalogResolution::Matched(_) => unreachable!("matched was handled above"),
            });
        };

        let definitions = match &self.policy {
            ResolvedCatalogPolicy::Configured(definitions) => definitions,
            ResolvedCatalogPolicy::Absent | ResolvedCatalogPolicy::Empty => {
                unreachable!("a matched assignment requires configured policy")
            }
        };
        let mut expected: Option<(CatalogScopeName, ResolvedLocale, CatalogDefinitionRef)> = None;
        let mut participating_definitions = Vec::new();

        for definition_ref in catalog.surviving_definitions() {
            let definition = &definitions[definition_ref.definition_index()];
            let Some(binding) = definition.binding.as_ref() else {
                continue;
            };
            let locale = binding.locale.resolve(path).map_err(|error| {
                LinkerCatalogAssignmentError::Locale(CatalogLocaleAssignmentError {
                    definition: *definition_ref,
                    error,
                })
            })?;

            if let Some((expected_scope, expected_locale, expected_definition)) = &expected {
                let field = if binding.scope != *expected_scope {
                    Some(CatalogBindingConflictField::Scope)
                } else if locale != *expected_locale {
                    Some(CatalogBindingConflictField::Locale)
                } else {
                    None
                };
                if let Some(field) = field {
                    return Err(LinkerCatalogAssignmentError::Binding(
                        CatalogBindingConflict {
                            field,
                            assignment: *definition_ref,
                            conflicting_assignment: *expected_definition,
                        },
                    ));
                }
            } else {
                expected = Some((binding.scope.clone(), locale.clone(), *definition_ref));
            }
            participating_definitions.push(*definition_ref);
        }

        let Some((scope, locale, _)) = expected else {
            return Ok(LinkerCatalogResolution::EntryLevelOnly(catalog));
        };
        Ok(LinkerCatalogResolution::Matched(
            ResolvedLinkerCatalogAssignment {
                catalog,
                scope,
                locale,
                participating_definitions: Arc::from(participating_definitions),
            },
        ))
    }

    /// Resolve project policy followed by the fallback-only overlay layer.
    pub fn resolve_path_with_overlay(
        &self,
        overlay: &ResolvedCatalogOverlay,
        path: &ProjectRelativeResourcePath,
    ) -> Result<LayeredCatalogResolution, CatalogAssignmentConflict> {
        match self.resolve_path(path)? {
            CatalogResolution::Matched(assignment) => {
                Ok(LayeredCatalogResolution::Matched(LayeredCatalogMatch {
                    origin: CatalogAssignmentOrigin::Project,
                    assignment,
                }))
            }
            CatalogResolution::PolicyEmpty | CatalogResolution::Excluded => {
                Ok(LayeredCatalogResolution::Unmatched)
            }
            CatalogResolution::PolicyAbsent | CatalogResolution::Unmatched => {
                match resolve_layer(&overlay.definitions, CatalogAssignmentOrigin::Overlay, path)? {
                    LayerResolution::Matched(assignment) => {
                        Ok(LayeredCatalogResolution::Matched(LayeredCatalogMatch {
                            origin: CatalogAssignmentOrigin::Overlay,
                            assignment,
                        }))
                    }
                    LayerResolution::Unmatched | LayerResolution::Excluded => {
                        Ok(LayeredCatalogResolution::Unmatched)
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LayerResolution {
    Unmatched,
    Excluded,
    Matched(ResolvedCatalogAssignment),
}

fn resolve_layer(
    definitions: &[ResolvedCatalogDefinition],
    origin: CatalogAssignmentOrigin,
    path: &ProjectRelativeResourcePath,
) -> Result<LayerResolution, CatalogAssignmentConflict> {
    let mut included = false;
    let mut surviving = Vec::new();
    for definition in definitions {
        if definition
            .include
            .iter()
            .any(|pattern| pattern.is_match(path.as_str()))
        {
            included = true;
            if !definition
                .exclude
                .iter()
                .any(|pattern| pattern.is_match(path.as_str()))
            {
                surviving.push(definition);
            }
        }
    }

    if !included {
        return Ok(LayerResolution::Unmatched);
    }
    if surviving.is_empty() {
        return Ok(LayerResolution::Excluded);
    }

    let derived = classify_logical_path(path.as_str());
    let surviving_references = surviving
        .iter()
        .map(|definition| CatalogDefinitionRef::new(origin, definition.index))
        .collect::<Vec<_>>();
    let mut resolved_assignment = None;
    let mut assigning_references = Vec::new();

    for definition in &surviving {
        let classification = definition
            .format
            .map_or(derived.classification, HostFormatClassification::Shipped);
        let Some(known_id) = classification.known_id() else {
            continue;
        };
        let definition_ref = CatalogDefinitionRef::new(origin, definition.index);

        match resolved_assignment {
            None => {
                resolved_assignment = Some((known_id, classification, definition_ref));
                assigning_references.push(definition_ref);
            }
            Some((resolved_id, _, conflicting_assignment)) if resolved_id != known_id => {
                return Err(CatalogAssignmentConflict {
                    assignment: definition_ref,
                    conflicting_assignment,
                });
            }
            Some(_) => assigning_references.push(definition_ref),
        }
    }

    let classification = resolved_assignment.map_or(
        HostFormatClassification::UnrecognizedExtension,
        |(_, classification, _)| classification,
    );
    Ok(LayerResolution::Matched(ResolvedCatalogAssignment::new(
        classification,
        derived.retained_extension,
        Arc::from(surviving_references),
        Arc::from(assigning_references),
    )))
}

#[cfg(test)]
mod tests {
    use schemars::generate::SchemaSettings;
    use schemars::schema_for;
    use serde_json::{json, Value};

    use super::{
        CatalogAssignmentOrigin, CatalogBindingConflictField, CatalogOverlayConfig,
        CatalogPolicyState, CatalogResolution, CatalogScopeName, LayeredCatalogResolution,
        LinkerCatalogAssignmentError, LinkerCatalogResolution, ProjectRelativeResourcePath,
        ProjectRelativeResourcePathError, ResourceConfigReason, ResourcesConfig,
    };
    use crate::{
        HostFormat, HostFormatClassification, HostFormatRegistry, KnownHostFormatId,
        ResourceErrorDetails,
    };

    fn validate(value: &Value) -> Result<ResourcesConfig, super::ResourceConfigViolation> {
        ResourcesConfig::validate(Some(value))
    }

    fn path(value: &str) -> ProjectRelativeResourcePath {
        ProjectRelativeResourcePath::try_from(value).unwrap()
    }

    #[test]
    fn preserves_absent_empty_and_configured_policy() {
        let absent = ResourcesConfig::validate(None).unwrap();
        let missing = validate(&json!({})).unwrap();
        let empty = validate(&json!({ "catalogs": [] })).unwrap();
        let configured = validate(&json!({
            "catalogs": [{ "include": ["locales/**/*.json"] }]
        }))
        .unwrap();

        assert!(absent.catalogs().is_none());
        assert!(missing.catalogs().is_none());
        assert_eq!(empty.catalogs().unwrap().len(), 0);
        assert_eq!(configured.catalogs().unwrap().len(), 1);
        assert_eq!(absent.resolve().policy_state(), CatalogPolicyState::Absent);
        assert_eq!(empty.resolve().policy_state(), CatalogPolicyState::Empty);
        assert_eq!(
            configured.resolve().policy_state(),
            CatalogPolicyState::Configured
        );
    }

    #[test]
    fn reports_section_and_unknown_field_evidence_deterministically() {
        let null = validate(&Value::Null).unwrap_err();
        assert_eq!(null.reason(), ResourceConfigReason::InvalidSectionShape);
        assert_eq!(null.pointer(), "/resources");
        assert_eq!(null.value(), Some(&Value::Null));

        let unknown = validate(&json!({ "zeta": [], "alpha": true })).unwrap_err();
        assert_eq!(unknown.reason(), ResourceConfigReason::UnknownField);
        assert_eq!(unknown.pointer(), "/resources/alpha");
        assert_eq!(unknown.field(), Some("alpha"));
        assert_eq!(unknown.value(), Some(&Value::Bool(true)));

        let escaped = validate(&json!({ "a/b~c": true })).unwrap_err();
        assert_eq!(escaped.pointer(), "/resources/a~1b~0c");
        assert_eq!(escaped.field(), Some("a/b~c"));
    }

    #[test]
    fn validates_catalog_fields_in_definition_local_order() {
        let cases = [
            (
                json!({ "catalogs": null }),
                ResourceConfigReason::InvalidCatalogsShape,
                "/resources/catalogs",
            ),
            (
                json!({ "catalogs": [false] }),
                ResourceConfigReason::InvalidCatalogDefinitionShape,
                "/resources/catalogs/0",
            ),
            (
                json!({ "catalogs": [{ "include": [] }] }),
                ResourceConfigReason::InvalidCatalogIncludeShape,
                "/resources/catalogs/0/include",
            ),
            (
                json!({ "catalogs": [{ "include": [1] }] }),
                ResourceConfigReason::InvalidCatalogGlob,
                "/resources/catalogs/0/include/0",
            ),
            (
                json!({ "catalogs": [{ "include": ["[" ] }] }),
                ResourceConfigReason::InvalidCatalogGlob,
                "/resources/catalogs/0/include/0",
            ),
            (
                json!({ "catalogs": [{ "include": ["*.json"], "exclude": null }] }),
                ResourceConfigReason::InvalidCatalogExcludeShape,
                "/resources/catalogs/0/exclude",
            ),
            (
                json!({ "catalogs": [{ "include": ["*.json"], "format": ["json"] }] }),
                ResourceConfigReason::InvalidCatalogFormat,
                "/resources/catalogs/0/format",
            ),
            (
                json!({ "catalogs": [{ "include": ["*.json"], "format": "yaml" }] }),
                ResourceConfigReason::InvalidCatalogFormat,
                "/resources/catalogs/0/format",
            ),
        ];

        for (value, reason, pointer) in cases {
            let violation = validate(&value).unwrap_err();
            assert_eq!(violation.reason(), reason);
            assert_eq!(violation.pointer(), pointer);
        }

        let definition_order = validate(&json!({
            "catalogs": [
                { "include": ["*.json"], "format": "yaml" },
                {}
            ]
        }))
        .unwrap_err();
        assert_eq!(definition_order.pointer(), "/resources/catalogs/0/format");

        let unknown = validate(&json!({
            "catalogs": [{ "zeta": true, "alpha": false }]
        }))
        .unwrap_err();
        assert_eq!(unknown.pointer(), "/resources/catalogs/0/alpha");
        assert_eq!(unknown.field(), Some("alpha"));
    }

    #[test]
    fn accepts_only_exact_shipped_explicit_format() {
        for invalid in ["JSON", " json", "json ", "yaml", "json,yaml", "yml"] {
            let violation = validate(&json!({
                "catalogs": [{ "include": ["*.data"], "format": invalid }]
            }))
            .unwrap_err();
            assert_eq!(
                violation.reason(),
                ResourceConfigReason::InvalidCatalogFormat
            );
            assert_eq!(violation.value(), Some(&Value::String(invalid.to_owned())));
        }

        let config = validate(&json!({
            "catalogs": [{ "include": ["*.data"], "format": "json" }]
        }))
        .unwrap();
        assert_eq!(
            config.catalogs().unwrap()[0].format(),
            Some(HostFormat::Json)
        );
    }

    #[test]
    fn validates_scope_and_locale_after_the_existing_catalog_fields() {
        let cases = [
            (
                json!({ "catalogs": [{ "include": ["*.json"], "scope": "app" }] }),
                ResourceConfigReason::InvalidCatalogScopeLocalePair,
                "/resources/catalogs/0",
            ),
            (
                json!({ "catalogs": [{ "include": ["*.json"], "locale": { "from": "fixed", "value": "en" } }] }),
                ResourceConfigReason::InvalidCatalogScopeLocalePair,
                "/resources/catalogs/0",
            ),
            (
                json!({ "catalogs": [{ "include": ["*.json"], "scope": null, "locale": { "from": "fixed", "value": "en" } }] }),
                ResourceConfigReason::InvalidCatalogScope,
                "/resources/catalogs/0/scope",
            ),
            (
                json!({ "catalogs": [{ "include": ["*.json"], "scope": "", "locale": { "from": "fixed", "value": "en" } }] }),
                ResourceConfigReason::InvalidCatalogScope,
                "/resources/catalogs/0/scope",
            ),
            (
                json!({ "catalogs": [{ "include": ["*.json"], "scope": "app", "locale": "en" }] }),
                ResourceConfigReason::InvalidCatalogLocaleBinding,
                "/resources/catalogs/0/locale",
            ),
            (
                json!({ "catalogs": [{ "include": ["*.json"], "scope": "app", "locale": {} }] }),
                ResourceConfigReason::InvalidCatalogLocaleBinding,
                "/resources/catalogs/0/locale/from",
            ),
            (
                json!({ "catalogs": [{ "include": ["*.json"], "scope": "app", "locale": { "from": "host" } }] }),
                ResourceConfigReason::InvalidCatalogLocaleBinding,
                "/resources/catalogs/0/locale/from",
            ),
            (
                json!({ "catalogs": [{ "include": ["*.json"], "scope": "app", "locale": { "from": "path" } }] }),
                ResourceConfigReason::InvalidCatalogLocaleBinding,
                "/resources/catalogs/0/locale/pattern",
            ),
            (
                json!({ "catalogs": [{ "include": ["*.json"], "scope": "app", "locale": { "from": "path", "pattern": "*.json" } }] }),
                ResourceConfigReason::InvalidLocaleCapturePattern,
                "/resources/catalogs/0/locale/pattern",
            ),
            (
                json!({ "catalogs": [{ "include": ["*.json"], "scope": "app", "locale": { "from": "fixed", "value": "" } }] }),
                ResourceConfigReason::InvalidCatalogLocaleBinding,
                "/resources/catalogs/0/locale/value",
            ),
        ];

        for (value, reason, pointer) in cases {
            let violation = validate(&value).unwrap_err();
            assert_eq!(violation.reason(), reason);
            assert_eq!(violation.pointer(), pointer);
        }

        let overlong_scope = validate(&json!({
            "catalogs": [{
                "include": ["*.json"],
                "scope": "s".repeat(256),
                "locale": { "from": "fixed", "value": "en" }
            }]
        }))
        .unwrap_err();
        assert_eq!(
            overlong_scope.reason(),
            ResourceConfigReason::InvalidCatalogScope
        );

        let overlong_locale = validate(&json!({
            "catalogs": [{
                "include": ["*.json"],
                "scope": "app",
                "locale": { "from": "fixed", "value": "l".repeat(256) }
            }]
        }))
        .unwrap_err();
        assert_eq!(
            overlong_locale.reason(),
            ResourceConfigReason::InvalidCatalogLocaleBinding
        );

        let format_precedes_pair = validate(&json!({
            "catalogs": [{
                "include": ["*.json"],
                "format": "yaml",
                "scope": "app"
            }]
        }))
        .unwrap_err();
        assert_eq!(
            format_precedes_pair.reason(),
            ResourceConfigReason::InvalidCatalogFormat
        );

        let locale_unknown_precedes_discriminator = validate(&json!({
            "catalogs": [{
                "include": ["*.json"],
                "scope": "app",
                "locale": { "zeta": true, "alpha": false }
            }]
        }))
        .unwrap_err();
        assert_eq!(
            locale_unknown_precedes_discriminator.reason(),
            ResourceConfigReason::UnknownField
        );
        assert_eq!(
            locale_unknown_precedes_discriminator.pointer(),
            "/resources/catalogs/0/locale/alpha"
        );

        let removed_group = validate(&json!({
            "catalogs": [{
                "include": ["*.json"],
                "group": "app"
            }]
        }))
        .unwrap_err();
        assert_eq!(removed_group.reason(), ResourceConfigReason::UnknownField);
        assert_eq!(removed_group.pointer(), "/resources/catalogs/0/group");
        assert_eq!(removed_group.field(), Some("group"));
    }

    #[test]
    fn retains_exact_coordinated_bindings_and_linker_scope_registry() {
        let config = validate(&json!({
            "catalogs": [
                { "include": ["drafts/**/*.json"] },
                {
                    "include": ["locales/*.json"],
                    "scope": "z-app",
                    "locale": { "from": "path", "pattern": "locales/{locale}.json" }
                },
                {
                    "include": ["vendor/**/*.json"],
                    "scope": "a-vendor",
                    "locale": { "from": "fixed", "value": "EN_us" }
                },
                {
                    "include": ["other/**/*.json"],
                    "scope": "z-app",
                    "locale": { "from": "fixed", "value": "ja" }
                }
            ]
        }))
        .unwrap();

        assert!(!config.catalogs().unwrap()[0].is_linker_participating());
        assert!(config.catalogs().unwrap()[1].is_linker_participating());
        assert_eq!(
            config.catalogs().unwrap()[2]
                .locale()
                .unwrap()
                .fixed_locale()
                .unwrap()
                .as_str(),
            "EN_us"
        );

        let resolved = config.resolve();
        assert!(resolved.definitions()[0].binding().is_none());
        assert!(resolved.definitions()[1].binding().is_some());
        assert_eq!(
            resolved
                .linker_scopes()
                .iter()
                .map(CatalogScopeName::as_str)
                .collect::<Vec<_>>(),
            vec!["a-vendor", "z-app"]
        );

        let fixed_policy_failure = resolved
            .first_fixed_locale_not_production(|locale| locale == "ja")
            .expect("the first fixed non-production locale should be retained");
        assert_eq!(fixed_policy_failure.definition_index(), 2);
        assert_eq!(fixed_policy_failure.scope().as_str(), "a-vendor");
        assert_eq!(fixed_policy_failure.locale().as_str(), "EN_us");

        let LinkerCatalogResolution::Matched(assignment) = resolved
            .resolve_linker_path(&path("vendor/messages.json"))
            .unwrap()
        else {
            panic!("the fixed linker catalog should resolve");
        };
        let concrete_policy_failure = assignment
            .validate_production_locale(|locale| locale == "ja")
            .expect_err("the fixed catalog locale is outside the production set");
        assert_eq!(concrete_policy_failure.definition_index(), 2);
        assert_eq!(concrete_policy_failure.scope().as_str(), "a-vendor");
        assert_eq!(concrete_policy_failure.locale().as_str(), "EN_us");
    }

    #[test]
    fn admits_exact_binding_boundaries_and_joins_equal_definitions() {
        let scope = "s".repeat(255);
        let locale = "l".repeat(255);
        let resolved = validate(&json!({
            "catalogs": [
                {
                    "include": ["fixed/*.json"],
                    "scope": scope,
                    "locale": { "from": "fixed", "value": locale }
                },
                {
                    "include": ["fixed/*.json"],
                    "scope": scope,
                    "locale": { "from": "fixed", "value": locale }
                }
            ]
        }))
        .unwrap()
        .resolve();

        assert_eq!(resolved.linker_scopes()[0].as_str(), scope);
        let LinkerCatalogResolution::Matched(assignment) = resolved
            .resolve_linker_path(&path("fixed/messages.json"))
            .unwrap()
        else {
            panic!("equal coordinated definitions should join");
        };
        assert_eq!(assignment.scope().as_str(), scope);
        assert_eq!(assignment.locale().as_str(), locale);
        assert_eq!(assignment.participating_definitions().len(), 2);

        let captured = validate(&json!({
            "catalogs": [{
                "include": ["locales/*.json"],
                "scope": "app",
                "locale": { "from": "path", "pattern": "locales/{locale}.json" }
            }]
        }))
        .unwrap()
        .resolve();
        let LinkerCatalogResolution::Matched(assignment) = captured
            .resolve_linker_path(&path(&format!("locales/{locale}.json")))
            .unwrap()
        else {
            panic!("the inclusive locale boundary should resolve");
        };
        assert_eq!(assignment.locale().as_str(), locale);
    }

    #[test]
    fn linker_resolution_distinguishes_entry_only_and_resolves_path_locales() {
        let resolved = validate(&json!({
            "catalogs": [
                { "include": ["drafts/**/*.json"] },
                {
                    "include": ["locales/*.json"],
                    "scope": "app",
                    "locale": { "from": "path", "pattern": "locales/{locale}.json" }
                }
            ]
        }))
        .unwrap()
        .resolve();

        assert!(matches!(
            resolved
                .resolve_linker_path(&path("drafts/en/messages.json"))
                .unwrap(),
            LinkerCatalogResolution::EntryLevelOnly(_)
        ));

        let LinkerCatalogResolution::Matched(assignment) = resolved
            .resolve_linker_path(&path("locales/JA_jp.json"))
            .unwrap()
        else {
            panic!("coordinated definition should resolve");
        };
        assert_eq!(assignment.scope().as_str(), "app");
        assert_eq!(assignment.locale().as_str(), "JA_jp");
        assert_eq!(assignment.participating_definitions().len(), 1);

        let mismatch = validate(&json!({
            "catalogs": [{
                "include": ["other/*.json"],
                "scope": "app",
                "locale": { "from": "path", "pattern": "locales/{locale}.json" }
            }]
        }))
        .unwrap()
        .resolve()
        .resolve_linker_path(&path("other/en.json"))
        .unwrap_err();
        assert!(matches!(mismatch, LinkerCatalogAssignmentError::Locale(_)));
    }

    #[test]
    fn linker_resolution_rejects_scope_then_locale_binding_conflicts() {
        let scope_conflict = validate(&json!({
            "catalogs": [
                {
                    "include": ["locales/*.json"],
                    "scope": "app",
                    "locale": { "from": "fixed", "value": "en" }
                },
                {
                    "include": ["locales/*.json"],
                    "scope": "admin",
                    "locale": { "from": "fixed", "value": "ja" }
                }
            ]
        }))
        .unwrap()
        .resolve()
        .resolve_linker_path(&path("locales/messages.json"))
        .unwrap_err();
        let LinkerCatalogAssignmentError::Binding(scope_conflict) = scope_conflict else {
            panic!("scope conflict expected");
        };
        assert_eq!(scope_conflict.field(), CatalogBindingConflictField::Scope);

        let locale_conflict = validate(&json!({
            "catalogs": [
                {
                    "include": ["locales/*.json"],
                    "scope": "app",
                    "locale": { "from": "fixed", "value": "en" }
                },
                {
                    "include": ["locales/*.json"],
                    "scope": "app",
                    "locale": { "from": "fixed", "value": "ja" }
                }
            ]
        }))
        .unwrap()
        .resolve()
        .resolve_linker_path(&path("locales/messages.json"))
        .unwrap_err();
        let LinkerCatalogAssignmentError::Binding(locale_conflict) = locale_conflict else {
            panic!("locale conflict expected");
        };
        assert_eq!(locale_conflict.field(), CatalogBindingConflictField::Locale);
    }

    #[test]
    fn validates_project_relative_path_without_os_reinterpretation() {
        for valid in [
            "locales/en.json",
            "日本語/メッセージ.json",
            r"literal\backslash.json",
        ] {
            assert_eq!(path(valid).as_str(), valid);
        }

        for (invalid, reason) in [
            ("", ProjectRelativeResourcePathError::Empty),
            (
                "/absolute.json",
                ProjectRelativeResourcePathError::NotRelative,
            ),
            (
                "C:/absolute.json",
                ProjectRelativeResourcePathError::NotRelative,
            ),
            (
                "a//b.json",
                ProjectRelativeResourcePathError::InvalidSegment,
            ),
            ("./a.json", ProjectRelativeResourcePathError::InvalidSegment),
            (
                "a/../b.json",
                ProjectRelativeResourcePathError::InvalidSegment,
            ),
        ] {
            assert_eq!(
                ProjectRelativeResourcePath::try_from(invalid).unwrap_err(),
                reason
            );
        }
    }

    #[test]
    fn resolves_membership_exclusion_and_extension_classification() {
        let resolved = validate(&json!({
            "catalogs": [{
                "include": ["locales/**/*"],
                "exclude": ["locales/generated/**"]
            }]
        }))
        .unwrap()
        .resolve();

        assert!(matches!(
            resolved.resolve_path(&path("src/messages.json")).unwrap(),
            CatalogResolution::Unmatched
        ));
        assert!(matches!(
            resolved
                .resolve_path(&path("locales/generated/en.json"))
                .unwrap(),
            CatalogResolution::Excluded
        ));

        let CatalogResolution::Matched(json) =
            resolved.resolve_path(&path("locales/en.JSON")).unwrap()
        else {
            panic!("JSON should match");
        };
        assert_eq!(
            json.classification(),
            HostFormatClassification::Shipped(HostFormat::Json)
        );
        assert_eq!(json.retained_extension(), ".JSON");

        let CatalogResolution::Matched(yaml) =
            resolved.resolve_path(&path("locales/en.YML")).unwrap()
        else {
            panic!("YAML should match membership");
        };
        assert_eq!(
            yaml.classification(),
            HostFormatClassification::KnownButUnshipped(KnownHostFormatId::Yaml)
        );
        assert_eq!(yaml.retained_extension(), ".YML");
    }

    #[test]
    fn applies_excludes_only_within_their_own_definition() {
        let resolved = validate(&json!({
            "catalogs": [
                {
                    "include": ["locales/**/*.json"],
                    "exclude": ["locales/en.json"]
                },
                { "include": ["locales/en.json"] }
            ]
        }))
        .unwrap()
        .resolve();

        let CatalogResolution::Matched(assignment) =
            resolved.resolve_path(&path("locales/en.json")).unwrap()
        else {
            panic!("one surviving definition should retain membership");
        };
        assert_eq!(
            assignment
                .surviving_definitions()
                .iter()
                .map(|reference| reference.definition_index())
                .collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(
            assignment
                .assigning_definitions()
                .iter()
                .map(|reference| reference.definition_index())
                .collect::<Vec<_>>(),
            vec![1]
        );
    }

    #[test]
    fn deduplicates_same_format_and_ignores_unrecognized_assignments() {
        let resolved = validate(&json!({
            "catalogs": [
                { "include": ["messages.*"] },
                { "include": ["*.data"], "format": "json" },
                { "include": ["messages.data"], "format": "json" }
            ]
        }))
        .unwrap()
        .resolve();

        let CatalogResolution::Matched(assignment) =
            resolved.resolve_path(&path("messages.data")).unwrap()
        else {
            panic!("path should match");
        };
        assert_eq!(
            assignment.classification(),
            HostFormatClassification::Shipped(HostFormat::Json)
        );
        assert_eq!(assignment.surviving_definitions().len(), 3);
        assert_eq!(
            assignment
                .assigning_definitions()
                .iter()
                .map(|reference| reference.definition_index())
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[test]
    fn retains_all_unrecognized_matches_without_an_assignment() {
        let resolved = validate(&json!({
            "catalogs": [{ "include": ["messages.*"] }]
        }))
        .unwrap()
        .resolve();

        let CatalogResolution::Matched(assignment) =
            resolved.resolve_path(&path("messages.data")).unwrap()
        else {
            panic!("unrecognized extension still establishes membership");
        };
        assert_eq!(
            assignment.classification(),
            HostFormatClassification::UnrecognizedExtension
        );
        assert_eq!(assignment.retained_extension(), ".data");
        assert_eq!(assignment.surviving_definitions().len(), 1);
        assert!(assignment.assigning_definitions().is_empty());

        let error = HostFormatRegistry::new()
            .resolve_format(&assignment)
            .unwrap_err();
        assert!(matches!(
            error.details(),
            ResourceErrorDetails::FormatUnsupported {
                format: None,
                extension,
                ..
            } if extension.as_ref() == ".data"
        ));
    }

    #[test]
    fn reports_the_decisive_known_format_conflict() {
        let resolved = validate(&json!({
            "catalogs": [
                { "include": ["locales/*"], "format": "json" },
                { "include": ["locales/*.yaml"] },
                { "include": ["locales/*"], "format": "json" }
            ]
        }))
        .unwrap()
        .resolve();

        let conflict = resolved.resolve_path(&path("locales/en.yaml")).unwrap_err();
        assert_eq!(conflict.assignment().definition_index(), 1);
        assert_eq!(
            conflict.assignment().origin(),
            CatalogAssignmentOrigin::Project
        );
        assert_eq!(conflict.conflicting_assignment().definition_index(), 0);
    }

    #[test]
    fn applies_overlay_only_as_project_fallback() {
        let overlay = CatalogOverlayConfig::validate(&json!({
            "catalogs": [{ "include": ["locales/**/*.json"] }]
        }))
        .unwrap()
        .resolve();
        let target = path("locales/en.json");

        let absent = ResourcesConfig::default().resolve();
        let LayeredCatalogResolution::Matched(overlay_match) =
            absent.resolve_path_with_overlay(&overlay, &target).unwrap()
        else {
            panic!("absent project policy should fall back");
        };
        assert_eq!(overlay_match.origin(), CatalogAssignmentOrigin::Overlay);

        let unmatched = validate(&json!({
            "catalogs": [{ "include": ["other/**/*.json"] }]
        }))
        .unwrap()
        .resolve();
        let LayeredCatalogResolution::Matched(unmatched_fallback) = unmatched
            .resolve_path_with_overlay(&overlay, &target)
            .unwrap()
        else {
            panic!("configured unmatched project path should fall back");
        };
        assert_eq!(
            unmatched_fallback.origin(),
            CatalogAssignmentOrigin::Overlay
        );

        let empty = validate(&json!({ "catalogs": [] })).unwrap().resolve();
        assert!(matches!(
            empty.resolve_path_with_overlay(&overlay, &target).unwrap(),
            LayeredCatalogResolution::Unmatched
        ));

        let excluded = validate(&json!({
            "catalogs": [{
                "include": ["locales/**/*.json"],
                "exclude": ["locales/en.json"]
            }]
        }))
        .unwrap()
        .resolve();
        assert!(matches!(
            excluded
                .resolve_path_with_overlay(&overlay, &target)
                .unwrap(),
            LayeredCatalogResolution::Unmatched
        ));

        let project = validate(&json!({
            "catalogs": [{ "include": ["locales/**/*.json"] }]
        }))
        .unwrap()
        .resolve();
        let LayeredCatalogResolution::Matched(project_match) = project
            .resolve_path_with_overlay(&overlay, &target)
            .unwrap()
        else {
            panic!("project match should be authoritative");
        };
        assert_eq!(project_match.origin(), CatalogAssignmentOrigin::Project);
    }

    #[test]
    fn does_not_evaluate_overlay_when_project_is_authoritative() {
        let project = validate(&json!({
            "catalogs": [{
                "include": ["locales/**/*.yaml"],
                "format": "json"
            }]
        }))
        .unwrap()
        .resolve();
        let conflicting_overlay = CatalogOverlayConfig::validate(&json!({
            "catalogs": [
                { "include": ["locales/**/*.yaml"], "format": "json" },
                { "include": ["locales/**/*.yaml"] }
            ]
        }))
        .unwrap()
        .resolve();

        let LayeredCatalogResolution::Matched(layered) = project
            .resolve_path_with_overlay(&conflicting_overlay, &path("locales/en.yaml"))
            .expect("an authoritative project match must bypass overlay conflicts")
        else {
            panic!("project definition should match");
        };
        assert_eq!(layered.origin(), CatalogAssignmentOrigin::Project);
        assert_eq!(
            layered.assignment().classification(),
            HostFormatClassification::Shipped(HostFormat::Json)
        );
    }

    #[test]
    fn reports_overlay_conflicts_with_overlay_definition_evidence() {
        let overlay = CatalogOverlayConfig::validate(&json!({
            "catalogs": [
                { "include": ["locales/*"], "format": "json" },
                { "include": ["locales/*.yaml"] }
            ]
        }))
        .unwrap()
        .resolve();
        let conflict = ResourcesConfig::default()
            .resolve()
            .resolve_path_with_overlay(&overlay, &path("locales/en.yaml"))
            .unwrap_err();

        assert_eq!(
            conflict.assignment().origin(),
            CatalogAssignmentOrigin::Overlay
        );
        assert_eq!(conflict.assignment().definition_index(), 1);
        assert_eq!(
            conflict.conflicting_assignment().origin(),
            CatalogAssignmentOrigin::Overlay
        );
        assert_eq!(conflict.conflicting_assignment().definition_index(), 0);
    }

    #[test]
    fn overlay_validation_uses_normalized_root_pointers() {
        let violation = CatalogOverlayConfig::validate(&json!({
            "catalogs": [{ "include": ["["] }]
        }))
        .unwrap_err();
        assert_eq!(violation.pointer(), "/catalogs/0/include/0");

        let root = CatalogOverlayConfig::validate(&Value::Null).unwrap_err();
        assert_eq!(root.pointer(), "");
        assert_eq!(root.reason(), ResourceConfigReason::InvalidSectionShape);

        for field in ["scope", "locale"] {
            let violation = CatalogOverlayConfig::validate(&json!({
                "catalogs": [{ "include": ["*.json"], (field): true }]
            }))
            .unwrap_err();
            assert_eq!(violation.reason(), ResourceConfigReason::UnknownField);
            assert_eq!(violation.field(), Some(field));
        }
    }

    #[test]
    fn generated_resource_schema_keeps_presence_only_fields_non_null() {
        let schema = serde_json::to_value(schema_for!(ResourcesConfig)).unwrap();
        let catalogs = &schema["properties"]["catalogs"];
        assert_eq!(catalogs["type"], "array");
        assert!(catalogs.get("anyOf").is_none());
        assert!(schema.get("required").is_none());

        let catalog = &schema["$defs"]["CatalogConfig"];
        assert_eq!(catalog["properties"]["scope"]["type"], "string");
        assert_eq!(catalog["properties"]["scope"]["maxLength"], 255);
        assert!(catalog["properties"]["scope"].get("anyOf").is_none());
        assert!(catalog["properties"]["locale"].get("oneOf").is_some());
        assert_eq!(
            catalog["properties"]["locale"]["oneOf"][1]["properties"]["value"]["maxLength"],
            255
        );
        assert_eq!(catalog["dependentRequired"]["scope"], json!(["locale"]));
        assert_eq!(catalog["dependentRequired"]["locale"], json!(["scope"]));
        assert!(catalog.get("dependencies").is_none());

        let draft07 = SchemaSettings::draft07()
            .into_generator()
            .into_root_schema_for::<ResourcesConfig>();
        let draft07 = serde_json::to_value(draft07).unwrap();
        let draft07_catalog = &draft07["definitions"]["CatalogConfig"];
        assert_eq!(draft07_catalog["dependencies"]["scope"], json!(["locale"]));
        assert_eq!(draft07_catalog["dependencies"]["locale"], json!(["scope"]));
        assert!(draft07_catalog.get("dependentRequired").is_none());

        let overlay = serde_json::to_value(schema_for!(CatalogOverlayConfig)).unwrap();
        let overlay_properties = overlay
            .pointer("/$defs/CatalogOverlaySchemaDefinition/properties")
            .and_then(Value::as_object)
            .expect("the overlay item definition has an object property map");
        assert!(!overlay_properties.contains_key("scope"));
        assert!(!overlay_properties.contains_key("locale"));
    }
}
