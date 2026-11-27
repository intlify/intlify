// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The harness of a Resolver Case's entries (D15:3172-3237).
//!
//! An entry submits the case's common input and its own entry admission
//! input as one Resolver Invocation Input Set, each slot's input in its
//! boundary fixture's form (D15:3252). The harness admits the set against
//! the resolver the named construction case built, resolves it, and writes
//! the outcome in the expected-outcome form (D15:3271-3303): each
//! projection's records in their canonical order with their owners, the
//! terminal Finding and the Entry reserve's cause last without one, and the
//! binding keys in the order of their canonical bytes. A set the boundaries
//! do not admit as the fixtures give it, or a locator candidate that is not
//! in its canonical form, makes the case invalid rather than an expectation.
//!
//! The harness also checks what an outcome holds beyond its form (D15:3227,
//! 3229): every ordinary record's owner is one its check's Check Registry
//! row permits, no projection lists one Finding Key twice, and when both
//! domains are complete the Coverage Decision Evidence covers each Basis
//! exactly. Two paired entries carry one Resolver Construction Identity,
//! and two that reach equal Materialized Resolution Input Sets share their
//! Shared projection and Shared terminal state and, when both bundles are
//! complete, their outcome, profile view, and bindings (D15:3233).

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::boundary::invocation::{EntryInputs, InvocationInputs, SubmittedInvocation};
use crate::boundary::locator::Locator;
use crate::boundary::text::TextInput;
use crate::construction::registry::check::{kind_id, CheckRegistry, CheckRow, RecordKind};
use crate::construction::resolver::ConstructedResolver;
use crate::construction::root::ConstructionRoot;
use crate::locale::icu4x::Icu4xData;
use crate::resolver::invocation::{OutcomeStorage, Resolver};
use crate::resolver::materialized::MaterializedInputs;
use crate::resolver::outcome::{Checked, Outcome};
use crate::resolver::resolution_evidence::Projection;

use super::admission::{verification_assertion, with_common, with_entry};
use super::fixture::Bodies;

/// The arrays of the Shared projection, each with the record kind it lists,
/// in their order (D15:3271).
const SHARED: [(&str, RecordKind); 5] = [
    ("findings", RecordKind::SharedFinding),
    ("evaluationStatus", RecordKind::SharedEvaluationStatus),
    ("coverageDecisionBases", RecordKind::CoverageDecisionBasis),
    ("semanticDependencies", RecordKind::SemanticDependency),
    (
        "materializedDependencies",
        RecordKind::MaterializedDependency,
    ),
];

/// The arrays of the Entry projection, each with the record kind it lists,
/// in their order (D15:3271).
const ENTRY: [(&str, RecordKind); 7] = [
    ("findings", RecordKind::EntryFinding),
    ("evaluationStatus", RecordKind::EntryEvaluationStatus),
    ("occurrences", RecordKind::FindingOccurrence),
    (
        "coverageDecisionEvidence",
        RecordKind::CoverageDecisionEvidence,
    ),
    ("evidenceIndex", RecordKind::EvidenceIndex),
    ("entryDependencies", RecordKind::EntryDependency),
    (
        "sharedUnavailabilityCauses",
        RecordKind::SharedUnavailabilityCause,
    ),
];

/// What one entry of a Resolver Case resolves to: the outcome in the
/// expected-outcome form, and its Materialized Resolution Input Set once
/// Phase 1 materialized a value (D15:2658).
pub(crate) struct Resolved {
    pub(crate) outcome: Value,
    pub(crate) materialized: Option<MaterializedInputs<Icu4xData>>,
}

/// Why an entry of a Resolver Case cannot run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unrunnable {
    /// The boundaries do not admit the set as the fixtures give it: a
    /// candidate that is not its slot's ID, or an input its boundary refuses
    /// (D15:3252).
    NotAdmitted,
    /// A locator candidate is not in its canonical form, so its boundary
    /// admits it as absence (D15:3252).
    LocatorNotCanonical,
    /// A phase found a resolver bug.
    Resolver,
}

/// Admit one entry: the common input fixture `common` and the entry
/// admission input fixture `entry`, submitted with the identity reference
/// `reference` to the resolver `constructed` (D15:3172-3216, 3252); then `f`
/// with the resolver and the admitted set.
fn admitted<T>(
    common: &Value,
    entry: &Value,
    bodies: &Bodies,
    constructed: &ConstructedResolver,
    reference: &str,
    f: impl FnOnce(&Resolver<'_>, &InvocationInputs<Icu4xData>) -> Result<T, Unrunnable>,
) -> Result<T, Unrunnable> {
    let root = ConstructionRoot::built_in().expect("the built-in construction root verifies");
    let resolver = Resolver::new(constructed, root);
    let assertion = verification_assertion(&common["assertion"]);
    with_common(common, bodies, |submitted_common, _| {
        with_entry(entry, bodies, |submitted_entry| {
            let submitted = SubmittedInvocation {
                construction_identity: Some(TextInput::Text(reference)),
                common: submitted_common,
                entry: submitted_entry,
            };
            let inputs = resolver
                .admit(submitted, &assertion)
                .map_err(|_| Unrunnable::NotAdmitted)?;
            if !canonical(entry, &inputs) {
                return Err(Unrunnable::LocatorNotCanonical);
            }
            f(&resolver, &inputs)
        })
    })
}

/// Whether one entry's set is one the boundaries admit as its fixtures give
/// it, which the runner checks when it lists the case (D15:3252).
///
/// # Errors
///
/// Returns why the entry cannot run.
pub(crate) fn admissible(
    common: &Value,
    entry: &Value,
    bodies: &Bodies,
    constructed: &ConstructedResolver,
    reference: &str,
) -> Result<(), Unrunnable> {
    admitted(common, entry, bodies, constructed, reference, |_, _| Ok(()))
}

/// Resolve one entry, admitted as [`admissible`] admits it.
///
/// # Errors
///
/// Returns why the entry cannot run.
pub(crate) fn resolved(
    common: &Value,
    entry: &Value,
    bodies: &Bodies,
    constructed: &ConstructedResolver,
    reference: &str,
) -> Result<Resolved, Unrunnable> {
    admitted(
        common,
        entry,
        bodies,
        constructed,
        reference,
        |resolver, inputs| {
            let mut storage = OutcomeStorage::default();
            let resolution = resolver
                .resolve(inputs, &mut storage)
                .map_err(|_| Unrunnable::Resolver)?;
            Ok(Resolved {
                outcome: view(&resolution.outcome),
                materialized: resolution.materialized,
            })
        },
    )
}

/// Whether each locator candidate of `entry` is admitted as it is given, and
/// each locator absent where the fixture gives none: the entry's own, and a
/// programmatic entry's call-site sidecar's (D15:3252).
fn canonical(entry: &Value, inputs: &InvocationInputs<Icu4xData>) -> bool {
    let same = |input: &Value, admitted: Option<&Locator>| {
        let candidate = input.get("locator").map(|candidate| {
            let mut view = Map::new();
            view.insert(
                candidate["variant"].as_str().unwrap_or_default().to_owned(),
                candidate["payload"].clone(),
            );
            Value::Object(view)
        });
        candidate == admitted.map(Locator::view)
    };
    let slots = entry.as_object().into_iter().flatten();
    let input = slots
        .filter(|(_, slot)| slot["input"].get("identity").is_some())
        .map(|(_, slot)| &slot["input"])
        .next()
        .unwrap_or(&Value::Null);
    match &inputs.entry {
        EntryInputs::File(file) => same(input, file.locator.as_ref()),
        EntryInputs::Programmatic(programmatic) => {
            same(input, programmatic.locator.as_ref())
                && match (&input.get("callSite"), &programmatic.call_site) {
                    (Some(sidecar), Some(admitted)) => same(sidecar, admitted.locator()),
                    _ => true,
                }
        }
    }
}

/// `outcome` in the expected-outcome form (D15:3271-3303).
pub(crate) fn view(outcome: &Outcome<'_, '_>) -> Value {
    let evidence = &outcome.evidence;
    let checked = outcome.checked.as_ref();
    let causes = projection(&evidence.entry, &ENTRY)["sharedUnavailabilityCauses"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| row["record"]["cause"].clone())
        .collect::<Vec<_>>();
    json!({
        "outcome": if checked.is_some() { "checked" } else { "blocked" },
        "resolverConstructionIdentity": outcome.identity.text(),
        "profile": checked.map_or(Value::Null, |checked| {
            serde_json::to_value(&checked.profile).expect("a profile view serializes")
        }),
        "resolvedProfileArtifactBindings": checked.map_or(Value::Null, bindings),
        "resolutionEvidence": {
            "complete": evidence.complete(),
            "terminalStates": evidence
                .terminal_states()
                .map(|terminal| terminal.state.clone())
                .collect::<Vec<_>>(),
            "shared": evidence
                .shared
                .as_ref()
                .map_or(Value::Null, |shared| projection(shared, &SHARED)),
            "sharedUnavailableCauseKeys": causes,
            "entry": projection(&evidence.entry, &ENTRY),
        },
    })
}

/// One row per exact binding key, `{ "key": <exact reference> }`, in the
/// order of the keys' canonical bytes (D15:3303).
fn bindings(checked: &Checked<'_, '_>) -> Value {
    let mut keys = checked
        .bindings
        .iter()
        .map(|(tuple, _)| {
            let key = json!({
                "kind": tuple.kind.spelling(),
                "identity": tuple.identity.as_str(),
                "revision": tuple.revision.as_str(),
                "specificationRevision": tuple.specification_revision.as_str(),
                "semanticDigest": tuple.semantic_digest.as_str(),
            });
            let bytes =
                intlify_shared_json::encoding::encode(&key).expect("an exact reference encodes");
            (bytes, key)
        })
        .collect::<Vec<_>>();
    keys.sort_by(|(left, _), (right, _)| left.cmp(right));
    Value::Array(
        keys.into_iter()
            .map(|(_, key)| json!({ "key": key }))
            .collect(),
    )
}

/// `projection`'s records, array by array of `arrays`, each a row of its
/// owner and its view; an exhausted domain's terminal Finding last in its
/// `findings`, and the Entry reserve's cause last in its causes, neither
/// with an owner (D15:3303).
fn projection(projection: &Projection<'_>, arrays: &[(&str, RecordKind)]) -> Value {
    let mut view = Map::new();
    for (name, kind) in arrays {
        let mut rows = projection
            .records
            .iter()
            .filter(|(_, retained)| retained.kind == *kind)
            .map(|(_, retained)| json!({ "owner": retained.owner, "record": retained.view }))
            .collect::<Vec<_>>();
        if let Some(terminal) = &projection.terminal {
            match *name {
                "findings" => rows.push(json!({ "owner": null, "record": terminal.finding })),
                "sharedUnavailabilityCauses" => {
                    if let Some(cause) = &terminal.cause {
                        rows.push(json!({ "owner": null, "record": cause }));
                    }
                }
                _ => {}
            }
        }
        view.insert((*name).to_owned(), Value::Array(rows));
    }
    Value::Object(view)
}

/// What `outcome`, in the expected-outcome form, breaks of what the harness
/// checks beyond its form (D15:3227, 3229): each violation, in the order
/// found.
pub(crate) fn duties(outcome: &Value) -> Vec<String> {
    let mut violations = Vec::new();
    let registry = CheckRegistry::revision_zero();
    let rows: BTreeMap<&str, &CheckRow> = registry
        .rows
        .iter()
        .map(|row| (row.check_id.as_str(), row))
        .collect();
    // The owner is a unit of its check's row: the row's phase and subject
    // kind, and a row that may own the record's kind.
    let permitted = |owner: &Value, kind: RecordKind| {
        rows.get(owner["check"].as_str().unwrap_or_default())
            .is_some_and(|row| {
                owner["phase"] == row.phase_id.id()
                    && owner["subjectKind"] == kind_id(row.subject_kind)
                    && row.possible_evidence_record_kinds.contains(&kind)
            })
    };
    let evidence = &outcome["resolutionEvidence"];
    for (domain, arrays) in [("shared", &SHARED[..]), ("entry", &ENTRY[..])] {
        let Some(projection) = evidence[domain].as_object() else {
            continue;
        };
        for (name, kind) in arrays {
            let rows = projection[*name].as_array().into_iter().flatten();
            for row in rows.filter(|row| !row["owner"].is_null()) {
                if !permitted(&row["owner"], *kind) {
                    let check = row["owner"]["check"].as_str().unwrap_or_default();
                    violations.push(format!("{domain}.{name}: {check} owns no {}", kind.id()));
                }
            }
        }
        let mut keys = Vec::new();
        for row in projection["findings"].as_array().into_iter().flatten() {
            let key = &row["record"]["key"];
            if keys.contains(&key) {
                violations.push(format!("{domain}.findings: a Finding Key listed twice"));
            }
            keys.push(key);
        }
    }
    if evidence["complete"] == true {
        violations.extend(cover(evidence));
    }
    violations
}

/// Where the Coverage Decision Evidence does not cover the Bases exactly:
/// one mapping for each `default` Basis, one per matched rule domain for each
/// `rule` Basis, and none besides (D15:3227).
fn cover(evidence: &Value) -> Vec<String> {
    let records = |projection: &Value, name: &str| -> Vec<Value> {
        projection[name]
            .as_array()
            .into_iter()
            .flatten()
            .map(|row| row["record"].clone())
            .collect()
    };
    let bases = records(&evidence["shared"], "coverageDecisionBases");
    let mut mappings = records(&evidence["entry"], "coverageDecisionEvidence");
    let mut violations = Vec::new();
    for basis in &bases {
        let sources = match basis["decision"].as_str() {
            Some("default") => vec![json!("default")],
            _ => basis["domains"].as_array().cloned().unwrap_or_default(),
        };
        for source in sources {
            let found = mappings.iter().position(|mapping| {
                mapping["cell"] == basis["cell"] && mapping["source"] == source
            });
            match found {
                Some(index) => {
                    mappings.remove(index);
                }
                None => {
                    violations.push(format!("no Evidence maps {} from {source}", basis["cell"]));
                }
            }
        }
    }
    violations.extend(
        mappings
            .iter()
            .map(|mapping| format!("an extra or orphan mapping of {}", mapping["cell"])),
    );
    violations
}

/// Whether paired entries' outcomes agree where 015 makes them agree: the
/// Resolver Construction Identity always, and once both reached equal
/// Materialized Resolution Input Sets (`materialized`), the Shared projection
/// and the Shared terminal state, and when both bundles are complete, the
/// outcome, the profile view, and the bindings (D15:3233).
pub(crate) fn paired(file: &Value, programmatic: &Value, materialized: bool) -> bool {
    fn evidence(outcome: &Value) -> &Value {
        &outcome["resolutionEvidence"]
    }
    let shared_terminal = |outcome: &Value| {
        evidence(outcome)["terminalStates"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|state| state["domain"] == "shared")
            .cloned()
            .collect::<Vec<_>>()
    };
    let shared = evidence(file)["shared"] == evidence(programmatic)["shared"]
        && shared_terminal(file) == shared_terminal(programmatic);
    let complete = evidence(file)["complete"] == true && evidence(programmatic)["complete"] == true;
    let identity =
        file["resolverConstructionIdentity"] == programmatic["resolverConstructionIdentity"];
    identity
        && (!materialized || shared)
        && (!materialized
            || !complete
            || ["outcome", "profile", "resolvedProfileArtifactBindings"]
                .iter()
                .all(|member| file[*member] == programmatic[*member]))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn empty() -> Value {
        json!({
            "resolutionEvidence": {
                "complete": true,
                "terminalStates": [],
                "shared": {
                    "findings": [],
                    "evaluationStatus": [],
                    "coverageDecisionBases": [],
                    "semanticDependencies": [],
                    "materializedDependencies": []
                },
                "sharedUnavailableCauseKeys": [],
                "entry": {
                    "findings": [],
                    "evaluationStatus": [],
                    "occurrences": [],
                    "coverageDecisionEvidence": [],
                    "evidenceIndex": [],
                    "entryDependencies": [],
                    "sharedUnavailabilityCauses": []
                }
            }
        })
    }

    /// A record's owner must be one its check's Check Registry row permits
    /// to own the record's kind, and a reserved row has no owner to check.
    #[test]
    fn an_owner_its_check_does_not_permit_is_a_violation() {
        let registry = CheckRegistry::revision_zero();
        let status = RecordKind::SharedEvaluationStatus;
        let owning = registry
            .rows
            .iter()
            .find(|row| row.possible_evidence_record_kinds.contains(&status))
            .unwrap();
        let other = registry
            .rows
            .iter()
            .find(|row| {
                !row.possible_evidence_record_kinds.contains(&status)
                    && row.phase_id != owning.phase_id
                    && row.subject_kind != owning.subject_kind
            })
            .unwrap();
        let owner = json!({
            "phase": owning.phase_id.id(),
            "subjectKind": kind_id(owning.subject_kind),
            "subject": { "token": "t" },
            "check": owning.check_id,
        });
        let with = |owner: &Value| {
            let mut outcome = empty();
            outcome["resolutionEvidence"]["shared"]["evaluationStatus"] =
                json!([{ "owner": owner, "record": {} }]);
            outcome["resolutionEvidence"]["shared"]["findings"] =
                json!([{ "owner": null, "record": { "key": 1 } }]);
            duties(&outcome)
        };
        assert!(with(&owner).is_empty());
        let changed = [
            ("check", json!(other.check_id)),
            ("phase", json!(other.phase_id.id())),
            ("subjectKind", json!(kind_id(other.subject_kind))),
        ];
        for (member, value) in changed {
            let mut wrong = owner.clone();
            wrong[member] = value;
            assert_eq!(with(&wrong).len(), 1, "{member}");
        }
    }

    /// A projection lists each Finding Key once.
    #[test]
    fn a_finding_key_listed_twice_is_a_violation() {
        let mut outcome = empty();
        let row = json!({ "owner": null, "record": { "key": { "code": "c" } } });
        outcome["resolutionEvidence"]["entry"]["findings"] = json!([row.clone()]);
        assert!(duties(&outcome).is_empty());
        outcome["resolutionEvidence"]["entry"]["findings"] = json!([row.clone(), row]);
        assert_eq!(duties(&outcome).len(), 1);
    }

    /// Each `default` Basis has one mapping, each `rule` Basis one per
    /// matched domain, and no mapping is left over; an incomplete bundle is
    /// not checked.
    #[test]
    fn the_decision_evidence_covers_the_bases_exactly() {
        let cell = json!({ "locale": "en", "surfaceClass": "nav" });
        let domain = json!({ "locales": ["en"], "surfaceClasses": ["nav"] });
        let mapping =
            |source: &Value| json!({ "owner": null, "record": { "cell": cell, "source": source } });
        let mut outcome = empty();
        outcome["resolutionEvidence"]["shared"]["coverageDecisionBases"] = json!([
            { "owner": null, "record": { "cell": cell, "decision": "default", "domains": [] } },
            { "owner": null, "record": { "cell": cell, "decision": "rule", "domains": [domain] } }
        ]);
        let entry = &mut outcome["resolutionEvidence"]["entry"]["coverageDecisionEvidence"];
        *entry = json!([mapping(&json!("default")), mapping(&domain)]);
        assert!(cover(&outcome["resolutionEvidence"]).is_empty());
        outcome["resolutionEvidence"]["entry"]["coverageDecisionEvidence"] =
            json!([mapping(&json!("default"))]);
        assert_eq!(cover(&outcome["resolutionEvidence"]).len(), 1);
        outcome["resolutionEvidence"]["entry"]["coverageDecisionEvidence"] = json!([
            mapping(&json!("default")),
            mapping(&domain),
            mapping(&domain)
        ]);
        assert_eq!(cover(&outcome["resolutionEvidence"]).len(), 1);
        outcome["resolutionEvidence"]["complete"] = json!(false);
        outcome["resolutionEvidence"]["entry"]["coverageDecisionEvidence"] = json!([]);
        assert!(duties(&outcome)
            .iter()
            .all(|violation| !violation.contains("maps")));
    }

    /// An entry runs only on inputs its boundaries admit as its fixtures give
    /// them: a refused input or a candidate of another slot is not admitted,
    /// and a locator, the entry's own or its call-site sidecar's, that its
    /// boundary admits as absence is not canonical; an absent locator is
    /// absent (D15:3252).
    #[test]
    fn an_entry_runs_on_inputs_admitted_as_given_only() {
        use crate::boundary::source::call_site_source_revision;
        use crate::conformance::resolver_cases::{data_files, Storefront};
        use crate::construction::resolver::committed;
        use crate::resolver::evidence_source::hex;
        let constructed = committed();
        let reference = constructed
            .identity(ConstructionRoot::built_in().unwrap())
            .text();
        let storefront = Storefront::new();
        let common = storefront.common(constructed);
        let bodies: Bodies = data_files()
            .into_iter()
            .map(|(path, bytes)| (path, bytes.to_vec()))
            .collect();
        let admit = |common: &Value, entry: &Value| {
            admissible(common, entry, &bodies, constructed, &reference)
        };
        assert_eq!(admit(&common, &storefront.file()), Ok(()));
        let mut locator = storefront.file();
        locator["configuration-file"]["input"]["locator"]["payload"] =
            json!("./intlify.config.json");
        assert_eq!(
            admit(&common, &locator),
            Err(Unrunnable::LocatorNotCanonical)
        );
        let mut absent = storefront.file();
        absent["configuration-file"]["input"]
            .as_object_mut()
            .unwrap()
            .remove("locator");
        assert_eq!(admit(&common, &absent), Ok(()));
        let mut identity = storefront.programmatic();
        identity["configuration-programmatic"]["input"]
            .as_object_mut()
            .unwrap()
            .remove("identity");
        assert_eq!(admit(&common, &identity), Err(Unrunnable::NotAdmitted));
        let mut candidate = common.clone();
        candidate["implementation-capability"]["candidate"] =
            json!("intent-surface-class-vocabulary");
        assert_eq!(
            admit(&candidate, &storefront.file()),
            Err(Unrunnable::NotAdmitted)
        );
        let source = b"defineConfig({})\n";
        let sidecar = |payload: &str| {
            json!({
                "identity": "intlify-call-site.v0:storefront/app.ts",
                "nonSecret": false,
                "revision": call_site_source_revision(source),
                "locator": { "variant": "safe-label", "payload": payload, "nonSecret": true },
                "source": { "bytes": hex(source) },
                "mappings": [{ "path": [], "role": "node", "span": { "start": 0, "end": 16 } }],
            })
        };
        let mut call_site = storefront.programmatic();
        call_site["configuration-programmatic"]["input"]["callSite"] = sidecar("app.ts");
        assert_eq!(admit(&common, &call_site), Ok(()));
        call_site["configuration-programmatic"]["input"]["callSite"] = sidecar("");
        assert_eq!(
            admit(&common, &call_site),
            Err(Unrunnable::LocatorNotCanonical)
        );
    }

    /// Paired entries that reached equal sets share the Shared projection
    /// and Shared terminal state, and when both bundles are complete their
    /// outcome and profile; their Entry projections may differ. Entries
    /// that did not reach equal sets share only the construction identity.
    #[test]
    fn paired_entries_share_what_015_makes_entry_independent() {
        let file = json!({
            "outcome": "checked",
            "resolverConstructionIdentity": "rci0_a",
            "profile": { "a": 1 },
            "resolvedProfileArtifactBindings": [],
            "resolutionEvidence": {
                "complete": true,
                "terminalStates": [],
                "shared": { "findings": [] },
                "entry": { "findings": [1] }
            }
        });
        let mut programmatic = file.clone();
        programmatic["resolutionEvidence"]["entry"] = json!({ "findings": [2] });
        assert!(paired(&file, &programmatic, true));
        let mut other = programmatic.clone();
        other["resolutionEvidence"]["shared"] = json!({ "findings": [3] });
        assert!(!paired(&file, &other, true));
        assert!(paired(&file, &other, false));
        let mut profile = programmatic.clone();
        profile["profile"] = json!({ "a": 2 });
        assert!(!paired(&file, &profile, true));
        assert!(paired(&file, &profile, false));
        profile["resolutionEvidence"]["complete"] = json!(false);
        assert!(paired(&file, &profile, true));
        let mut terminal = programmatic.clone();
        terminal["resolutionEvidence"]["terminalStates"] = json!([{ "domain": "shared" }]);
        assert!(!paired(&file, &terminal, true));
        let mut identity = programmatic;
        identity["resolverConstructionIdentity"] = json!("rci0_b");
        assert!(!paired(&file, &identity, true));
        assert!(!paired(&file, &identity, false));
    }
}
