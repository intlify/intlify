// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Source-backed semantic construction and data-model validation fixtures.

use ox_mf2_parser::{
    build_semantic_model, parse_source, validate_semantics, AttributeOwnerKind, DeclarationKind,
    DiagnosticCode, ParseOptions, ReferenceKind, SemanticDiagnostic, SemanticDiagnosticCode,
    SemanticInvariantErrorKind, SemanticModel, SourceFileInput, SourceStore,
};
use std::fmt::Write;

fn parse(source: &str) -> (SourceStore, ox_mf2_parser::ParseResult) {
    let mut sources = SourceStore::new();
    let source_id = sources.add(SourceFileInput {
        source,
        ..Default::default()
    });
    let result = parse_source(&sources, source_id, ParseOptions::default()).unwrap();
    (sources, result)
}

fn diagnostics(source: &str) -> Vec<SemanticDiagnosticCode> {
    semantic_diagnostics(source)
        .into_iter()
        .map(|diagnostic| diagnostic.code())
        .collect()
}

fn semantic_diagnostics(source: &str) -> Vec<SemanticDiagnostic> {
    let (sources, result) = parse(source);
    assert!(
        result.diagnostics.is_empty(),
        "parser diagnostics: {source}"
    );
    let model = build_semantic_model(&sources, &result).unwrap();
    validate_semantics(&model).unwrap()
}

#[test]
fn construction_rejects_parser_diagnostics_and_mismatched_owner() {
    let (sources, malformed) = parse("Hello {$name");
    assert_eq!(
        build_semantic_model(&sources, &malformed)
            .unwrap_err()
            .kind(),
        SemanticInvariantErrorKind::ApiMisuse
    );

    let (sources, valid) = parse("Hello {$name}");
    let mut unrelated = SourceStore::new();
    unrelated.add(SourceFileInput {
        source: "x",
        ..Default::default()
    });
    assert_eq!(
        build_semantic_model(&unrelated, &valid).unwrap_err().kind(),
        SemanticInvariantErrorKind::ApiMisuse
    );
    assert!(build_semantic_model(&sources, &valid).is_ok());

    assert_eq!(
        validate_semantics(&SemanticModel::default())
            .unwrap_err()
            .kind(),
        SemanticInvariantErrorKind::InvariantViolation
    );
}

#[test]
fn semantic_fact_surface_cooks_names_resolves_references_and_retains_annotations() {
    let (sources, result) = parse(
        ".input {$count :number}\n.local $label = {$external}\n{{{$count} {$label} {$external}}}",
    );
    let model = build_semantic_model(&sources, &result).unwrap();
    let declarations = model.semantic_declarations();
    assert_eq!(declarations.len(), 2);
    assert_eq!(declarations[0].kind(), DeclarationKind::Input);
    assert_eq!(declarations[0].name(), "count");
    assert_eq!(declarations[0].input_annotation(), Some("number"));
    assert_eq!(declarations[1].kind(), DeclarationKind::Local);
    assert_eq!(declarations[1].name(), "label");

    let references = model.semantic_references();
    assert!(references.iter().any(|reference| {
        reference.name() == "external"
            && reference.kind() == ReferenceKind::LocalRhs
            && reference.resolved_declaration().is_none()
            && reference.is_declaration_dependency()
    }));
    assert!(references.iter().any(|reference| {
        reference.name() == "count"
            && reference.kind() == ReferenceKind::MessageBody
            && reference.resolved_declaration() == Some(declarations[0].id())
    }));
}

#[test]
fn output_and_selection_helpers_partition_body_and_selector_setup() {
    let (sources, result) = parse(
        ".input {$digits :number}\n.input {$count :number min=$digits}\n.local $selector = {$count}\n{{{$external} {$body :number option=$bodyOption}}}",
    );
    let model = build_semantic_model(&sources, &result).unwrap();
    let output_names: Vec<&str> = model
        .output_references()
        .map(ox_mf2_parser::SemanticReference::name)
        .collect();
    assert_eq!(output_names, ["external", "body", "bodyOption"]);

    let (sources, result) = parse(
        ".input {$digits :number}\n.input {$count :number min=$digits}\n.local $selector = {$count}\n.match $selector\n* {{ok}}",
    );
    let model = build_semantic_model(&sources, &result).unwrap();
    let selection_names: Vec<&str> = model
        .selection_references()
        .map(ox_mf2_parser::SemanticReference::name)
        .collect();
    assert_eq!(selection_names, ["digits", "count", "selector"]);
}

#[test]
fn attribute_facts_preserve_owner_taxonomy_and_owner_local_order() {
    let (sources, result) = parse(
        "{{{$value @first=|a| @second=|b|} {#link @open=|x|}x{/link @close=|y|} {#icon @standalone=|z|/}}}",
    );
    let model = build_semantic_model(&sources, &result).unwrap();
    let attributes = model.semantic_attributes();
    assert_eq!(attributes.len(), 5);
    assert_eq!(attributes[0].owner_kind(), AttributeOwnerKind::Expression);
    assert_eq!(attributes[0].owner_local_ordinal(), 0);
    assert_eq!(attributes[1].owner_kind(), AttributeOwnerKind::Expression);
    assert_eq!(attributes[1].owner_local_ordinal(), 1);
    assert_eq!(attributes[2].owner_kind(), AttributeOwnerKind::MarkupOpen);
    assert_eq!(attributes[3].owner_kind(), AttributeOwnerKind::MarkupClose);
    assert_eq!(
        attributes[4].owner_kind(),
        AttributeOwnerKind::MarkupStandalone
    );
}

#[test]
fn duplicate_declarations_report_every_occurrence_after_the_first() {
    assert_eq!(
        diagnostics(".input {$x :string}\n.local $x = {|a|}\n.local $x = {|b|}\n{{ok}}"),
        vec![
            SemanticDiagnosticCode::DuplicateDeclaration,
            SemanticDiagnosticCode::DuplicateDeclaration,
        ]
    );
}

#[test]
fn declaration_dependency_owns_self_forward_and_cycle_root_causes() {
    assert_eq!(
        diagnostics(".local $a = {$b}\n.local $b = {$a}\n{{{$a}}}"),
        vec![SemanticDiagnosticCode::InvalidDeclarationDependency]
    );
    assert_eq!(
        diagnostics(".input {$count :number minimumFractionDigits=$count}\n{{{$count}}}"),
        vec![SemanticDiagnosticCode::InvalidDeclarationDependency]
    );
    assert_eq!(
        diagnostics(".local $a = {$b}\n.local $b = {$c}\n.local $c = {$a}\n{{{$a}}}"),
        vec![SemanticDiagnosticCode::InvalidDeclarationDependency]
    );
}

#[test]
fn dependency_labels_are_bounded_without_changing_the_primary_violation() {
    let mut options = String::new();
    for index in 0..40 {
        write!(options, " option{index}=$later").unwrap();
    }
    let mut source = format!(".input {{$value :number{options}}}\n.input {{$later :number}}");
    source.push_str("\n{{{$value}}}");
    let actual = semantic_diagnostics(&source);
    assert_eq!(actual.len(), 1);
    assert_eq!(
        actual[0].code(),
        SemanticDiagnosticCode::InvalidDeclarationDependency
    );
    assert_eq!(actual[0].labels().len(), 32);
}

#[test]
fn selector_annotation_follows_local_dependencies_and_reports_missing_roots() {
    assert!(diagnostics(
        ".input {$count :number}\n.local $selector = {$count}\n.match $selector\n* {{ok}}"
    )
    .is_empty());
    assert_eq!(
        diagnostics(".input {$count}\n.match $count\n* {{ok}}"),
        vec![SemanticDiagnosticCode::MissingSelectorAnnotation]
    );
    assert_eq!(
        diagnostics(".match $external\n* {{ok}}"),
        vec![SemanticDiagnosticCode::MissingSelectorAnnotation]
    );
}

#[test]
fn matcher_validation_separates_arity_fallback_and_duplicate_variants() {
    assert_eq!(
        diagnostics(".input {$x :string}\n.match $x\none two {{bad}}\nother {{ok}}"),
        vec![
            SemanticDiagnosticCode::MissingFallbackVariant,
            SemanticDiagnosticCode::VariantKeyArityMismatch,
        ]
    );
    assert_eq!(
        diagnostics(".input {$x :string}\n.match $x\n1 {{one}}\n|1| {{same}}\n* {{ok}}"),
        vec![SemanticDiagnosticCode::DuplicateVariant]
    );
}

#[test]
fn duplicate_options_are_owner_local_and_nfc_compared() {
    assert_eq!(
        diagnostics("{{{$x :number min=1 min=2}}}"),
        vec![SemanticDiagnosticCode::DuplicateOptionName]
    );
    assert!(diagnostics("{{{$x :number min=1} {$y :number min=2}}}").is_empty());
    assert_eq!(
        diagnostics("{{{$x :number café=1 cafe\u{301}=2 cafe\u{301}=3}}}"),
        vec![
            SemanticDiagnosticCode::DuplicateOptionName,
            SemanticDiagnosticCode::DuplicateOptionName,
        ]
    );
}

#[test]
fn duplicate_variants_compare_cooked_nfc_literals() {
    assert_eq!(
        diagnostics(
            ".input {$x :string}\n.match $x\né {{first}}\n|e\u{301}| {{second}}\n* {{fallback}}"
        ),
        vec![SemanticDiagnosticCode::DuplicateVariant]
    );
}

#[test]
fn diagnostics_lock_primary_spans_labels_and_report_order() {
    let source = ".input {$x :string}\n.input {$x :string}\n.match $missing\none two {{bad}}";
    let actual = semantic_diagnostics(source);
    assert_eq!(
        actual
            .iter()
            .map(SemanticDiagnostic::code)
            .collect::<Vec<_>>(),
        [
            SemanticDiagnosticCode::DuplicateDeclaration,
            SemanticDiagnosticCode::MissingFallbackVariant,
            SemanticDiagnosticCode::MissingSelectorAnnotation,
            SemanticDiagnosticCode::VariantKeyArityMismatch,
        ]
    );
    let duplicate = &actual[0];
    let duplicate_start = source.rfind("$x").unwrap() as u32 + 1;
    assert_eq!(duplicate.span().start, duplicate_start);
    assert_eq!(duplicate.labels().len(), 1);
    assert!(duplicate.labels()[0].span.start < duplicate.span().start);
}

#[test]
fn parser_and_semantic_json_code_catalogs_do_not_collide() {
    for parser_code in DiagnosticCode::all() {
        assert!(SemanticDiagnosticCode::all()
            .iter()
            .all(|semantic_code| parser_code.json_code() != semantic_code.json_code()));
    }
}

#[test]
fn semantic_contract_values_are_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SemanticModel>();
    assert_send_sync::<SemanticDiagnostic>();
}

#[test]
fn broken_selector_dependency_suppresses_only_its_dependent_diagnostic() {
    assert_eq!(
        diagnostics(
            ".local $selector = {$later}\n.local $later = {|x|}\n.match $selector\n* {{ok}}"
        ),
        vec![SemanticDiagnosticCode::InvalidDeclarationDependency]
    );
    assert_eq!(
        diagnostics(
            ".local $selector = {$later}\n.local $later = {|x|}\n.match $selector $other\n* * {{ok}}"
        ),
        vec![
            SemanticDiagnosticCode::InvalidDeclarationDependency,
            SemanticDiagnosticCode::MissingSelectorAnnotation,
        ]
    );
}
