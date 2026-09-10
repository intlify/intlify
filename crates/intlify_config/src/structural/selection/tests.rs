// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use serde_json::json;

use crate::fixtures::{minimal_config, FixturePolicyReference};
use crate::input_limits::Bound;
use crate::structural::admission_tests::{analyze_fixture, analyze_with_limits, limits};

use super::{
    InvalidSelectorType, ProvisionalSelection, Selection, SelectionFailure, SelectionInvariant,
    SelectionPrerequisite, SelectorInput,
};

fn selected(
    result: Selection<FixturePolicyReference>,
) -> ProvisionalSelection<FixturePolicyReference> {
    match result {
        Selection::Selected(value) => value,
        Selection::Rejected(reason) => panic!("unexpected selection failure: {reason:?}"),
        Selection::Unavailable(reason) => panic!("unexpected missing prerequisite: {reason:?}"),
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "assertion helpers consume completed selections, including their owned success branch"
)]
fn failure(result: Selection<FixturePolicyReference>) -> SelectionFailure {
    match result {
        Selection::Rejected(reason) => reason,
        Selection::Unavailable(reason) => panic!("unexpected missing prerequisite: {reason:?}"),
        Selection::Selected(_) => panic!("unexpected selection"),
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "assertion helpers consume completed selections, including their owned success branch"
)]
fn unavailable(result: Selection<FixturePolicyReference>) -> SelectionPrerequisite {
    match result {
        Selection::Unavailable(reason) => reason,
        Selection::Rejected(reason) => panic!("unexpected selection failure: {reason:?}"),
        Selection::Selected(_) => panic!("unexpected selection"),
    }
}

#[test]
fn omission_and_explicit_single_selection_agree_and_own_their_outputs() {
    let bound = limits().max_profile_id_bytes;
    let analysis = analyze_fixture(&minimal_config());
    let omitted = selected(analysis.select(&SelectorInput::absent(bound)).unwrap());
    let explicit = selected(
        analysis
            .select(&SelectorInput::string("app", bound))
            .unwrap(),
    );
    assert_eq!(omitted.id(), explicit.id());
    assert_eq!(
        omitted.declaration_key_span(),
        explicit.declaration_key_span()
    );
    drop(analysis);
    assert_eq!(omitted.id().as_str(), "app");
    assert_eq!(omitted.resource_limits(), explicit.resource_limits());
    assert_eq!(
        serde_json::to_value(omitted.resource_limits()).unwrap(),
        json!({"$testPolicy":"resource-limits"})
    );
}

#[test]
fn several_profiles_require_exact_selection_without_merging_or_order_defaults() {
    let bound = limits().max_profile_id_bytes;
    let mut value = minimal_config();
    value["profiles"]["second"] = value["profiles"]["app"].clone();
    value["profiles"]["second"]["projectId"] = json!("another-project");
    for reverse in [false, true] {
        if reverse {
            let original = value["profiles"].as_object().unwrap();
            value["profiles"] = serde_json::Value::Object(
                original
                    .iter()
                    .rev()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            );
        }
        let analysis = analyze_fixture(&value);
        assert_eq!(
            failure(analysis.select(&SelectorInput::absent(bound)).unwrap()),
            SelectionFailure::Required
        );
        for name in ["app", "second"] {
            assert_eq!(
                selected(
                    analysis
                        .select(&SelectorInput::string(name, bound))
                        .unwrap()
                )
                .id()
                .as_str(),
                name
            );
        }
        assert_eq!(analysis.construct().unwrap().unwrap().profiles().len(), 2);
    }
}

#[test]
fn omission_counts_invalid_and_over_limit_declarations_in_the_original_map() {
    let mut capacity = limits();
    capacity.max_profile_id_bytes = Bound::new(3).unwrap();
    let bound = capacity.max_profile_id_bytes;
    for key in ["BAD", "private-long-id"] {
        let mut value = minimal_config();
        value["profiles"][key] = json!(false);
        let analysis = analyze_with_limits(&value, capacity);
        assert_eq!(
            failure(analysis.select(&SelectorInput::absent(bound)).unwrap()),
            SelectionFailure::Required
        );
        assert_eq!(
            selected(
                analysis
                    .select(&SelectorInput::string("app", bound))
                    .unwrap()
            )
            .id()
            .as_str(),
            "app"
        );
        assert!(analysis.construct().unwrap().is_none());
    }
}

#[test]
fn invalid_and_unknown_strings_are_distinct_without_disclosing_their_contents() {
    let analysis = analyze_fixture(&minimal_config());
    let bound = limits().max_profile_id_bytes;
    for text in ["", "App", " app", "app/child", "a.", "日本", "a\u{0}b"] {
        assert_eq!(
            failure(
                analysis
                    .select(&SelectorInput::string(text, bound))
                    .unwrap()
            ),
            SelectionFailure::InvalidSyntax {
                bytes: u64::try_from(text.len()).unwrap()
            }
        );
    }
    let secret = "private-customer-token";
    let reason = failure(
        analysis
            .select(&SelectorInput::string(secret, bound))
            .unwrap(),
    );
    assert_eq!(reason, SelectionFailure::Unknown { bytes: 22 });
    assert!(!format!("{reason:?}").contains(secret));
}

#[test]
fn invalid_non_string_selectors_hold_only_top_level_type_tags() {
    let analysis = analyze_fixture(&minimal_config());
    let bound = limits().max_profile_id_bytes;
    for kind in [
        InvalidSelectorType::Null,
        InvalidSelectorType::Boolean,
        InvalidSelectorType::Number,
        InvalidSelectorType::Array,
        InvalidSelectorType::Object,
    ] {
        assert_eq!(
            failure(
                analysis
                    .select(&SelectorInput::invalid_type(kind, bound))
                    .unwrap()
            ),
            SelectionFailure::InvalidType(kind)
        );
    }
}

#[test]
fn byte_limits_precede_syntax_and_keep_only_smallest_first_over_witness() {
    let mut capacity = limits();
    capacity.max_profile_id_bytes = Bound::new(3).unwrap();
    let bound = capacity.max_profile_id_bytes;
    let analysis = analyze_with_limits(&minimal_config(), capacity);
    assert_eq!(
        selected(
            analysis
                .select(&SelectorInput::string("app", bound))
                .unwrap()
        )
        .id()
        .as_str(),
        "app"
    );
    for text in ["apps", "日本", &"PRIVATE".repeat(1000)] {
        let input = SelectorInput::string(text, bound);
        assert!(matches!(input.kind, super::SelectorKind::OverLimitString));
        assert_eq!(
            failure(analysis.select(&input).unwrap()),
            SelectionFailure::OverLimit {
                limit: bound,
                first_over: 4
            }
        );
    }
    assert_eq!(
        failure(
            analysis
                .select(&SelectorInput::string("日", bound))
                .unwrap()
        ),
        SelectionFailure::InvalidSyntax { bytes: 3 }
    );
}

#[test]
fn normalized_input_cannot_silently_cross_bootstrap_contexts() {
    let analysis = analyze_fixture(&minimal_config());
    let different = Bound::new(3).unwrap();
    for input in [
        SelectorInput::absent(different),
        SelectorInput::string("private-long-selector", different),
        SelectorInput::invalid_type(InvalidSelectorType::Object, different),
    ] {
        assert_eq!(
            analysis.select(&input).err(),
            Some(SelectionInvariant::BootstrapBoundMismatch)
        );
    }
}

#[test]
fn matching_requires_version_profiles_shape_and_complete_structural_work() {
    let bound = limits().max_profile_id_bytes;
    let name = SelectorInput::string("app", bound);
    let missing_version = analyze_fixture(&json!({"profiles":{}}));
    assert_eq!(
        unavailable(missing_version.select(&name).unwrap()),
        SelectionPrerequisite::ConfigurationVersion
    );
    for profiles in [json!(null), json!(false), json!([]), json!({})] {
        let analysis = analyze_fixture(&json!({"schemaVersion":"0", "profiles":profiles}));
        assert_eq!(
            unavailable(analysis.select(&name).unwrap()),
            SelectionPrerequisite::ProfilesShape
        );
    }
    let mut capacity = limits();
    capacity.max_structural_analysis_units = Bound::new(1).unwrap();
    assert_eq!(
        unavailable(
            analyze_with_limits(&minimal_config(), capacity)
                .select(&name)
                .unwrap()
        ),
        SelectionPrerequisite::StructuralWork
    );
    // Selector type admission needs no schema-owned declaration.
    assert_eq!(
        failure(
            missing_version
                .select(&SelectorInput::invalid_type(
                    InvalidSelectorType::Null,
                    bound
                ))
                .unwrap()
        ),
        SelectionFailure::InvalidType(InvalidSelectorType::Null)
    );
}

#[test]
fn count_overrun_and_unadmitted_only_identity_never_choose_a_partial_profile() {
    let mut value = minimal_config();
    value["profiles"]["second"] = value["profiles"]["app"].clone();
    let mut capacity = limits();
    capacity.max_profiles = Bound::new(1).unwrap();
    let bound = capacity.max_profile_id_bytes;
    assert_eq!(
        unavailable(
            analyze_with_limits(&value, capacity)
                .select(&SelectorInput::string("app", bound))
                .unwrap()
        ),
        SelectionPrerequisite::ProfilesLimit
    );
    for key in ["BAD", &"a".repeat(257)] {
        let analysis = analyze_fixture(&json!({"schemaVersion":"0", "profiles":{key: {}}}));
        assert_eq!(
            unavailable(analysis.select(&SelectorInput::absent(bound)).unwrap()),
            SelectionPrerequisite::DeclarationIdentity
        );
    }
}

#[test]
fn selected_declaration_boundary_and_resource_reference_are_independent_prerequisites() {
    let bound = limits().max_profile_id_bytes;
    let name = SelectorInput::string("app", bound);
    for declaration in [json!(false), json!({})] {
        let analysis =
            analyze_fixture(&json!({"schemaVersion":"0", "profiles":{"app":declaration}}));
        assert_eq!(
            unavailable(analysis.select(&name).unwrap()),
            SelectionPrerequisite::DeclarationBoundary
        );
    }
    let mut unknown = minimal_config();
    unknown["profiles"]["app"]["unknown-secret"] = json!(true);
    assert_eq!(
        unavailable(analyze_fixture(&unknown).select(&name).unwrap()),
        SelectionPrerequisite::DeclarationBoundary
    );
    for resource in [json!(null), json!(false), json!({"$testPolicy":"unknown"})] {
        let mut value = minimal_config();
        value["profiles"]["app"]["policies"]["resourceLimits"] = resource;
        assert_eq!(
            unavailable(analyze_fixture(&value).select(&name).unwrap()),
            SelectionPrerequisite::ResourceLimitsReference
        );
    }
}

#[test]
fn independent_nested_and_sibling_errors_allow_only_provisional_selection() {
    let bound = limits().max_profile_id_bytes;
    let mut value = minimal_config();
    value["$schema"] = json!(false);
    value["profiles"]["app"]["coverage"] = json!(false);
    value["profiles"]["broken"] = json!(false);
    let analysis = analyze_fixture(&value);
    assert_eq!(
        selected(
            analysis
                .select(&SelectorInput::string("app", bound))
                .unwrap()
        )
        .id()
        .as_str(),
        "app"
    );
    assert!(analysis.construct().unwrap().is_none());
    assert!(analysis.profile("app").unwrap().is_none());
    assert!(analysis.profile("broken").unwrap().is_none());
}

#[test]
fn failed_and_repeated_selections_cannot_mutate_the_analysis_or_previous_result() {
    let analysis = analyze_fixture(&minimal_config());
    let bound = limits().max_profile_id_bytes;
    let first = selected(analysis.select(&SelectorInput::absent(bound)).unwrap());
    assert_eq!(
        failure(
            analysis
                .select(&SelectorInput::string("unknown", bound))
                .unwrap()
        ),
        SelectionFailure::Unknown { bytes: 7 }
    );
    let repeated = selected(analysis.select(&SelectorInput::absent(bound)).unwrap());
    drop(analysis);
    assert_eq!(first.id(), repeated.id());
    assert_eq!(first.resource_limits(), repeated.resource_limits());
}
