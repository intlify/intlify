// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::collections::BTreeSet;

use icu_locale::provider::{
    LocaleAliasesV1, LocaleLikelySubtagsExtendedV1, LocaleLikelySubtagsLanguageV1,
    LocaleLikelySubtagsScriptRegionV1,
};
use icu_provider::prelude::*;
use intlify_shared_json::encoding::{hash, Domain};
use serde_json::{json, Value};

use super::*;
use crate::conformance::tests::vectors;
use crate::conformance::{Collection, ConformanceFailure};
use crate::payload::PayloadFailure;

fn marker(name: &Value) -> Marker {
    Marker::ALL
        .into_iter()
        .find(|marker| name == marker.name())
        .unwrap()
}

/// The dataset built from `{ markers, conformance }` parts.
fn dataset(parts: &Value) -> Result<LocaleCanonicalizationDataset, DatasetFailure> {
    let markers = parts["markers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            LogicalPayload::new(marker(&entry["marker"]), entry["payload"].clone()).unwrap()
        })
        .collect();
    let conformance = serde_json::from_value(parts["conformance"].clone()).unwrap();
    LocaleCanonicalizationDataset::new(markers, conformance)
}

fn parts() -> Value {
    let vectors = vectors();
    json!({"markers": vectors["markers"], "conformance": vectors["conformance"]})
}

fn digest(parts: &Value) -> String {
    dataset(parts)
        .unwrap()
        .semantic_digest()
        .unwrap()
        .as_str()
        .to_owned()
}

#[test]
fn the_preimage_is_exactly_the_dataset_017_frames() {
    let vectors = vectors();
    assert_eq!(
        dataset(&parts()).unwrap().preimage().unwrap(),
        json!({
            "datasetSchema": {
                "identity": "intlify-locale-canonicalization-dataset",
                "revision": "0",
            },
            "markers": vectors["markers"],
            "conformance": vectors["conformance"],
        })
    );
    assert_eq!(
        vectors["datasetSchema"]["identity"],
        DATASET_SCHEMA_IDENTITY
    );
    assert_eq!(
        vectors["datasetSchema"]["revision"],
        DATASET_SCHEMA_REVISION
    );
}

#[test]
fn the_digest_frames_the_preimage_under_the_dataset_domain_only() {
    let dataset = dataset(&parts()).unwrap();
    let preimage = dataset.preimage().unwrap();
    let framed =
        |domain| SemanticDigest::from_hash(hash(Domain::literal(domain), &preimage).unwrap());
    assert_eq!(DATASET_DOMAIN, "locale-canonicalization-dataset");
    assert_eq!(dataset.semantic_digest().unwrap(), framed(DATASET_DOMAIN));
    assert_ne!(
        dataset.semantic_digest().unwrap(),
        framed("locale-canonicalization-data-integrity")
    );
}

#[test]
fn the_digests_are_the_vectors_an_independent_implementation_checked() {
    let vectors = vectors();
    let parts = parts();
    assert_eq!(json!(digest(&parts)), vectors["semanticDigest"]);
    // Each edit keeps the dataset in canonical order and must change its
    // digest: one validity code, one type, one correction, ICU4X's pairs.
    let mut seen = BTreeSet::from([digest(&parts)]);
    let edits = vectors["edits"].as_array().unwrap();
    assert_eq!(edits.len(), 5);
    for edit in edits {
        let mut edited = parts.clone();
        *edited
            .pointer_mut(edit["pointer"].as_str().unwrap())
            .unwrap() = edit["value"].clone();
        let digest = digest(&edited);
        assert_eq!(json!(digest), edit["semanticDigest"], "{}", edit["id"]);
        assert!(seen.insert(digest), "{}", edit["id"]);
    }
}

#[test]
fn every_payload_the_vectors_refuse_is_refused() {
    let refused = vectors()["refused"].as_array().unwrap().clone();
    assert_eq!(refused.len(), 3);
    for case in refused {
        let marker = marker(&case["marker"]);
        assert_eq!(
            LogicalPayload::new(marker, case["payload"].clone()),
            Err(PayloadFailure::Unrepresentable(marker)),
            "{}",
            case["id"]
        );
    }
}

#[test]
fn the_vector_payloads_are_icu4x_data_as_icu4x_serializes_it() {
    // A data struct borrows from its source, so the source has to live as
    // long as the struct ICU4X's marker names.
    macro_rules! reserialized {
        ($marker:ty, $value:expr) => {{
            let text: &'static str =
                Box::leak(serde_json::to_string($value).unwrap().into_boxed_str());
            let data: <$marker as DynamicDataMarker>::DataStruct =
                serde_json::from_str(text).unwrap();
            serde_json::to_value(&data).unwrap()
        }};
    }
    let vectors = vectors();
    let payload = |index: usize| &vectors["markers"][index]["payload"];
    assert_eq!(&reserialized!(LocaleAliasesV1, payload(0)), payload(0));
    assert_eq!(
        &reserialized!(LocaleLikelySubtagsExtendedV1, payload(1)),
        payload(1)
    );
    assert_eq!(
        &reserialized!(LocaleLikelySubtagsLanguageV1, payload(2)),
        payload(2)
    );
    assert_eq!(
        &reserialized!(LocaleLikelySubtagsScriptRegionV1, payload(3)),
        payload(3)
    );
}

#[test]
fn the_four_families_are_required_once_each_in_name_order() {
    let reordered = |order: &[usize]| {
        let parts = parts();
        let markers = parts["markers"].as_array().unwrap();
        json!({
            "markers": order.iter().map(|index| markers[*index].clone()).collect::<Vec<_>>(),
            "conformance": parts["conformance"],
        })
    };
    assert!(dataset(&reordered(&[0, 1, 2, 3])).is_ok());
    for order in [
        &[1, 0, 2, 3][..],
        &[0, 1, 2],
        &[1, 2, 3],
        &[0, 1, 2, 3, 3],
        &[0, 0, 2, 3],
        &[],
    ] {
        assert_eq!(
            dataset(&reordered(order)),
            Err(DatasetFailure::Markers),
            "{order:?}"
        );
    }
}

#[test]
fn conformance_out_of_canonical_order_has_no_dataset() {
    // Deserializing does not check order; building a dataset does, so a
    // digest is never taken over a second spelling of the same records.
    let mut parts = parts();
    parts["conformance"]["validity"]
        .as_array_mut()
        .unwrap()
        .swap(0, 1);
    assert_eq!(
        dataset(&parts),
        Err(DatasetFailure::Conformance(ConformanceFailure::Unsorted(
            Collection::ValidityGroups
        )))
    );
}
