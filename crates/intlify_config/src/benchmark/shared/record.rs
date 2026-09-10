// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Closed common records for the initial observational projection. Decoding or
//! integrity verification alone does not admit references or measurement meaning.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use super::encoding::{self, EncodingFailure};
use super::identity::{InstanceDomain, IntegrityDigest, RecordIdentity, VersionedIdentity};
use crate::benchmark::quantity::Quantity;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(super) enum RevisionZero {
    #[serde(rename = "0")]
    Value,
}

macro_rules! record_kind {
    ($name:ident, $value:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        pub(super) enum $name {
            #[serde(rename = $value)]
            Value,
        }
    };
}
record_kind!(EvidenceKind, "measurement-evidence-set");
record_kind!(EvaluationKind, "measurement-run-evaluation");
record_kind!(ReportKind, "structured-report");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreationContext {
    created_at_unix_nanoseconds: Quantity,
}

fn present<'de, D: Deserializer<'de>>(decoder: D) -> Result<Option<CreationContext>, D::Error> {
    CreationContext::deserialize(decoder).map(Some)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Envelope<K> {
    record_kind: K,
    record_schema_revision: RevisionZero,
    governing_specification: VersionedIdentity,
    record_identity: RecordIdentity,
    integrity_digest: IntegrityDigest,
    producing_tool: VersionedIdentity,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "CreationContext")]
    creation_context: Option<CreationContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Record<K, B> {
    envelope: Envelope<K>,
    pub(super) body: B,
}

impl<K: Serialize, B: Serialize> Record<K, B> {
    pub(super) fn seal(
        kind: K,
        identity: RecordIdentity,
        body: B,
    ) -> Result<Self, EncodingFailure> {
        if identity.domain() != InstanceDomain::Record {
            return Err(EncodingFailure::RecordShape);
        }
        let mut record = Self {
            envelope: Envelope {
                record_kind: kind,
                record_schema_revision: RevisionZero::Value,
                governing_specification: VersionedIdentity::specification(),
                record_identity: identity,
                integrity_digest: IntegrityDigest::from_hash([0; 32]),
                producing_tool: VersionedIdentity::new(
                    "intlify-config-minimum-measurement",
                    env!("CARGO_PKG_VERSION"),
                )
                .map_err(|_| EncodingFailure::RecordShape)?,
                creation_context: None,
            },
            body,
        };
        record.envelope.integrity_digest =
            IntegrityDigest::from_hash(encoding::record_hash(&record)?);
        Ok(record)
    }

    pub(super) fn identity(&self) -> &RecordIdentity {
        &self.envelope.record_identity
    }

    pub(super) fn verify_integrity(&self) -> Result<bool, EncodingFailure> {
        Ok(
            self.envelope.record_identity.domain() == InstanceDomain::Record
                && self.envelope.governing_specification == VersionedIdentity::specification()
                && self.envelope.integrity_digest
                    == IntegrityDigest::from_hash(encoding::record_hash(self)?),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct NestedReference {
    pub(super) parent_record_identity: RecordIdentity,
    pub(super) local_record_identity: super::identity::Token,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(super) enum Reference {
    TopLevel { record_identity: RecordIdentity },
    NestedRecord { reference: NestedReference },
}

impl Reference {
    pub(super) fn top(identity: &RecordIdentity) -> Self {
        Self::TopLevel {
            record_identity: identity.clone(),
        }
    }
    pub(super) fn nested(parent: &RecordIdentity, local: super::identity::Token) -> Self {
        Self::NestedRecord {
            reference: NestedReference {
                parent_record_identity: parent.clone(),
                local_record_identity: local,
            },
        }
    }
}
