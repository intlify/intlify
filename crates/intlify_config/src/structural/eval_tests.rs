// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::sync::Arc;

use serde_json::{json, Value};

use crate::fixtures::{complete_config, minimal_config, FixtureConfig};
use crate::input_limits::Bound;
use crate::materialize::{materialize_file, MaterializedDocument, NodeId, NodeKind};
use crate::materialize_tests::limits;

use super::eval::{
    evaluate, EvaluationFailure, FragmentState, IssueKind, LocationRole, SchemaEvaluation,
};
use super::facts::tests::names;
use super::facts::{At, Check, Facts, Level, Outcome};
use super::program::SchemaProgram;

fn program() -> SchemaProgram {
    SchemaProgram::compile(&crate::schema::draft7_schema::<FixtureConfig>().unwrap()).unwrap()
}
fn materialize(value: &Value) -> MaterializedDocument {
    materialize_file(Arc::from(serde_json::to_vec(value).unwrap()), limits()).unwrap()
}
fn run(program: &SchemaProgram, doc: &MaterializedDocument) -> SchemaEvaluation {
    evaluate(
        program,
        doc,
        program.root(),
        doc.root(),
        Bound::new(1_000_000).unwrap(),
    )
    .unwrap()
}
fn member(doc: &MaterializedDocument, owner: NodeId, name: &str) -> NodeId {
    let NodeKind::Object(members) = doc.node(owner).kind() else {
        panic!()
    };
    members[name].value()
}

#[test]
fn all_applicable_owned_schema_branches_have_the_oracles_result() {
    let schema = crate::schema::draft7_schema::<FixtureConfig>().unwrap();
    let oracle = jsonschema::draft7::new(&schema).unwrap();
    let program = SchemaProgram::compile(&schema).unwrap();
    for value in [minimal_config(), complete_config()] {
        let doc = materialize(&value);
        let result = run(&program, &doc);
        assert_eq!(result.admitted, oracle.is_valid(&value));
        assert!(result.admitted);
        assert!(result.issues.is_empty());
        assert!(result.units > 0);
    }
}

#[test]
fn bad_sibling_does_not_erase_a_fully_admitted_declaration() {
    let mut value = complete_config();
    value["profiles"]["bad"] = value["profiles"]["app"].clone();
    value["profiles"]["bad"]["projectId"] = json!("UPPERCASE");
    value["profiles"]["bad"]["coverage"] = json!(false);
    let doc = materialize(&value);
    let program = program();
    let result = run(&program, &doc);
    assert!(!result.admitted);
    let profiles = member(&doc, doc.root(), "profiles");
    let good = member(&doc, profiles, "app");
    let bad = member(&doc, profiles, "bad");
    let declaration = program.location("/definitions/ProfileDeclaration").unwrap();
    assert!(result.fragments.iter().any(|f| f.subject == good
        && f.schema == declaration
        && f.state == FragmentState::Admitted));
    assert!(result
        .fragments
        .iter()
        .any(|f| f.subject == bad && f.schema == declaration && f.state == FragmentState::Invalid));
    let bad_coverage = member(&doc, bad, "coverage");
    assert!(result
        .fragments
        .iter()
        .any(|f| f.subject == bad_coverage && f.state == FragmentState::TypeUnavailable));
    let valid_default = member(&doc, bad, "defaultRequestedLocale");
    assert!(result
        .fragments
        .iter()
        .any(|f| f.subject == valid_default && f.state == FragmentState::Admitted));
    assert_eq!(result.issues.len(), 2);
    assert!(result
        .issues
        .iter()
        .any(|i| i.subject == bad_coverage && i.kind == IssueKind::TypeInvalid));
}

#[test]
fn missing_fields_have_known_schema_names_and_owning_object_spans() {
    let mut value = minimal_config();
    value["profiles"]["app"]["policies"]
        .as_object_mut()
        .unwrap()
        .remove("approval");
    value["profiles"]["app"]
        .as_object_mut()
        .unwrap()
        .remove("requestedLocales");
    let doc = materialize(&value);
    let result = run(&program(), &doc);
    assert!(!result.admitted);
    let missing: Vec<_> = result
        .issues
        .iter()
        .filter(|i| i.kind == IssueKind::RequiredFieldMissing)
        .collect();
    assert_eq!(missing.len(), 2);
    assert!(missing
        .iter()
        .any(|i| i.missing_field.as_deref() == Some("approval")));
    assert!(missing
        .iter()
        .any(|i| i.missing_field.as_deref() == Some("requestedLocales")));
    for issue in missing {
        assert_eq!(issue.role, LocationRole::MissingField);
        assert_eq!(issue.span, doc.node(issue.subject).span());
    }
}

#[test]
fn work_limit_uses_complete_exact_total_and_returns_no_evaluation_prefix() {
    let doc = materialize(&complete_config());
    let program = program();
    let expected = run(&program, &doc).units;
    assert!(evaluate(
        &program,
        &doc,
        program.root(),
        doc.root(),
        Bound::new(expected).unwrap()
    )
    .is_ok());
    for limit in [expected - 1, 1] {
        let error = evaluate(
            &program,
            &doc,
            program.root(),
            doc.root(),
            Bound::new(limit).unwrap(),
        )
        .err()
        .unwrap();
        assert_eq!(
            error,
            EvaluationFailure::WorkLimit {
                limit: Bound::new(limit).unwrap(),
                actual: expected
            }
        );
    }
}

#[test]
fn type_failure_suppresses_only_inapplicable_descendant_work() {
    let root = materialize(&json!([]));
    let program = program();
    let result = run(&program, &root);
    assert!(!result.admitted);
    assert_eq!(result.units, 1);
    assert_eq!(result.issues.len(), 1);
    assert_eq!(result.issues[0].kind, IssueKind::TypeInvalid);
    assert_eq!(result.fragments.len(), 1);
    assert_eq!(result.fragments[0].state, FragmentState::TypeUnavailable);
}

#[test]
fn unsafe_unknown_or_invalid_dynamic_keys_never_enter_issue_text() {
    let mut value = minimal_config();
    value["private-token"] = json!("private-value");
    value["profiles"]["PRIVATE-PROFILE"] = json!({"private-nested":"secret"});
    let doc = materialize(&value);
    let result = run(&program(), &doc);
    assert!(!result.admitted);
    // The root's undeclared member and the invalid key, and under the key's
    // entry, its undeclared member and the seven required members its value
    // lacks (D15:2184). A key is never named, and a missing member only by
    // the schema's own name.
    assert_eq!(result.issues.len(), 10);
    for issue in &result.issues {
        match issue.role {
            LocationRole::MemberKey => assert!(issue.missing_field.is_none()),
            LocationRole::MissingField => assert!(matches!(
                issue.missing_field.as_deref(),
                Some(
                    "projectId"
                        | "selectionScope"
                        | "requestedLocales"
                        | "defaultRequestedLocale"
                        | "policies"
                        | "targetProfiles"
                        | "deploymentGroups"
                )
            )),
            LocationRole::Value => panic!("{issue:?}"),
        }
    }
    assert_eq!(
        result
            .issues
            .iter()
            .filter(|issue| issue.role == LocationRole::MemberKey)
            .count(),
        3
    );
    let rendered = format!("{:?} {:?}", result.issues, result.facts);
    for excluded in [
        "private-token",
        "private-value",
        "PRIVATE-PROFILE",
        "private-nested",
        "secret",
    ] {
        assert!(!rendered.contains(excluded));
    }
}

#[test]
fn reused_immutable_program_has_no_previous_failure_or_counter_state() {
    let program = program();
    let good = materialize(&complete_config());
    let expected = run(&program, &good);
    let bad = materialize(&json!({"schemaVersion":"0", "profiles":false}));
    assert!(!run(&program, &bad).admitted);
    let repeated = run(&program, &good);
    assert_eq!(expected.units, repeated.units);
    assert_eq!(expected.issues, repeated.issues);
    assert_eq!(expected.fragments, repeated.fragments);
}

#[test]
fn object_member_order_does_not_change_logical_work() {
    let value = complete_config();
    let mut reordered = serde_json::Map::new();
    for (key, value) in value.as_object().unwrap().iter().rev() {
        reordered.insert(key.clone(), value.clone());
    }
    let program = program();
    let original = run(&program, &materialize(&value));
    let reversed = run(&program, &materialize(&Value::Object(reordered)));
    assert!(original.admitted && reversed.admitted);
    assert_eq!(original.units, reversed.units);
    assert_eq!(shapes(&original), shapes(&reversed));
}

/// The `app` declaration's member `name` as materialized from `doc`.
fn declaration_member(doc: &MaterializedDocument, name: &str) -> NodeId {
    let profiles = member(doc, doc.root(), "profiles");
    member(doc, member(doc, profiles, "app"), name)
}

#[test]
fn delivery_admits_duplicate_placement_alone_and_no_override_graph_or_output() {
    let program = program();
    for delivery in [json!({}), json!({"placement": "duplicate"})] {
        let mut value = complete_config();
        value["profiles"]["app"]["delivery"] = delivery;
        assert!(run(&program, &materialize(&value)).admitted);
    }
    // Another mode is an invalid value at `placement`, never `duplicate`.
    for mode in ["hoist", "Duplicate", "unknown"] {
        let mut value = complete_config();
        value["profiles"]["app"]["delivery"]["placement"] = json!(mode);
        let doc = materialize(&value);
        let placement = member(&doc, declaration_member(&doc, "delivery"), "placement");
        let result = run(&program, &doc);
        assert_eq!(
            result
                .issues
                .iter()
                .map(|issue| (issue.kind, issue.subject, issue.role))
                .collect::<Vec<_>>(),
            [(IssueKind::ValueInvalid, placement, LocationRole::Value)],
            "{mode}"
        );
    }
    // An override, a graph member, or a physical output is no delivery member.
    for (name, extra) in [
        ("targetPlacement", json!({"browser": "duplicate"})),
        ("nodes", json!([["main"]])),
        ("edges", json!([])),
        ("outputDirectory", json!("dist")),
    ] {
        let mut value = complete_config();
        value["profiles"]["app"]["delivery"][name] = extra;
        let doc = materialize(&value);
        let delivery = declaration_member(&doc, "delivery");
        let result = run(&program, &doc);
        assert_eq!(
            result
                .issues
                .iter()
                .map(|issue| (issue.kind, issue.subject, issue.role))
                .collect::<Vec<_>>(),
            [(IssueKind::UnknownField, delivery, LocationRole::MemberKey)],
            "{name}"
        );
    }
}

#[test]
fn an_explicitly_empty_fallback_sequence_is_an_invalid_value_where_it_is_declared() {
    let mut value = complete_config();
    value["profiles"]["app"]["messageFallback"]["ja"] = json!([]);
    let doc = materialize(&value);
    let sequence = member(&doc, declaration_member(&doc, "messageFallback"), "ja");
    let result = run(&program(), &doc);
    assert_eq!(
        result
            .issues
            .iter()
            .map(|issue| (issue.kind, issue.subject, issue.role))
            .collect::<Vec<_>>(),
        [(IssueKind::ValueInvalid, sequence, LocationRole::Value)]
    );
}

/// What each candidate of `result` checked: its check, its scope's level, its
/// path from the scope's entry as text, and its outcome.
fn shapes(result: &SchemaEvaluation) -> Vec<(Check, Level, String, Outcome)> {
    let facts = &result.facts;
    facts
        .candidates()
        .iter()
        .map(|candidate| {
            (
                candidate.check,
                facts.scope(candidate.scope).level,
                names(facts, candidate.path).join("/"),
                candidate.outcome,
            )
        })
        .collect()
}

/// The checks and outcomes of `result`'s candidates at `path` under an entry
/// of `level`, in the order they were recorded.
fn at(result: &SchemaEvaluation, level: Level, path: &str) -> Vec<(Check, Outcome)> {
    shapes(result)
        .into_iter()
        .filter(|shape| shape.1 == level && shape.2 == path)
        .map(|shape| (shape.0, shape.3))
        .collect()
}

/// Where the key of `owner`'s member `name` is.
fn key_span(doc: &MaterializedDocument, owner: NodeId, name: &str) -> crate::materialize::ByteSpan {
    let NodeKind::Object(members) = doc.node(owner).kind() else {
        panic!()
    };
    members[name].key_span()
}

fn compiled(schema: &Value) -> SchemaProgram {
    SchemaProgram::compile(schema).unwrap()
}

#[test]
fn a_complete_configuration_records_each_check_where_it_applies() {
    use Check::{Key, Known, Required, Type, Value};
    use Level::{Group, Profile, Root, Target};
    let doc = materialize(&minimal_config());
    let result = run(&program(), &doc);
    assert!(result.admitted);
    let shapes = shapes(&result);
    assert!(shapes.iter().all(|shape| shape.3 == Outcome::Passed));
    for (check, level, path) in [
        (Type, Root, ""),
        (Required, Root, "schemaVersion"),
        (Required, Root, "profiles"),
        (Known, Root, ""),
        (Type, Root, "profiles"),
        (Value, Root, "profiles"),
        (Key, Root, "profiles/*"),
        // An identity map entry's value is the empty path under the entry.
        (Type, Profile, ""),
        (Known, Profile, ""),
        (Required, Profile, "projectId"),
        (Type, Profile, "requestedLocales"),
        (Value, Profile, "requestedLocales"),
        (Type, Profile, "requestedLocales/*"),
        (Key, Profile, "targetProfiles/*"),
        (Key, Profile, "deploymentGroups/*"),
        (Type, Target, ""),
        (Type, Target, "requestedLocales/*"),
        (Type, Group, ""),
        (Type, Group, "members/*"),
    ] {
        let shape = (check, level, path.to_owned(), Outcome::Passed);
        assert!(shapes.contains(&shape), "{shape:?}");
    }
    // An identity map's closedness is its keys' check.
    for (level, path) in [
        (Root, "profiles"),
        (Profile, "targetProfiles"),
        (Profile, "deploymentGroups"),
    ] {
        assert!(!at(&result, level, path)
            .iter()
            .any(|shape| shape.0 == Known));
    }
    // Each key scopes its entry, under the scope its map is in.
    let facts = &result.facts;
    let profiles = member(&doc, doc.root(), "profiles");
    let app = member(&doc, profiles, "app");
    let keys: Vec<_> = facts
        .candidates()
        .iter()
        .filter(|candidate| candidate.check == Key)
        .collect();
    assert_eq!(keys.len(), 3);
    for key in keys {
        let At::Key(scope) = key.at else {
            panic!("{key:?}")
        };
        let scope = facts.scope(scope);
        let entry = scope.entry.unwrap();
        assert!(entry.admitted);
        assert_eq!(scope.parent, Some(key.scope));
        let (map, name) = match scope.level {
            Profile => {
                assert_eq!(key.scope, Facts::ROOT);
                (profiles, "app")
            }
            Target => (declaration_member(&doc, "targetProfiles"), "browser"),
            Group => (declaration_member(&doc, "deploymentGroups"), "web"),
            Root => panic!("{key:?}"),
        };
        if scope.level != Profile {
            assert_eq!(facts.scope(key.scope).entry.unwrap().value, app);
        }
        assert_eq!(entry.map, map);
        assert_eq!(entry.value, member(&doc, map, name));
        assert_eq!(entry.key, key_span(&doc, map, name));
    }
}

#[test]
fn a_missing_member_fails_its_required_check_at_its_own_path() {
    let mut value = minimal_config();
    value["profiles"]["app"]
        .as_object_mut()
        .unwrap()
        .remove("requestedLocales");
    let doc = materialize(&value);
    let result = run(&program(), &doc);
    let facts = &result.facts;
    let app = member(&doc, member(&doc, doc.root(), "profiles"), "app");
    let failed: Vec<_> = facts
        .candidates()
        .iter()
        .filter(|candidate| candidate.outcome != Outcome::Passed)
        .collect();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].check, Check::Required);
    assert_eq!(failed[0].outcome, Outcome::Failed);
    assert_eq!(failed[0].at, At::Missing { object: app });
    assert_eq!(names(facts, failed[0].path), ["requestedLocales"]);
    assert_eq!(facts.scope(failed[0].scope).level, Level::Profile);
    // A present member's candidate is at its value.
    let project = member(&doc, app, "projectId");
    assert!(facts
        .candidates()
        .iter()
        .any(|candidate| candidate.check == Check::Required
            && candidate.at == At::Value(project)
            && names(facts, candidate.path) == ["projectId"]));
}

#[test]
fn each_undeclared_member_fails_the_known_check_of_its_object() {
    let mut value = minimal_config();
    value["extra-a"] = json!(1);
    value["extra-b"] = json!(2);
    let doc = materialize(&value);
    let result = run(&program(), &doc);
    let known: Vec<_> = result
        .facts
        .candidates()
        .iter()
        .filter(|candidate| {
            candidate.check == Check::Known
                && candidate.scope == Facts::ROOT
                && candidate.path.is_none()
        })
        .collect();
    assert_eq!(known.len(), 2, "one per member, and none that passes");
    for (candidate, name) in known.iter().zip(["extra-a", "extra-b"]) {
        assert_eq!(candidate.outcome, Outcome::Failed);
        assert_eq!(
            candidate.at,
            At::Member {
                object: doc.root(),
                key: key_span(&doc, doc.root(), name)
            }
        );
    }
    assert_eq!(
        at(&result, Level::Profile, ""),
        [
            (Check::Type, Outcome::Passed),
            (Check::Known, Outcome::Passed)
        ]
    );
}

#[test]
fn a_type_failure_excludes_its_paths_other_checks_and_records_nothing_below() {
    let mut value = minimal_config();
    value["profiles"]["app"]["requestedLocales"] = json!("en");
    value["profiles"]["app"]["policies"] = json!(7);
    let doc = materialize(&value);
    let result = run(&program(), &doc);
    assert_eq!(
        at(&result, Level::Profile, "requestedLocales"),
        [
            (Check::Required, Outcome::Passed),
            (Check::Type, Outcome::Failed),
            (Check::Value, Outcome::Excluded)
        ]
    );
    assert_eq!(
        at(&result, Level::Profile, "policies"),
        [
            (Check::Required, Outcome::Passed),
            (Check::Type, Outcome::Failed),
            (Check::Known, Outcome::Excluded)
        ]
    );
    assert!(
        !shapes(&result).iter().any(|shape| shape.1 == Level::Profile
            && (shape.2.starts_with("requestedLocales/") || shape.2.starts_with("policies/")))
    );
    let values = [
        At::Value(declaration_member(&doc, "policies")),
        At::Value(declaration_member(&doc, "requestedLocales")),
    ];
    for outcome in [Outcome::Failed, Outcome::Excluded] {
        let at: Vec<_> = result
            .facts
            .candidates()
            .iter()
            .filter(|candidate| candidate.outcome == outcome)
            .map(|candidate| candidate.at)
            .collect();
        assert_eq!(at, values, "{outcome:?}");
    }
}

#[test]
fn an_any_of_is_one_value_check_and_its_branches_record_no_candidate() {
    let mut value = complete_config();
    value["profiles"]["app"]["messageFallback"]["ja"] =
        json!(["en", {"kind": "intent-source-locale"}, 42]);
    value["profiles"]["app"]["coverage"]["rules"] = json!([{"mode": "fallback-allowed"}]);
    let doc = materialize(&value);
    let result = run(&program(), &doc);
    assert_eq!(
        at(&result, Level::Profile, "messageFallback/*/*"),
        [
            (Check::Value, Outcome::Passed),
            (Check::Value, Outcome::Passed),
            (Check::Value, Outcome::Failed)
        ]
    );
    // A coverage rule with neither selector fails its required alternative.
    assert_eq!(
        at(&result, Level::Profile, "coverage/rules/*"),
        [
            (Check::Type, Outcome::Passed),
            (Check::Known, Outcome::Passed),
            (Check::Value, Outcome::Failed)
        ]
    );
    assert_eq!(
        at(&result, Level::Profile, "coverage/rules/*/mode"),
        [
            (Check::Required, Outcome::Passed),
            (Check::Type, Outcome::Passed),
            (Check::Value, Outcome::Passed)
        ]
    );
    let shapes = shapes(&result);
    assert!(!shapes
        .iter()
        .any(|shape| shape.2.starts_with("messageFallback/*/*/")
            || shape.2 == "coverage/rules/*/requestedLocales"
            || shape.2 == "coverage/rules/*/intentSurfaceClasses"));
}

#[test]
fn identity_keys_are_checked_under_their_map_and_scope_their_entries() {
    let mut value = minimal_config();
    value["profiles"]["Bad Key"] = value["profiles"]["app"].clone();
    let doc = materialize(&value);
    let result = run(&program(), &doc);
    let facts = &result.facts;
    let profiles = member(&doc, doc.root(), "profiles");
    let keys: Vec<_> = facts
        .candidates()
        .iter()
        .filter(|candidate| candidate.check == Check::Key && candidate.scope == Facts::ROOT)
        .collect();
    assert_eq!(keys.len(), 2);
    for (key, (name, admitted)) in keys.iter().zip([("Bad Key", false), ("app", true)]) {
        assert_eq!(names(facts, key.path), ["profiles", "*"]);
        assert_eq!(key.outcome, Outcome::of(admitted));
        let At::Key(scope) = key.at else {
            panic!("{key:?}")
        };
        let entry = facts.scope(scope).entry.unwrap();
        assert_eq!(entry.admitted, admitted);
        assert_eq!(entry.map, profiles);
        assert_eq!(entry.value, member(&doc, profiles, name));
        assert_eq!(entry.key, key_span(&doc, profiles, name));
        assert_eq!(facts.scope(scope).level, Level::Profile);
    }
    // Each entry's value is evaluated under its own scope, the failing key's
    // included, so the two raw parents stay apart (D15:2184, 2190).
    let scopes: Vec<_> = keys
        .iter()
        .map(|key| match key.at {
            At::Key(scope) => scope,
            _ => unreachable!("a key"),
        })
        .collect();
    assert_ne!(scopes[0], scopes[1]);
    let under = |scope| {
        facts
            .candidates()
            .iter()
            .filter(|candidate| candidate.scope == scope)
            .map(|candidate| {
                (
                    candidate.check,
                    names(facts, candidate.path),
                    candidate.outcome,
                )
            })
            .collect::<Vec<_>>()
    };
    assert!(under(scopes[0]).contains(&(Check::Type, Vec::new(), Outcome::Passed)));
    assert_eq!(under(scopes[0]), under(scopes[1]));
}

#[test]
fn an_identity_map_outside_015s_scope_levels_is_a_layout_error() {
    let identities = json!({
        "type": "object",
        "patternProperties": {crate::model::ID_PATTERN: {}},
        "additionalProperties": false
    });
    let draft = "http://json-schema.org/draft-07/schema#";
    for (schema, value) in [
        (
            json!({"$schema": draft, "type": "object", "properties": {"other": identities}}),
            json!({"other": {"a": 1}}),
        ),
        (
            json!({"$schema": draft, "type": "object", "properties": {"profiles": {
                "type": "object", "properties": {"profiles": identities}
            }}}),
            json!({"profiles": {"profiles": {"a": 1}}}),
        ),
    ] {
        let program = compiled(&schema);
        let doc = materialize(&value);
        let failure = evaluate(
            &program,
            &doc,
            program.root(),
            doc.root(),
            Bound::new(1_000).unwrap(),
        )
        .err();
        assert_eq!(failure, Some(EvaluationFailure::ScopeLayout), "{value}");
    }
}

#[test]
fn a_value_fails_its_closed_value_check_when_any_constraint_fails() {
    let program = compiled(&json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "string",
        "enum": ["Upper", "lower"],
        "pattern": crate::model::ID_PATTERN
    }));
    for (value, passed) in [("lower", true), ("Upper", false), ("other", false)] {
        let result = run(&program, &materialize(&json!(value)));
        let checks: Vec<_> = result
            .facts
            .candidates()
            .iter()
            .map(|candidate| (candidate.check, candidate.outcome))
            .collect();
        assert_eq!(
            checks,
            [
                (Check::Type, Outcome::Passed),
                (Check::Value, Outcome::of(passed))
            ],
            "{value}"
        );
    }
}

#[test]
fn each_closed_value_constraint_leaves_a_mistyped_value_to_its_type_check() {
    use Check::{Known, Required, Type, Value};
    use Outcome::{Excluded, Failed, Passed};
    let mut value = complete_config();
    let app = &mut value["profiles"]["app"];
    // A pattern, an enumeration, alternatives of a closed object, and the
    // minimum member count of an identity map.
    app["projectId"] = json!(5);
    app["coverage"]["defaultMode"] = json!(5);
    app["coverage"]["rules"] = json!(["x"]);
    app["targetProfiles"] = json!("x");
    let result = run(&program(), &materialize(&value));
    for (path, expected) in [
        (
            "projectId",
            vec![(Required, Passed), (Type, Failed), (Value, Excluded)],
        ),
        (
            "coverage/defaultMode",
            vec![(Type, Failed), (Value, Excluded)],
        ),
        (
            "coverage/rules/*",
            vec![(Type, Failed), (Value, Excluded), (Known, Excluded)],
        ),
        (
            "targetProfiles",
            vec![(Required, Passed), (Type, Failed), (Value, Excluded)],
        ),
    ] {
        assert_eq!(at(&result, Level::Profile, path), expected, "{path}");
    }
}

#[test]
fn an_identity_map_without_entries_fails_its_minimum_member_count() {
    let mut value = minimal_config();
    value["profiles"]["app"]["targetProfiles"] = json!({});
    let result = run(&program(), &materialize(&value));
    assert_eq!(
        at(&result, Level::Profile, "targetProfiles"),
        [
            (Check::Required, Outcome::Passed),
            (Check::Type, Outcome::Passed),
            (Check::Value, Outcome::Failed)
        ]
    );
    assert!(!shapes(&result).iter().any(|shape| shape.0 == Check::Key
        && shape.1 == Level::Profile
        && shape.2 == "targetProfiles/*"));
}

/// Every applicable evaluation counts even when another check blocks
/// (D15:1887): the value of an entry whose key fails is the same work as the
/// value of an entry whose key passes.
#[test]
fn a_failing_keys_value_counts_the_work_of_any_other_entry() {
    let program = program();
    let units = |name: &str| {
        let mut value = minimal_config();
        value["profiles"][name] = value["profiles"]["app"].clone();
        run(&program, &materialize(&value)).units
    };
    let one = run(&program, &materialize(&minimal_config())).units;
    assert!(units("zz") > one);
    assert_eq!(units("Bad Key"), units("zz"));
}

/// Each candidate needs the type check its evaluation depends on: its
/// containing value's for a type, its value's own, or else the containing
/// value's, for the other checks of its path, the object's for a required
/// member, and the map's for a key (D15:2149, 2184).
#[test]
fn each_candidate_needs_the_type_check_it_depends_on() {
    let mut value = complete_config();
    value["profiles"]["app"]["requestedLocales"] = json!("en");
    let result = run(&program(), &materialize(&value));
    let facts = &result.facts;
    let find = |check: Check, level: Level, path: &str| {
        facts
            .identified()
            .find(|(_, candidate)| {
                candidate.check == check
                    && facts.scope(candidate.scope).level == level
                    && names(facts, candidate.path).join("/") == path
            })
            .map(|(id, candidate)| (id, candidate.after))
            .unwrap()
    };
    let (root, after) = find(Check::Type, Level::Root, "");
    assert_eq!(after, None, "the measure admits the root's type");
    let (profiles, after) = find(Check::Type, Level::Root, "profiles");
    assert_eq!(after, Some(root));
    assert_eq!(find(Check::Required, Level::Root, "profiles").1, Some(root));
    assert_eq!(find(Check::Known, Level::Root, "").1, Some(root));
    assert_eq!(
        find(Check::Value, Level::Root, "profiles").1,
        Some(profiles)
    );
    assert_eq!(
        find(Check::Key, Level::Root, "profiles/*").1,
        Some(profiles)
    );
    let (declaration, after) = find(Check::Type, Level::Profile, "");
    assert_eq!(after, Some(profiles));
    assert_eq!(
        find(Check::Required, Level::Profile, "requestedLocales").1,
        Some(declaration)
    );
    // A value whose schema has no type needs its container's.
    let (sequence, _) = find(Check::Type, Level::Profile, "messageFallback/*");
    assert_eq!(
        find(Check::Value, Level::Profile, "messageFallback/*/*").1,
        Some(sequence)
    );
    // An excluded check needs the type that excluded it.
    let (locales, _) = find(Check::Type, Level::Profile, "requestedLocales");
    let (_, excluded) = find(Check::Value, Level::Profile, "requestedLocales");
    assert_eq!(excluded, Some(locales));
    // Every candidate comes after the one it needs.
    assert!(facts
        .identified()
        .all(|(id, candidate)| candidate.after.is_none_or(|after| after < id)));
}
