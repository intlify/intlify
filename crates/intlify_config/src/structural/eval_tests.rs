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
    assert_eq!(result.issues.len(), 2);
    assert!(result
        .issues
        .iter()
        .all(|i| i.role == LocationRole::MemberKey && i.missing_field.is_none()));
    let rendered = format!("{:?}", result.issues);
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
}
