// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use super::*;
use crate::markers::tests::COMMITTED;

#[test]
fn the_committed_blob_is_written_byte_for_byte() {
    // Regenerate with `cargo run -p intlify_locale_icu4x --features generate
    // --example generate_locale_data -- --write`.
    assert!(
        markers_blob().unwrap() == COMMITTED,
        "the marker blob is stale"
    );
}

#[test]
fn writing_the_same_families_twice_gives_the_same_bytes() {
    assert_eq!(markers_blob().unwrap(), markers_blob().unwrap());
    let reordered = blob(&[
        Family::Admitted(Marker::LocaleLikelySubtagsScriptRegionV1),
        Family::Admitted(Marker::LocaleAliasesV1),
        Family::Admitted(Marker::LocaleLikelySubtagsLanguageV1),
        Family::Admitted(Marker::LocaleLikelySubtagsExtendedV1),
    ])
    .unwrap();
    assert_eq!(reordered, markers_blob().unwrap());
}
