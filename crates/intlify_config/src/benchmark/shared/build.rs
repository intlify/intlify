// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Complete applicable Build facts for the private native component subject.
//! Source/lock snapshots remain observations, not executable attestations.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::environment::{BuildConfiguration, Observation};
use super::identity::{IdentityFailure, IntegrityDigest, RecordIdentity, VersionedIdentity};
use super::reason::{BuildField, MissingObservation, Reason, Selector};
use crate::benchmark::build::Acquisition;
use crate::benchmark::observation::Digest;
use crate::benchmark::quantity::{Quantity, Repetitions};
use crate::benchmark::run::ProjectionSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
enum Blake3 {
    #[serde(rename = "blake3-256")]
    Value,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
enum SourceFraming {
    #[serde(rename = "intlify-config-build-source/0")]
    Value,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
enum LockRepresentation {
    #[serde(rename = "complete-lock-file-octets")]
    Value,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
enum PhysicalEngine {
    #[serde(rename = "rust-native-component")]
    Value,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
enum SourceControlState {
    Clean,
    Dirty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SourceContent {
    algorithm: Blake3,
    framing: SourceFraming,
    digest: Digest,
    files: Repetitions,
    bytes: Quantity,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LockContent {
    algorithm: Blake3,
    representation: LockRepresentation,
    digest: Digest,
    bytes: Quantity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Build {
    source_content: Observation<SourceContent>,
    implementation: VersionedIdentity,
    executable: Observation<IntegrityDigest>,
    dependency_lock: Observation<LockContent>,
    #[serde(rename = "buildConfiguration")]
    configuration: Observation<BuildConfiguration>,
    physical_engine: PhysicalEngine,
    source_control_state: Observation<SourceControlState>,
    measurement_profile: VersionedIdentity,
}

fn missing<T>(
    parent: &RecordIdentity,
    field: BuildField,
    cause: MissingObservation,
) -> Observation<T> {
    Observation::Unavailable {
        reasons: vec![Reason::missing_observation(
            Selector::BuildField {
                record_identity: parent.clone(),
                field,
            },
            cause,
        )],
    }
}

impl Build {
    pub(super) fn project(
        source: &ProjectionSource<'_>,
        parent: &RecordIdentity,
    ) -> Result<Self, IdentityFailure> {
        let context = &source.document().result().context;
        let input = &context.build;
        let (profile, revision) = context.profile.identity_revision();
        Ok(Self {
            source_content: match &input.source {
                Acquisition::Observed { value } => Observation::Observed {
                    value: SourceContent {
                        algorithm: Blake3::Value,
                        framing: SourceFraming::Value,
                        digest: value.digest,
                        files: value.files,
                        bytes: value.bytes,
                    },
                },
                Acquisition::Unavailable { .. } => missing(
                    parent,
                    BuildField::SourceContent,
                    MissingObservation::NativeAcquisitionUnavailable,
                ),
            },
            implementation: VersionedIdentity::new(
                &input.package.identity,
                &input.package.revision,
            )?,
            executable: missing(
                parent,
                BuildField::Executable,
                MissingObservation::ExecutableNotAttested,
            ),
            dependency_lock: match &input.dependency_lock {
                Acquisition::Observed { value } => Observation::Observed {
                    value: LockContent {
                        algorithm: Blake3::Value,
                        representation: LockRepresentation::Value,
                        digest: value.digest,
                        bytes: value.bytes,
                    },
                },
                Acquisition::Unavailable { .. } => missing(
                    parent,
                    BuildField::DependencyLock,
                    MissingObservation::NativeAcquisitionUnavailable,
                ),
            },
            configuration: missing(
                parent,
                BuildField::BuildConfiguration,
                MissingObservation::EffectiveBuildNotAttested,
            ),
            physical_engine: PhysicalEngine::Value,
            source_control_state: missing(
                parent,
                BuildField::SourceControlState,
                MissingObservation::SourceControlStateNotAttested,
            ),
            measurement_profile: VersionedIdentity::new(profile, revision)?,
        })
    }

    pub(super) fn validate(&self, source: &ProjectionSource<'_>, parent: &RecordIdentity) -> bool {
        Self::project(source, parent).is_ok_and(|expected| expected == *self)
    }
}
