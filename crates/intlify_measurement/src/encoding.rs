// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The digest domains design 026 registers and the record-integrity preimage.
//!
//! Framing and the digest function belong to `intlify_shared_json`. This module
//! only selects the domains 026 registers and applies the verification record's
//! single-member self-exclusion.

use serde_json::Value;

pub use intlify_shared_json::encoding::EncodingFailure;

/// The verification record excludes exactly one envelope member from its own
/// preimage. A nested member with the same name is not recursively removed.
const RECORD_INTEGRITY_EXCLUSION: [&str; 2] = ["envelope", "integrityDigest"];

/// One registered 026 digest domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    /// The preimage of a verification record's own integrity digest.
    RecordIntegrity,
    /// The preimage of a Measurement Case identity.
    MeasurementCase,
}

impl Domain {
    fn shared(self) -> intlify_shared_json::encoding::Domain {
        intlify_shared_json::encoding::Domain::literal(match self {
            Self::RecordIntegrity => "verification-record-integrity",
            Self::MeasurementCase => "measurement-case-identity",
        })
    }

    /// Borrow the exact registered spelling.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.shared().name()
    }
}

/// Compute `H(D, V)` for one registered 026 domain.
pub fn hash(domain: Domain, value: &Value) -> Result<[u8; 32], EncodingFailure> {
    intlify_shared_json::encoding::hash(domain.shared(), value)
}

/// Compute a verification record's integrity digest over its own preimage.
pub fn record_hash(value: &impl serde::Serialize) -> Result<[u8; 32], EncodingFailure> {
    intlify_shared_json::encoding::hash_with_excluded_member(
        Domain::RecordIntegrity.shared(),
        value,
        &RECORD_INTEGRITY_EXCLUSION,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn hex(bytes: &[u8]) -> String {
        use std::fmt::Write;

        bytes.iter().fold(String::new(), |mut output, byte| {
            write!(&mut output, "{byte:02x}").unwrap();
            output
        })
    }

    #[test]
    fn registered_domains_keep_their_exact_spelling_and_stay_separated() {
        assert_eq!(
            Domain::RecordIntegrity.name(),
            "verification-record-integrity"
        );
        assert_eq!(Domain::MeasurementCase.name(), "measurement-case-identity");
        let value = json!({"z": ["1", null], "é": true, "a": {"b": false}});
        assert_ne!(
            hash(Domain::RecordIntegrity, &value),
            hash(Domain::MeasurementCase, &value)
        );
    }

    #[test]
    fn domain_hashes_match_independently_framed_node_crypto_vectors() {
        // Cross-checked by a separate Buffer/BigInt framing implementation and
        // Node's SHA-256, not copied from the Rust encoder's output.
        let complex = json!({"z": ["1", null], "é": true, "a": {"b": false}});
        for (domain, value, expected) in [
            (
                Domain::RecordIntegrity,
                Value::Null,
                "22df83382d3dd624514a94d41f08360df09553b44971ac2d645a973d676e839a",
            ),
            (
                Domain::MeasurementCase,
                Value::Null,
                "32483e47700ae1b656a73cc68328e948d764e0cb857a699492501ceffaa4ba3c",
            ),
            (
                Domain::RecordIntegrity,
                complex.clone(),
                "4e12da295b02e33e81665b680ad6845571ae8bcf21265aeebd7608e53bc16599",
            ),
            (
                Domain::MeasurementCase,
                complex,
                "49811a27f9c8edba886a661162e094f404307c43f95954e6a1811f80fe2656cb",
            ),
        ] {
            assert_eq!(hex(&hash(domain, &value).unwrap()), expected);
        }
    }

    #[test]
    fn only_the_top_envelope_integrity_member_is_excluded() {
        let mut record = json!({"envelope": {"integrityDigest": "old", "identity": "one"}, "body": {"integrityDigest": "nested"}});
        let initial = record_hash(&record).unwrap();
        record["envelope"]["integrityDigest"] = json!("replacement");
        assert_eq!(record_hash(&record).unwrap(), initial);
        record["body"]["integrityDigest"] = json!("changed");
        assert_ne!(record_hash(&record).unwrap(), initial);
        record["body"]["integrityDigest"] = json!("nested");
        record["envelope"]["identity"] = json!("two");
        assert_ne!(record_hash(&record).unwrap(), initial);
        assert_eq!(
            record_hash(&json!({"envelope": {}})),
            Err(EncodingFailure::RecordShape)
        );
    }
}
