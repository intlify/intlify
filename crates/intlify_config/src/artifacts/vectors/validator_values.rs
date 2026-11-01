// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The validator value vectors an independent implementation checks.
//!
//! They hold the exact readings revision `"0"`'s Draft 7 validator makes of
//! the values its keywords meet: JSON numbers read as decimals, with their
//! order, the integer test, the counts of the length keywords, and
//! `multipleOf`; JSON values grouped by equality, with `uniqueItems`; URI
//! references split, resolved, and normalized under RFC 3986; and the JSON
//! Pointers URI fragments spell, with the values they name. The Node checker
//! recomputes every reading with `BigInt` arithmetic, its own comparison of
//! values, its own reading of RFC 3986, Node's IPv6 grammar, and
//! `decodeURIComponent`, and it carries its own transcription of the examples
//! RFC 3986 and RFC 6901 list, so a disagreement says which of the two is
//! wrong.

use std::cmp::Ordering;

use serde_json::{json, Value};

use crate::construction::validator::equal::{all_unique, key, Key};
use crate::construction::validator::number::{Decimal, DivisorFailure};
use crate::construction::validator::pointer::Pointer;
use crate::construction::validator::uri::Reference;

/// Texts read as JSON numbers, the last ones not JSON numbers.
const NUMBER_TEXTS: &[&str] = &[
    "0",
    "-0",
    "-0.0e-5",
    "1",
    "-7",
    "10",
    "1.50",
    "0.001",
    "-12.3400e-2",
    "1E+2",
    "1e-0",
    "100e-2",
    "2.5E-3",
    "1e00000000000000000000000000001",
    "7e99999999999999999999999",
    "1.5e99999999999999999999999",
    "25e-99999999999999999999999",
    "123456789012345678901234567890",
    "",
    "-",
    "+1",
    "01",
    "00",
    "-01",
    "1.",
    ".1",
    "1.e1",
    "1e",
    "1e+",
    "1ee1",
    "1e1.5",
    "0x1",
    "1 ",
    " 1",
    "NaN",
    "Infinity",
    "--1",
    "1_000",
    "\u{661}",
];

/// Numbers compared in every pairing, tested as integers and as counts, and
/// divided by every divisor.
const VALUES: &[&str] = &[
    "0",
    "-0",
    "0.0",
    "-0e5",
    "1",
    "1.0",
    "10e-1",
    "-1",
    "0.1",
    "1e-1",
    "0.10",
    "0.15",
    "0.2",
    "0.3",
    "3e-1",
    "1.5",
    "-1.5",
    "2.5",
    "-2.5",
    "3",
    "7",
    "10",
    "99",
    "100",
    "1e2",
    "1024",
    "0.0075",
    "0.0001",
    "75e-4",
    "1e-5",
    "123456789012345678901234567890",
    "1.2345678901234567890123456789e29",
    "9223372036854775808",
    "7450580596923828125",
    "18446744073709551615",
    "18446744073709551616",
    "1e19",
    "1e20",
    "1e400",
    "-1e400",
    "1e-400",
    "1e99999999999999999999",
    "2e99999999999999999999",
    "-1e99999999999999999999",
    "1e-99999999999999999999",
    "1e100000000000000000000",
];

/// Numbers read as `multipleOf` divisors, the refused ones last.
const DIVISORS: &[&str] = &[
    "1",
    "0.1",
    "0.0001",
    "1.5",
    "3",
    "7",
    "1024",
    "9223372036854775808",
    "7450580596923828125",
    "9999999999999999999",
    "1e-400",
    "1e-99999999999999999999",
    "0",
    "-0",
    "-1",
    "-0.5",
    "12345678901234567891",
    "1.0000000000000000001",
];

/// JSON values, as texts, grouped by equality.
const EQUALITY: &[&str] = &[
    "1",
    "1.0",
    "1e0",
    "10e-1",
    "0.1e1",
    "100E-2",
    "0",
    "-0",
    "0.0",
    "-0e5",
    "-1",
    "-1.00",
    "0.1",
    "1e-1",
    "1e400",
    "10e399",
    "1e-400",
    "123456789012345678901234567890",
    "1.2345678901234567890123456789e29",
    "null",
    "true",
    "false",
    "\"\"",
    "\"a\"",
    "\"1\"",
    "\"null\"",
    "\"\\u00e9\"",
    "\"\u{e9}\"",
    "\"e\\u0301\"",
    "\"ab\"",
    "[]",
    "[[]]",
    "[{}]",
    "[null]",
    "[null,null]",
    "[[null],null]",
    "[[null,null]]",
    "[1,2]",
    "[1.0,2e0]",
    "[2,1]",
    "[\"ab\"]",
    "[\"a\",\"b\"]",
    "[1,[2,{\"x\":null}]]",
    "[1.0,[2e0,{\"x\":null}]]",
    "{}",
    "{\"\":null}",
    "{\"a\":null}",
    "{\"a\":{}}",
    "{\"a\":[]}",
    "{\"a\":1}",
    "{\"a\":1,\"b\":1}",
    "{\"a\":1,\"b\":[true]}",
    "{\"b\":[true],\"a\":1.00}",
    "{\"a\":\"b\"}",
    "{\"b\":\"a\"}",
    "{\"ab\":null}",
    "{\"a\":{\"b\":null}}",
    "{\"0\":1,\"1\":2}",
    "{\"1\":2,\"0\":1.0}",
];

/// Arrays, as texts, tested for `uniqueItems`.
const ARRAYS: &[&str] = &[
    "[]",
    "[1]",
    "[1,1.0]",
    "[1,\"1\"]",
    "[0,-0.0]",
    "[{\"a\":1,\"b\":2},{\"b\":2,\"a\":1}]",
    "[[1],[1.0]]",
    "[[1,2],[2,1]]",
    "[null,false,0,\"\",[],{}]",
    "[1,2,3,1]",
    "[3,1,2,2e0]",
    "[true,true]",
    "[\"a\",\"b\",\"c\"]",
    "[1e400,10e399]",
    "[1e400,1e401]",
];

/// Texts read as URI references, the last ones outside the grammar.
const REFERENCE_TEXTS: &[&str] = &[
    "http://a/b/c/d;p?q",
    "",
    "#",
    "?",
    "?#",
    "#?",
    "a?b?c",
    "http://a",
    "http://",
    "//g",
    "///g",
    "urn:uuid:ee564b8a-7a87-4125-8c96-e9f123d6766f",
    "a:b/c:d",
    "./a:b",
    "/a:b",
    "a/:b?:",
    "g?y/./x",
    "g#s/../x",
    "#/definitions/a%20b",
    "foo://u:p@[::1]:80/a?b#c",
    "mailto:a@b",
    "http://a!$&'()*+,;=b:8/",
    "A+1-.b:",
    "http://a:/?!$&'()*+,;=:@/?#!$&'()*+,;=:@/?",
    "%41%7e",
    "http://[::]/",
    "http://[1:2:3:4:5:6:7:8]/",
    "http://[1:2:3:4:5:6:7::]/",
    "http://[::2:3:4:5:6:7:8]/",
    "http://[::1.2.3.4]/",
    "http://[1:2:3:4:5:6:1.2.3.4]/",
    "http://[1:2:3:4:5::1.2.3.4]/",
    "http://[FFFF::abcd]/",
    "http://[v1.a]/",
    "http://[V1F.a:b!]/",
    "http://u@[::1]:8080/",
    "http://127.0.0.1:80/",
    "http://999.1.1.1/",
    " ",
    "a b",
    "#a b",
    "?a b",
    "\"",
    "<a>",
    "a{b}",
    "a|b",
    "a\\b",
    "a^b",
    "a`b",
    "[",
    "a]",
    "%",
    "%4",
    "%4g",
    "#%2",
    "\u{e9}",
    "#\u{e9}",
    "1a:b",
    "+a:b",
    "a_b:c",
    ":a",
    "a#b#c",
    "http://a b/",
    "http://a^b",
    "http://a^b:80/",
    "http://u%zz@a",
    "http://a@b@c",
    "http://a:b:c",
    "http://a:8a",
    "http://[::1",
    "http://[::1]x",
    "http://[::1]::80",
    "http://[]",
    "http://[1:2:3:4:5:6:7:8:9]/",
    "http://[1:2:3:4:5:6:7]/",
    "http://[1::2::3]/",
    "http://[:::]/",
    "http://[1.2.3.4::]/",
    "http://[1:2:3:4:5:6::1.2.3.4]/",
    "http://[::ffff:256.1.1.1]/",
    "http://[::ffff:01.1.1.1]/",
    "http://[::1.1.1.+1]/",
    "http://[12345::]/",
    "http://[fe80::1%25eth0]/",
    "http://[v1]/",
    "http://[vg.a]/",
    "http://[v1.a%20]/",
];

/// RFC 3986 section 5.4's base.
const EXAMPLE_BASE: &str = "http://a/b/c/d;p?q";

/// The references RFC 3986 section 5.4 resolves against [`EXAMPLE_BASE`],
/// the normal examples and then the abnormal ones.
const EXAMPLES: &[&str] = &[
    "g:h",
    "g",
    "./g",
    "g/",
    "/g",
    "//g",
    "?y",
    "g?y",
    "#s",
    "g#s",
    "g?y#s",
    ";x",
    "g;x",
    "g;x?y#s",
    "",
    ".",
    "./",
    "..",
    "../",
    "../g",
    "../..",
    "../../",
    "../../g",
    "../../../g",
    "../../../../g",
    "/./g",
    "/../g",
    "g.",
    ".g",
    "g..",
    "..g",
    "./../g",
    "./g/.",
    "g/./h",
    "g/../h",
    "g;x=1/./y",
    "g;x=1/../y",
    "g?y/./x",
    "g?y/../x",
    "g#s/./x",
    "g#s/../x",
    "http:g",
];

/// Bases each reference of [`REFERENCES`] is resolved against.
const BASES: &[&str] = &[
    "http://a/b/c/d;p?q",
    "http://a",
    "http://a/",
    "http://a/b?q#f",
    "foo:",
    "foo:bar",
    "foo:bar/baz",
    "foo:/a/b",
    "urn:uuid:ee564b8a-7a87-4125-8c96-e9f123d6766f",
    "json-schema:///",
    "http://example.com/root.json",
    "http://u@[::1]:8080/b/c/?q",
];

/// References resolved against every base of [`BASES`].
const REFERENCES: &[&str] = &[
    "",
    "#",
    "#f",
    "#/definitions/A",
    "?",
    "?y",
    "?y#s",
    "g",
    "g/",
    "./g",
    "../g",
    "../../../g",
    "/g",
    "/./g",
    "//g",
    "//g/./h/../i",
    ".",
    "..",
    "./",
    "../",
    "g/.",
    "g/..",
    "g/./h",
    "g/../h",
    ";x",
    "g;x?y#s",
    "other.json",
    "t/inner.json",
    "urn:x",
    "http:g",
    "g:h",
    ".//a",
    "%2E%2E/g",
];

/// References normalized.
const NORMALIZED: &[&str] = &[
    "HTTP://Example.COM/%7Euser/%7e%2f?%41#%61%3a",
    "http://%41%42.com",
    "http://U%40%3a@A",
    "http://User@Host:80/Path",
    "http://[FE80::1]:80/",
    "foo:/a/%2E%2E/b",
    "foo:/a/.%2e/b",
    "foo:/a/%2e/b",
    "foo:a/%2E%2E/b",
    "%2E%2E/a",
    "./%2e/a",
    "http://a/b?%7E%20",
    "URN:Uuid:ABC",
    "#%2f%41",
    "http://a/%e9",
    "",
];

/// Fragments, as written in a reference, read as pointers into
/// [`pointer_document`]: RFC 6901 section 6's, and then more.
const FRAGMENTS: &[&str] = &[
    "",
    "/foo",
    "/foo/0",
    "/",
    "/a~1b",
    "/c%25d",
    "/e%5Ef",
    "/g%7Ch",
    "/i%5Cj",
    "/k%22l",
    "/%20",
    "/m~0n",
    "/foo/1",
    "/foo/2",
    "/foo/01",
    "/foo/-",
    "/foo/+1",
    "/foo/0/x",
    "/0",
    "/01",
    "/-",
    "/%C3%A9",
    "/nested//",
    "/nested/list/0/1",
    "/nested/list/1/x",
    "/nested/list/1/y",
    "/m%7E0n",
    "/a%2Fb",
    "/~",
    "/~2",
    "/%7E2",
    "/%FF",
    "/%C3",
    "/c%d",
    "a",
    "%2Fa",
];

/// RFC 6901 section 5's document, with members that look like indices, a
/// name past ASCII, and nested empty names.
fn pointer_document() -> Value {
    json!({
        "foo": ["bar", "baz"],
        "": 0,
        "a/b": 1,
        "c%d": 2,
        "e^f": 3,
        "g|h": 4,
        "i\\j": 5,
        "k\"l": 6,
        " ": 7,
        "m~n": 8,
        "0": "zero",
        "01": "one",
        "-": "dash",
        "\u{e9}": "accent",
        "nested": { "": { "": "empty" }, "list": [[1, 2], { "x": null }] }
    })
}

fn decimal(text: &str) -> Result<Decimal, String> {
    Decimal::parse(text).ok_or_else(|| format!("{text:?} is not a JSON number"))
}

fn reference(text: &str) -> Result<Reference, String> {
    Reference::parse(text).ok_or_else(|| format!("{text:?} is not a URI reference"))
}

fn json_value(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|error| format!("{text:?} is not JSON: {error}"))
}

fn failure_name(failure: DivisorFailure) -> &'static str {
    match failure {
        DivisorFailure::NotPositive => "notPositive",
        DivisorFailure::Digits => "digits",
    }
}

fn numbers() -> Result<Value, String> {
    let parsed: Vec<Value> = NUMBER_TEXTS
        .iter()
        .map(|text| json!({ "text": text, "decimal": Decimal::parse(text).map(|decimal| decimal.to_string()) }))
        .collect();
    let values = VALUES
        .iter()
        .map(|text| decimal(text))
        .collect::<Result<Vec<_>, _>>()?;
    let order: String = values
        .iter()
        .flat_map(|left| {
            values.iter().map(move |right| match left.cmp(right) {
                Ordering::Less => '<',
                Ordering::Equal => '=',
                Ordering::Greater => '>',
            })
        })
        .collect();
    let integers: String = values
        .iter()
        .map(|value| if value.is_integer() { '1' } else { '0' })
        .collect();
    let counts: Vec<Value> = values
        .iter()
        .map(|value| {
            value
                .to_count()
                .map_or(Value::Null, |count| json!(count.to_string()))
        })
        .collect();
    let mut divisors = Vec::with_capacity(DIVISORS.len());
    let mut accepted = Vec::new();
    for text in DIVISORS {
        match decimal(text)?.divisor() {
            Ok(divisor) => {
                divisors.push(json!({ "text": text, "failure": null }));
                accepted.push(divisor);
            }
            Err(failure) => {
                divisors.push(json!({ "text": text, "failure": failure_name(failure) }));
            }
        }
    }
    let multiples: String = values
        .iter()
        .flat_map(|value| {
            accepted.iter().map(move |divisor| {
                if value.is_multiple_of(divisor) {
                    '1'
                } else {
                    '0'
                }
            })
        })
        .collect();
    Ok(json!({
        "parsed": parsed,
        "values": VALUES,
        "order": order,
        "integers": integers,
        "counts": counts,
        "divisors": divisors,
        "multiples": multiples,
    }))
}

fn equality() -> Result<Value, String> {
    let mut classes: Vec<(Key, Vec<&str>)> = Vec::new();
    for text in EQUALITY {
        let value = json_value(text)?;
        let key = key(&value).ok_or_else(|| format!("{text:?} has no key"))?;
        match classes.iter_mut().find(|(class, _)| *class == key) {
            Some((_, members)) => members.push(text),
            None => classes.push((key, vec![text])),
        }
    }
    let mut arrays = Vec::with_capacity(ARRAYS.len());
    for text in ARRAYS {
        let Value::Array(items) = json_value(text)? else {
            return Err(format!("{text:?} is not an array"));
        };
        let unique =
            all_unique(&items).ok_or_else(|| format!("{text:?} has an item with no key"))?;
        arrays.push(json!({ "items": text, "unique": unique }));
    }
    Ok(json!({
        "classes": classes.into_iter().map(|(_, members)| members).collect::<Vec<_>>(),
        "uniqueItems": arrays,
    }))
}

fn components(reference: &Reference) -> Value {
    json!({
        "scheme": reference.scheme(),
        "authority": reference.authority(),
        "path": reference.path(),
        "query": reference.query(),
        "fragment": reference.fragment(),
    })
}

fn resolve(base: &Reference, text: &str) -> Result<String, String> {
    reference(text)?
        .resolve(base)
        .map(|target| target.to_string())
        .ok_or_else(|| format!("{text:?} has no base"))
}

fn references() -> Result<Value, String> {
    let parsed: Vec<Value> = REFERENCE_TEXTS
        .iter()
        .map(|text| json!({ "text": text, "components": Reference::parse(text).as_ref().map(components) }))
        .collect();
    let base = reference(EXAMPLE_BASE)?;
    let examples = EXAMPLES
        .iter()
        .map(|text| Ok(json!({ "reference": text, "target": resolve(&base, text)? })))
        .collect::<Result<Vec<_>, String>>()?;
    let mut targets = Vec::with_capacity(BASES.len());
    for text in BASES {
        let base = reference(text)?;
        targets.push(
            REFERENCES
                .iter()
                .map(|reference| resolve(&base, reference))
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    let normalized = NORMALIZED
        .iter()
        .map(|text| {
            Ok(json!({ "text": text, "normalized": reference(text)?.normalize().to_string() }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(json!({
        "parsed": parsed,
        "exampleBase": EXAMPLE_BASE,
        "examples": examples,
        "bases": BASES,
        "references": REFERENCES,
        "targets": targets,
        "normalized": normalized,
    }))
}

fn pointers() -> Value {
    let document = pointer_document();
    let fragments: Vec<Value> = FRAGMENTS
        .iter()
        .map(|fragment| match Pointer::from_fragment(fragment) {
            Some(pointer) => match pointer.evaluate(&document) {
                Some(value) => {
                    json!({ "fragment": fragment, "tokens": pointer.tokens(), "found": true, "value": value })
                }
                None => json!({ "fragment": fragment, "tokens": pointer.tokens(), "found": false }),
            },
            None => json!({ "fragment": fragment, "tokens": null }),
        })
        .collect();
    json!({ "document": document, "fragments": fragments })
}

/// The vectors document.
///
/// # Errors
///
/// A listed number, JSON value, or URI reference that does not read, or a
/// reference that does not resolve.
pub fn document() -> Result<Value, String> {
    Ok(json!({
        "numbers": numbers()?,
        "equality": equality()?,
        "references": references()?,
        "pointers": pointers(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMITTED: &str = include_str!("../../../fixtures/validator-values/vectors.json");

    #[test]
    fn the_committed_vectors_are_the_ones_this_crate_writes() {
        let committed: Value = serde_json::from_str(COMMITTED).expect("the vectors are JSON");
        assert_eq!(committed, document().expect("the vectors build"));
    }

    #[test]
    fn the_numbers_reach_every_reading() {
        let document = document().expect("the vectors build");
        let numbers = &document["numbers"];
        let parsed = numbers["parsed"].as_array().expect("parsed numbers");
        assert!(parsed.iter().any(|vector| vector["decimal"].is_null()));
        assert!(parsed.iter().any(|vector| vector["decimal"].is_string()));
        let order = numbers["order"].as_str().expect("an order");
        assert_eq!(order.len(), VALUES.len() * VALUES.len());
        for mark in ['<', '=', '>'] {
            assert!(order.contains(mark), "{mark}");
        }
        let integers = numbers["integers"].as_str().expect("integers");
        assert!(integers.contains('0') && integers.contains('1'));
        let counts = numbers["counts"].as_array().expect("counts");
        assert!(counts.contains(&Value::Null));
        assert!(counts.contains(&json!("0")));
        assert!(counts.contains(&json!(u64::MAX.to_string())));
        let failures: Vec<&Value> = numbers["divisors"]
            .as_array()
            .expect("divisors")
            .iter()
            .map(|vector| &vector["failure"])
            .collect();
        for failure in [Value::Null, json!("notPositive"), json!("digits")] {
            assert!(failures.contains(&&failure), "{failure}");
        }
        let multiples = numbers["multiples"].as_str().expect("multiples");
        assert_eq!(
            multiples.len(),
            VALUES.len() * failures.iter().filter(|failure| failure.is_null()).count()
        );
        assert!(multiples.contains('0') && multiples.contains('1'));
    }

    #[test]
    fn the_equality_classes_hold_equal_spellings_together() {
        let document = document().expect("the vectors build");
        let classes = document["equality"]["classes"].as_array().expect("classes");
        let sizes: Vec<usize> = classes
            .iter()
            .map(|class| class.as_array().expect("a class").len())
            .collect();
        assert_eq!(sizes.iter().sum::<usize>(), EQUALITY.len());
        assert!(sizes.iter().any(|size| *size > 2));
        assert!(sizes.contains(&1));
        let unique: Vec<&Value> = document["equality"]["uniqueItems"]
            .as_array()
            .expect("arrays")
            .iter()
            .map(|vector| &vector["unique"])
            .collect();
        assert!(unique.contains(&&json!(true)) && unique.contains(&&json!(false)));
    }

    #[test]
    fn the_references_reach_every_reading() {
        let document = document().expect("the vectors build");
        let references = &document["references"];
        let parsed = references["parsed"].as_array().expect("parsed references");
        assert!(parsed.iter().any(|vector| vector["components"].is_null()));
        assert!(parsed.iter().any(|vector| vector["components"].is_object()));
        assert_eq!(references["examples"].as_array().map(Vec::len), Some(42));
        let targets = references["targets"].as_array().expect("targets");
        assert_eq!(targets.len(), BASES.len());
        assert!(targets
            .iter()
            .all(|row| row.as_array().map(Vec::len) == Some(REFERENCES.len())));
        // Every normalized reference is changed by normalizing, but the empty one.
        for vector in references["normalized"].as_array().expect("normalized") {
            assert_eq!(
                vector["text"] == vector["normalized"],
                vector["text"] == "",
                "{vector}"
            );
        }
    }

    #[test]
    fn the_fragments_reach_every_reading() {
        let document = document().expect("the vectors build");
        let fragments = document["pointers"]["fragments"]
            .as_array()
            .expect("fragments");
        assert!(fragments.iter().any(|vector| vector["tokens"].is_null()));
        assert!(fragments.iter().any(|vector| vector["found"] == true));
        assert!(fragments.iter().any(|vector| vector["found"] == false));
        assert!(fragments
            .iter()
            .any(|vector| vector["found"] == true && vector["value"].is_null()));
    }

    #[test]
    fn an_input_that_does_not_read_names_itself() {
        assert_eq!(decimal("1."), Err("\"1.\" is not a JSON number".to_owned()));
        assert_eq!(
            reference("a b"),
            Err("\"a b\" is not a URI reference".to_owned())
        );
        assert!(json_value("[")
            .unwrap_err()
            .starts_with("\"[\" is not JSON"));
        let base = reference("g").expect("a relative reference");
        assert_eq!(resolve(&base, "h"), Err("\"h\" has no base".to_owned()));
        assert_eq!(failure_name(DivisorFailure::NotPositive), "notPositive");
        assert_eq!(failure_name(DivisorFailure::Digits), "digits");
    }
}
