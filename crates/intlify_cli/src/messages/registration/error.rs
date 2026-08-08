// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Typed, bounded evidence for destination and output-registration failures.
//!
//! The values in this module preserve the closed machine-facing failure
//! vocabulary without retaining dependency errors, absolute paths, payloads,
//! or lossy host names. Reporter adapters consume these values later; this
//! module itself performs no filesystem access or serialization.

use std::io;

use intlify_export::{
    ExportArtifactFormatVersion, ExportArtifactKind, ExportArtifactPath,
    ExportArtifactRelationshipKind,
};

use super::super::delivery::DeliveryOutputRoot;

const EVIDENCE_PATH_SEGMENTS_LIMIT: usize = 128;
const EVIDENCE_PATH_BYTES_LIMIT: usize = 8_192;

/// Complete private failure from destination preparation or registration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::messages) struct OutputRegistrationError {
    evidence: OutputRegistrationErrorEvidence,
    output_state: OutputState,
}

impl OutputRegistrationError {
    /// Construct an unsupported destination-capability failure.
    pub(super) const fn unsupported_capability(evidence: UnsupportedCapabilityEvidence) -> Self {
        Self {
            evidence: OutputRegistrationErrorEvidence::UnsupportedCapability(evidence),
            output_state: OutputState::Unchanged,
        }
    }

    /// Construct a portable-to-host destination mapping failure.
    pub(super) const fn destination_mapping(evidence: DestinationMappingEvidence) -> Self {
        Self {
            evidence: OutputRegistrationErrorEvidence::DestinationMapping(evidence),
            output_state: OutputState::Unchanged,
        }
    }

    /// Construct a collision between a destination and another owned name.
    pub(super) const fn destination_collision(evidence: DestinationCollisionEvidence) -> Self {
        Self {
            evidence: OutputRegistrationErrorEvidence::DestinationCollision(evidence),
            output_state: OutputState::Unchanged,
        }
    }

    /// Construct an output ownership or snapshot failure.
    pub(super) const fn registration(evidence: RegistrationFailureEvidence) -> Self {
        Self {
            evidence: OutputRegistrationErrorEvidence::Registration(evidence),
            output_state: OutputState::Unchanged,
        }
    }

    /// Construct an impossible output-registration state.
    pub(super) const fn internal_invariant() -> Self {
        Self {
            evidence: OutputRegistrationErrorEvidence::InternalInvariant,
            output_state: OutputState::Unchanged,
        }
    }

    /// Attach the strongest output-root state proved after transaction work.
    pub(super) const fn with_output_state(mut self, output_state: OutputState) -> Self {
        self.output_state = output_state;
        self
    }

    /// Return the public-boundary classification derived from the evidence.
    pub(super) const fn kind(&self) -> OutputRegistrationErrorKind {
        match self.evidence {
            OutputRegistrationErrorEvidence::UnsupportedCapability(_) => {
                OutputRegistrationErrorKind::UnsupportedCapability
            }
            OutputRegistrationErrorEvidence::DestinationMapping(_) => {
                OutputRegistrationErrorKind::DestinationMappingFailed
            }
            OutputRegistrationErrorEvidence::DestinationCollision(_) => {
                OutputRegistrationErrorKind::DestinationCollision
            }
            OutputRegistrationErrorEvidence::Registration(_) => {
                OutputRegistrationErrorKind::RegistrationFailed
            }
            OutputRegistrationErrorEvidence::InternalInvariant => {
                OutputRegistrationErrorKind::InternalInvariant
            }
        }
    }

    /// Return the sole retained checked evidence value.
    pub(super) const fn evidence(&self) -> &OutputRegistrationErrorEvidence {
        &self.evidence
    }

    /// Return the output-root state proved when this failure was selected.
    pub(super) const fn output_state(&self) -> OutputState {
        self.output_state
    }
}

impl core::fmt::Display for OutputRegistrationError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self.kind() {
            OutputRegistrationErrorKind::UnsupportedCapability => {
                "output destination capability is unsupported"
            }
            OutputRegistrationErrorKind::DestinationMappingFailed => {
                "portable output path cannot be mapped to the host destination"
            }
            OutputRegistrationErrorKind::DestinationCollision => {
                "mapped output destination collides with another owned name"
            }
            OutputRegistrationErrorKind::RegistrationFailed => {
                "output ownership or snapshot validation failed"
            }
            OutputRegistrationErrorKind::InternalInvariant => {
                "output registration internal invariant failed"
            }
        })
    }
}

impl std::error::Error for OutputRegistrationError {}

/// Stable boundary kind derived from output-registration evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OutputRegistrationErrorKind {
    UnsupportedCapability,
    DestinationMappingFailed,
    DestinationCollision,
    RegistrationFailed,
    InternalInvariant,
}

/// Sole stored union for output-registration failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum OutputRegistrationErrorEvidence {
    UnsupportedCapability(UnsupportedCapabilityEvidence),
    DestinationMapping(DestinationMappingEvidence),
    DestinationCollision(DestinationCollisionEvidence),
    Registration(RegistrationFailureEvidence),
    InternalInvariant,
}

/// Output-root side effect proved at result construction time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::messages) enum OutputState {
    Unchanged,
    Updated,
    Restored,
    Indeterminate,
}

/// Closed capability evidence admitted before filesystem ownership inspection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum UnsupportedCapabilityEvidence {
    ArtifactKind {
        artifact_path: ExportArtifactPath,
        artifact_kind: ExportArtifactKind,
    },
    ArtifactFormatVersion {
        artifact_path: ExportArtifactPath,
        artifact_kind: ExportArtifactKind,
        format_version: ExportArtifactFormatVersion,
    },
    MediaType {
        artifact_path: ExportArtifactPath,
        artifact_kind: ExportArtifactKind,
    },
    RelationshipKind {
        artifact_path: ExportArtifactPath,
        relationship_kind: ExportArtifactRelationshipKind,
    },
    RelationshipGraph {
        artifact_path: Option<ExportArtifactPath>,
    },
    FilesystemNoFollow,
    ProcessLocking,
    SameFilesystemStaging,
    DurableFlush,
    SafeRename,
    SecureRandom,
    DeterministicRecovery,
}

/// Why one portable artifact path cannot be represented by the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DestinationMappingReason {
    NameUnrepresentable,
    NameReserved,
    PathLimit,
    DestinationUnsupported,
}

/// Checked evidence for one path-mapping failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DestinationMappingEvidence {
    reason: DestinationMappingReason,
    artifact_path: ExportArtifactPath,
    segment_index: Option<u16>,
    mapped_destination: Option<EvidencePath>,
}

impl DestinationMappingEvidence {
    /// Retain the exact failed path and only bounded optional host evidence.
    pub(super) const fn new(
        reason: DestinationMappingReason,
        artifact_path: ExportArtifactPath,
        segment_index: Option<u16>,
        mapped_destination: Option<EvidencePath>,
    ) -> Self {
        Self {
            reason,
            artifact_path,
            segment_index,
            mapped_destination,
        }
    }

    pub(super) const fn reason(&self) -> DestinationMappingReason {
        self.reason
    }

    pub(super) const fn artifact_path(&self) -> &ExportArtifactPath {
        &self.artifact_path
    }

    pub(super) const fn segment_index(&self) -> Option<u16> {
        self.segment_index
    }

    pub(super) const fn mapped_destination(&self) -> Option<&EvidencePath> {
        self.mapped_destination.as_ref()
    }
}

/// Closed collision evidence selected after all artifact paths are mapped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DestinationCollisionEvidence {
    MappedPath {
        first_artifact_path: ExportArtifactPath,
        second_artifact_path: ExportArtifactPath,
        mapped_destination: Option<EvidencePath>,
    },
    HostEquivalentPath {
        first_artifact_path: ExportArtifactPath,
        second_artifact_path: ExportArtifactPath,
        mapped_destination: Option<EvidencePath>,
    },
    ReservedManifest {
        first_artifact_path: ExportArtifactPath,
    },
    OutputRootControlId {
        first_output_root: DeliveryOutputRoot,
        second_output_root: DeliveryOutputRoot,
    },
}

/// Bounded safely representable project-root-relative path evidence.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct EvidencePath(Box<[Box<str>]>);

impl EvidencePath {
    /// Validate grammar and the dedicated evidence-only path ceilings.
    pub(super) fn try_new<I, S>(segments: I) -> Option<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<Box<str>>,
    {
        let mut decoded_bytes = 0_usize;
        let mut checked = Vec::new();
        for segment in segments {
            if checked.len() == EVIDENCE_PATH_SEGMENTS_LIMIT {
                return None;
            }
            let segment = segment.into();
            decoded_bytes = decoded_bytes.checked_add(segment.len())?;
            if decoded_bytes > EVIDENCE_PATH_BYTES_LIMIT || !valid_evidence_segment(&segment) {
                return None;
            }
            checked.push(segment);
        }
        (!checked.is_empty()).then(|| Self(checked.into_boxed_slice()))
    }

    /// Return exact relative path segments without slash joining.
    pub(super) fn segments(&self) -> impl ExactSizeIterator<Item = &str> {
        self.0.iter().map(Box::as_ref)
    }
}

fn valid_evidence_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && !segment.chars().any(|character| {
            matches!(character, '/' | '\\' | '\0')
                || character.is_control()
                || matches!(
                    character,
                    '\u{061C}'
                        | '\u{200E}'..='\u{200F}'
                        | '\u{202A}'..='\u{202E}'
                        | '\u{2066}'..='\u{2069}'
                )
        })
}

/// Ownership-stage reasons supported by the current filesystem reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OwnershipFailureReason {
    RootNotDirectory,
    ManifestMissing,
    ManifestInvalid,
    ManifestUnsupported,
    ManifestLimit,
    UnownedEntry,
    UnsafeEntry,
    EntryUnrepresentable,
}

/// Check-stage reasons for an unreadable or changing filesystem snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CheckFailureReason {
    SnapshotUnreadable,
    SnapshotChanged,
}

/// Lock-stage failures selected before recovery or ownership admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LockFailureReason {
    ControlConflict,
    ControlInvalid,
    AcquireFailed,
    ConcurrentStateChanged,
}

/// Staging-stage failures while constructing the complete new root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StagingFailureReason {
    ControlConflict,
    CreateFailed,
    WriteFailed,
    FlushFailed,
}

/// Commit-stage failures after the complete staging root is durable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommitFailureReason {
    JournalConflict,
    JournalWriteFailed,
    OldRootMoveFailed,
    NewRootInstallFailed,
    CleanupFailed,
    FlushFailed,
}

/// Rollback-stage failures while re-establishing the prior root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RollbackFailureReason {
    OldRootRestore,
    Cleanup,
    Flush,
}

/// Recovery-stage failures from a prior interrupted transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecoveryFailureReason {
    JournalInvalid,
    StateAmbiguous,
    OldRootRestoreFailed,
    CleanupFailed,
    FlushFailed,
}

/// Stable subject selected by a registration failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RegistrationSubject {
    OutputRoot,
    Manifest,
    Lock,
    Journal,
    Staging,
    Backup,
    Artifact,
}

/// Stable control-file identity without retaining a host basename.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RegistrationControl {
    Manifest,
    Lock,
    Journal,
    Staging,
    Backup,
}

/// Closed filesystem operation retained only when an OS operation failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RegistrationOperation {
    Inspect,
    Open,
    Read,
    Create,
    Write,
    Flush,
    Lock,
    Replace,
    Rename,
    Remove,
}

/// Portable classification of an underlying operating-system error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RegistrationIoKind {
    NotFound,
    PermissionDenied,
    NotFile,
    NotDirectory,
    Unknown,
}

/// Bounded operating-system evidence attached to one failed operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RegistrationIoEvidence {
    operation: RegistrationOperation,
    kind: RegistrationIoKind,
    raw_os_error: Option<i32>,
}

impl RegistrationIoEvidence {
    /// Normalize a Rust I/O error without retaining its message or source chain.
    pub(super) fn from_error(operation: RegistrationOperation, error: &io::Error) -> Self {
        Self {
            operation,
            kind: match error.kind() {
                io::ErrorKind::NotFound => RegistrationIoKind::NotFound,
                io::ErrorKind::PermissionDenied => RegistrationIoKind::PermissionDenied,
                io::ErrorKind::NotADirectory => RegistrationIoKind::NotDirectory,
                io::ErrorKind::IsADirectory => RegistrationIoKind::NotFile,
                _ => RegistrationIoKind::Unknown,
            },
            raw_os_error: error.raw_os_error(),
        }
    }

    pub(super) const fn operation(self) -> RegistrationOperation {
        self.operation
    }

    pub(super) const fn kind(self) -> RegistrationIoKind {
        self.kind
    }

    pub(super) const fn raw_os_error(self) -> Option<i32> {
        self.raw_os_error
    }
}

/// One closed ownership or check failure record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum RegistrationFailureEvidence {
    Ownership {
        reason: OwnershipFailureReason,
        subject: RegistrationSubject,
        control: Option<RegistrationControl>,
        artifact_path: Option<ExportArtifactPath>,
        relative_path: Option<EvidencePath>,
        io: Option<RegistrationIoEvidence>,
    },
    Check {
        reason: CheckFailureReason,
        subject: RegistrationSubject,
        artifact_path: Option<ExportArtifactPath>,
        relative_path: Option<EvidencePath>,
        io: Option<RegistrationIoEvidence>,
    },
    Lock {
        reason: LockFailureReason,
        subject: RegistrationSubject,
        control: RegistrationControl,
        io: Option<RegistrationIoEvidence>,
    },
    Staging {
        reason: StagingFailureReason,
        subject: RegistrationSubject,
        control: Option<RegistrationControl>,
        artifact_path: Option<ExportArtifactPath>,
        relative_path: Option<EvidencePath>,
        io: Option<RegistrationIoEvidence>,
    },
    Commit {
        reason: CommitFailureReason,
        subject: RegistrationSubject,
        control: Option<RegistrationControl>,
        io: Option<RegistrationIoEvidence>,
    },
    Rollback {
        reason: RollbackFailureReason,
        subject: RegistrationSubject,
        control: Option<RegistrationControl>,
        io: Option<RegistrationIoEvidence>,
    },
    Recovery {
        reason: RecoveryFailureReason,
        subject: RegistrationSubject,
        control: Option<RegistrationControl>,
        io: Option<RegistrationIoEvidence>,
    },
}

impl RegistrationFailureEvidence {
    pub(super) const fn root_not_directory(relative_path: Option<EvidencePath>) -> Self {
        Self::Ownership {
            reason: OwnershipFailureReason::RootNotDirectory,
            subject: RegistrationSubject::OutputRoot,
            control: None,
            artifact_path: None,
            relative_path,
            io: None,
        }
    }

    pub(super) const fn manifest(
        reason: OwnershipFailureReason,
        io: Option<RegistrationIoEvidence>,
    ) -> Self {
        Self::Ownership {
            reason,
            subject: RegistrationSubject::Manifest,
            control: Some(RegistrationControl::Manifest),
            artifact_path: None,
            relative_path: None,
            io,
        }
    }

    pub(super) const fn unowned_entry(relative_path: EvidencePath) -> Self {
        Self::Ownership {
            reason: OwnershipFailureReason::UnownedEntry,
            subject: RegistrationSubject::OutputRoot,
            control: None,
            artifact_path: None,
            relative_path: Some(relative_path),
            io: None,
        }
    }

    pub(super) const fn unsafe_entry(
        subject: RegistrationSubject,
        artifact_path: Option<ExportArtifactPath>,
        relative_path: Option<EvidencePath>,
    ) -> Self {
        Self::Ownership {
            reason: OwnershipFailureReason::UnsafeEntry,
            subject,
            control: None,
            artifact_path,
            relative_path,
            io: None,
        }
    }

    pub(super) const fn entry_unrepresentable() -> Self {
        Self::Ownership {
            reason: OwnershipFailureReason::EntryUnrepresentable,
            subject: RegistrationSubject::OutputRoot,
            control: None,
            artifact_path: None,
            relative_path: None,
            io: None,
        }
    }

    pub(super) const fn snapshot_unreadable(
        subject: RegistrationSubject,
        artifact_path: Option<ExportArtifactPath>,
        relative_path: Option<EvidencePath>,
        io: RegistrationIoEvidence,
    ) -> Self {
        Self::Check {
            reason: CheckFailureReason::SnapshotUnreadable,
            subject,
            artifact_path,
            relative_path,
            io: Some(io),
        }
    }

    pub(super) const fn snapshot_changed(
        subject: RegistrationSubject,
        artifact_path: Option<ExportArtifactPath>,
        relative_path: Option<EvidencePath>,
    ) -> Self {
        Self::Check {
            reason: CheckFailureReason::SnapshotChanged,
            subject,
            artifact_path,
            relative_path,
            io: None,
        }
    }

    pub(super) const fn lock(
        reason: LockFailureReason,
        io: Option<RegistrationIoEvidence>,
    ) -> Self {
        Self::Lock {
            reason,
            subject: RegistrationSubject::Lock,
            control: RegistrationControl::Lock,
            io,
        }
    }

    pub(super) const fn staging(
        reason: StagingFailureReason,
        subject: RegistrationSubject,
        control: Option<RegistrationControl>,
        artifact_path: Option<ExportArtifactPath>,
        relative_path: Option<EvidencePath>,
        io: Option<RegistrationIoEvidence>,
    ) -> Self {
        Self::Staging {
            reason,
            subject,
            control,
            artifact_path,
            relative_path,
            io,
        }
    }

    pub(super) const fn commit(
        reason: CommitFailureReason,
        subject: RegistrationSubject,
        control: Option<RegistrationControl>,
        io: Option<RegistrationIoEvidence>,
    ) -> Self {
        Self::Commit {
            reason,
            subject,
            control,
            io,
        }
    }

    pub(super) const fn rollback(
        reason: RollbackFailureReason,
        subject: RegistrationSubject,
        control: Option<RegistrationControl>,
        io: Option<RegistrationIoEvidence>,
    ) -> Self {
        Self::Rollback {
            reason,
            subject,
            control,
            io,
        }
    }

    pub(super) const fn recovery(
        reason: RecoveryFailureReason,
        subject: RegistrationSubject,
        control: Option<RegistrationControl>,
        io: Option<RegistrationIoEvidence>,
    ) -> Self {
        Self::Recovery {
            reason,
            subject,
            control,
            io,
        }
    }
}

#[cfg(test)]
mod tests {
    use intlify_export::ExportArtifactPath;

    use super::{
        DestinationCollisionEvidence, DestinationMappingEvidence, DestinationMappingReason,
        EvidencePath, OutputRegistrationError, OutputRegistrationErrorEvidence,
        OutputRegistrationErrorKind, RegistrationIoEvidence, RegistrationIoKind,
        RegistrationOperation,
    };

    #[test]
    fn evidence_path_enforces_grammar_and_both_bounds() {
        assert_eq!(
            EvidencePath::try_new(["dist", "messages"])
                .unwrap()
                .segments()
                .collect::<Vec<_>>(),
            ["dist", "messages"]
        );
        assert!(EvidencePath::try_new(["."]).is_none());
        assert!(EvidencePath::try_new(["line\nfeed"]).is_none());
        assert!(EvidencePath::try_new((0..129).map(|_| "x")).is_none());
        assert!(EvidencePath::try_new(["x".repeat(8_193)]).is_none());
    }

    #[test]
    fn io_evidence_retains_only_normalized_fields() {
        let error = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let evidence = RegistrationIoEvidence::from_error(RegistrationOperation::Read, &error);
        assert_eq!(evidence.operation(), RegistrationOperation::Read);
        assert_eq!(evidence.kind(), RegistrationIoKind::PermissionDenied);
        assert_eq!(evidence.raw_os_error(), None);
    }

    #[test]
    fn every_destination_mapping_reason_retains_the_same_bounded_shape() {
        let path = ExportArtifactPath::try_new(["locale.mjs"]).unwrap();
        for reason in [
            DestinationMappingReason::NameUnrepresentable,
            DestinationMappingReason::NameReserved,
            DestinationMappingReason::PathLimit,
            DestinationMappingReason::DestinationUnsupported,
        ] {
            let error =
                OutputRegistrationError::destination_mapping(DestinationMappingEvidence::new(
                    reason,
                    path.clone(),
                    Some(0),
                    EvidencePath::try_new(["dist", "locale.mjs"]),
                ));
            assert_eq!(
                error.kind(),
                OutputRegistrationErrorKind::DestinationMappingFailed
            );
            assert!(matches!(
                error.evidence(),
                OutputRegistrationErrorEvidence::DestinationMapping(evidence)
                    if evidence.reason() == reason
                        && evidence.artifact_path() == &path
                        && evidence.segment_index() == Some(0)
                        && evidence.mapped_destination().is_some()
            ));
        }
    }

    #[test]
    fn artifact_collision_evidence_keeps_both_canonical_logical_paths() {
        let first = ExportArtifactPath::try_new(["A.mjs"]).unwrap();
        let second = ExportArtifactPath::try_new(["a.mjs"]).unwrap();
        let mapped = EvidencePath::try_new(["dist", "a.mjs"]);
        for evidence in [
            DestinationCollisionEvidence::MappedPath {
                first_artifact_path: first.clone(),
                second_artifact_path: second.clone(),
                mapped_destination: mapped.clone(),
            },
            DestinationCollisionEvidence::HostEquivalentPath {
                first_artifact_path: first.clone(),
                second_artifact_path: second.clone(),
                mapped_destination: mapped.clone(),
            },
            DestinationCollisionEvidence::ReservedManifest {
                first_artifact_path: first.clone(),
            },
        ] {
            let error = OutputRegistrationError::destination_collision(evidence);
            assert_eq!(
                error.kind(),
                OutputRegistrationErrorKind::DestinationCollision
            );
            match error.evidence() {
                OutputRegistrationErrorEvidence::DestinationCollision(
                    DestinationCollisionEvidence::MappedPath {
                        first_artifact_path,
                        second_artifact_path,
                        mapped_destination,
                    }
                    | DestinationCollisionEvidence::HostEquivalentPath {
                        first_artifact_path,
                        second_artifact_path,
                        mapped_destination,
                    },
                ) => {
                    assert_eq!(first_artifact_path, &first);
                    assert_eq!(second_artifact_path, &second);
                    assert_eq!(mapped_destination, &mapped);
                }
                OutputRegistrationErrorEvidence::DestinationCollision(
                    DestinationCollisionEvidence::ReservedManifest {
                        first_artifact_path,
                    },
                ) => assert_eq!(first_artifact_path, &first),
                other => panic!("unexpected evidence: {other:?}"),
            }
        }
    }
}
