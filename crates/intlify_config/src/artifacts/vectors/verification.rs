// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Resource Limit Policy Verification Input vectors an independent
//! implementation checks (D17:1181-1192).
//!
//! They hold sealed inputs with their integrity and semantic preimages and
//! semantic digests, edits that must keep or change the semantic digest, and
//! raw inputs under stated bounds and a stated assertion, each admitted or
//! refused at the first of 015's boundary checks it fails, with the reason and
//! the check. The bounds include the exact value and the first value over it
//! for each of the three. The Node checker reads every raw input itself,
//! re-frames and re-hashes every preimage, and decides every outcome, so a
//! disagreement says whether this crate or the checker is wrong. The inputs
//! are test values, not ones trusted setup supplies.

use intlify_shared_json::encoding::encode;
use intlify_shared_json::token::Token;
use serde_json::{json, Value};

use crate::artifacts::verification::{
    sealed, unsealed, VerificationInputArtifact, INTEGRITY_DOMAIN, SEMANTIC_DOMAIN,
};
use crate::artifacts::{count, depth, SemanticDigest};
use crate::boundary::verification::{
    read_input, Malformation, VerificationAssertion, VerificationBound, VerificationLimits,
    VerificationRefusal,
};
use crate::input_limits::Bound;

/// The fixture input's identity.
const IDENTITY: &str = "intlify-test-verification-input";

/// The fixture input's revision.
const REVISION: &str = "0";

/// A Resource Limit Policy reference to a test policy, with a digest of one
/// repeated digit.
fn pin(identity: &str, digit: char) -> Value {
    json!({
        "kind": "resource-limit-policy",
        "identity": identity,
        "revision": "1",
        "specificationRevision": "0",
        "semanticDigest": format!("sha256:{}", digit.to_string().repeat(64)),
    })
}

/// `pins` in the order of the unsigned bytes of `C(pin)`.
fn sorted(mut pins: Vec<Value>) -> Result<Vec<Value>, String> {
    let mut framed = Vec::with_capacity(pins.len());
    for pin in pins.drain(..) {
        let bytes = encode(&pin).map_err(|failure| format!("{failure:?}"))?;
        framed.push((bytes, pin));
    }
    framed.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(framed.into_iter().map(|(_, pin)| pin).collect())
}

/// The fixture input's two pins, sorted.
fn pins() -> Result<Vec<Value>, String> {
    sorted(vec![
        pin("intlify-test-limits-b", '2'),
        pin("intlify-test-limits-a", '1'),
    ])
}

fn seal(input: Value) -> Result<Value, String> {
    sealed(input).map_err(|failure| format!("{failure:?}"))
}

fn text(value: &Value) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| error.to_string())
}

fn artifact(value: &Value) -> Result<VerificationInputArtifact, String> {
    serde_json::from_value(value.clone()).map_err(|error| error.to_string())
}

/// The assertion the input itself makes: its identity, revision, and
/// semantic digest.
fn assertion_of(value: &Value) -> Result<VerificationAssertion, String> {
    let read = artifact(value)?;
    Ok(VerificationAssertion {
        identity: read.identity().clone(),
        revision: read.revision().clone(),
        semantic_digest: read
            .semantic_digest()
            .map_err(|failure| format!("{failure:?}"))?,
    })
}

/// Every bound at its largest.
fn roomy() -> VerificationLimits {
    let largest = Bound::new(u64::MAX - 1).expect("the largest bound");
    VerificationLimits {
        bytes: largest,
        depth: largest,
        pins: largest,
    }
}

/// One sealed input with its preimages and semantic digest, read back through
/// the boundary like any other.
fn input(id: &str, value: &Value) -> Result<Value, String> {
    let raw = text(value)?;
    let admitted = read_input(Some(raw.as_bytes()), roomy(), &assertion_of(value)?)
        .map_err(|refusal| format!("{id}: {refusal:?}"))?;
    let mut integrity = value.clone();
    integrity
        .as_object_mut()
        .and_then(|object| object.remove("integrityDigest"))
        .ok_or(format!("{id}: no integrity digest"))?;
    Ok(json!({
        "id": id,
        "input": value,
        "integrityPreimage": integrity,
        "semanticPreimage": artifact(value)?.semantic_preimage(),
        "semanticDigest": admitted.semantic_digest().as_str(),
    }))
}

/// The check a refusal reports, by the position 015 gives it.
const fn check(refusal: VerificationRefusal) -> &'static str {
    match refusal {
        VerificationRefusal::Missing => "presence",
        VerificationRefusal::OverBound(VerificationBound::InputBytes) => "bytes",
        VerificationRefusal::OverBound(VerificationBound::Depth) => "depth",
        VerificationRefusal::OverBound(VerificationBound::Pins) => "pin-count",
        VerificationRefusal::Malformed(Malformation::Syntax) => "syntax",
        VerificationRefusal::Malformed(Malformation::Shape) => "shape",
        VerificationRefusal::DuplicateMember => "repeated-member",
        VerificationRefusal::UnknownMember => "unknown-member",
        VerificationRefusal::SecretBearing => "secret-bearing-member",
        VerificationRefusal::AmbientAuthority => "ambient-authority-member",
        VerificationRefusal::Malformed(Malformation::Integrity) => "integrity",
        VerificationRefusal::Malformed(Malformation::Unframable) => "unframable",
        VerificationRefusal::Malformed(Malformation::Mode) => "mode",
        VerificationRefusal::Malformed(Malformation::Pins) => "pins",
        VerificationRefusal::IdentityMismatch => "identity",
        VerificationRefusal::RevisionMismatch => "revision",
        VerificationRefusal::DigestMismatch => "semantic-digest",
    }
}

/// One raw input at the boundary under `limits` and `assertion`, with the
/// outcome the boundary reaches.
fn boundary(
    id: &str,
    raw: Option<&str>,
    limits: VerificationLimits,
    assertion: &VerificationAssertion,
) -> Value {
    let outcome = match read_input(raw.map(str::as_bytes), limits, assertion) {
        Ok(_) => json!({"admitted": true}),
        Err(refusal) => json!({"reason": refusal.reason(), "check": check(refusal)}),
    };
    json!({
        "id": id,
        "text": raw,
        "limits": {
            "bytes": limits.bytes.get().to_string(),
            "depth": limits.depth.get().to_string(),
            "pins": limits.pins.get().to_string(),
        },
        "assertion": {
            "identity": assertion.identity.as_str(),
            "revision": assertion.revision.as_str(),
            "semanticDigest": assertion.semantic_digest.as_str(),
        },
        "outcome": outcome,
    })
}

/// A bound of exactly `value`.
fn bound(value: u64) -> Result<Bound, String> {
    Bound::new(value).ok_or(format!("{value} is no bound"))
}

/// The raw inputs at the boundary: the exact and first-over value of each
/// bound, and an input that fails each later check first.
fn boundary_cases(fixture: &Value) -> Result<Vec<Value>, String> {
    let raw = text(fixture)?;
    let asserted = assertion_of(fixture)?;
    let (exact_bytes, under_bytes) = (bound(count(raw.len()))?, bound(count(raw.len()) - 1)?);
    let levels = depth(raw.as_bytes());
    let (exact_depth, under_depth) = (bound(levels)?, bound(levels - 1)?);
    let (exact_pins, under_pins) = (bound(2)?, bound(1)?);
    // Each bound at its exact value or one below it, the others at their
    // largest, so each case is about one bound.
    let only = |set: &dyn Fn(&mut VerificationLimits)| {
        let mut limits = roomy();
        set(&mut limits);
        limits
    };
    let edited = |edit: &dyn Fn(&mut Value)| -> String {
        let mut value = fixture.clone();
        edit(&mut value);
        serde_json::to_string(&value).expect("a JSON value serializes")
    };
    let resealed = |edit: &dyn Fn(&mut Value)| -> Result<String, String> {
        let mut value = fixture.clone();
        edit(&mut value);
        text(&seal(value)?)
    };
    let replaced = |from: &str, to: &str| -> Result<String, String> {
        let changed = raw.replacen(from, to, 1);
        (changed != raw)
            .then_some(changed)
            .ok_or(format!("{from} is not in the fixture's text"))
    };
    let tampered = |value: &mut Value| {
        value["integrityDigest"] = json!(format!("sha256:{}", "e".repeat(64)));
    };
    let mut cases = vec![
        boundary("admitted", Some(&raw), roomy(), &asserted),
        boundary("missing", None, roomy(), &asserted),
        boundary(
            "bytes-exact",
            Some(&raw),
            only(&|limits| {
                limits.bytes = exact_bytes;
            }),
            &asserted,
        ),
        boundary(
            "bytes-first-over",
            Some(&raw),
            only(&|limits| {
                limits.bytes = under_bytes;
            }),
            &asserted,
        ),
        boundary(
            "depth-exact",
            Some(&raw),
            only(&|limits| {
                limits.depth = exact_depth;
            }),
            &asserted,
        ),
        boundary(
            "depth-first-over",
            Some(&raw),
            only(&|limits| {
                limits.depth = under_depth;
            }),
            &asserted,
        ),
        boundary(
            "pins-exact",
            Some(&raw),
            only(&|limits| {
                limits.pins = exact_pins;
            }),
            &asserted,
        ),
        boundary(
            "pins-first-over",
            Some(&raw),
            only(&|limits| {
                limits.pins = under_pins;
            }),
            &asserted,
        ),
        // Depth is counted outside strings: brackets in a string add none, so
        // this input passes the exact depth and fails only for its member.
        boundary(
            "brackets-in-a-string",
            Some(&edited(&|value| {
                value["note"] = json!("[[[[{{{{");
            })),
            only(&|limits| {
                limits.depth = exact_depth;
            }),
            &asserted,
        ),
    ];
    let refused: Vec<(&str, Option<String>)> = vec![
        ("syntax", Some(raw[..raw.len() - 1].to_owned())),
        (
            "another-kind",
            Some(edited(&|value| {
                value["kind"] = json!("trust-policy");
            })),
        ),
        (
            "identity-not-a-token",
            Some(edited(&|value| {
                value["identity"] = json!("Test Pins");
            })),
        ),
        (
            "pin-of-a-target-profile",
            Some(edited(&|value| {
                value["pins"][0]["kind"] = json!("target-profile");
            })),
        ),
        (
            "repeated-member",
            Some(replaced("{\"kind\"", "{\"mode\":\"exact-pin\",\"kind\"")?),
        ),
        (
            "repeated-member-spelled-otherwise",
            Some(replaced(
                "{\"kind\"",
                "{\"mod\\u0065\":\"exact-pin\",\"kind\"",
            )?),
        ),
        (
            "repeated-member-in-a-pin",
            Some(replaced(
                "\"specificationRevision\":\"0\"",
                "\"specificationRevision\":\"0\",\"revision\":\"1\"",
            )?),
        ),
        (
            "unknown-member",
            Some(edited(&|value| {
                value["signature"] = json!("ed25519:0");
            })),
        ),
        (
            "unknown-member-in-a-pin",
            Some(edited(&|value| {
                value["pins"][1]["locator"] = json!("https://example.com/limits");
            })),
        ),
        (
            "private-key",
            Some(edited(&|value| {
                value["privateKey"] = json!("0");
            })),
        ),
        (
            "credential-in-a-pin",
            Some(edited(&|value| {
                value["pins"][0]["credential"] = json!("0");
            })),
        ),
        (
            "secret-locator",
            Some(edited(&|value| {
                value["secretLocator"] = json!("file:///key");
            })),
        ),
        (
            "trust-store",
            Some(edited(&|value| {
                value["trustStore"] = json!("system");
            })),
        ),
        (
            "network-source-in-a-pin",
            Some(edited(&|value| {
                value["pins"][1]["networkSource"] = json!("x");
            })),
        ),
        (
            "host-trust",
            Some(edited(&|value| {
                value["hostTrust"] = json!(true);
            })),
        ),
        (
            "reserved-before-integrity",
            Some(edited(&|value| {
                tampered(value);
                value["credential"] = json!("0");
            })),
        ),
        ("stale-integrity", Some(edited(&tampered))),
        (
            "another-mode",
            Some(resealed(&|value| {
                value["mode"] = json!("floating");
            })?),
        ),
        (
            "no-pin",
            Some(resealed(&|value| {
                value["pins"] = json!([]);
            })?),
        ),
        (
            "unsorted-pins",
            Some(resealed(&|value| {
                let mut reversed = value["pins"].as_array().cloned().unwrap_or_default();
                reversed.reverse();
                value["pins"] = Value::Array(reversed);
            })?),
        ),
        (
            "repeated-pin",
            Some(resealed(&|value| {
                let first = value["pins"][0].clone();
                value["pins"] = json!([first.clone(), first]);
            })?),
        ),
        (
            "trust-policy-pin",
            // The first pin: a framing begins with its length, so the shorter kind keeps the pins
            // ascending, and only the kind is wrong.
            Some(resealed(&|value| {
                value["pins"][0]["kind"] = json!("trust-policy");
            })?),
        ),
    ];
    for (id, raw) in &refused {
        cases.push(boundary(id, raw.as_deref(), roomy(), &asserted));
    }
    let mismatched = |edit: &dyn Fn(&mut VerificationAssertion)| {
        let mut changed = asserted.clone();
        edit(&mut changed);
        changed
    };
    cases.push(boundary(
        "identity-mismatch",
        Some(&raw),
        roomy(),
        &mismatched(&|asserted| {
            asserted.identity = Token::literal("other-pins");
        }),
    ));
    cases.push(boundary(
        "revision-mismatch",
        Some(&raw),
        roomy(),
        &mismatched(&|asserted| {
            asserted.revision = Token::literal("1");
        }),
    ));
    cases.push(boundary(
        "digest-mismatch",
        Some(&raw),
        roomy(),
        &mismatched(&|asserted| {
            asserted.semantic_digest = SemanticDigest::from_hash([7; 32]);
        }),
    ));
    // Each pair of neighbouring checks failing at once, so that the order
    // between them is pinned as well: the earlier check decides.
    let repeated = |text: &str| -> Result<String, String> {
        let changed = text.replacen("{\"kind\"", "{\"mode\":\"exact-pin\",\"kind\"", 1);
        (changed != text)
            .then_some(changed)
            .ok_or("the text does not start with its kind".to_owned())
    };
    let another_kind = edited(&|value| {
        value["kind"] = json!("trust-policy");
    });
    let signed = edited(&|value| {
        value["signature"] = json!("ed25519:0");
    });
    let order: Vec<(&str, String, VerificationLimits, VerificationAssertion)> = vec![
        (
            "bytes-before-depth",
            raw.clone(),
            only(&|limits| {
                limits.bytes = under_bytes;
                limits.depth = under_depth;
            }),
            asserted.clone(),
        ),
        (
            "depth-before-syntax",
            raw[..raw.len() - 1].to_owned(),
            only(&|limits| {
                limits.depth = under_depth;
            }),
            asserted.clone(),
        ),
        (
            "syntax-before-pin-count",
            raw[..raw.len() - 1].to_owned(),
            only(&|limits| {
                limits.pins = under_pins;
            }),
            asserted.clone(),
        ),
        (
            "pin-count-before-shape",
            another_kind.clone(),
            only(&|limits| {
                limits.pins = under_pins;
            }),
            asserted.clone(),
        ),
        (
            "shape-before-a-repeated-member",
            repeated(&another_kind)?,
            roomy(),
            asserted.clone(),
        ),
        (
            "repeated-before-unknown",
            repeated(&signed)?,
            roomy(),
            asserted.clone(),
        ),
        (
            "unknown-before-reserved",
            edited(&|value| {
                value["signature"] = json!("ed25519:0");
                value["privateKey"] = json!("0");
            }),
            roomy(),
            asserted.clone(),
        ),
        (
            "secret-bearing-before-ambient-authority",
            edited(&|value| {
                value["hostTrust"] = json!(true);
                value["pins"][0]["credential"] = json!("0");
            }),
            roomy(),
            asserted.clone(),
        ),
        (
            "ambient-authority-before-integrity",
            edited(&|value| {
                tampered(value);
                value["trustStore"] = json!("system");
            }),
            roomy(),
            asserted.clone(),
        ),
        (
            "integrity-before-mode",
            edited(&|value| {
                value["mode"] = json!("floating");
            }),
            roomy(),
            asserted.clone(),
        ),
        (
            "mode-before-pins",
            resealed(&|value| {
                value["mode"] = json!("floating");
                value["pins"] = json!([]);
            })?,
            roomy(),
            asserted.clone(),
        ),
        (
            "pins-before-the-assertion",
            resealed(&|value| {
                value["pins"] = json!([]);
            })?,
            roomy(),
            mismatched(&|asserted| {
                asserted.identity = Token::literal("other-pins");
            }),
        ),
        (
            "identity-before-revision",
            raw.clone(),
            roomy(),
            mismatched(&|asserted| {
                asserted.identity = Token::literal("other-pins");
                asserted.revision = Token::literal("1");
            }),
        ),
        (
            "revision-before-digest",
            raw.clone(),
            roomy(),
            mismatched(&|asserted| {
                asserted.revision = Token::literal("1");
                asserted.semantic_digest = SemanticDigest::from_hash([7; 32]);
            }),
        ),
    ];
    for (id, text, limits, assertion) in &order {
        cases.push(boundary(id, Some(text), *limits, assertion));
    }
    Ok(cases)
}

/// The complete vectors document.
pub fn document() -> Result<Value, String> {
    let fixture = seal(unsealed(IDENTITY, REVISION, &pins()?))?;
    let changed_pin = sorted(vec![
        pin("intlify-test-limits-a", '1'),
        pin("intlify-test-limits-b", '3'),
    ])?;
    let inputs = vec![
        input("fixture", &fixture)?,
        input(
            "renamed",
            &seal(unsealed(
                "intlify-renamed-verification-input",
                REVISION,
                &pins()?,
            ))?,
        )?,
        input("revised", &seal(unsealed(IDENTITY, "1", &pins()?))?)?,
        input(
            "pin-changed",
            &seal(unsealed(IDENTITY, REVISION, &changed_pin))?,
        )?,
        input(
            "one-pin",
            &seal(unsealed(
                IDENTITY,
                REVISION,
                &[pin("intlify-test-limits-a", '1')],
            ))?,
        )?,
    ];
    Ok(json!({
        "note": "Resource Limit Policy Verification Inputs sealed by intlify_config. An \
                 independent implementation recomputes each integrity digest over its input \
                 without the top-level integrityDigest and each semantic digest over kind, \
                 schemaRevision, mode, and pins; checks that renaming or revising an input \
                 keeps its semantic digest while changing or removing a pin changes it; and \
                 reads every raw input of the boundary list under its bounds and assertion, \
                 deciding whether 015's resource-limit-policy-verification-input boundary \
                 admits it or which of its checks refuses it first, with the reason. None of \
                 these inputs is one trusted setup supplies.",
        "integrityDomain": INTEGRITY_DOMAIN,
        "semanticDomain": SEMANTIC_DOMAIN,
        "inputs": inputs,
        "sameSemanticDigest": [["fixture", "renamed"], ["fixture", "revised"]],
        "differentSemanticDigest": [["fixture", "pin-changed"], ["fixture", "one-pin"]],
        "boundary": boundary_cases(&fixture)?,
    }))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    const COMMITTED: &str = include_str!("../../../fixtures/verification/vectors.json");

    fn committed() -> Value {
        serde_json::from_str(COMMITTED).unwrap()
    }

    fn cases<'a>(vectors: &'a Value, list: &str) -> &'a [Value] {
        vectors[list].as_array().unwrap()
    }

    #[test]
    fn the_committed_vectors_are_the_ones_this_crate_writes() {
        // Compared as values: the repository formatter owns the file's layout.
        assert_eq!(committed(), document().unwrap());
    }

    /// Every boundary vector reaches the outcome it states when the boundary
    /// reads its raw input again.
    #[test]
    fn every_boundary_vector_reaches_the_outcome_it_states() {
        let vectors = committed();
        for case in cases(&vectors, "boundary") {
            let limit = |name: &str| Bound::parse(case["limits"][name].as_str().unwrap()).unwrap();
            let limits = VerificationLimits {
                bytes: limit("bytes"),
                depth: limit("depth"),
                pins: limit("pins"),
            };
            let asserted = &case["assertion"];
            let assertion = VerificationAssertion {
                identity: Token::new(asserted["identity"].as_str().unwrap()).unwrap(),
                revision: Token::new(asserted["revision"].as_str().unwrap()).unwrap(),
                semantic_digest: serde_json::from_value(asserted["semanticDigest"].clone())
                    .unwrap(),
            };
            let raw = case["text"].as_str().map(str::as_bytes);
            let outcome = match read_input(raw, limits, &assertion) {
                Ok(_) => json!({"admitted": true}),
                Err(refusal) => json!({"reason": refusal.reason(), "check": check(refusal)}),
            };
            assert_eq!(outcome, case["outcome"], "{}", case["id"]);
        }
    }

    /// The boundary list reaches every check but the unframable one, which no
    /// input within these bounds can, and admits the inputs at each exact
    /// bound.
    #[test]
    fn the_boundary_vectors_reach_every_check() {
        let vectors = committed();
        let reached: BTreeSet<&str> = cases(&vectors, "boundary")
            .iter()
            .filter_map(|case| case["outcome"]["check"].as_str())
            .collect();
        let expected: BTreeSet<&str> = [
            "presence",
            "bytes",
            "depth",
            "pin-count",
            "syntax",
            "shape",
            "repeated-member",
            "unknown-member",
            "secret-bearing-member",
            "ambient-authority-member",
            "integrity",
            "mode",
            "pins",
            "identity",
            "revision",
            "semantic-digest",
        ]
        .into_iter()
        .collect();
        assert_eq!(reached, expected);
        for id in ["admitted", "bytes-exact", "depth-exact", "pins-exact"] {
            let case = cases(&vectors, "boundary")
                .iter()
                .find(|case| case["id"] == id)
                .unwrap();
            assert_eq!(case["outcome"], json!({"admitted": true}), "{id}");
        }
    }

    /// Each pair of neighbouring checks has a vector failing both, and the
    /// earlier check decides it, from the bytes to the semantic digest.
    #[test]
    fn the_order_vectors_pin_every_neighbouring_pair() {
        let vectors = committed();
        let pairs = [
            ("bytes-before-depth", "bytes"),
            ("depth-before-syntax", "depth"),
            ("syntax-before-pin-count", "syntax"),
            ("pin-count-before-shape", "pin-count"),
            ("shape-before-a-repeated-member", "shape"),
            ("repeated-before-unknown", "repeated-member"),
            ("unknown-before-reserved", "unknown-member"),
            (
                "secret-bearing-before-ambient-authority",
                "secret-bearing-member",
            ),
            (
                "ambient-authority-before-integrity",
                "ambient-authority-member",
            ),
            ("integrity-before-mode", "integrity"),
            ("mode-before-pins", "mode"),
            ("pins-before-the-assertion", "pins"),
            ("identity-before-revision", "identity"),
            ("revision-before-digest", "revision"),
        ];
        for (id, check) in pairs {
            let case = cases(&vectors, "boundary")
                .iter()
                .find(|case| case["id"] == id)
                .unwrap();
            assert_eq!(case["outcome"]["check"], check, "{id}");
        }
    }

    /// Renaming or revising keeps the semantic digest, and changing or
    /// removing a pin changes it.
    #[test]
    fn the_semantic_digest_pins_the_pins_and_not_the_name() {
        let vectors = committed();
        let digest = |id: &str| {
            cases(&vectors, "inputs")
                .iter()
                .find(|case| case["id"] == id)
                .unwrap()["semanticDigest"]
                .clone()
        };
        assert_eq!(digest("renamed"), digest("fixture"));
        assert_eq!(digest("revised"), digest("fixture"));
        assert_ne!(digest("pin-changed"), digest("fixture"));
        assert_ne!(digest("one-pin"), digest("fixture"));
    }
}
