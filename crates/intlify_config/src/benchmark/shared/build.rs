// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! This owner's Build observations, projected into 026's applicable fields.
//!
//! The field set, the observation states, and the absence rule belong to
//! `intlify_measurement`. What this module decides is what this owner acquired
//! and, for each field it cannot attest, the exact reason it is absent.

use intlify_measurement::build::{missing, LockContent, SourceContent, SourceControlState};
use intlify_measurement::environment::Observation;

pub(super) use intlify_measurement::build::Build;

use super::identity::{
    IdentityFailure, NativeChecksum, OwnerLabel, RecordIdentity, VersionedIdentity,
};
use super::reason::{BuildField, MissingObservation};
use crate::benchmark::build::Acquisition;
use crate::benchmark::run::ProjectionSource;

fn algorithm() -> OwnerLabel {
    OwnerLabel::literal("blake3-256")
}

// Generic over the field's own value type: each Build field admits its own, so
// a single closure cannot stand in for all of them.
fn unavailable<T>(
    parent: &RecordIdentity,
    field: BuildField,
    cause: MissingObservation,
) -> Observation<T> {
    Observation::Unavailable {
        reasons: missing(parent, field, cause),
    }
}

/// Project this owner's acquired build observations into the common fields.
pub(super) fn project(
    source: &ProjectionSource<'_>,
    parent: &RecordIdentity,
) -> Result<Build, IdentityFailure> {
    let context = &source.document().result().context;
    let input = &context.build;
    let (profile, revision) = context.profile.identity_revision();
    Ok(Build {
        source_content: match &input.source {
            Acquisition::Observed { value } => Observation::Observed {
                value: SourceContent {
                    algorithm: algorithm(),
                    framing: OwnerLabel::literal("intlify-config-build-source/0"),
                    digest: NativeChecksum::from_bytes(value.digest.bytes()),
                    files: value.files,
                    bytes: value.bytes,
                },
            },
            Acquisition::Unavailable { .. } => unavailable(
                parent,
                BuildField::SourceContent,
                MissingObservation::NativeAcquisitionUnavailable,
            ),
        },
        implementation: VersionedIdentity::new(&input.package.identity, &input.package.revision)?,
        executable: unavailable(
            parent,
            BuildField::Executable,
            MissingObservation::ExecutableNotAttested,
        ),
        dependency_lock: match &input.dependency_lock {
            Acquisition::Observed { value } => Observation::Observed {
                value: LockContent {
                    algorithm: algorithm(),
                    representation: OwnerLabel::literal("complete-lock-file-octets"),
                    digest: NativeChecksum::from_bytes(value.digest.bytes()),
                    bytes: value.bytes,
                },
            },
            Acquisition::Unavailable { .. } => unavailable(
                parent,
                BuildField::DependencyLock,
                MissingObservation::NativeAcquisitionUnavailable,
            ),
        },
        configuration: unavailable(
            parent,
            BuildField::BuildConfiguration,
            MissingObservation::EffectiveBuildNotAttested,
        ),
        physical_engine: OwnerLabel::literal("rust-native-component"),
        source_control_state: unavailable::<SourceControlState>(
            parent,
            BuildField::SourceControlState,
            MissingObservation::SourceControlStateNotAttested,
        ),
        measurement_profile: VersionedIdentity::new(profile, revision)?,
    })
}

/// Rebuild the expected Build facts from independently checked owner inputs.
pub(super) fn validate(
    build: &Build,
    source: &ProjectionSource<'_>,
    parent: &RecordIdentity,
) -> bool {
    project(source, parent).is_ok_and(|expected| expected == *build)
}
