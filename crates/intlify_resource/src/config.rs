// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::fmt;
use std::sync::Arc;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::Serialize;
use serde_json::{Map, Value};

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
        let catalogs = validate_catalog_section(value, "/resources")?;
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
        ResolvedResources { policy }
    }
}

/// One validated resource catalog definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct CatalogConfig {
    #[schemars(
        length(min = 1),
        schema_with = "resource_glob_array_schema",
        description = "Non-empty project-relative resource membership patterns."
    )]
    include: Vec<ResourceGlob>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(
        default,
        schema_with = "resource_glob_array_schema",
        description = "Project-relative patterns removed from this definition's include set."
    )]
    exclude: Vec<ResourceGlob>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(
        default,
        schema_with = "host_format_schema",
        description = "Optional shipped host-format override."
    )]
    format: Option<HostFormat>,
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
}

/// Validated additive editor catalog overlay without a `resources` wrapper.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct CatalogOverlayConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(default, schema_with = "catalogs_schema")]
    catalogs: Vec<CatalogConfig>,
}

impl CatalogOverlayConfig {
    /// Validate one normalized overlay object.
    pub fn validate(value: &Value) -> Result<Self, ResourceConfigViolation> {
        let catalogs = validate_catalog_section(value, "")?.unwrap_or_default();
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

fn resource_glob_array_schema(generator: &mut SchemaGenerator) -> Schema {
    Vec::<String>::json_schema(generator)
}

fn host_format_schema(_: &mut SchemaGenerator) -> Schema {
    schemars::json_schema!({
        "type": "string",
        "enum": SUPPORTED_FORMATS,
    })
}

fn validate_catalog_section(
    value: &Value,
    base_pointer: &str,
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
            validate_catalog_definition(catalog, &pointer_index(&catalogs_pointer, index))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn validate_catalog_definition(
    value: &Value,
    pointer: &str,
) -> Result<CatalogConfig, ResourceConfigViolation> {
    let Some(definition) = value.as_object() else {
        return Err(ResourceConfigViolation::new(
            ResourceConfigReason::InvalidCatalogDefinitionShape,
            pointer.to_owned(),
            Some(value),
            None,
        ));
    };

    if let Some((field, rejected)) =
        first_unknown_field(definition, &["include", "exclude", "format"])
    {
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

    Ok(CatalogConfig {
        include,
        exclude,
        format,
    })
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedCatalogDefinition {
    index: usize,
    include: Arc<[ResourceGlob]>,
    exclude: Arc<[ResourceGlob]>,
    format: Option<HostFormat>,
}

fn resolve_definitions(catalogs: Vec<CatalogConfig>) -> Arc<[ResolvedCatalogDefinition]> {
    catalogs
        .into_iter()
        .enumerate()
        .map(|(index, catalog)| ResolvedCatalogDefinition {
            index,
            include: Arc::from(catalog.include),
            exclude: Arc::from(catalog.exclude),
            format: catalog.format,
        })
        .collect::<Vec<_>>()
        .into()
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
    use schemars::schema_for;
    use serde_json::{json, Value};

    use super::{
        CatalogAssignmentOrigin, CatalogOverlayConfig, CatalogPolicyState, CatalogResolution,
        LayeredCatalogResolution, ProjectRelativeResourcePath, ProjectRelativeResourcePathError,
        ResourceConfigReason, ResourcesConfig,
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
    }

    #[test]
    fn generated_resource_schema_keeps_presence_only_fields_non_null() {
        let schema = serde_json::to_value(schema_for!(ResourcesConfig)).unwrap();
        let catalogs = &schema["properties"]["catalogs"];
        assert_eq!(catalogs["type"], "array");
        assert!(catalogs.get("anyOf").is_none());
        assert!(schema.get("required").is_none());
    }
}
