// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Durable, target-local filesystem registration and recovery.
//!
//! This module holds one advisory lock while it inspects or mutates an output
//! root. Write mode constructs a complete same-parent staging tree, publishes
//! a canonical journal around every rename, and either installs the full tree
//! or proves the prior tree was restored. Check mode takes the same shared lock
//! when it exists and never creates control state.

use std::collections::BTreeSet;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, Metadata, OpenOptions};
use intlify_export::ExportArtifactPath;

use super::control::{
    ControlCodecErrorKind, JournalOldRoot, JournalPhase, JournalRecord, LockRecord,
    OutputRootControlId, TransactionId, CONTROL_WIRE_LIMIT,
};
use super::difference::CheckComparison;
use super::error::{
    CommitFailureReason, DestinationCollisionEvidence, EvidencePath, LockFailureReason,
    OutputRegistrationError, OutputRegistrationErrorKind, OutputState, RecoveryFailureReason,
    RegistrationControl, RegistrationFailureEvidence, RegistrationIoEvidence,
    RegistrationOperation, RegistrationSubject, RollbackFailureReason, StagingFailureReason,
    UnsupportedCapabilityEvidence,
};
use super::inspection::{self, PriorOutputState, RecoveryRootObservation};
use super::manifest::OUTPUT_MANIFEST_BASENAME;
use super::PreparedOutput;
use crate::messages::delivery::DeliveryOutputRoot;

/// Write result consumed by the later command-level result adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::messages) struct WriteRegistrationOutcome {
    status: WriteRegistrationStatus,
    output_state: OutputState,
}

impl WriteRegistrationOutcome {
    pub(in crate::messages) const fn status(self) -> WriteRegistrationStatus {
        self.status
    }

    pub(in crate::messages) const fn output_state(self) -> OutputState {
        self.output_state
    }
}

/// Whether write mode skipped or completed one durable transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::messages) enum WriteRegistrationStatus {
    Unchanged,
    Written,
}

/// Deterministic interception points for durability and publication tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FaultPoint {
    ParentDirectoryFlush,
    LockAcquire,
    LockFileFlush,
    LockDirectoryFlush,
    RecoveryRename,
    RecoveryCleanup,
    RecoveryDirectoryFlush,
    StagingFileFlush,
    StagingDirectoryFlush,
    StagingParentFlush,
    StagingCleanup,
    StagingCleanupFlush,
    JournalCreate(JournalPhase),
    JournalFileFlush(JournalPhase),
    JournalReplace(JournalPhase),
    JournalDirectoryFlush(JournalPhase),
    OldRootRename,
    OldRootDirectoryFlush,
    NewRootRename,
    NewRootDirectoryFlush,
    BackupCleanup,
    BackupDirectoryFlush,
    JournalCleanup,
    JournalCleanupFlush,
    RollbackRename,
    RollbackCleanup,
    RollbackDirectoryFlush,
}

trait TransactionHooks {
    fn before(&self, _point: FaultPoint) -> io::Result<()> {
        Ok(())
    }

    fn sync_directory(&self, directory: &Dir) -> io::Result<()> {
        sync_directory(directory)
    }
}

struct NoTransactionHooks;

impl TransactionHooks for NoTransactionHooks {}

pub(super) fn check_output(
    prepared: &PreparedOutput<'_>,
    project_root: &Path,
) -> Result<CheckComparison, OutputRegistrationError> {
    let control_id = OutputRootControlId::derive(prepared.output_root)
        .map_err(|_| OutputRegistrationError::internal_invariant())?;
    let parent = open_output_parent(
        project_root,
        prepared.output_root,
        ParentMode::Check,
        &NoTransactionHooks,
    )?;
    let lock_basename = control_id.lock_basename();
    let held_lock = match parent.as_ref() {
        Some(parent) => acquire_existing_lock(
            parent,
            &lock_basename,
            prepared.output_root,
            LockMode::Shared,
            &NoTransactionHooks,
        )?,
        None => None,
    };

    let inspection = match parent.as_ref() {
        Some(parent) => inspection::inspect_named_output(
            &parent.dir,
            &parent.output_basename,
            prepared.output_root,
            prepared.exporter,
            prepared.artifacts,
            prepared.manifest(),
        ),
        None => inspection::inspect_output(
            project_root,
            prepared.output_root,
            prepared.exporter,
            prepared.artifacts,
            prepared.manifest(),
        ),
    }?;

    match (&parent, &held_lock) {
        (Some(parent), Some(lock)) => verify_lock_identity(parent, &lock_basename, lock.identity)?,
        (Some(parent), None) => verify_lock_absent(parent, &lock_basename)?,
        (None, None) => {
            if let Some(parent) = open_output_parent(
                project_root,
                prepared.output_root,
                ParentMode::Check,
                &NoTransactionHooks,
            )? {
                verify_lock_absent(&parent, &lock_basename)?;
            }
        }
        (None, Some(_)) => return Err(OutputRegistrationError::internal_invariant()),
    }
    if let Some(parent) = &parent {
        verify_output_parent(project_root, prepared.output_root, parent)?;
    }

    Ok(inspection.into_comparison())
}

pub(super) fn write_output(
    prepared: &PreparedOutput<'_>,
    project_root: &Path,
) -> Result<WriteRegistrationOutcome, OutputRegistrationError> {
    write_output_with_transaction_id_source_and_hooks(
        prepared,
        project_root,
        || {
            TransactionId::generate().map_err(|_| {
                OutputRegistrationError::unsupported_capability(
                    UnsupportedCapabilityEvidence::SecureRandom,
                )
            })
        },
        &NoTransactionHooks,
    )
}

fn write_output_with_transaction_id_source(
    prepared: &PreparedOutput<'_>,
    project_root: &Path,
    transaction_id_source: impl FnOnce() -> Result<TransactionId, OutputRegistrationError>,
) -> Result<WriteRegistrationOutcome, OutputRegistrationError> {
    write_output_with_transaction_id_source_and_hooks(
        prepared,
        project_root,
        transaction_id_source,
        &NoTransactionHooks,
    )
}

fn write_output_with_transaction_id_source_and_hooks(
    prepared: &PreparedOutput<'_>,
    project_root: &Path,
    transaction_id_source: impl FnOnce() -> Result<TransactionId, OutputRegistrationError>,
    hooks: &dyn TransactionHooks,
) -> Result<WriteRegistrationOutcome, OutputRegistrationError> {
    let control_id = OutputRootControlId::derive(prepared.output_root)
        .map_err(|_| OutputRegistrationError::internal_invariant())?;
    let parent = open_output_parent(project_root, prepared.output_root, ParentMode::Write, hooks)?
        .ok_or_else(OutputRegistrationError::internal_invariant)?;
    let lock_basename = control_id.lock_basename();
    let _lock = acquire_write_lock(&parent, &lock_basename, prepared.output_root, hooks)?;
    verify_output_parent(project_root, prepared.output_root, &parent)?;

    let recovery_effect = recover_prior_transaction(prepared, &parent, control_id, hooks)?;
    let inspection = inspection::inspect_named_output(
        &parent.dir,
        &parent.output_basename,
        prepared.output_root,
        prepared.exporter,
        prepared.artifacts,
        prepared.manifest(),
    )
    .map_err(|error| error.with_output_state(recovery_effect.output_state()))?;
    verify_output_parent(project_root, prepared.output_root, &parent)
        .map_err(|error| error.with_output_state(OutputState::Indeterminate))?;
    if inspection.comparison().is_matched() {
        return Ok(WriteRegistrationOutcome {
            status: WriteRegistrationStatus::Unchanged,
            output_state: recovery_effect.output_state(),
        });
    }

    // Randomness is intentionally requested only after recovery and equality
    // inspection prove that a real transaction is required.
    let transaction_id = transaction_id_source()
        .map_err(|error| error.with_output_state(recovery_effect.output_state()))?;
    verify_output_parent(project_root, prepared.output_root, &parent)
        .map_err(|error| error.with_output_state(OutputState::Indeterminate))?;
    let old_root = journal_old_root(inspection.state());
    let journal = JournalRecord::new(
        control_id,
        transaction_id,
        prepared.output_root.clone(),
        old_root,
        prepared.manifest().canonical_fingerprint(),
        JournalPhase::Staged,
    )
    .map_err(|_| OutputRegistrationError::internal_invariant())?;

    preflight_transaction_names(&parent, control_id, &journal)
        .map_err(|error| error.with_output_state(recovery_effect.output_state()))?;
    if let Err(failure) = construct_staging(prepared, &parent, &journal, hooks) {
        let output_state = recovery_effect.output_state();
        if !failure.owns_staging {
            return Err(failure.error.with_output_state(output_state));
        }
        return match cleanup_failed_staging(&parent, journal.staging(), hooks) {
            Ok(()) => Err(failure.error.with_output_state(output_state)),
            Err(cleanup_error) => Err(cleanup_error.with_output_state(output_state)),
        };
    }

    match commit_staging(&parent, &journal, hooks) {
        Ok(()) => {
            verify_output_parent(project_root, prepared.output_root, &parent)
                .map_err(|error| error.with_output_state(OutputState::Indeterminate))?;
            Ok(WriteRegistrationOutcome {
                status: WriteRegistrationStatus::Written,
                output_state: OutputState::Updated,
            })
        }
        Err(failure) => {
            let error = rollback_after_commit(&parent, &journal, failure, hooks);
            match verify_output_parent(project_root, prepared.output_root, &parent) {
                Ok(()) => Err(error),
                Err(parent_error) => {
                    Err(parent_error.with_output_state(OutputState::Indeterminate))
                }
            }
        }
    }
}

fn journal_old_root(state: PriorOutputState) -> JournalOldRoot {
    match state {
        PriorOutputState::Absent => JournalOldRoot::Absent,
        PriorOutputState::Empty => JournalOldRoot::Empty,
        PriorOutputState::Managed {
            manifest_fingerprint,
        } => JournalOldRoot::Managed {
            manifest_fingerprint,
        },
    }
}

#[derive(Debug, Clone, Copy)]
enum ParentMode {
    Check,
    Write,
}

struct OutputParent {
    dir: Dir,
    output_basename: Box<str>,
    identity: FileIdentity,
}

fn open_output_parent(
    project_root: &Path,
    output_root: &DeliveryOutputRoot,
    mode: ParentMode,
    hooks: &dyn TransactionHooks,
) -> Result<Option<OutputParent>, OutputRegistrationError> {
    let mut current = Dir::open_ambient_dir(project_root, cap_std::ambient_authority())
        .map_err(|error| parent_io_error(mode, RegistrationOperation::Open, &error, None))?;
    let segments = output_root.segments().collect::<Vec<_>>();
    let Some((output_basename, parents)) = segments.split_last() else {
        return Err(OutputRegistrationError::internal_invariant());
    };
    let mut relative = Vec::<Box<str>>::new();
    for segment in parents {
        relative.push((*segment).into());
        let evidence = EvidencePath::try_new(relative.iter().cloned());
        let metadata = match current.symlink_metadata(segment) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if matches!(mode, ParentMode::Check) {
                    return Ok(None);
                }
                current.create_dir(segment).map_err(|error| {
                    staging_io_error(
                        StagingFailureReason::CreateFailed,
                        RegistrationSubject::OutputRoot,
                        None,
                        None,
                        evidence.clone(),
                        RegistrationOperation::Create,
                        &error,
                    )
                })?;
                sync_directory_at(&current, hooks, FaultPoint::ParentDirectoryFlush).map_err(
                    |error| {
                        staging_io_error(
                            StagingFailureReason::FlushFailed,
                            RegistrationSubject::OutputRoot,
                            None,
                            None,
                            evidence.clone(),
                            RegistrationOperation::Flush,
                            &error,
                        )
                    },
                )?;
                current.symlink_metadata(segment).map_err(|error| {
                    staging_io_error(
                        StagingFailureReason::CreateFailed,
                        RegistrationSubject::OutputRoot,
                        None,
                        None,
                        evidence.clone(),
                        RegistrationOperation::Inspect,
                        &error,
                    )
                })?
            }
            Err(error) => {
                return Err(parent_io_error(
                    mode,
                    RegistrationOperation::Inspect,
                    &error,
                    evidence,
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
            parent_io_error(
                mode,
                RegistrationOperation::Open,
                &error,
                EvidencePath::try_new(relative.iter().cloned()),
            )
        })?;
    }
    let identity = file_identity(&current.dir_metadata().map_err(|error| {
        parent_io_error(
            mode,
            RegistrationOperation::Inspect,
            &error,
            EvidencePath::try_new(relative.iter().cloned()),
        )
    })?)?;
    Ok(Some(OutputParent {
        dir: current,
        output_basename: (*output_basename).into(),
        identity,
    }))
}

fn verify_output_parent(
    project_root: &Path,
    output_root: &DeliveryOutputRoot,
    expected: &OutputParent,
) -> Result<(), OutputRegistrationError> {
    let Some(current) = open_output_parent(
        project_root,
        output_root,
        ParentMode::Check,
        &NoTransactionHooks,
    )?
    else {
        return Err(output_parent_changed());
    };
    if current.identity != expected.identity {
        return Err(output_parent_changed());
    }
    Ok(())
}

fn output_parent_changed() -> OutputRegistrationError {
    OutputRegistrationError::registration(RegistrationFailureEvidence::snapshot_changed(
        RegistrationSubject::OutputRoot,
        None,
        None,
    ))
}

fn parent_io_error(
    mode: ParentMode,
    operation: RegistrationOperation,
    error: &io::Error,
    relative_path: Option<EvidencePath>,
) -> OutputRegistrationError {
    match mode {
        ParentMode::Check => {
            OutputRegistrationError::registration(RegistrationFailureEvidence::snapshot_unreadable(
                RegistrationSubject::OutputRoot,
                None,
                relative_path,
                RegistrationIoEvidence::from_error(operation, error),
            ))
        }
        ParentMode::Write => staging_io_error(
            StagingFailureReason::CreateFailed,
            RegistrationSubject::OutputRoot,
            None,
            None,
            relative_path,
            operation,
            error,
        ),
    }
}

#[derive(Debug, Clone, Copy)]
enum LockMode {
    Shared,
    Exclusive,
}

struct HeldLock {
    _guard: std::fs::File,
    identity: FileIdentity,
}

fn acquire_write_lock(
    parent: &OutputParent,
    basename: &str,
    output_root: &DeliveryOutputRoot,
    hooks: &dyn TransactionHooks,
) -> Result<HeldLock, OutputRegistrationError> {
    loop {
        match acquire_existing_lock(parent, basename, output_root, LockMode::Exclusive, hooks)? {
            Some(lock) => return Ok(lock),
            None => match create_and_initialize_lock(parent, basename, output_root, hooks) {
                Ok(lock) => return Ok(lock),
                Err(CreateLockError::Raced) => {}
                Err(CreateLockError::Registration(error)) => return Err(error),
            },
        }
    }
}

fn acquire_existing_lock(
    parent: &OutputParent,
    basename: &str,
    output_root: &DeliveryOutputRoot,
    mode: LockMode,
    hooks: &dyn TransactionHooks,
) -> Result<Option<HeldLock>, OutputRegistrationError> {
    let metadata = match parent.dir.symlink_metadata(basename) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(lock_io_error(
                LockFailureReason::AcquireFailed,
                RegistrationOperation::Inspect,
                &error,
            ))
        }
    };
    if metadata.is_symlink() || !metadata.is_file() {
        return Err(lock_error(LockFailureReason::ControlConflict, None));
    }
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    if matches!(mode, LockMode::Exclusive) {
        options.write(true);
    }
    let mut file = parent.dir.open_with(basename, &options).map_err(|error| {
        lock_io_error(
            LockFailureReason::AcquireFailed,
            RegistrationOperation::Open,
            &error,
        )
    })?;
    let identity = file_identity(&file.metadata().map_err(|error| {
        lock_io_error(
            LockFailureReason::AcquireFailed,
            RegistrationOperation::Inspect,
            &error,
        )
    })?)?;
    let guard = file
        .try_clone()
        .map(cap_std::fs::File::into_std)
        .map_err(|error| {
            lock_io_error(
                LockFailureReason::AcquireFailed,
                RegistrationOperation::Open,
                &error,
            )
        })?;
    acquire_os_lock_at(&guard, mode, hooks)?;
    let bytes = read_control_file(&mut file).map_err(|error| {
        lock_io_error(
            LockFailureReason::ControlInvalid,
            RegistrationOperation::Read,
            &error,
        )
    })?;
    let record = LockRecord::decode(&bytes).map_err(map_lock_codec_error)?;
    if record.output_root() != output_root {
        return Err(OutputRegistrationError::destination_collision(
            DestinationCollisionEvidence::OutputRootControlId {
                first_output_root: output_root.clone(),
                second_output_root: record.output_root().clone(),
            },
        ));
    }
    verify_lock_identity(parent, basename, identity)?;
    Ok(Some(HeldLock {
        _guard: guard,
        identity,
    }))
}

enum CreateLockError {
    Raced,
    Registration(OutputRegistrationError),
}

fn create_and_initialize_lock(
    parent: &OutputParent,
    basename: &str,
    output_root: &DeliveryOutputRoot,
    hooks: &dyn TransactionHooks,
) -> Result<HeldLock, CreateLockError> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create_new(true)
        .follow(FollowSymlinks::No);
    let mut file = match parent.dir.open_with(basename, &options) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Err(CreateLockError::Raced);
        }
        Err(error) => {
            return Err(CreateLockError::Registration(lock_io_error(
                LockFailureReason::AcquireFailed,
                RegistrationOperation::Create,
                &error,
            )));
        }
    };
    let identity = file_identity(&file.metadata().map_err(|error| {
        CreateLockError::Registration(lock_io_error(
            LockFailureReason::AcquireFailed,
            RegistrationOperation::Inspect,
            &error,
        ))
    })?)
    .map_err(CreateLockError::Registration)?;
    let guard = file
        .try_clone()
        .map(cap_std::fs::File::into_std)
        .map_err(|error| {
            CreateLockError::Registration(lock_io_error(
                LockFailureReason::AcquireFailed,
                RegistrationOperation::Open,
                &error,
            ))
        })?;
    acquire_os_lock_at(&guard, LockMode::Exclusive, hooks)
        .map_err(CreateLockError::Registration)?;
    let record = LockRecord::new(output_root.clone()).map_err(|_| {
        CreateLockError::Registration(OutputRegistrationError::internal_invariant())
    })?;
    file.set_len(0).map_err(|error| {
        CreateLockError::Registration(lock_io_error(
            LockFailureReason::ControlInvalid,
            RegistrationOperation::Write,
            &error,
        ))
    })?;
    file.seek(SeekFrom::Start(0)).map_err(|error| {
        CreateLockError::Registration(lock_io_error(
            LockFailureReason::ControlInvalid,
            RegistrationOperation::Write,
            &error,
        ))
    })?;
    file.write_all(record.canonical_bytes()).map_err(|error| {
        CreateLockError::Registration(lock_io_error(
            LockFailureReason::ControlInvalid,
            RegistrationOperation::Write,
            &error,
        ))
    })?;
    sync_file_at(&file, hooks, FaultPoint::LockFileFlush).map_err(|error| {
        CreateLockError::Registration(lock_io_error(
            LockFailureReason::ControlInvalid,
            RegistrationOperation::Flush,
            &error,
        ))
    })?;
    sync_directory_at(&parent.dir, hooks, FaultPoint::LockDirectoryFlush).map_err(|error| {
        CreateLockError::Registration(lock_io_error(
            LockFailureReason::ControlInvalid,
            RegistrationOperation::Flush,
            &error,
        ))
    })?;
    Ok(HeldLock {
        _guard: guard,
        identity,
    })
}

fn acquire_os_lock(file: &std::fs::File, mode: LockMode) -> Result<(), OutputRegistrationError> {
    let result = match mode {
        LockMode::Shared => file.lock_shared(),
        LockMode::Exclusive => file.lock(),
    };
    result.map_err(|error| {
        if error.kind() == io::ErrorKind::Unsupported {
            OutputRegistrationError::unsupported_capability(
                UnsupportedCapabilityEvidence::ProcessLocking,
            )
        } else {
            lock_io_error(
                LockFailureReason::AcquireFailed,
                RegistrationOperation::Lock,
                &error,
            )
        }
    })
}

fn acquire_os_lock_at(
    file: &std::fs::File,
    mode: LockMode,
    hooks: &dyn TransactionHooks,
) -> Result<(), OutputRegistrationError> {
    hooks.before(FaultPoint::LockAcquire).map_err(|error| {
        if error.kind() == io::ErrorKind::Unsupported {
            OutputRegistrationError::unsupported_capability(
                UnsupportedCapabilityEvidence::ProcessLocking,
            )
        } else {
            lock_io_error(
                LockFailureReason::AcquireFailed,
                RegistrationOperation::Lock,
                &error,
            )
        }
    })?;
    acquire_os_lock(file, mode)
}

fn read_control_file(file: &mut cap_std::fs::File) -> io::Result<Box<[u8]>> {
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    file.take((CONTROL_WIRE_LIMIT as u64) + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > CONTROL_WIRE_LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "persistent control record exceeds its wire limit",
        ));
    }
    Ok(bytes.into_boxed_slice())
}

fn verify_lock_absent(
    parent: &OutputParent,
    basename: &str,
) -> Result<(), OutputRegistrationError> {
    match parent.dir.symlink_metadata(basename) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(OutputRegistrationError::registration(
            RegistrationFailureEvidence::snapshot_changed(RegistrationSubject::Lock, None, None),
        )),
        Err(error) => Err(OutputRegistrationError::registration(
            RegistrationFailureEvidence::snapshot_unreadable(
                RegistrationSubject::Lock,
                None,
                None,
                RegistrationIoEvidence::from_error(RegistrationOperation::Inspect, &error),
            ),
        )),
    }
}

fn verify_lock_identity(
    parent: &OutputParent,
    basename: &str,
    identity: FileIdentity,
) -> Result<(), OutputRegistrationError> {
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let file = parent.dir.open_with(basename, &options).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            lock_error(LockFailureReason::ConcurrentStateChanged, None)
        } else {
            lock_io_error(
                LockFailureReason::ConcurrentStateChanged,
                RegistrationOperation::Open,
                &error,
            )
        }
    })?;
    let current = file_identity(&file.metadata().map_err(|error| {
        lock_io_error(
            LockFailureReason::ConcurrentStateChanged,
            RegistrationOperation::Inspect,
            &error,
        )
    })?)?;
    if current != identity {
        return Err(lock_error(LockFailureReason::ConcurrentStateChanged, None));
    }
    Ok(())
}

fn map_lock_codec_error(error: super::control::ControlCodecError) -> OutputRegistrationError {
    match error.kind() {
        ControlCodecErrorKind::Invalid
        | ControlCodecErrorKind::Unsupported
        | ControlCodecErrorKind::Limit => lock_error(LockFailureReason::ControlInvalid, None),
        ControlCodecErrorKind::InternalInvariant => OutputRegistrationError::internal_invariant(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    first: u64,
    second: u64,
}

#[cfg(unix)]
#[allow(clippy::unnecessary_wraps)]
fn file_identity(metadata: &Metadata) -> Result<FileIdentity, OutputRegistrationError> {
    use cap_std::fs::MetadataExt;

    Ok(FileIdentity {
        first: metadata.dev(),
        second: metadata.ino(),
    })
}

#[cfg(windows)]
#[allow(clippy::unnecessary_wraps)]
fn file_identity(metadata: &Metadata) -> Result<FileIdentity, OutputRegistrationError> {
    use cap_fs_ext::MetadataExt;

    Ok(FileIdentity {
        first: metadata.dev(),
        second: metadata.ino(),
    })
}

#[cfg(not(any(unix, windows)))]
fn file_identity(_metadata: &Metadata) -> Result<FileIdentity, OutputRegistrationError> {
    Err(OutputRegistrationError::unsupported_capability(
        UnsupportedCapabilityEvidence::FilesystemNoFollow,
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryEffect {
    None,
    Restored,
    Updated,
}

impl RecoveryEffect {
    const fn output_state(self) -> OutputState {
        match self {
            Self::None => OutputState::Unchanged,
            Self::Restored => OutputState::Restored,
            Self::Updated => OutputState::Updated,
        }
    }
}

fn recover_prior_transaction(
    prepared: &PreparedOutput<'_>,
    parent: &OutputParent,
    control_id: OutputRootControlId,
    hooks: &dyn TransactionHooks,
) -> Result<RecoveryEffect, OutputRegistrationError> {
    let journal_basename = control_id.journal_basename();
    let Some(held_journal) = read_journal(parent, &journal_basename, control_id)? else {
        return Ok(RecoveryEffect::None);
    };
    let journal = held_journal.record;
    if journal.output_root() != prepared.output_root {
        return Err(recovery_error(
            RecoveryFailureReason::JournalInvalid,
            RegistrationSubject::Journal,
            Some(RegistrationControl::Journal),
            None,
        ));
    }

    let output = recovery_observation(
        parent,
        &parent.output_basename,
        prepared.output_root,
        prepared.exporter,
    )?;
    let staging = recovery_observation(
        parent,
        journal.staging(),
        prepared.output_root,
        prepared.exporter,
    )?;
    let backup = recovery_observation(
        parent,
        journal.backup(),
        prepared.output_root,
        prepared.exporter,
    )?;

    let output_is_old = matches_old_root(output, journal.old_root());
    let backup_is_old = matches_old_root(backup, journal.old_root());
    let output_is_new = matches_manifest(output, journal.new_manifest_fingerprint());
    let staging_is_new = matches_manifest(staging, journal.new_manifest_fingerprint());
    let backup_not_required = matches!(journal.old_root(), JournalOldRoot::Absent);
    let backup_is_missing = matches!(backup, RecoveryRootObservation::Missing);
    let staging_is_missing = matches!(staging, RecoveryRootObservation::Missing);

    verify_recovery_journal(
        parent,
        &journal_basename,
        held_journal.identity,
        journal.canonical_bytes(),
        OutputState::Unchanged,
    )?;

    if output_is_old && (staging_is_new || staging_is_missing) && backup_is_missing {
        if staging_is_new {
            remove_recovery_directory(
                parent,
                journal.staging(),
                RegistrationSubject::Staging,
                RegistrationControl::Staging,
                OutputState::Restored,
                hooks,
            )?;
        }
        remove_journal_controls(
            parent,
            control_id,
            &journal,
            held_journal.identity,
            OutputState::Restored,
            hooks,
        )?;
        return Ok(RecoveryEffect::Restored);
    }

    if matches!(output, RecoveryRootObservation::Missing)
        && backup_is_old
        && (staging_is_new || staging_is_missing)
        && !backup_not_required
    {
        recovery_rename(
            parent,
            journal.backup(),
            &parent.output_basename,
            OutputState::Indeterminate,
            hooks,
        )?;
        if staging_is_new {
            remove_recovery_directory(
                parent,
                journal.staging(),
                RegistrationSubject::Staging,
                RegistrationControl::Staging,
                OutputState::Restored,
                hooks,
            )?;
        }
        remove_journal_controls(
            parent,
            control_id,
            &journal,
            held_journal.identity,
            OutputState::Restored,
            hooks,
        )?;
        return Ok(RecoveryEffect::Restored);
    }

    if output_is_new && (backup_is_old || backup_is_missing) && staging_is_missing {
        if backup_is_old && !backup_not_required {
            remove_recovery_directory(
                parent,
                journal.backup(),
                RegistrationSubject::Backup,
                RegistrationControl::Backup,
                OutputState::Updated,
                hooks,
            )?;
        }
        if staging_is_new {
            remove_recovery_directory(
                parent,
                journal.staging(),
                RegistrationSubject::Staging,
                RegistrationControl::Staging,
                OutputState::Updated,
                hooks,
            )?;
        }
        remove_journal_controls(
            parent,
            control_id,
            &journal,
            held_journal.identity,
            OutputState::Updated,
            hooks,
        )?;
        return Ok(RecoveryEffect::Updated);
    }

    if journal.phase() == JournalPhase::NewInstalled
        && matches!(output, RecoveryRootObservation::Missing)
        && backup_is_old
        && !backup_not_required
        && (staging_is_new || staging_is_missing)
    {
        recovery_rename(
            parent,
            journal.backup(),
            &parent.output_basename,
            OutputState::Indeterminate,
            hooks,
        )?;
        if staging_is_new {
            remove_recovery_directory(
                parent,
                journal.staging(),
                RegistrationSubject::Staging,
                RegistrationControl::Staging,
                OutputState::Restored,
                hooks,
            )?;
        }
        remove_journal_controls(
            parent,
            control_id,
            &journal,
            held_journal.identity,
            OutputState::Restored,
            hooks,
        )?;
        return Ok(RecoveryEffect::Restored);
    }

    Err(recovery_error(
        RecoveryFailureReason::StateAmbiguous,
        RegistrationSubject::Journal,
        Some(RegistrationControl::Journal),
        None,
    ))
}

struct HeldJournal {
    record: JournalRecord,
    identity: FileIdentity,
}

fn read_journal(
    parent: &OutputParent,
    basename: &str,
    control_id: OutputRootControlId,
) -> Result<Option<HeldJournal>, OutputRegistrationError> {
    let metadata = match parent.dir.symlink_metadata(basename) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(recovery_io_error(
                RecoveryFailureReason::JournalInvalid,
                RegistrationSubject::Journal,
                Some(RegistrationControl::Journal),
                RegistrationOperation::Inspect,
                &error,
                OutputState::Unchanged,
            ));
        }
    };
    if metadata.is_symlink() || !metadata.is_file() {
        return Err(recovery_error(
            RecoveryFailureReason::JournalInvalid,
            RegistrationSubject::Journal,
            Some(RegistrationControl::Journal),
            None,
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let mut file = parent.dir.open_with(basename, &options).map_err(|error| {
        recovery_io_error(
            RecoveryFailureReason::JournalInvalid,
            RegistrationSubject::Journal,
            Some(RegistrationControl::Journal),
            RegistrationOperation::Open,
            &error,
            OutputState::Unchanged,
        )
    })?;
    let identity = file_identity(&file.metadata().map_err(|error| {
        recovery_io_error(
            RecoveryFailureReason::JournalInvalid,
            RegistrationSubject::Journal,
            Some(RegistrationControl::Journal),
            RegistrationOperation::Inspect,
            &error,
            OutputState::Unchanged,
        )
    })?)?;
    let bytes = read_control_file(&mut file).map_err(|error| {
        recovery_io_error(
            RecoveryFailureReason::JournalInvalid,
            RegistrationSubject::Journal,
            Some(RegistrationControl::Journal),
            RegistrationOperation::Read,
            &error,
            OutputState::Unchanged,
        )
    })?;
    let record = JournalRecord::decode(&bytes, control_id).map_err(|error| match error.kind() {
        ControlCodecErrorKind::InternalInvariant => OutputRegistrationError::internal_invariant(),
        ControlCodecErrorKind::Invalid
        | ControlCodecErrorKind::Unsupported
        | ControlCodecErrorKind::Limit => recovery_error(
            RecoveryFailureReason::JournalInvalid,
            RegistrationSubject::Journal,
            Some(RegistrationControl::Journal),
            None,
        ),
    })?;
    verify_recovery_journal(
        parent,
        basename,
        identity,
        record.canonical_bytes(),
        OutputState::Unchanged,
    )?;
    Ok(Some(HeldJournal { record, identity }))
}

fn verify_recovery_journal(
    parent: &OutputParent,
    basename: &str,
    expected_identity: FileIdentity,
    expected_bytes: &[u8],
    output_state: OutputState,
) -> Result<(), OutputRegistrationError> {
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    let mut file = parent.dir.open_with(basename, &options).map_err(|error| {
        recovery_io_error(
            RecoveryFailureReason::StateAmbiguous,
            RegistrationSubject::Journal,
            Some(RegistrationControl::Journal),
            RegistrationOperation::Open,
            &error,
            output_state,
        )
    })?;
    let identity = file_identity(&file.metadata().map_err(|error| {
        recovery_io_error(
            RecoveryFailureReason::StateAmbiguous,
            RegistrationSubject::Journal,
            Some(RegistrationControl::Journal),
            RegistrationOperation::Inspect,
            &error,
            output_state,
        )
    })?)?;
    if identity != expected_identity {
        return Err(recovery_error(
            RecoveryFailureReason::StateAmbiguous,
            RegistrationSubject::Journal,
            Some(RegistrationControl::Journal),
            None,
        )
        .with_output_state(output_state));
    }
    let bytes = read_control_file(&mut file).map_err(|error| {
        recovery_io_error(
            RecoveryFailureReason::StateAmbiguous,
            RegistrationSubject::Journal,
            Some(RegistrationControl::Journal),
            RegistrationOperation::Read,
            &error,
            output_state,
        )
    })?;
    if bytes.as_ref() != expected_bytes {
        return Err(recovery_error(
            RecoveryFailureReason::StateAmbiguous,
            RegistrationSubject::Journal,
            Some(RegistrationControl::Journal),
            None,
        )
        .with_output_state(output_state));
    }
    Ok(())
}

fn recovery_observation(
    parent: &OutputParent,
    basename: &str,
    output_root: &DeliveryOutputRoot,
    exporter: crate::messages::delivery::BuiltInExporterId,
) -> Result<RecoveryRootObservation, OutputRegistrationError> {
    inspection::inspect_recovery_root(&parent.dir, basename, output_root, exporter).map_err(
        |error| match error.kind() {
            OutputRegistrationErrorKind::UnsupportedCapability => error,
            OutputRegistrationErrorKind::InternalInvariant => {
                OutputRegistrationError::internal_invariant()
            }
            OutputRegistrationErrorKind::DestinationMappingFailed
            | OutputRegistrationErrorKind::DestinationCollision
            | OutputRegistrationErrorKind::RegistrationFailed => recovery_error(
                RecoveryFailureReason::StateAmbiguous,
                RegistrationSubject::Journal,
                Some(RegistrationControl::Journal),
                None,
            ),
        },
    )
}

fn matches_old_root(observed: RecoveryRootObservation, old_root: JournalOldRoot) -> bool {
    match (observed, old_root) {
        (RecoveryRootObservation::Missing, JournalOldRoot::Absent)
        | (RecoveryRootObservation::Empty, JournalOldRoot::Empty) => true,
        (
            RecoveryRootObservation::Managed {
                manifest_fingerprint: observed,
            },
            JournalOldRoot::Managed {
                manifest_fingerprint: expected,
            },
        ) => observed == expected,
        _ => false,
    }
}

fn matches_manifest(
    observed: RecoveryRootObservation,
    expected: super::manifest::Blake3Fingerprint,
) -> bool {
    matches!(
        observed,
        RecoveryRootObservation::Managed {
            manifest_fingerprint
        } if manifest_fingerprint == expected
    )
}

fn remove_recovery_directory(
    parent: &OutputParent,
    basename: &str,
    subject: RegistrationSubject,
    control: RegistrationControl,
    output_state: OutputState,
    hooks: &dyn TransactionHooks,
) -> Result<(), OutputRegistrationError> {
    hooks
        .before(FaultPoint::RecoveryCleanup)
        .and_then(|()| parent.dir.remove_dir_all(basename))
        .map_err(|error| {
            recovery_io_error(
                RecoveryFailureReason::CleanupFailed,
                subject,
                Some(control),
                RegistrationOperation::Remove,
                &error,
                output_state,
            )
        })?;
    sync_directory_at(&parent.dir, hooks, FaultPoint::RecoveryDirectoryFlush).map_err(|error| {
        recovery_io_error(
            RecoveryFailureReason::FlushFailed,
            subject,
            Some(control),
            RegistrationOperation::Flush,
            &error,
            output_state,
        )
    })
}

fn remove_journal_controls(
    parent: &OutputParent,
    control_id: OutputRootControlId,
    journal: &JournalRecord,
    journal_identity: FileIdentity,
    output_state: OutputState,
    hooks: &dyn TransactionHooks,
) -> Result<(), OutputRegistrationError> {
    let journal_basename = control_id.journal_basename();
    verify_recovery_journal(
        parent,
        &journal_basename,
        journal_identity,
        journal.canonical_bytes(),
        output_state,
    )?;
    let temporary = control_id.journal_temporary_basename(journal.transaction_id());
    remove_optional_regular_control(
        parent,
        &temporary,
        RecoveryFailureReason::CleanupFailed,
        output_state,
        hooks,
    )?;
    remove_required_regular_control(
        parent,
        &journal_basename,
        RecoveryFailureReason::CleanupFailed,
        output_state,
        journal_identity,
        journal.canonical_bytes(),
        hooks,
    )?;
    sync_directory_at(&parent.dir, hooks, FaultPoint::RecoveryDirectoryFlush).map_err(|error| {
        recovery_io_error(
            RecoveryFailureReason::FlushFailed,
            RegistrationSubject::Journal,
            Some(RegistrationControl::Journal),
            RegistrationOperation::Flush,
            &error,
            output_state,
        )
    })
}

fn remove_optional_regular_control(
    parent: &OutputParent,
    basename: &str,
    reason: RecoveryFailureReason,
    output_state: OutputState,
    hooks: &dyn TransactionHooks,
) -> Result<(), OutputRegistrationError> {
    match parent.dir.symlink_metadata(basename) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Ok(metadata) if metadata.is_file() && !metadata.is_symlink() => hooks
            .before(FaultPoint::RecoveryCleanup)
            .and_then(|()| parent.dir.remove_file(basename))
            .map_err(|error| {
                recovery_io_error(
                    reason,
                    RegistrationSubject::Journal,
                    Some(RegistrationControl::Journal),
                    RegistrationOperation::Remove,
                    &error,
                    output_state,
                )
            }),
        Ok(_) => Err(recovery_error(
            RecoveryFailureReason::StateAmbiguous,
            RegistrationSubject::Journal,
            Some(RegistrationControl::Journal),
            None,
        )),
        Err(error) => Err(recovery_io_error(
            reason,
            RegistrationSubject::Journal,
            Some(RegistrationControl::Journal),
            RegistrationOperation::Inspect,
            &error,
            output_state,
        )),
    }
}

fn remove_required_regular_control(
    parent: &OutputParent,
    basename: &str,
    reason: RecoveryFailureReason,
    output_state: OutputState,
    identity: FileIdentity,
    canonical_bytes: &[u8],
    hooks: &dyn TransactionHooks,
) -> Result<(), OutputRegistrationError> {
    verify_recovery_journal(parent, basename, identity, canonical_bytes, output_state)?;
    match parent.dir.symlink_metadata(basename) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Err(recovery_error(
            RecoveryFailureReason::StateAmbiguous,
            RegistrationSubject::Journal,
            Some(RegistrationControl::Journal),
            None,
        )
        .with_output_state(output_state)),
        Ok(_) => remove_optional_regular_control(parent, basename, reason, output_state, hooks),
        Err(error) => Err(recovery_io_error(
            reason,
            RegistrationSubject::Journal,
            Some(RegistrationControl::Journal),
            RegistrationOperation::Inspect,
            &error,
            output_state,
        )),
    }
}

fn recovery_rename(
    parent: &OutputParent,
    from: &str,
    to: &str,
    failure_state: OutputState,
    hooks: &dyn TransactionHooks,
) -> Result<(), OutputRegistrationError> {
    hooks
        .before(FaultPoint::RecoveryRename)
        .and_then(|()| parent.dir.rename(from, &parent.dir, to))
        .map_err(|error| {
            recovery_io_error(
                RecoveryFailureReason::OldRootRestoreFailed,
                RegistrationSubject::Backup,
                Some(RegistrationControl::Backup),
                RegistrationOperation::Rename,
                &error,
                failure_state,
            )
        })?;
    let restored = parent.dir.open_dir_nofollow(to).map_err(|error| {
        recovery_io_error(
            RecoveryFailureReason::OldRootRestoreFailed,
            RegistrationSubject::OutputRoot,
            None,
            RegistrationOperation::Open,
            &error,
            failure_state,
        )
    })?;
    sync_directory_at(&restored, hooks, FaultPoint::RecoveryDirectoryFlush).map_err(|error| {
        recovery_io_error(
            RecoveryFailureReason::FlushFailed,
            RegistrationSubject::OutputRoot,
            None,
            RegistrationOperation::Flush,
            &error,
            failure_state,
        )
    })?;
    sync_directory_at(&parent.dir, hooks, FaultPoint::RecoveryDirectoryFlush).map_err(|error| {
        recovery_io_error(
            RecoveryFailureReason::FlushFailed,
            RegistrationSubject::OutputRoot,
            None,
            RegistrationOperation::Flush,
            &error,
            failure_state,
        )
    })
}

fn preflight_transaction_names(
    parent: &OutputParent,
    control_id: OutputRootControlId,
    journal: &JournalRecord,
) -> Result<(), OutputRegistrationError> {
    for (basename, subject, control) in [
        (
            journal.staging().to_owned(),
            RegistrationSubject::Staging,
            RegistrationControl::Staging,
        ),
        (
            journal.backup().to_owned(),
            RegistrationSubject::Backup,
            RegistrationControl::Backup,
        ),
        (
            control_id.journal_temporary_basename(journal.transaction_id()),
            RegistrationSubject::Journal,
            RegistrationControl::Journal,
        ),
    ] {
        match parent.dir.symlink_metadata(&basename) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(OutputRegistrationError::registration(
                    RegistrationFailureEvidence::staging(
                        StagingFailureReason::ControlConflict,
                        subject,
                        Some(control),
                        None,
                        None,
                        None,
                    ),
                ));
            }
            Err(error) => {
                return Err(staging_io_error(
                    StagingFailureReason::CreateFailed,
                    subject,
                    Some(control),
                    None,
                    None,
                    RegistrationOperation::Inspect,
                    &error,
                ));
            }
        }
    }
    Ok(())
}

struct StagingConstructionFailure {
    error: OutputRegistrationError,
    owns_staging: bool,
}

fn construct_staging(
    prepared: &PreparedOutput<'_>,
    parent: &OutputParent,
    journal: &JournalRecord,
    hooks: &dyn TransactionHooks,
) -> Result<(), StagingConstructionFailure> {
    match parent.dir.symlink_metadata(journal.staging()) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Ok(_) => {
            return Err(StagingConstructionFailure {
                error: OutputRegistrationError::registration(RegistrationFailureEvidence::staging(
                    StagingFailureReason::ControlConflict,
                    RegistrationSubject::Staging,
                    Some(RegistrationControl::Staging),
                    None,
                    None,
                    None,
                )),
                owns_staging: false,
            });
        }
        Err(error) => {
            return Err(StagingConstructionFailure {
                error: staging_io_error(
                    StagingFailureReason::CreateFailed,
                    RegistrationSubject::Staging,
                    Some(RegistrationControl::Staging),
                    None,
                    None,
                    RegistrationOperation::Inspect,
                    &error,
                ),
                owns_staging: false,
            });
        }
    }
    parent
        .dir
        .create_dir(journal.staging())
        .map_err(|error| StagingConstructionFailure {
            error: staging_io_error(
                StagingFailureReason::CreateFailed,
                RegistrationSubject::Staging,
                Some(RegistrationControl::Staging),
                None,
                None,
                RegistrationOperation::Create,
                &error,
            ),
            owns_staging: false,
        })?;
    construct_staging_contents(prepared, parent, journal, hooks).map_err(|error| {
        StagingConstructionFailure {
            error,
            owns_staging: true,
        }
    })
}

fn construct_staging_contents(
    prepared: &PreparedOutput<'_>,
    parent: &OutputParent,
    journal: &JournalRecord,
    hooks: &dyn TransactionHooks,
) -> Result<(), OutputRegistrationError> {
    let staging = parent
        .dir
        .open_dir_nofollow(journal.staging())
        .map_err(|error| {
            staging_io_error(
                StagingFailureReason::CreateFailed,
                RegistrationSubject::Staging,
                Some(RegistrationControl::Staging),
                None,
                None,
                RegistrationOperation::Open,
                &error,
            )
        })?;

    let mut directories = BTreeSet::new();
    for artifact in prepared.artifacts.artifacts() {
        let destination = prepared
            .destinations()
            .artifact(artifact.logical_path())
            .ok_or_else(OutputRegistrationError::internal_invariant)?;
        write_staging_artifact(
            &staging,
            destination.relative_path(),
            artifact.logical_path(),
            artifact.payload().as_bytes(),
            &mut directories,
            hooks,
        )?;
    }
    write_new_file(
        &staging,
        Path::new(OUTPUT_MANIFEST_BASENAME),
        prepared.manifest().canonical_bytes(),
        RegistrationSubject::Manifest,
        Some(RegistrationControl::Manifest),
        None,
        hooks,
    )?;

    let mut directories = directories.into_iter().collect::<Vec<_>>();
    directories.sort_by(|left, right| {
        right
            .components()
            .count()
            .cmp(&left.components().count())
            .then_with(|| left.cmp(right))
    });
    for directory in directories {
        let dir = staging.open_dir_nofollow(&directory).map_err(|error| {
            staging_io_error(
                StagingFailureReason::FlushFailed,
                RegistrationSubject::Staging,
                Some(RegistrationControl::Staging),
                None,
                evidence_path(&directory),
                RegistrationOperation::Open,
                &error,
            )
        })?;
        sync_directory_at(&dir, hooks, FaultPoint::StagingDirectoryFlush).map_err(|error| {
            staging_io_error(
                StagingFailureReason::FlushFailed,
                RegistrationSubject::Staging,
                Some(RegistrationControl::Staging),
                None,
                evidence_path(&directory),
                RegistrationOperation::Flush,
                &error,
            )
        })?;
    }
    sync_directory_at(&staging, hooks, FaultPoint::StagingDirectoryFlush).map_err(|error| {
        staging_io_error(
            StagingFailureReason::FlushFailed,
            RegistrationSubject::Staging,
            Some(RegistrationControl::Staging),
            None,
            None,
            RegistrationOperation::Flush,
            &error,
        )
    })?;
    sync_directory_at(&parent.dir, hooks, FaultPoint::StagingParentFlush).map_err(|error| {
        staging_io_error(
            StagingFailureReason::FlushFailed,
            RegistrationSubject::Staging,
            Some(RegistrationControl::Staging),
            None,
            None,
            RegistrationOperation::Flush,
            &error,
        )
    })
}

fn cleanup_failed_staging(
    parent: &OutputParent,
    basename: &str,
    hooks: &dyn TransactionHooks,
) -> Result<(), OutputRegistrationError> {
    let metadata = match parent.dir.symlink_metadata(basename) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(rollback_io_error(
                RollbackFailureReason::Cleanup,
                RegistrationSubject::Staging,
                Some(RegistrationControl::Staging),
                RegistrationOperation::Inspect,
                &error,
                OutputState::Unchanged,
            ));
        }
    };
    if metadata.is_symlink() || !metadata.is_dir() {
        return Err(rollback_error(
            RollbackFailureReason::Cleanup,
            RegistrationSubject::Staging,
            Some(RegistrationControl::Staging),
            OutputState::Unchanged,
        ));
    }
    hooks
        .before(FaultPoint::StagingCleanup)
        .and_then(|()| parent.dir.remove_dir_all(basename))
        .map_err(|error| {
            rollback_io_error(
                RollbackFailureReason::Cleanup,
                RegistrationSubject::Staging,
                Some(RegistrationControl::Staging),
                RegistrationOperation::Remove,
                &error,
                OutputState::Unchanged,
            )
        })?;
    sync_directory_at(&parent.dir, hooks, FaultPoint::StagingCleanupFlush).map_err(|error| {
        rollback_io_error(
            RollbackFailureReason::Flush,
            RegistrationSubject::Staging,
            Some(RegistrationControl::Staging),
            RegistrationOperation::Flush,
            &error,
            OutputState::Unchanged,
        )
    })
}

fn write_staging_artifact(
    staging: &Dir,
    relative_path: &Path,
    logical_path: &ExportArtifactPath,
    bytes: &[u8],
    directories: &mut BTreeSet<PathBuf>,
    hooks: &dyn TransactionHooks,
) -> Result<(), OutputRegistrationError> {
    let Some(parent_path) = relative_path.parent() else {
        return Err(OutputRegistrationError::internal_invariant());
    };
    let mut current = staging.try_clone().map_err(|error| {
        staging_io_error(
            StagingFailureReason::CreateFailed,
            RegistrationSubject::Artifact,
            None,
            Some(logical_path.clone()),
            evidence_path(relative_path),
            RegistrationOperation::Open,
            &error,
        )
    })?;
    let mut accumulated = PathBuf::new();
    for component in parent_path.components() {
        let std::path::Component::Normal(segment) = component else {
            return Err(OutputRegistrationError::internal_invariant());
        };
        accumulated.push(segment);
        match current.symlink_metadata(segment) {
            Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => {}
            Ok(_) => return Err(OutputRegistrationError::internal_invariant()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                current.create_dir(segment).map_err(|error| {
                    staging_io_error(
                        StagingFailureReason::CreateFailed,
                        RegistrationSubject::Artifact,
                        None,
                        Some(logical_path.clone()),
                        evidence_path(relative_path),
                        RegistrationOperation::Create,
                        &error,
                    )
                })?;
            }
            Err(error) => {
                return Err(staging_io_error(
                    StagingFailureReason::CreateFailed,
                    RegistrationSubject::Artifact,
                    None,
                    Some(logical_path.clone()),
                    evidence_path(relative_path),
                    RegistrationOperation::Inspect,
                    &error,
                ));
            }
        }
        current = current.open_dir_nofollow(segment).map_err(|error| {
            staging_io_error(
                StagingFailureReason::CreateFailed,
                RegistrationSubject::Artifact,
                None,
                Some(logical_path.clone()),
                evidence_path(relative_path),
                RegistrationOperation::Open,
                &error,
            )
        })?;
        directories.insert(accumulated.clone());
    }
    let Some(filename) = relative_path.file_name() else {
        return Err(OutputRegistrationError::internal_invariant());
    };
    write_new_file(
        &current,
        Path::new(filename),
        bytes,
        RegistrationSubject::Artifact,
        None,
        Some(logical_path),
        hooks,
    )
}

fn write_new_file(
    parent: &Dir,
    path: &Path,
    bytes: &[u8],
    subject: RegistrationSubject,
    control: Option<RegistrationControl>,
    artifact_path: Option<&ExportArtifactPath>,
    hooks: &dyn TransactionHooks,
) -> Result<(), OutputRegistrationError> {
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .follow(FollowSymlinks::No);
    let mut file = parent.open_with(path, &options).map_err(|error| {
        staging_io_error(
            StagingFailureReason::CreateFailed,
            subject,
            control,
            artifact_path.cloned(),
            artifact_path.and_then(export_path_evidence),
            RegistrationOperation::Create,
            &error,
        )
    })?;
    file.write_all(bytes).map_err(|error| {
        staging_io_error(
            StagingFailureReason::WriteFailed,
            subject,
            control,
            artifact_path.cloned(),
            artifact_path.and_then(export_path_evidence),
            RegistrationOperation::Write,
            &error,
        )
    })?;
    sync_file_at(&file, hooks, FaultPoint::StagingFileFlush).map_err(|error| {
        staging_io_error(
            StagingFailureReason::FlushFailed,
            subject,
            control,
            artifact_path.cloned(),
            artifact_path.and_then(export_path_evidence),
            RegistrationOperation::Flush,
            &error,
        )
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
enum CommitRootState {
    #[default]
    Staged,
    OldMoved,
    NewInstalled,
    BackupRemoved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum CommitControlState {
    #[default]
    None,
    Journal,
    JournalAndTemporary,
}

#[derive(Debug, Clone, Copy, Default)]
struct CommitProgress {
    root: CommitRootState,
    controls: CommitControlState,
}

impl CommitProgress {
    const fn old_moved(self) -> bool {
        matches!(
            self.root,
            CommitRootState::OldMoved
                | CommitRootState::NewInstalled
                | CommitRootState::BackupRemoved
        )
    }

    const fn new_installed(self) -> bool {
        matches!(
            self.root,
            CommitRootState::NewInstalled | CommitRootState::BackupRemoved
        )
    }

    const fn backup_removed(self) -> bool {
        matches!(self.root, CommitRootState::BackupRemoved)
    }

    const fn owns_journal(self) -> bool {
        matches!(
            self.controls,
            CommitControlState::Journal | CommitControlState::JournalAndTemporary
        )
    }

    const fn owns_journal_temporary(self) -> bool {
        matches!(self.controls, CommitControlState::JournalAndTemporary)
    }
}

struct CommitFailure {
    error: OutputRegistrationError,
    progress: CommitProgress,
}

fn commit_staging(
    parent: &OutputParent,
    journal: &JournalRecord,
    hooks: &dyn TransactionHooks,
) -> Result<(), CommitFailure> {
    let mut progress = CommitProgress::default();
    if let Err(failure) = write_initial_journal(parent, journal, hooks) {
        if failure.created {
            progress.controls = CommitControlState::Journal;
        }
        return Err(CommitFailure {
            error: failure.error,
            progress,
        });
    }
    progress.controls = CommitControlState::Journal;

    if matches!(journal.old_root(), JournalOldRoot::Absent) {
        progress.root = CommitRootState::OldMoved;
    } else {
        ensure_entry_missing(
            parent,
            journal.backup(),
            CommitFailureReason::OldRootMoveFailed,
            RegistrationSubject::Backup,
            Some(RegistrationControl::Backup),
        )
        .map_err(|error| CommitFailure { error, progress })?;
        hooks
            .before(FaultPoint::OldRootRename)
            .and_then(|()| {
                parent.dir.rename(
                    parent.output_basename.as_ref(),
                    &parent.dir,
                    journal.backup(),
                )
            })
            .map_err(|error| CommitFailure {
                error: commit_io_error(
                    CommitFailureReason::OldRootMoveFailed,
                    RegistrationSubject::OutputRoot,
                    Some(RegistrationControl::Backup),
                    RegistrationOperation::Rename,
                    &error,
                ),
                progress,
            })?;
        progress.root = CommitRootState::OldMoved;
        sync_directory_at(&parent.dir, hooks, FaultPoint::OldRootDirectoryFlush).map_err(
            |error| CommitFailure {
                error: commit_io_error(
                    CommitFailureReason::FlushFailed,
                    RegistrationSubject::Backup,
                    Some(RegistrationControl::Backup),
                    RegistrationOperation::Flush,
                    &error,
                ),
                progress,
            },
        )?;
    }

    let old_moved = journal
        .with_phase(JournalPhase::OldMoved)
        .map_err(|_| CommitFailure {
            error: OutputRegistrationError::internal_invariant(),
            progress,
        })?;
    if let Err(failure) = replace_journal(parent, &old_moved, hooks) {
        if failure.created {
            progress.controls = CommitControlState::JournalAndTemporary;
        }
        return Err(CommitFailure {
            error: failure.error,
            progress,
        });
    }

    hooks
        .before(FaultPoint::NewRootRename)
        .and_then(|()| {
            parent.dir.rename(
                journal.staging(),
                &parent.dir,
                parent.output_basename.as_ref(),
            )
        })
        .map_err(|error| CommitFailure {
            error: commit_io_error(
                CommitFailureReason::NewRootInstallFailed,
                RegistrationSubject::Staging,
                Some(RegistrationControl::Staging),
                RegistrationOperation::Rename,
                &error,
            ),
            progress,
        })?;
    progress.root = CommitRootState::NewInstalled;
    sync_directory_at(&parent.dir, hooks, FaultPoint::NewRootDirectoryFlush).map_err(|error| {
        CommitFailure {
            error: commit_io_error(
                CommitFailureReason::FlushFailed,
                RegistrationSubject::OutputRoot,
                None,
                RegistrationOperation::Flush,
                &error,
            ),
            progress,
        }
    })?;

    let new_installed = journal
        .with_phase(JournalPhase::NewInstalled)
        .map_err(|_| CommitFailure {
            error: OutputRegistrationError::internal_invariant(),
            progress,
        })?;
    if let Err(failure) = replace_journal(parent, &new_installed, hooks) {
        if failure.created {
            progress.controls = CommitControlState::JournalAndTemporary;
        }
        return Err(CommitFailure {
            error: failure.error,
            progress,
        });
    }

    if matches!(journal.old_root(), JournalOldRoot::Absent) {
        progress.root = CommitRootState::BackupRemoved;
    } else {
        hooks
            .before(FaultPoint::BackupCleanup)
            .and_then(|()| parent.dir.remove_dir_all(journal.backup()))
            .map_err(|error| CommitFailure {
                error: commit_io_error(
                    CommitFailureReason::CleanupFailed,
                    RegistrationSubject::Backup,
                    Some(RegistrationControl::Backup),
                    RegistrationOperation::Remove,
                    &error,
                ),
                progress,
            })?;
        progress.root = CommitRootState::BackupRemoved;
        sync_directory_at(&parent.dir, hooks, FaultPoint::BackupDirectoryFlush).map_err(
            |error| CommitFailure {
                error: commit_io_error(
                    CommitFailureReason::FlushFailed,
                    RegistrationSubject::Backup,
                    Some(RegistrationControl::Backup),
                    RegistrationOperation::Flush,
                    &error,
                ),
                progress,
            },
        )?;
    }

    let journal_basename = OutputRootControlId::derive(journal.output_root())
        .map_err(|_| CommitFailure {
            error: OutputRegistrationError::internal_invariant(),
            progress,
        })?
        .journal_basename();
    hooks
        .before(FaultPoint::JournalCleanup)
        .and_then(|()| parent.dir.remove_file(&journal_basename))
        .map_err(|error| CommitFailure {
            error: commit_io_error(
                CommitFailureReason::CleanupFailed,
                RegistrationSubject::Journal,
                Some(RegistrationControl::Journal),
                RegistrationOperation::Remove,
                &error,
            ),
            progress,
        })?;
    progress.controls = CommitControlState::None;
    sync_directory_at(&parent.dir, hooks, FaultPoint::JournalCleanupFlush).map_err(|error| {
        CommitFailure {
            error: commit_io_error(
                CommitFailureReason::FlushFailed,
                RegistrationSubject::Journal,
                Some(RegistrationControl::Journal),
                RegistrationOperation::Flush,
                &error,
            ),
            progress,
        }
    })?;
    Ok(())
}

struct ControlWriteFailure {
    error: OutputRegistrationError,
    created: bool,
}

fn write_initial_journal(
    parent: &OutputParent,
    journal: &JournalRecord,
    hooks: &dyn TransactionHooks,
) -> Result<(), ControlWriteFailure> {
    let control_id =
        OutputRootControlId::derive(journal.output_root()).map_err(|_| ControlWriteFailure {
            error: OutputRegistrationError::internal_invariant(),
            created: false,
        })?;
    let basename = control_id.journal_basename();
    ensure_entry_missing(
        parent,
        &basename,
        CommitFailureReason::JournalConflict,
        RegistrationSubject::Journal,
        Some(RegistrationControl::Journal),
    )
    .map_err(|error| ControlWriteFailure {
        error,
        created: false,
    })?;
    write_commit_file(
        parent,
        &basename,
        journal.canonical_bytes(),
        CommitFailureReason::JournalWriteFailed,
        RegistrationOperation::Create,
        JournalPhase::Staged,
        hooks,
    )?;
    sync_directory_at(
        &parent.dir,
        hooks,
        FaultPoint::JournalDirectoryFlush(JournalPhase::Staged),
    )
    .map_err(|error| ControlWriteFailure {
        error: commit_io_error(
            CommitFailureReason::FlushFailed,
            RegistrationSubject::Journal,
            Some(RegistrationControl::Journal),
            RegistrationOperation::Flush,
            &error,
        ),
        created: true,
    })
}

fn replace_journal(
    parent: &OutputParent,
    journal: &JournalRecord,
    hooks: &dyn TransactionHooks,
) -> Result<(), ControlWriteFailure> {
    let control_id =
        OutputRootControlId::derive(journal.output_root()).map_err(|_| ControlWriteFailure {
            error: OutputRegistrationError::internal_invariant(),
            created: false,
        })?;
    let temporary = control_id.journal_temporary_basename(journal.transaction_id());
    ensure_entry_missing(
        parent,
        &temporary,
        CommitFailureReason::JournalConflict,
        RegistrationSubject::Journal,
        Some(RegistrationControl::Journal),
    )
    .map_err(|error| ControlWriteFailure {
        error,
        created: false,
    })?;
    write_commit_file(
        parent,
        &temporary,
        journal.canonical_bytes(),
        CommitFailureReason::JournalWriteFailed,
        RegistrationOperation::Create,
        journal.phase(),
        hooks,
    )?;
    hooks
        .before(FaultPoint::JournalReplace(journal.phase()))
        .and_then(|()| {
            parent
                .dir
                .rename(&temporary, &parent.dir, control_id.journal_basename())
        })
        .map_err(|error| ControlWriteFailure {
            error: commit_io_error(
                CommitFailureReason::JournalWriteFailed,
                RegistrationSubject::Journal,
                Some(RegistrationControl::Journal),
                RegistrationOperation::Replace,
                &error,
            ),
            created: true,
        })?;
    sync_directory_at(
        &parent.dir,
        hooks,
        FaultPoint::JournalDirectoryFlush(journal.phase()),
    )
    .map_err(|error| ControlWriteFailure {
        error: commit_io_error(
            CommitFailureReason::FlushFailed,
            RegistrationSubject::Journal,
            Some(RegistrationControl::Journal),
            RegistrationOperation::Flush,
            &error,
        ),
        created: false,
    })
}

fn write_commit_file(
    parent: &OutputParent,
    basename: &str,
    bytes: &[u8],
    reason: CommitFailureReason,
    operation: RegistrationOperation,
    phase: JournalPhase,
    hooks: &dyn TransactionHooks,
) -> Result<(), ControlWriteFailure> {
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .follow(FollowSymlinks::No);
    let mut file = hooks
        .before(FaultPoint::JournalCreate(phase))
        .and_then(|()| parent.dir.open_with(basename, &options))
        .map_err(|error| ControlWriteFailure {
            error: commit_io_error(
                reason,
                RegistrationSubject::Journal,
                Some(RegistrationControl::Journal),
                operation,
                &error,
            ),
            created: false,
        })?;
    file.write_all(bytes).map_err(|error| ControlWriteFailure {
        error: commit_io_error(
            reason,
            RegistrationSubject::Journal,
            Some(RegistrationControl::Journal),
            RegistrationOperation::Write,
            &error,
        ),
        created: true,
    })?;
    sync_file_at(&file, hooks, FaultPoint::JournalFileFlush(phase)).map_err(|error| {
        ControlWriteFailure {
            error: commit_io_error(
                CommitFailureReason::FlushFailed,
                RegistrationSubject::Journal,
                Some(RegistrationControl::Journal),
                RegistrationOperation::Flush,
                &error,
            ),
            created: true,
        }
    })
}

fn ensure_entry_missing(
    parent: &OutputParent,
    basename: &str,
    reason: CommitFailureReason,
    subject: RegistrationSubject,
    control: Option<RegistrationControl>,
) -> Result<(), OutputRegistrationError> {
    match parent.dir.symlink_metadata(basename) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(OutputRegistrationError::registration(
            RegistrationFailureEvidence::commit(reason, subject, control, None),
        )),
        Err(error) => Err(commit_io_error(
            reason,
            subject,
            control,
            RegistrationOperation::Inspect,
            &error,
        )),
    }
}

fn rollback_after_commit(
    parent: &OutputParent,
    journal: &JournalRecord,
    failure: CommitFailure,
    hooks: &dyn TransactionHooks,
) -> OutputRegistrationError {
    if !failure.progress.old_moved() {
        // Only sibling staging or control entries can have changed before this
        // point. The configured output root is still provably unchanged even
        // when cleanup of those sibling entries fails.
        return match cleanup_before_old_move(parent, journal, failure.progress, hooks) {
            Ok(()) => failure.error.with_output_state(OutputState::Unchanged),
            Err(error) => error.with_output_state(OutputState::Unchanged),
        };
    }
    if failure.progress.backup_removed() {
        return failure.error.with_output_state(OutputState::Updated);
    }

    match restore_prior_root(parent, journal, failure.progress, hooks) {
        Ok(state) => failure.error.with_output_state(state),
        Err(error) if error.output_state() == OutputState::Restored => error,
        Err(error) => error.with_output_state(OutputState::Indeterminate),
    }
}

fn cleanup_before_old_move(
    parent: &OutputParent,
    journal: &JournalRecord,
    progress: CommitProgress,
    hooks: &dyn TransactionHooks,
) -> Result<(), OutputRegistrationError> {
    let mut removed =
        remove_rollback_staging(parent, journal.staging(), OutputState::Unchanged, hooks)?;

    let control_id = OutputRootControlId::derive(journal.output_root()).map_err(|_| {
        OutputRegistrationError::internal_invariant().with_output_state(OutputState::Unchanged)
    })?;
    if progress.owns_journal_temporary() {
        removed |= remove_rollback_control(
            parent,
            &control_id.journal_temporary_basename(journal.transaction_id()),
            OutputState::Unchanged,
            hooks,
        )?;
    }
    if progress.owns_journal() {
        removed |= remove_rollback_control(
            parent,
            &control_id.journal_basename(),
            OutputState::Unchanged,
            hooks,
        )?;
    }

    if removed {
        sync_directory_at(&parent.dir, hooks, FaultPoint::RollbackDirectoryFlush).map_err(
            |error| {
                rollback_io_error(
                    RollbackFailureReason::Flush,
                    RegistrationSubject::OutputRoot,
                    None,
                    RegistrationOperation::Flush,
                    &error,
                    OutputState::Unchanged,
                )
            },
        )?;
    }
    Ok(())
}

fn restore_prior_root(
    parent: &OutputParent,
    journal: &JournalRecord,
    progress: CommitProgress,
    hooks: &dyn TransactionHooks,
) -> Result<OutputState, OutputRegistrationError> {
    if progress.new_installed() {
        hooks
            .before(FaultPoint::RollbackRename)
            .and_then(|()| {
                parent.dir.rename(
                    parent.output_basename.as_ref(),
                    &parent.dir,
                    journal.staging(),
                )
            })
            .map_err(|error| {
                rollback_io_error(
                    RollbackFailureReason::OldRootRestore,
                    RegistrationSubject::OutputRoot,
                    None,
                    RegistrationOperation::Rename,
                    &error,
                    OutputState::Indeterminate,
                )
            })?;
        sync_directory_at(&parent.dir, hooks, FaultPoint::RollbackDirectoryFlush).map_err(
            |error| {
                rollback_io_error(
                    RollbackFailureReason::Flush,
                    RegistrationSubject::OutputRoot,
                    None,
                    RegistrationOperation::Flush,
                    &error,
                    OutputState::Indeterminate,
                )
            },
        )?;
    }

    if !matches!(journal.old_root(), JournalOldRoot::Absent) {
        hooks
            .before(FaultPoint::RollbackRename)
            .and_then(|()| {
                parent.dir.rename(
                    journal.backup(),
                    &parent.dir,
                    parent.output_basename.as_ref(),
                )
            })
            .map_err(|error| {
                rollback_io_error(
                    RollbackFailureReason::OldRootRestore,
                    RegistrationSubject::Backup,
                    Some(RegistrationControl::Backup),
                    RegistrationOperation::Rename,
                    &error,
                    OutputState::Indeterminate,
                )
            })?;
        let restored = parent
            .dir
            .open_dir_nofollow(parent.output_basename.as_ref())
            .map_err(|error| {
                rollback_io_error(
                    RollbackFailureReason::OldRootRestore,
                    RegistrationSubject::OutputRoot,
                    None,
                    RegistrationOperation::Open,
                    &error,
                    OutputState::Indeterminate,
                )
            })?;
        sync_directory_at(&restored, hooks, FaultPoint::RollbackDirectoryFlush).map_err(
            |error| {
                rollback_io_error(
                    RollbackFailureReason::Flush,
                    RegistrationSubject::OutputRoot,
                    None,
                    RegistrationOperation::Flush,
                    &error,
                    OutputState::Indeterminate,
                )
            },
        )?;
        sync_directory_at(&parent.dir, hooks, FaultPoint::RollbackDirectoryFlush).map_err(
            |error| {
                rollback_io_error(
                    RollbackFailureReason::Flush,
                    RegistrationSubject::OutputRoot,
                    None,
                    RegistrationOperation::Flush,
                    &error,
                    OutputState::Indeterminate,
                )
            },
        )?;
    }

    remove_rollback_staging(parent, journal.staging(), OutputState::Restored, hooks)?;
    let control_id = OutputRootControlId::derive(journal.output_root()).map_err(|_| {
        OutputRegistrationError::internal_invariant().with_output_state(OutputState::Restored)
    })?;
    if progress.owns_journal_temporary() {
        remove_rollback_control(
            parent,
            &control_id.journal_temporary_basename(journal.transaction_id()),
            OutputState::Restored,
            hooks,
        )?;
    }
    if progress.owns_journal() {
        remove_rollback_control(
            parent,
            &control_id.journal_basename(),
            OutputState::Restored,
            hooks,
        )?;
    }
    sync_directory_at(&parent.dir, hooks, FaultPoint::RollbackDirectoryFlush).map_err(|error| {
        rollback_io_error(
            RollbackFailureReason::Flush,
            RegistrationSubject::OutputRoot,
            None,
            RegistrationOperation::Flush,
            &error,
            OutputState::Restored,
        )
    })?;
    Ok(OutputState::Restored)
}

fn remove_rollback_staging(
    parent: &OutputParent,
    basename: &str,
    output_state: OutputState,
    hooks: &dyn TransactionHooks,
) -> Result<bool, OutputRegistrationError> {
    match parent.dir.symlink_metadata(basename) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => {
            hooks
                .before(FaultPoint::RollbackCleanup)
                .and_then(|()| parent.dir.remove_dir_all(basename))
                .map_err(|error| {
                    rollback_io_error(
                        RollbackFailureReason::Cleanup,
                        RegistrationSubject::Staging,
                        Some(RegistrationControl::Staging),
                        RegistrationOperation::Remove,
                        &error,
                        output_state,
                    )
                })?;
            Ok(true)
        }
        Ok(_) => Err(rollback_error(
            RollbackFailureReason::Cleanup,
            RegistrationSubject::Staging,
            Some(RegistrationControl::Staging),
            output_state,
        )),
        Err(error) => Err(rollback_io_error(
            RollbackFailureReason::Cleanup,
            RegistrationSubject::Staging,
            Some(RegistrationControl::Staging),
            RegistrationOperation::Inspect,
            &error,
            output_state,
        )),
    }
}

fn remove_rollback_control(
    parent: &OutputParent,
    basename: &str,
    output_state: OutputState,
    hooks: &dyn TransactionHooks,
) -> Result<bool, OutputRegistrationError> {
    match parent.dir.symlink_metadata(basename) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Ok(metadata) if metadata.is_file() && !metadata.is_symlink() => {
            hooks
                .before(FaultPoint::RollbackCleanup)
                .and_then(|()| parent.dir.remove_file(basename))
                .map_err(|error| {
                    rollback_io_error(
                        RollbackFailureReason::Cleanup,
                        RegistrationSubject::Journal,
                        Some(RegistrationControl::Journal),
                        RegistrationOperation::Remove,
                        &error,
                        output_state,
                    )
                })?;
            Ok(true)
        }
        Ok(_) => Err(rollback_error(
            RollbackFailureReason::Cleanup,
            RegistrationSubject::Journal,
            Some(RegistrationControl::Journal),
            output_state,
        )),
        Err(error) => Err(rollback_io_error(
            RollbackFailureReason::Cleanup,
            RegistrationSubject::Journal,
            Some(RegistrationControl::Journal),
            RegistrationOperation::Inspect,
            &error,
            output_state,
        )),
    }
}

#[cfg(windows)]
fn sync_directory(_directory: &Dir) -> io::Result<()> {
    // Windows exposes durable regular-file flushes but no documented primitive
    // that durably publishes directory-entry changes. Treating a successful
    // file flush as a directory flush would weaken the transaction contract.
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

#[cfg(not(windows))]
fn sync_directory(directory: &Dir) -> io::Result<()> {
    // `cap_std::fs::Dir` may hold an `O_PATH` descriptor on Linux. Cloning
    // that descriptor preserves `O_PATH`, which cannot be passed to `fsync`.
    // Reopen the directory itself through the existing capability so the
    // resulting handle has read access and is suitable for durable flushes.
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    directory
        .open_with(Path::new("."), &options)
        .and_then(|syncable_directory| syncable_directory.sync_all())
        .map_err(|error| {
            if matches!(
                error.kind(),
                io::ErrorKind::Unsupported | io::ErrorKind::InvalidInput
            ) {
                io::Error::from(io::ErrorKind::Unsupported)
            } else {
                error
            }
        })
}

fn sync_directory_at(
    directory: &Dir,
    hooks: &dyn TransactionHooks,
    point: FaultPoint,
) -> io::Result<()> {
    hooks.before(point)?;
    hooks.sync_directory(directory)
}

fn sync_file_at(
    file: &cap_std::fs::File,
    hooks: &dyn TransactionHooks,
    point: FaultPoint,
) -> io::Result<()> {
    hooks.before(point)?;
    file.sync_all()
}

fn evidence_path(path: &Path) -> Option<EvidencePath> {
    EvidencePath::try_new(path.components().filter_map(|component| match component {
        std::path::Component::Normal(segment) => segment.to_str().map(Box::<str>::from),
        _ => None,
    }))
}

fn export_path_evidence(path: &ExportArtifactPath) -> Option<EvidencePath> {
    EvidencePath::try_new(
        path.segments()
            .iter()
            .map(|segment| Box::<str>::from(segment.as_str())),
    )
}

fn lock_error(
    reason: LockFailureReason,
    io: Option<RegistrationIoEvidence>,
) -> OutputRegistrationError {
    OutputRegistrationError::registration(RegistrationFailureEvidence::lock(reason, io))
}

fn lock_io_error(
    reason: LockFailureReason,
    operation: RegistrationOperation,
    error: &io::Error,
) -> OutputRegistrationError {
    if let Some(capability) = required_capability(operation, error) {
        return OutputRegistrationError::unsupported_capability(capability);
    }
    lock_error(
        reason,
        Some(RegistrationIoEvidence::from_error(operation, error)),
    )
}

#[allow(clippy::too_many_arguments)]
fn staging_io_error(
    reason: StagingFailureReason,
    subject: RegistrationSubject,
    control: Option<RegistrationControl>,
    artifact_path: Option<ExportArtifactPath>,
    relative_path: Option<EvidencePath>,
    operation: RegistrationOperation,
    error: &io::Error,
) -> OutputRegistrationError {
    if let Some(capability) = required_capability(operation, error) {
        return OutputRegistrationError::unsupported_capability(capability);
    }
    OutputRegistrationError::registration(RegistrationFailureEvidence::staging(
        reason,
        subject,
        control,
        artifact_path,
        relative_path,
        Some(RegistrationIoEvidence::from_error(operation, error)),
    ))
}

fn commit_io_error(
    reason: CommitFailureReason,
    subject: RegistrationSubject,
    control: Option<RegistrationControl>,
    operation: RegistrationOperation,
    error: &io::Error,
) -> OutputRegistrationError {
    if let Some(capability) = required_capability(operation, error) {
        return OutputRegistrationError::unsupported_capability(capability);
    }
    OutputRegistrationError::registration(RegistrationFailureEvidence::commit(
        reason,
        subject,
        control,
        Some(RegistrationIoEvidence::from_error(operation, error)),
    ))
}

fn rollback_io_error(
    reason: RollbackFailureReason,
    subject: RegistrationSubject,
    control: Option<RegistrationControl>,
    operation: RegistrationOperation,
    error: &io::Error,
    output_state: OutputState,
) -> OutputRegistrationError {
    if let Some(capability) = required_capability(operation, error) {
        return OutputRegistrationError::unsupported_capability(capability)
            .with_output_state(output_state);
    }
    OutputRegistrationError::registration(RegistrationFailureEvidence::rollback(
        reason,
        subject,
        control,
        Some(RegistrationIoEvidence::from_error(operation, error)),
    ))
    .with_output_state(output_state)
}

fn rollback_error(
    reason: RollbackFailureReason,
    subject: RegistrationSubject,
    control: Option<RegistrationControl>,
    output_state: OutputState,
) -> OutputRegistrationError {
    OutputRegistrationError::registration(RegistrationFailureEvidence::rollback(
        reason, subject, control, None,
    ))
    .with_output_state(output_state)
}

fn recovery_error(
    reason: RecoveryFailureReason,
    subject: RegistrationSubject,
    control: Option<RegistrationControl>,
    io: Option<RegistrationIoEvidence>,
) -> OutputRegistrationError {
    OutputRegistrationError::registration(RegistrationFailureEvidence::recovery(
        reason, subject, control, io,
    ))
}

fn recovery_io_error(
    reason: RecoveryFailureReason,
    subject: RegistrationSubject,
    control: Option<RegistrationControl>,
    operation: RegistrationOperation,
    error: &io::Error,
    output_state: OutputState,
) -> OutputRegistrationError {
    if let Some(capability) = required_capability(operation, error) {
        return OutputRegistrationError::unsupported_capability(capability)
            .with_output_state(output_state);
    }
    if error.kind() == io::ErrorKind::Unsupported {
        return OutputRegistrationError::unsupported_capability(
            UnsupportedCapabilityEvidence::DeterministicRecovery,
        )
        .with_output_state(output_state);
    }
    recovery_error(
        reason,
        subject,
        control,
        Some(RegistrationIoEvidence::from_error(operation, error)),
    )
    .with_output_state(output_state)
}

fn required_capability(
    operation: RegistrationOperation,
    error: &io::Error,
) -> Option<UnsupportedCapabilityEvidence> {
    if operation == RegistrationOperation::Flush && error.kind() == io::ErrorKind::Unsupported {
        return Some(UnsupportedCapabilityEvidence::DurableFlush);
    }
    if matches!(
        operation,
        RegistrationOperation::Rename | RegistrationOperation::Replace
    ) {
        if error.kind() == io::ErrorKind::CrossesDevices {
            return Some(UnsupportedCapabilityEvidence::SameFilesystemStaging);
        }
        if error.kind() == io::ErrorKind::Unsupported {
            return Some(UnsupportedCapabilityEvidence::SafeRename);
        }
        if operation == RegistrationOperation::Replace
            && error.kind() == io::ErrorKind::AlreadyExists
        {
            // Some platforms expose rename but cannot atomically replace an
            // existing journal entry. A remove-then-rename fallback would
            // create an unrecoverable publication gap.
            return Some(UnsupportedCapabilityEvidence::SafeRename);
        }
    }
    if matches!(
        operation,
        RegistrationOperation::Inspect
            | RegistrationOperation::Open
            | RegistrationOperation::Create
    ) && error.kind() == io::ErrorKind::Unsupported
    {
        return Some(UnsupportedCapabilityEvidence::FilesystemNoFollow);
    }
    None
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::fs;
    use std::path::{Path, PathBuf};

    use intlify_contract::{LinkLimits, Locale};
    use intlify_export::{
        ExportArtifact, ExportArtifactFormatVersion, ExportArtifactKind, ExportArtifactMetadata,
        ExportArtifactPath, ExportArtifactPayload, ExportArtifactSet, ExportMediaType,
    };
    use intlify_linker::{DynamicReferenceMode, LinkPolicy, PlacementPolicy};
    use tempfile::{Builder, TempDir};

    #[cfg(windows)]
    use super::write_output_with_transaction_id_source;
    use super::{
        sync_directory, write_output_with_transaction_id_source_and_hooks, FaultPoint,
        NoTransactionHooks, OutputRootControlId, TransactionHooks, TransactionId,
        WriteRegistrationOutcome, WriteRegistrationStatus,
    };
    use crate::messages::delivery::{
        DeliveryTargetInput, ResolvedDeliveryTarget, ResolvedDeliveryTargets,
    };
    use crate::messages::registration::error::{
        OutputRegistrationError, OutputRegistrationErrorEvidence, OutputState,
        UnsupportedCapabilityEvidence,
    };
    use crate::messages::registration::manifest::OUTPUT_MANIFEST_BASENAME;
    use crate::messages::registration::PreparedOutput;

    struct TempProject(TempDir);

    impl TempProject {
        fn new() -> Self {
            Self(
                Builder::new()
                    .prefix("intlify-output-transaction-")
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

        fn output_parent(&self) -> PathBuf {
            self.path().join("generated")
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn directory_flush_uses_a_syncable_capability_relative_handle() {
        let project = TempProject::new();
        let directory =
            cap_std::fs::Dir::open_ambient_dir(project.path(), cap_std::ambient_authority())
                .unwrap();

        sync_directory(&directory).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn directory_flush_reports_the_missing_windows_durability_capability() {
        let project = TempProject::new();
        let directory =
            cap_std::fs::Dir::open_ambient_dir(project.path(), cap_std::ambient_authority())
                .unwrap();

        assert_eq!(
            sync_directory(&directory).unwrap_err().kind(),
            std::io::ErrorKind::Unsupported
        );
    }

    #[cfg(windows)]
    #[test]
    fn output_transaction_preserves_the_windows_durable_flush_failure() {
        let project = TempProject::new();
        let set = artifact_set(&[(&["loader.mjs"], b"created")]);
        let target = target();
        let prepared = PreparedOutput::try_new(&target, &set).unwrap();

        let error = write_output_with_transaction_id_source(&prepared, project.path(), || {
            Ok(transaction_id(0x10))
        })
        .unwrap_err();

        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::UnsupportedCapability(
                UnsupportedCapabilityEvidence::DurableFlush
            )
        ));
        assert_eq!(error.output_state(), OutputState::Unchanged);
        assert!(!project.output().exists());
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

    fn artifact(path: &[&str], payload: &[u8]) -> ExportArtifact {
        ExportArtifact::new(
            ExportArtifactPath::try_new(path.iter().copied()).unwrap(),
            ExportArtifactKind::try_new("dev.intlify/esm-module").unwrap(),
            ExportArtifactFormatVersion::DRAFT_V0_1,
            ExportArtifactPayload::try_new(payload.to_vec()).unwrap(),
            ExportArtifactMetadata::try_new(
                Some(ExportMediaType::try_new("text/javascript").unwrap()),
                Vec::new(),
            )
            .unwrap(),
        )
    }

    fn artifact_set(records: &[(&[&str], &[u8])]) -> ExportArtifactSet {
        ExportArtifactSet::try_new(
            records
                .iter()
                .map(|(path, payload)| artifact(path, payload))
                .collect(),
        )
        .unwrap()
    }

    fn transaction_id(byte: u8) -> TransactionId {
        TransactionId::from_bytes([byte; 16])
    }

    fn write_with_id(
        prepared: &PreparedOutput<'_>,
        project: &TempProject,
        byte: u8,
    ) -> super::WriteRegistrationOutcome {
        write_with_id_result(prepared, project, byte).unwrap()
    }

    fn write_with_id_result(
        prepared: &PreparedOutput<'_>,
        project: &TempProject,
        byte: u8,
    ) -> Result<super::WriteRegistrationOutcome, OutputRegistrationError> {
        write_with_transaction_id_source(prepared, project, || Ok(transaction_id(byte)))
    }

    fn write_with_transaction_id_source(
        prepared: &PreparedOutput<'_>,
        project: &TempProject,
        transaction_id_source: impl FnOnce() -> Result<TransactionId, OutputRegistrationError>,
    ) -> Result<WriteRegistrationOutcome, OutputRegistrationError> {
        let hooks = PlatformTestHooks {
            inner: &NoTransactionHooks,
        };
        write_output_with_transaction_id_source_and_hooks(
            prepared,
            project.path(),
            transaction_id_source,
            &hooks,
        )
    }

    /// Keeps the transaction state-machine fixtures portable without claiming
    /// that Windows provides the missing production durability primitive.
    /// The real platform result is fixed by the Windows-only tests above; these
    /// fixtures model a successful directory flush so every later commit,
    /// rollback, recovery, and fault boundary remains exercised.
    struct PlatformTestHooks<'a> {
        inner: &'a dyn TransactionHooks,
    }

    impl TransactionHooks for PlatformTestHooks<'_> {
        fn before(&self, point: FaultPoint) -> std::io::Result<()> {
            self.inner.before(point)
        }

        fn sync_directory(&self, directory: &cap_std::fs::Dir) -> std::io::Result<()> {
            #[cfg(windows)]
            {
                let _ = directory;
                Ok(())
            }
            #[cfg(not(windows))]
            {
                self.inner.sync_directory(directory)
            }
        }
    }

    struct RecordingHooks {
        fail_at: Option<usize>,
        points: RefCell<Vec<FaultPoint>>,
    }

    impl RecordingHooks {
        fn recording() -> Self {
            Self {
                fail_at: None,
                points: RefCell::new(Vec::new()),
            }
        }

        fn failing_at(index: usize) -> Self {
            Self {
                fail_at: Some(index),
                points: RefCell::new(Vec::new()),
            }
        }

        fn points(&self) -> Vec<FaultPoint> {
            self.points.borrow().clone()
        }
    }

    impl TransactionHooks for RecordingHooks {
        fn before(&self, point: FaultPoint) -> std::io::Result<()> {
            let mut points = self.points.borrow_mut();
            let index = points.len();
            points.push(point);
            if self.fail_at == Some(index) {
                Err(std::io::Error::other("injected transaction failure"))
            } else {
                Ok(())
            }
        }
    }

    struct PointFailureHooks {
        remaining: RefCell<Vec<FaultPoint>>,
    }

    struct KindPointFailureHooks {
        point: FaultPoint,
        kind: std::io::ErrorKind,
        used: Cell<bool>,
    }

    impl KindPointFailureHooks {
        fn new(point: FaultPoint, kind: std::io::ErrorKind) -> Self {
            Self {
                point,
                kind,
                used: Cell::new(false),
            }
        }
    }

    impl TransactionHooks for KindPointFailureHooks {
        fn before(&self, point: FaultPoint) -> std::io::Result<()> {
            if point == self.point && !self.used.replace(true) {
                Err(std::io::Error::from(self.kind))
            } else {
                Ok(())
            }
        }
    }

    struct CreateCollisionHooks {
        point: FaultPoint,
        path: PathBuf,
        used: Cell<bool>,
    }

    impl TransactionHooks for CreateCollisionHooks {
        fn before(&self, point: FaultPoint) -> std::io::Result<()> {
            if point == self.point && !self.used.replace(true) {
                fs::write(&self.path, b"caller-owned control collision")?;
            }
            Ok(())
        }
    }

    impl PointFailureHooks {
        fn new(points: impl IntoIterator<Item = FaultPoint>) -> Self {
            Self {
                remaining: RefCell::new(points.into_iter().collect()),
            }
        }
    }

    impl TransactionHooks for PointFailureHooks {
        fn before(&self, point: FaultPoint) -> std::io::Result<()> {
            let mut remaining = self.remaining.borrow_mut();
            if remaining.first() == Some(&point) {
                remaining.remove(0);
                Err(std::io::Error::other("injected transaction failure"))
            } else {
                Ok(())
            }
        }
    }

    fn write_with_hooks(
        prepared: &PreparedOutput<'_>,
        project: &TempProject,
        byte: u8,
        hooks: &dyn TransactionHooks,
    ) -> Result<WriteRegistrationOutcome, OutputRegistrationError> {
        let hooks = PlatformTestHooks { inner: hooks };
        write_output_with_transaction_id_source_and_hooks(
            prepared,
            project.path(),
            || Ok(transaction_id(byte)),
            &hooks,
        )
    }

    fn install_old_output<'a>(
        project: &TempProject,
        target: &'a ResolvedDeliveryTarget,
        old: &'a ExportArtifactSet,
    ) -> PreparedOutput<'a> {
        let prepared = PreparedOutput::try_new(target, old).unwrap();
        write_with_id(&prepared, project, 0x71);
        prepared
    }

    fn assert_complete_old_or_new(
        project: &TempProject,
        target: &ResolvedDeliveryTarget,
        old: &ExportArtifactSet,
        new: &ExportArtifactSet,
    ) {
        let old_matches = PreparedOutput::try_new(target, old)
            .unwrap()
            .check(project.path())
            .is_ok_and(|comparison| comparison.is_matched());
        let new_matches = PreparedOutput::try_new(target, new)
            .unwrap()
            .check(project.path())
            .is_ok_and(|comparison| comparison.is_matched());
        assert!(
            old_matches || new_matches,
            "fault exposed neither the complete prior nor complete expected root"
        );
    }

    fn sibling_names(project: &TempProject) -> Vec<String> {
        let mut names = fs::read_dir(project.output_parent())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    #[test]
    fn absent_root_is_installed_as_one_complete_durable_tree() {
        let project = TempProject::new();
        let set = artifact_set(&[
            (&["locales", "en.mjs"], b"export default 'hello'"),
            (&["loader.mjs"], b"export const loader = {}"),
        ]);
        let target = target();
        let prepared = PreparedOutput::try_new(&target, &set).unwrap();

        let outcome = write_with_id(&prepared, &project, 0x11);
        assert_eq!(outcome.status(), WriteRegistrationStatus::Written);
        assert_eq!(outcome.output_state(), OutputState::Updated);
        assert_eq!(
            fs::read(project.output().join("locales/en.mjs")).unwrap(),
            b"export default 'hello'"
        );
        assert_eq!(
            fs::read(project.output().join("loader.mjs")).unwrap(),
            b"export const loader = {}"
        );
        assert_eq!(
            fs::read(project.output().join(OUTPUT_MANIFEST_BASENAME)).unwrap(),
            prepared.manifest().canonical_bytes()
        );

        let control = OutputRootControlId::derive(prepared.output_root).unwrap();
        assert_eq!(
            sibling_names(&project),
            [control.lock_basename(), "messages".to_owned()]
        );
        assert!(prepared.check(project.path()).unwrap().is_matched());
    }

    #[test]
    fn exact_write_short_circuits_before_requesting_transaction_randomness() {
        let project = TempProject::new();
        let set = artifact_set(&[(&["loader.mjs"], b"same")]);
        let target = target();
        let prepared = PreparedOutput::try_new(&target, &set).unwrap();
        write_with_id(&prepared, &project, 0x22);
        let before = sibling_names(&project);

        let outcome = write_with_transaction_id_source(&prepared, &project, || {
            panic!("an unchanged write must not request a transaction ID")
        })
        .unwrap();
        assert_eq!(outcome.status(), WriteRegistrationStatus::Unchanged);
        assert_eq!(outcome.output_state(), OutputState::Unchanged);
        assert_eq!(sibling_names(&project), before);
    }

    #[cfg(unix)]
    #[test]
    fn replaced_output_parent_is_rejected_before_staging_publication() {
        let project = TempProject::new();
        let old = artifact_set(&[(&["loader.mjs"], b"old")]);
        let new = artifact_set(&[(&["loader.mjs"], b"new")]);
        let target = target();
        install_old_output(&project, &target, &old);
        let prepared = PreparedOutput::try_new(&target, &new).unwrap();
        let displaced_parent = project.path().join("generated-displaced");

        let error = write_with_transaction_id_source(&prepared, &project, || {
            fs::rename(project.output_parent(), &displaced_parent).unwrap();
            fs::create_dir(project.output_parent()).unwrap();
            fs::write(project.output_parent().join("caller-owned"), b"keep").unwrap();
            Ok(transaction_id(0x2a))
        })
        .unwrap_err();

        assert_eq!(error.output_state(), OutputState::Indeterminate);
        assert_eq!(
            fs::read(project.output_parent().join("caller-owned")).unwrap(),
            b"keep"
        );
        assert_eq!(
            fs::read(displaced_parent.join("messages/loader.mjs")).unwrap(),
            b"old"
        );
        assert!(!displaced_parent.read_dir().unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("staging")));
    }

    #[test]
    fn managed_update_replaces_the_complete_tree_and_removes_stale_output() {
        let project = TempProject::new();
        let old = artifact_set(&[(&["loader.mjs"], b"old"), (&["stale.mjs"], b"remove me")]);
        let new = artifact_set(&[
            (&["loader.mjs"], b"new"),
            (&["nested", "fresh.mjs"], b"fresh"),
        ]);
        let target = target();
        write_with_id(
            &PreparedOutput::try_new(&target, &old).unwrap(),
            &project,
            0x33,
        );

        let prepared = PreparedOutput::try_new(&target, &new).unwrap();
        let outcome = write_with_id(&prepared, &project, 0x44);
        assert_eq!(outcome.status(), WriteRegistrationStatus::Written);
        assert_eq!(outcome.output_state(), OutputState::Updated);
        assert_eq!(
            fs::read(project.output().join("loader.mjs")).unwrap(),
            b"new"
        );
        assert_eq!(
            fs::read(project.output().join("nested/fresh.mjs")).unwrap(),
            b"fresh"
        );
        assert!(!project.output().join("stale.mjs").exists());
        assert!(prepared.check(project.path()).unwrap().is_matched());
    }

    #[test]
    fn empty_root_is_adopted_without_file_by_file_publication() {
        let project = TempProject::new();
        fs::create_dir_all(project.output()).unwrap();
        let set = artifact_set(&[(&["loader.mjs"], b"created")]);
        let target = target();
        let prepared = PreparedOutput::try_new(&target, &set).unwrap();

        let outcome = write_with_id(&prepared, &project, 0x55);
        assert_eq!(outcome.status(), WriteRegistrationStatus::Written);
        assert_eq!(outcome.output_state(), OutputState::Updated);
        assert!(prepared.check(project.path()).unwrap().is_matched());
    }

    #[test]
    fn secure_random_failure_occurs_only_for_a_required_transaction() {
        let project = TempProject::new();
        let set = artifact_set(&[(&["loader.mjs"], b"created")]);
        let target = target();
        let prepared = PreparedOutput::try_new(&target, &set).unwrap();
        let error = write_with_transaction_id_source(&prepared, &project, || {
            Err(OutputRegistrationError::unsupported_capability(
                UnsupportedCapabilityEvidence::SecureRandom,
            ))
        })
        .unwrap_err();
        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::UnsupportedCapability(
                UnsupportedCapabilityEvidence::SecureRandom
            )
        ));
        assert!(!project.output().exists());
    }

    #[test]
    fn every_success_path_flush_rename_journal_and_cleanup_boundary_is_injectable() {
        let old = artifact_set(&[(&["loader.mjs"], b"old")]);
        let new = artifact_set(&[
            (&["loader.mjs"], b"new"),
            (&["nested", "en.mjs"], b"locale"),
        ]);
        let target = target();

        let trace_project = TempProject::new();
        install_old_output(&trace_project, &target, &old);
        let expected = PreparedOutput::try_new(&target, &new).unwrap();
        let recording = RecordingHooks::recording();
        write_with_hooks(&expected, &trace_project, 0x72, &recording).unwrap();
        let points = recording.points();
        for required in [
            FaultPoint::StagingFileFlush,
            FaultPoint::StagingDirectoryFlush,
            FaultPoint::StagingParentFlush,
            FaultPoint::JournalCreate(super::JournalPhase::Staged),
            FaultPoint::JournalFileFlush(super::JournalPhase::Staged),
            FaultPoint::JournalDirectoryFlush(super::JournalPhase::Staged),
            FaultPoint::OldRootRename,
            FaultPoint::OldRootDirectoryFlush,
            FaultPoint::JournalCreate(super::JournalPhase::OldMoved),
            FaultPoint::JournalReplace(super::JournalPhase::OldMoved),
            FaultPoint::NewRootRename,
            FaultPoint::NewRootDirectoryFlush,
            FaultPoint::JournalCreate(super::JournalPhase::NewInstalled),
            FaultPoint::JournalReplace(super::JournalPhase::NewInstalled),
            FaultPoint::BackupCleanup,
            FaultPoint::BackupDirectoryFlush,
            FaultPoint::JournalCleanup,
            FaultPoint::JournalCleanupFlush,
        ] {
            assert!(
                points.contains(&required),
                "missing fault point {required:?}"
            );
        }

        for failure_index in 0..points.len() {
            let project = TempProject::new();
            install_old_output(&project, &target, &old);
            let prepared = PreparedOutput::try_new(&target, &new).unwrap();
            let hooks = RecordingHooks::failing_at(failure_index);
            let error = write_with_hooks(&prepared, &project, 0x73, &hooks).unwrap_err();
            assert_ne!(error.output_state(), OutputState::Indeterminate);
            assert_complete_old_or_new(&project, &target, &old, &new);
        }
    }

    #[test]
    fn parent_and_lock_durability_boundaries_fail_before_output_publication() {
        let set = artifact_set(&[(&["loader.mjs"], b"new")]);
        let target = target();

        let trace_project = TempProject::new();
        let prepared = PreparedOutput::try_new(&target, &set).unwrap();
        let recording = RecordingHooks::recording();
        write_with_hooks(&prepared, &trace_project, 0x81, &recording).unwrap();
        let points = recording.points();
        for required in [
            FaultPoint::ParentDirectoryFlush,
            FaultPoint::LockFileFlush,
            FaultPoint::LockDirectoryFlush,
        ] {
            assert!(
                points.contains(&required),
                "missing fault point {required:?}"
            );
        }

        for failure_index in 0..points.len() {
            let project = TempProject::new();
            let prepared = PreparedOutput::try_new(&target, &set).unwrap();
            let hooks = RecordingHooks::failing_at(failure_index);
            let result = write_with_hooks(&prepared, &project, 0x82, &hooks);
            if let Err(error) = result {
                assert_ne!(error.output_state(), OutputState::Indeterminate);
                if project.output().exists() {
                    assert!(prepared
                        .check(project.path())
                        .is_ok_and(|comparison| comparison.is_matched()));
                }
            }
        }
    }

    #[test]
    fn interrupted_rollback_is_recovered_without_guessing_partial_output() {
        let old = artifact_set(&[(&["loader.mjs"], b"old")]);
        let new = artifact_set(&[(&["loader.mjs"], b"new")]);
        let target = target();
        for rollback_fault in [
            FaultPoint::RollbackRename,
            FaultPoint::RollbackDirectoryFlush,
            FaultPoint::RollbackCleanup,
        ] {
            let project = TempProject::new();
            install_old_output(&project, &target, &old);
            let prepared = PreparedOutput::try_new(&target, &new).unwrap();
            let hooks = PointFailureHooks::new([FaultPoint::NewRootDirectoryFlush, rollback_fault]);
            let error = write_with_hooks(&prepared, &project, 0x91, &hooks).unwrap_err();
            assert!(matches!(
                error.output_state(),
                OutputState::Indeterminate | OutputState::Restored
            ));

            let outcome = write_with_id(&prepared, &project, 0x92);
            assert!(matches!(
                outcome.status(),
                WriteRegistrationStatus::Written | WriteRegistrationStatus::Unchanged
            ));
            assert!(prepared.check(project.path()).unwrap().is_matched());
        }
    }

    #[test]
    fn rollback_failures_report_the_strongest_proven_output_state() {
        let old = artifact_set(&[(&["loader.mjs"], b"old")]);
        let new = artifact_set(&[(&["loader.mjs"], b"new")]);
        let target = target();

        let unchanged_project = TempProject::new();
        install_old_output(&unchanged_project, &target, &old);
        let prepared = PreparedOutput::try_new(&target, &new).unwrap();
        let unchanged_error = write_with_hooks(
            &prepared,
            &unchanged_project,
            0x93,
            &PointFailureHooks::new([FaultPoint::OldRootRename, FaultPoint::RollbackCleanup]),
        )
        .unwrap_err();
        assert_eq!(unchanged_error.output_state(), OutputState::Unchanged);
        assert!(PreparedOutput::try_new(&target, &old)
            .unwrap()
            .check(unchanged_project.path())
            .is_ok_and(|comparison| comparison.is_matched()));

        let restored_project = TempProject::new();
        install_old_output(&restored_project, &target, &old);
        let restored_error = write_with_hooks(
            &prepared,
            &restored_project,
            0x94,
            &PointFailureHooks::new([
                FaultPoint::NewRootDirectoryFlush,
                FaultPoint::RollbackCleanup,
            ]),
        )
        .unwrap_err();
        assert_eq!(restored_error.output_state(), OutputState::Restored);
        assert!(PreparedOutput::try_new(&target, &old)
            .unwrap()
            .check(restored_project.path())
            .is_ok_and(|comparison| comparison.is_matched()));

        let indeterminate_project = TempProject::new();
        install_old_output(&indeterminate_project, &target, &old);
        let indeterminate_error = write_with_hooks(
            &prepared,
            &indeterminate_project,
            0x95,
            &PointFailureHooks::new([
                FaultPoint::NewRootDirectoryFlush,
                FaultPoint::RollbackRename,
            ]),
        )
        .unwrap_err();
        assert_eq!(
            indeterminate_error.output_state(),
            OutputState::Indeterminate
        );
    }

    #[derive(Debug, Clone, Copy)]
    enum RecoveryScenario {
        OldAtOutputWithStaging,
        MissingOutputWithBackupAndStaging,
        NewAtOutputWithBackup,
        NewAtOutputWithoutBackup,
        NewInstalledMissingOutput,
    }

    fn leave_interrupted_state(
        project: &TempProject,
        target: &ResolvedDeliveryTarget,
        old: &ExportArtifactSet,
        new: &ExportArtifactSet,
        scenario: RecoveryScenario,
    ) {
        let prepared = PreparedOutput::try_new(target, new).unwrap();
        let hooks = match scenario {
            RecoveryScenario::OldAtOutputWithStaging => {
                PointFailureHooks::new([FaultPoint::OldRootRename, FaultPoint::RollbackCleanup])
            }
            RecoveryScenario::MissingOutputWithBackupAndStaging => {
                PointFailureHooks::new([FaultPoint::NewRootRename, FaultPoint::RollbackRename])
            }
            RecoveryScenario::NewAtOutputWithBackup
            | RecoveryScenario::NewInstalledMissingOutput => {
                PointFailureHooks::new([FaultPoint::BackupCleanup, FaultPoint::RollbackRename])
            }
            RecoveryScenario::NewAtOutputWithoutBackup => {
                PointFailureHooks::new([FaultPoint::JournalCleanup])
            }
        };
        if !matches!(scenario, RecoveryScenario::NewAtOutputWithoutBackup) {
            install_old_output(project, target, old);
        }
        write_with_hooks(&prepared, project, 0xa1, &hooks).unwrap_err();
        if matches!(scenario, RecoveryScenario::NewInstalledMissingOutput) {
            fs::remove_dir_all(project.output()).unwrap();
        }
    }

    fn recovery_expected<'a>(
        scenario: RecoveryScenario,
        old: &'a ExportArtifactSet,
        new: &'a ExportArtifactSet,
    ) -> (&'a ExportArtifactSet, OutputState) {
        match scenario {
            RecoveryScenario::OldAtOutputWithStaging
            | RecoveryScenario::MissingOutputWithBackupAndStaging
            | RecoveryScenario::NewInstalledMissingOutput => (old, OutputState::Restored),
            RecoveryScenario::NewAtOutputWithBackup
            | RecoveryScenario::NewAtOutputWithoutBackup => (new, OutputState::Updated),
        }
    }

    fn recover_without_new_transaction(
        prepared: &PreparedOutput<'_>,
        project: &TempProject,
        hooks: &dyn TransactionHooks,
    ) -> Result<WriteRegistrationOutcome, OutputRegistrationError> {
        let hooks = PlatformTestHooks { inner: hooks };
        write_output_with_transaction_id_source_and_hooks(
            prepared,
            project.path(),
            || panic!("a completed recovery must short-circuit before a new transaction"),
            &hooks,
        )
    }

    #[test]
    fn every_supported_interrupted_state_recovers_to_one_proved_root() {
        let old = artifact_set(&[(&["loader.mjs"], b"old")]);
        let new = artifact_set(&[(&["loader.mjs"], b"new")]);
        let target = target();
        for scenario in [
            RecoveryScenario::OldAtOutputWithStaging,
            RecoveryScenario::MissingOutputWithBackupAndStaging,
            RecoveryScenario::NewAtOutputWithBackup,
            RecoveryScenario::NewAtOutputWithoutBackup,
            RecoveryScenario::NewInstalledMissingOutput,
        ] {
            let project = TempProject::new();
            leave_interrupted_state(&project, &target, &old, &new, scenario);
            let (expected, state) = recovery_expected(scenario, &old, &new);
            let prepared = PreparedOutput::try_new(&target, expected).unwrap();
            let outcome =
                recover_without_new_transaction(&prepared, &project, &super::NoTransactionHooks)
                    .unwrap();
            assert_eq!(outcome.status(), WriteRegistrationStatus::Unchanged);
            assert_eq!(outcome.output_state(), state, "scenario {scenario:?}");
            assert!(prepared.check(project.path()).unwrap().is_matched());
        }
    }

    #[test]
    fn every_recovery_mutation_boundary_can_resume_after_another_failure() {
        let old = artifact_set(&[(&["loader.mjs"], b"old")]);
        let new = artifact_set(&[(&["loader.mjs"], b"new")]);
        let target = target();
        for scenario in [
            RecoveryScenario::OldAtOutputWithStaging,
            RecoveryScenario::MissingOutputWithBackupAndStaging,
            RecoveryScenario::NewAtOutputWithBackup,
            RecoveryScenario::NewAtOutputWithoutBackup,
            RecoveryScenario::NewInstalledMissingOutput,
        ] {
            let trace_project = TempProject::new();
            leave_interrupted_state(&trace_project, &target, &old, &new, scenario);
            let (expected, _) = recovery_expected(scenario, &old, &new);
            let prepared = PreparedOutput::try_new(&target, expected).unwrap();
            let recording = RecordingHooks::recording();
            recover_without_new_transaction(&prepared, &trace_project, &recording).unwrap();
            let recovery_points = recording
                .points()
                .into_iter()
                .filter(|point| {
                    matches!(
                        point,
                        FaultPoint::RecoveryRename
                            | FaultPoint::RecoveryCleanup
                            | FaultPoint::RecoveryDirectoryFlush
                    )
                })
                .collect::<Vec<_>>();
            assert!(!recovery_points.is_empty(), "scenario {scenario:?}");

            for failure_index in 0..recovery_points.len() {
                let project = TempProject::new();
                leave_interrupted_state(&project, &target, &old, &new, scenario);
                let prepared = PreparedOutput::try_new(&target, expected).unwrap();
                let hooks = RecordingHooks::failing_at(failure_index);
                recover_without_new_transaction(&prepared, &project, &hooks).unwrap_err();
                let outcome = recover_without_new_transaction(
                    &prepared,
                    &project,
                    &super::NoTransactionHooks,
                )
                .unwrap();
                assert_eq!(outcome.status(), WriteRegistrationStatus::Unchanged);
                assert!(prepared.check(project.path()).unwrap().is_matched());
            }
        }
    }

    #[test]
    fn ambiguous_recovery_state_is_preserved_without_mutation() {
        let old = artifact_set(&[(&["loader.mjs"], b"old")]);
        let new = artifact_set(&[(&["loader.mjs"], b"new")]);
        let target = target();
        let project = TempProject::new();
        leave_interrupted_state(
            &project,
            &target,
            &old,
            &new,
            RecoveryScenario::OldAtOutputWithStaging,
        );
        let control = OutputRootControlId::derive(target.out()).unwrap();
        let staging = project
            .output_parent()
            .join(control.staging_basename(transaction_id(0xa1)));
        let journal = project.output_parent().join(control.journal_basename());
        fs::write(staging.join("loader.mjs"), b"externally modified").unwrap();
        let before_staging = fs::read(staging.join("loader.mjs")).unwrap();
        let before_journal = fs::read(&journal).unwrap();

        let prepared = PreparedOutput::try_new(&target, &old).unwrap();
        let error = write_with_id_result(&prepared, &project, 0xa2).unwrap_err();
        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::Registration(
                crate::messages::registration::error::RegistrationFailureEvidence::Recovery {
                    reason:
                        crate::messages::registration::error::RecoveryFailureReason::StateAmbiguous,
                    ..
                }
            )
        ));
        assert_eq!(
            fs::read(staging.join("loader.mjs")).unwrap(),
            before_staging
        );
        assert_eq!(fs::read(journal).unwrap(), before_journal);
        assert!(PreparedOutput::try_new(&target, &old)
            .unwrap()
            .check(project.path())
            .unwrap()
            .is_matched());
    }

    #[test]
    fn two_complete_new_roots_are_ambiguous_and_preserved() {
        let old = artifact_set(&[(&["loader.mjs"], b"old")]);
        let new = artifact_set(&[(&["loader.mjs"], b"new")]);
        let target = target();
        let project = TempProject::new();
        leave_interrupted_state(
            &project,
            &target,
            &old,
            &new,
            RecoveryScenario::NewAtOutputWithBackup,
        );
        let control = OutputRootControlId::derive(target.out()).unwrap();
        let staging = project
            .output_parent()
            .join(control.staging_basename(transaction_id(0xa1)));
        fs::create_dir(&staging).unwrap();
        fs::copy(
            project.output().join("loader.mjs"),
            staging.join("loader.mjs"),
        )
        .unwrap();
        fs::copy(
            project.output().join(OUTPUT_MANIFEST_BASENAME),
            staging.join(OUTPUT_MANIFEST_BASENAME),
        )
        .unwrap();

        let prepared = PreparedOutput::try_new(&target, &new).unwrap();
        let error = write_with_id_result(&prepared, &project, 0xa2).unwrap_err();
        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::Registration(
                crate::messages::registration::error::RegistrationFailureEvidence::Recovery {
                    reason:
                        crate::messages::registration::error::RecoveryFailureReason::StateAmbiguous,
                    ..
                }
            )
        ));
        assert_eq!(
            fs::read(project.output().join("loader.mjs")).unwrap(),
            b"new"
        );
        assert_eq!(fs::read(staging.join("loader.mjs")).unwrap(), b"new");
    }

    #[test]
    fn check_mode_does_not_create_output_or_control_state() {
        let project = TempProject::new();
        let set = artifact_set(&[(&["loader.mjs"], b"expected")]);
        let target = target();
        let prepared = PreparedOutput::try_new(&target, &set).unwrap();

        assert!(!prepared.check(project.path()).unwrap().is_matched());
        assert!(fs::read_dir(project.path()).unwrap().next().is_none());
    }

    #[test]
    fn invalid_persistent_lock_is_preserved_and_blocks_registration() {
        let project = TempProject::new();
        let set = artifact_set(&[(&["loader.mjs"], b"old")]);
        let target = target();
        let prepared = install_old_output(&project, &target, &set);
        let control = OutputRootControlId::derive(target.out()).unwrap();
        let lock = project.output_parent().join(control.lock_basename());
        fs::write(&lock, b"invalid lock record").unwrap();

        let error = write_with_id_result(&prepared, &project, 0xb1).unwrap_err();
        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::Registration(
                crate::messages::registration::error::RegistrationFailureEvidence::Lock {
                    reason: crate::messages::registration::error::LockFailureReason::ControlInvalid,
                    ..
                }
            )
        ));
        assert_eq!(fs::read(lock).unwrap(), b"invalid lock record");
        assert_eq!(
            fs::read(project.output().join("loader.mjs")).unwrap(),
            b"old"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_lock_is_rejected_without_following_its_target() {
        use std::os::unix::fs::symlink;

        let project = TempProject::new();
        fs::create_dir(project.output_parent()).unwrap();
        let set = artifact_set(&[(&["loader.mjs"], b"new")]);
        let target = target();
        let prepared = PreparedOutput::try_new(&target, &set).unwrap();
        let control = OutputRootControlId::derive(target.out()).unwrap();
        let outside = project.path().join("caller-owned-lock-target");
        fs::write(&outside, b"keep").unwrap();
        symlink(
            &outside,
            project.output_parent().join(control.lock_basename()),
        )
        .unwrap();

        for error in [
            prepared.check(project.path()).unwrap_err(),
            write_with_id_result(&prepared, &project, 0xb5).unwrap_err(),
        ] {
            assert!(matches!(
                error.evidence(),
                OutputRegistrationErrorEvidence::Registration(
                    crate::messages::registration::error::RegistrationFailureEvidence::Lock {
                        reason:
                            crate::messages::registration::error::LockFailureReason::ControlConflict,
                        ..
                    }
                )
            ));
        }
        assert_eq!(fs::read(outside).unwrap(), b"keep");
        assert!(!project.output().exists());
    }

    #[test]
    fn shared_checks_and_exclusive_writes_wait_for_the_persistent_lock() {
        use std::sync::mpsc;
        use std::time::Duration;

        let project = TempProject::new();
        let old = artifact_set(&[(&["loader.mjs"], b"old")]);
        let new = artifact_set(&[(&["loader.mjs"], b"new")]);
        let target = target();
        let old_prepared = install_old_output(&project, &target, &old);
        let new_prepared = PreparedOutput::try_new(&target, &new).unwrap();
        let control = OutputRootControlId::derive(target.out()).unwrap();
        let lock_path = project.output_parent().join(control.lock_basename());

        let exclusive = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path)
            .unwrap();
        exclusive.lock().unwrap();
        std::thread::scope(|scope| {
            let (started_tx, started_rx) = mpsc::channel();
            let (result_tx, result_rx) = mpsc::channel();
            let project = &project;
            let prepared = &old_prepared;
            scope.spawn(move || {
                started_tx.send(()).unwrap();
                result_tx.send(prepared.check(project.path())).unwrap();
            });
            started_rx.recv().unwrap();
            assert!(result_rx.recv_timeout(Duration::from_millis(75)).is_err());
            std::fs::File::unlock(&exclusive).unwrap();
            assert!(result_rx
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .unwrap()
                .is_matched());
        });

        let shared = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(lock_path)
            .unwrap();
        shared.lock_shared().unwrap();
        std::thread::scope(|scope| {
            let (started_tx, started_rx) = mpsc::channel();
            let (result_tx, result_rx) = mpsc::channel();
            let project = &project;
            let prepared = &new_prepared;
            scope.spawn(move || {
                started_tx.send(()).unwrap();
                result_tx
                    .send(write_with_id_result(prepared, project, 0xb6))
                    .unwrap();
            });
            started_rx.recv().unwrap();
            assert!(result_rx.recv_timeout(Duration::from_millis(75)).is_err());
            std::fs::File::unlock(&shared).unwrap();
            assert_eq!(
                result_rx
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap()
                    .unwrap()
                    .status(),
                WriteRegistrationStatus::Written
            );
        });
    }

    #[test]
    fn invalid_journal_is_not_treated_as_recovery_authority() {
        let project = TempProject::new();
        let old = artifact_set(&[(&["loader.mjs"], b"old")]);
        let new = artifact_set(&[(&["loader.mjs"], b"new")]);
        let target = target();
        install_old_output(&project, &target, &old);
        let control = OutputRootControlId::derive(target.out()).unwrap();
        let journal = project.output_parent().join(control.journal_basename());
        fs::write(&journal, b"invalid journal").unwrap();

        let prepared = PreparedOutput::try_new(&target, &new).unwrap();
        let error = write_with_id_result(&prepared, &project, 0xb2).unwrap_err();
        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::Registration(
                crate::messages::registration::error::RegistrationFailureEvidence::Recovery {
                    reason:
                        crate::messages::registration::error::RecoveryFailureReason::JournalInvalid,
                    ..
                }
            )
        ));
        assert_eq!(fs::read(journal).unwrap(), b"invalid journal");
        assert!(PreparedOutput::try_new(&target, &old)
            .unwrap()
            .check(project.path())
            .unwrap()
            .is_matched());
    }

    #[test]
    fn derived_control_name_collision_is_preserved_before_staging() {
        let project = TempProject::new();
        let old = artifact_set(&[(&["loader.mjs"], b"old")]);
        let new = artifact_set(&[(&["loader.mjs"], b"new")]);
        let target = target();
        install_old_output(&project, &target, &old);
        let control = OutputRootControlId::derive(target.out()).unwrap();
        let backup = project
            .output_parent()
            .join(control.backup_basename(transaction_id(0xb3)));
        fs::create_dir(&backup).unwrap();
        fs::write(backup.join("caller-owned"), b"keep").unwrap();

        let prepared = PreparedOutput::try_new(&target, &new).unwrap();
        let error = write_with_id_result(&prepared, &project, 0xb3).unwrap_err();
        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::Registration(
                crate::messages::registration::error::RegistrationFailureEvidence::Staging {
                    reason:
                        crate::messages::registration::error::StagingFailureReason::ControlConflict,
                    subject: crate::messages::registration::error::RegistrationSubject::Backup,
                    ..
                }
            )
        ));
        assert_eq!(fs::read(backup.join("caller-owned")).unwrap(), b"keep");
        assert!(PreparedOutput::try_new(&target, &old)
            .unwrap()
            .check(project.path())
            .unwrap()
            .is_matched());
    }

    #[test]
    fn control_entry_created_during_commit_is_never_deleted_as_owned() {
        let old = artifact_set(&[(&["loader.mjs"], b"old")]);
        let new = artifact_set(&[(&["loader.mjs"], b"new")]);
        let target = target();
        for (point, temporary) in [
            (
                FaultPoint::JournalCreate(super::JournalPhase::Staged),
                false,
            ),
            (
                FaultPoint::JournalCreate(super::JournalPhase::OldMoved),
                true,
            ),
        ] {
            let project = TempProject::new();
            install_old_output(&project, &target, &old);
            let prepared = PreparedOutput::try_new(&target, &new).unwrap();
            let control = OutputRootControlId::derive(target.out()).unwrap();
            let path = if temporary {
                project
                    .output_parent()
                    .join(control.journal_temporary_basename(transaction_id(0xb7)))
            } else {
                project.output_parent().join(control.journal_basename())
            };
            let hooks = CreateCollisionHooks {
                point,
                path: path.clone(),
                used: Cell::new(false),
            };

            write_with_hooks(&prepared, &project, 0xb7, &hooks).unwrap_err();
            assert_eq!(fs::read(path).unwrap(), b"caller-owned control collision");
            assert!(PreparedOutput::try_new(&target, &old)
                .unwrap()
                .check(project.path())
                .unwrap()
                .is_matched());
        }
    }

    #[test]
    fn failed_staging_is_removed_without_touching_the_prior_root() {
        let project = TempProject::new();
        let old = artifact_set(&[(&["loader.mjs"], b"old")]);
        let new = artifact_set(&[(&["loader.mjs"], b"new")]);
        let target = target();
        install_old_output(&project, &target, &old);
        let prepared = PreparedOutput::try_new(&target, &new).unwrap();
        let hooks = PointFailureHooks::new([FaultPoint::StagingFileFlush]);

        write_with_hooks(&prepared, &project, 0xb4, &hooks).unwrap_err();
        let control = OutputRootControlId::derive(target.out()).unwrap();
        assert_eq!(
            sibling_names(&project),
            [control.lock_basename(), "messages".to_owned()]
        );
        assert!(PreparedOutput::try_new(&target, &old)
            .unwrap()
            .check(project.path())
            .unwrap()
            .is_matched());
    }

    #[test]
    fn unsupported_filesystem_primitives_keep_their_typed_capabilities() {
        let set = artifact_set(&[(&["loader.mjs"], b"new")]);
        let target = target();

        let project = TempProject::new();
        let prepared = PreparedOutput::try_new(&target, &set).unwrap();
        let hooks =
            KindPointFailureHooks::new(FaultPoint::LockAcquire, std::io::ErrorKind::Unsupported);
        let error = write_with_hooks(&prepared, &project, 0xc1, &hooks).unwrap_err();
        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::UnsupportedCapability(
                UnsupportedCapabilityEvidence::ProcessLocking
            )
        ));

        for (kind, capability) in [
            (
                std::io::ErrorKind::Unsupported,
                UnsupportedCapabilityEvidence::SafeRename,
            ),
            (
                std::io::ErrorKind::CrossesDevices,
                UnsupportedCapabilityEvidence::SameFilesystemStaging,
            ),
        ] {
            let project = TempProject::new();
            let old = artifact_set(&[(&["loader.mjs"], b"old")]);
            install_old_output(&project, &target, &old);
            let prepared = PreparedOutput::try_new(&target, &set).unwrap();
            let hooks = KindPointFailureHooks::new(FaultPoint::OldRootRename, kind);
            let error = write_with_hooks(&prepared, &project, 0xc2, &hooks).unwrap_err();
            assert!(matches!(
                error.evidence(),
                OutputRegistrationErrorEvidence::UnsupportedCapability(actual)
                    if *actual == capability
            ));
        }

        assert_eq!(
            super::required_capability(
                super::RegistrationOperation::Flush,
                &std::io::Error::from(std::io::ErrorKind::Unsupported),
            ),
            Some(UnsupportedCapabilityEvidence::DurableFlush)
        );
        assert_eq!(
            super::required_capability(
                super::RegistrationOperation::Open,
                &std::io::Error::from(std::io::ErrorKind::Unsupported),
            ),
            Some(UnsupportedCapabilityEvidence::FilesystemNoFollow)
        );
        assert_eq!(
            super::required_capability(
                super::RegistrationOperation::Replace,
                &std::io::Error::from(std::io::ErrorKind::AlreadyExists),
            ),
            Some(UnsupportedCapabilityEvidence::SafeRename)
        );

        let project = TempProject::new();
        let old = artifact_set(&[(&["loader.mjs"], b"old")]);
        leave_interrupted_state(
            &project,
            &target,
            &old,
            &set,
            RecoveryScenario::OldAtOutputWithStaging,
        );
        let prepared = PreparedOutput::try_new(&target, &old).unwrap();
        let hooks = KindPointFailureHooks::new(
            FaultPoint::RecoveryCleanup,
            std::io::ErrorKind::Unsupported,
        );
        let error = recover_without_new_transaction(&prepared, &project, &hooks).unwrap_err();
        assert!(matches!(
            error.evidence(),
            OutputRegistrationErrorEvidence::UnsupportedCapability(
                UnsupportedCapabilityEvidence::DeterministicRecovery
            )
        ));
    }
}
