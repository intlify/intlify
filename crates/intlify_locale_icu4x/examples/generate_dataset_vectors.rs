// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Write the dataset vectors an independent implementation checks.
//!
//! The vectors hold a small dataset's parts, its conformance payload as
//! canonical JSON text, and its semantic digest, with edits that must each
//! change the digest and payloads that must be refused. The Node checker
//! rebuilds the preimage from the parts and re-frames and re-hashes it, so a
//! disagreement says whether this crate or the checker is wrong.
//!
//! The marker payloads are ICU4X's own data for the four families, cut down
//! to two entries per map and list. They are test values, not the dataset
//! revision "0" admits.

use std::path::PathBuf;
use std::process::ExitCode;

use intlify_locale_icu4x::{
    LocaleCanonicalizationDataset, LocaleConformanceData, LogicalPayload, Marker, DATASET_DOMAIN,
    DATASET_SCHEMA_IDENTITY, DATASET_SCHEMA_REVISION,
};
use serde_json::{json, Value};

const ARTIFACT: &str = "fixtures/dataset/vectors.json";

/// The four logical payloads, in 017's order.
fn markers() -> Value {
    json!([
        {
            "marker": "LocaleAliasesV1",
            "payload": {
                "complex_region": {
                    "062": ["034", "143"],
                    "172": ["RU", "AM", "AZ", "BY", "GE", "KG", "KZ", "MD", "TJ", "TM", "UA", "UZ"]
                },
                "language": [],
                "language_len2": {"in": "id", "iw": "he"},
                "language_len3": {"aar": "aa", "abk": "ab"},
                "language_variants": [["aa", "saaho", "ssy"], ["art", "lojban", "jbo"]],
                "region_alpha": {"BU": "MM", "CT": "KI"},
                "region_num": {"004": "AF", "008": "AL"},
                "script": {"Qaai": "Zinh"},
                "sgn_region": {"BR": "bzs", "CO": "csn"},
                "subdivision": {"cn11": "cnbj", "cn12": "cntj"},
                "variant": {"heploc": "alalc97", "polytoni": "polyton"}
            }
        },
        {
            "marker": "LocaleLikelySubtagsExtendedV1",
            "payload": {
                "language": {"aa": ["Latn", "ET"], "aaa": ["Latn", "NG"]},
                "language_region": [[["abq", "TR"], "Latn"], [["hak", "TW"], "Hant"]],
                "language_script": [[["abq", "Latn"], "TR"], [["arc", "Hatr"], "IQ"]],
                "region": {"AS": ["sm", "Latn"], "BI": ["rn", "Latn"]},
                "script": {"Aghb": ["xag", "AZ"], "Ahom": ["aho", "IN"]},
                "script_region": [[["Arab", "KH"], "cja"], [["Arab", "MM"], "rhg"]]
            }
        },
        {
            "marker": "LocaleLikelySubtagsLanguageV1",
            "payload": {
                "language": {"af": ["Latn", "ZA"], "ak": ["Latn", "GH"]},
                "language_region": [[["az", "IQ"], "Arab"], [["az", "IR"], "Arab"]],
                "language_script": [[["az", "Arab"], "IR"], [["en", "Shaw"], "GB"]],
                "und": ["en", "Latn", "US"]
            }
        },
        {
            "marker": "LocaleLikelySubtagsScriptRegionV1",
            "payload": {
                "region": {"419": ["es", "Latn"], "AD": ["ca", "Latn"]},
                "script": {"Adlm": ["ff", "GN"], "Arab": ["ar", "EG"]},
                "script_region": [[["Arab", "AF"], "fa"], [["Arab", "AZ"], "az"]]
            }
        }
    ])
}

/// Conformance data modelled on CLDR's records, in canonical order.
fn conformance() -> Value {
    json!({
        "validity": [
            {"field": "language", "status": "regular", "codes": ["en", "fr", "he", "ja"]},
            {"field": "language", "status": "reserved", "codes": ["qaa", "qfz"]},
            {"field": "region", "status": "macroregion", "codes": ["001", "419"]},
            {"field": "region", "status": "private-use", "codes": ["XC", "XZ"]},
            {"field": "region", "status": "regular", "codes": ["FR", "IL", "JP", "US"]},
            {"field": "region", "status": "special", "codes": ["XA", "XB"]},
            {"field": "region", "status": "unknown", "codes": ["ZZ"]},
            {"field": "script", "status": "private-use", "codes": ["Qaaq"]},
            {"field": "script", "status": "regular", "codes": ["Hant", "Latn"]}
        ],
        "extensionKeys": [
            {
                "extension": "t",
                "key": "x0",
                "deprecated": false,
                "valueType": "single",
                "types": [{"kind": "class", "class": "private-use"}]
            },
            {
                "extension": "u",
                "key": "ca",
                "deprecated": false,
                "valueType": "incremental",
                "types": [
                    {"kind": "literal", "type": "ethioaa", "deprecated": false},
                    {"kind": "literal", "type": "gregory", "deprecated": false},
                    {"kind": "literal", "type": "islamic-civil", "deprecated": false},
                    {"kind": "literal", "type": "islamicc", "deprecated": true}
                ]
            },
            {
                "extension": "u",
                "key": "kr",
                "deprecated": false,
                "valueType": "multiple",
                "types": [
                    {"kind": "class", "class": "reorder-code"},
                    {"kind": "literal", "type": "digit", "deprecated": false},
                    {"kind": "literal", "type": "space", "deprecated": false}
                ]
            },
            {
                "extension": "u",
                "key": "vt",
                "deprecated": true,
                "valueType": "multiple",
                "types": [{"kind": "class", "class": "codepoints"}]
            }
        ],
        "corrections": [
            {"extension": "u", "key": "ca", "from": "ethiopic-amete-alem", "to": "ethioaa"},
            {"extension": "u", "key": "ca", "from": "islamicc", "to": "islamic-civil"}
        ]
    })
}

/// One edit to the dataset's parts, by JSON pointer, that keeps it in canonical
/// order and must give it another digest.
struct Edit {
    id: &'static str,
    pointer: &'static str,
    value: Value,
}

fn edits() -> Vec<Edit> {
    vec![
        Edit {
            id: "validity-code",
            pointer: "/conformance/validity/0/codes/0",
            value: json!("eo"),
        },
        Edit {
            id: "extension-type",
            pointer: "/conformance/extensionKeys/2/types/1/type",
            value: json!("digits"),
        },
        Edit {
            id: "extension-type-deprecation",
            pointer: "/conformance/extensionKeys/1/types/1/deprecated",
            value: json!(true),
        },
        Edit {
            id: "correction",
            pointer: "/conformance/corrections/1/to",
            value: json!("islamic-tbla"),
        },
        // ICU4X's pairs are kept in its order, so swapping two is a different
        // dataset rather than the same one sorted differently.
        Edit {
            id: "marker-pairs-swapped",
            pointer: "/markers/2/payload/language_script",
            value: json!([[["en", "Shaw"], "GB"], [["az", "Arab"], "IR"]]),
        },
    ]
}

/// A payload with one value 017's framing does not carry.
struct Refused {
    id: &'static str,
    marker: Marker,
    payload: Value,
}

fn refused() -> Vec<Refused> {
    vec![
        Refused {
            id: "number",
            marker: Marker::LocaleLikelySubtagsLanguageV1,
            payload: json!({"und": ["en", "Latn", 1]}),
        },
        Refused {
            id: "boolean",
            marker: Marker::LocaleAliasesV1,
            payload: json!({"script": {"Qaai": true}}),
        },
        Refused {
            id: "null",
            marker: Marker::LocaleLikelySubtagsScriptRegionV1,
            payload: json!({"script_region": [[["Arab", "AF"], null]]}),
        },
    ]
}

/// Build the dataset from its parts, through the same checks as any other.
fn dataset(parts: &Value) -> Result<LocaleCanonicalizationDataset, String> {
    let markers = parts["markers"]
        .as_array()
        .ok_or("markers are not a list")?
        .iter()
        .map(|entry| {
            let marker = Marker::ALL
                .into_iter()
                .find(|marker| entry["marker"] == marker.name())
                .ok_or("an unknown marker")?;
            LogicalPayload::new(marker, entry["payload"].clone())
                .map_err(|failure| format!("{failure:?}"))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let conformance: LocaleConformanceData =
        serde_json::from_value(parts["conformance"].clone()).map_err(|error| error.to_string())?;
    LocaleCanonicalizationDataset::new(markers, conformance)
        .map_err(|failure| format!("{failure:?}"))
}

fn digest(parts: &Value) -> Result<String, String> {
    dataset(parts)?
        .semantic_digest()
        .map(|digest| digest.as_str().to_owned())
        .map_err(|failure| format!("{failure:?}"))
}

fn document() -> Result<Value, String> {
    let parts = json!({"markers": markers(), "conformance": conformance()});
    let semantic_digest = digest(&parts)?;
    let text = serde_json::from_value::<LocaleConformanceData>(conformance())
        .map_err(|error| error.to_string())?
        .to_text();

    let mut edited = Vec::new();
    for edit in edits() {
        let mut parts = parts.clone();
        *parts
            .pointer_mut(edit.pointer)
            .ok_or(format!("{}: nothing at {}", edit.id, edit.pointer))? = edit.value.clone();
        edited.push(json!({
            "id": edit.id,
            "pointer": edit.pointer,
            "value": edit.value,
            "semanticDigest": digest(&parts)?,
        }));
    }

    let mut refusals = Vec::new();
    for case in refused() {
        if LogicalPayload::new(case.marker, case.payload.clone()).is_ok() {
            return Err(format!("{}: the payload was not refused", case.id));
        }
        refusals.push(json!({
            "id": case.id,
            "marker": case.marker.name(),
            "payload": case.payload,
        }));
    }

    Ok(json!({
        "note": "A small dataset, its conformance payload as canonical JSON text, and its \
                 semantic digest. An independent implementation rebuilds the preimage from \
                 datasetSchema, markers, and conformance, re-frames and re-hashes it, applies \
                 each edit and does the same, and checks that every refused payload holds a \
                 value the framing does not carry. The marker payloads are ICU4X's data cut \
                 down to two entries per map and list; none of this is the dataset revision \
                 \"0\" admits.",
        "domain": DATASET_DOMAIN,
        "datasetSchema": {"identity": DATASET_SCHEMA_IDENTITY, "revision": DATASET_SCHEMA_REVISION},
        "markers": markers(),
        "conformance": conformance(),
        "conformanceText": text,
        "semanticDigest": semantic_digest,
        "edits": edited,
        "refused": refusals,
    }))
}

fn main() -> ExitCode {
    let write = matches!(std::env::args().nth(1).as_deref(), Some("--write"));
    let document = match document() {
        Ok(document) => document,
        Err(error) => {
            eprintln!("the vectors could not be built: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut rendered = serde_json::to_string_pretty(&document).expect("serializable document");
    rendered.push('\n');

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ARTIFACT);
    if write {
        if let Some(parent) = path.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                eprintln!("could not create {}: {error}", parent.display());
                return ExitCode::FAILURE;
            }
        }
        return match std::fs::write(&path, rendered) {
            Ok(()) => {
                println!("wrote {}", path.display());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("could not write {}: {error}", path.display());
                ExitCode::FAILURE
            }
        };
    }
    // Compare decoded values: the repository formatter owns the file's layout.
    match std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    {
        Some(committed) if committed == document => {
            println!("{} is fresh", path.display());
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{} is stale; rerun with --write", path.display());
            ExitCode::FAILURE
        }
    }
}
