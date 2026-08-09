// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Private output-destination preparation and ownership inspection.
//!
//! This module combines a checked exporter artifact set with deterministic
//! host mapping, a strict ownership manifest, non-mutating output checks, and
//! target-local durable registration. Public command reporting remains a
//! separate integration responsibility.

mod control;
mod destination;
mod difference;
mod error;
mod inspection;
mod manifest;
mod transaction;

use std::path::Path;

use intlify_export::ExportArtifactSet;

use self::destination::{preflight_capabilities, DestinationMap};
use self::difference::CheckComparison;
pub(in crate::messages) use self::error::{output_state_token, reporter_error};
use self::error::{OutputRegistrationError, OwnershipFailureReason, RegistrationFailureEvidence};
use self::manifest::{CheckedOutputManifest, ManifestCodecErrorKind};
pub(in crate::messages) use self::transaction::WriteRegistrationStatus;
use super::delivery::{BuiltInExporterId, DeliveryOutputRoot, ResolvedDeliveryTarget};
use super::observation::{
    observation_checksum, MessageBenchmarkObserver, MessageBenchmarkStage,
    NoopMessageBenchmarkObserver,
};

/// Fully preflighted expected output for one immutable selected target.
#[derive(Debug)]
pub(crate) struct PreparedOutput<'a> {
    exporter: BuiltInExporterId,
    output_root: &'a DeliveryOutputRoot,
    artifacts: &'a ExportArtifactSet,
    manifest: CheckedOutputManifest,
    // The durable writer consumes this exact preflighted mapping next.
    destinations: DestinationMap,
}

impl<'a> PreparedOutput<'a> {
    /// Complete capability, manifest-capacity, mapping, and collision preflight.
    pub(crate) fn try_new(
        target: &'a ResolvedDeliveryTarget,
        artifacts: &'a ExportArtifactSet,
    ) -> Result<Self, OutputRegistrationError> {
        Self::try_new_with_observer(target, artifacts, &mut NoopMessageBenchmarkObserver)
    }

    pub(crate) fn try_new_with_observer<O>(
        target: &'a ResolvedDeliveryTarget,
        artifacts: &'a ExportArtifactSet,
        observer: &mut O,
    ) -> Result<Self, OutputRegistrationError>
    where
        O: MessageBenchmarkObserver + ?Sized,
    {
        let identity = target.name().as_str();
        observer.begin(MessageBenchmarkStage::OutputCapabilityPreflight, identity);
        let exporter = target.exporter();
        let manifest = match preflight_capabilities(exporter, artifacts).and_then(|()| {
            CheckedOutputManifest::from_artifact_set(exporter, artifacts).map_err(|error| {
                match error.kind() {
                    ManifestCodecErrorKind::Limit => OutputRegistrationError::registration(
                        RegistrationFailureEvidence::manifest(
                            OwnershipFailureReason::ManifestLimit,
                            None,
                        ),
                    ),
                    ManifestCodecErrorKind::Invalid
                    | ManifestCodecErrorKind::Unsupported
                    | ManifestCodecErrorKind::InternalInvariant => {
                        OutputRegistrationError::internal_invariant()
                    }
                }
            })
        }) {
            Ok(manifest) => manifest,
            Err(error) => {
                observer.abandon(MessageBenchmarkStage::OutputCapabilityPreflight, identity);
                return Err(error);
            }
        };
        finish_stage(
            observer,
            MessageBenchmarkStage::OutputCapabilityPreflight,
            identity,
            artifacts,
            b"capability_preflight",
        );

        observer.begin(
            MessageBenchmarkStage::OutputPathMappingAndOwnershipInspection,
            identity,
        );
        let destinations = match DestinationMap::try_new(target.out(), artifacts) {
            Ok(destinations) => destinations,
            Err(error) => {
                observer.abandon(
                    MessageBenchmarkStage::OutputPathMappingAndOwnershipInspection,
                    identity,
                );
                return Err(error);
            }
        };
        finish_stage(
            observer,
            MessageBenchmarkStage::OutputPathMappingAndOwnershipInspection,
            identity,
            artifacts,
            b"path_mapping_and_ownership_inspection",
        );
        Ok(Self {
            exporter,
            output_root: target.out(),
            artifacts,
            manifest,
            destinations,
        })
    }

    /// Compare expected bytes with one safe filesystem snapshot without mutation.
    pub(crate) fn check(
        &self,
        project_root: &Path,
    ) -> Result<CheckComparison, OutputRegistrationError> {
        transaction::check_output(self, project_root)
    }

    pub(crate) fn check_with_observer<O>(
        &self,
        project_root: &Path,
        identity: &str,
        observer: &mut O,
    ) -> Result<CheckComparison, OutputRegistrationError>
    where
        O: MessageBenchmarkObserver + ?Sized,
    {
        observer.begin(MessageBenchmarkStage::OutputCheckComparison, identity);
        let comparison = match transaction::check_output(self, project_root) {
            Ok(comparison) => comparison,
            Err(error) => {
                observer.abandon(MessageBenchmarkStage::OutputCheckComparison, identity);
                return Err(error);
            }
        };
        finish_stage(
            observer,
            MessageBenchmarkStage::OutputCheckComparison,
            identity,
            self.artifacts,
            if comparison.is_matched() {
                b"check_matched"
            } else {
                b"check_different"
            },
        );
        Ok(comparison)
    }

    /// Durably install the complete expected output or prove it is unchanged.
    pub(crate) fn write(
        &self,
        project_root: &Path,
    ) -> Result<transaction::WriteRegistrationOutcome, OutputRegistrationError> {
        transaction::write_output(self, project_root)
    }

    pub(crate) fn write_with_observer<O>(
        &self,
        project_root: &Path,
        identity: &str,
        observer: &mut O,
    ) -> Result<transaction::WriteRegistrationOutcome, OutputRegistrationError>
    where
        O: MessageBenchmarkObserver + ?Sized,
    {
        transaction::write_output_with_observer(self, project_root, identity, observer)
    }

    const fn manifest(&self) -> &CheckedOutputManifest {
        &self.manifest
    }

    /// Return the exact mapping retained for the durable write transaction.
    const fn destinations(&self) -> &DestinationMap {
        &self.destinations
    }
}

fn finish_stage(
    observer: &mut (impl MessageBenchmarkObserver + ?Sized),
    stage: MessageBenchmarkStage,
    identity: &str,
    artifacts: &ExportArtifactSet,
    state: &[u8],
) {
    observer.finish(stage, identity);
    let mut fields = vec![state.to_vec()];
    for artifact in artifacts.artifacts() {
        fields.push(
            artifact
                .logical_path()
                .segments()
                .iter()
                .map(intlify_export::ExportArtifactPathSegment::as_str)
                .collect::<Vec<_>>()
                .join("/")
                .into_bytes(),
        );
        fields.push(artifact.payload().fingerprint().as_bytes().to_vec());
    }
    observer.observe(
        stage,
        identity,
        observation_checksum(stage.cost().as_bytes(), fields.iter().map(Vec::as_slice)),
    );
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use intlify_contract::{LinkLimits, Locale};
    use intlify_export::{
        ExportArtifact, ExportArtifactFormatVersion, ExportArtifactKind, ExportArtifactMetadata,
        ExportArtifactPath, ExportArtifactPayload, ExportArtifactSet, ExportMediaType,
    };
    use intlify_linker::{DynamicReferenceMode, LinkPolicy, PlacementPolicy};
    use tempfile::{Builder, TempDir};

    use super::difference::{ArtifactChangedComponent, CheckDifference};
    use super::error::{
        OutputRegistrationErrorEvidence, OwnershipFailureReason, RegistrationFailureEvidence,
        UnsupportedCapabilityEvidence,
    };
    use super::manifest::{CheckedOutputManifest, OUTPUT_MANIFEST_BASENAME};
    use super::PreparedOutput;
    use crate::messages::delivery::{
        BuiltInExporterId, DeliveryTargetInput, ResolvedDeliveryTarget, ResolvedDeliveryTargets,
    };

    struct TempProject(TempDir);

    impl TempProject {
        fn new() -> Self {
            Self(
                Builder::new()
                    .prefix("intlify-output-inspection-")
                    .tempdir()
                    .unwrap(),
            )
        }

        fn path(&self) -> &Path {
            self.0.path()
        }

        fn output(&self) -> PathBuf {
            self.path().join("generated/messages")
        }
    }

    fn policy() -> LinkPolicy {
        LinkPolicy::try_new(
            vec![Locale::try_new("en").unwrap()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &LinkLimits::default(),
        )
        .unwrap()
    }

    fn target() -> ResolvedDeliveryTarget {
        ResolvedDeliveryTargets::try_new(
            &[DeliveryTargetInput::new(
                "web",
                "esm",
                "generated/messages",
                &[],
                None,
            )],
            &policy(),
        )
        .unwrap()
        .targets()[0]
            .clone()
    }

    fn artifact(path: &[&str], kind: &str, media: &str, payload: &[u8]) -> ExportArtifact {
        ExportArtifact::new(
            ExportArtifactPath::try_new(path.iter().copied()).unwrap(),
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

    fn loader_set(payload: &[u8]) -> ExportArtifactSet {
        ExportArtifactSet::try_new(vec![artifact(
            &["loader.mjs"],
            "dev.intlify/loader-map",
            "text/javascript",
            payload,
        )])
        .unwrap()
    }

    fn module_set(records: &[(&[&str], &[u8])]) -> ExportArtifactSet {
        ExportArtifactSet::try_new(
            records
                .iter()
                .map(|(path, payload)| {
                    artifact(path, "dev.intlify/esm-module", "text/javascript", payload)
                })
                .collect(),
        )
        .unwrap()
    }

    fn write_managed(project: &TempProject, set: &ExportArtifactSet, canonical: bool) {
        let target = target();
        let prepared = PreparedOutput::try_new(&target, set).unwrap();
        fs::create_dir_all(project.output()).unwrap();
        for artifact in set.artifacts() {
            let path = artifact
                .logical_path()
                .segments()
                .iter()
                .fold(project.output(), |path, segment| {
                    path.join(segment.as_str())
                });
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, artifact.payload().as_bytes()).unwrap();
        }
        let bytes = if canonical {
            prepared.manifest().canonical_bytes().to_vec()
        } else {
            // JSON permits trailing whitespace, so this preserves the decoded
            // manifest while guaranteeing a byte-distinct noncanonical wire form.
            let mut bytes = prepared.manifest().canonical_bytes().to_vec();
            bytes.push(b' ');
            bytes
        };
        fs::write(project.output().join(OUTPUT_MANIFEST_BASENAME), bytes).unwrap();
    }

    fn tree_contents(root: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
        fn visit(root: &Path, relative: &Path, output: &mut Vec<(PathBuf, Option<Vec<u8>>)>) {
            let mut entries = fs::read_dir(root.join(relative))
                .unwrap()
                .map(Result::unwrap)
                .collect::<Vec<_>>();
            entries.sort_by_key(fs::DirEntry::file_name);
            for entry in entries {
                let child = relative.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    output.push((child.clone(), None));
                    visit(root, &child, output);
                } else {
                    output.push((child, Some(fs::read(entry.path()).unwrap())));
                }
            }
        }
        let mut output = Vec::new();
        if root.is_dir() {
            visit(root, Path::new(""), &mut output);
        }
        output
    }

    #[test]
    fn absent_and_empty_roots_are_one_non_mutating_difference() {
        let project = TempProject::new();
        let set = loader_set(b"loader");
        let target = target();
        let prepared = PreparedOutput::try_new(&target, &set).unwrap();
        assert_eq!(
            prepared.check(project.path()).unwrap().differences(),
            [CheckDifference::OutputMissing]
        );
        assert!(!project.output().exists());

        fs::create_dir_all(project.output()).unwrap();
        let before = tree_contents(&project.output());
        assert_eq!(
            prepared.check(project.path()).unwrap().differences(),
            [CheckDifference::OutputMissing]
        );
        assert_eq!(tree_contents(&project.output()), before);
    }

    #[test]
    fn exact_owned_output_matches_without_modifying_any_entry() {
        let project = TempProject::new();
        let set = loader_set(b"loader");
        write_managed(&project, &set, true);
        let before = tree_contents(&project.output());
        let target = target();
        let comparison = PreparedOutput::try_new(&target, &set)
            .unwrap()
            .check(project.path())
            .unwrap();
        assert!(comparison.is_matched());
        assert_eq!(tree_contents(&project.output()), before);
    }

    #[test]
    fn host_equivalent_ancestor_directories_can_hold_injective_artifact_paths() {
        let project = TempProject::new();
        let set = module_set(&[(&["A", "x.mjs"], b"x"), (&["a", "y.mjs"], b"y")]);
        // Case-insensitive hosts intentionally share one physical ancestor;
        // the distinct final names keep the destination mapping injective.
        write_managed(&project, &set, true);
        let target = target();
        assert!(PreparedOutput::try_new(&target, &set)
            .unwrap()
            .check(project.path())
            .unwrap()
            .is_matched());
    }

    #[test]
    fn noncanonical_manifest_is_a_safe_difference() {
        let project = TempProject::new();
        let set = loader_set(b"loader");
        write_managed(&project, &set, false);
        let target = target();
        assert_eq!(
            PreparedOutput::try_new(&target, &set)
                .unwrap()
                .check(project.path())
                .unwrap()
                .differences(),
            [CheckDifference::ManifestNoncanonical]
        );
    }

    #[test]
    fn actual_payload_bytes_are_compared_even_when_manifest_digest_matches() {
        let project = TempProject::new();
        let set = loader_set(b"expected");
        write_managed(&project, &set, true);
        fs::write(project.output().join("loader.mjs"), b"tampered").unwrap();
        let target = target();
        assert!(matches!(
            PreparedOutput::try_new(&target, &set)
                .unwrap()
                .check(project.path())
                .unwrap()
                .differences(),
            [CheckDifference::ArtifactChanged { path, components }]
                if path.segments()[0].as_str() == "loader.mjs"
                    && components.as_ref() == [ArtifactChangedComponent::Payload]
        ));
    }

    #[test]
    fn missing_manifest_owned_payload_is_a_safe_missing_difference() {
        let project = TempProject::new();
        let set = loader_set(b"expected");
        write_managed(&project, &set, true);
        fs::remove_file(project.output().join("loader.mjs")).unwrap();
        let target = target();
        assert!(matches!(
            PreparedOutput::try_new(&target, &set)
                .unwrap()
                .check(project.path())
                .unwrap()
                .differences(),
            [CheckDifference::ArtifactMissing { path }]
                if path.segments()[0].as_str() == "loader.mjs"
        ));
    }

    #[test]
    fn missing_changed_and_stale_records_have_complete_canonical_order() {
        let project = TempProject::new();
        let prior = module_set(&[(&["b.mjs"], b"prior"), (&["z.mjs"], b"stale")]);
        write_managed(&project, &prior, true);
        let expected = module_set(&[(&["a.mjs"], b"missing"), (&["b.mjs"], b"expected")]);
        let target = target();
        assert!(matches!(
            PreparedOutput::try_new(&target, &expected)
                .unwrap()
                .check(project.path())
                .unwrap()
                .differences(),
            [
                CheckDifference::ArtifactMissing { path: missing },
                CheckDifference::ArtifactChanged { path: changed, components },
                CheckDifference::ArtifactStale { path: stale },
            ] if missing.segments()[0].as_str() == "a.mjs"
                && changed.segments()[0].as_str() == "b.mjs"
                && components.as_ref() == [ArtifactChangedComponent::Payload]
                && stale.segments()[0].as_str() == "z.mjs"
        ));
    }

    #[test]
    fn invalid_unsupported_and_missing_manifests_are_operational() {
        let set = loader_set(b"loader");
        let target = target();
        for (bytes, reason) in [
            (b"not-json".as_slice(), OwnershipFailureReason::ManifestInvalid),
            (
                b"{\"schemaVersion\":{\"major\":0,\"minor\":2},\"exporter\":\"esm\",\"artifacts\":[]}".as_slice(),
                OwnershipFailureReason::ManifestUnsupported,
            ),
        ] {
            let project = TempProject::new();
            fs::create_dir_all(project.output()).unwrap();
            fs::write(project.output().join(OUTPUT_MANIFEST_BASENAME), bytes).unwrap();
            let error = PreparedOutput::try_new(&target, &set)
                .unwrap()
                .check(project.path())
                .unwrap_err();
            assert!(matches!(
                error.evidence(),
                OutputRegistrationErrorEvidence::Registration(
                    RegistrationFailureEvidence::Ownership { reason: actual, .. }
                ) if *actual == reason
            ));
        }

        let project = TempProject::new();
        fs::create_dir_all(project.output()).unwrap();
        fs::write(project.output().join("unowned"), b"keep").unwrap();
        let error = PreparedOutput::try_new(&target, &set)
            .unwrap()
            .check(project.path())
            .unwrap_err();
        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::Registration(RegistrationFailureEvidence::Ownership {
                reason: OwnershipFailureReason::ManifestMissing,
                ..
            })
        ));
        assert_eq!(fs::read(project.output().join("unowned")).unwrap(), b"keep");
    }

    #[test]
    fn prior_manifest_reapplies_selected_exporter_capabilities() {
        let project = TempProject::new();
        let prior = ExportArtifactSet::try_new(vec![artifact(
            &["custom.bin"],
            "dev.example/custom",
            "application/octet-stream",
            b"keep",
        )])
        .unwrap();
        let manifest =
            CheckedOutputManifest::from_artifact_set(BuiltInExporterId::Esm, &prior).unwrap();
        fs::create_dir_all(project.output()).unwrap();
        fs::write(project.output().join("custom.bin"), b"keep").unwrap();
        fs::write(
            project.output().join(OUTPUT_MANIFEST_BASENAME),
            manifest.canonical_bytes(),
        )
        .unwrap();

        let target = target();
        let error = PreparedOutput::try_new(&target, &loader_set(b"loader"))
            .unwrap()
            .check(project.path())
            .unwrap_err();
        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::UnsupportedCapability(
                UnsupportedCapabilityEvidence::ArtifactKind { artifact_path, .. }
            ) if artifact_path.segments()[0].as_str() == "custom.bin"
        ));
        assert_eq!(
            fs::read(project.output().join("custom.bin")).unwrap(),
            b"keep"
        );
    }

    #[test]
    fn unowned_entry_fails_without_deleting_or_overwriting_it() {
        let project = TempProject::new();
        let set = loader_set(b"loader");
        write_managed(&project, &set, true);
        fs::write(project.output().join("caller-owned"), b"keep").unwrap();
        let before = tree_contents(&project.output());
        let target = target();
        let error = PreparedOutput::try_new(&target, &set)
            .unwrap()
            .check(project.path())
            .unwrap_err();
        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::Registration(RegistrationFailureEvidence::Ownership {
                reason: OwnershipFailureReason::UnownedEntry,
                ..
            })
        ));
        assert_eq!(tree_contents(&project.output()), before);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_output_root_is_never_followed() {
        use std::os::unix::fs::symlink;

        let project = TempProject::new();
        let outside = project.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("sentinel"), b"keep").unwrap();
        fs::create_dir_all(project.path().join("generated")).unwrap();
        symlink(&outside, project.output()).unwrap();
        let set = loader_set(b"loader");
        let target = target();
        let error = PreparedOutput::try_new(&target, &set)
            .unwrap()
            .check(project.path())
            .unwrap_err();
        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::Registration(RegistrationFailureEvidence::Ownership {
                reason: OwnershipFailureReason::UnsafeEntry,
                ..
            })
        ));
        assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"keep");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_owned_artifact_is_never_followed() {
        use std::os::unix::fs::symlink;

        let project = TempProject::new();
        let set = loader_set(b"expected");
        write_managed(&project, &set, true);
        let outside = project.path().join("outside-payload");
        fs::write(&outside, b"keep").unwrap();
        fs::remove_file(project.output().join("loader.mjs")).unwrap();
        symlink(&outside, project.output().join("loader.mjs")).unwrap();
        let target = target();

        let error = PreparedOutput::try_new(&target, &set)
            .unwrap()
            .check(project.path())
            .unwrap_err();
        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::Registration(RegistrationFailureEvidence::Ownership {
                reason: OwnershipFailureReason::UnsafeEntry,
                ..
            })
        ));
        assert_eq!(fs::read(outside).unwrap(), b"keep");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_owned_directories_select_the_first_portable_path() {
        use std::os::unix::fs::symlink;

        let project = TempProject::new();
        let set = module_set(&[(&["a", "x.mjs"], b"x"), (&["b", "y.mjs"], b"y")]);
        write_managed(&project, &set, true);
        let outside = project.path().join("outside-directory");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("sentinel"), b"keep").unwrap();
        fs::remove_dir_all(project.output().join("a")).unwrap();
        fs::remove_dir_all(project.output().join("b")).unwrap();
        // Create in reverse canonical order to prove enumeration is irrelevant.
        symlink(&outside, project.output().join("b")).unwrap();
        symlink(&outside, project.output().join("a")).unwrap();
        let target = target();

        let error = PreparedOutput::try_new(&target, &set)
            .unwrap()
            .check(project.path())
            .unwrap_err();
        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::Registration(RegistrationFailureEvidence::Ownership {
                reason: OwnershipFailureReason::UnsafeEntry,
                relative_path: Some(path),
                ..
            }) if path.segments().collect::<Vec<_>>() == ["a"]
        ));
        assert_eq!(fs::read(outside.join("sentinel")).unwrap(), b"keep");
    }

    #[cfg(all(unix, not(target_vendor = "apple")))]
    #[test]
    fn native_non_utf8_entry_is_unrepresentable_without_lossy_evidence() {
        use std::os::unix::ffi::OsStringExt;

        let project = TempProject::new();
        let empty = ExportArtifactSet::try_new(Vec::new()).unwrap();
        write_managed(&project, &empty, true);
        let invalid_name = std::ffi::OsString::from_vec(vec![0x80]);
        fs::write(project.output().join(invalid_name), b"keep").unwrap();
        let target = target();
        let error = PreparedOutput::try_new(&target, &empty)
            .unwrap()
            .check(project.path())
            .unwrap_err();
        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::Registration(RegistrationFailureEvidence::Ownership {
                reason: OwnershipFailureReason::EntryUnrepresentable,
                relative_path: None,
                ..
            })
        ));
    }

    #[test]
    fn prepared_values_are_immutable_thread_capable_proofs() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<PreparedOutput<'static>>();
    }
}
