// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Immutable built-in JS producer provenance.
//!
//! The producer family is fixed by the artifact contract. Its revision is
//! compiled from the package version and a deterministic hash of the producer
//! sources, the shared artifact-contract implementation, and resolved workspace
//! build inputs. A locally changed artifact-producing build therefore cannot
//! continue to claim the unchanged release revision.

use std::sync::OnceLock;

use intlify_contract::{ProducerId, ProducerIdentity, ProducerRevision};

/// Stable implementation-family identifier for built-in JS reference artifacts.
pub const JS_PRODUCER_ID: &str = "dev.intlify/js-reference";

/// Immutable build-derived revision compiled into this producer implementation.
pub const JS_PRODUCER_REVISION: &str = env!("INTLIFY_JS_PRODUCER_REVISION");

/// Return the complete checked built-in producer provenance.
#[must_use]
pub fn js_producer_identity() -> ProducerIdentity {
    static IDENTITY: OnceLock<ProducerIdentity> = OnceLock::new();
    IDENTITY
        .get_or_init(|| {
            ProducerIdentity::new(
                ProducerId::try_new(JS_PRODUCER_ID)
                    .expect("the built-in JS producer ID is contract-valid"),
                ProducerRevision::try_new(JS_PRODUCER_REVISION)
                    .expect("the build-derived JS producer revision is contract-valid"),
            )
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::{js_producer_identity, JS_PRODUCER_ID, JS_PRODUCER_REVISION};

    #[test]
    fn compiled_provenance_is_checked_and_build_specific() {
        let identity = js_producer_identity();
        assert_eq!(identity.id().as_str(), JS_PRODUCER_ID);
        assert_eq!(identity.revision().as_str(), JS_PRODUCER_REVISION);
        let expected_prefix = format!("{}+src.", env!("CARGO_PKG_VERSION"));
        let fingerprint = JS_PRODUCER_REVISION
            .strip_prefix(&expected_prefix)
            .expect("the revision begins with the package version");
        assert_eq!(fingerprint.len(), 64);
        assert!(fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    }
}
