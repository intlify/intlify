// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The complete applicable Build facts of a measured subject.
//!
//! Source and lock snapshots are observations, not executable attestations: a
//! recorded digest says what was read, never that the running binary was built
//! from it. A field this owner cannot attest stays unavailable with its reason.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use intlify_shared_json::quantity::{Quantity, Repetitions};

use crate::environment::{BuildConfiguration, Observation};
use crate::identity::{
    IntegrityDigest, NativeChecksum, OwnerLabel, RecordIdentity, VersionedIdentity,
};
use crate::reason::{BuildField, MissingObservation, Reason, Selector};

/// Whether a working tree matched its recorded source control state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SourceControlState {
    Clean,
    Dirty,
}

/// One complete observation of the source a subject was built from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceContent {
    pub algorithm: OwnerLabel,
    pub framing: OwnerLabel,
    pub digest: NativeChecksum,
    pub files: Repetitions,
    pub bytes: Quantity,
}

/// One complete observation of the dependency lock a subject resolved against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LockContent {
    pub algorithm: OwnerLabel,
    pub representation: OwnerLabel,
    pub digest: NativeChecksum,
    pub bytes: Quantity,
}

/// Build the reasons one absent Build field carries.
#[must_use]
pub fn missing(
    parent: &RecordIdentity,
    field: BuildField,
    cause: MissingObservation,
) -> Vec<Reason> {
    vec![Reason::missing_observation(
        Selector::BuildField {
            record_identity: parent.clone(),
            field,
        },
        cause,
    )]
}

/// The complete applicable Build facts one Evidence Set carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Build {
    pub source_content: Observation<SourceContent>,
    pub implementation: VersionedIdentity,
    pub executable: Observation<IntegrityDigest>,
    pub dependency_lock: Observation<LockContent>,
    #[serde(rename = "buildConfiguration")]
    pub configuration: Observation<BuildConfiguration>,
    pub physical_engine: OwnerLabel,
    pub source_control_state: Observation<SourceControlState>,
    pub measurement_profile: VersionedIdentity,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::CommonDomain;
    use crate::reason::valid_reasons;

    #[test]
    fn an_unattested_field_carries_its_reason_and_names_the_record_it_belongs_to() {
        let parent = RecordIdentity::fresh(CommonDomain::Record).unwrap();
        let reasons = missing(
            &parent,
            BuildField::Executable,
            MissingObservation::ExecutableNotAttested,
        );
        assert!(valid_reasons(&reasons));
        assert_eq!(
            serde_json::to_value(reasons[0].affected()).unwrap(),
            serde_json::json!({
                "kind": "build-field",
                "recordIdentity": serde_json::to_value(&parent).unwrap(),
                "field": "executable"
            })
        );
    }

    #[test]
    fn a_source_observation_records_its_framing_beside_its_digest() {
        // Equal digests under different framings are not the same observation,
        // so the framing travels with the value rather than being assumed.
        let content = SourceContent {
            algorithm: OwnerLabel::literal("blake3-256"),
            framing: OwnerLabel::literal("intlify-measurement-test-source/0"),
            digest: NativeChecksum::from_bytes([0x11; 32]),
            files: Repetitions::new(3).unwrap(),
            bytes: Quantity::new(4096),
        };
        let value = serde_json::to_value(&content).unwrap();
        assert_eq!(value["digest"], "11".repeat(32));
        assert_eq!(value["files"], "3");
        assert_eq!(value["bytes"], "4096");
        assert_eq!(
            serde_json::from_value::<SourceContent>(value).unwrap(),
            content
        );
    }
}
