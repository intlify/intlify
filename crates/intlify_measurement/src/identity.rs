// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The instance-identity domains design 026 registers, over 017's primitives.
//!
//! Token, 256-bit value, digest, and versioned-identity spellings belong to
//! `intlify_shared_json`. The two common instance domains, the Measurement Case
//! presentation, and the governing specification are 026 decisions and stay
//! here. Every other instance domain belongs to the owner that registers it,
//! so this module admits an owner domain as a checked token rather than
//! enumerating one owner's spellings for every other owner to carry.
//!
//! An instance ID is neither a semantic checksum nor an assertion about a
//! trusted runner.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use intlify_shared_json::token::hex;
pub use intlify_shared_json::token::{
    valid_hex256, valid_identity, Hex256, IdentityFailure, IntegrityDigest, Token,
    VersionedIdentity,
};

/// The 026 specification that governs every common record.
#[must_use]
pub fn specification() -> VersionedIdentity {
    VersionedIdentity::literal("intlify-design-026", "0")
}

const COMMON_RECORD: &str = "intlify-verification-record-v0";
const COMMON_RUN: &str = "intlify-measurement-run-v0";

fn valid_case(value: &str) -> bool {
    value.strip_prefix("mc0_").is_some_and(valid_hex256)
}

intlify_shared_json::shared_string_type!(
    pub CaseIdentity,
    valid_case,
    "^mc0_[0-9a-f]{64}$",
    "invalid Measurement Case identity"
);

impl CaseIdentity {
    /// Present one complete case digest in its registered spelling.
    #[must_use]
    pub fn from_hash(bytes: [u8; 32]) -> Self {
        Self::from_validated(&format!("mc0_{}", hex(bytes))).expect("rendered case identity")
    }
}

/// The two instance domains 026 itself registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum CommonDomain {
    #[serde(rename = "intlify-verification-record-v0")]
    Record,
    #[serde(rename = "intlify-measurement-run-v0")]
    Run,
}

impl CommonDomain {
    /// Return the exact registered spelling.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Record => COMMON_RECORD,
            Self::Run => COMMON_RUN,
        }
    }
}

// Ordering follows the registered spelling, not declaration order, so that the
// order of two domains does not change when another one is registered.
impl Ord for CommonDomain {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name().as_bytes().cmp(other.name().as_bytes())
    }
}
impl PartialOrd for CommonDomain {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn valid_owner_domain(value: &str) -> bool {
    // An owner cannot register either common domain: those two are produced by
    // this crate, and a record naming one of them must decode as the common
    // identity rather than as an owner's look-alike.
    valid_identity(value) && value != COMMON_RECORD && value != COMMON_RUN
}

// The pattern is the exact token grammar. Excluding the two common spellings
// is a decode rule rather than a pattern, because a schema pattern must stay
// inside the regular subset every admitting implementation can compile, and a
// document that names a common domain decodes as the common identity anyway.
intlify_shared_json::shared_string_type!(
    pub OwnerDomain,
    valid_owner_domain,
    intlify_shared_json::token::ID_PATTERN,
    "invalid owner instance domain"
);

/// One fresh instance identity in a domain this crate registers.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordIdentity {
    domain: CommonDomain,
    value: Hex256,
}

/// One fresh instance identity in a domain an owner registers.
///
/// The domain is the owner's, so this crate checks its grammar and its
/// separation from the common domains, never its meaning.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnerRecordIdentity {
    domain: OwnerDomain,
    value: Hex256,
}

/// Either instance identity, as a reference target carries them equally.
///
/// The two are still separate types: a common record's own identity is never
/// an owner instance, and reading one out of a reference does not make it one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum AnyIdentity {
    Common(RecordIdentity),
    Owner(OwnerRecordIdentity),
}

impl AnyIdentity {
    /// Borrow this target when it is a common record identity.
    ///
    /// Absence is meaningful: an owner instance is never resolvable as a
    /// common record merely because a reference can name either one.
    #[must_use]
    pub const fn as_common(&self) -> Option<&RecordIdentity> {
        match self {
            Self::Common(identity) => Some(identity),
            Self::Owner(_) => None,
        }
    }

    /// Borrow this target when it is an owner instance identity.
    #[must_use]
    pub const fn as_owner(&self) -> Option<&OwnerRecordIdentity> {
        match self {
            Self::Owner(identity) => Some(identity),
            Self::Common(_) => None,
        }
    }

    fn parts(&self) -> (&str, &Hex256) {
        match self {
            Self::Common(identity) => (identity.domain.name(), &identity.value),
            Self::Owner(identity) => (identity.domain.as_str(), &identity.value),
        }
    }
}

// Ordering is over the wire spelling, so two identities compare the same way
// whether they were issued by this crate or by an owner. Enum position would
// instead sort every common identity before every owner one.
impl Ord for AnyIdentity {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let (domain, value) = self.parts();
        let (other_domain, other_value) = other.parts();
        domain
            .as_bytes()
            .cmp(other_domain.as_bytes())
            .then_with(|| value.cmp(other_value))
    }
}
impl PartialOrd for AnyIdentity {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn fresh_value() -> Result<Hex256, IdentityFailure> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let mut bytes = [0; 32];
        getrandom::fill(&mut bytes).map_err(|_| IdentityFailure::EntropyUnavailable)?;
        Ok(Hex256::from_hash(bytes))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err(IdentityFailure::EntropyUnavailable)
    }
}

impl RecordIdentity {
    /// Issue one fresh identity in a common domain.
    pub fn fresh(domain: CommonDomain) -> Result<Self, IdentityFailure> {
        Ok(Self {
            domain,
            value: fresh_value()?,
        })
    }

    /// Return the registered domain this identity was issued in.
    #[must_use]
    pub const fn domain(&self) -> CommonDomain {
        self.domain
    }
}

impl OwnerRecordIdentity {
    /// Issue one fresh identity in a domain the owner registers.
    pub fn fresh(domain: &str) -> Result<Self, IdentityFailure> {
        Ok(Self {
            domain: OwnerDomain::from_validated(domain)?,
            value: fresh_value()?,
        })
    }

    /// Borrow the registered domain this identity was issued in.
    #[must_use]
    pub fn domain(&self) -> &str {
        self.domain.as_str()
    }
}

impl From<&RecordIdentity> for AnyIdentity {
    fn from(value: &RecordIdentity) -> Self {
        Self::Common(value.clone())
    }
}

impl From<&OwnerRecordIdentity> for AnyIdentity {
    fn from(value: &OwnerRecordIdentity) -> Self {
        Self::Owner(value.clone())
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

    #[test]
    fn an_owner_domain_is_checked_but_can_never_be_a_common_one() {
        let owner = json!({"domain": "intlify-config-owner-result-v1", "value": "0".repeat(64)});
        let decoded: OwnerRecordIdentity = serde_json::from_value(owner.clone()).unwrap();
        assert_eq!(decoded.domain(), "intlify-config-owner-result-v1");
        for domain in [
            "intlify-verification-record-v0",
            "intlify-measurement-run-v0",
            "Owner-Domain",
            "",
            "-leading",
        ] {
            let mut invalid = owner.clone();
            invalid["domain"] = json!(domain);
            assert!(
                serde_json::from_value::<OwnerRecordIdentity>(invalid).is_err(),
                "{domain} was admitted as an owner instance domain"
            );
        }
    }

    #[test]
    fn a_reference_target_keeps_the_two_identity_kinds_apart() {
        let common = json!({"domain": "intlify-verification-record-v0", "value": "0".repeat(64)});
        let owner = json!({"domain": "intlify-config-owner-result-v1", "value": "0".repeat(64)});
        // A common domain resolves to the common identity, never to an owner
        // look-alike whose domain merely satisfies the token grammar.
        assert!(matches!(
            serde_json::from_value::<AnyIdentity>(common.clone()).unwrap(),
            AnyIdentity::Common(_)
        ));
        assert!(matches!(
            serde_json::from_value::<AnyIdentity>(owner.clone()).unwrap(),
            AnyIdentity::Owner(_)
        ));
        // Both keep their exact wire form, so a reference is unchanged by the
        // kind its target happens to have.
        for value in [common, owner] {
            assert_eq!(
                serde_json::to_value(serde_json::from_value::<AnyIdentity>(value.clone()).unwrap())
                    .unwrap(),
                value
            );
        }
        let mut unknown = json!({"domain": "intlify-measurement-run-v0", "value": "0".repeat(64)});
        unknown["extra"] = json!(true);
        assert!(serde_json::from_value::<AnyIdentity>(unknown).is_err());
    }

    #[test]
    fn a_fresh_identity_is_unique_and_stays_in_its_requested_domain() {
        let first = RecordIdentity::fresh(CommonDomain::Record).unwrap();
        let second = RecordIdentity::fresh(CommonDomain::Record).unwrap();
        assert_ne!(first, second);
        assert_eq!(first.domain(), CommonDomain::Record);
        assert_eq!(
            RecordIdentity::fresh(CommonDomain::Run).unwrap().domain(),
            CommonDomain::Run
        );
        let owner = OwnerRecordIdentity::fresh("intlify-config-owner-result-v1").unwrap();
        assert_ne!(
            owner,
            OwnerRecordIdentity::fresh("intlify-config-owner-result-v1").unwrap()
        );
        assert_eq!(
            OwnerRecordIdentity::fresh("intlify-verification-record-v0"),
            Err(IdentityFailure::InvalidToken)
        );
    }
}
