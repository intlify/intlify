// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use serde_json::json;

use super::*;

#[test]
fn nonempty_text_is_any_string_but_the_empty_one() {
    for text in ["a", " ", "\n", "en-US", "日本"] {
        let parsed: NonemptyText = serde_json::from_value(json!(text)).unwrap();
        assert_eq!(parsed.as_str(), text);
        assert_eq!(NonemptyText::from_validated(text).unwrap().as_str(), text);
    }
    assert!(serde_json::from_value::<NonemptyText>(json!("")).is_err());
    assert!(NonemptyText::from_validated("").is_err());
    assert!(serde_json::from_value::<NonemptyText>(json!(["a"])).is_err());
}

#[test]
fn a_semantic_digest_is_exactly_sha256_and_64_lowercase_hex_digits() {
    let digest = SemanticDigest::from_hash([0xab; 32]);
    assert_eq!(digest.as_str(), format!("sha256:{}", "ab".repeat(32)));
    assert_eq!(
        serde_json::from_value::<SemanticDigest>(json!(digest.as_str())).unwrap(),
        digest
    );
    for text in [
        String::new(),
        "ab".repeat(32),
        format!("sha256:{}", "AB".repeat(32)),
        format!("sha256:{}", "ab".repeat(31)),
        format!("sha256:{}0", "ab".repeat(32)),
        format!("sha512:{}", "ab".repeat(32)),
        format!("sha256:{}\n", "ab".repeat(32)),
    ] {
        assert!(SemanticDigest::from_validated(&text).is_err(), "{text:?}");
    }
}
