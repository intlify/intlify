// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The bound vectors an independent implementation checks.
//!
//! They hold `ResourceBoundValue` decimal strings with the eight bytes of their
//! semantic form, strings and byte forms that are no bound with the reason
//! 015's capability boundary gives for each, and every capability bound path
//! with its revision-"0" minimum, in the Capability Specification's canonical
//! order. The Node checker reads every string, encodes every value, and lists
//! the bounds from its own copy of design 015's tables, so a disagreement says
//! whether this crate or the checker is wrong.

use std::fmt::Write as _;

use serde_json::{json, Value};

use crate::input_limits::{Bound, BoundFailure};
use crate::resolver::capability::CapabilityBound;

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(text, "{byte:02x}").expect("writing to a String does not fail");
    }
    text
}

const fn reason(failure: BoundFailure) -> &'static str {
    match failure {
        BoundFailure::NonCanonical => "non-canonical-bound",
        BoundFailure::NonPositive => "non-positive-bound",
        BoundFailure::Unrepresentable => "unrepresentable-bound",
    }
}

/// A bound's decimal string with its semantic form, read back both ways.
fn value(text: &str) -> Result<Value, String> {
    let bound = Bound::read(text).map_err(|failure| format!("{text}: {}", reason(failure)))?;
    let semantic = bound.to_be_bytes();
    if bound.get().to_string() != text || Bound::from_be_bytes(semantic) != Ok(bound) {
        return Err(format!("{text}: does not read back"));
    }
    Ok(json!({ "text": text, "semantic": hex(&semantic) }))
}

/// A string that is no bound.
fn refused_text(id: &str, text: &str) -> Result<Value, String> {
    let failure = Bound::read(text)
        .err()
        .ok_or_else(|| format!("{id}: the string was read"))?;
    Ok(json!({ "id": id, "text": text, "reason": reason(failure) }))
}

/// Eight bytes that are no bound's semantic form.
fn refused_semantic(bytes: [u8; 8]) -> Result<Value, String> {
    let failure = Bound::from_be_bytes(bytes)
        .err()
        .ok_or_else(|| format!("{}: the bytes were read", hex(&bytes)))?;
    Ok(json!({ "semantic": hex(&bytes), "reason": reason(failure) }))
}

/// Build the vectors document.
///
/// # Errors
///
/// Returns a description of the first vector whose value does not read back
/// or whose refusal does not happen.
pub fn document() -> Result<Value, String> {
    let values = [
        "1",
        "2",
        "9",
        "10",
        "255",
        "256",
        "1024",
        "4096",
        "65536",
        "4294967295",
        "4294967296",
        "9007199254740991",
        "9007199254740993",
        "18446744073709551614",
    ]
    .iter()
    .map(|text| value(text))
    .collect::<Result<Vec<_>, _>>()?;
    let refused_texts = [
        ("empty", ""),
        ("zero", "0"),
        ("two-zeros", "00"),
        ("leading-zero", "01"),
        ("leading-zeros", "007"),
        ("plus-sign", "+1"),
        ("minus-sign", "-1"),
        ("negative-zero", "-0"),
        ("leading-space", " 1"),
        ("trailing-space", "1 "),
        ("decimal-point", "1.0"),
        ("exponent", "1e3"),
        ("hexadecimal", "0x10"),
        ("digit-separator", "1_000"),
        ("fullwidth-digit", "\u{ff11}"),
        ("u64-max", "18446744073709551615"),
        ("past-u64-max", "18446744073709551616"),
        ("twenty-nines", "99999999999999999999"),
        ("twenty-one-digits", "100000000000000000000"),
        ("leading-zero-before-u64-max", "018446744073709551615"),
    ]
    .iter()
    .map(|(id, text)| refused_text(id, text))
    .collect::<Result<Vec<_>, _>>()?;
    let refused_semantics = [[0; 8], [0xff; 8]]
        .into_iter()
        .map(refused_semantic)
        .collect::<Result<Vec<_>, _>>()?;
    let capability_bounds = CapabilityBound::inventory()
        .iter()
        .map(|(bound, minimum)| {
            json!({ "path": bound.to_string(), "minimum": minimum.get().to_string() })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "note": "Bounds written by intlify_config. An independent implementation reads every \
                 value's decimal string as a count from 1 through 18446744073709551614 and \
                 encodes it as eight bytes, big-endian. For every refused string it decides the \
                 reason the capability boundary gives: non-canonical-bound for any spelling \
                 other than a count's shortest decimal digits, then non-positive-bound for zero, \
                 then unrepresentable-bound for 18446744073709551615 or more. It refuses the two \
                 byte forms whose count is zero or 18446744073709551615, and lists every \
                 capability bound path, policyComparable mirroring the projectProfileResolution \
                 tables and then bootstrapOnly, with its minimum as a decimal string. \
                 Hexadecimal stands for bytes.",
        "values": values,
        "refusedTexts": refused_texts,
        "refusedSemantics": refused_semantics,
        "capabilityBounds": capability_bounds,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMITTED: &str = include_str!("../../../fixtures/bounds/vectors.json");

    fn committed() -> Value {
        serde_json::from_str(COMMITTED).unwrap()
    }

    fn cases<'a>(vectors: &'a Value, list: &str) -> &'a [Value] {
        vectors[list].as_array().unwrap()
    }

    fn semantic(text: &Value) -> [u8; 8] {
        let text = text.as_str().unwrap();
        std::array::from_fn(|index| {
            u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).unwrap()
        })
    }

    #[test]
    fn the_committed_vectors_are_the_ones_this_crate_writes() {
        // Compared as values: the repository formatter owns the file's layout.
        assert_eq!(committed(), document().unwrap());
    }

    #[test]
    fn every_value_reads_back_with_its_semantic_form() {
        let vectors = committed();
        for case in cases(&vectors, "values") {
            let bound = Bound::read(case["text"].as_str().unwrap()).unwrap();
            assert_eq!(bound.to_be_bytes(), semantic(&case["semantic"]), "{case}");
            assert_eq!(Bound::from_be_bytes(semantic(&case["semantic"])), Ok(bound));
        }
    }

    #[test]
    fn every_refused_vector_fails_where_the_independent_implementation_says() {
        let vectors = committed();
        for case in cases(&vectors, "refusedTexts") {
            let failure = Bound::read(case["text"].as_str().unwrap()).unwrap_err();
            assert_eq!(reason(failure), case["reason"], "{}", case["id"]);
        }
        for case in cases(&vectors, "refusedSemantics") {
            let failure = Bound::from_be_bytes(semantic(&case["semantic"])).unwrap_err();
            assert_eq!(reason(failure), case["reason"], "{case}");
        }
    }

    #[test]
    fn every_failure_appears_among_the_refused_vectors() {
        let vectors = committed();
        let reasons = |list| {
            cases(&vectors, list)
                .iter()
                .map(|case| case["reason"].as_str().unwrap().to_owned())
                .collect::<std::collections::BTreeSet<_>>()
        };
        assert_eq!(
            reasons("refusedTexts"),
            [
                "non-canonical-bound",
                "non-positive-bound",
                "unrepresentable-bound"
            ]
            .map(str::to_owned)
            .into()
        );
        assert_eq!(
            reasons("refusedSemantics"),
            ["non-positive-bound", "unrepresentable-bound"]
                .map(str::to_owned)
                .into()
        );
    }

    #[test]
    fn the_capability_bounds_are_the_inventory_in_its_order() {
        let vectors = committed();
        let listed = cases(&vectors, "capabilityBounds");
        let inventory = CapabilityBound::inventory();
        assert_eq!(listed.len(), inventory.len());
        for (case, (bound, minimum)) in listed.iter().zip(inventory) {
            assert_eq!(case["path"], bound.to_string());
            assert_eq!(
                Bound::read(case["minimum"].as_str().unwrap()),
                Ok(minimum),
                "{case}"
            );
        }
    }
}
