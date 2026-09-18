// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The identity domains this owner registers, over 017's shared primitives.
//!
//! Token, 256-bit value, digest, and versioned-identity spellings belong to
//! `intlify_shared_json`. Registered instance domains, the Measurement Case
//! presentation, and the governing specification are 026 decisions and stay
//! here. An instance ID is neither a semantic checksum nor an assertion about
//! a trusted runner.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use intlify_shared_json::token::hex;
pub(in crate::benchmark) use intlify_shared_json::token::{
    valid_hex256, Hex256, IdentityFailure, IntegrityDigest, Token, VersionedIdentity,
};

/// The 026 specification that governs every common record this owner produces.
pub(in crate::benchmark) fn specification() -> VersionedIdentity {
    VersionedIdentity::literal("intlify-design-026", "0")
}

fn valid_case(value: &str) -> bool {
    value.strip_prefix("mc0_").is_some_and(valid_hex256)
}

intlify_shared_json::shared_string_type!(
    pub(in crate::benchmark) CaseIdentity,
    valid_case,
    "^mc0_[0-9a-f]{64}$",
    "invalid Measurement Case identity"
);

impl CaseIdentity {
    pub(in crate::benchmark) fn from_hash(bytes: [u8; 32]) -> Self {
        Self::from_validated(&format!("mc0_{}", hex(bytes))).expect("rendered case identity")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(in crate::benchmark) enum InstanceDomain {
    #[serde(rename = "intlify-verification-record-v0")]
    Record,
    #[serde(rename = "intlify-measurement-run-v0")]
    Run,
    // Native schema v1 explicitly gives one fresh immutable ID to the one-shot
    // owner result. The native content checksum remains independently retained.
    #[serde(rename = "intlify-config-owner-result-v1")]
    NativeOwnerResult,
    // Per-run local harness instance, not a machine identity or qualification.
    #[serde(rename = "intlify-config-local-runner-instance-v0")]
    LocalRunnerInstance,
}

impl InstanceDomain {
    fn name(self) -> &'static str {
        match self {
            Self::Record => "intlify-verification-record-v0",
            Self::Run => "intlify-measurement-run-v0",
            Self::NativeOwnerResult => "intlify-config-owner-result-v1",
            Self::LocalRunnerInstance => "intlify-config-local-runner-instance-v0",
        }
    }
}
impl Ord for InstanceDomain {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name().as_bytes().cmp(other.name().as_bytes())
    }
}
impl PartialOrd for InstanceDomain {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(in crate::benchmark) struct RecordIdentity {
    domain: InstanceDomain,
    value: Hex256,
}

impl RecordIdentity {
    pub(in crate::benchmark) fn fresh(domain: InstanceDomain) -> Result<Self, IdentityFailure> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            let mut bytes = [0; 32];
            getrandom::fill(&mut bytes).map_err(|_| IdentityFailure::EntropyUnavailable)?;
            Ok(Self {
                domain,
                value: Hex256::from_hash(bytes),
            })
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = domain;
            Err(IdentityFailure::EntropyUnavailable)
        }
    }
    pub(in crate::benchmark) const fn domain(&self) -> InstanceDomain {
        self.domain
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn case_identity_is_closed_and_distinct_from_a_shared_digest() {
        let case = CaseIdentity::from_hash([0xab; 32]);
        assert_eq!(
            serde_json::to_value(&case).unwrap(),
            json!(format!("mc0_{}", "ab".repeat(32)))
        );
        let digest = IntegrityDigest::from_hash([0xab; 32]);
        assert!(
            serde_json::from_value::<CaseIdentity>(serde_json::to_value(digest).unwrap()).is_err()
        );
        for text in [
            "ab".repeat(32),
            format!("mc0_{}", "AB".repeat(32)),
            format!("mc0_{}", "a".repeat(63)),
            format!("mc0_{}", "g".repeat(64)),
        ] {
            assert!(serde_json::from_value::<CaseIdentity>(json!(text)).is_err());
        }
        for value in [json!(0), json!(null), json!({}), json!([])] {
            assert!(serde_json::from_value::<CaseIdentity>(value).is_err());
        }
    }

    #[test]
    fn the_governing_specification_is_the_exact_registered_pair() {
        assert_eq!(
            serde_json::to_value(specification()).unwrap(),
            json!({"identity": "intlify-design-026", "revision": "0"})
        );
    }

    #[test]
    fn identity_domains_and_complete_pairs_are_not_interchangeable() {
        let original = json!({"domain": "intlify-verification-record-v0", "value": "0".repeat(64)});
        let first: RecordIdentity = serde_json::from_value(original.clone()).unwrap();
        let mut changed = original.clone();
        changed["domain"] = json!("intlify-measurement-run-v0");
        assert_ne!(
            first,
            serde_json::from_value::<RecordIdentity>(changed).unwrap()
        );
        for action in 0..4 {
            let mut invalid = original.clone();
            match action {
                0 => invalid["domain"] = json!("unknown-domain"),
                1 => invalid["value"] = json!("0".repeat(63)),
                2 => {
                    invalid.as_object_mut().unwrap().remove("domain");
                }
                _ => invalid["extra"] = json!(true),
            }
            assert!(serde_json::from_value::<RecordIdentity>(invalid).is_err());
        }
    }
}
