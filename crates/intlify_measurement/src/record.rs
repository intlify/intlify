// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The closed common record envelope and its reference forms.
//!
//! Decoding a record or recomputing its integrity digest admits nothing on its
//! own: a self-consistent document still has to resolve the exact Run Plan,
//! inventory, and owner binding it claims.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use intlify_shared_json::quantity::Quantity;

use crate::encoding::{self, EncodingFailure};
use crate::identity::{
    AnyIdentity, CommonDomain, IntegrityDigest, RecordIdentity, Token, VersionedIdentity,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum RevisionZero {
    #[serde(rename = "0")]
    Value,
}

macro_rules! record_kind {
    ($name:ident, $value:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        pub enum $name {
            #[serde(rename = $value)]
            Value,
        }
    };
}
record_kind!(
    EvidenceKind,
    "measurement-evidence-set",
    "The Measurement Evidence Set record kind."
);
record_kind!(
    EvaluationKind,
    "measurement-run-evaluation",
    "The Measurement Run Evaluation record kind."
);
record_kind!(
    ReportKind,
    "structured-report",
    "The structured report record kind."
);
record_kind!(
    RunPlanKind,
    "measurement-run-plan",
    "The Measurement Run Plan record kind."
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreationContext {
    created_at_unix_nanoseconds: Quantity,
}

fn present<'de, D: Deserializer<'de>>(decoder: D) -> Result<Option<CreationContext>, D::Error> {
    CreationContext::deserialize(decoder).map(Some)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Envelope<K> {
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

impl<K> Envelope<K> {
    /// Build one sealed-shaped envelope with a zeroed integrity member.
    pub(crate) fn new(
        kind: K,
        identity: RecordIdentity,
        producing_tool: &VersionedIdentity,
    ) -> Self {
        Self {
            record_kind: kind,
            record_schema_revision: RevisionZero::Value,
            governing_specification: crate::identity::specification(),
            record_identity: identity,
            integrity_digest: IntegrityDigest::from_hash([0; 32]),
            producing_tool: producing_tool.clone(),
            creation_context: None,
        }
    }

    pub(crate) fn seal_digest(&mut self, digest: IntegrityDigest) {
        self.integrity_digest = digest;
    }

    pub(crate) const fn identity(&self) -> &RecordIdentity {
        &self.record_identity
    }

    pub(crate) fn matches_specification(&self) -> bool {
        self.record_identity.domain() == CommonDomain::Record
            && self.governing_specification == crate::identity::specification()
    }

    pub(crate) fn digest(&self) -> &IntegrityDigest {
        &self.integrity_digest
    }
}

/// One complete common record: a closed envelope and its typed body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Record<K, B> {
    envelope: Envelope<K>,
    pub body: B,
}

impl<K: Serialize, B: Serialize> Record<K, B> {
    /// Seal one record, computing its integrity digest over its own preimage.
    ///
    /// The producing tool is the owner's, because the owner is what produced
    /// the record. The governing specification is not: it is 026's, and a
    /// caller cannot select another one.
    pub fn seal(
        kind: K,
        identity: RecordIdentity,
        producing_tool: &VersionedIdentity,
        body: B,
    ) -> Result<Self, EncodingFailure> {
        if identity.domain() != CommonDomain::Record {
            return Err(EncodingFailure::RecordShape);
        }
        let mut record = Self {
            envelope: Envelope::new(kind, identity, producing_tool),
            body,
        };
        let digest = IntegrityDigest::from_hash(encoding::record_hash(&record)?);
        record.envelope.seal_digest(digest);
        Ok(record)
    }

    /// Borrow this record's own instance identity.
    pub const fn identity(&self) -> &RecordIdentity {
        self.envelope.identity()
    }

    /// Recompute this record's integrity digest and compare it in place.
    pub fn verify_integrity(&self) -> Result<bool, EncodingFailure> {
        Ok(self.envelope.matches_specification()
            && *self.envelope.digest() == IntegrityDigest::from_hash(encoding::record_hash(self)?))
    }
}

/// One local record inside a named parent record.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NestedReference {
    pub parent_record_identity: AnyIdentity,
    pub local_record_identity: Token,
}

/// One reference to a complete record or to a local record inside one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Reference {
    TopLevel { record_identity: AnyIdentity },
    NestedRecord { reference: NestedReference },
}

impl Reference {
    /// Reference one complete record.
    pub fn top(identity: impl Into<AnyIdentity>) -> Self {
        Self::TopLevel {
            record_identity: identity.into(),
        }
    }

    /// Reference one local record inside a named parent.
    pub fn nested(parent: impl Into<AnyIdentity>, local: Token) -> Self {
        Self::NestedRecord {
            reference: NestedReference {
                parent_record_identity: parent.into(),
                local_record_identity: local,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::OwnerRecordIdentity;
    use serde_json::json;

    fn tool() -> VersionedIdentity {
        VersionedIdentity::literal("intlify-measurement-test-owner", "0")
    }

    #[test]
    fn a_sealed_record_verifies_and_any_change_to_it_does_not() {
        let identity = RecordIdentity::fresh(CommonDomain::Record).unwrap();
        let record =
            Record::seal(EvidenceKind::Value, identity, &tool(), json!({"cases": []})).unwrap();
        assert!(record.verify_integrity().unwrap());

        let mut altered = record.clone();
        altered.body = json!({"cases": ["one"]});
        assert!(!altered.verify_integrity().unwrap());
        // Recomputing the digest of altered content produces a self-consistent
        // document. Integrity alone is therefore never admission.
        let resealed = Record::seal(
            EvidenceKind::Value,
            altered.identity().clone(),
            &tool(),
            altered.body.clone(),
        )
        .unwrap();
        assert!(resealed.verify_integrity().unwrap());
        assert_ne!(
            serde_json::to_value(&resealed).unwrap(),
            serde_json::to_value(&record).unwrap()
        );
    }

    #[test]
    fn a_record_cannot_be_sealed_under_a_run_or_owner_instance_identity() {
        assert_eq!(
            Record::seal(
                EvidenceKind::Value,
                RecordIdentity::fresh(CommonDomain::Run).unwrap(),
                &tool(),
                json!({})
            )
            .map(|_| ()),
            Err(EncodingFailure::RecordShape)
        );
    }

    #[test]
    fn a_reference_targets_a_common_or_an_owner_instance_without_renaming_it() {
        let common = RecordIdentity::fresh(CommonDomain::Record).unwrap();
        let owner = OwnerRecordIdentity::fresh("intlify-config-owner-result-v1").unwrap();
        assert_eq!(
            serde_json::to_value(Reference::top(&common)).unwrap(),
            json!({"kind": "top-level", "recordIdentity": serde_json::to_value(&common).unwrap()})
        );
        assert_eq!(
            serde_json::to_value(Reference::nested(&owner, Token::literal("attempt-0"))).unwrap(),
            json!({
                "kind": "nested-record",
                "reference": {
                    "parentRecordIdentity": serde_json::to_value(&owner).unwrap(),
                    "localRecordIdentity": "attempt-0"
                }
            })
        );
    }
}
