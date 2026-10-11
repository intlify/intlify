// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The 017 string primitives the locale canonicalization data uses and no
//! shared crate holds yet. They check spelling only.

use intlify_shared_json::token::{hex, valid_hex256};

// A JSON Schema pattern is an ECMA-262 regular expression, which has no inline
// dot-all flag, so the class spells out "any scalar, newline included".
intlify_shared_json::shared_string_type!(
    pub NonemptyText,
    nonempty,
    "^[\\s\\S]+$",
    "value must be a nonempty string"
);

intlify_shared_json::shared_string_type!(
    pub SemanticDigest,
    sha256_presentation,
    "^sha256:[0-9a-f]{64}$",
    "invalid semantic digest"
);

fn nonempty(value: &str) -> bool {
    !value.is_empty()
}

fn sha256_presentation(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(valid_hex256)
}

impl SemanticDigest {
    /// Present a complete SHA-256 result in its `sha256:` spelling.
    pub fn from_hash(bytes: [u8; 32]) -> Self {
        Self(format!("sha256:{}", hex(bytes)))
    }
}

#[cfg(test)]
mod tests;
