// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 017's representations of the resolver inputs that 015
//! Implementation Phase 3 admits besides configuration and canonicalization
//! data: each one's closed shape, its digests, and its reader. 015 owns the
//! checks over what a reader returns and every Finding, in `resolver`. Nothing
//! here acquires bytes; the caller hands them over.

use intlify_shared_json::token::hex;

use crate::references::{valid_semantic_digest, SEMANTIC_DIGEST_PATTERN};

pub(crate) mod collection;
pub(crate) mod envelope;
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

/// An in-memory length as the `u64` the limits are written in.
pub(crate) fn count(length: usize) -> u64 {
    u64::try_from(length).expect("an in-memory length fits u64")
}

/// The deepest nesting of objects and arrays, counted outside strings without
/// decoding anything, so a depth limit holds before any value is built.
pub(crate) fn depth(bytes: &[u8]) -> u64 {
    let (mut depth, mut deepest) = (0_u64, 0_u64);
    let (mut quoted, mut escaped) = (false, false);
    for &byte in bytes {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
            continue;
        }
        match byte {
            b'"' => quoted = true,
            b'{' | b'[' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}
