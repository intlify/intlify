// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Programmatic Entry Snapshot vectors an independent implementation
//! checks (D15:3444).
//!
//! Each vector is one canonical Snapshot in a JSON notation, with its framing
//! `S`, its content digest, and what it counts under the nine
//! `bootstrapOnly.snapshot` bounds. The Node checker reads the notation,
//! frames, digests, and counts every tree itself from design 015, and checks
//! that the notation is canonical, so a disagreement says whether this crate
//! or the checker is wrong.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde_json::{json, Value};

use crate::materialize::PortableNumber;
use crate::snapshot::{
    Marker, MarkerReason, Node, SnapshotBound, SnapshotCounts, BOOTSTRAP_LIMIT_EXCEEDED,
};

const NOTE: &str = "Canonical Programmatic Entry Snapshots written by intlify_config. Each node is an object with one member naming its variant: null, boolean, number as the shortest decimal that reads back as its binary64 value, string, array of nodes, object as [key, node] pairs in ascending unsigned UTF-8 order of their unique keys, or marker with its reason and, for bootstrap-limit-exceeded, its capability bound path and decimal witness. An independent implementation frames every tree with design 015's S, digests intlify.programmatic-entry-snapshot.v0 followed by those bytes with SHA-256, and counts it under each bootstrapOnly.snapshot bound. Hexadecimal stands for bytes, and decimal strings for counts and witnesses.";

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(text, "{byte:02x}").expect("writing to a String does not fail");
    }
    text
}

/// The JSON notation of `node`.
fn notation(node: &Node) -> Value {
    match node {
        Node::Null => json!({ "null": null }),
        Node::Boolean(value) => json!({ "boolean": value }),
        Node::Number(number) => json!({ "number": number.get().to_string() }),
        Node::String(text) => json!({ "string": text }),
        Node::Array(elements) => {
            json!({ "array": elements.iter().map(notation).collect::<Vec<_>>() })
        }
        Node::Object(members) => json!({
            "object": members
                .iter()
                .map(|(key, value)| json!([key, notation(value)]))
                .collect::<Vec<_>>(),
        }),
        Node::Marker(Marker::Rejected(reason)) => json!({ "marker": { "reason": reason.id() } }),
        Node::Marker(Marker::BootstrapLimitExceeded { bound, witness }) => json!({
            "marker": {
                "reason": BOOTSTRAP_LIMIT_EXCEEDED,
                "bound": bound.path(),
                "witness": witness.to_string(),
            },
        }),
    }
}

/// The counts keyed by each bound's name, as decimal strings.
fn counts(node: &Node) -> Value {
    Value::Object(
        SnapshotCounts::of(node)
            .by_bound()
            .into_iter()
            .map(|(bound, count)| (bound.id().name.to_owned(), json!(count.to_string())))
            .collect(),
    )
}

fn vector(id: &str, node: &Node) -> Value {
    json!({
        "id": id,
        "snapshot": notation(node),
        "bytes": hex(&node.framed()),
        "digest": node.digest(),
        "counts": counts(node),
    })
}

fn number(value: f64) -> Node {
    Node::Number(PortableNumber::new(value).expect("a portable number"))
}

fn string(text: &str) -> Node {
    Node::String(text.to_owned())
}

fn object(members: Vec<(&str, Node)>) -> Node {
    Node::Object(
        members
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn rejected(reason: MarkerReason) -> Node {
    Node::Marker(Marker::Rejected(reason))
}

/// A configuration a binding could submit, with every scalar kind.
fn configuration() -> Node {
    object(vec![
        ("schemaVersion", string("0")),
        (
            "project",
            object(vec![
                ("sourceLocale", string("en")),
                (
                    "requestedLocales",
                    Node::Array(vec![string("en"), string("ja"), string("zh-Hant")]),
                ),
                ("strict", Node::Boolean(true)),
            ]),
        ),
        (
            "targets",
            object(vec![(
                "browser",
                object(vec![
                    ("budget", number(48.5)),
                    ("fallback", Node::Null),
                    ("legacy", Node::Boolean(false)),
                ]),
            )]),
        ),
    ])
}

/// Ordinary markers among values, siblings kept, then a terminal bootstrap
/// marker as the last node.
fn markers_among_values() -> Node {
    object(vec![
        (
            "a",
            Node::Array(vec![
                number(1.0),
                rejected(MarkerReason::SparseArray),
                rejected(MarkerReason::NonFiniteNumber),
            ]),
        ),
        ("b", rejected(MarkerReason::DynamicContainer)),
        (
            "c",
            Node::Marker(Marker::BootstrapLimitExceeded {
                bound: SnapshotBound::CollectionEntries,
                witness: 7,
            }),
        ),
    ])
}

/// The vectors document.
pub fn document() -> Value {
    let mut snapshots = vec![
        vector("null", &Node::Null),
        vector("false", &Node::Boolean(false)),
        vector("true", &Node::Boolean(true)),
        vector("empty-string", &string("")),
        vector("multi-byte-string", &string("日本語é")),
        vector("zero", &number(0.0)),
        vector("one", &number(1.0)),
        vector("negative-integer", &number(-42.0)),
        vector("fraction", &number(0.5)),
        vector("decimal", &number(1234.5678)),
        vector("small", &number(1e-7)),
        vector("largest-portable", &number(9_007_199_254_740_991.0)),
        vector("smallest-portable", &number(-9_007_199_254_740_991.0)),
        vector("empty-array", &Node::Array(Vec::new())),
        vector("empty-object", &Node::Object(BTreeMap::new())),
        vector(
            "array-of-scalars",
            &Node::Array(vec![
                Node::Null,
                Node::Boolean(true),
                number(2.0),
                string("s"),
            ]),
        ),
        vector(
            "object-key-order",
            &object(vec![
                ("é", number(1.0)),
                ("z", number(2.0)),
                ("aa", number(3.0)),
                ("a", number(4.0)),
                ("Z", number(5.0)),
            ]),
        ),
        vector(
            "nested",
            &object(vec![
                (
                    "a",
                    Node::Array(vec![
                        Node::Array(vec![Node::Array(Vec::new())]),
                        object(vec![("k", Node::Null)]),
                    ]),
                ),
                ("b", object(vec![("c", object(vec![("d", string("e"))]))])),
            ]),
        ),
        vector("configuration", &configuration()),
        vector("markers-among-values", &markers_among_values()),
    ];
    for reason in MarkerReason::ALL {
        snapshots.push(vector(
            &format!("marker-{}", reason.id()),
            &rejected(reason),
        ));
    }
    for (index, bound) in SnapshotBound::ALL.into_iter().enumerate() {
        let witness = u64::try_from(index).expect("nine bounds") * 1_000 + 2;
        snapshots.push(vector(
            &format!("bootstrap-{}", bound.id().name),
            &Node::Marker(Marker::BootstrapLimitExceeded { bound, witness }),
        ));
    }
    // The largest witness is one over the largest bound, `u64::MAX - 1`, so
    // its U64 has every bit set (D15:140, 2003; plan 025 decision #297).
    snapshots.push(vector(
        "bootstrap-largest-witness",
        &Node::Marker(Marker::BootstrapLimitExceeded {
            bound: SnapshotBound::SingleMarkerCanonicalBytes,
            witness: u64::MAX,
        }),
    ));
    json!({ "note": NOTE, "snapshots": snapshots })
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMITTED: &str = include_str!("../../../fixtures/snapshot/vectors.json");

    fn committed() -> Value {
        serde_json::from_str(COMMITTED).unwrap()
    }

    /// The tree a vector's notation stands for.
    fn read(value: &Value) -> Node {
        let (variant, inner) = value.as_object().unwrap().iter().next().unwrap();
        match variant.as_str() {
            "null" => Node::Null,
            "boolean" => Node::Boolean(inner.as_bool().unwrap()),
            "number" => number(inner.as_str().unwrap().parse().unwrap()),
            "string" => string(inner.as_str().unwrap()),
            "array" => Node::Array(inner.as_array().unwrap().iter().map(read).collect()),
            "object" => Node::Object(
                inner
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|pair| (pair[0].as_str().unwrap().to_owned(), read(&pair[1])))
                    .collect(),
            ),
            "marker" => {
                let reason = inner["reason"].as_str().unwrap();
                Node::Marker(if reason == BOOTSTRAP_LIMIT_EXCEEDED {
                    let path = inner["bound"].as_str().unwrap();
                    Marker::BootstrapLimitExceeded {
                        bound: SnapshotBound::ALL
                            .into_iter()
                            .find(|bound| bound.path() == path)
                            .unwrap(),
                        witness: inner["witness"].as_str().unwrap().parse().unwrap(),
                    }
                } else {
                    Marker::Rejected(
                        MarkerReason::ALL
                            .into_iter()
                            .find(|candidate| candidate.id() == reason)
                            .unwrap(),
                    )
                })
            }
            _ => unreachable!("{variant}"),
        }
    }

    #[test]
    fn the_committed_vectors_are_the_ones_this_crate_writes() {
        // Compared as values: the repository formatter owns the file's layout.
        assert_eq!(committed(), document());
    }

    /// Every notation reads back as a tree with the vector's bytes, digest,
    /// and counts, and a number's decimal reads back as its binary64 value.
    #[test]
    fn every_vector_reads_back_as_its_snapshot() {
        let vectors = committed();
        for case in vectors["snapshots"].as_array().unwrap() {
            let node = read(&case["snapshot"]);
            assert_eq!(notation(&node), case["snapshot"], "{}", case["id"]);
            assert_eq!(hex(&node.framed()), case["bytes"], "{}", case["id"]);
            assert_eq!(node.digest(), case["digest"], "{}", case["id"]);
            assert_eq!(counts(&node), case["counts"], "{}", case["id"]);
        }
    }

    /// The vectors hold every variant, every ordinary reason, and a
    /// bootstrap marker naming each bound.
    #[test]
    fn the_vectors_hold_every_variant_reason_and_bound() {
        let text = serde_json::to_string(&committed()).unwrap();
        for variant in ["null", "boolean", "number", "string", "array", "object"] {
            assert!(text.contains(&format!("{{\"{variant}\":")), "{variant}");
        }
        for reason in MarkerReason::ALL {
            assert!(text.contains(&format!("\"reason\":\"{}\"", reason.id())));
        }
        for bound in SnapshotBound::ALL {
            assert!(text.contains(&format!("\"bound\":\"{}\"", bound.path())));
        }
    }
}
