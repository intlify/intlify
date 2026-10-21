// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 017's representations of the resolver inputs that 015
//! Implementation Phase 3 admits besides configuration and canonicalization
//! data: each one's closed shape, its digests, and its reader. 015 owns the
//! checks over what a reader returns and every Finding, in `resolver`. Nothing
//! here acquires bytes; the caller hands them over.

use intlify_shared_json::token::hex;

use crate::references::{valid_semantic_digest, SEMANTIC_DIGEST_PATTERN};

pub(crate) mod vocabulary;

#[cfg(any(test, feature = "vectors"))]
pub mod vectors;

intlify_shared_json::shared_string_type!(
    pub(crate) SemanticDigest,
    valid_semantic_digest,
    SEMANTIC_DIGEST_PATTERN,
    "invalid semantic digest"
);

impl SemanticDigest {
    /// Present a complete SHA-256 result in its `sha256:` spelling.
    pub(crate) fn from_hash(bytes: [u8; 32]) -> Self {
        Self(format!("sha256:{}", hex(bytes)))
    }
}
