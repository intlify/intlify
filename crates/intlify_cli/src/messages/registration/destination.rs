// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Exporter capability preflight and injective host-destination mapping.
//!
//! This module visits checked artifacts in canonical logical-path order,
//! validates the initial ESM representation capabilities, preserves exact path
//! spelling during host conversion, and rejects control-name or host-equivalent
//! collisions. It never truncates, escapes, suffixes, or writes a path.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound::{Excluded, Included, Unbounded};
use std::path::{Path, PathBuf};

use intlify_export::{
    ExportArtifact, ExportArtifactFormatVersion, ExportArtifactKind, ExportArtifactPath,
    ExportArtifactRelationship, ExportArtifactRelationshipKind, ExportArtifactSet, ExportMediaType,
};
use unicode_normalization::UnicodeNormalization;

use super::super::delivery::{BuiltInExporterId, DeliveryOutputRoot};
use super::error::{
    DestinationCollisionEvidence, DestinationMappingEvidence, DestinationMappingReason,
    EvidencePath, OutputRegistrationError, UnsupportedCapabilityEvidence,
};
use super::manifest::{
    ManifestArtifact, OutputManifest, ESM_ACCESSOR_KIND, ESM_LOADER_KIND, ESM_MODULE_KIND,
    OUTPUT_MANIFEST_BASENAME,
};

const JAVASCRIPT_MEDIA_TYPE: &str = "text/javascript";
const TYPESCRIPT_MEDIA_TYPE: &str = "text/typescript";

/// Complete canonical artifact-to-host mapping for one selected target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DestinationMap {
    artifacts: Box<[MappedArtifactDestination]>,
}

impl DestinationMap {
    /// Map all artifacts, then reject the reserved manifest and collision pairs.
    pub(super) fn try_new(
        output_root: &DeliveryOutputRoot,
        artifacts: &ExportArtifactSet,
    ) -> Result<Self, OutputRegistrationError> {
        Self::try_from_paths(
            output_root,
            artifacts
                .artifacts()
                .iter()
                .map(intlify_export::ExportArtifact::logical_path),
        )
    }

    /// Reapply host mapping to every path decoded from an ownership manifest.
    pub(super) fn try_from_manifest(
        output_root: &DeliveryOutputRoot,
        manifest: &OutputManifest,
    ) -> Result<Self, OutputRegistrationError> {
        Self::try_from_paths(
            output_root,
            manifest
                .artifacts()
                .iter()
                .map(super::manifest::ManifestArtifact::path),
        )
    }

    fn try_from_paths<'a>(
        output_root: &DeliveryOutputRoot,
        paths: impl Iterator<Item = &'a ExportArtifactPath>,
    ) -> Result<Self, OutputRegistrationError> {
        Self::try_from_paths_with_namespace(HostNamespace::current(), output_root, paths)
    }

    fn try_from_paths_with_namespace<'a>(
        namespace: HostNamespace,
        output_root: &DeliveryOutputRoot,
        paths: impl Iterator<Item = &'a ExportArtifactPath>,
    ) -> Result<Self, OutputRegistrationError> {
        let mut mapped = Vec::new();
        let mut relative_paths = BTreeSet::new();
        let mut equivalence_keys = BTreeSet::new();
        for path in paths {
            let candidate = map_artifact(namespace, output_root, path)?;
            if path_prefix_conflict(&relative_paths, &candidate.relative_path)
                || key_prefix_conflict(&equivalence_keys, &candidate.equivalence_key)
            {
                return Err(OutputRegistrationError::destination_mapping(
                    DestinationMappingEvidence::new(
                        DestinationMappingReason::DestinationUnsupported,
                        path.clone(),
                        None,
                        candidate.evidence_path,
                    ),
                ));
            }
            relative_paths.insert(candidate.relative_path.clone());
            equivalence_keys.insert(candidate.equivalence_key.clone());
            mapped.push(candidate);
        }

        let manifest_key = HostEquivalenceKey(
            vec![host_equivalent_segment(namespace, OUTPUT_MANIFEST_BASENAME).into_boxed_str()]
                .into_boxed_slice(),
        );
        if let Some(artifact) = mapped.iter().find(|artifact| {
            artifact.equivalence_key == manifest_key
                || key_is_proper_prefix(&manifest_key, &artifact.equivalence_key)
                || key_is_proper_prefix(&artifact.equivalence_key, &manifest_key)
        }) {
            return Err(OutputRegistrationError::destination_collision(
                DestinationCollisionEvidence::ReservedManifest {
                    first_artifact_path: artifact.logical_path.clone(),
                },
            ));
        }

        if let Some((first_index, second_index, kind)) = first_destination_collision(&mapped) {
            let first = &mapped[first_index];
            let second = &mapped[second_index];
            let evidence = match kind {
                DestinationCollisionKind::MappedPath => DestinationCollisionEvidence::MappedPath {
                    first_artifact_path: first.logical_path.clone(),
                    second_artifact_path: second.logical_path.clone(),
                    mapped_destination: first.evidence_path.clone(),
                },
                DestinationCollisionKind::HostEquivalentPath => {
                    DestinationCollisionEvidence::HostEquivalentPath {
                        first_artifact_path: first.logical_path.clone(),
                        second_artifact_path: second.logical_path.clone(),
                        mapped_destination: first.evidence_path.clone(),
                    }
                }
            };
            return Err(OutputRegistrationError::destination_collision(evidence));
        }

        mapped.sort_by(|left, right| left.logical_path.cmp(&right.logical_path));
        Ok(Self {
            artifacts: mapped.into_boxed_slice(),
        })
    }

    #[cfg(test)]
    pub(super) const fn artifacts(&self) -> &[MappedArtifactDestination] {
        &self.artifacts
    }

    /// Look up a logical path in the canonical order established by construction.
    pub(super) fn artifact(&self, path: &ExportArtifactPath) -> Option<&MappedArtifactDestination> {
        self.artifacts
            .binary_search_by(|artifact| artifact.logical_path.cmp(path))
            .ok()
            .map(|index| &self.artifacts[index])
    }
}

/// Exact host-relative path and equivalence key for one logical artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MappedArtifactDestination {
    logical_path: ExportArtifactPath,
    relative_path: PathBuf,
    evidence_path: Option<EvidencePath>,
    equivalence_key: HostEquivalenceKey,
}

impl MappedArtifactDestination {
    #[cfg(test)]
    pub(super) const fn logical_path(&self) -> &ExportArtifactPath {
        &self.logical_path
    }

    pub(super) fn relative_path(&self) -> &Path {
        &self.relative_path
    }
}

/// Validate every common artifact field supported by the selected destination.
pub(super) fn preflight_capabilities(
    exporter: BuiltInExporterId,
    artifacts: &ExportArtifactSet,
) -> Result<(), OutputRegistrationError> {
    match exporter {
        BuiltInExporterId::Esm => preflight_esm(artifacts.artifacts()),
    }
}

/// Reapply the same platform capabilities to a decoded ownership manifest.
pub(super) fn preflight_manifest_capabilities(
    exporter: BuiltInExporterId,
    manifest: &OutputManifest,
) -> Result<(), OutputRegistrationError> {
    match exporter {
        BuiltInExporterId::Esm => preflight_esm(manifest.artifacts()),
    }
}

trait CapabilityArtifact {
    fn path(&self) -> &ExportArtifactPath;
    fn kind(&self) -> &ExportArtifactKind;
    fn format_version(&self) -> ExportArtifactFormatVersion;
    fn media_type(&self) -> Option<&ExportMediaType>;
    fn relationships(&self) -> &[ExportArtifactRelationship];
}

impl CapabilityArtifact for ExportArtifact {
    fn path(&self) -> &ExportArtifactPath {
        ExportArtifact::logical_path(self)
    }

    fn kind(&self) -> &ExportArtifactKind {
        ExportArtifact::kind(self)
    }

    fn format_version(&self) -> ExportArtifactFormatVersion {
        ExportArtifact::format_version(self)
    }

    fn media_type(&self) -> Option<&ExportMediaType> {
        ExportArtifact::metadata(self).media_type()
    }

    fn relationships(&self) -> &[ExportArtifactRelationship] {
        ExportArtifact::metadata(self).relationships()
    }
}

impl CapabilityArtifact for ManifestArtifact {
    fn path(&self) -> &ExportArtifactPath {
        ManifestArtifact::path(self)
    }

    fn kind(&self) -> &ExportArtifactKind {
        ManifestArtifact::kind(self)
    }

    fn format_version(&self) -> ExportArtifactFormatVersion {
        ManifestArtifact::format_version(self)
    }

    fn media_type(&self) -> Option<&ExportMediaType> {
        ManifestArtifact::media_type(self)
    }

    fn relationships(&self) -> &[ExportArtifactRelationship] {
        ManifestArtifact::relationships(self)
    }
}

fn preflight_esm<T: CapabilityArtifact>(artifacts: &[T]) -> Result<(), OutputRegistrationError> {
    for artifact in artifacts {
        let kind = artifact.kind().as_str();
        if !matches!(kind, ESM_MODULE_KIND | ESM_LOADER_KIND | ESM_ACCESSOR_KIND) {
            return Err(OutputRegistrationError::unsupported_capability(
                UnsupportedCapabilityEvidence::ArtifactKind {
                    artifact_path: artifact.path().clone(),
                    artifact_kind: artifact.kind().clone(),
                },
            ));
        }
        if artifact.format_version() != ExportArtifactFormatVersion::DRAFT_V0_1 {
            return Err(OutputRegistrationError::unsupported_capability(
                UnsupportedCapabilityEvidence::ArtifactFormatVersion {
                    artifact_path: artifact.path().clone(),
                    artifact_kind: artifact.kind().clone(),
                    format_version: artifact.format_version(),
                },
            ));
        }
        let expected_media_type = if kind == ESM_ACCESSOR_KIND {
            TYPESCRIPT_MEDIA_TYPE
        } else {
            JAVASCRIPT_MEDIA_TYPE
        };
        if artifact.media_type().map(ExportMediaType::as_str) != Some(expected_media_type) {
            return Err(OutputRegistrationError::unsupported_capability(
                UnsupportedCapabilityEvidence::MediaType {
                    artifact_path: artifact.path().clone(),
                    artifact_kind: artifact.kind().clone(),
                },
            ));
        }
        for relationship in artifact.relationships() {
            if !matches!(
                relationship.kind(),
                ExportArtifactRelationshipKind::EagerLoad
                    | ExportArtifactRelationshipKind::LazyLoad
            ) {
                return Err(OutputRegistrationError::unsupported_capability(
                    UnsupportedCapabilityEvidence::RelationshipKind {
                        artifact_path: artifact.path().clone(),
                        relationship_kind: relationship.kind(),
                    },
                ));
            }
            let target_kind = artifacts
                .binary_search_by(|candidate| candidate.path().cmp(relationship.target()))
                .ok()
                .map(|index| artifacts[index].kind().as_str());
            if artifact.kind().as_str() != ESM_LOADER_KIND || target_kind != Some(ESM_MODULE_KIND) {
                return Err(OutputRegistrationError::unsupported_capability(
                    UnsupportedCapabilityEvidence::RelationshipGraph {
                        artifact_path: Some(artifact.path().clone()),
                    },
                ));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostNamespace {
    PosixExact,
    AppleConservative,
    Windows,
    Unsupported,
}

impl HostNamespace {
    const fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_vendor = "apple") {
            Self::AppleConservative
        } else if cfg!(unix) {
            Self::PosixExact
        } else {
            Self::Unsupported
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct HostEquivalenceKey(Box<[Box<str>]>);

/// Construct the current host namespace key for an exact segment sequence.
pub(super) fn host_equivalence_key<'a>(
    segments: impl IntoIterator<Item = &'a str>,
) -> HostEquivalenceKey {
    let namespace = HostNamespace::current();
    HostEquivalenceKey(
        segments
            .into_iter()
            .map(|segment| host_equivalent_segment(namespace, segment).into_boxed_str())
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    )
}

fn map_artifact(
    namespace: HostNamespace,
    output_root: &DeliveryOutputRoot,
    logical_path: &ExportArtifactPath,
) -> Result<MappedArtifactDestination, OutputRegistrationError> {
    if namespace == HostNamespace::Unsupported {
        return Err(OutputRegistrationError::destination_mapping(
            DestinationMappingEvidence::new(
                DestinationMappingReason::DestinationUnsupported,
                logical_path.clone(),
                None,
                None,
            ),
        ));
    }

    let mut relative_path = PathBuf::new();
    let mut key = Vec::with_capacity(logical_path.segments().len());
    for (segment_index, segment) in logical_path.segments().iter().enumerate() {
        if let Err(reason) = validate_host_segment(namespace, segment.as_str()) {
            let mapped_destination = mapped_evidence(output_root, logical_path);
            return Err(OutputRegistrationError::destination_mapping(
                DestinationMappingEvidence::new(
                    reason,
                    logical_path.clone(),
                    u16::try_from(segment_index).ok(),
                    mapped_destination,
                ),
            ));
        }
        relative_path.push(segment.as_str());
        key.push(host_equivalent_segment(namespace, segment.as_str()).into_boxed_str());
    }

    Ok(MappedArtifactDestination {
        logical_path: logical_path.clone(),
        relative_path,
        evidence_path: mapped_evidence(output_root, logical_path),
        equivalence_key: HostEquivalenceKey(key.into_boxed_slice()),
    })
}

fn mapped_evidence(
    output_root: &DeliveryOutputRoot,
    logical_path: &ExportArtifactPath,
) -> Option<EvidencePath> {
    EvidencePath::try_new(
        output_root.segments().chain(
            logical_path
                .segments()
                .iter()
                .map(intlify_export::ExportArtifactPathSegment::as_str),
        ),
    )
}

fn validate_host_segment(
    namespace: HostNamespace,
    segment: &str,
) -> Result<(), DestinationMappingReason> {
    if segment.encode_utf16().count() > 255 {
        return Err(DestinationMappingReason::PathLimit);
    }
    if namespace != HostNamespace::Windows {
        return Ok(());
    }
    if segment.ends_with([' ', '.'])
        || segment
            .chars()
            .any(|character| matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
        || windows_reserved_basename(segment)
    {
        return Err(DestinationMappingReason::NameReserved);
    }
    Ok(())
}

fn windows_reserved_basename(segment: &str) -> bool {
    let basename = segment.split('.').next().unwrap_or(segment);
    let uppercase = basename.to_ascii_uppercase();
    matches!(
        uppercase.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || uppercase
        .strip_prefix("COM")
        .is_some_and(windows_reserved_port_number)
        || uppercase
            .strip_prefix("LPT")
            .is_some_and(windows_reserved_port_number)
}

fn windows_reserved_port_number(suffix: &str) -> bool {
    matches!(
        suffix,
        "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
    )
}

fn host_equivalent_segment(namespace: HostNamespace, segment: &str) -> String {
    match namespace {
        HostNamespace::PosixExact | HostNamespace::Unsupported => segment.to_owned(),
        HostNamespace::AppleConservative => segment
            .nfd()
            .flat_map(char::to_lowercase)
            .collect::<String>(),
        HostNamespace::Windows => segment.chars().flat_map(char::to_lowercase).collect(),
    }
}

fn path_prefix_conflict(paths: &BTreeSet<PathBuf>, candidate: &PathBuf) -> bool {
    paths
        .range::<PathBuf, _>((Unbounded, Excluded(candidate)))
        .next_back()
        .is_some_and(|prior| path_is_proper_prefix(prior, candidate))
        || paths
            .range::<PathBuf, _>((Included(candidate), Unbounded))
            .next()
            .is_some_and(|next| path_is_proper_prefix(candidate, next))
}

fn key_prefix_conflict(
    keys: &BTreeSet<HostEquivalenceKey>,
    candidate: &HostEquivalenceKey,
) -> bool {
    keys.range::<HostEquivalenceKey, _>((Unbounded, Excluded(candidate)))
        .next_back()
        .is_some_and(|prior| key_is_proper_prefix(prior, candidate))
        || keys
            .range::<HostEquivalenceKey, _>((Included(candidate), Unbounded))
            .next()
            .is_some_and(|next| key_is_proper_prefix(candidate, next))
}

fn path_is_proper_prefix(first: &Path, second: &Path) -> bool {
    let mut first = first.components();
    let mut second = second.components();
    loop {
        match (first.next(), second.next()) {
            (Some(left), Some(right)) if left == right => {}
            (None, Some(_)) => return true,
            _ => return false,
        }
    }
}

fn key_is_proper_prefix(first: &HostEquivalenceKey, second: &HostEquivalenceKey) -> bool {
    first.0.len() < second.0.len()
        && first
            .0
            .iter()
            .zip(second.0.iter())
            .all(|(left, right)| left == right)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DestinationCollisionKind {
    MappedPath,
    HostEquivalentPath,
}

fn first_destination_collision(
    artifacts: &[MappedArtifactDestination],
) -> Option<(usize, usize, DestinationCollisionKind)> {
    let mut relative_paths = BTreeMap::<&Path, usize>::new();
    let mut equivalence_keys = BTreeMap::<&HostEquivalenceKey, usize>::new();
    let mut selected = None;

    for (second_index, artifact) in artifacts.iter().enumerate() {
        let relative_path = artifact.relative_path.as_path();
        if let Some(&first_index) = relative_paths.get(relative_path) {
            retain_earlier_collision(
                &mut selected,
                (
                    first_index,
                    second_index,
                    DestinationCollisionKind::MappedPath,
                ),
            );
        } else {
            relative_paths.insert(relative_path, second_index);
        }

        if let Some(&first_index) = equivalence_keys.get(&artifact.equivalence_key) {
            retain_earlier_collision(
                &mut selected,
                (
                    first_index,
                    second_index,
                    DestinationCollisionKind::HostEquivalentPath,
                ),
            );
        } else {
            equivalence_keys.insert(&artifact.equivalence_key, second_index);
        }
    }

    selected
}

fn retain_earlier_collision(
    selected: &mut Option<(usize, usize, DestinationCollisionKind)>,
    candidate: (usize, usize, DestinationCollisionKind),
) {
    match selected {
        Some(current) if *current <= candidate => {}
        _ => *selected = Some(candidate),
    }
}

#[cfg(test)]
mod tests {
    use intlify_export::{
        ExportArtifact, ExportArtifactFormatVersion, ExportArtifactKind, ExportArtifactMetadata,
        ExportArtifactPath, ExportArtifactPayload, ExportArtifactRelationship,
        ExportArtifactRelationshipKind, ExportArtifactSet, ExportMediaType,
    };

    use super::{
        host_equivalent_segment, map_artifact, preflight_capabilities, DestinationMap,
        HostNamespace,
    };
    use crate::messages::delivery::{BuiltInExporterId, DeliveryOutputRoot};
    use crate::messages::registration::error::{
        DestinationCollisionEvidence, DestinationMappingReason, OutputRegistrationErrorEvidence,
        UnsupportedCapabilityEvidence,
    };

    fn artifact(
        path: &[&str],
        kind: &str,
        version: ExportArtifactFormatVersion,
        media: Option<&str>,
        relationships: Vec<ExportArtifactRelationship>,
    ) -> ExportArtifact {
        ExportArtifact::new(
            ExportArtifactPath::try_new(path.iter().copied()).unwrap(),
            ExportArtifactKind::try_new(kind).unwrap(),
            version,
            ExportArtifactPayload::try_new([]).unwrap(),
            ExportArtifactMetadata::try_new(
                media.map(|value| ExportMediaType::try_new(value).unwrap()),
                relationships,
            )
            .unwrap(),
        )
    }

    #[test]
    fn current_host_mapping_preserves_every_exact_segment() {
        let set = ExportArtifactSet::try_new(vec![artifact(
            &["locales", "locale-a.mjs"],
            "dev.intlify/esm-module",
            ExportArtifactFormatVersion::DRAFT_V0_1,
            Some("text/javascript"),
            Vec::new(),
        )])
        .unwrap();
        let root = DeliveryOutputRoot::try_new("dist/messages").unwrap();
        let mapped = DestinationMap::try_new(&root, &set).unwrap();
        assert_eq!(
            mapped.artifacts()[0].relative_path(),
            std::path::Path::new("locales/locale-a.mjs")
        );
        assert_eq!(
            mapped.artifacts()[0]
                .evidence_path
                .as_ref()
                .unwrap()
                .segments()
                .collect::<Vec<_>>(),
            ["dist", "messages", "locales", "locale-a.mjs"]
        );
    }

    #[test]
    fn reserved_manifest_is_rejected_without_rewriting_the_artifact() {
        let set = ExportArtifactSet::try_new(vec![artifact(
            &[".intlify-output-manifest.json"],
            "dev.intlify/esm-module",
            ExportArtifactFormatVersion::DRAFT_V0_1,
            Some("text/javascript"),
            Vec::new(),
        )])
        .unwrap();
        let error = DestinationMap::try_new(&DeliveryOutputRoot::try_new("out").unwrap(), &set)
            .unwrap_err();
        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::DestinationCollision(
                DestinationCollisionEvidence::ReservedManifest { first_artifact_path }
            ) if first_artifact_path.segments()[0].as_str() == ".intlify-output-manifest.json"
        ));
    }

    #[test]
    fn windows_rules_reject_reserved_names_instead_of_suffixing_them() {
        let root = DeliveryOutputRoot::try_new("out").unwrap();
        for path in [
            "CON",
            "CONIN$",
            "conout$.txt",
            "com1.txt",
            "trail.",
            "stream:name",
        ] {
            let logical = ExportArtifactPath::try_new([path]).unwrap();
            let error = map_artifact(HostNamespace::Windows, &root, &logical).unwrap_err();
            assert!(matches!(
                error.evidence(),
                OutputRegistrationErrorEvidence::DestinationMapping(evidence)
                    if evidence.reason() == DestinationMappingReason::NameReserved
                        && evidence.artifact_path() == &logical
                        && evidence.segment_index() == Some(0)
            ));
        }
    }

    #[test]
    fn platform_equivalence_keys_cover_case_and_apple_decomposition() {
        assert_eq!(
            host_equivalent_segment(HostNamespace::Windows, "Loader.MJS"),
            host_equivalent_segment(HostNamespace::Windows, "loader.mjs")
        );
        assert_eq!(
            host_equivalent_segment(HostNamespace::AppleConservative, "é.mjs"),
            host_equivalent_segment(HostNamespace::AppleConservative, "e\u{301}.mjs")
        );
        assert_ne!(
            host_equivalent_segment(HostNamespace::PosixExact, "A"),
            host_equivalent_segment(HostNamespace::PosixExact, "a")
        );
    }

    #[test]
    fn host_equivalent_paths_and_file_directory_prefixes_fail_completely() {
        let root = DeliveryOutputRoot::try_new("out").unwrap();
        let upper = ExportArtifactPath::try_new(["Loader.mjs"]).unwrap();
        let lower = ExportArtifactPath::try_new(["loader.mjs"]).unwrap();
        let error = DestinationMap::try_from_paths_with_namespace(
            HostNamespace::Windows,
            &root,
            [&upper, &lower].into_iter(),
        )
        .unwrap_err();
        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::DestinationCollision(
                DestinationCollisionEvidence::HostEquivalentPath {
                    first_artifact_path,
                    second_artifact_path,
                    ..
                }
            ) if first_artifact_path == &upper && second_artifact_path == &lower
        ));

        let parent = ExportArtifactPath::try_new(["file"]).unwrap();
        let child = ExportArtifactPath::try_new(["file", "child"]).unwrap();
        let error = DestinationMap::try_from_paths_with_namespace(
            HostNamespace::PosixExact,
            &root,
            [&parent, &child].into_iter(),
        )
        .unwrap_err();
        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::DestinationMapping(evidence)
                if evidence.reason() == DestinationMappingReason::DestinationUnsupported
                    && evidence.artifact_path() == &child
        ));
    }

    #[test]
    fn collision_selection_preserves_first_then_second_input_order() {
        let root = DeliveryOutputRoot::try_new("out").unwrap();
        let first = ExportArtifactPath::try_new(["A.mjs"]).unwrap();
        let later_first = ExportArtifactPath::try_new(["B.mjs"]).unwrap();
        let later_second = ExportArtifactPath::try_new(["b.MJS"]).unwrap();
        let second = ExportArtifactPath::try_new(["a.MJS"]).unwrap();
        let error = DestinationMap::try_from_paths_with_namespace(
            HostNamespace::Windows,
            &root,
            [&first, &later_first, &later_second, &second].into_iter(),
        )
        .unwrap_err();
        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::DestinationCollision(
                DestinationCollisionEvidence::HostEquivalentPath {
                    first_artifact_path,
                    second_artifact_path,
                    ..
                }
            ) if first_artifact_path == &first && second_artifact_path == &second
        ));
    }

    #[test]
    fn successful_mapping_canonicalizes_storage_for_binary_lookup() {
        let root = DeliveryOutputRoot::try_new("out").unwrap();
        let first = ExportArtifactPath::try_new(["a.mjs"]).unwrap();
        let second = ExportArtifactPath::try_new(["b.mjs"]).unwrap();
        let mapping = DestinationMap::try_from_paths_with_namespace(
            HostNamespace::PosixExact,
            &root,
            [&second, &first].into_iter(),
        )
        .unwrap();
        assert_eq!(
            mapping.artifacts()[0].logical_path(),
            &first,
            "successful mappings must be stored in logical-path order"
        );
        assert_eq!(mapping.artifact(&first).unwrap().logical_path(), &first);
    }

    #[test]
    fn host_equivalent_manifest_control_name_is_reserved() {
        let root = DeliveryOutputRoot::try_new("out").unwrap();
        let path = ExportArtifactPath::try_new([".INTLIFY-OUTPUT-MANIFEST.JSON"]).unwrap();
        let error = DestinationMap::try_from_paths_with_namespace(
            HostNamespace::Windows,
            &root,
            [&path].into_iter(),
        )
        .unwrap_err();
        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::DestinationCollision(
                DestinationCollisionEvidence::ReservedManifest { first_artifact_path }
            ) if first_artifact_path == &path
        ));
    }

    #[test]
    fn capability_preflight_selects_kind_version_media_then_graph() {
        let unknown = ExportArtifactSet::try_new(vec![artifact(
            &["a"],
            "dev.example/custom",
            ExportArtifactFormatVersion::new(9, 9),
            None,
            Vec::new(),
        )])
        .unwrap();
        assert!(matches!(
            preflight_capabilities(BuiltInExporterId::Esm, &unknown)
                .unwrap_err()
                .evidence(),
            OutputRegistrationErrorEvidence::UnsupportedCapability(
                UnsupportedCapabilityEvidence::ArtifactKind { .. }
            )
        ));

        let wrong_version = ExportArtifactSet::try_new(vec![artifact(
            &["a"],
            "dev.intlify/esm-module",
            ExportArtifactFormatVersion::new(0, 2),
            None,
            Vec::new(),
        )])
        .unwrap();
        assert!(matches!(
            preflight_capabilities(BuiltInExporterId::Esm, &wrong_version)
                .unwrap_err()
                .evidence(),
            OutputRegistrationErrorEvidence::UnsupportedCapability(
                UnsupportedCapabilityEvidence::ArtifactFormatVersion { .. }
            )
        ));

        let wrong_media = ExportArtifactSet::try_new(vec![artifact(
            &["a"],
            "dev.intlify/esm-module",
            ExportArtifactFormatVersion::DRAFT_V0_1,
            Some("text/typescript"),
            Vec::new(),
        )])
        .unwrap();
        assert!(matches!(
            preflight_capabilities(BuiltInExporterId::Esm, &wrong_media)
                .unwrap_err()
                .evidence(),
            OutputRegistrationErrorEvidence::UnsupportedCapability(
                UnsupportedCapabilityEvidence::MediaType { .. }
            )
        ));

        let target = ExportArtifactPath::try_new(["b"]).unwrap();
        let invalid_graph = ExportArtifactSet::try_new(vec![
            artifact(
                &["a"],
                "dev.intlify/esm-module",
                ExportArtifactFormatVersion::DRAFT_V0_1,
                Some("text/javascript"),
                vec![ExportArtifactRelationship::new(
                    ExportArtifactRelationshipKind::LazyLoad,
                    target,
                )],
            ),
            artifact(
                &["b"],
                "dev.intlify/esm-module",
                ExportArtifactFormatVersion::DRAFT_V0_1,
                Some("text/javascript"),
                Vec::new(),
            ),
        ])
        .unwrap();
        assert!(matches!(
            preflight_capabilities(BuiltInExporterId::Esm, &invalid_graph)
                .unwrap_err()
                .evidence(),
            OutputRegistrationErrorEvidence::UnsupportedCapability(
                UnsupportedCapabilityEvidence::RelationshipGraph { .. }
            )
        ));

        let graph_before_later_kind = ExportArtifactSet::try_new(vec![
            artifact(
                &["a"],
                "dev.intlify/esm-module",
                ExportArtifactFormatVersion::DRAFT_V0_1,
                Some("text/javascript"),
                vec![ExportArtifactRelationship::new(
                    ExportArtifactRelationshipKind::LazyLoad,
                    ExportArtifactPath::try_new(["b"]).unwrap(),
                )],
            ),
            artifact(
                &["b"],
                "dev.intlify/esm-module",
                ExportArtifactFormatVersion::DRAFT_V0_1,
                Some("text/javascript"),
                Vec::new(),
            ),
            artifact(
                &["z"],
                "dev.example/custom",
                ExportArtifactFormatVersion::DRAFT_V0_1,
                Some("text/javascript"),
                Vec::new(),
            ),
        ])
        .unwrap();
        assert!(matches!(
            preflight_capabilities(BuiltInExporterId::Esm, &graph_before_later_kind)
                .unwrap_err()
                .evidence(),
            OutputRegistrationErrorEvidence::UnsupportedCapability(
                UnsupportedCapabilityEvidence::RelationshipGraph {
                    artifact_path: Some(path)
                }
            ) if path.segments()[0].as_str() == "a"
        ));
    }
}
