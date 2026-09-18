// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The registered 026 digest domains and the record-integrity preimage rule.
//!
//! Framing and the digest function belong to `intlify_shared_json`. This module
//! only selects the domains this owner registers and applies the verification
//! record's single-member self-exclusion.

use serde_json::Value;

pub(in crate::benchmark) use intlify_shared_json::encoding::EncodingFailure;

/// The verification record excludes exactly one envelope member from its own
/// preimage. A nested member with the same name is not recursively removed.
const RECORD_INTEGRITY_EXCLUSION: [&str; 2] = ["envelope", "integrityDigest"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::benchmark) enum Domain {
    RecordIntegrity,
    MeasurementCase,
}

impl Domain {
    fn shared(self) -> intlify_shared_json::encoding::Domain {
        intlify_shared_json::encoding::Domain::literal(match self {
            Self::RecordIntegrity => "verification-record-integrity",
            Self::MeasurementCase => "measurement-case-identity",
        })
    }
}

pub(in crate::benchmark) fn hash(
    domain: Domain,
    value: &Value,
) -> Result<[u8; 32], EncodingFailure> {
    intlify_shared_json::encoding::hash(domain.shared(), value)
}

pub(in crate::benchmark) fn record_hash(
    value: &impl serde::Serialize,
) -> Result<[u8; 32], EncodingFailure> {
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
            Domain::RecordIntegrity.shared().name(),
            "verification-record-integrity"
        );
        assert_eq!(
            Domain::MeasurementCase.shared().name(),
            "measurement-case-identity"
        );
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
