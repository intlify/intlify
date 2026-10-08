// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Reading registry snapshots and updates, and what admitting one establishes.
//!
//! Every case starts from an artifact of the committed chain and changes one
//! thing about it, resealed so that each structural rule is reached rather
//! than stopped at the digest.

mod support;

use intlify_authoring::test_context::admit_inventory;
use intlify_authoring::{
    AnalysisWorkspace, ArtifactRelation, AuthoringArtifact, ByteRange, MessageIntentId, Occurrence,
    OccurrenceRole, ReadFailure, SourceBytes, SourceSnapshot, VersionedIdentity,
};
use intlify_authoring_identity::{
    admit_registry, admit_update, AllocationBasis, ContinuationBasis, EntryState, IdentityDecision,
    IdentityLimitKind, IdentityLimits, IntentRegistrySnapshot, IntentRegistryUpdate, LineageKind,
    LineageLink, RegistryAdmissionFailure, RegistryArtifact, RegistryEntry, RegistryUpdateArtifact,
    Replacement, SnapshotFailure, SourceEdit, UpdateFailure,
};
use serde_json::{json, Value};
use support::{artifact, authoring_limits, bytes, context, document, limits, owner, reseal};

const REGISTRY_SCHEMA: &str = include_str!("../schema/intent-registry-v0.schema.json");
const UPDATE_SCHEMA: &str = include_str!("../schema/intent-registry-update-v0.schema.json");

/// One labelled edit to an artifact's JSON, for refusal tables.
type Damage = (&'static str, fn(&mut Value));

/// A labelled edit to one committed snapshot, and the rule it breaks.
type SnapshotCase = (&'static str, &'static str, fn(&mut Value), SnapshotFailure);

/// A labelled edit to update-2, and the rule it breaks.
type UpdateCase = (&'static str, fn(&mut Value), UpdateFailure);

fn structure<F>(failure: F) -> RegistryAdmissionFailure<F> {
    RegistryAdmissionFailure::Structure(failure)
}

#[test]
fn every_committed_vector_is_admitted_and_names_what_came_before_it() {
    // The vectors are what a second implementation checks, so they have to be
    // artifacts the readers actually admit, not merely documents that hash.
    let document = document();
    let texts: Vec<(String, String, String)> = document["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| {
            (
                source["unit"].as_str().unwrap().to_owned(),
                source["revision"].as_str().unwrap().to_owned(),
                source["text"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let context = context();
    let mut earlier = Vec::new();
    let mut kinds = Vec::new();
    for vector in document["artifacts"].as_array().unwrap() {
        let id = vector["id"].as_str().unwrap();
        let encoded = bytes(&vector["artifact"]);
        match vector["artifact"]["kind"].as_str().unwrap() {
            "authoring-inventory" => {
                let units = vector["artifact"]["body"]["units"].as_array().unwrap();
                let sources: Vec<SourceBytes<'_>> = units
                    .iter()
                    .map(|unit| {
                        let (unit, revision) =
                            (&unit["source"]["unit"], &unit["source"]["revision"]);
                        let (unit, _, text) = texts
                            .iter()
                            .find(|(name, number, _)| unit == name && revision == number)
                            .expect("retained text for every unit");
                        SourceBytes {
                            unit,
                            bytes: text.as_bytes(),
                        }
                    })
                    .collect();
                let admitted = admit_inventory(
                    &encoded,
                    &context,
                    &sources,
                    &authoring_limits(),
                    &mut AnalysisWorkspace::new(),
                )
                .unwrap_or_else(|failure| panic!("{id} was refused: {failure:?}"));
                assert!(admitted.unverified_units().is_empty(), "{id}");
                earlier.push(admitted.reference());
            }
            "intent-registry" => {
                let admitted = admit_registry(&encoded, &limits())
                    .unwrap_or_else(|failure| panic!("{id} was refused: {failure:?}"));
                let snapshot = admitted.snapshot();
                for named in snapshot.base().into_iter().chain(snapshot.update()) {
                    assert!(earlier.contains(named), "{id} names an artifact before it");
                }
                assert_eq!(snapshot.is_genesis(), id == "registry-0");
                earlier.push(admitted.reference());
            }
            "intent-registry-update" => {
                let admitted = admit_update(&encoded, &limits())
                    .unwrap_or_else(|failure| panic!("{id} was refused: {failure:?}"));
                let update = admitted.update();
                assert!(earlier.contains(update.base()), "{id}");
                assert!(earlier.contains(update.inventory()), "{id}");
                assert!(!update.is_empty(), "{id}");
                for decision in update.decisions() {
                    kinds.push(match decision {
                        IdentityDecision::Continue(_) => "continue",
                        IdentityDecision::Allocate(_) => "allocate",
                        IdentityDecision::Retire(_) => "retire",
                        IdentityDecision::Restore(_) => "restore",
                    });
                }
                earlier.push(admitted.reference());
            }
            other => panic!("{id} has an unexpected kind {other}"),
        }
    }
    kinds.sort_unstable();
    kinds.dedup();
    assert_eq!(
        kinds,
        ["allocate", "continue", "restore", "retire"],
        "the chain holds every decision kind"
    );
}

#[test]
fn a_snapshot_and_an_update_are_never_read_as_each_other() {
    let snapshot = bytes(&artifact("registry-1"));
    let update = bytes(&artifact("update-1"));
    assert_eq!(
        admit_registry(&update, &limits()),
        Err(RegistryAdmissionFailure::Read(ReadFailure::Unsupported))
    );
    assert_eq!(
        admit_update(&snapshot, &limits()),
        Err(RegistryAdmissionFailure::Read(ReadFailure::Unsupported))
    );
}

#[test]
fn a_tuple_this_reader_does_not_implement_is_unsupported_not_malformed() {
    let cases: [Damage; 3] = [
        ("a later schema revision", |v| {
            v["schemaRevision"] = json!("1");
        }),
        ("a later specification revision", |v| {
            v["authoringSpecification"]["revision"] = json!("1");
        }),
        ("another specification", |v| {
            v["authoringSpecification"]["identity"] = json!("intlify-design-017");
        }),
    ];
    for (label, damage) in cases {
        let mut registry = artifact("registry-1");
        damage(&mut registry);
        assert_eq!(
            admit_registry(&reseal(registry), &limits()),
            Err(RegistryAdmissionFailure::Read(ReadFailure::Unsupported)),
            "{label}"
        );
        let mut update = artifact("update-2");
        damage(&mut update);
        assert_eq!(
            admit_update(&reseal(update), &limits()),
            Err(RegistryAdmissionFailure::Read(ReadFailure::Unsupported)),
            "{label}"
        );
    }
}

#[test]
fn content_changed_after_sealing_is_refused_before_anything_reads_it() {
    let mut registry = artifact("registry-2");
    registry["body"]["entries"][2]["state"] = json!("active");
    assert_eq!(
        admit_registry(&bytes(&registry), &limits()),
        Err(RegistryAdmissionFailure::Read(ReadFailure::Integrity))
    );
    let mut update = artifact("update-3");
    update["body"]["decisions"][1]["basis"]["reason"] = json!("A different explanation.");
    assert_eq!(
        admit_update(&bytes(&update), &limits()),
        Err(RegistryAdmissionFailure::Read(ReadFailure::Integrity))
    );
}

/// The first entry of the first non-genesis snapshot, for cases that need a
/// real one somewhere else.
fn first_entry() -> Value {
    artifact("registry-1")["body"]["entries"][0].clone()
}

#[test]
fn each_snapshot_rule_is_refused_by_its_own_name() {
    // registry-1 holds three active entries in Intent ID order: pay and
    // cancel in checkout revision 1, and home in nav.
    let cases: [SnapshotCase; 13] = [
        (
            "a base without its update",
            "registry-1",
            |v| {
                v["body"].as_object_mut().unwrap().remove("update");
            },
            SnapshotFailure::UnpairedHistory,
        ),
        (
            "an update without its base",
            "registry-1",
            |v| {
                v["body"].as_object_mut().unwrap().remove("base");
            },
            SnapshotFailure::UnpairedHistory,
        ),
        (
            "a genesis with an entry",
            "registry-0",
            |v| v["body"]["entries"] = json!([first_entry()]),
            SnapshotFailure::GenesisEntries,
        ),
        (
            "a base naming another kind",
            "registry-1",
            |v| v["body"]["base"]["kind"] = json!("message-intent"),
            SnapshotFailure::ReferenceKind,
        ),
        (
            "a base under a later schema revision",
            "registry-1",
            |v| v["body"]["base"]["schemaRevision"] = json!("1"),
            SnapshotFailure::ReferenceKind,
        ),
        (
            "an update naming a registry",
            "registry-1",
            |v| v["body"]["update"]["kind"] = json!("intent-registry"),
            SnapshotFailure::ReferenceKind,
        ),
        (
            "an ID of another owner",
            "registry-1",
            |v| v["body"]["entries"][0]["intentId"]["owner"]["kind"] = json!("library"),
            SnapshotFailure::ForeignOwner,
        ),
        (
            "a declaration of another owner",
            "registry-1",
            |v| {
                v["body"]["entries"][0]["declaration"]["source"]["owner"]["identity"] =
                    json!("elsewhere");
            },
            SnapshotFailure::ForeignOwner,
        ),
        (
            "an ID held twice",
            "registry-1",
            |v| {
                let entries = v["body"]["entries"].as_array_mut().unwrap();
                entries.insert(1, entries[0].clone());
            },
            SnapshotFailure::DuplicateEntry,
        ),
        (
            "entries out of Intent ID order",
            "registry-1",
            |v| v["body"]["entries"].as_array_mut().unwrap().reverse(),
            SnapshotFailure::EntriesUnordered,
        ),
        (
            "a use site where a declaration belongs",
            "registry-1",
            |v| v["body"]["entries"][0]["declaration"]["role"] = json!("reference"),
            SnapshotFailure::RoleMismatch,
        ),
        (
            "a declaration past the end of its unit",
            "registry-1",
            |v| v["body"]["entries"][0]["declaration"]["range"]["end"] = json!("63"),
            SnapshotFailure::RangeOutsideSource,
        ),
        (
            "two active IDs on one declaration",
            "registry-1",
            |v| {
                v["body"]["entries"][2]["declaration"] =
                    v["body"]["entries"][0]["declaration"].clone();
            },
            SnapshotFailure::SharedDeclaration,
        ),
    ];
    for (label, start, damage, expected) in cases {
        let mut value = artifact(start);
        damage(&mut value);
        assert_eq!(
            admit_registry(&reseal(value), &limits()),
            Err(structure(expected)),
            "{label}"
        );
    }
}

#[test]
fn a_retired_entry_keeps_a_declaration_an_active_one_may_hold_again() {
    // A retired ID keeps its last declaration, and the same exact position
    // can come back under another ID, for example after a revert. Only two
    // active IDs on one declaration contradict each other.
    let mut value = artifact("registry-1");
    value["body"]["entries"][2]["state"] = json!("retired");
    value["body"]["entries"][2]["declaration"] = value["body"]["entries"][0]["declaration"].clone();
    assert!(admit_registry(&reseal(value), &limits()).is_ok());
}

/// The nav unit's snapshot, for a second edit in one change list.
fn nav_snapshot() -> Value {
    artifact("registry-1")["body"]["entries"][1]["declaration"]["source"].clone()
}

/// update-2's change list, which holds one edit of checkout.
fn changes(value: &mut Value) -> &mut Vec<Value> {
    value["body"]["decisions"][0]["basis"]["changes"]
        .as_array_mut()
        .unwrap()
}

/// That edit's two replacements: the header insertion and the copied line.
fn replacements(value: &mut Value) -> &mut Vec<Value> {
    changes(value)[0]["replacements"].as_array_mut().unwrap()
}

/// One of the Intent IDs update-2 acts on, by its position there.
fn decision_id(value: &Value, index: usize) -> Value {
    value["body"]["decisions"][index]["intentId"].clone()
}

#[test]
fn each_update_rule_is_refused_by_its_own_name() {
    // update-2 continues pay (index 0) across one edit, retires cancel (1),
    // allocates the copy of pay (2), and links pay to its copy.
    let cases: [UpdateCase; 29] = [
        (
            "a base naming an update",
            |v| v["body"]["base"]["kind"] = json!("intent-registry-update"),
            UpdateFailure::ReferenceKind,
        ),
        (
            "an inventory naming a registry",
            |v| v["body"]["inventory"]["kind"] = json!("intent-registry"),
            UpdateFailure::ReferenceKind,
        ),
        (
            "a decision on another owner's ID",
            |v| v["body"]["decisions"][1]["intentId"]["owner"]["identity"] = json!("elsewhere"),
            UpdateFailure::ForeignOwner,
        ),
        (
            "a declaration of another owner",
            |v| v["body"]["decisions"][0]["to"]["source"]["owner"]["identity"] = json!("elsewhere"),
            UpdateFailure::ForeignOwner,
        ),
        (
            "an edit of another owner's unit",
            |v| changes(v)[0]["before"]["owner"]["identity"] = json!("elsewhere"),
            UpdateFailure::ForeignOwner,
        ),
        (
            "a link to another owner's ID",
            |v| {
                v["body"]["lineageLinks"][0]["predecessors"][0]["owner"]["identity"] =
                    json!("elsewhere");
            },
            UpdateFailure::ForeignOwner,
        ),
        (
            "two decisions on one ID",
            |v| {
                let decisions = v["body"]["decisions"].as_array_mut().unwrap();
                decisions.insert(1, decisions[0].clone());
            },
            UpdateFailure::DuplicateDecision,
        ),
        (
            "decisions out of Intent ID order",
            |v| v["body"]["decisions"].as_array_mut().unwrap().swap(0, 2),
            UpdateFailure::DecisionsUnordered,
        ),
        (
            "an exclusion where a declaration belongs",
            |v| v["body"]["decisions"][2]["to"]["role"] = json!("exclusion"),
            UpdateFailure::RoleMismatch,
        ),
        (
            "a declaration past the end of its unit",
            |v| v["body"]["decisions"][2]["to"]["range"]["end"] = json!("86"),
            UpdateFailure::RangeOutsideSource,
        ),
        (
            "two identities for one current declaration",
            |v| v["body"]["decisions"][2]["to"] = v["body"]["decisions"][0]["to"].clone(),
            UpdateFailure::SharedDeclaration,
        ),
        (
            "an unchanged snapshot that moved",
            |v| v["body"]["decisions"][0]["basis"] = json!({"kind": "unchanged-snapshot"}),
            UpdateFailure::UnchangedSnapshotMoved,
        ),
        (
            "an edit with no side",
            |v| {
                let edit = changes(v)[0].as_object_mut().unwrap();
                edit.remove("before");
                edit.remove("after");
            },
            UpdateFailure::EmptyEdit,
        ),
        (
            "one unit twice on a side",
            |v| {
                let edits = changes(v);
                edits.push(edits[0].clone());
            },
            UpdateFailure::DuplicateUnit,
        ),
        (
            "a change list out of source order",
            |v| {
                let nav =
                    json!({"before": nav_snapshot(), "after": nav_snapshot(), "replacements": []});
                changes(v).insert(0, nav);
            },
            UpdateFailure::ChangesUnordered,
        ),
        (
            "a replacement past the end of the before bytes",
            |v| replacements(v)[1]["range"]["end"] = json!("63"),
            UpdateFailure::ReplacementOutsideSource,
        ),
        (
            "a replacement in an empty starting buffer",
            |v| {
                changes(v)[0].as_object_mut().unwrap().remove("before");
            },
            UpdateFailure::ReplacementOutsideSource,
        ),
        (
            "replacements out of order",
            |v| replacements(v).reverse(),
            UpdateFailure::ReplacementsUnordered,
        ),
        (
            "overlapping replacements",
            |v| replacements(v)[0]["range"] = json!({"start": "0", "end": "40"}),
            UpdateFailure::ReplacementsUnordered,
        ),
        (
            "an insertion recorded after a replacement at its position",
            |v| {
                let list = replacements(v);
                list[0]["range"] = json!({"start": "30", "end": "30"});
                list.reverse();
            },
            UpdateFailure::ReplacementsUnordered,
        ),
        (
            "two insertions at one position",
            |v| {
                let list = replacements(v);
                list.insert(1, list[0].clone());
            },
            UpdateFailure::UncombinedInsertions,
        ),
        (
            "a copy with two successors",
            |v| {
                let cancel = decision_id(v, 1);
                let copy = decision_id(v, 2);
                v["body"]["lineageLinks"][0]["successors"] = json!([cancel, copy]);
            },
            UpdateFailure::LinkCardinality,
        ),
        (
            "a copy of an ID onto itself",
            |v| v["body"]["lineageLinks"][0]["successors"] = json!([decision_id(v, 0)]),
            UpdateFailure::LinkCardinality,
        ),
        (
            "a split with one successor",
            |v| v["body"]["lineageLinks"][0]["kind"] = json!("split"),
            UpdateFailure::LinkCardinality,
        ),
        (
            "a merge with one predecessor",
            |v| v["body"]["lineageLinks"][0]["kind"] = json!("merge"),
            UpdateFailure::LinkCardinality,
        ),
        (
            "a link's set out of Intent ID order",
            |v| {
                let (pay, copy) = (decision_id(v, 0), decision_id(v, 2));
                v["body"]["lineageLinks"][0] = json!({"kind": "split", "predecessors": [pay.clone()], "successors": [copy, pay]});
            },
            UpdateFailure::LinkMembersUnordered,
        ),
        (
            "a link's set naming one ID twice",
            |v| {
                let (pay, copy) = (decision_id(v, 0), decision_id(v, 2));
                v["body"]["lineageLinks"][0] = json!({"kind": "split", "predecessors": [pay], "successors": [copy.clone(), copy]});
            },
            UpdateFailure::LinkMembersUnordered,
        ),
        (
            "links out of order",
            |v| {
                let (pay, copy) = (decision_id(v, 0), decision_id(v, 2));
                let split = json!({"kind": "split", "predecessors": [pay.clone()], "successors": [pay, copy]});
                v["body"]["lineageLinks"]
                    .as_array_mut()
                    .unwrap()
                    .insert(0, split);
            },
            UpdateFailure::LinksUnordered,
        ),
        (
            "a copy whose successor this update does not allocate",
            |v| v["body"]["lineageLinks"][0]["successors"] = json!([decision_id(v, 1)]),
            UpdateFailure::CopyNotAllocated,
        ),
    ];
    for (label, damage, expected) in cases {
        let mut value = artifact("update-2");
        damage(&mut value);
        assert_eq!(
            admit_update(&reseal(value), &limits()),
            Err(structure(expected)),
            "{label}"
        );
    }

    let mut repeated = artifact("update-2");
    let links = repeated["body"]["lineageLinks"].as_array_mut().unwrap();
    links.push(links[0].clone());
    assert_eq!(
        admit_update(&reseal(repeated), &limits()),
        Err(structure(UpdateFailure::DuplicateLink))
    );
}

#[test]
fn an_insertion_may_come_just_before_a_replacement_at_its_position() {
    // An empty range addresses no bytes, so an insertion at a replacement's
    // start overlaps nothing. 017's order puts it first, which says where its
    // text goes.
    let mut value = artifact("update-2");
    replacements(&mut value)[1]["range"]["start"] = json!("0");
    assert!(admit_update(&reseal(value), &limits()).is_ok());
}

#[test]
fn a_split_may_keep_its_predecessor_among_its_successors() {
    // A predecessor that continues is still one of the results of a split,
    // so naming it on both sides is a well-formed link. Whether it resolves
    // in the base and the result is the transition's question.
    let mut value = artifact("update-2");
    let (pay, copy) = (decision_id(&value, 0), decision_id(&value, 2));
    value["body"]["lineageLinks"][0] =
        json!({"kind": "split", "predecessors": [pay.clone()], "successors": [pay, copy]});
    assert!(admit_update(&reseal(value), &limits()).is_ok());
}

#[test]
fn the_committed_schemas_refuse_what_the_readers_refuse() {
    // A schema looser than its reader lets a consumer validate a document the
    // reader would never admit. Every refusal Draft 7 can express is checked
    // on both sides, including the reference narrowing the reader applies by
    // name after decoding.
    let registry_cases: [Damage; 12] = [
        ("an unknown top-level member", |v| v["extra"] = json!(true)),
        ("an update read as a registry", |v| {
            v["kind"] = json!("intent-registry-update");
        }),
        ("a later schema revision", |v| {
            v["schemaRevision"] = json!("1");
        }),
        ("a later specification revision", |v| {
            v["authoringSpecification"]["revision"] = json!("1");
        }),
        ("a missing integrity digest", |v| {
            v.as_object_mut().unwrap().remove("integrityDigest");
        }),
        ("a digest in another presentation", |v| {
            v["integrityDigest"] = json!(format!("md5:{}", "0".repeat(32)));
        }),
        ("an unknown body member", |v| {
            v["body"]["extra"] = json!(true);
        }),
        ("a base naming another kind", |v| {
            v["body"]["base"]["kind"] = json!("message-intent");
        }),
        ("a base under a later schema revision", |v| {
            v["body"]["base"]["schemaRevision"] = json!("1");
        }),
        ("an unregistered entry state", |v| {
            v["body"]["entries"][0]["state"] = json!("deleted");
        }),
        ("an uppercase registry identity", |v| {
            let upper = v["body"]["registryIdentity"]
                .as_str()
                .unwrap()
                .to_uppercase();
            v["body"]["registryIdentity"] = json!(upper);
        }),
        ("an Intent ID one digit short", |v| {
            v["body"]["entries"][0]["intentId"]["value"] = json!("0".repeat(31));
        }),
    ];
    let update_cases: [Damage; 10] = [
        ("an unregistered decision kind", |v| {
            v["body"]["decisions"][2]["kind"] = json!("rename");
        }),
        ("a retirement with an explicit basis", |v| {
            v["body"]["decisions"][1]["basis"] = json!({"kind": "explicit", "reason": "gone"});
        }),
        ("an allocation with an unchanged-snapshot basis", |v| {
            v["body"]["decisions"][2]["basis"] = json!({"kind": "unchanged-snapshot"});
        }),
        ("an explicit basis without a reason", |v| {
            v["body"]["decisions"][2]["basis"] = json!({"kind": "explicit", "reason": ""});
        }),
        ("a verified edit without a profile", |v| {
            v["body"]["decisions"][0]["basis"]
                .as_object_mut()
                .unwrap()
                .remove("profile");
        }),
        ("an inventory naming a registry", |v| {
            v["body"]["inventory"]["kind"] = json!("intent-registry");
        }),
        ("a base under a later specification", |v| {
            v["body"]["base"]["authoringSpecification"]["revision"] = json!("1");
        }),
        ("an unregistered lineage kind", |v| {
            v["body"]["lineageLinks"][0]["kind"] = json!("fork");
        }),
        ("an offset that is not a decimal string", |v| {
            changes(v)[0]["replacements"][0]["range"]["start"] = json!("zero");
        }),
        ("a replacement with an unknown member", |v| {
            changes(v)[0]["replacements"][0]["note"] = json!("x");
        }),
    ];

    let registry_schema: Value = serde_json::from_str(REGISTRY_SCHEMA).unwrap();
    let registry_validator = jsonschema::draft7::new(&registry_schema).expect("a valid schema");
    let update_schema: Value = serde_json::from_str(UPDATE_SCHEMA).unwrap();
    let update_validator = jsonschema::draft7::new(&update_schema).expect("a valid schema");

    for id in ["registry-0", "registry-1", "registry-2", "registry-3"] {
        assert!(registry_validator.is_valid(&artifact(id)), "{id}");
    }
    for id in ["update-1", "update-2", "update-3"] {
        assert!(update_validator.is_valid(&artifact(id)), "{id}");
    }

    let encode = |damaged: Value| {
        if damaged
            .get("integrityDigest")
            .and_then(Value::as_str)
            .is_some_and(|digest| digest.starts_with("sha256:"))
        {
            reseal(damaged)
        } else {
            bytes(&damaged)
        }
    };
    for (label, damage) in registry_cases {
        let mut damaged = artifact("registry-1");
        damage(&mut damaged);
        assert!(
            !registry_validator.is_valid(&damaged),
            "the schema admitted {label}"
        );
        assert!(
            admit_registry(&encode(damaged), &limits()).is_err(),
            "the reader admitted {label}"
        );
    }
    for (label, damage) in update_cases {
        let mut damaged = artifact("update-2");
        damage(&mut damaged);
        assert!(
            !update_validator.is_valid(&damaged),
            "the schema admitted {label}"
        );
        assert!(
            admit_update(&encode(damaged), &limits()).is_err(),
            "the reader admitted {label}"
        );
    }
}

#[test]
fn every_bound_admits_its_exact_value_and_refuses_one_less() {
    // Counts are taken from the JSON, not from the reader under test.
    let entries = artifact("registry-3")["body"]["entries"]
        .as_array()
        .unwrap()
        .len() as u64;
    let exact = IdentityLimits {
        entries,
        ..limits()
    };
    assert!(admit_registry(&bytes(&artifact("registry-3")), &exact).is_ok());
    assert_eq!(
        admit_registry(
            &bytes(&artifact("registry-3")),
            &IdentityLimits {
                entries: entries - 1,
                ..limits()
            }
        ),
        Err(RegistryAdmissionFailure::Limit(IdentityLimitKind::Entries))
    );

    for id in ["update-2", "update-3"] {
        let value = artifact(id);
        let body = &value["body"];
        let decisions = body["decisions"].as_array().unwrap();
        let links = body["lineageLinks"].as_array().unwrap();
        let edits: Vec<&Value> = decisions
            .iter()
            .filter_map(|decision| decision["basis"]["changes"].as_array())
            .flatten()
            .collect();
        let replacements: Vec<&Value> = edits
            .iter()
            .flat_map(|edit| edit["replacements"].as_array().unwrap())
            .collect();
        let counts = [
            (decisions.len(), IdentityLimitKind::Decisions),
            (links.len(), IdentityLimitKind::LineageLinks),
            (
                links
                    .iter()
                    .map(|link| {
                        link["predecessors"].as_array().unwrap().len()
                            + link["successors"].as_array().unwrap().len()
                    })
                    .sum(),
                IdentityLimitKind::LineageMembers,
            ),
            (edits.len(), IdentityLimitKind::SourceEdits),
            (replacements.len(), IdentityLimitKind::Replacements),
            (
                replacements
                    .iter()
                    .map(|replacement| replacement["text"].as_str().unwrap().len())
                    .sum(),
                IdentityLimitKind::ReplacementBytes,
            ),
        ];
        for (count, kind) in counts {
            let bound = |value: u64| {
                let mut limits = limits();
                *match kind {
                    IdentityLimitKind::Decisions => &mut limits.decisions,
                    IdentityLimitKind::LineageLinks => &mut limits.lineage_links,
                    IdentityLimitKind::LineageMembers => &mut limits.lineage_members,
                    IdentityLimitKind::SourceEdits => &mut limits.source_edits,
                    IdentityLimitKind::Replacements => &mut limits.replacements,
                    IdentityLimitKind::ReplacementBytes => &mut limits.replacement_bytes,
                    IdentityLimitKind::Entries | IdentityLimitKind::HistorySteps => {
                        unreachable!("not an update bound")
                    }
                } = value;
                limits
            };
            let count = count as u64;
            assert!(
                admit_update(&bytes(&value), &bound(count)).is_ok(),
                "{id}: {kind:?} at its exact bound"
            );
            if count > 0 {
                assert_eq!(
                    admit_update(&bytes(&value), &bound(count - 1)),
                    Err(RegistryAdmissionFailure::Limit(kind)),
                    "{id}: {kind:?} one past its bound"
                );
            }
        }
    }
}

#[test]
fn one_reference_naming_two_contents_is_a_conflict_not_a_duplicate() {
    let read = |value: Value| -> RegistryArtifact { serde_json::from_value(value).unwrap() };
    let original = read(artifact("registry-1"));
    let mut altered = artifact("registry-1");
    altered["body"]["entries"][0]["state"] = json!("retired");
    let altered = read(altered);
    assert_eq!(original.relation(&original.clone()), ArtifactRelation::Same);
    assert_eq!(original.relation(&altered), ArtifactRelation::Conflict);
    assert_eq!(
        original.relation(&read(artifact("registry-2"))),
        ArtifactRelation::Distinct
    );

    let read = |value: Value| -> RegistryUpdateArtifact { serde_json::from_value(value).unwrap() };
    let original = read(artifact("update-3"));
    let mut altered = artifact("update-3");
    altered["body"]["decisions"][1]["basis"]["reason"] = json!("A different explanation.");
    assert_eq!(
        original.relation(&read(altered)),
        ArtifactRelation::Conflict
    );
}

fn snapshot(unit: &str, byte_length: u64) -> SourceSnapshot {
    SourceSnapshot::new(
        owner(),
        unit,
        "1",
        VersionedIdentity::literal("intlify-grammar-js-module", "0"),
        byte_length,
        &format!("sha256:{}", "0".repeat(64)),
    )
    .unwrap()
}

fn range(start: u64, end: u64) -> ByteRange {
    ByteRange::new(start, end).unwrap()
}

fn id(value: &str) -> MessageIntentId {
    MessageIntentId::retained(owner(), value).unwrap()
}

#[test]
fn a_successor_pairs_its_base_only_with_an_update_planned_against_it() {
    // The independent checker refuses a snapshot whose update was planned
    // against another base, so the builder must not be able to write one.
    let registry = |id: &str| -> RegistryArtifact { serde_json::from_value(artifact(id)).unwrap() };
    let update =
        |bytes: &[u8]| -> RegistryUpdateArtifact { serde_json::from_slice(bytes).unwrap() };
    let genesis = registry("registry-0");
    let allocated = registry("registry-1");
    let allocate = update(&bytes(&artifact("update-1")));
    let entries = allocated.body().entries().to_vec();

    assert_eq!(
        IntentRegistrySnapshot::successor(&allocated, &allocate, entries.clone()),
        Err(SnapshotFailure::UpdateBaseMismatch),
        "update-1 was planned against the genesis, not against registry-1"
    );
    let mut foreign = artifact("update-1");
    foreign["body"]["owner"]["kind"] = json!("library");
    assert_eq!(
        IntentRegistrySnapshot::successor(&genesis, &update(&reseal(foreign)), entries.clone()),
        Err(SnapshotFailure::ForeignOwner),
        "an update of another owner is not a link of this chain"
    );
    // The right pairing gives back the committed snapshot exactly.
    assert_eq!(
        IntentRegistrySnapshot::successor(&genesis, &allocate, entries),
        Ok(allocated.body().clone())
    );
}

#[test]
fn an_edit_is_recorded_in_order_with_insertions_at_one_position_joined() {
    let edit = SourceEdit::new(
        Some(snapshot("checkout", 10)),
        Some(snapshot("checkout", 12)),
        vec![
            Replacement::new(range(3, 3), "B"),
            Replacement::new(range(3, 5), "y"),
            Replacement::new(range(0, 2), "x"),
            Replacement::new(range(3, 3), "C"),
        ],
    );
    let recorded: Vec<(u64, u64, &str)> = edit
        .replacements()
        .iter()
        .map(|replacement| {
            (
                replacement.range().start(),
                replacement.range().end(),
                replacement.text(),
            )
        })
        .collect();
    // The insertions keep the order they were given in, and come before the
    // replacement that starts where they are.
    assert_eq!(recorded, [(0, 2, "x"), (3, 3, "BC"), (3, 5, "y")]);
}

#[test]
fn the_builders_record_canonical_order_and_report_repeats() {
    let base = RegistryArtifact::seal(
        IntentRegistrySnapshot::genesis(
            owner(),
            "storefront-web",
            intlify_authoring_identity::RegistryIdentity::retained(&"1".repeat(32)).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let inventory: intlify_authoring::InventoryArtifact =
        serde_json::from_value(document()["artifacts"][0]["artifact"].clone()).unwrap();
    let declaration = |start: u64| {
        Occurrence::new(
            snapshot("checkout", 40),
            range(start, start + 3),
            OccurrenceRole::IntentLiteral,
        )
        .unwrap()
    };
    let (low, high) = (id(&"a".repeat(32)), id(&"b".repeat(32)));
    let decisions = vec![
        IdentityDecision::allocation(
            high.clone(),
            declaration(10),
            AllocationBasis::confirmed_new(),
        ),
        IdentityDecision::allocation(
            low.clone(),
            declaration(0),
            AllocationBasis::confirmed_new(),
        ),
    ];
    let update = IntentRegistryUpdate::new(
        owner(),
        base.reference(),
        inventory.reference(),
        decisions.clone(),
        vec![LineageLink::new(
            LineageKind::Copy,
            vec![low.clone()],
            vec![high.clone()],
        )],
    )
    .unwrap();
    let order: Vec<&MessageIntentId> = update
        .decisions()
        .iter()
        .map(IdentityDecision::intent_id)
        .collect();
    assert_eq!(order, [&low, &high]);

    let mut repeated = decisions;
    repeated.push(IdentityDecision::allocation(
        low.clone(),
        declaration(20),
        AllocationBasis::confirmed_new(),
    ));
    assert_eq!(
        IntentRegistryUpdate::new(
            owner(),
            base.reference(),
            inventory.reference(),
            repeated,
            vec![]
        )
        .unwrap_err(),
        UpdateFailure::DuplicateDecision,
        "a repeated decision is reported, never dropped"
    );

    let update = RegistryUpdateArtifact::seal(update).unwrap();
    let successor = IntentRegistrySnapshot::successor(
        &base,
        &update,
        vec![
            RegistryEntry::new(high.clone(), EntryState::Active, declaration(10)),
            RegistryEntry::new(low.clone(), EntryState::Active, declaration(0)),
        ],
    )
    .unwrap();
    assert_eq!(successor.owner(), base.body().owner());
    assert_eq!(successor.scope(), base.body().scope());
    assert_eq!(
        successor.registry_identity(),
        base.body().registry_identity()
    );
    assert_eq!(successor.base(), Some(&base.reference()));
    assert_eq!(successor.update(), Some(&update.reference()));
    assert_eq!(
        successor.entry(&low).map(RegistryEntry::state),
        Some(EntryState::Active)
    );
    let order: Vec<&MessageIntentId> = successor
        .entries()
        .iter()
        .map(RegistryEntry::intent_id)
        .collect();
    assert_eq!(order, [&low, &high]);
    assert!(
        matches!(
            ContinuationBasis::unchanged_snapshot(),
            ContinuationBasis::UnchangedSnapshot(_)
        ),
        "the unit-like bases are built without a payload"
    );
}
