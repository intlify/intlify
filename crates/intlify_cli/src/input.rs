// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::{self, DirEntry};
use std::io;
use std::path::{Component, Path, PathBuf};

use file_id::FileId;
use glob::{glob_with, MatchOptions};
use intlify_resource::{
    CatalogAssignmentConflict, CatalogPolicyState, CatalogResolution, DeclaredFormat, HostFormat,
    HostFormatRegistry, ProjectRelativeResourcePath, ResolvedHostFormat, ResolvedResources,
    ResourceError, ResourceErrorDetails,
};
use serde_json::{json, Value};

use crate::error::OperationalError;

const STANDALONE_EXTENSION: &str = ".mf2";
const DEFAULT_EXCLUDED_DIRS: [&str; 8] = [
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "vendor",
    "target",
    "dist",
    "coverage",
];

/// Command-owned ignore behavior applied after supported-input classification.
pub(crate) trait InputIgnore {
    fn is_ignored(&self, path_label: &str) -> bool;

    /// Permit traversal pruning only when no later negation can select a descendant.
    fn can_prune_directory(&self, path_label: &str) -> bool;
}

/// Resource classification is implemented here before a command enables its consumer.
#[derive(Clone, Copy)]
pub(crate) enum CatalogSelection<'a> {
    Disabled,
    #[allow(dead_code)] // Enabled by the catalog command consumer in PR 7.
    Enabled {
        resources: &'a ResolvedResources,
        registry: &'a HostFormatRegistry,
        config_path: Option<&'a Path>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiscoveredCandidate {
    pub(crate) logical_path: PathBuf,
    pub(crate) normalized_absolute_path: String,
    pub(crate) label: String,
    pub(crate) origins: Vec<CandidateOrigin>,
}

impl DiscoveredCandidate {
    fn has_direct_origin(&self) -> bool {
        self.origins
            .iter()
            .any(|origin| matches!(origin, CandidateOrigin::DirectFile { .. }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateOrigin {
    DirectFile { operand_index: usize },
    Directory { operand_index: usize },
    CliGlob { operand_index: usize },
}

#[derive(Debug, Clone)]
pub(crate) enum WorkflowClassification {
    StandaloneMf2,
    Catalog {
        format: HostFormat,
        #[allow(dead_code)] // Carried intact for extraction by the PR 7 consumer.
        resolved: ResolvedHostFormat,
    },
}

impl WorkflowClassification {
    pub(crate) fn token(&self) -> String {
        match self {
            Self::StandaloneMf2 => "standalone:mf2".to_owned(),
            Self::Catalog { format, .. } => format!("catalog:{}", format.as_str()),
        }
    }

    fn is_same_workflow(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::StandaloneMf2, Self::StandaloneMf2) => true,
            (Self::Catalog { format: left, .. }, Self::Catalog { format: right, .. }) => {
                left == right
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SelectedTarget {
    pub(crate) candidate: DiscoveredCandidate,
    pub(crate) classification: WorkflowClassification,
}

#[derive(Debug)]
pub(crate) struct PhysicalFileGroup {
    pub(crate) aliases: Vec<SelectedTarget>,
}

#[derive(Debug)]
pub(crate) struct TargetSelectionError {
    pub(crate) target: SelectedTarget,
    pub(crate) error: OperationalError,
}

#[derive(Debug)]
pub(crate) enum ExecutionUnit {
    Group(PhysicalFileGroup),
    TargetError(Box<TargetSelectionError>),
}

impl ExecutionUnit {
    fn first_path(&self) -> &str {
        match self {
            Self::Group(group) => &group.aliases[0].candidate.normalized_absolute_path,
            Self::TargetError(failure) => &failure.target.candidate.normalized_absolute_path,
        }
    }
}

#[derive(Debug)]
pub(crate) struct FileSelection {
    pub(crate) units: Vec<ExecutionUnit>,
    pub(crate) errors: Vec<OperationalError>,
    /// An assignment conflict invalidates the complete target set.
    pub(crate) aborted: bool,
}

#[derive(Debug)]
pub(crate) enum StdinSelection {
    Selected {
        label: String,
        classification: WorkflowClassification,
    },
    Skipped {
        label: String,
    },
    Error(OperationalError),
}

#[derive(Debug)]
struct Discovery {
    candidates: BTreeMap<String, DiscoveredCandidate>,
    errors: Vec<OperationalError>,
}

pub(crate) fn select_file_inputs(
    cwd: &Path,
    project_root: &Path,
    operands: &[String],
    ignore: &dyn InputIgnore,
    catalogs: CatalogSelection<'_>,
) -> FileSelection {
    let discovery = discover_candidates(cwd, project_root, operands, ignore);
    let mut errors = discovery.errors;
    let mut selected = Vec::new();
    let mut first_assignment_conflict = None;

    for candidate in discovery.candidates.into_values() {
        match classify_candidate(project_root, candidate, &catalogs) {
            CandidateClassification::Selected(target) => selected.push(target),
            CandidateClassification::Skipped => {}
            CandidateClassification::Error(error) => errors.push(error),
            CandidateClassification::AssignmentConflict(conflict) => {
                if first_assignment_conflict.is_none() {
                    first_assignment_conflict = Some(conflict);
                }
            }
        }
    }

    if let Some(conflict) = first_assignment_conflict {
        errors.push(catalog_assignment_conflict_error(
            project_root,
            &catalogs,
            &conflict,
        ));
        return FileSelection {
            units: Vec::new(),
            errors,
            aborted: true,
        };
    }

    selected.retain(|target| !ignore.is_ignored(&target.candidate.label));
    let (mut units, physical_conflicts) = group_physical_targets(selected);
    errors.extend(physical_conflicts);
    units.sort_by(|left, right| left.first_path().cmp(right.first_path()));

    FileSelection {
        units,
        errors,
        aborted: false,
    }
}

pub(crate) fn select_stdin_input(
    cwd: &Path,
    project_root: &Path,
    stdin_filepath: &str,
    catalogs: CatalogSelection<'_>,
) -> StdinSelection {
    let logical_path = resolve_operand_path(cwd, stdin_filepath);
    let Some(normalized_absolute_path) = exact_slash_path(&logical_path) else {
        return StdinSelection::Error(unrepresentable_stdin_error());
    };
    let Some(label) = exact_display_path(project_root, &logical_path) else {
        return StdinSelection::Error(unrepresentable_stdin_error());
    };
    let candidate = DiscoveredCandidate {
        logical_path,
        normalized_absolute_path,
        label: label.clone(),
        origins: Vec::new(),
    };

    match classify_logical_input(project_root, candidate, &catalogs, true) {
        CandidateClassification::Selected(target) => StdinSelection::Selected {
            label,
            classification: target.classification,
        },
        CandidateClassification::Skipped => StdinSelection::Skipped { label },
        CandidateClassification::Error(error) => StdinSelection::Error(error),
        CandidateClassification::AssignmentConflict(conflict) => StdinSelection::Error(
            catalog_assignment_conflict_error(project_root, &catalogs, &conflict),
        ),
    }
}

fn discover_candidates(
    cwd: &Path,
    project_root: &Path,
    operands: &[String],
    ignore: &dyn InputIgnore,
) -> Discovery {
    let mut discovery = Discovery {
        candidates: BTreeMap::new(),
        errors: Vec::new(),
    };

    for (operand_index, operand) in operands.iter().enumerate() {
        if has_glob_meta(operand) {
            discover_glob(
                cwd,
                project_root,
                operand,
                operand_index,
                ignore,
                &mut discovery,
            );
        } else {
            discover_path(
                cwd,
                project_root,
                operand,
                operand_index,
                ignore,
                &mut discovery,
            );
        }
    }

    discovery
}

fn discover_path(
    cwd: &Path,
    project_root: &Path,
    operand: &str,
    operand_index: usize,
    ignore: &dyn InputIgnore,
    discovery: &mut Discovery,
) {
    let path = resolve_operand_path(cwd, operand);
    if exact_slash_path(&path).is_none() {
        discovery
            .errors
            .push(unrepresentable_operand_error(operand_index));
        return;
    }

    let Ok(metadata) = fs::symlink_metadata(&path) else {
        discovery
            .errors
            .push(unmatched_input_error(operand, "path"));
        return;
    };

    if metadata.file_type().is_symlink() {
        match fs::metadata(&path) {
            Ok(target) if target.is_dir() => {}
            Ok(target) if target.is_file() => add_candidate(
                project_root,
                &path,
                CandidateOrigin::DirectFile { operand_index },
                None,
                discovery,
            ),
            Ok(_) => discovery.errors.push(unsupported_input_error(
                &exact_display_path(project_root, &path).expect("operand path is Unicode"),
                &path,
                &[STANDALONE_EXTENSION],
            )),
            // Retain a broken logical file alias so selected inputs receive the
            // target-local metadata failure required by the grouping boundary.
            Err(_) => add_candidate(
                project_root,
                &path,
                CandidateOrigin::DirectFile { operand_index },
                None,
                discovery,
            ),
        }
    } else if metadata.is_dir() {
        collect_directory(project_root, &path, operand_index, ignore, discovery);
    } else if metadata.is_file() {
        add_candidate(
            project_root,
            &path,
            CandidateOrigin::DirectFile { operand_index },
            None,
            discovery,
        );
    } else {
        discovery.errors.push(unsupported_input_error(
            &exact_display_path(project_root, &path).expect("operand path is Unicode"),
            &path,
            &[STANDALONE_EXTENSION],
        ));
    }
}

fn collect_directory(
    project_root: &Path,
    directory: &Path,
    operand_index: usize,
    ignore: &dyn InputIgnore,
    discovery: &mut Discovery,
) {
    let directory_label = exact_display_path(project_root, directory)
        .expect("only representable directories enter traversal");
    if directory != project_root && ignore.can_prune_directory(&directory_label) {
        return;
    }

    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            discovery
                .errors
                .push(input_read_error(&directory_label, &error));
            return;
        }
    };
    let mut entries = entries.collect::<Vec<_>>();
    entries.sort_by(compare_directory_results);

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                discovery
                    .errors
                    .push(input_read_error(&directory_label, &error));
                continue;
            }
        };
        let path = entry.path();
        if exact_slash_path(&path).is_none() {
            discovery
                .errors
                .push(unrepresentable_discovery_error(&directory_label));
            continue;
        }
        if should_skip_bulk_path(project_root, &path) {
            continue;
        }

        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                let label = exact_display_path(project_root, &path).expect("entry path is Unicode");
                discovery.errors.push(input_read_error(&label, &error));
                continue;
            }
        };

        if file_type.is_symlink() {
            match fs::metadata(&path) {
                Ok(target) if target.is_file() => add_candidate(
                    project_root,
                    &path,
                    CandidateOrigin::Directory { operand_index },
                    Some(&directory_label),
                    discovery,
                ),
                Ok(_) => {}
                Err(_) => add_candidate(
                    project_root,
                    &path,
                    CandidateOrigin::Directory { operand_index },
                    Some(&directory_label),
                    discovery,
                ),
            }
        } else if file_type.is_dir() {
            let label = exact_display_path(project_root, &path).expect("entry path is Unicode");
            if !ignore.can_prune_directory(&label) {
                collect_directory(project_root, &path, operand_index, ignore, discovery);
            }
        } else if file_type.is_file() {
            add_candidate(
                project_root,
                &path,
                CandidateOrigin::Directory { operand_index },
                Some(&directory_label),
                discovery,
            );
        }
    }
}

fn discover_glob(
    cwd: &Path,
    project_root: &Path,
    operand: &str,
    operand_index: usize,
    _ignore: &dyn InputIgnore,
    discovery: &mut Discovery,
) {
    let Some(pattern) = resolve_glob_pattern(cwd, operand) else {
        discovery
            .errors
            .push(unrepresentable_operand_error(operand_index));
        return;
    };
    let Ok(entries) = glob_with(&pattern, glob_match_options()) else {
        discovery.errors.push(invalid_glob_error(operand));
        return;
    };

    let mut matched_entries = 0usize;
    for entry in entries {
        matched_entries += 1;
        let path = match entry {
            Ok(path) => path,
            Err(error) => {
                let parent = error.path().parent().unwrap_or(project_root);
                let Some(parent_label) = exact_display_path(project_root, parent) else {
                    discovery
                        .errors
                        .push(unrepresentable_operand_error(operand_index));
                    continue;
                };
                if exact_slash_path(error.path()).is_none() {
                    discovery
                        .errors
                        .push(unrepresentable_discovery_error(&parent_label));
                } else {
                    let label = exact_display_path(project_root, error.path())
                        .expect("glob error path is Unicode");
                    discovery
                        .errors
                        .push(input_read_error(&label, error.error()));
                }
                continue;
            }
        };
        let Some(parent_label) = path
            .parent()
            .and_then(|parent| exact_display_path(project_root, parent))
        else {
            discovery
                .errors
                .push(unrepresentable_operand_error(operand_index));
            continue;
        };
        if exact_slash_path(&path).is_none() {
            discovery
                .errors
                .push(unrepresentable_discovery_error(&parent_label));
            continue;
        }
        if should_skip_bulk_path(project_root, &path) {
            continue;
        }

        match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => add_candidate(
                project_root,
                &path,
                CandidateOrigin::CliGlob { operand_index },
                Some(&parent_label),
                discovery,
            ),
            Ok(_) => {}
            Err(_) => add_candidate(
                project_root,
                &path,
                CandidateOrigin::CliGlob { operand_index },
                Some(&parent_label),
                discovery,
            ),
        }
    }

    if matched_entries == 0 {
        discovery
            .errors
            .push(unmatched_input_error(operand, "glob"));
    }
}

fn add_candidate(
    project_root: &Path,
    path: &Path,
    origin: CandidateOrigin,
    discovered_parent: Option<&str>,
    discovery: &mut Discovery,
) {
    let path = normalize_path(path);
    let Some(normalized_absolute_path) = exact_slash_path(&path) else {
        let error = discovered_parent.map_or_else(
            || unrepresentable_operand_error(origin.operand_index()),
            unrepresentable_discovery_error,
        );
        discovery.errors.push(error);
        return;
    };
    let label = exact_display_path(project_root, &path)
        .expect("an exact absolute path has an exact display identity");

    match discovery.candidates.entry(normalized_absolute_path.clone()) {
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(DiscoveredCandidate {
                logical_path: path,
                normalized_absolute_path,
                label,
                origins: vec![origin],
            });
        }
        std::collections::btree_map::Entry::Occupied(mut entry) => {
            entry.get_mut().origins.push(origin);
        }
    }
}

impl CandidateOrigin {
    const fn operand_index(self) -> usize {
        match self {
            Self::DirectFile { operand_index }
            | Self::Directory { operand_index }
            | Self::CliGlob { operand_index } => operand_index,
        }
    }
}

enum CandidateClassification {
    Selected(SelectedTarget),
    Skipped,
    Error(OperationalError),
    AssignmentConflict(PendingAssignmentConflict),
}

struct PendingAssignmentConflict {
    target_path: String,
    conflict: CatalogAssignmentConflict,
}

fn classify_candidate(
    project_root: &Path,
    candidate: DiscoveredCandidate,
    catalogs: &CatalogSelection<'_>,
) -> CandidateClassification {
    let direct = candidate.has_direct_origin();
    classify_logical_input(project_root, candidate, catalogs, direct)
}

fn classify_logical_input(
    project_root: &Path,
    candidate: DiscoveredCandidate,
    catalogs: &CatalogSelection<'_>,
    direct: bool,
) -> CandidateClassification {
    if has_extension(&candidate.logical_path, STANDALONE_EXTENSION) {
        return CandidateClassification::Selected(SelectedTarget {
            candidate,
            classification: WorkflowClassification::StandaloneMf2,
        });
    }

    let CatalogSelection::Enabled {
        resources,
        registry,
        ..
    } = catalogs
    else {
        return if direct {
            CandidateClassification::Error(unsupported_input_error(
                &candidate.label,
                &candidate.logical_path,
                &[STANDALONE_EXTENSION],
            ))
        } else {
            CandidateClassification::Skipped
        };
    };

    let relative = project_relative_path(project_root, &candidate.logical_path);
    let resolution = match relative.as_ref() {
        Some(relative) => match resources.resolve_path(relative) {
            Ok(resolution) => resolution,
            Err(conflict) => {
                return CandidateClassification::AssignmentConflict(PendingAssignmentConflict {
                    target_path: relative.as_str().to_owned(),
                    conflict,
                });
            }
        },
        None => match resources.policy_state() {
            CatalogPolicyState::Absent => CatalogResolution::PolicyAbsent,
            CatalogPolicyState::Empty => CatalogResolution::PolicyEmpty,
            CatalogPolicyState::Configured => CatalogResolution::Unmatched,
        },
    };

    match resolution {
        CatalogResolution::Matched(assignment) => match registry.resolve_format(&assignment) {
            Ok(resolved) => CandidateClassification::Selected(SelectedTarget {
                candidate,
                classification: WorkflowClassification::Catalog {
                    format: resolved.format(),
                    resolved,
                },
            }),
            Err(error) => {
                CandidateClassification::Error(resource_selection_error(&candidate.label, &error))
            }
        },
        CatalogResolution::PolicyAbsent if direct => {
            let extension = extension_label(&candidate.logical_path);
            match registry.resolve_direct_extension(&extension) {
                Some(resolved) => CandidateClassification::Selected(SelectedTarget {
                    candidate,
                    classification: WorkflowClassification::Catalog {
                        format: resolved.format(),
                        resolved,
                    },
                }),
                None => CandidateClassification::Error(unsupported_input_error(
                    &candidate.label,
                    &candidate.logical_path,
                    &supported_direct_extensions(registry),
                )),
            }
        }
        CatalogResolution::Unmatched if direct => {
            CandidateClassification::Error(unsupported_input_error(
                &candidate.label,
                &candidate.logical_path,
                &supported_direct_extensions(registry),
            ))
        }
        CatalogResolution::PolicyEmpty
        | CatalogResolution::Excluded
        | CatalogResolution::PolicyAbsent
        | CatalogResolution::Unmatched => CandidateClassification::Skipped,
    }
}

fn supported_direct_extensions(registry: &HostFormatRegistry) -> Vec<&'static str> {
    let mut extensions = registry.supported_direct_extensions().to_vec();
    extensions.push(STANDALONE_EXTENSION);
    extensions.sort_unstable();
    extensions.dedup();
    extensions
}

fn resource_selection_error(path_label: &str, error: &ResourceError) -> OperationalError {
    let details = match error.details() {
        ResourceErrorDetails::FormatUnsupported {
            classification_source,
            declared_format,
            format,
            extension,
            outer_format,
            supported_formats,
        } => {
            let mut details = serde_json::Map::new();
            details.insert(
                "classificationSource".to_owned(),
                json!(classification_source.as_str()),
            );
            if !matches!(declared_format, DeclaredFormat::Absent) {
                details.insert(
                    "declaredFormat".to_owned(),
                    match declared_format {
                        DeclaredFormat::Absent | DeclaredFormat::Valueless => Value::Null,
                        DeclaredFormat::Value(value) => json!(value.as_ref()),
                    },
                );
            }
            if let Some(format) = format {
                details.insert("format".to_owned(), json!(format));
            }
            details.insert("extension".to_owned(), json!(extension.as_ref()));
            if let Some(outer_format) = outer_format {
                details.insert("outerFormat".to_owned(), json!(outer_format));
            }
            details.insert(
                "supportedFormats".to_owned(),
                json!(supported_formats.as_ref()),
            );
            Value::Object(details)
        }
        _ => json!({ "phase": error.phase().as_str() }),
    };

    OperationalError {
        kind: if error.code().as_str() == "internal_error" {
            "internal"
        } else {
            "input"
        },
        code: error.code().as_str(),
        message: format!("Resource input could not be selected: {path_label}"),
        path: Some(path_label.to_owned()),
        details: Some(details),
    }
}

fn catalog_assignment_conflict_error(
    project_root: &Path,
    catalogs: &CatalogSelection<'_>,
    pending: &PendingAssignmentConflict,
) -> OperationalError {
    let config_path = match catalogs {
        CatalogSelection::Enabled { config_path, .. } => {
            config_path.and_then(|path| exact_display_path(project_root, path))
        }
        CatalogSelection::Disabled => None,
    };
    OperationalError {
        kind: "config",
        code: "config_validation_failed",
        message: "Resource catalog definitions assign conflicting formats.".to_owned(),
        path: config_path,
        details: Some(json!({
            "reason": "catalog_format_conflict",
            "pointer": format!(
                "/resources/catalogs/{}",
                pending.conflict.assignment().definition_index()
            ),
            "conflictingPointer": format!(
                "/resources/catalogs/{}",
                pending.conflict.conflicting_assignment().definition_index()
            ),
            "targetPath": pending.target_path
        })),
    }
}

fn group_physical_targets(
    selected: Vec<SelectedTarget>,
) -> (Vec<ExecutionUnit>, Vec<OperationalError>) {
    // `FileId` is device/inode on Unix and native volume/file identity on
    // Windows while keeping platform-specific unsafe code outside this crate.
    let mut groups = BTreeMap::<FileId, Vec<SelectedTarget>>::new();
    let mut target_errors = Vec::new();

    for target in selected {
        match inspect_physical_identity(&target.candidate.logical_path) {
            Ok(identity) => groups.entry(identity).or_default().push(target),
            Err(error) => {
                target_errors.push(ExecutionUnit::TargetError(Box::new(TargetSelectionError {
                    error: metadata_error(&target.candidate.label, &error),
                    target,
                })));
            }
        }
    }

    let mut units = target_errors;
    let mut conflicts = Vec::new();
    for (_, mut aliases) in groups {
        aliases.sort_by(|left, right| {
            left.candidate
                .normalized_absolute_path
                .cmp(&right.candidate.normalized_absolute_path)
        });
        if aliases[1..].iter().any(|alias| {
            !aliases[0]
                .classification
                .is_same_workflow(&alias.classification)
        }) {
            conflicts.push((
                aliases[0].candidate.normalized_absolute_path.clone(),
                input_target_conflict_error(&aliases),
            ));
        } else {
            units.push(ExecutionUnit::Group(PhysicalFileGroup { aliases }));
        }
    }
    conflicts.sort_by(|left, right| left.0.cmp(&right.0));
    (
        units,
        conflicts.into_iter().map(|(_, error)| error).collect(),
    )
}

fn inspect_physical_identity(path: &Path) -> io::Result<FileId> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::IsADirectory,
            "selected input is not a file",
        ));
    }
    file_id::get_file_id(path)
}

fn input_target_conflict_error(aliases: &[SelectedTarget]) -> OperationalError {
    OperationalError {
        kind: "input",
        code: "input_target_conflict",
        message: "The same physical input was selected with conflicting classifications."
            .to_owned(),
        path: None,
        details: Some(json!({
            "paths": aliases
                .iter()
                .map(|alias| alias.candidate.label.as_str())
                .collect::<Vec<_>>(),
            "classifications": aliases
                .iter()
                .map(|alias| alias.classification.token())
                .collect::<Vec<_>>()
        })),
    }
}

fn metadata_error(path_label: &str, error: &io::Error) -> OperationalError {
    let mut details = serde_json::Map::new();
    details.insert("reason".to_owned(), json!("metadata_failed"));
    details.insert("ioKind".to_owned(), json!(normalized_io_kind(error)));
    if let Some(raw_os_error) = error.raw_os_error() {
        details.insert("rawOsError".to_owned(), json!(raw_os_error));
    }
    OperationalError {
        kind: "io",
        code: "input_read_failed",
        message: format!("Input file metadata could not be inspected: {path_label}"),
        path: Some(path_label.to_owned()),
        details: Some(Value::Object(details)),
    }
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

fn unsupported_input_error(
    path_label: &str,
    path: &Path,
    supported_extensions: &[&str],
) -> OperationalError {
    OperationalError {
        kind: "input",
        code: "unsupported_input_file",
        message: format!("Input file extension is not supported: {path_label}"),
        path: Some(path_label.to_owned()),
        details: Some(json!({
            "extension": extension_label(path),
            "supportedExtensions": supported_extensions
        })),
    }
}

fn invalid_glob_error(input: &str) -> OperationalError {
    OperationalError {
        kind: "input",
        code: "invalid_cli_argument",
        message: format!("Input glob is invalid: {input}"),
        path: None,
        details: Some(json!({
            "input": input,
            "kind": "glob",
            "reason": "invalid_glob"
        })),
    }
}

fn unmatched_input_error(input: &str, kind: &'static str) -> OperationalError {
    OperationalError {
        kind: "input",
        code: "unmatched_input",
        message: format!("Input did not match any filesystem entries: {input}"),
        path: None,
        details: Some(json!({ "input": input, "kind": kind })),
    }
}

fn unrepresentable_operand_error(operand_index: usize) -> OperationalError {
    OperationalError {
        kind: "input",
        code: "input_path_unrepresentable",
        message: "An input operand path is not valid Unicode.".to_owned(),
        path: None,
        details: Some(json!({
            "reason": "non_unicode_path",
            "source": "operand",
            "operandIndex": operand_index
        })),
    }
}

fn unrepresentable_discovery_error(parent_path: &str) -> OperationalError {
    OperationalError {
        kind: "input",
        code: "input_path_unrepresentable",
        message: "A discovered input path is not valid Unicode.".to_owned(),
        path: None,
        details: Some(json!({
            "reason": "non_unicode_path",
            "source": "discovery",
            "parentPath": parent_path
        })),
    }
}

fn unrepresentable_stdin_error() -> OperationalError {
    OperationalError {
        kind: "input",
        code: "input_path_unrepresentable",
        message: "The stdin filepath is not valid Unicode.".to_owned(),
        path: None,
        details: Some(json!({
            "reason": "non_unicode_path",
            "source": "stdin-filepath"
        })),
    }
}

fn input_read_error(path_label: &str, error: &io::Error) -> OperationalError {
    let mut details = serde_json::Map::new();
    details.insert("ioKind".to_owned(), json!(format!("{:?}", error.kind())));
    if let Some(raw_os_error) = error.raw_os_error() {
        details.insert("rawOsError".to_owned(), json!(raw_os_error));
    }
    OperationalError {
        kind: "io",
        code: "input_read_failed",
        message: format!("Input file could not be read: {path_label}"),
        path: Some(path_label.to_owned()),
        details: Some(Value::Object(details)),
    }
}

fn extension_label(path: &Path) -> String {
    let Some(basename) = path.file_name().and_then(OsStr::to_str) else {
        return String::new();
    };
    basename
        .char_indices()
        .rev()
        .find_map(|(index, character)| (character == '.').then_some(index))
        .filter(|index| *index != 0 && *index + 1 < basename.len())
        .map_or_else(String::new, |index| basename[index..].to_owned())
}

fn has_extension(path: &Path, extension: &str) -> bool {
    extension_label(path).eq_ignore_ascii_case(extension)
}

fn project_relative_path(
    project_root: &Path,
    logical_path: &Path,
) -> Option<ProjectRelativeResourcePath> {
    let relative = logical_path.strip_prefix(project_root).ok()?;
    if relative.as_os_str().is_empty() {
        return None;
    }
    ProjectRelativeResourcePath::try_from(exact_slash_path(relative)?.as_str()).ok()
}

fn has_glob_meta(input: &str) -> bool {
    input.contains('*') || input.contains('?') || input.contains('[')
}

fn should_skip_bulk_path(project_root: &Path, path: &Path) -> bool {
    let relative = path.strip_prefix(project_root).unwrap_or(path);
    relative.components().any(|component| match component {
        Component::Normal(part) => part
            .to_str()
            .is_some_and(|part| is_hidden_name(part) || DEFAULT_EXCLUDED_DIRS.contains(&part)),
        _ => false,
    })
}

fn is_hidden_name(name: &str) -> bool {
    name.starts_with('.') && name != "." && name != ".."
}

fn exact_display_path(project_root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(project_root)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .map_or_else(|| exact_slash_path(path), exact_slash_path)
}

fn exact_slash_path(path: &Path) -> Option<String> {
    let mut normalized = String::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                let value = prefix.as_os_str().to_str()?.replace('\\', "/");
                normalized.push_str(&value);
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

fn resolve_operand_path(cwd: &Path, operand: &str) -> PathBuf {
    let path = Path::new(operand);
    if path.is_absolute() {
        normalize_path(path)
    } else {
        let absolute_cwd = if cwd.is_absolute() {
            normalize_path(cwd)
        } else {
            let process_cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            normalize_path(&process_cwd.join(cwd))
        };
        normalize_path(&absolute_cwd.join(path))
    }
}

fn resolve_glob_pattern(cwd: &Path, operand: &str) -> Option<String> {
    exact_slash_path(&resolve_operand_path(cwd, operand))
}

fn normalize_path(path: &Path) -> PathBuf {
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

fn glob_match_options() -> MatchOptions {
    MatchOptions {
        case_sensitive: true,
        require_literal_separator: true,
        require_literal_leading_dot: false,
    }
}

fn compare_directory_results(
    left: &Result<DirEntry, io::Error>,
    right: &Result<DirEntry, io::Error>,
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::ops::Deref;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use intlify_resource::{HostFormatRegistry, ResourcesConfig};
    use serde_json::{json, Value};

    use super::{
        select_file_inputs, select_stdin_input, CandidateOrigin, CatalogSelection, ExecutionUnit,
        FileSelection, InputIgnore, StdinSelection,
    };

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new(name: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after the epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "intlify-input-{name}-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("temp root should be created");
            Self(path)
        }
    }

    impl Deref for TempRoot {
        type Target = Path;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    struct NoIgnore;

    impl InputIgnore for NoIgnore {
        fn is_ignored(&self, _: &str) -> bool {
            false
        }

        fn can_prune_directory(&self, _: &str) -> bool {
            false
        }
    }

    struct IgnoreAll;

    impl InputIgnore for IgnoreAll {
        fn is_ignored(&self, _: &str) -> bool {
            true
        }

        fn can_prune_directory(&self, _: &str) -> bool {
            false
        }
    }

    fn write(path: &Path, source: &str) {
        fs::create_dir_all(path.parent().expect("fixture parent"))
            .expect("fixture parent should be created");
        fs::write(path, source).expect("fixture should be written");
    }

    fn resources(value: Option<&Value>) -> intlify_resource::ResolvedResources {
        ResourcesConfig::validate(value)
            .expect("resource config should be valid")
            .resolve()
    }

    fn enabled_selection(
        root: &Path,
        operands: &[&str],
        resolved: &intlify_resource::ResolvedResources,
        ignore: &dyn InputIgnore,
    ) -> FileSelection {
        let registry = HostFormatRegistry::new();
        let operands = operands
            .iter()
            .map(|operand| (*operand).to_owned())
            .collect::<Vec<_>>();
        select_file_inputs(
            root,
            root,
            &operands,
            ignore,
            CatalogSelection::Enabled {
                resources: resolved,
                registry: &registry,
                config_path: Some(&root.join("intlify.config.json")),
            },
        )
    }

    fn disabled_selection(root: &Path, operands: &[&str]) -> FileSelection {
        let operands = operands
            .iter()
            .map(|operand| (*operand).to_owned())
            .collect::<Vec<_>>();
        select_file_inputs(root, root, &operands, &NoIgnore, CatalogSelection::Disabled)
    }

    fn enabled_stdin_selection(
        root: &Path,
        stdin_filepath: &str,
        resolved: &intlify_resource::ResolvedResources,
    ) -> StdinSelection {
        let registry = HostFormatRegistry::new();
        select_stdin_input(
            root,
            root,
            stdin_filepath,
            CatalogSelection::Enabled {
                resources: resolved,
                registry: &registry,
                config_path: Some(&root.join("intlify.config.json")),
            },
        )
    }

    fn selected_aliases(selection: &FileSelection) -> Vec<&super::SelectedTarget> {
        selection
            .units
            .iter()
            .filter_map(|unit| match unit {
                ExecutionUnit::Group(group) => Some(group.aliases.iter()),
                ExecutionUnit::TargetError(_) => None,
            })
            .flatten()
            .collect()
    }

    #[test]
    fn merges_direct_and_bulk_origins_and_keeps_direct_catalog_semantics() {
        let root = TempRoot::new("origins");
        write(&root.join("locales/en.JSON"), "{}");
        let resolved = resources(None);

        let selection =
            enabled_selection(&root, &["locales", "locales/en.JSON"], &resolved, &NoIgnore);
        let aliases = selected_aliases(&selection);

        assert!(selection.errors.is_empty());
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].candidate.label, "locales/en.JSON");
        assert_eq!(aliases[0].classification.token(), "catalog:json");
        assert_eq!(
            aliases[0].candidate.origins,
            [
                CandidateOrigin::Directory { operand_index: 0 },
                CandidateOrigin::DirectFile { operand_index: 1 }
            ]
        );
    }

    #[test]
    fn absent_policy_bulk_discovery_selects_only_standalone_inputs() {
        let root = TempRoot::new("bulk-absent");
        write(&root.join("messages/a.mf2"), "Hello");
        write(&root.join("messages/b.json"), "{}");
        write(&root.join("messages/c.txt"), "notes");
        let resolved = resources(None);

        let selection = enabled_selection(&root, &["."], &resolved, &NoIgnore);
        let aliases = selected_aliases(&selection);

        assert!(selection.errors.is_empty());
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].candidate.label, "messages/a.mf2");
        assert_eq!(aliases[0].classification.token(), "standalone:mf2");
    }

    #[test]
    fn stdin_uses_the_same_direct_and_policy_classification() {
        let root = TempRoot::new("stdin-selection");
        let absent = resources(None);

        let StdinSelection::Selected {
            label,
            classification,
        } = enabled_stdin_selection(&root, "virtual/en.JSON", &absent)
        else {
            panic!("direct JSON stdin should be selected");
        };
        assert_eq!(label, "virtual/en.JSON");
        assert_eq!(classification.token(), "catalog:json");

        let empty = resources(Some(&json!({ "catalogs": [] })));
        let StdinSelection::Skipped { label } =
            enabled_stdin_selection(&root, "virtual/en.json", &empty)
        else {
            panic!("empty catalog policy should skip stdin");
        };
        assert_eq!(label, "virtual/en.json");

        let configured = resources(Some(&json!({
            "catalogs": [{
                "include": ["virtual/**"],
                "format": "json"
            }]
        })));
        let StdinSelection::Selected { classification, .. } =
            enabled_stdin_selection(&root, "virtual/messages", &configured)
        else {
            panic!("matched extensionless stdin should be selected");
        };
        assert_eq!(classification.token(), "catalog:json");
    }

    #[test]
    fn disabled_stdin_selection_preserves_the_standalone_integration_gate() {
        let root = TempRoot::new("stdin-gate");

        let StdinSelection::Selected { classification, .. } = select_stdin_input(
            &root,
            &root,
            "virtual/message.MF2",
            CatalogSelection::Disabled,
        ) else {
            panic!("standalone stdin should be selected");
        };
        assert_eq!(classification.token(), "standalone:mf2");

        let StdinSelection::Error(error) =
            select_stdin_input(&root, &root, "virtual/en.json", CatalogSelection::Disabled)
        else {
            panic!("catalog stdin should remain gated");
        };
        assert_eq!(error.code, "unsupported_input_file");
        assert_eq!(
            error.details.expect("details")["supportedExtensions"],
            json!([".mf2"])
        );
    }

    #[test]
    fn direct_extension_classification_is_case_insensitive_and_explicit() {
        let root = TempRoot::new("direct-extensions");
        write(&root.join("upper.JSON"), "{}");
        write(&root.join("future.yaml"), "message: Hello");
        write(&root.join("messages"), "Hello");
        write(&root.join("trailing."), "Hello");
        write(&root.join(".json"), "{}");
        write(&root.join("unknown.txt"), "Hello");
        let resolved = resources(None);

        let selection = enabled_selection(
            &root,
            &[
                "upper.JSON",
                "future.yaml",
                "messages",
                "trailing.",
                ".json",
                "unknown.txt",
            ],
            &resolved,
            &NoIgnore,
        );
        let aliases = selected_aliases(&selection);

        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].classification.token(), "catalog:json");
        assert_eq!(selection.errors.len(), 5);
        assert!(selection
            .errors
            .iter()
            .all(|error| error.code == "unsupported_input_file"));
        for error in &selection.errors {
            assert_eq!(
                error.details.as_ref().expect("details")["supportedExtensions"],
                json!([".json", ".mf2"])
            );
        }
        assert_eq!(
            selection
                .errors
                .iter()
                .find(|error| error.path.as_deref() == Some(".json"))
                .and_then(|error| error.details.as_ref())
                .expect("dot file error")["extension"],
            ""
        );
        assert_eq!(
            selection
                .errors
                .iter()
                .find(|error| error.path.as_deref() == Some("trailing."))
                .and_then(|error| error.details.as_ref())
                .expect("trailing dot error")["extension"],
            ""
        );
    }

    #[test]
    fn project_policy_distinguishes_empty_unmatched_excluded_and_matched() {
        let root = TempRoot::new("policy-matrix");
        write(&root.join("locales/messages"), "{}");
        write(&root.join("other.json"), "{}");

        let empty = resources(Some(&json!({ "catalogs": [] })));
        let empty_selection = enabled_selection(&root, &["other.json"], &empty, &NoIgnore);
        assert!(empty_selection.units.is_empty());
        assert!(empty_selection.errors.is_empty());

        let configured = resources(Some(&json!({
            "catalogs": [{
                "include": ["locales/**"],
                "exclude": ["locales/excluded.json"],
                "format": "json"
            }]
        })));
        let unmatched = enabled_selection(&root, &["other.json"], &configured, &NoIgnore);
        assert_eq!(unmatched.errors[0].code, "unsupported_input_file");

        write(&root.join("locales/excluded.json"), "{}");
        let excluded = enabled_selection(&root, &["locales/excluded.json"], &configured, &NoIgnore);
        assert!(excluded.units.is_empty());
        assert!(excluded.errors.is_empty());

        let matched = enabled_selection(&root, &["locales/messages"], &configured, &NoIgnore);
        let aliases = selected_aliases(&matched);
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].classification.token(), "catalog:json");

        let bulk = enabled_selection(&root, &["locales"], &configured, &NoIgnore);
        let aliases = selected_aliases(&bulk);
        assert!(bulk.errors.is_empty());
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].candidate.label, "locales/messages");
    }

    #[test]
    fn classification_errors_are_not_hidden_by_ordinary_ignore() {
        let root = TempRoot::new("classification-before-ignore");
        write(&root.join("other.json"), "{}");
        let configured = resources(Some(&json!({
            "catalogs": [{ "include": ["locales/**"] }]
        })));

        let selection = enabled_selection(&root, &["other.json"], &configured, &IgnoreAll);

        assert_eq!(selection.errors.len(), 1);
        assert_eq!(selection.errors[0].code, "unsupported_input_file");
        assert!(selection.units.is_empty());
    }

    #[test]
    fn assignment_conflict_aborts_before_ignore_and_uses_stable_evidence() {
        let root = TempRoot::new("assignment-conflict");
        write(&root.join("locales/z.yaml"), "message: Hello");
        write(&root.join("locales/a.yaml"), "message: Hello");
        write(&root.join("other.json"), "{}");
        let configured = resources(Some(&json!({
            "catalogs": [
                { "include": ["locales/**"] },
                { "include": ["locales/**"], "format": "json" }
            ]
        })));

        let selection = enabled_selection(
            &root,
            &["locales/z.yaml", "locales/a.yaml", "other.json"],
            &configured,
            &IgnoreAll,
        );

        assert!(selection.aborted);
        assert!(selection.units.is_empty());
        assert_eq!(selection.errors.len(), 2);
        assert_eq!(selection.errors[0].code, "unsupported_input_file");
        let error = &selection.errors[1];
        assert_eq!(error.code, "config_validation_failed");
        assert_eq!(error.path.as_deref(), Some("intlify.config.json"));
        assert_eq!(
            error.details.as_ref().expect("details")["pointer"],
            "/resources/catalogs/1"
        );
        assert_eq!(
            error.details.as_ref().expect("details")["conflictingPointer"],
            "/resources/catalogs/0"
        );
        assert_eq!(
            error.details.as_ref().expect("details")["targetPath"],
            "locales/a.yaml"
        );
    }

    #[test]
    fn known_unshipped_matched_format_uses_resource_error() {
        let root = TempRoot::new("known-unshipped");
        write(&root.join("locales/en.yaml"), "message: Hello");
        let configured = resources(Some(&json!({
            "catalogs": [{ "include": ["locales/**"] }]
        })));

        let selection = enabled_selection(&root, &["locales/*.yaml"], &configured, &NoIgnore);

        assert_eq!(selection.errors.len(), 1);
        let error = &selection.errors[0];
        assert_eq!(error.code, "resource_format_unsupported");
        let details = error.details.as_ref().expect("details");
        assert_eq!(details["format"], "yaml");
        assert_eq!(details["extension"], ".yaml");
        assert_eq!(details["supportedFormats"], json!(["json"]));
    }

    #[cfg(unix)]
    #[test]
    fn project_membership_uses_internal_symlink_logical_path() {
        use std::os::unix::fs::symlink;

        let root = TempRoot::new("logical-symlink");
        let external = TempRoot::new("logical-symlink-external");
        write(&external.join("catalog.data"), "{}");
        fs::create_dir_all(root.join("locales")).expect("locales should be created");
        symlink(
            external.join("catalog.data"),
            root.join("locales/en.resource"),
        )
        .expect("symlink should be created");
        let configured = resources(Some(&json!({
            "catalogs": [{
                "include": ["locales/**"],
                "format": "json"
            }]
        })));

        let selection = enabled_selection(&root, &["locales/en.resource"], &configured, &NoIgnore);
        let aliases = selected_aliases(&selection);

        assert!(selection.errors.is_empty());
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].candidate.label, "locales/en.resource");
        assert_eq!(aliases[0].classification.token(), "catalog:json");
    }

    #[test]
    fn absent_policy_allows_direct_json_outside_project_root() {
        let root = TempRoot::new("outside-root");
        let external = TempRoot::new("outside-root-file");
        write(&external.join("catalog.JSON"), "{}");
        let resolved = resources(None);
        let registry = HostFormatRegistry::new();
        let external_path = external.join("catalog.JSON");
        let operands = vec![external_path.to_string_lossy().into_owned()];

        let selection = select_file_inputs(
            &root,
            &root,
            &operands,
            &NoIgnore,
            CatalogSelection::Enabled {
                resources: &resolved,
                registry: &registry,
                config_path: None,
            },
        );
        let aliases = selected_aliases(&selection);

        assert!(selection.errors.is_empty());
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].classification.token(), "catalog:json");
        assert_eq!(
            aliases[0].candidate.label,
            super::exact_slash_path(&external_path).expect("external path should be Unicode")
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_and_hardlink_aliases_share_one_physical_group() {
        use std::os::unix::fs::symlink;

        let root = TempRoot::new("physical-group");
        write(&root.join("a.mf2"), "Hello");
        fs::hard_link(root.join("a.mf2"), root.join("b.mf2")).expect("hard link should be created");
        symlink(root.join("a.mf2"), root.join("c.mf2")).expect("symlink should be created");

        let selection = disabled_selection(&root, &["c.mf2", "b.mf2", "a.mf2"]);

        assert!(selection.errors.is_empty());
        assert_eq!(selection.units.len(), 1);
        let ExecutionUnit::Group(group) = &selection.units[0] else {
            panic!("aliases should form a physical group");
        };
        assert_eq!(
            group
                .aliases
                .iter()
                .map(|alias| alias.candidate.label.as_str())
                .collect::<Vec<_>>(),
            ["a.mf2", "b.mf2", "c.mf2"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn conflicting_alias_classifications_reject_only_the_physical_group() {
        use std::os::unix::fs::symlink;

        let root = TempRoot::new("physical-conflict");
        write(&root.join("message.mf2"), "Hello");
        symlink(root.join("message.mf2"), root.join("resource.json"))
            .expect("symlink should be created");
        write(&root.join("unrelated.mf2"), "Hello");
        let resolved = resources(None);

        let selection = enabled_selection(
            &root,
            &["resource.json", "message.mf2", "unrelated.mf2"],
            &resolved,
            &NoIgnore,
        );

        assert!(!selection.aborted);
        assert_eq!(selection.units.len(), 1);
        assert_eq!(
            selected_aliases(&selection)[0].candidate.label,
            "unrelated.mf2"
        );
        assert_eq!(selection.errors.len(), 1);
        let error = &selection.errors[0];
        assert_eq!(error.code, "input_target_conflict");
        let details = error.details.as_ref().expect("details");
        assert_eq!(details["paths"], json!(["message.mf2", "resource.json"]));
        assert_eq!(
            details["classifications"],
            json!(["standalone:mf2", "catalog:json"])
        );
    }

    #[cfg(unix)]
    #[test]
    fn broken_selected_symlink_is_a_target_local_metadata_failure() {
        use std::os::unix::fs::symlink;

        let root = TempRoot::new("broken-symlink");
        symlink(root.join("missing.mf2"), root.join("broken.mf2"))
            .expect("symlink should be created");

        let selection = disabled_selection(&root, &["broken.mf2"]);

        assert!(selection.errors.is_empty());
        assert_eq!(selection.units.len(), 1);
        let ExecutionUnit::TargetError(failure) = &selection.units[0] else {
            panic!("broken symlink should be a target-local error");
        };
        assert_eq!(failure.error.code, "input_read_failed");
        let details = failure.error.details.as_ref().expect("details");
        assert_eq!(details["reason"], "metadata_failed");
        assert_eq!(details["ioKind"], "not_found");
    }

    #[test]
    fn discovery_errors_precede_path_sorted_classification_errors() {
        let root = TempRoot::new("error-order");
        write(&root.join("z.txt"), "notes");
        write(&root.join("a.txt"), "notes");

        let selection = disabled_selection(&root, &["missing", "z.txt", "a.txt"]);

        assert_eq!(selection.errors.len(), 3);
        assert_eq!(selection.errors[0].code, "unmatched_input");
        assert_eq!(selection.errors[1].path.as_deref(), Some("a.txt"));
        assert_eq!(selection.errors[2].path.as_deref(), Some("z.txt"));
    }

    #[test]
    fn cli_glob_with_only_config_free_json_is_zero_target_success() {
        let root = TempRoot::new("glob-zero-target");
        write(&root.join("locales/en.json"), "{}");
        let resolved = resources(None);

        let selection = enabled_selection(&root, &["locales/*.json"], &resolved, &NoIgnore);

        assert!(selection.units.is_empty());
        assert!(selection.errors.is_empty());
    }

    #[test]
    fn shared_execution_values_are_send() {
        fn assert_send<T: Send>() {}

        assert_send::<super::DiscoveredCandidate>();
        assert_send::<super::CandidateOrigin>();
        assert_send::<super::WorkflowClassification>();
        assert_send::<super::SelectedTarget>();
        assert_send::<super::PhysicalFileGroup>();
        assert_send::<super::TargetSelectionError>();
        assert_send::<super::ExecutionUnit>();
        assert_send::<super::FileSelection>();
        assert_send::<super::StdinSelection>();
    }

    // APFS rejects arbitrary non-UTF-8 file names before discovery can observe them.
    #[cfg(target_os = "linux")]
    #[test]
    fn non_unicode_discovered_entry_is_reported_without_lossy_path() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let root = TempRoot::new("non-unicode");
        let invalid = OsString::from_vec(vec![b'b', b'a', b'd', 0xff]);
        fs::write(root.join(invalid), b"Hello").expect("fixture should be written");

        let selection = disabled_selection(&root, &["."]);

        assert_eq!(selection.errors.len(), 1);
        let error = &selection.errors[0];
        assert_eq!(error.code, "input_path_unrepresentable");
        assert!(error.path.is_none());
        let details = error.details.as_ref().expect("details");
        assert_eq!(details["source"], "discovery");
        assert_eq!(details["parentPath"], root.to_string_lossy().as_ref());
    }
}
