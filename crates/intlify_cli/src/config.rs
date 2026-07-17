// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::cell::Cell;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use intlify_format::FormatMode;
use intlify_resource::{ResolvedResources, ResourceConfigViolation, ResourcesConfig};
use schemars::JsonSchema;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const CONFIG_JSON: &str = "intlify.config.json";
const CONFIG_JSONC: &str = "intlify.config.jsonc";
const SUPPORTED_EXTENSIONS: [&str; 2] = [".json", ".jsonc"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectConfig {
    pub fmt: FormatterConfig,
    pub lint: EmptyConfig,
    pub resources: ResourcesConfig,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            fmt: FormatterConfig::default(),
            lint: EmptyConfig {},
            resources: ResourcesConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FormatterConfig {
    #[serde(default)]
    #[schemars(description = "Formatting strategy. Defaults to standard.")]
    pub mode: FormatterMode,
    #[serde(default)]
    #[schemars(description = "Project-root-relative formatter ignore patterns.")]
    pub ignore_patterns: Vec<String>,
}

impl Default for FormatterConfig {
    fn default() -> Self {
        Self {
            mode: FormatterMode::Standard,
            ignore_patterns: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FormatterMode {
    #[default]
    Standard,
    Preserve,
}

impl FormatterMode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Preserve => "preserve",
        }
    }

    pub(crate) const fn to_format_mode(self) -> FormatMode {
        match self {
            Self::Standard => FormatMode::Standard,
            Self::Preserve => FormatMode::Preserve,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmptyConfig {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectConfigFile {
    #[serde(rename = "$schema")]
    #[schemars(description = "Editor-facing schema metadata. Ignored by the CLI at runtime.")]
    pub(crate) schema: Option<String>,
    #[schemars(description = "Formatter configuration.")]
    pub(crate) fmt: Option<FormatterConfig>,
    #[schemars(description = "Linter configuration. Phase 3C accepts only an empty object.")]
    pub(crate) lint: Option<EmptyConfig>,
    #[schemars(
        with = "Option<ResourcesConfig>",
        description = "Resource catalog configuration."
    )]
    pub(crate) resources: Option<Value>,
}

impl ProjectConfigFile {
    fn into_project_config(self, resources: ResourcesConfig) -> ProjectConfig {
        let Self {
            schema: _,
            fmt,
            lint,
            resources: _,
        } = self;
        ProjectConfig {
            fmt: fmt.unwrap_or_default(),
            lint: lint.unwrap_or_default(),
            resources,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedProjectConfig {
    pub project_root: PathBuf,
    pub config_path: Option<PathBuf>,
    pub source: ConfigSource,
    pub config: ProjectConfig,
    pub resolved_resources: ResolvedResources,
}

struct LoadedConfigBody {
    config: ProjectConfig,
    resolved_resources: ResolvedResources,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    Default,
    Discovered,
    Explicit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub kind: &'static str,
    pub code: &'static str,
    pub message: String,
    pub path: Option<String>,
    pub details: Option<Value>,
}

impl ConfigError {
    fn new(
        code: &'static str,
        message: String,
        path: Option<String>,
        details: Option<Value>,
    ) -> Self {
        Self {
            kind: "config",
            code,
            message,
            path,
            details,
        }
    }
}

pub fn discover_project_root(cwd: &Path) -> PathBuf {
    let absolute_cwd = absolutize_path(cwd);

    for ancestor in absolute_cwd.ancestors() {
        let git_marker = ancestor.join(".git");
        if git_marker.is_dir() || git_marker.is_file() {
            return ancestor.to_path_buf();
        }
    }

    absolute_cwd
}

pub fn load_project_config(
    cwd: &Path,
    explicit_config_path: Option<&str>,
) -> Result<LoadedProjectConfig, ConfigError> {
    let project_root = discover_project_root(cwd);

    if let Some(config_path) = explicit_config_path {
        let path = resolve_explicit_config_path(cwd, config_path);
        let LoadedConfigBody {
            config,
            resolved_resources,
        } = load_config_file(&project_root, &path)?;
        return Ok(LoadedProjectConfig {
            project_root,
            config_path: Some(path),
            source: ConfigSource::Explicit,
            config,
            resolved_resources,
        });
    }

    if let Some(path) = discover_root_config(&project_root)? {
        let LoadedConfigBody {
            config,
            resolved_resources,
        } = load_config_file(&project_root, &path)?;
        return Ok(LoadedProjectConfig {
            project_root,
            config_path: Some(path),
            source: ConfigSource::Discovered,
            config,
            resolved_resources,
        });
    }

    let config = ProjectConfig::default();
    let resolved_resources = config.resources.clone().resolve();
    Ok(LoadedProjectConfig {
        project_root,
        config_path: None,
        source: ConfigSource::Default,
        config,
        resolved_resources,
    })
}

pub fn discover_root_config(project_root: &Path) -> Result<Option<PathBuf>, ConfigError> {
    let json_path = project_root.join(CONFIG_JSON);
    let jsonc_path = project_root.join(CONFIG_JSONC);
    let has_json = json_path.is_file();
    let has_jsonc = jsonc_path.is_file();

    match (has_json, has_jsonc) {
        (true, true) => Err(ConfigError::new(
            "config_conflict",
            "Multiple root config files were found.".to_owned(),
            None,
            Some(json!({
                "paths": [
                    config_error_path(project_root, &json_path),
                    config_error_path(project_root, &jsonc_path)
                ]
            })),
        )),
        (true, false) => Ok(Some(json_path)),
        (false, true) => Ok(Some(jsonc_path)),
        (false, false) => Ok(None),
    }
}

pub fn slash_normalize_path(path: &Path) -> String {
    let mut normalized = String::new();

    for component in path.components() {
        let Some(part) = component_to_slash_string(component) else {
            continue;
        };

        // Preserve root components (`/`, `C:/`, UNC prefixes) while still
        // joining ordinary path segments with exactly one slash.
        if normalized.is_empty() || normalized.ends_with('/') || part == "/" {
            normalized.push_str(&part);
        } else {
            normalized.push('/');
            normalized.push_str(&part);
        }
    }

    normalized
}

pub fn config_error_path(project_root: &Path, path: &Path) -> String {
    let root = normalize_components(project_root);
    let path = normalize_components(path);

    path.strip_prefix(&root)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .map_or_else(|| slash_normalize_path(&path), slash_normalize_path)
}

pub fn resolve_explicit_config_path(cwd: &Path, config_path: &str) -> PathBuf {
    let path = Path::new(config_path);
    // Explicit config paths are resolved from process cwd but do not alter the
    // project root used in JSON envelopes.
    if path.is_absolute() {
        normalize_components(path)
    } else {
        normalize_components(&cwd.join(path))
    }
}

fn load_config_file(project_root: &Path, path: &Path) -> Result<LoadedConfigBody, ConfigError> {
    let path_label = config_error_path(project_root, path);

    if !path.exists() {
        return Err(ConfigError::new(
            "config_not_found",
            format!("Config file was not found: {path_label}"),
            Some(path_label),
            None,
        ));
    }

    let source = fs::read_to_string(path).map_err(|error| read_error(&path_label, &error))?;

    match config_syntax(path) {
        Some(ConfigSyntax::Json) => parse_json_config(&source, &source, &path_label),
        Some(ConfigSyntax::Jsonc) => {
            let normalized = normalize_jsonc(&source);
            parse_json_config(&normalized, &source, &path_label)
        }
        None => Err(ConfigError::new(
            "config_extension_unsupported",
            format!("Config file extension is not supported: {path_label}"),
            Some(path_label),
            Some(json!({ "supportedExtensions": SUPPORTED_EXTENSIONS })),
        )),
    }
}

fn parse_json_config(
    source: &str,
    location_source: &str,
    path_label: &str,
) -> Result<LoadedConfigBody, ConfigError> {
    debug_assert_eq!(source.len(), location_source.len());
    let duplicate_found = Cell::new(false);
    let mut deserializer = serde_json::Deserializer::from_str(source);
    let parsed = UniqueJsonValueSeed {
        duplicate_found: &duplicate_found,
    }
    .deserialize(&mut deserializer)
    .and_then(|value| {
        deserializer.end()?;
        Ok(value)
    });
    let value = parsed.map_err(|error| {
        config_parse_error(
            source,
            location_source,
            path_label,
            &error,
            duplicate_found.get(),
        )
    })?;

    let resources = validate_config_value(&value, path_label)?;

    let file = serde_json::from_value::<ProjectConfigFile>(value).map_err(|error| {
        ConfigError::new(
            "config_validation_failed",
            format!("Config file is not valid: {path_label}"),
            Some(path_label.to_owned()),
            Some(json!({
                "pointer": "",
                "reason": "model_validation_failed",
                "message": error.to_string()
            })),
        )
    })?;

    let resolved_resources = resources.clone().resolve();
    Ok(LoadedConfigBody {
        config: file.into_project_config(resources),
        resolved_resources,
    })
}

#[derive(Clone, Copy)]
struct UniqueJsonValueSeed<'a> {
    duplicate_found: &'a Cell<bool>,
}

struct UniqueJsonValueVisitor<'a> {
    duplicate_found: &'a Cell<bool>,
}

impl<'de> DeserializeSeed<'de> for UniqueJsonValueSeed<'_> {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(UniqueJsonValueVisitor {
            duplicate_found: self.duplicate_found,
        })
    }
}

impl<'de> Visitor<'de> for UniqueJsonValueVisitor<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("any JSON value without duplicate object members")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
        Ok(serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(Value::String(value))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(UniqueJsonValueSeed {
            duplicate_found: self.duplicate_found,
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A>(self, mut object: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = serde_json::Map::new();
        while let Some(key) = object.next_key::<String>()? {
            if values.contains_key(&key) {
                self.duplicate_found.set(true);
                return Err(de::Error::custom("duplicate object member"));
            }
            let value = object.next_value_seed(UniqueJsonValueSeed {
                duplicate_found: self.duplicate_found,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

fn config_parse_error(
    source: &str,
    location_source: &str,
    path_label: &str,
    error: &serde_json::Error,
    duplicate_found: bool,
) -> ConfigError {
    let details = if duplicate_found {
        let (line, column) = duplicate_member_location(source, location_source, error);
        json!({
            "line": line,
            "column": column,
            "reason": "duplicate_object_member"
        })
    } else {
        json!({
            "line": error.line(),
            "column": error.column().saturating_sub(1)
        })
    };

    ConfigError::new(
        "config_parse_failed",
        format!("Config file could not be parsed: {path_label}"),
        Some(path_label.to_owned()),
        Some(details),
    )
}

fn duplicate_member_location(
    source: &str,
    location_source: &str,
    error: &serde_json::Error,
) -> (usize, usize) {
    let detected =
        byte_offset_for_line_column(source, error.line(), error.column()).unwrap_or(source.len());
    let opening = previous_unescaped_quote(source, detected)
        .and_then(|closing| closing.checked_sub(1))
        .and_then(|before_closing| previous_unescaped_quote(source, before_closing))
        .unwrap_or(detected.min(location_source.len()));
    line_and_byte_column(location_source, opening)
}

fn byte_offset_for_line_column(source: &str, line: usize, column: usize) -> Option<usize> {
    if line == 0 {
        return None;
    }

    let mut line_start = 0;
    for _ in 1..line {
        let newline = source.as_bytes()[line_start..]
            .iter()
            .position(|byte| *byte == b'\n')?;
        line_start += newline + 1;
    }
    Some(
        line_start
            .saturating_add(column.saturating_sub(1))
            .min(source.len()),
    )
}

fn previous_unescaped_quote(source: &str, before_or_at: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut index = before_or_at.min(bytes.len().checked_sub(1)?);

    loop {
        if bytes[index] == b'"' {
            let preceding_backslashes = bytes[..index]
                .iter()
                .rev()
                .take_while(|byte| **byte == b'\\')
                .count();
            if preceding_backslashes % 2 == 0 {
                return Some(index);
            }
        }
        index = index.checked_sub(1)?;
    }
}

fn line_and_byte_column(source: &str, byte_offset: usize) -> (usize, usize) {
    let prefix = &source.as_bytes()[..byte_offset.min(source.len())];
    let mut line = 1;
    let mut line_start = 0;
    for (index, byte) in prefix.iter().enumerate() {
        if *byte == b'\n' {
            line += 1;
            line_start = index + 1;
        }
    }
    (line, prefix.len() - line_start)
}

fn validate_config_value(value: &Value, path_label: &str) -> Result<ResourcesConfig, ConfigError> {
    let Some(root) = value.as_object() else {
        return validation_error(path_label, "", "expected_object");
    };

    if let Some(key) = first_unknown_key(root, &["$schema", "fmt", "lint", "resources"]) {
        return validation_error(path_label, &json_pointer(&[key]), "unknown_field");
    }
    if let Some(schema) = root.get("$schema") {
        if !schema.is_string() {
            return validation_error(path_label, "/$schema", "expected_string");
        }
    }
    if let Some(formatter) = root.get("fmt") {
        validate_formatter_section(path_label, formatter)?;
    }
    if let Some(linter) = root.get("lint") {
        validate_empty_section(path_label, "lint", linter)?;
    }

    ResourcesConfig::validate(root.get("resources"))
        .map_err(|violation| resource_validation_error(path_label, &violation))
}

fn validate_formatter_section(path_label: &str, value: &Value) -> Result<(), ConfigError> {
    let Some(fields) = value.as_object() else {
        return validation_error(path_label, "/fmt", "expected_object");
    };

    if let Some(key) = first_unknown_key(fields, &["mode", "ignorePatterns"]) {
        return validation_error(path_label, &json_pointer(&["fmt", key]), "unknown_field");
    }
    if let Some(mode) = fields.get("mode") {
        validate_formatter_mode(path_label, mode)?;
    }
    if let Some(patterns) = fields.get("ignorePatterns") {
        validate_formatter_ignore_patterns(path_label, patterns)?;
    }

    Ok(())
}

fn validate_formatter_mode(path_label: &str, value: &Value) -> Result<(), ConfigError> {
    let Some(mode) = value.as_str() else {
        return validation_error(path_label, "/fmt/mode", "expected_string");
    };

    if matches!(mode, "standard" | "preserve") {
        Ok(())
    } else {
        validation_error_with_details(
            path_label,
            "/fmt/mode",
            "invalid_value",
            json!({
                "allowedValues": ["standard", "preserve"]
            }),
        )
    }
}

fn validate_formatter_ignore_patterns(path_label: &str, value: &Value) -> Result<(), ConfigError> {
    let Some(patterns) = value.as_array() else {
        return validation_error(path_label, "/fmt/ignorePatterns", "expected_array");
    };

    for (index, pattern) in patterns.iter().enumerate() {
        let pointer = format!("/fmt/ignorePatterns/{index}");
        let Some(pattern) = pattern.as_str() else {
            return validation_error(path_label, &pointer, "expected_string");
        };
        if let Err(reason) = validate_formatter_ignore_pattern(pattern) {
            return validation_error_with_details(
                path_label,
                &pointer,
                "invalid_ignore_pattern",
                json!({
                    "pattern": pattern,
                    "patternReason": reason
                }),
            );
        }
    }

    Ok(())
}

fn validate_empty_section(
    path_label: &str,
    section: &str,
    value: &Value,
) -> Result<(), ConfigError> {
    let Some(fields) = value.as_object() else {
        return validation_error(path_label, &json_pointer(&[section]), "expected_object");
    };

    if let Some(key) = first_unknown_key(fields, &[]) {
        return validation_error(path_label, &json_pointer(&[section, key]), "unknown_field");
    }

    Ok(())
}

fn first_unknown_key<'a>(
    object: &'a serde_json::Map<String, Value>,
    known: &[&str],
) -> Option<&'a str> {
    object
        .keys()
        .filter(|field| !known.contains(&field.as_str()))
        .min_by(|left, right| left.as_bytes().cmp(right.as_bytes()))
        .map(String::as_str)
}

fn resource_validation_error(path_label: &str, violation: &ResourceConfigViolation) -> ConfigError {
    let mut details = serde_json::Map::new();
    details.insert("pointer".to_owned(), json!(violation.pointer()));
    details.insert("reason".to_owned(), json!(violation.reason().as_str()));
    if let Some(field) = violation.field() {
        details.insert("field".to_owned(), json!(field));
    }
    if let Some(value) = violation.value() {
        details.insert("value".to_owned(), value.clone());
    }

    ConfigError::new(
        "config_validation_failed",
        format!("Config file is not valid: {path_label}"),
        Some(path_label.to_owned()),
        Some(Value::Object(details)),
    )
}

fn validation_error<T>(
    path_label: &str,
    pointer: &str,
    reason: &'static str,
) -> Result<T, ConfigError> {
    validation_error_with_details(path_label, pointer, reason, Value::Null)
}

fn validation_error_with_details<T>(
    path_label: &str,
    pointer: &str,
    reason: &'static str,
    extra: Value,
) -> Result<T, ConfigError> {
    let mut details = serde_json::Map::new();
    details.insert("pointer".to_owned(), json!(pointer));
    details.insert("reason".to_owned(), json!(reason));
    if let Value::Object(extra) = extra {
        for (key, value) in extra {
            details.insert(key, value);
        }
    }

    Err(ConfigError::new(
        "config_validation_failed",
        format!("Config file is not valid: {path_label}"),
        Some(path_label.to_owned()),
        Some(Value::Object(details)),
    ))
}

pub(crate) fn validate_formatter_ignore_pattern(pattern: &str) -> Result<(), &'static str> {
    let Some(pattern) = normalize_ignore_pattern(pattern) else {
        return Ok(());
    };

    let body = pattern.strip_prefix('!').unwrap_or(pattern);
    if body.is_empty() {
        return Err("empty_negation");
    }

    let body = body.strip_prefix('/').unwrap_or(body);
    let body = body.strip_suffix('/').unwrap_or(body);
    if body.is_empty() {
        return Err("empty_pattern");
    }

    glob::Pattern::new(body).map_err(|_| "invalid_glob")?;
    Ok(())
}

pub(crate) fn normalize_ignore_pattern(pattern: &str) -> Option<&str> {
    if pattern.trim().is_empty() || pattern.starts_with('#') {
        return None;
    }
    Some(pattern)
}

fn json_pointer(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|part| part.replace('~', "~0").replace('/', "~1"))
        .fold(String::new(), |mut pointer, part| {
            pointer.push('/');
            pointer.push_str(&part);
            pointer
        })
}

fn read_error(path_label: &str, error: &io::Error) -> ConfigError {
    let mut details = serde_json::Map::new();
    details.insert("ioKind".to_owned(), json!(format!("{:?}", error.kind())));
    if let Some(raw_os_error) = error.raw_os_error() {
        details.insert("rawOsError".to_owned(), json!(raw_os_error));
    }

    ConfigError::new(
        "config_read_failed",
        format!("Config file could not be read: {path_label}"),
        Some(path_label.to_owned()),
        Some(Value::Object(details)),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigSyntax {
    Json,
    Jsonc,
}

fn config_syntax(path: &Path) -> Option<ConfigSyntax> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("json") => Some(ConfigSyntax::Json),
        Some("jsonc") => Some(ConfigSyntax::Jsonc),
        _ => None,
    }
}

fn normalize_jsonc(source: &str) -> String {
    let without_comments = strip_jsonc_comments(source);
    let normalized = strip_jsonc_trailing_commas(&without_comments);
    debug_assert_eq!(normalized.len(), source.len());
    normalized
}

fn strip_jsonc_comments(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            output.push(ch);
            continue;
        }

        if ch == '/' && chars.peek() == Some(&'/') {
            output.push(' ');
            output.push(' ');
            chars.next();
            for comment in chars.by_ref() {
                if comment == '\n' {
                    output.push('\n');
                    break;
                }
                push_jsonc_comment_mask(&mut output, comment);
            }
            continue;
        }

        if ch == '/' && chars.peek() == Some(&'*') {
            output.push(' ');
            output.push(' ');
            chars.next();
            let mut previous = '\0';
            for comment in chars.by_ref() {
                if comment == '\n' {
                    output.push('\n');
                } else {
                    push_jsonc_comment_mask(&mut output, comment);
                }
                if previous == '*' && comment == '/' {
                    break;
                }
                previous = comment;
            }
            continue;
        }

        output.push(ch);
    }

    output
}

fn push_jsonc_comment_mask(output: &mut String, character: char) {
    output.extend(std::iter::repeat_n(' ', character.len_utf8()));
}

fn strip_jsonc_trailing_commas(source: &str) -> String {
    let mut chars = source.chars().collect::<Vec<_>>();
    let mut index = 0;
    let mut in_string = false;
    let mut escaped = false;

    while index < chars.len() {
        let ch = chars[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }

        if ch == '"' {
            in_string = true;
            index += 1;
            continue;
        }

        if ch == ',' {
            let mut next = index + 1;
            while next < chars.len() && chars[next].is_whitespace() {
                next += 1;
            }
            if next < chars.len() && matches!(chars[next], '}' | ']') {
                chars[index] = ' ';
            }
        }
        index += 1;
    }

    chars.into_iter().collect()
}

fn absolutize_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        normalize_components(path)
    } else {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        normalize_components(&cwd.join(path))
    }
}

fn normalize_components(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Normal(part) => normalized.push(part),
        }
    }

    normalized
}

fn component_to_slash_string(component: Component<'_>) -> Option<String> {
    let value = match component {
        Component::Prefix(prefix) => Some(prefix.as_os_str().to_string_lossy().into_owned()),
        Component::RootDir => Some("/".to_owned()),
        Component::CurDir => None,
        Component::ParentDir => Some("..".to_owned()),
        Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
    }?;

    // Prefix text on Windows may contain backslashes; normalize it here so all
    // machine-readable paths use slash separators.
    Some(value.replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_to_cwd_without_git_marker() {
        let root = discover_project_root(Path::new("."));

        assert!(root.is_absolute());
    }

    #[test]
    fn slash_normalizes_absolute_paths() {
        assert_eq!(
            slash_normalize_path(Path::new("/repo/project")),
            "/repo/project"
        );
        assert_eq!(
            slash_normalize_path(Path::new(r"C:\repo\project")),
            "C:/repo/project"
        );
    }

    #[test]
    fn resolves_explicit_config_from_cwd_without_changing_project_root() {
        let resolved =
            resolve_explicit_config_path(Path::new("/repo/project"), "fixtures/config.json");

        assert_eq!(
            slash_normalize_path(&resolved),
            "/repo/project/fixtures/config.json"
        );
    }
}
