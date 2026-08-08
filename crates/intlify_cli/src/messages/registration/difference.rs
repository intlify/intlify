// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Deterministic complete comparison between expected and owned output state.
//!
//! This module owns the five closed check-difference records, component
//! coalescing, canonical ordering, and the fixed complete-vector ceiling. It
//! consumes only checked manifest and byte-comparison observations and never
//! reads or mutates the filesystem.

use std::collections::BTreeMap;

use intlify_export::ExportArtifactPath;
use serde_json::{json, Value};

use super::error::OutputRegistrationError;
use super::manifest::CheckedOutputManifest;

/// One manifest record plus 5,121 expected paths and 5,121 prior-only paths.
pub(super) const CHECK_DIFFERENCE_LIMIT: usize = 10_243;

/// Complete check result for one selected target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::messages) struct CheckComparison {
    differences: Box<[CheckDifference]>,
}

impl CheckComparison {
    /// Construct the sole difference for an absent or adoptable empty root.
    pub(super) fn output_missing() -> Self {
        Self {
            differences: Box::new([CheckDifference::OutputMissing]),
        }
    }

    /// Compare one expected manifest with a safely inspected managed root.
    pub(super) fn compare_managed(
        expected: &CheckedOutputManifest,
        observed: &ManagedOutputObservation,
    ) -> Result<Self, OutputRegistrationError> {
        let mut differences = DifferenceAccumulator::new();
        if !observed.manifest_is_canonical {
            differences.push(CheckDifference::ManifestNoncanonical)?;
        }

        for expected_artifact in expected.manifest().artifacts() {
            let path = expected_artifact.path();
            let Some(prior_artifact) = observed.manifest.manifest().artifact(path) else {
                differences.push(CheckDifference::ArtifactMissing { path: path.clone() })?;
                continue;
            };
            let Some(file) = observed.files.get(path) else {
                differences.push(CheckDifference::ArtifactMissing { path: path.clone() })?;
                continue;
            };
            if !file.present {
                differences.push(CheckDifference::ArtifactMissing { path: path.clone() })?;
                continue;
            }

            let mut components = Vec::with_capacity(5);
            if expected_artifact.kind() != prior_artifact.kind() {
                components.push(ArtifactChangedComponent::Kind);
            }
            if expected_artifact.format_version() != prior_artifact.format_version() {
                components.push(ArtifactChangedComponent::FormatVersion);
            }
            if expected_artifact.media_type() != prior_artifact.media_type() {
                components.push(ArtifactChangedComponent::MediaType);
            }
            if expected_artifact.relationships() != prior_artifact.relationships() {
                components.push(ArtifactChangedComponent::Relationships);
            }
            if expected_artifact.payload_bytes() != prior_artifact.payload_bytes()
                || expected_artifact.payload_fingerprint() != prior_artifact.payload_fingerprint()
                || file.expected_payload_equal != Some(true)
            {
                components.push(ArtifactChangedComponent::Payload);
            }
            if !components.is_empty() {
                differences.push(CheckDifference::ArtifactChanged {
                    path: path.clone(),
                    components: components.into_boxed_slice(),
                })?;
            }
        }

        for prior_artifact in observed.manifest.manifest().artifacts() {
            if expected
                .manifest()
                .artifact(prior_artifact.path())
                .is_none()
            {
                differences.push(CheckDifference::ArtifactStale {
                    path: prior_artifact.path().clone(),
                })?;
            }
        }

        Ok(Self {
            differences: differences.finish(),
        })
    }

    #[cfg(test)]
    pub(super) const fn differences(&self) -> &[CheckDifference] {
        &self.differences
    }

    pub(in crate::messages) fn is_matched(&self) -> bool {
        self.differences.is_empty()
    }

    /// Project the closed comparison records into their stable reporter shape.
    pub(in crate::messages) fn reporter_differences(&self) -> Vec<Value> {
        self.differences.iter().map(difference_value).collect()
    }
}

fn difference_value(difference: &CheckDifference) -> Value {
    match difference {
        CheckDifference::OutputMissing => json!({ "kind": "output_missing" }),
        CheckDifference::ManifestNoncanonical => json!({ "kind": "manifest_noncanonical" }),
        CheckDifference::ArtifactMissing { path } => json!({
            "kind": "artifact_missing",
            "path": artifact_path_value(path),
        }),
        CheckDifference::ArtifactChanged { path, components } => json!({
            "kind": "artifact_changed",
            "path": artifact_path_value(path),
            "components": components.iter().map(|component| match component {
                ArtifactChangedComponent::Kind => "kind",
                ArtifactChangedComponent::FormatVersion => "format_version",
                ArtifactChangedComponent::MediaType => "media_type",
                ArtifactChangedComponent::Relationships => "relationships",
                ArtifactChangedComponent::Payload => "payload",
            }).collect::<Vec<_>>(),
        }),
        CheckDifference::ArtifactStale { path } => json!({
            "kind": "artifact_stale",
            "path": artifact_path_value(path),
        }),
    }
}

fn artifact_path_value(path: &ExportArtifactPath) -> Value {
    Value::Array(
        path.segments()
            .iter()
            .map(|segment| Value::String(segment.as_str().to_owned()))
            .collect(),
    )
}

/// One closed check difference record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CheckDifference {
    OutputMissing,
    ManifestNoncanonical,
    ArtifactMissing {
        path: ExportArtifactPath,
    },
    ArtifactChanged {
        path: ExportArtifactPath,
        components: Box<[ArtifactChangedComponent]>,
    },
    ArtifactStale {
        path: ExportArtifactPath,
    },
}

/// Canonical metadata or payload components for one changed artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ArtifactChangedComponent {
    Kind,
    FormatVersion,
    MediaType,
    Relationships,
    Payload,
}

/// Safely inspected managed-root state consumed by pure comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ManagedOutputObservation {
    manifest: CheckedOutputManifest,
    manifest_is_canonical: bool,
    files: BTreeMap<ExportArtifactPath, ObservedArtifactFile>,
}

impl ManagedOutputObservation {
    pub(super) fn new(
        manifest: CheckedOutputManifest,
        manifest_is_canonical: bool,
        files: BTreeMap<ExportArtifactPath, ObservedArtifactFile>,
    ) -> Self {
        Self {
            manifest,
            manifest_is_canonical,
            files,
        }
    }
}

/// Presence and byte-equality result for one prior manifest path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ObservedArtifactFile {
    present: bool,
    expected_payload_equal: Option<bool>,
}

impl ObservedArtifactFile {
    pub(super) const fn missing() -> Self {
        Self {
            present: false,
            expected_payload_equal: None,
        }
    }

    pub(super) const fn present(expected_payload_equal: Option<bool>) -> Self {
        Self {
            present: true,
            expected_payload_equal,
        }
    }

    /// Whether one safely read file exactly matched the selected payload proof.
    pub(super) const fn is_present_and_equal(self) -> bool {
        self.present && matches!(self.expected_payload_equal, Some(true))
    }
}

struct DifferenceAccumulator {
    differences: Vec<CheckDifference>,
}

impl DifferenceAccumulator {
    fn new() -> Self {
        Self {
            differences: Vec::new(),
        }
    }

    fn push(&mut self, difference: CheckDifference) -> Result<(), OutputRegistrationError> {
        if self.differences.len() == CHECK_DIFFERENCE_LIMIT {
            return Err(OutputRegistrationError::internal_invariant());
        }
        self.differences.push(difference);
        Ok(())
    }

    fn finish(self) -> Box<[CheckDifference]> {
        self.differences.into_boxed_slice()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use intlify_export::{
        ExportArtifact, ExportArtifactFormatVersion, ExportArtifactKind, ExportArtifactMetadata,
        ExportArtifactPath, ExportArtifactPayload, ExportArtifactRelationship,
        ExportArtifactRelationshipKind, ExportArtifactSet, ExportMediaType,
    };

    use super::{
        ArtifactChangedComponent, CheckComparison, CheckDifference, DifferenceAccumulator,
        ManagedOutputObservation, ObservedArtifactFile, CHECK_DIFFERENCE_LIMIT,
    };
    use crate::messages::delivery::BuiltInExporterId;
    use crate::messages::registration::manifest::CheckedOutputManifest;

    fn artifact(path: &str, kind: &str, media: &str, payload: &[u8]) -> ExportArtifact {
        ExportArtifact::new(
            ExportArtifactPath::try_new([path]).unwrap(),
            ExportArtifactKind::try_new(kind).unwrap(),
            ExportArtifactFormatVersion::DRAFT_V0_1,
            ExportArtifactPayload::try_new(payload.to_vec()).unwrap(),
            ExportArtifactMetadata::try_new(
                Some(ExportMediaType::try_new(media).unwrap()),
                Vec::new(),
            )
            .unwrap(),
        )
    }

    fn manifest(artifacts: Vec<ExportArtifact>) -> CheckedOutputManifest {
        CheckedOutputManifest::from_artifact_set(
            BuiltInExporterId::Esm,
            &ExportArtifactSet::try_new(artifacts).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn managed_comparison_coalesces_components_in_canonical_order() {
        let expected = manifest(vec![artifact(
            "a",
            "dev.intlify/esm-module",
            "text/javascript",
            b"expected",
        )]);
        let prior = manifest(vec![artifact(
            "a",
            "dev.intlify/loader-map",
            "text/typescript",
            b"prior",
        )]);
        let path = ExportArtifactPath::try_new(["a"]).unwrap();
        let observed = ManagedOutputObservation::new(
            prior,
            false,
            BTreeMap::from([(path.clone(), ObservedArtifactFile::present(Some(false)))]),
        );
        let comparison = CheckComparison::compare_managed(&expected, &observed).unwrap();
        assert_eq!(
            comparison.differences(),
            [
                CheckDifference::ManifestNoncanonical,
                CheckDifference::ArtifactChanged {
                    path,
                    components: Box::new([
                        ArtifactChangedComponent::Kind,
                        ArtifactChangedComponent::MediaType,
                        ArtifactChangedComponent::Payload,
                    ]),
                },
            ]
        );
    }

    #[test]
    fn changed_record_reports_all_five_components_in_fixed_order() {
        let source_path = ExportArtifactPath::try_new(["a"]).unwrap();
        let target_path = ExportArtifactPath::try_new(["z"]).unwrap();
        let source = |kind: &str,
                      version: ExportArtifactFormatVersion,
                      media: &str,
                      relationship_kind: ExportArtifactRelationshipKind,
                      payload: &[u8]| {
            ExportArtifact::new(
                source_path.clone(),
                ExportArtifactKind::try_new(kind).unwrap(),
                version,
                ExportArtifactPayload::try_new(payload.to_vec()).unwrap(),
                ExportArtifactMetadata::try_new(
                    Some(ExportMediaType::try_new(media).unwrap()),
                    vec![ExportArtifactRelationship::new(
                        relationship_kind,
                        target_path.clone(),
                    )],
                )
                .unwrap(),
            )
        };
        let target = artifact("z", "dev.intlify/esm-module", "text/javascript", b"target");
        let expected = manifest(vec![
            source(
                "dev.intlify/loader-map",
                ExportArtifactFormatVersion::DRAFT_V0_1,
                "text/javascript",
                ExportArtifactRelationshipKind::LazyLoad,
                b"expected",
            ),
            target.clone(),
        ]);
        let prior = manifest(vec![
            source(
                "dev.intlify/typescript-accessor",
                ExportArtifactFormatVersion::new(0, 2),
                "text/typescript",
                ExportArtifactRelationshipKind::EagerLoad,
                b"prior",
            ),
            target,
        ]);
        let observed = ManagedOutputObservation::new(
            prior,
            true,
            BTreeMap::from([
                (
                    source_path.clone(),
                    ObservedArtifactFile::present(Some(false)),
                ),
                (target_path, ObservedArtifactFile::present(Some(true))),
            ]),
        );

        assert_eq!(
            CheckComparison::compare_managed(&expected, &observed)
                .unwrap()
                .differences(),
            [CheckDifference::ArtifactChanged {
                path: source_path,
                components: Box::new([
                    ArtifactChangedComponent::Kind,
                    ArtifactChangedComponent::FormatVersion,
                    ArtifactChangedComponent::MediaType,
                    ArtifactChangedComponent::Relationships,
                    ArtifactChangedComponent::Payload,
                ]),
            }]
        );
    }

    #[test]
    fn missing_expected_paths_precede_stale_prior_paths() {
        let expected = manifest(vec![artifact(
            "a",
            "dev.intlify/esm-module",
            "text/javascript",
            b"a",
        )]);
        let prior = manifest(vec![artifact(
            "z",
            "dev.intlify/esm-module",
            "text/javascript",
            b"z",
        )]);
        let observed = ManagedOutputObservation::new(prior, true, BTreeMap::new());
        let comparison = CheckComparison::compare_managed(&expected, &observed).unwrap();
        assert!(matches!(
            comparison.differences(),
            [CheckDifference::ArtifactMissing { path: missing }, CheckDifference::ArtifactStale { path: stale }]
                if missing.segments()[0].as_str() == "a" && stale.segments()[0].as_str() == "z"
        ));
    }

    #[test]
    fn missing_payload_file_wins_over_metadata_components() {
        let expected = manifest(vec![artifact(
            "a",
            "dev.intlify/esm-module",
            "text/javascript",
            b"expected",
        )]);
        let prior = manifest(vec![artifact(
            "a",
            "dev.intlify/loader-map",
            "text/typescript",
            b"prior",
        )]);
        let path = ExportArtifactPath::try_new(["a"]).unwrap();
        let observed = ManagedOutputObservation::new(
            prior,
            true,
            BTreeMap::from([(path.clone(), ObservedArtifactFile::missing())]),
        );
        assert_eq!(
            CheckComparison::compare_managed(&expected, &observed)
                .unwrap()
                .differences(),
            [CheckDifference::ArtifactMissing { path }]
        );
    }

    #[test]
    fn unknown_payload_equality_is_a_payload_difference() {
        let expected = manifest(vec![artifact(
            "a",
            "dev.intlify/esm-module",
            "text/javascript",
            b"payload",
        )]);
        let path = ExportArtifactPath::try_new(["a"]).unwrap();
        let observed = ManagedOutputObservation::new(
            expected.clone(),
            true,
            BTreeMap::from([(path.clone(), ObservedArtifactFile::present(None))]),
        );
        assert_eq!(
            CheckComparison::compare_managed(&expected, &observed)
                .unwrap()
                .differences(),
            [CheckDifference::ArtifactChanged {
                path,
                components: Box::new([ArtifactChangedComponent::Payload]),
            }]
        );
    }

    #[test]
    fn exact_difference_ceiling_is_complete_and_first_excess_is_invariant() {
        let mut accumulator = DifferenceAccumulator::new();
        for _ in 0..CHECK_DIFFERENCE_LIMIT {
            accumulator
                .push(CheckDifference::ManifestNoncanonical)
                .unwrap();
        }
        assert_eq!(accumulator.differences.len(), CHECK_DIFFERENCE_LIMIT);
        assert!(accumulator
            .push(CheckDifference::ManifestNoncanonical)
            .is_err());
    }

    #[test]
    fn output_missing_is_one_record_and_matched_is_empty() {
        assert_eq!(
            CheckComparison::output_missing().differences(),
            [CheckDifference::OutputMissing]
        );
        let empty = manifest(Vec::new());
        let observed = ManagedOutputObservation::new(empty.clone(), true, BTreeMap::new());
        assert!(CheckComparison::compare_managed(&empty, &observed)
            .unwrap()
            .is_matched());
    }
}
