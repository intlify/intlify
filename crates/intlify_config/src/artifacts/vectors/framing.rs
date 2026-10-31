// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The framing vectors an independent implementation checks.
//!
//! They hold records and collections written by 015's field framing, byte
//! strings that the strict readers refuse with the failure each reports, the
//! unpadded base64url of byte strings, text the canonical decoder refuses, and
//! the text of Resolver Construction Identities. The Node checker frames,
//! encodes, and classifies every one itself from design 015 and RFC 4648, so a
//! disagreement says whether this crate or the checker is wrong.

use std::fmt::Write as _;

use serde_json::{json, Value};

use crate::base64url::{
    construction_identity, decode, encode, parse_construction_identity, IdentityTextFailure,
    TextFailure,
};
use crate::framing::{collection, items, record, FrameFailure, Record};

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(text, "{byte:02x}").expect("writing to a String does not fail");
    }
    text
}

/// One record of `fields`, framed by this crate.
fn written_record(id: &str, fields: &[&[u8]]) -> Value {
    let bytes = fields
        .iter()
        .fold(Record::new(), |record, field| record.field(field))
        .finish();
    json!({
        "id": id,
        "fields": fields.iter().map(|field| hex(field)).collect::<Vec<_>>(),
        "bytes": hex(&bytes),
    })
}

/// One collection of `members`, framed by this crate.
fn written_collection(id: &str, members: &[&[u8]]) -> Value {
    json!({
        "id": id,
        "items": members.iter().map(|item| hex(item)).collect::<Vec<_>>(),
        "bytes": hex(&collection(members.iter().copied())),
    })
}

/// Frames with the given tags, each over a one-byte payload, as a record.
fn tagged(tags: &[u8]) -> Vec<u8> {
    tags.iter()
        .flat_map(|&tag| {
            let mut frame = vec![tag];
            frame.extend_from_slice(&1_u64.to_be_bytes());
            frame.push(tag);
            frame
        })
        .collect()
}

const fn frame_reason(failure: FrameFailure) -> &'static str {
    match failure {
        FrameFailure::Truncated => "truncated",
        FrameFailure::ZeroTag => "zero-tag",
        FrameFailure::UnknownTag => "unknown-tag",
        FrameFailure::RepeatedTag => "repeated-tag",
        FrameFailure::OutOfOrder => "out-of-order",
        FrameFailure::MissingTag => "missing-tag",
        FrameFailure::CountMismatch => "count-mismatch",
    }
}

const fn text_reason(failure: TextFailure) -> &'static str {
    match failure {
        TextFailure::Character => "character",
        TextFailure::Length => "length",
        TextFailure::UnusedBits => "unused-bits",
    }
}

const fn identity_reason(failure: IdentityTextFailure) -> &'static str {
    match failure {
        IdentityTextFailure::Prefix => "prefix",
        IdentityTextFailure::Length => "length",
        IdentityTextFailure::Text(failure) => text_reason(failure),
    }
}

/// A record of `fields` fields that the reader refuses.
fn refused_record(id: &str, bytes: &[u8], fields: u8) -> Result<Value, String> {
    let failure = record(bytes, fields)
        .err()
        .ok_or_else(|| format!("{id}: the record was read"))?;
    Ok(json!({
        "id": id,
        "bytes": hex(bytes),
        "fields": fields.to_string(),
        "reason": frame_reason(failure),
    }))
}

/// A collection payload that the reader refuses.
fn refused_collection(id: &str, bytes: &[u8]) -> Result<Value, String> {
    let failure = items(bytes)
        .err()
        .ok_or_else(|| format!("{id}: the collection was read"))?;
    Ok(json!({ "id": id, "bytes": hex(bytes), "reason": frame_reason(failure) }))
}

/// Bytes with their text, read back through the canonical decoder.
fn encoded(bytes: &[u8]) -> Result<Value, String> {
    let text = encode(bytes);
    if decode(&text).as_deref() != Ok(bytes) {
        return Err(format!("{text}: does not decode to its bytes"));
    }
    Ok(json!({ "bytes": hex(bytes), "text": text }))
}

/// Text the canonical decoder refuses.
fn refused_text(text: &str) -> Result<Value, String> {
    let failure = decode(text)
        .err()
        .ok_or_else(|| format!("{text:?}: the text was decoded"))?;
    Ok(json!({ "text": text, "reason": text_reason(failure) }))
}

/// A digest with its identity text, read back through the parser.
fn identity(digest: &[u8; 32]) -> Result<Value, String> {
    let text = construction_identity(digest);
    if parse_construction_identity(&text) != Ok(*digest) {
        return Err(format!("{text}: does not parse to its digest"));
    }
    Ok(json!({ "digest": hex(digest), "text": text }))
}

/// Identity text the parser refuses.
fn refused_identity(text: &str) -> Result<Value, String> {
    let failure = parse_construction_identity(text)
        .err()
        .ok_or_else(|| format!("{text:?}: the identity was parsed"))?;
    Ok(json!({ "text": text, "reason": identity_reason(failure) }))
}

/// The complete vectors document.
pub fn document() -> Result<Value, String> {
    let inner = Record::new().field(b"x").finish();
    let records = vec![
        written_record("three-fields", &[b"a", b"", b"bc"]),
        written_record("no-fields", &[]),
        written_record("one-empty-field", &[b""]),
        written_record("nested-record", &[&inner, b"y"]),
        written_record("a-zero-byte-payload", &[&[0x00], &[0xff, 0x00]]),
    ];
    let collections = vec![
        written_collection("three-items", &[b"ab", b"", b"c"]),
        written_collection("no-items", &[]),
        written_collection("records-as-items", &[&inner, &inner]),
    ];
    let mut huge_length = vec![0x01];
    huge_length.extend_from_slice(&u64::MAX.to_be_bytes());
    let mut fewer = collection([b"a".as_slice(), b"b"]);
    fewer[7] = 3;
    let mut more = collection([b"a".as_slice(), b"b"]);
    more[7] = 1;
    let mut after_none = collection([]);
    after_none.extend_from_slice(&[0; 8]);
    let mut trailing = collection([b"a".as_slice()]);
    trailing.extend_from_slice(&[1, 2, 3]);
    let refused_records = vec![
        refused_record("zero-tag", &tagged(&[0x01, 0x00]), 2)?,
        refused_record("unknown-tag", &tagged(&[0x01, 0x03]), 2)?,
        refused_record("repeated-tag", &tagged(&[0x01, 0x01]), 2)?,
        refused_record("out-of-order", &tagged(&[0x02, 0x01]), 2)?,
        refused_record("missing-middle-tag", &tagged(&[0x01, 0x03]), 3)?,
        refused_record("missing-last-tag", &tagged(&[0x01]), 2)?,
        refused_record("no-frames-for-a-field", &[], 1)?,
        refused_record(
            "repeat-below-its-predecessor",
            &tagged(&[0x01, 0x02, 0x01]),
            2,
        )?,
        refused_record("unknown-before-zero", &tagged(&[0x09, 0x00]), 2)?,
        refused_record("out-of-order-before-missing", &tagged(&[0x03, 0x01]), 4)?,
        refused_record("lone-tag", &[0x01], 1)?,
        refused_record("inside-the-length", &tagged(&[0x01])[..5], 1)?,
        refused_record(
            "inside-the-payload",
            &[0x01, 0, 0, 0, 0, 0, 0, 0, 2, 0x61],
            1,
        )?,
        refused_record("length-past-the-end", &huge_length, 1)?,
    ];
    let refused_collections = vec![
        refused_collection("fewer-items-than-counted", &fewer)?,
        refused_collection("more-items-than-counted", &more)?,
        refused_collection("an-item-after-none", &after_none)?,
        refused_collection("bytes-after-the-counted-items", &trailing)?,
        refused_collection("inside-the-count", &[0, 0, 0])?,
        refused_collection("inside-an-item", &collection([b"abc".as_slice()])[..17])?,
    ];
    let byte_strings: [&[u8]; 10] = [
        b"",
        b"f",
        b"fo", // spellchecker:disable-line
        b"foo",
        b"foob",
        b"fooba",
        b"foobar",
        &[0xfb, 0xff],
        &[0xf8, 0x00, 0x00],
        &[0xff; 3],
    ];
    let mut base64url = byte_strings
        .into_iter()
        .map(encoded)
        .collect::<Result<Vec<_>, _>>()?;
    base64url.push(encoded(&(0..=255).collect::<Vec<u8>>())?);
    let refused_base64url = [
        "Zg==", "+/8", "Zm 9v", "A", "Zm9vY", "Zm9v=", "Zh", "Zo", "Zm9", "Zm-",
    ]
    .into_iter()
    .map(refused_text)
    .collect::<Result<Vec<_>, _>>()?;
    let counting = core::array::from_fn::<u8, 32, _>(|index| index as u8);
    let identities = vec![
        identity(&[0; 32])?,
        identity(&[0xff; 32])?,
        identity(&counting)?,
    ];
    let body = "A".repeat(43);
    let refused_identities = [
        format!("rci1_{body}"),
        body.clone(),
        "rci0_".to_owned(),
        format!("rci0_{}", "A".repeat(42)),
        format!("rci0_{body}A"),
        format!("rci0_{}=", "A".repeat(42)),
        format!("rci0_{}B", "A".repeat(42)),
    ]
    .iter()
    .map(|text| refused_identity(text))
    .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "note": "Records, collections, and texts written by intlify_config. An independent \
                 implementation frames every record's fields with the tags 0x01, 0x02, and so \
                 on, frames every collection's items with its count, and decides for every \
                 refused record or collection which failure its strict reader reports first: \
                 truncated bytes, then for each frame in order tag 0x00, a tag beyond the \
                 record's fields, a repeated tag, or a tag below its predecessor, then a missing \
                 tag; and for a collection a count its items do not match. It encodes every byte \
                 string as unpadded base64url and decides for every refused text whether it has \
                 a character outside the alphabet, a length one past a multiple of four, or an \
                 unused bit set, and checks every Resolver Construction Identity text as rci0_ \
                 and the 43 characters of its digest. Hexadecimal stands for bytes, and decimal \
                 strings for field counts.",
        "records": records,
        "collections": collections,
        "refusedRecords": refused_records,
        "refusedCollections": refused_collections,
        "base64url": base64url,
        "refusedBase64url": refused_base64url,
        "constructionIdentities": identities,
        "refusedConstructionIdentities": refused_identities,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMITTED: &str = include_str!("../../../fixtures/framing/vectors.json");

    fn committed() -> Value {
        serde_json::from_str(COMMITTED).unwrap()
    }

    fn cases<'a>(vectors: &'a Value, list: &str) -> &'a [Value] {
        vectors[list].as_array().unwrap()
    }

    fn unhex(text: &Value) -> Vec<u8> {
        let text = text.as_str().unwrap();
        (0..text.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&text[index..index + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn the_committed_vectors_are_the_ones_this_crate_writes() {
        // Compared as values: the repository formatter owns the file's layout.
        assert_eq!(committed(), document().unwrap());
    }

    #[test]
    fn every_record_and_collection_reads_back_as_its_parts() {
        let vectors = committed();
        for case in cases(&vectors, "records") {
            let fields = case["fields"].as_array().unwrap();
            let bytes = unhex(&case["bytes"]);
            let read = record(&bytes, u8::try_from(fields.len()).unwrap()).unwrap();
            assert_eq!(
                read,
                fields.iter().map(unhex).collect::<Vec<_>>(),
                "{}",
                case["id"]
            );
        }
        for case in cases(&vectors, "collections") {
            let bytes = unhex(&case["bytes"]);
            let read = items(&bytes).unwrap();
            let expected = case["items"].as_array().unwrap().iter().map(unhex);
            assert_eq!(read, expected.collect::<Vec<_>>(), "{}", case["id"]);
        }
    }

    #[test]
    fn every_refused_vector_fails_where_the_independent_implementation_says() {
        let vectors = committed();
        for case in cases(&vectors, "refusedRecords") {
            let fields = case["fields"].as_str().unwrap().parse().unwrap();
            let failure = record(&unhex(&case["bytes"]), fields).unwrap_err();
            assert_eq!(frame_reason(failure), case["reason"], "{}", case["id"]);
        }
        for case in cases(&vectors, "refusedCollections") {
            let failure = items(&unhex(&case["bytes"])).unwrap_err();
            assert_eq!(frame_reason(failure), case["reason"], "{}", case["id"]);
        }
        for case in cases(&vectors, "refusedBase64url") {
            let failure = decode(case["text"].as_str().unwrap()).unwrap_err();
            assert_eq!(text_reason(failure), case["reason"], "{}", case["text"]);
        }
        for case in cases(&vectors, "refusedConstructionIdentities") {
            let failure = parse_construction_identity(case["text"].as_str().unwrap()).unwrap_err();
            assert_eq!(identity_reason(failure), case["reason"], "{}", case["text"]);
        }
    }

    #[test]
    fn every_failure_appears_among_the_refused_vectors() {
        let vectors = committed();
        let reasons = |lists: &[&str]| {
            lists
                .iter()
                .flat_map(|list| cases(&vectors, list))
                .map(|case| case["reason"].as_str().unwrap().to_owned())
                .collect::<std::collections::BTreeSet<_>>()
        };
        assert_eq!(
            reasons(&["refusedRecords", "refusedCollections"]),
            [
                "count-mismatch",
                "missing-tag",
                "out-of-order",
                "repeated-tag",
                "truncated",
                "unknown-tag",
                "zero-tag"
            ]
            .map(str::to_owned)
            .into()
        );
        assert_eq!(
            reasons(&["refusedBase64url"]),
            ["character", "length", "unused-bits"]
                .map(str::to_owned)
                .into()
        );
        assert_eq!(
            reasons(&["refusedConstructionIdentities"]),
            ["character", "length", "prefix", "unused-bits"]
                .map(str::to_owned)
                .into()
        );
    }

    #[test]
    fn the_identity_vectors_parse_back_to_their_digests() {
        let vectors = committed();
        for case in cases(&vectors, "constructionIdentities") {
            let digest = unhex(&case["digest"]);
            let parsed = parse_construction_identity(case["text"].as_str().unwrap()).unwrap();
            assert_eq!(parsed.as_slice(), digest, "{}", case["text"]);
        }
    }
}
