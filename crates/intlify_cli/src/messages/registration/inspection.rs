// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Non-mutating no-follow ownership inspection for filesystem output roots.
//!
//! The reader starts from the trusted project-root handle, opens every existing
//! descendant without following the final component, admits only manifest-owned
//! files and their exact real ancestors, compares payload bytes directly, and
//! verifies that the observed snapshot remained stable. It creates, repairs,
//! renames, or removes nothing.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::io::{self, Read};
use std::path::Path;

use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, Metadata, OpenOptions};
use intlify_export::{ExportArtifactPath, ExportArtifactSet};

use super::super::delivery::{BuiltInExporterId, DeliveryOutputRoot};
use super::destination::{
    host_equivalence_key, preflight_manifest_capabilities, DestinationMap, HostEquivalenceKey,
};
use super::difference::{CheckComparison, ManagedOutputObservation, ObservedArtifactFile};
use super::error::{
    CheckFailureReason, EvidencePath, OutputRegistrationError, OutputRegistrationErrorEvidence,
    OwnershipFailureReason, RegistrationFailureEvidence, RegistrationIoEvidence,
    RegistrationOperation, RegistrationSubject, UnsupportedCapabilityEvidence,
};
use super::manifest::{
    CheckedOutputManifest, ManifestCodecErrorKind, OutputManifest, OUTPUT_MANIFEST_BASENAME,
    OUTPUT_MANIFEST_WIRE_LIMIT,
};

/// Inspect one selected output root and return its complete deterministic check.
pub(super) fn inspect_output(
    project_root: &Path,
    output_root: &DeliveryOutputRoot,
    exporter: BuiltInExporterId,
    expected_artifacts: &ExportArtifactSet,
    expected_manifest: &CheckedOutputManifest,
) -> Result<CheckComparison, OutputRegistrationError> {
    let first_root = match open_output_root(project_root, output_root)? {
        OpenedOutputRoot::Missing => {
            return verify_missing_root(project_root, output_root)
                .map(|()| CheckComparison::output_missing());
        }
        OpenedOutputRoot::Present(root) => root,
    };

    if directory_is_empty(&first_root.dir)? {
        verify_empty_root(project_root, output_root, &first_root.stamp)?;
        return Ok(CheckComparison::output_missing());
    }

    let manifest_file = read_manifest_file(&first_root.dir)?;
    let prior_manifest = CheckedOutputManifest::decode(&manifest_file.bytes, exporter).map_err(
        |error| match error.kind() {
            ManifestCodecErrorKind::Invalid => {
                OutputRegistrationError::registration(RegistrationFailureEvidence::manifest(
                    OwnershipFailureReason::ManifestInvalid,
                    None,
                ))
            }
            ManifestCodecErrorKind::Unsupported => {
                OutputRegistrationError::registration(RegistrationFailureEvidence::manifest(
                    OwnershipFailureReason::ManifestUnsupported,
                    None,
                ))
            }
            ManifestCodecErrorKind::Limit => OutputRegistrationError::registration(
                RegistrationFailureEvidence::manifest(OwnershipFailureReason::ManifestLimit, None),
            ),
            ManifestCodecErrorKind::InternalInvariant => {
                OutputRegistrationError::internal_invariant()
            }
        },
    )?;
    let manifest_is_canonical = manifest_file.bytes.as_ref() == prior_manifest.canonical_bytes();

    preflight_manifest_capabilities(exporter, prior_manifest.manifest())?;

    // A decoded portable path is not cleanup authority until the current host
    // can map the complete prior set injectively under the same destination
    // rules used for the expected set.
    let _prior_destinations =
        DestinationMap::try_from_manifest(output_root, prior_manifest.manifest())?;
    let layout = OwnershipLayout::try_new(prior_manifest.manifest())?;
    let first_tree = scan_owned_tree(
        &first_root.dir,
        &layout,
        expected_artifacts,
        &manifest_file.stamp,
        ScanMode::ComparePayloads,
    )?;

    let second_root = match open_output_root(project_root, output_root) {
        Ok(OpenedOutputRoot::Present(root)) => root,
        Ok(OpenedOutputRoot::Missing) => return Err(snapshot_changed_output_root()),
        Err(error) => {
            return Err(preserve_unreadable_or_changed(
                error,
                snapshot_changed_output_root,
            ));
        }
    };
    if second_root.stamp != first_root.stamp {
        return Err(snapshot_changed_output_root());
    }
    let second_tree = scan_owned_tree(
        &second_root.dir,
        &layout,
        expected_artifacts,
        &manifest_file.stamp,
        ScanMode::MetadataOnly,
    )
    .map_err(|error| preserve_unreadable_or_changed(error, snapshot_changed_output_root))?;
    if first_tree.entries != second_tree.entries {
        return Err(snapshot_changed_output_root());
    }

    let final_manifest = read_manifest_file(&second_root.dir)
        .map_err(|error| preserve_unreadable_or_changed(error, snapshot_changed_manifest))?;
    if final_manifest.bytes != manifest_file.bytes || final_manifest.stamp != manifest_file.stamp {
        return Err(snapshot_changed_manifest());
    }
    let final_root_stamp = snapshot_stamp(&second_root.dir.dir_metadata().map_err(|error| {
        snapshot_unreadable(
            RegistrationSubject::OutputRoot,
            None,
            None,
            RegistrationOperation::Inspect,
            &error,
        )
    })?)?;
    if final_root_stamp != first_root.stamp {
        return Err(snapshot_changed_output_root());
    }

    let observation =
        ManagedOutputObservation::new(prior_manifest, manifest_is_canonical, first_tree.files);
    CheckComparison::compare_managed(expected_manifest, &observation)
}

enum OpenedOutputRoot {
    Missing,
    Present(VerifiedOutputRoot),
}

struct VerifiedOutputRoot {
    dir: Dir,
    stamp: SnapshotStamp,
}

fn open_output_root(
    project_root: &Path,
    output_root: &DeliveryOutputRoot,
) -> Result<OpenedOutputRoot, OutputRegistrationError> {
    let mut current =
        Dir::open_ambient_dir(project_root, cap_std::ambient_authority()).map_err(|error| {
            snapshot_unreadable(
                RegistrationSubject::OutputRoot,
                None,
                None,
                RegistrationOperation::Open,
                &error,
            )
        })?;
    let mut relative = Vec::<Box<str>>::new();
    for segment in output_root.segments() {
        relative.push(segment.into());
        let evidence = EvidencePath::try_new(relative.iter().cloned());
        let metadata = match current.symlink_metadata(segment) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(OpenedOutputRoot::Missing);
            }
            Err(error) => {
                return Err(snapshot_unreadable(
                    RegistrationSubject::OutputRoot,
                    None,
                    evidence,
                    RegistrationOperation::Inspect,
                    &error,
                ));
            }
        };
        if metadata.is_symlink() {
            return Err(OutputRegistrationError::registration(
                RegistrationFailureEvidence::unsafe_entry(
                    RegistrationSubject::OutputRoot,
                    None,
                    evidence,
                ),
            ));
        }
        if !metadata.is_dir() {
            return Err(OutputRegistrationError::registration(
                RegistrationFailureEvidence::root_not_directory(evidence),
            ));
        }
        current = current.open_dir_nofollow(segment).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                snapshot_changed_output_root()
            } else {
                snapshot_unreadable(
                    RegistrationSubject::OutputRoot,
                    None,
                    EvidencePath::try_new(relative.iter().cloned()),
                    RegistrationOperation::Open,
                    &error,
                )
            }
        })?;
        if !current
            .dir_metadata()
            .map_err(|error| {
                snapshot_unreadable(
                    RegistrationSubject::OutputRoot,
                    None,
                    EvidencePath::try_new(relative.iter().cloned()),
                    RegistrationOperation::Inspect,
                    &error,
                )
            })?
            .is_dir()
        {
            return Err(snapshot_changed_output_root());
        }
    }
    let stamp = snapshot_stamp(&current.dir_metadata().map_err(|error| {
        snapshot_unreadable(
            RegistrationSubject::OutputRoot,
            None,
            EvidencePath::try_new(output_root.segments()),
            RegistrationOperation::Inspect,
            &error,
        )
    })?)?;
    Ok(OpenedOutputRoot::Present(VerifiedOutputRoot {
        dir: current,
        stamp,
    }))
}

fn verify_missing_root(
    project_root: &Path,
    output_root: &DeliveryOutputRoot,
) -> Result<(), OutputRegistrationError> {
    match open_output_root(project_root, output_root) {
        Ok(OpenedOutputRoot::Missing) => Ok(()),
        Ok(OpenedOutputRoot::Present(_)) => Err(snapshot_changed_output_root()),
        Err(error) => Err(preserve_unreadable_or_changed(
            error,
            snapshot_changed_output_root,
        )),
    }
}

fn verify_empty_root(
    project_root: &Path,
    output_root: &DeliveryOutputRoot,
    first_stamp: &SnapshotStamp,
) -> Result<(), OutputRegistrationError> {
    let second = match open_output_root(project_root, output_root) {
        Ok(OpenedOutputRoot::Present(root)) => root,
        Ok(OpenedOutputRoot::Missing) => return Err(snapshot_changed_output_root()),
        Err(error) => {
            return Err(preserve_unreadable_or_changed(
                error,
                snapshot_changed_output_root,
            ));
        }
    };
    if &second.stamp != first_stamp || !directory_is_empty(&second.dir)? {
        return Err(snapshot_changed_output_root());
    }
    Ok(())
}

fn directory_is_empty(dir: &Dir) -> Result<bool, OutputRegistrationError> {
    let mut entries = dir.entries().map_err(|error| {
        snapshot_unreadable(
            RegistrationSubject::OutputRoot,
            None,
            None,
            RegistrationOperation::Inspect,
            &error,
        )
    })?;
    match entries.next() {
        None => Ok(true),
        Some(Ok(_)) => Ok(false),
        Some(Err(error)) => Err(snapshot_unreadable(
            RegistrationSubject::OutputRoot,
            None,
            None,
            RegistrationOperation::Inspect,
            &error,
        )),
    }
}

struct ManifestFileSnapshot {
    bytes: Box<[u8]>,
    stamp: SnapshotStamp,
}

fn read_manifest_file(dir: &Dir) -> Result<ManifestFileSnapshot, OutputRegistrationError> {
    let metadata = match dir.symlink_metadata(OUTPUT_MANIFEST_BASENAME) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(OutputRegistrationError::registration(
                RegistrationFailureEvidence::manifest(
                    OwnershipFailureReason::ManifestMissing,
                    None,
                ),
            ));
        }
        Err(error) => {
            return Err(snapshot_unreadable(
                RegistrationSubject::Manifest,
                None,
                None,
                RegistrationOperation::Inspect,
                &error,
            ));
        }
    };
    if metadata.is_symlink() || !metadata.is_file() {
        return Err(OutputRegistrationError::registration(
            RegistrationFailureEvidence::manifest(OwnershipFailureReason::ManifestInvalid, None),
        ));
    }

    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let mut file = dir
        .open_with(OUTPUT_MANIFEST_BASENAME, &options)
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                snapshot_changed_manifest()
            } else {
                snapshot_unreadable(
                    RegistrationSubject::Manifest,
                    None,
                    None,
                    RegistrationOperation::Open,
                    &error,
                )
            }
        })?;
    let before = file.metadata().map_err(|error| {
        snapshot_unreadable(
            RegistrationSubject::Manifest,
            None,
            None,
            RegistrationOperation::Inspect,
            &error,
        )
    })?;
    if !before.is_file() {
        return Err(snapshot_changed_manifest());
    }
    if before.len() > OUTPUT_MANIFEST_WIRE_LIMIT as u64 {
        return Err(OutputRegistrationError::registration(
            RegistrationFailureEvidence::manifest(OwnershipFailureReason::ManifestLimit, None),
        ));
    }
    let before = snapshot_stamp(&before)?;
    let mut bytes = Vec::with_capacity(
        usize::try_from(before.len)
            .unwrap_or(OUTPUT_MANIFEST_WIRE_LIMIT)
            .min(OUTPUT_MANIFEST_WIRE_LIMIT),
    );
    (&mut file)
        .take((OUTPUT_MANIFEST_WIRE_LIMIT as u64) + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            snapshot_unreadable(
                RegistrationSubject::Manifest,
                None,
                None,
                RegistrationOperation::Read,
                &error,
            )
        })?;
    if bytes.len() > OUTPUT_MANIFEST_WIRE_LIMIT {
        return Err(OutputRegistrationError::registration(
            RegistrationFailureEvidence::manifest(OwnershipFailureReason::ManifestLimit, None),
        ));
    }
    let after = snapshot_stamp(&file.metadata().map_err(|error| {
        snapshot_unreadable(
            RegistrationSubject::Manifest,
            None,
            None,
            RegistrationOperation::Inspect,
            &error,
        )
    })?)?;
    if before != after {
        return Err(snapshot_changed_manifest());
    }
    Ok(ManifestFileSnapshot {
        bytes: bytes.into_boxed_slice(),
        stamp: after,
    })
}

#[derive(Debug)]
struct OwnershipLayout {
    entries: BTreeMap<HostEquivalenceKey, OwnedEntry>,
}

impl OwnershipLayout {
    fn try_new(manifest: &OutputManifest) -> Result<Self, OutputRegistrationError> {
        let mut entries = BTreeMap::new();
        let manifest_path = ExportArtifactPath::try_new([OUTPUT_MANIFEST_BASENAME])
            .map_err(|_| OutputRegistrationError::internal_invariant())?;
        entries.insert(
            host_equivalence_key([OUTPUT_MANIFEST_BASENAME]),
            OwnedEntry::Manifest(manifest_path),
        );
        let artifact_paths = manifest
            .artifacts()
            .iter()
            .map(|artifact| artifact.path().clone())
            .collect::<BTreeSet<_>>();
        let mut directory_paths = BTreeSet::new();
        for path in &artifact_paths {
            for length in 1..path.segments().len() {
                let prefix = ExportArtifactPath::try_new(
                    path.segments()[..length]
                        .iter()
                        .map(intlify_export::ExportArtifactPathSegment::as_str),
                )
                .map_err(|_| OutputRegistrationError::internal_invariant())?;
                if artifact_paths.contains(&prefix) {
                    return Err(OutputRegistrationError::registration(
                        RegistrationFailureEvidence::manifest(
                            OwnershipFailureReason::ManifestInvalid,
                            None,
                        ),
                    ));
                }
                directory_paths.insert(prefix);
            }
        }
        for path in directory_paths {
            let key = path_key(&path);
            if let Some(existing) = entries.get_mut(&key) {
                // Distinct portable prefixes may identify one physical
                // ancestor on a case- or normalization-insensitive host while
                // their complete artifact destinations remain injective.
                if let OwnedEntry::Directory { aliases, .. } = existing {
                    aliases.push(path);
                    continue;
                }
                return Err(OutputRegistrationError::registration(
                    RegistrationFailureEvidence::manifest(
                        OwnershipFailureReason::ManifestInvalid,
                        None,
                    ),
                ));
            }
            entries.insert(
                key,
                OwnedEntry::Directory {
                    canonical: path.clone(),
                    aliases: vec![path],
                },
            );
        }
        for path in artifact_paths {
            let key = path_key(&path);
            if entries.insert(key, OwnedEntry::Artifact(path)).is_some() {
                return Err(OutputRegistrationError::registration(
                    RegistrationFailureEvidence::manifest(
                        OwnershipFailureReason::ManifestInvalid,
                        None,
                    ),
                ));
            }
        }
        Ok(Self { entries })
    }

    fn classify(&self, actual_segments: &[Box<str>]) -> Option<&OwnedEntry> {
        self.entries.get(&host_equivalence_key(
            actual_segments.iter().map(Box::as_ref),
        ))
    }
}

fn path_key(path: &ExportArtifactPath) -> HostEquivalenceKey {
    host_equivalence_key(
        path.segments()
            .iter()
            .map(intlify_export::ExportArtifactPathSegment::as_str),
    )
}

#[derive(Debug)]
enum OwnedEntry {
    Manifest(ExportArtifactPath),
    Directory {
        canonical: ExportArtifactPath,
        aliases: Vec<ExportArtifactPath>,
    },
    Artifact(ExportArtifactPath),
}

impl OwnedEntry {
    fn path(&self) -> &ExportArtifactPath {
        match self {
            Self::Manifest(path) | Self::Artifact(path) => path,
            Self::Directory { canonical, .. } => canonical,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ScanMode {
    ComparePayloads,
    MetadataOnly,
}

struct DirectoryCandidate {
    native_path: NativePathKey,
    actual: Option<(Vec<Box<str>>, ExportArtifactPath)>,
}

#[derive(Debug, PartialEq, Eq)]
struct TreeSnapshot {
    entries: BTreeMap<ExportArtifactPath, SnapshotEntry>,
    files: BTreeMap<ExportArtifactPath, ObservedArtifactFile>,
    actual_paths: BTreeMap<ExportArtifactPath, ExportArtifactPath>,
}

fn scan_owned_tree(
    root: &Dir,
    layout: &OwnershipLayout,
    expected_artifacts: &ExportArtifactSet,
    manifest_stamp: &SnapshotStamp,
    mode: ScanMode,
) -> Result<TreeSnapshot, OutputRegistrationError> {
    let mut snapshot = TreeSnapshot {
        entries: BTreeMap::new(),
        files: BTreeMap::new(),
        actual_paths: BTreeMap::new(),
    };
    let mut failures = ScanFailures::default();
    scan_directory(
        root,
        &[],
        &NativePathKey::root(),
        layout,
        expected_artifacts,
        manifest_stamp,
        mode,
        &mut snapshot,
        &mut failures,
    )?;
    failures.finish()?;
    for entry in layout.entries.values() {
        if let OwnedEntry::Artifact(path) = entry {
            snapshot
                .files
                .entry(path.clone())
                .or_insert_with(ObservedArtifactFile::missing);
        }
    }
    Ok(snapshot)
}

#[allow(clippy::too_many_arguments)]
fn scan_directory(
    dir: &Dir,
    actual_prefix: &[Box<str>],
    native_prefix: &NativePathKey,
    layout: &OwnershipLayout,
    expected_artifacts: &ExportArtifactSet,
    manifest_stamp: &SnapshotStamp,
    mode: ScanMode,
    snapshot: &mut TreeSnapshot,
    failures: &mut ScanFailures,
) -> Result<(), OutputRegistrationError> {
    let entries = dir.entries().map_err(|error| {
        snapshot_unreadable(
            RegistrationSubject::OutputRoot,
            None,
            EvidencePath::try_new(actual_prefix.iter().cloned()),
            RegistrationOperation::Inspect,
            &error,
        )
    })?;
    let mut candidates = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            snapshot_unreadable(
                RegistrationSubject::OutputRoot,
                None,
                EvidencePath::try_new(actual_prefix.iter().cloned()),
                RegistrationOperation::Inspect,
                &error,
            )
        })?;
        let name = entry.file_name();
        let native_path = native_prefix.child(&name);
        let actual = name.to_str().and_then(|name| {
            let mut segments = actual_prefix.to_vec();
            segments.push(name.into());
            ExportArtifactPath::try_new(segments.iter().cloned())
                .ok()
                .map(|path| (segments, path))
        });
        candidates.push(DirectoryCandidate {
            native_path,
            actual,
        });
    }
    candidates.sort_unstable_by(|left, right| match (&left.actual, &right.actual) {
        (Some((_, left)), Some((_, right))) => left.cmp(right),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => left.native_path.cmp(&right.native_path),
    });

    for candidate in candidates {
        let Some((actual_segments, actual_path)) = candidate.actual else {
            failures.record_unrepresentable(candidate.native_path);
            continue;
        };
        let name = actual_segments
            .last()
            .map(Box::as_ref)
            .ok_or_else(OutputRegistrationError::internal_invariant)?;
        let native_path = candidate.native_path;
        let actual_evidence = EvidencePath::try_new(actual_segments.iter().cloned());
        let Some(owned) = layout.classify(&actual_segments) else {
            failures.record_representable(
                actual_path,
                OutputRegistrationError::registration(RegistrationFailureEvidence::unowned_entry(
                    actual_evidence.ok_or_else(OutputRegistrationError::internal_invariant)?,
                )),
            );
            continue;
        };
        if !admit_one_host_equivalent_entry(&actual_path, owned, snapshot, failures)? {
            continue;
        }

        let metadata = match dir.symlink_metadata(name) {
            Ok(metadata) => metadata,
            Err(error) => {
                failures.record_representable(
                    actual_path.clone(),
                    entry_io_failure(
                        owned,
                        actual_evidence,
                        RegistrationOperation::Inspect,
                        &error,
                    ),
                );
                continue;
            }
        };
        match owned {
            OwnedEntry::Manifest(logical_path) => {
                if metadata.is_symlink() || !metadata.is_file() {
                    failures.record_representable(
                        actual_path.clone(),
                        OutputRegistrationError::registration(
                            RegistrationFailureEvidence::manifest(
                                OwnershipFailureReason::ManifestInvalid,
                                None,
                            ),
                        ),
                    );
                    continue;
                }
                match open_regular_file(dir, name, None) {
                    Ok((stamp, _)) => {
                        if &stamp != manifest_stamp {
                            failures.record_representable(
                                actual_path.clone(),
                                snapshot_changed_manifest(),
                            );
                        }
                        snapshot.entries.insert(
                            logical_path.clone(),
                            SnapshotEntry::new(SnapshotEntryKind::Manifest, stamp),
                        );
                    }
                    Err(error) => failures.record_representable(
                        actual_path.clone(),
                        map_entry_read_error(owned, actual_evidence, error),
                    ),
                }
            }
            OwnedEntry::Directory { .. } => {
                if metadata.is_symlink() || !metadata.is_dir() {
                    failures.record_representable(
                        actual_path.clone(),
                        OutputRegistrationError::registration(
                            RegistrationFailureEvidence::unsafe_entry(
                                RegistrationSubject::OutputRoot,
                                None,
                                actual_evidence,
                            ),
                        ),
                    );
                    continue;
                }
                match open_real_directory(dir, name) {
                    Ok((child, stamp)) => {
                        snapshot.entries.insert(
                            actual_path.clone(),
                            SnapshotEntry::new(SnapshotEntryKind::Directory, stamp),
                        );
                        scan_directory(
                            &child,
                            &actual_segments,
                            &native_path,
                            layout,
                            expected_artifacts,
                            manifest_stamp,
                            mode,
                            snapshot,
                            failures,
                        )?;
                    }
                    Err(error) => failures.record_representable(
                        actual_path.clone(),
                        map_entry_read_error(owned, actual_evidence, error),
                    ),
                }
            }
            OwnedEntry::Artifact(logical_path) => {
                if metadata.is_symlink() || !metadata.is_file() {
                    failures.record_representable(
                        actual_path.clone(),
                        OutputRegistrationError::registration(
                            RegistrationFailureEvidence::unsafe_entry(
                                RegistrationSubject::Artifact,
                                Some(logical_path.clone()),
                                actual_evidence,
                            ),
                        ),
                    );
                    continue;
                }
                let expected_payload = if matches!(mode, ScanMode::ComparePayloads) {
                    expected_payload(expected_artifacts, logical_path)
                } else {
                    None
                };
                match open_regular_file(dir, name, expected_payload) {
                    Ok((stamp, payload_equal)) => {
                        snapshot.entries.insert(
                            logical_path.clone(),
                            SnapshotEntry::new(SnapshotEntryKind::Artifact, stamp),
                        );
                        snapshot.files.insert(
                            logical_path.clone(),
                            ObservedArtifactFile::present(payload_equal),
                        );
                    }
                    Err(error) => failures.record_representable(
                        actual_path.clone(),
                        map_entry_read_error(owned, actual_evidence, error),
                    ),
                }
            }
        }
    }
    Ok(())
}

/// Admit at most one concrete spelling for each host-equivalent owned path.
///
/// Case-sensitive volumes can expose several names that a conservative host
/// namespace must treat as one destination. The exact logical spelling wins;
/// otherwise the smallest portable spelling wins. Every other physical entry
/// is caller-owned instead of being silently overwritten in the snapshot map.
fn admit_one_host_equivalent_entry(
    actual_path: &ExportArtifactPath,
    owned: &OwnedEntry,
    snapshot: &mut TreeSnapshot,
    failures: &mut ScanFailures,
) -> Result<bool, OutputRegistrationError> {
    let logical_path = owned.path();
    if let OwnedEntry::Directory { aliases, .. } = owned {
        if aliases.iter().any(|alias| alias == actual_path) {
            return Ok(true);
        }
        if aliases.len() > 1 {
            let evidence = EvidencePath::try_new(
                actual_path
                    .segments()
                    .iter()
                    .map(intlify_export::ExportArtifactPathSegment::as_str),
            )
            .ok_or_else(OutputRegistrationError::internal_invariant)?;
            failures.record_representable(
                actual_path.clone(),
                OutputRegistrationError::registration(RegistrationFailureEvidence::unowned_entry(
                    evidence,
                )),
            );
            return Ok(false);
        }
    }
    let Some(prior_actual) = snapshot.actual_paths.get(logical_path).cloned() else {
        snapshot
            .actual_paths
            .insert(logical_path.clone(), actual_path.clone());
        return Ok(true);
    };

    let current_wins = if actual_path == logical_path {
        prior_actual != *logical_path
    } else if prior_actual == *logical_path {
        false
    } else {
        actual_path < &prior_actual
    };
    let unowned = if current_wins {
        snapshot
            .actual_paths
            .insert(logical_path.clone(), actual_path.clone());
        prior_actual
    } else {
        actual_path.clone()
    };
    let evidence = EvidencePath::try_new(
        unowned
            .segments()
            .iter()
            .map(intlify_export::ExportArtifactPathSegment::as_str),
    )
    .ok_or_else(OutputRegistrationError::internal_invariant)?;
    failures.record_representable(
        unowned,
        OutputRegistrationError::registration(RegistrationFailureEvidence::unowned_entry(evidence)),
    );
    Ok(current_wins)
}

fn expected_payload<'a>(
    artifacts: &'a ExportArtifactSet,
    path: &ExportArtifactPath,
) -> Option<&'a [u8]> {
    artifacts
        .artifacts()
        .binary_search_by(|artifact| artifact.logical_path().cmp(path))
        .ok()
        .map(|index| artifacts.artifacts()[index].payload().as_bytes())
}

fn open_real_directory(parent: &Dir, name: &str) -> Result<(Dir, SnapshotStamp), EntryReadError> {
    let dir = parent
        .open_dir_nofollow(name)
        .map_err(|error| EntryReadError::io(RegistrationOperation::Open, error))?;
    let metadata = dir
        .dir_metadata()
        .map_err(|error| EntryReadError::io(RegistrationOperation::Inspect, error))?;
    if !metadata.is_dir() {
        return Err(EntryReadError::Changed);
    }
    let stamp = snapshot_stamp(&metadata).map_err(EntryReadError::Registration)?;
    Ok((dir, stamp))
}

fn open_regular_file(
    parent: &Dir,
    name: &str,
    expected_payload: Option<&[u8]>,
) -> Result<(SnapshotStamp, Option<bool>), EntryReadError> {
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let mut file = parent
        .open_with(name, &options)
        .map_err(|error| EntryReadError::io(RegistrationOperation::Open, error))?;
    let before_metadata = file
        .metadata()
        .map_err(|error| EntryReadError::io(RegistrationOperation::Inspect, error))?;
    if !before_metadata.is_file() {
        return Err(EntryReadError::Changed);
    }
    let before = snapshot_stamp(&before_metadata).map_err(EntryReadError::Registration)?;
    let payload_equal = expected_payload
        .map(|expected| compare_payload_bytes(&mut file, before.len, expected))
        .transpose()
        .map_err(|error| EntryReadError::io(RegistrationOperation::Read, error))?;
    let after_metadata = file
        .metadata()
        .map_err(|error| EntryReadError::io(RegistrationOperation::Inspect, error))?;
    let after = snapshot_stamp(&after_metadata).map_err(EntryReadError::Registration)?;
    if before != after {
        return Err(EntryReadError::Changed);
    }
    Ok((after, payload_equal))
}

fn compare_payload_bytes(
    file: &mut cap_std::fs::File,
    actual_len: u64,
    expected: &[u8],
) -> io::Result<bool> {
    if actual_len != u64::try_from(expected.len()).unwrap_or(u64::MAX) {
        return Ok(false);
    }
    let mut offset = 0_usize;
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(offset == expected.len());
        }
        let Some(end) = offset.checked_add(read) else {
            return Ok(false);
        };
        if end > expected.len() || buffer[..read] != expected[offset..end] {
            return Ok(false);
        }
        offset = end;
    }
}

enum EntryReadError {
    Changed,
    Io {
        operation: RegistrationOperation,
        error: io::Error,
    },
    Registration(OutputRegistrationError),
}

impl EntryReadError {
    fn io(operation: RegistrationOperation, error: io::Error) -> Self {
        Self::Io { operation, error }
    }
}

fn map_entry_read_error(
    owned: &OwnedEntry,
    relative_path: Option<EvidencePath>,
    error: EntryReadError,
) -> OutputRegistrationError {
    match error {
        EntryReadError::Changed => entry_snapshot_changed(owned, relative_path),
        EntryReadError::Io { error, .. } if error.kind() == io::ErrorKind::NotFound => {
            entry_snapshot_changed(owned, relative_path)
        }
        EntryReadError::Io { operation, error } => {
            entry_io_failure(owned, relative_path, operation, &error)
        }
        EntryReadError::Registration(error) => error,
    }
}

fn entry_snapshot_changed(
    owned: &OwnedEntry,
    relative_path: Option<EvidencePath>,
) -> OutputRegistrationError {
    match owned {
        OwnedEntry::Manifest(_) => snapshot_changed_manifest(),
        OwnedEntry::Directory { .. } => snapshot_changed_output_root(),
        OwnedEntry::Artifact(path) => {
            OutputRegistrationError::registration(RegistrationFailureEvidence::snapshot_changed(
                RegistrationSubject::Artifact,
                Some(path.clone()),
                relative_path,
            ))
        }
    }
}

fn entry_io_failure(
    owned: &OwnedEntry,
    relative_path: Option<EvidencePath>,
    operation: RegistrationOperation,
    error: &io::Error,
) -> OutputRegistrationError {
    let (subject, artifact_path) = match owned {
        OwnedEntry::Manifest(_) => (RegistrationSubject::Manifest, None),
        OwnedEntry::Directory { .. } => (RegistrationSubject::OutputRoot, None),
        OwnedEntry::Artifact(path) => (RegistrationSubject::Artifact, Some(path.clone())),
    };
    snapshot_unreadable(subject, artifact_path, relative_path, operation, error)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnapshotEntry {
    kind: SnapshotEntryKind,
    stamp: SnapshotStamp,
}

impl SnapshotEntry {
    const fn new(kind: SnapshotEntryKind, stamp: SnapshotStamp) -> Self {
        Self { kind, stamp }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapshotEntryKind {
    Manifest,
    Directory,
    Artifact,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SnapshotStamp {
    identity_a: u64,
    identity_b: u64,
    len: u64,
    change_a: i64,
    change_b: i64,
}

#[cfg(unix)]
// Other supported platforms can fail when a stable handle identity is absent;
// Unix metadata always supplies the required device/inode pair.
#[allow(clippy::unnecessary_wraps)]
fn snapshot_stamp(metadata: &Metadata) -> Result<SnapshotStamp, OutputRegistrationError> {
    use cap_std::fs::MetadataExt;

    Ok(SnapshotStamp {
        identity_a: metadata.dev(),
        identity_b: metadata.ino(),
        len: metadata.len(),
        change_a: metadata.ctime(),
        change_b: metadata.ctime_nsec(),
    })
}

#[cfg(windows)]
fn snapshot_stamp(metadata: &Metadata) -> Result<SnapshotStamp, OutputRegistrationError> {
    use cap_std::fs::MetadataExt;

    let Some(volume) = metadata.volume_serial_number() else {
        return Err(filesystem_no_follow_unsupported());
    };
    let Some(index) = metadata.file_index() else {
        return Err(filesystem_no_follow_unsupported());
    };
    Ok(SnapshotStamp {
        identity_a: u64::from(volume),
        identity_b: index,
        len: metadata.len(),
        change_a: i64::try_from(metadata.last_write_time()).unwrap_or(i64::MAX),
        change_b: i64::try_from(metadata.creation_time()).unwrap_or(i64::MAX),
    })
}

#[cfg(not(any(unix, windows)))]
fn snapshot_stamp(_metadata: &Metadata) -> Result<SnapshotStamp, OutputRegistrationError> {
    Err(filesystem_no_follow_unsupported())
}

fn filesystem_no_follow_unsupported() -> OutputRegistrationError {
    OutputRegistrationError::unsupported_capability(
        UnsupportedCapabilityEvidence::FilesystemNoFollow,
    )
}

#[derive(Default)]
struct ScanFailures {
    representable: Option<(ExportArtifactPath, OutputRegistrationError)>,
    unrepresentable: Option<NativePathKey>,
}

impl ScanFailures {
    fn record_representable(&mut self, path: ExportArtifactPath, error: OutputRegistrationError) {
        if self
            .representable
            .as_ref()
            .is_none_or(|(prior, _)| path < *prior)
        {
            self.representable = Some((path, error));
        }
    }

    fn record_unrepresentable(&mut self, path: NativePathKey) {
        if self
            .unrepresentable
            .as_ref()
            .is_none_or(|prior| path < *prior)
        {
            self.unrepresentable = Some(path);
        }
    }

    fn finish(self) -> Result<(), OutputRegistrationError> {
        if let Some((_, error)) = self.representable {
            return Err(error);
        }
        if self.unrepresentable.is_some() {
            return Err(OutputRegistrationError::registration(
                RegistrationFailureEvidence::entry_unrepresentable(),
            ));
        }
        Ok(())
    }
}

/// Native unsigned component order used only when UTF-8 identity is impossible.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct NativePathKey(Box<[Box<[u32]>]>);

impl NativePathKey {
    fn root() -> Self {
        Self(Box::new([]))
    }

    fn child(&self, name: &OsStr) -> Self {
        let mut components = self.0.to_vec();
        components.push(native_component(name));
        Self(components.into_boxed_slice())
    }
}

#[cfg(unix)]
fn native_component(name: &OsStr) -> Box<[u32]> {
    use std::os::unix::ffi::OsStrExt;

    name.as_bytes()
        .iter()
        .map(|byte| u32::from(*byte))
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

#[cfg(windows)]
fn native_component(name: &OsStr) -> Box<[u32]> {
    use std::os::windows::ffi::OsStrExt;

    name.encode_wide()
        .map(u32::from)
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

#[cfg(not(any(unix, windows)))]
fn native_component(name: &OsStr) -> Box<[u32]> {
    name.to_string_lossy()
        .bytes()
        .map(u32::from)
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

fn snapshot_unreadable(
    subject: RegistrationSubject,
    artifact_path: Option<ExportArtifactPath>,
    relative_path: Option<EvidencePath>,
    operation: RegistrationOperation,
    error: &io::Error,
) -> OutputRegistrationError {
    OutputRegistrationError::registration(RegistrationFailureEvidence::snapshot_unreadable(
        subject,
        artifact_path,
        relative_path,
        RegistrationIoEvidence::from_error(operation, error),
    ))
}

fn snapshot_changed_output_root() -> OutputRegistrationError {
    OutputRegistrationError::registration(RegistrationFailureEvidence::snapshot_changed(
        RegistrationSubject::OutputRoot,
        None,
        None,
    ))
}

fn snapshot_changed_manifest() -> OutputRegistrationError {
    OutputRegistrationError::registration(RegistrationFailureEvidence::snapshot_changed(
        RegistrationSubject::Manifest,
        None,
        None,
    ))
}

/// Preserve a concrete unreadable operation during a verification pass while
/// classifying every newly observed ownership shape as a concurrent change.
fn preserve_unreadable_or_changed(
    error: OutputRegistrationError,
    changed: fn() -> OutputRegistrationError,
) -> OutputRegistrationError {
    let preserve = matches!(
        error.evidence(),
        OutputRegistrationErrorEvidence::Registration(RegistrationFailureEvidence::Check {
            reason: CheckFailureReason::SnapshotUnreadable,
            ..
        }) | OutputRegistrationErrorEvidence::UnsupportedCapability(_)
            | OutputRegistrationErrorEvidence::InternalInvariant
    );
    if preserve {
        error
    } else {
        changed()
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use intlify_export::ExportArtifactPath;

    use super::{
        admit_one_host_equivalent_entry, map_entry_read_error, EntryReadError, NativePathKey,
        OsStr, OwnedEntry, ScanFailures, TreeSnapshot,
    };
    use crate::messages::registration::error::{
        OutputRegistrationErrorEvidence, RegistrationFailureEvidence, RegistrationOperation,
    };

    #[test]
    fn reverse_discovery_keeps_the_smallest_native_unrepresentable_path() {
        let root = NativePathKey::root();
        let first = root.child(OsStr::new("a"));
        let second = root.child(OsStr::new("z"));
        let mut failures = ScanFailures::default();
        failures.record_unrepresentable(second);
        failures.record_unrepresentable(first.clone());
        assert_eq!(failures.unrepresentable, Some(first));
    }

    #[test]
    fn exact_logical_spelling_wins_over_an_extra_host_equivalent_entry() {
        let logical = ExportArtifactPath::try_new(["a.mjs"]).unwrap();
        let alternate = ExportArtifactPath::try_new(["A.mjs"]).unwrap();
        let owned = OwnedEntry::Artifact(logical.clone());
        let mut snapshot = TreeSnapshot {
            entries: std::collections::BTreeMap::new(),
            files: std::collections::BTreeMap::new(),
            actual_paths: std::collections::BTreeMap::new(),
        };
        let mut failures = ScanFailures::default();

        assert!(
            admit_one_host_equivalent_entry(&alternate, &owned, &mut snapshot, &mut failures)
                .unwrap()
        );
        assert!(
            admit_one_host_equivalent_entry(&logical, &owned, &mut snapshot, &mut failures)
                .unwrap()
        );
        assert_eq!(snapshot.actual_paths.get(&logical), Some(&logical));
        assert_eq!(
            failures.representable.as_ref().map(|(path, _)| path),
            Some(&alternate)
        );
    }

    #[test]
    fn entry_io_failure_preserves_the_exact_read_operation() {
        let path = ExportArtifactPath::try_new(["a.mjs"]).unwrap();
        let owned = OwnedEntry::Artifact(path.clone());
        let error = map_entry_read_error(
            &owned,
            None,
            EntryReadError::io(
                RegistrationOperation::Read,
                std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            ),
        );
        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::Registration(RegistrationFailureEvidence::Check {
                artifact_path: Some(actual_path),
                io: Some(io),
                ..
            }) if actual_path == &path && io.operation() == RegistrationOperation::Read
        ));
    }

    #[cfg(unix)]
    #[test]
    fn posix_unrepresentable_names_use_raw_unsigned_byte_order() {
        use std::os::unix::ffi::OsStringExt;

        let root = NativePathKey::root();
        let first = OsString::from_vec(vec![0x80]);
        let second = OsString::from_vec(vec![0xff]);
        assert!(root.child(OsStr::new(&first)) < root.child(OsStr::new(&second)));
    }

    #[cfg(windows)]
    #[test]
    fn windows_unrepresentable_names_use_unsigned_utf16_order() {
        use std::os::windows::ffi::OsStringExt;

        let root = NativePathKey::root();
        let first = OsString::from_wide(&[0xd800]);
        let second = OsString::from_wide(&[0xdfff]);
        assert!(root.child(OsStr::new(&first)) < root.child(OsStr::new(&second)));
    }
}
