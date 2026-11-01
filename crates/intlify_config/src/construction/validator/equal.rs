// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Equality of JSON values, as Draft 7's `const`, `enum`, and `uniqueItems`
//! ask it.
//!
//! Two values are equal when they are of one type and equal as that type
//! (core 4.2.3): numbers by their mathematical values, strings by their code
//! points, arrays item by item, and objects by their members in any order. A
//! value's key is a byte string that two values share exactly when they are
//! equal, so a set of keys answers `enum`, and sorted keys answer
//! `uniqueItems`. A key reads its value once, so it costs the value's size.
//!
//! A key is a tag for the type and then the contents: a number's canonical
//! decimal text, a string's bytes, an array's items in order, or an object's
//! members ordered by their names' bytes. Every text and collection carries
//! its length first, so no two values run together.

use serde_json::Value;

use super::number::Decimal;

const NULL: u8 = 0;
const FALSE: u8 = 1;
const TRUE: u8 = 2;
const NUMBER: u8 = 3;
const STRING: u8 = 4;
const ARRAY: u8 = 5;
const OBJECT: u8 = 6;

/// The key of a JSON value: equal values have one key, and unequal values
/// different keys.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Key(Vec<u8>);

/// The key of `value`, or `None` when a number in it is not a JSON number,
/// which the shared reader never produces.
pub(crate) fn key(value: &Value) -> Option<Key> {
    let mut bytes = Vec::new();
    write(value, &mut bytes)?;
    Some(Key(bytes))
}

/// Whether no two of `items` are equal, as `uniqueItems` asks, or `None` when
/// one has no key.
pub(crate) fn all_unique(items: &[Value]) -> Option<bool> {
    let mut keys = items.iter().map(key).collect::<Option<Vec<_>>>()?;
    keys.sort_unstable();
    Some(keys.windows(2).all(|pair| pair[0] != pair[1]))
}

fn write(value: &Value, bytes: &mut Vec<u8>) -> Option<()> {
    match value {
        Value::Null => bytes.push(NULL),
        Value::Bool(false) => bytes.push(FALSE),
        Value::Bool(true) => bytes.push(TRUE),
        Value::Number(number) => {
            bytes.push(NUMBER);
            write_text(&Decimal::parse(number.as_str())?.to_string(), bytes);
        }
        Value::String(string) => {
            bytes.push(STRING);
            write_text(string, bytes);
        }
        Value::Array(items) => {
            bytes.push(ARRAY);
            write_length(items.len(), bytes);
            for item in items {
                write(item, bytes)?;
            }
        }
        Value::Object(members) => {
            bytes.push(OBJECT);
            write_length(members.len(), bytes);
            let mut ordered: Vec<_> = members.iter().collect();
            ordered.sort_unstable_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
            for (name, member) in ordered {
                write_text(name, bytes);
                write(member, bytes)?;
            }
        }
    }
    Some(())
}

fn write_text(text: &str, bytes: &mut Vec<u8>) {
    write_length(text.len(), bytes);
    bytes.extend_from_slice(text.as_bytes());
}

fn write_length(length: usize, bytes: &mut Vec<u8>) {
    bytes.extend_from_slice(&(length as u64).to_be_bytes());
}

#[cfg(test)]
mod tests {
    use serde_json::Number;

    use super::*;

    fn value(text: &str) -> Value {
        serde_json::from_str(text).unwrap_or_else(|error| panic!("{text:?}: {error}"))
    }

    fn key_of(text: &str) -> Key {
        key(&value(text)).unwrap_or_else(|| panic!("{text:?} has a key"))
    }

    /// Values in one class are equal, and values in different classes are
    /// not.
    const CLASSES: &[&[&str]] = &[
        &["1", "1.0", "1e0", "10e-1", "0.1e1", "100E-2"],
        &["0", "-0", "0.0", "-0e5"],
        &["-1", "-1.00"],
        &["0.1", "1e-1"],
        &["1e400", "10e399"],
        &["1e-400"],
        &[
            "123456789012345678901234567890",
            "1.2345678901234567890123456789e29",
        ],
        &["null"],
        &["true"],
        &["false"],
        &["\"\""],
        &["\"a\""],
        &["\"1\""],
        &["\"null\""],
        &["\"\\u00e9\"", "\"\u{e9}\""],
        &["\"e\\u0301\""],
        &["\"ab\""],
        &["[]"],
        &["[[]]"],
        &["[{}]"],
        &["[null]"],
        &["[null,null]"],
        &["[[null],null]"],
        &["[[null,null]]"],
        &["{\"a\":[null],\"b\":null}"],
        &["{\"a\":[null,null]}"],
        &["[1,2]", "[1.0,2e0]"],
        &["[2,1]"],
        &["[\"ab\"]"],
        &["[\"a\",\"b\"]"],
        &["[1,[2,{\"x\":null}]]", "[1.0,[2e0,{\"x\":null}]]"],
        &["{}"],
        &["{\"\":null}"],
        &["{\"a\":null}"],
        &["{\"a\":{}}"],
        &["{\"a\":[]}"],
        &["{\"a\":1}"],
        &["{\"a\":1,\"b\":1}"],
        &["{\"a\":1,\"b\":[true]}", "{\"b\":[true],\"a\":1.00}"],
        &["{\"a\":\"b\"}"],
        &["{\"b\":\"a\"}"],
        &["{\"ab\":null}"],
        &["{\"a\":{\"b\":null}}"],
    ];

    #[test]
    fn equal_values_share_a_key_and_unequal_values_do_not() {
        for (index, class) in CLASSES.iter().enumerate() {
            let first = key_of(class[0]);
            for text in &class[1..] {
                assert_eq!(key_of(text), first, "{text:?} and {:?}", class[0]);
            }
            for other in &CLASSES[index + 1..] {
                for left in *class {
                    for right in *other {
                        assert_ne!(key_of(left), key_of(right), "{left:?} and {right:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn unique_items_are_pairwise_unequal() {
        for (items, unique) in [
            ("[]", true),
            ("[1]", true),
            ("[1,1.0]", false),
            ("[1,\"1\"]", true),
            ("[0,-0.0]", false),
            ("[{\"a\":1,\"b\":2},{\"b\":2,\"a\":1}]", false),
            ("[[1],[1.0]]", false),
            ("[[1,2],[2,1]]", true),
            ("[null,false,0,\"\",[],{}]", true),
            ("[1,2,3,1]", false),
            ("[3,1,2,2e0]", false),
            ("[true,true]", false),
            ("[\"a\",\"b\",\"c\"]", true),
        ] {
            let Value::Array(items_value) = value(items) else {
                panic!("{items:?} is an array")
            };
            assert_eq!(all_unique(&items_value), Some(unique), "{items:?}");
        }
    }

    #[test]
    fn a_number_that_is_not_a_json_number_has_no_key() {
        // Only a hidden constructor makes one; the shared reader cannot.
        let malformed = Value::Number(Number::from_string_unchecked("1.".to_owned()));
        assert_eq!(key(&malformed), None);
        assert_eq!(
            key(&Value::Array(vec![Value::Null, malformed.clone()])),
            None
        );
        let mut members = serde_json::Map::new();
        members.insert("a".to_owned(), malformed.clone());
        assert_eq!(key(&Value::Object(members)), None);
        assert_eq!(all_unique(&[Value::Null, malformed]), None);
    }

    #[test]
    fn a_key_spells_its_tag_and_its_lengths() {
        assert_eq!(key_of("null"), Key(vec![NULL]));
        assert_eq!(
            key_of("[true]"),
            Key(vec![ARRAY, 0, 0, 0, 0, 0, 0, 0, 1, TRUE])
        );
        assert_eq!(
            key_of("{\"b\":false,\"a\":\"\"}"),
            Key(vec![
                OBJECT, 0, 0, 0, 0, 0, 0, 0, 2, //
                0, 0, 0, 0, 0, 0, 0, 1, b'a', STRING, 0, 0, 0, 0, 0, 0, 0, 0, //
                0, 0, 0, 0, 0, 0, 0, 1, b'b', FALSE,
            ])
        );
        assert_eq!(
            key_of("1.50"),
            Key(vec![
                NUMBER, 0, 0, 0, 0, 0, 0, 0, 5, b'1', b'5', b'e', b'-', b'1'
            ])
        );
    }
}
