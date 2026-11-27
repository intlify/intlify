// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The conditional obligations this crate writes, and the state the harness
//! finds for one (D15:3350-3393).
//!
//! Each row of the obligation table is one manifest, inactive with its
//! not-applicable reason while nothing activates it. Two rows have an
//! evaluated activation, which the harness computes from the Suite Index and
//! the Profile Specification the construction admits; the others are
//! declared, stated by their owner when it ships what activates them. An
//! evaluated obligation's `activationState` must be the computed one, and an
//! active obligation needs the cases its row names to exist (D15:3371). No
//! revision-`"0"` manifest can name such a case, so the harness finds an
//! active obligation unmet.

use serde_json::{json, Map, Value};

use crate::construction::component::file_digest;
use crate::construction::generate;

use super::case::{ManifestKind, OBLIGATIONS, REVISION};
use super::construction_cases::Cases;
use super::index::SuiteFile;

/// Where the suite holds the obligations' manifests.
const CASES: &str = "cases/conditional";

/// The Locale Canonicalization Specification revisions the harness knows to
/// admit no extension but `u` and `t`: revision `"0"` refuses private use
/// and every other singleton (D15:1288, 1311).
const UNEXTENDED: [&str; 1] = ["0"];

/// The decision that states the obligation `id`: 015-051 the first
/// cross-version obligation, 015-206 the editor-navigation adapter, 015-212
/// the rows it adds, and 015-217 each owner's handoff-consumer form.
fn decision(id: &str) -> &'static str {
    match id {
        "015-XV-001" => "015-051",
        "015-AD-001" => "015-206",
        "015-XV-002" | "015-AD-002" | "015-AD-003" | "015-AD-004" | "015-HV-001" | "015-HV-002" => {
            "015-212"
        }
        _ => "015-217",
    }
}

/// Whether the evaluated activation of the obligation `id` holds, as the
/// harness computes it from the Suite Index `index` and the Profile
/// Specification `profile` (D15:3371): `015-XV-001`'s once the Configuration
/// Schema Authority Set maps two configuration schema revisions, and
/// `015-XV-002`'s once the canonicalization specification the Profile
/// Specification names is a revision not known to refuse extensions. `None`
/// for a declared activation, which its owner states.
pub(crate) fn evaluated(id: &str, index: &Value, profile: &Value) -> Option<bool> {
    match id {
        "015-XV-001" => {
            let members = index["configurationSchemaAuthoritySet"]["members"].as_object();
            Some(members.map_or(0, Map::len) >= 2)
        }
        "015-XV-002" => {
            let slot = &profile["specificationSlots"]["locale-canonicalization"];
            let revision = slot["revision"].as_str();
            Some(!revision.is_some_and(|revision| UNEXTENDED.contains(&revision)))
        }
        _ => None,
    }
}

/// Whether the obligation `id`, `active` as its manifest states, is in the
/// state the harness finds: inactive, as an evaluated activation must
/// compute, since an active one would need cases no revision-`"0"` manifest
/// can name (D15:3371).
pub(crate) fn met(id: &str, active: bool, index: &Value, profile: &Value) -> bool {
    !active && evaluated(id, index, profile) != Some(true)
}

/// The suite's conditional obligations, one manifest for each row of the
/// table in ascending unsigned byte order of their IDs, each inactive with
/// its not-applicable reason.
///
/// # Errors
///
/// Returns the serialization failure of a manifest, which their values rule
/// out.
pub(crate) fn cases() -> Result<Cases, serde_json::Error> {
    let mut files = Vec::new();
    let mut listing = Vec::new();
    for (id, activation, reason, owner) in OBLIGATIONS {
        let manifest = generate::write(&json!({
            "suiteRevision": REVISION,
            "manifestKind": ManifestKind::ConditionalObligation.name(),
            "id": id,
            "activation": activation,
            "activationState": "inactive",
            "notApplicableReason": reason,
            "verificationOwner": owner,
            "traceability": { "decisionIds": [decision(id)], "ruleIds": [], "checkIds": [] },
        }))?;
        let path = format!("{CASES}/{id}.json");
        listing.push(json!({
            "id": id,
            "path": path,
            "digest": file_digest(&manifest).as_str(),
        }));
        files.push(SuiteFile {
            path,
            bytes: manifest,
        });
    }
    listing.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Ok(Cases { files, listing })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use super::*;
    use crate::conformance::index;

    /// The committed Suite Index and the Profile Specification it pins.
    fn committed() -> (Value, Value) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance");
        let read = |path: &str| -> Value {
            serde_json::from_slice(&std::fs::read(root.join(path)).unwrap()).unwrap()
        };
        let index = read(index::PATH);
        let profile = read(index["profileSpecification"]["path"].as_str().unwrap());
        (index, profile)
    }

    #[test]
    fn nothing_activates_an_evaluated_obligation_in_revision_zero() {
        let (index, profile) = committed();
        for (id, ..) in OBLIGATIONS {
            let expected = matches!(id, "015-XV-001" | "015-XV-002").then_some(false);
            assert_eq!(evaluated(id, &index, &profile), expected, "{id}");
            assert!(met(id, false, &index, &profile), "{id}");
            assert!(!met(id, true, &index, &profile), "{id}");
        }
    }

    #[test]
    fn a_second_configuration_schema_revision_activates_015_xv_001() {
        let (mut index, profile) = committed();
        let members = &mut index["configurationSchemaAuthoritySet"]["members"];
        let first = members["0"].clone();
        members["1"] = first;
        assert_eq!(evaluated("015-XV-001", &index, &profile), Some(true));
        assert!(!met("015-XV-001", false, &index, &profile));
        assert_eq!(evaluated("015-XV-002", &index, &profile), Some(false));
        index["configurationSchemaAuthoritySet"]["members"] = json!({});
        assert_eq!(evaluated("015-XV-001", &index, &profile), Some(false));
    }

    #[test]
    fn a_canonicalization_revision_not_known_to_refuse_extensions_activates_015_xv_002() {
        let (index, mut profile) = committed();
        let slot = &mut profile["specificationSlots"]["locale-canonicalization"];
        assert_eq!(slot["revision"], "0");
        slot["revision"] = json!("1");
        assert_eq!(evaluated("015-XV-002", &index, &profile), Some(true));
        assert!(!met("015-XV-002", false, &index, &profile));
        assert_eq!(evaluated("015-XV-001", &index, &profile), Some(false));
        profile["specificationSlots"] = json!({});
        assert_eq!(evaluated("015-XV-002", &index, &profile), Some(true));
    }

    #[test]
    fn each_row_is_one_inactive_manifest_under_the_decision_that_states_it() {
        let written = cases().unwrap();
        assert_eq!(written.files.len(), OBLIGATIONS.len());
        let ids = written
            .listing
            .iter()
            .map(|entry| entry["id"].as_str().unwrap())
            .collect::<Vec<_>>();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted);
        for (file, (id, activation, reason, owner)) in written.files.iter().zip(OBLIGATIONS) {
            assert_eq!(file.path, format!("{CASES}/{id}.json"));
            let manifest: Value = serde_json::from_slice(&file.bytes).unwrap();
            assert_eq!(manifest["id"], id);
            assert_eq!(manifest["activation"], activation);
            assert_eq!(manifest["activationState"], "inactive");
            assert_eq!(manifest["notApplicableReason"], reason);
            assert_eq!(manifest["verificationOwner"], owner);
            assert_eq!(
                manifest["traceability"]["decisionIds"],
                json!([decision(id)])
            );
        }
        // The decisions of the rows before the owners' forms, as the
        // Decision Log states them.
        let stated = [
            ("015-XV-001", "015-051"),
            ("015-XV-002", "015-212"),
            ("015-AD-001", "015-206"),
            ("015-AD-002", "015-212"),
            ("015-AD-003", "015-212"),
            ("015-AD-004", "015-212"),
            ("015-HV-001", "015-212"),
            ("015-HV-002", "015-212"),
        ];
        for (id, expected) in stated {
            assert_eq!(decision(id), expected, "{id}");
        }
        // 015-217 states exactly the owners' handoff-consumer forms.
        let forms = OBLIGATIONS
            .iter()
            .filter(|(id, ..)| decision(id) == "015-217")
            .map(|(_, activation, _, owner)| {
                assert_eq!(*activation, format!("{owner}-handoff-consumer-form"));
                *owner
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(forms.len(), 11);
        let decisions = OBLIGATIONS
            .iter()
            .map(|(id, ..)| decision(id))
            .collect::<BTreeSet<_>>();
        assert_eq!(
            decisions,
            BTreeSet::from(["015-051", "015-206", "015-212", "015-217"])
        );
    }
}
