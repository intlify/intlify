// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Shared project-file discovery, physical grouping, and snapshot acquisition.
//!
//! This module owns configuration-authoritative filesystem traversal, canonical
//! logical-path ordering, file-symlink and hard-link grouping, and one bounded
//! freshness-checked byte snapshot per physical group. Definition and reference
//! inventories supply their own selection policy, affected scopes, limits, and
//! domain-specific failure mapping.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::{self, File, Metadata};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use file_id::FileId;
use intlify_contract::{PortablePathSegment, PortableRelativePath, ValueConstructionError};
use intlify_resource::ProjectRelativeResourcePath;
use serde_json::{json, Map, Value};

use crate::error::OperationalError;

/// One representable regular-file candidate discovered beneath the project root.
#[derive(Debug)]
pub(crate) struct ProjectFileCandidate {
    pub(crate) relative: ProjectRelativeResourcePath,
    pub(crate) absolute: PathBuf,
}

/// One traversal issue whose path could not enter the ordinary candidate set.
#[derive(Debug)]
pub(crate) struct ProjectDiscoveryIssue {
    pub(crate) order_path: Option<String>,
    pub(crate) error: OperationalError,
}

/// Complete deterministic result of one project-tree traversal.
#[derive(Debug)]
pub(crate) struct ProjectFileDiscovery {
    pub(crate) candidates: Vec<ProjectFileCandidate>,
    pub(crate) issues: Vec<ProjectDiscoveryIssue>,
}

struct DiscoveryDirectory {
    directory: PathBuf,
    entries: std::vec::IntoIter<io::Result<fs::DirEntry>>,
}

/// Logical project-file input accepted by the physical grouping boundary.
pub(crate) trait ProjectFileTarget {
    /// Return the slash-normalized project-relative logical path.
    fn project_path(&self) -> &str;

    /// Return the host path selected for metadata and snapshot acquisition.
    fn absolute_path(&self) -> &Path;
}

/// One physical regular file and all selected logical aliases.
#[derive(Debug)]
pub(crate) struct PhysicalFileGroup<T> {
    pub(crate) identity: FileId,
    pub(crate) targets: Vec<T>,
}

impl<T: ProjectFileTarget> PhysicalFileGroup<T> {
    /// Return the canonical first logical target.
    pub(crate) fn primary(&self) -> &T {
        &self.targets[0]
    }

    /// Return the canonical primary project-relative path.
    pub(crate) fn primary_path(&self) -> &str {
        self.primary().project_path()
    }
}

/// One selected logical target whose physical identity could not be established.
#[derive(Debug)]
pub(crate) struct PhysicalMetadataFailure<T> {
    pub(crate) target: T,
    pub(crate) error: OperationalError,
}

/// Enumerate every representable file candidate beneath one project root.
///
/// Directory symlinks are never followed. File symlinks, including broken
/// symlinks, remain candidates so a caller's authoritative selection policy can
/// decide whether their later metadata failure is relevant.
pub(crate) fn discover_project_files(project_root: &Path) -> ProjectFileDiscovery {
    let mut discovery = ProjectFileDiscovery {
        candidates: Vec::new(),
        issues: Vec::new(),
    };
    discover_directory(project_root, project_root, &mut discovery);
    discovery.candidates.sort_by(|left, right| {
        compare_portable_path_str(left.relative.as_str(), right.relative.as_str())
    });
    discovery
        .candidates
        .dedup_by(|left, right| left.relative == right.relative);
    discovery
}

fn discover_directory(
    project_root: &Path,
    root_directory: &Path,
    discovery: &mut ProjectFileDiscovery,
) {
    let Some(root) = open_discovery_directory(project_root, root_directory, discovery) else {
        return;
    };
    let mut work = vec![root];

    while !work.is_empty() {
        let entry = work
            .last_mut()
            .expect("the discovery worklist is non-empty")
            .entries
            .next();
        let Some(entry) = entry else {
            work.pop();
            continue;
        };
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                let directory = &work
                    .last()
                    .expect("the current discovery directory remains present")
                    .directory;
                let label = project_relative_label(project_root, directory);
                discovery.issues.push(ProjectDiscoveryIssue {
                    order_path: label.clone(),
                    error: directory_read_error(label.as_deref(), &error),
                });
                continue;
            }
        };
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                record_discovery_path_failure(project_root, &path, &error, discovery);
                continue;
            }
        };

        if file_type.is_dir() {
            // Process each child before resuming its parent to retain exact
            // sorted depth-first traversal without using the Rust call stack.
            if let Some(child) = open_discovery_directory(project_root, &path, discovery) {
                work.push(child);
            }
            continue;
        }

        if file_type.is_symlink() {
            match fs::metadata(&path) {
                Ok(metadata) if metadata.is_dir() => continue,
                Ok(metadata) if metadata.is_file() => {}
                Ok(_) => continue,
                Err(_) => {
                    // A broken selected file symlink is admitted as a candidate.
                }
            }
        } else if !file_type.is_file() {
            continue;
        }

        let Some(relative) = project_relative_resource_path(project_root, &path) else {
            discovery.issues.push(ProjectDiscoveryIssue {
                order_path: project_relative_label(
                    project_root,
                    path.parent().unwrap_or(project_root),
                ),
                error: unrepresentable_discovery_error(project_root, &path),
            });
            continue;
        };
        discovery.candidates.push(ProjectFileCandidate {
            relative,
            absolute: path,
        });
    }
}

fn open_discovery_directory(
    project_root: &Path,
    directory: &Path,
    discovery: &mut ProjectFileDiscovery,
) -> Option<DiscoveryDirectory> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            let label = project_relative_label(project_root, directory);
            discovery.issues.push(ProjectDiscoveryIssue {
                order_path: label.clone(),
                error: directory_read_error(label.as_deref(), &error),
            });
            return None;
        }
    };
    let mut entries = entries.collect::<Vec<_>>();
    entries.sort_by(compare_directory_results);
    Some(DiscoveryDirectory {
        directory: directory.to_owned(),
        entries: entries.into_iter(),
    })
}

fn record_discovery_path_failure(
    project_root: &Path,
    path: &Path,
    error: &io::Error,
    discovery: &mut ProjectFileDiscovery,
) {
    let label = project_relative_label(project_root, path);
    discovery.issues.push(ProjectDiscoveryIssue {
        order_path: label.clone(),
        error: metadata_error(label.as_deref(), error),
    });
}

/// Group selected logical targets by regular-file physical identity.
pub(crate) fn group_physical_files<T: ProjectFileTarget>(
    targets: Vec<T>,
) -> (Vec<PhysicalFileGroup<T>>, Vec<PhysicalMetadataFailure<T>>) {
    let mut grouped = BTreeMap::<FileId, Vec<T>>::new();
    let mut failures = Vec::new();

    for target in targets {
        match inspect_physical_identity(target.absolute_path()) {
            Ok(identity) => grouped.entry(identity).or_default().push(target),
            Err(error) => failures.push(PhysicalMetadataFailure {
                error: metadata_error(Some(target.project_path()), &error),
                target,
            }),
        }
    }

    let mut groups = grouped
        .into_iter()
        .map(|(identity, mut targets)| {
            targets.sort_by(|left, right| {
                compare_portable_path_str(left.project_path(), right.project_path())
            });
            PhysicalFileGroup { identity, targets }
        })
        .collect::<Vec<_>>();
    groups.sort_by(|left, right| {
        compare_portable_path_str(left.primary_path(), right.primary_path())
    });
    failures.sort_by(|left, right| {
        compare_portable_path_str(left.target.project_path(), right.target.project_path())
    });
    (groups, failures)
}

/// Acquire one bounded byte snapshot and verify every alias still identifies it.
pub(crate) fn acquire_physical_snapshot<T: ProjectFileTarget>(
    group: &PhysicalFileGroup<T>,
    byte_limit: u64,
) -> Result<Box<[u8]>, OperationalError> {
    if !group_identity_is_current(group) {
        return Err(source_changed_error(group.primary_path()));
    }

    let primary = group.primary();
    let mut file = File::open(primary.absolute_path())
        .map_err(|error| input_read_error(primary.project_path(), &error))?;
    let before = file
        .metadata()
        .map_err(|error| input_read_error(primary.project_path(), &error))?;
    if !before.is_file() {
        return Err(source_changed_error(primary.project_path()));
    }
    let before = FileState::from_metadata(&before);

    let mut bytes = Vec::new();
    (&mut file)
        .take(byte_limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| input_read_error(primary.project_path(), &error))?;

    let after = file
        .metadata()
        .map_err(|_| source_changed_error(primary.project_path()))?;
    if before != FileState::from_metadata(&after) || !group_identity_is_current(group) {
        return Err(source_changed_error(primary.project_path()));
    }
    Ok(bytes.into_boxed_slice())
}

fn group_identity_is_current<T: ProjectFileTarget>(group: &PhysicalFileGroup<T>) -> bool {
    group.targets.iter().all(|target| {
        inspect_physical_identity(target.absolute_path())
            .is_ok_and(|identity| identity == group.identity)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileState {
    len: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    modified_nanos: i128,
    #[cfg(unix)]
    changed_nanos: i128,
}

impl FileState {
    fn from_metadata(metadata: &Metadata) -> Self {
        Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            modified_nanos: unix_modified_nanos(metadata),
            #[cfg(unix)]
            changed_nanos: unix_changed_nanos(metadata),
        }
    }
}

#[cfg(unix)]
fn unix_modified_nanos(metadata: &Metadata) -> i128 {
    use std::os::unix::fs::MetadataExt;

    i128::from(metadata.mtime()) * 1_000_000_000 + i128::from(metadata.mtime_nsec())
}

#[cfg(unix)]
fn unix_changed_nanos(metadata: &Metadata) -> i128 {
    use std::os::unix::fs::MetadataExt;

    i128::from(metadata.ctime()) * 1_000_000_000 + i128::from(metadata.ctime_nsec())
}

pub(crate) fn inspect_physical_identity(path: &Path) -> io::Result<FileId> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::IsADirectory,
            "selected project inventory participant is not a file",
        ));
    }
    file_id::get_file_id(path)
}

fn project_relative_resource_path(
    project_root: &Path,
    path: &Path,
) -> Option<ProjectRelativeResourcePath> {
    let relative = path.strip_prefix(project_root).ok()?;
    let label = exact_slash_path(relative)?;
    ProjectRelativeResourcePath::try_from(label.as_str()).ok()
}

fn project_relative_label(project_root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(project_root).ok()?;
    if relative.as_os_str().is_empty() {
        return Some(".".to_owned());
    }
    exact_slash_path(relative)
}

fn exact_slash_path(path: &Path) -> Option<String> {
    let mut normalized = String::new();
    for component in path.components() {
        use std::path::Component;
        match component {
            Component::Prefix(prefix) => {
                normalized.push_str(&prefix.as_os_str().to_str()?.replace('\\', "/"));
            }
            Component::RootDir => {
                if !normalized.ends_with('/') {
                    normalized.push('/');
                }
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.is_empty() && !normalized.ends_with('/') {
                    normalized.push('/');
                }
                normalized.push_str("..");
            }
            Component::Normal(part) => {
                if !normalized.is_empty() && !normalized.ends_with('/') {
                    normalized.push('/');
                }
                normalized.push_str(part.to_str()?);
            }
        }
    }
    Some(normalized)
}

/// Compare slash-separated portable paths segment by segment by exact UTF-8.
pub(crate) fn compare_portable_path_str(left: &str, right: &str) -> Ordering {
    let mut left = left.split('/');
    let mut right = right.split('/');
    loop {
        match (left.next(), right.next()) {
            (Some(left), Some(right)) => {
                let order = left.as_bytes().cmp(right.as_bytes());
                if !order.is_eq() {
                    return order;
                }
            }
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (None, None) => return Ordering::Equal,
        }
    }
}

/// Compare optional portable paths with pathless evidence first.
pub(crate) fn compare_optional_paths(left: Option<&str>, right: Option<&str>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => compare_portable_path_str(left, right),
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Convert one admitted slash-normalized project path to the shared contract path.
pub(crate) fn portable_path(path: &str) -> Result<PortableRelativePath, ValueConstructionError> {
    PortableRelativePath::try_new(
        path.split('/')
            .map(PortablePathSegment::try_new)
            .collect::<Result<Vec<_>, _>>()?,
    )
}

fn compare_directory_results(
    left: &Result<fs::DirEntry, io::Error>,
    right: &Result<fs::DirEntry, io::Error>,
) -> Ordering {
    match (left, right) {
        (Ok(left), Ok(right)) => compare_native_os_str(&left.file_name(), &right.file_name()),
        (Ok(_), Err(_)) => Ordering::Less,
        (Err(_), Ok(_)) => Ordering::Greater,
        (Err(_), Err(_)) => Ordering::Equal,
    }
}

#[cfg(unix)]
fn compare_native_os_str(left: &OsStr, right: &OsStr) -> Ordering {
    use std::os::unix::ffi::OsStrExt;

    left.as_bytes().cmp(right.as_bytes())
}

#[cfg(windows)]
fn compare_native_os_str(left: &OsStr, right: &OsStr) -> Ordering {
    use std::os::windows::ffi::OsStrExt;

    left.encode_wide().cmp(right.encode_wide())
}

#[cfg(not(any(unix, windows)))]
fn compare_native_os_str(left: &OsStr, right: &OsStr) -> Ordering {
    left.as_encoded_bytes().cmp(right.as_encoded_bytes())
}

fn directory_read_error(path: Option<&str>, error: &io::Error) -> OperationalError {
    let mut details = io_details(error);
    details.insert("reason".to_owned(), json!("directory_read_failed"));
    OperationalError {
        kind: "io",
        code: "input_read_failed",
        message: "Project inventory directory could not be read.".to_owned(),
        path: path.map(str::to_owned),
        details: Some(Value::Object(details)),
    }
}

fn metadata_error(path: Option<&str>, error: &io::Error) -> OperationalError {
    let mut details = io_details(error);
    details.insert("reason".to_owned(), json!("metadata_failed"));
    OperationalError {
        kind: "io",
        code: "input_read_failed",
        message: "Project inventory file metadata could not be inspected.".to_owned(),
        path: path.map(str::to_owned),
        details: Some(Value::Object(details)),
    }
}

fn input_read_error(path: &str, error: &io::Error) -> OperationalError {
    OperationalError {
        kind: "io",
        code: "input_read_failed",
        message: format!("Project inventory file could not be read: {path}"),
        path: Some(path.to_owned()),
        details: Some(Value::Object(io_details(error))),
    }
}

fn source_changed_error(path: &str) -> OperationalError {
    OperationalError {
        kind: "io",
        code: "input_read_failed",
        message: format!("Project inventory source changed during acquisition: {path}"),
        path: Some(path.to_owned()),
        details: Some(json!({
            "reason": "source_changed"
        })),
    }
}

fn unrepresentable_discovery_error(project_root: &Path, path: &Path) -> OperationalError {
    OperationalError {
        kind: "input",
        code: "input_path_unrepresentable",
        message: "A discovered project inventory path is not valid Unicode.".to_owned(),
        path: None,
        details: Some(json!({
            "reason": "non_unicode_path",
            "source": "discovery",
            "parentPath": project_relative_label(
                project_root,
                path.parent().unwrap_or(project_root)
            )
        })),
    }
}

fn io_details(error: &io::Error) -> Map<String, Value> {
    let mut details = Map::new();
    details.insert("ioKind".to_owned(), json!(normalized_io_kind(error)));
    if let Some(raw_os_error) = error.raw_os_error() {
        details.insert("rawOsError".to_owned(), json!(raw_os_error));
    }
    details
}

fn normalized_io_kind(error: &io::Error) -> &'static str {
    match error.kind() {
        io::ErrorKind::NotFound => "not_found",
        io::ErrorKind::PermissionDenied => "permission_denied",
        io::ErrorKind::IsADirectory => "not_file",
        io::ErrorKind::NotADirectory => "not_directory",
        _ => "unknown",
    }
}
