// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::path::{Component, Path, PathBuf};

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
