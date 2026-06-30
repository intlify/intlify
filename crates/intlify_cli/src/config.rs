// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const CONFIG_JSON: &str = "intlify.config.json";
const CONFIG_JSONC: &str = "intlify.config.jsonc";
const SUPPORTED_EXTENSIONS: [&str; 2] = [".json", ".jsonc"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectConfig {
    pub fmt: EmptyConfig,
    pub lint: EmptyConfig,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            fmt: EmptyConfig {},
            lint: EmptyConfig {},
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
    #[schemars(description = "Formatter configuration. Phase 3A accepts only an empty object.")]
    pub(crate) fmt: EmptyConfig,
    #[schemars(description = "Linter configuration. Phase 3A accepts only an empty object.")]
    pub(crate) lint: EmptyConfig,
}

impl From<ProjectConfigFile> for ProjectConfig {
    fn from(file: ProjectConfigFile) -> Self {
        Self {
            fmt: file.fmt,
            lint: file.lint,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedProjectConfig {
    pub project_root: PathBuf,
    pub config_path: Option<PathBuf>,
    pub source: ConfigSource,
    pub config: ProjectConfig,
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
        let config = load_config_file(&project_root, &path)?;
        return Ok(LoadedProjectConfig {
            project_root,
            config_path: Some(path),
            source: ConfigSource::Explicit,
            config,
        });
    }

    match discover_root_config(&project_root)? {
        Some(path) => {
            let config = load_config_file(&project_root, &path)?;
            Ok(LoadedProjectConfig {
                project_root,
                config_path: Some(path),
                source: ConfigSource::Discovered,
                config,
            })
        }
        None => Ok(LoadedProjectConfig {
            project_root,
            config_path: None,
            source: ConfigSource::Default,
            config: ProjectConfig::default(),
        }),
    }
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

fn load_config_file(project_root: &Path, path: &Path) -> Result<ProjectConfig, ConfigError> {
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
        Some(ConfigSyntax::Json) => parse_json_config(&source, &path_label),
        Some(ConfigSyntax::Jsonc) => {
            let normalized = normalize_jsonc(&source);
            parse_json_config(&normalized, &path_label)
        }
        None => Err(ConfigError::new(
            "config_extension_unsupported",
            format!("Config file extension is not supported: {path_label}"),
            Some(path_label),
            Some(json!({ "supportedExtensions": SUPPORTED_EXTENSIONS })),
        )),
    }
}

fn parse_json_config(source: &str, path_label: &str) -> Result<ProjectConfig, ConfigError> {
    let value = serde_json::from_str::<Value>(source).map_err(|error| {
        ConfigError::new(
            "config_parse_failed",
            format!("Config file could not be parsed: {path_label}"),
            Some(path_label.to_owned()),
            Some(json!({
                "line": error.line(),
                "column": error.column()
            })),
        )
    })?;

    validate_config_value(&value, path_label)?;

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

    Ok(file.into())
}

fn validate_config_value(value: &Value, path_label: &str) -> Result<(), ConfigError> {
    let Some(root) = value.as_object() else {
        return validation_error(path_label, "", "expected_object");
    };

    for (key, value) in root {
        match key.as_str() {
            "$schema" => {
                if !value.is_string() {
                    return validation_error(path_label, "/$schema", "expected_string");
                }
            }
            "fmt" | "lint" => validate_empty_section(path_label, key, value)?,
            key => return validation_error(path_label, &json_pointer(&[key]), "unknown_field"),
        }
    }

    for section in ["fmt", "lint"] {
        if !root.contains_key(section) {
            return validation_error(path_label, &json_pointer(&[section]), "missing_field");
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

    if let Some(key) = fields.keys().next() {
        return validation_error(path_label, &json_pointer(&[section, key]), "unknown_field");
    }

    Ok(())
}

fn validation_error<T>(
    path_label: &str,
    pointer: &str,
    reason: &'static str,
) -> Result<T, ConfigError> {
    Err(ConfigError::new(
        "config_validation_failed",
        format!("Config file is not valid: {path_label}"),
        Some(path_label.to_owned()),
        Some(json!({
            "pointer": pointer,
            "reason": reason
        })),
    ))
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
    strip_jsonc_trailing_commas(&without_comments)
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
                output.push(' ');
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
                    output.push(' ');
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
