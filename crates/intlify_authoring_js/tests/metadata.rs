// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! `@intlify` metadata: what an annotation says, and which declaration it
//! says it about.
//!
//! Every expected range is found in the source text by searching for the
//! bytes it covers, never read back from the analysis under test.

mod support;

use intlify_authoring::test_context::LocaleRule;
use intlify_authoring::{
    detail as shared_detail, intent_revision, Detail, OccurrenceRole, ReasonFamily,
    SourceLocaleBasis, UnitOutcome,
};
use intlify_authoring_js::{
    detail, DomGlobal, Grammar, JsAuthoringProfile, JsLimitKind, UnitAnalysis,
};
use support::{
    analyze, analyze_dom, at, context_builder, diagnostics, expect, limits, occurrence, profile,
    try_analyze, try_analyze_in, Reported, PRELUDE,
};

/// The annotation most fixtures place.
const NOTE: &str = r#"/* @intlify { "description": "Pay" } */"#;

/// What one established declaration carries: its role and range, its
/// surface class, and its description.
type Carried = (OccurrenceRole, (u64, u64), String, Option<String>);

fn carried(analysis: &UnitAnalysis) -> Vec<Carried> {
    analysis
        .inspection_facts()
        .declarations()
        .iter()
        .map(|facts| {
            let (role, range) = occurrence(facts.occurrence());
            let description = facts
                .projection()
                .description
                .as_ref()
                .map(|text| text.as_str().to_owned());
            (role, range, facts.surface_class().to_owned(), description)
        })
        .collect()
}

fn invalid(text: &str, annotation: &str, detail: Detail) -> Reported {
    expect(
        ReasonFamily::AuthoringMetadataInvalid,
        detail,
        at(text, annotation),
    )
}

/// The descriptions every established declaration carries, in order.
fn descriptions(analysis: &UnitAnalysis) -> Vec<Option<String>> {
    carried(analysis)
        .into_iter()
        .map(|(_, _, _, description)| description)
        .collect()
}

#[test]
fn one_annotation_means_the_same_above_every_declaration_form() {
    let note = r#"/* @intlify { "surfaceClass": "nav", "description": "Shown on the button" } */"#;
    let text = format!(
        "{PRELUDE}{note}\nconst greeting = mf2`Hello`\n\
         {note}\nexport const farewell = mf2`Bye`, count = 1\n\
         {note}\nintent('Pay now')\n\
         export function render() {{\n  const pay = document.querySelector('#pay')\n  \
         {note}\n  pay.textContent = 'Pay'\n  pay.textContent = 'Plain'\n}}\n"
    );
    let analysis = analyze_dom(&text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(analysis.outcome(), UnitOutcome::Checked);
    let nav = |role, needle| {
        (
            role,
            at(&text, needle),
            "nav".to_owned(),
            Some("Shown on the button".to_owned()),
        )
    };
    assert_eq!(
        carried(&analysis),
        [
            nav(OccurrenceRole::Mf2Declaration, "mf2`Hello`"),
            nav(OccurrenceRole::Mf2Declaration, "mf2`Bye`"),
            nav(OccurrenceRole::IntentLiteral, "'Pay now'"),
            nav(OccurrenceRole::UiLiteral, "'Pay'"),
            // The next statement is not described: an annotation describes
            // one statement and sets no default for what follows.
            (
                OccurrenceRole::UiLiteral,
                at(&text, "'Plain'"),
                "checkout".to_owned(),
                None
            ),
        ]
    );
}

#[test]
fn an_invalid_annotation_is_reported_and_withholds_its_declaration() {
    for (annotation, detail, why) in [
        (
            r#"/* @intlify { "description": } */"#,
            detail::metadata_json_malformed(),
            "malformed JSON",
        ),
        (
            r#"/* @intlify { "description": "Pay" } more */"#,
            detail::metadata_json_malformed(),
            "text after the object",
        ),
        (
            "/* @intlify */",
            detail::metadata_json_malformed(),
            "no object",
        ),
        (
            "/* @intlify { \"description\": \"\x5cuD800\" } */",
            detail::metadata_json_malformed(),
            "an escape that is no scalar value",
        ),
        (
            r#"/* @intlify ["Pay"] */"#,
            detail::metadata_not_object(),
            "an array",
        ),
        (
            r#"/* @intlify { "description": "a", "description": "b" } */"#,
            detail::metadata_member_duplicate(),
            "a member named twice",
        ),
        (
            r#"/* @intlify { "descripton": "Pay" } */"#, // spellchecker:disable-line
            detail::metadata_member_unknown(),
            "a misspelled member is not silently lost",
        ),
        (
            r#"/* @intlify { "description": 1 } */"#,
            detail::metadata_value_not_string(),
            "a number",
        ),
        (
            r#"/* @intlify { "sourceLocale": null } */"#,
            detail::metadata_value_not_string(),
            "null",
        ),
        (
            r#"/* @intlify { "surfaceClass": "" } */"#,
            detail::metadata_value_empty(),
            "an empty value",
        ),
    ] {
        let text = format!("{PRELUDE}{annotation}\nintent('Pay now')\nintent('Other')\n");
        let analysis = analyze(&text);
        assert_eq!(
            diagnostics(&analysis),
            [invalid(&text, annotation, detail)],
            "{why}"
        );
        assert_eq!(analysis.outcome(), UnitOutcome::Blocked, "{why}");
        // Only the annotated declaration is withheld, and its use with it.
        let established: Vec<(u64, u64)> = carried(&analysis)
            .into_iter()
            .map(|(_, range, _, _)| range)
            .collect();
        assert_eq!(established, [at(&text, "'Other'")], "{why}");
        assert_eq!(analysis.inspection_facts().references().len(), 1, "{why}");
    }
}

#[test]
fn every_member_is_optional() {
    let text = format!("{PRELUDE}/* @intlify {{}} */\nintent('Pay now')\n");
    let analysis = analyze(&text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(
        carried(&analysis),
        [(
            OccurrenceRole::IntentLiteral,
            at(&text, "'Pay now'"),
            "checkout".to_owned(),
            None
        )]
    );
}

#[test]
fn only_a_block_comment_starting_with_intlify_is_an_annotation() {
    for (comment, misspelled) in [
        ("// @intlify { \"description\": \"Pay\" }", true),
        ("/** @intlify { \"description\": \"Pay\" } */", true),
        ("/**\n * @intlify { \"description\": \"Pay\" }\n */", true),
        ("/* see @intlify for metadata */", false),
        ("/* @intlifyish { \"description\": \"Pay\" } */", false),
        ("// an ordinary note", false),
    ] {
        let text = format!("{PRELUDE}{comment}\nintent('Pay now')\n");
        let analysis = analyze(&text);
        let expected: Vec<Reported> = if misspelled {
            vec![invalid(&text, comment, detail::metadata_comment_form())]
        } else {
            Vec::new()
        };
        assert_eq!(diagnostics(&analysis), expected, "{comment}");
        // Neither kind describes the declaration.
        assert_eq!(descriptions(&analysis), [None], "{comment}");
    }
    let text = format!("{PRELUDE}/*@intlify{{\"description\":\"Pay\"}}*/\nintent('Pay now')\n");
    let analysis = analyze(&text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(descriptions(&analysis), [Some("Pay".to_owned())]);
}

#[test]
fn an_annotation_describes_exactly_one_declaration_of_the_next_statement() {
    let absent = detail::metadata_target_absent;
    let ambiguous = detail::metadata_target_ambiguous;
    let misplaced = detail::metadata_misplaced;
    for (body, failure, why) in [
        (
            "{A}\nrender(intent('Save'), intent('Cancel'))",
            Some(ambiguous()),
            "two candidates; neither is chosen",
        ),
        (
            "{A}\nconst a = mf2`A`, b = mf2`B`",
            Some(ambiguous()),
            "two declarators",
        ),
        ("{A}\nrender(1)", Some(absent()), "no declaration"),
        (
            "{A}\nrender(1)\nintent('Pay now')",
            Some(absent()),
            "a later statement is not searched",
        ),
        (
            "{A}\nfunction f() { intent('Pay now') }",
            Some(absent()),
            "not a whole function",
        ),
        (
            "{A}\nif (c) { intent('Pay now') }",
            Some(absent()),
            "not a whole block",
        ),
        (
            "{A}\nconst f = () => intent('Pay now')",
            Some(absent()),
            "an arrow body is another scope",
        ),
        (
            "{A}\nfunction f(label = intent('Pay now')) {}",
            Some(absent()),
            "a parameter default is the function's",
        ),
        (
            "const f = () => {A} intent('Pay now')",
            Some(misplaced()),
            "an arrow's expression body is no statement list",
        ),
        (
            "switch (k) {\n  case {A} 1:\n  intent('Pay now')\n}",
            Some(misplaced()),
            "inside a case test",
        ),
        (
            "{A}\nclass Label { text = intent('Pay now') }",
            Some(absent()),
            "a class body is another scope",
        ),
        (
            "{A}\nnoIntent(brand, 'Brand')",
            Some(absent()),
            "an exclusion declares nothing",
        ),
        (
            "function f() {\n  {A}\n  'use strict'\n  intent('Pay now')\n}",
            Some(absent()),
            "a directive is the next statement",
        ),
        (
            "render(intent('Save'), {A} intent('Cancel'))",
            Some(misplaced()),
            "inside an expression",
        ),
        (
            "intent('Pay now')\n{A}",
            Some(misplaced()),
            "no statement follows",
        ),
        (
            "if (c) {A} intent('Pay now')",
            Some(misplaced()),
            "a single statement body is not a list",
        ),
        (
            "{A}\nif (c) intent('Pay now')",
            None,
            "a single statement body is no other scope",
        ),
        (
            "{\n  {A}\n  intent('Pay now')\n}",
            None,
            "the first statement of a block",
        ),
        (
            "function f() {\n  'use strict'\n  {A}\n  intent('Pay now')\n}",
            None,
            "after a directive",
        ),
        (
            "switch (k) {\n  case 1:\n  {A}\n  intent('Pay now')\n}",
            None,
            "a case",
        ),
        (
            "class Label { static {\n  {A}\n  intent('Pay now')\n} }",
            None,
            "a static block",
        ),
        (
            "intent('First')\n{A}\nintent('Pay now')",
            None,
            "between two statements",
        ),
    ] {
        let text = format!("{PRELUDE}{}\n", body.replace("{A}", NOTE));
        let analysis = analyze(&text);
        let described = descriptions(&analysis)
            .into_iter()
            .filter(|description| description.as_deref() == Some("Pay"))
            .count();
        let expected: Vec<Reported> = failure
            .into_iter()
            .map(|failure| invalid(&text, NOTE, failure))
            .collect();
        assert_eq!(diagnostics(&analysis), expected, "{why}");
        // A reported annotation describes nothing; a placed one, one.
        assert_eq!(described, usize::from(expected.is_empty()), "{why}");
    }
}

#[test]
fn a_declaration_a_statement_does_not_own_stays_established() {
    // The annotation is reported, but the declarations around it are not
    // what it described, so they are established as written.
    let text = format!("{PRELUDE}{NOTE}\nrender(intent('Save'), intent('Cancel'))\n");
    let analysis = analyze(&text);
    assert_eq!(
        diagnostics(&analysis),
        [invalid(&text, NOTE, detail::metadata_target_ambiguous())]
    );
    assert_eq!(descriptions(&analysis), [None, None]);
}

#[test]
fn two_annotations_before_one_statement_are_both_reported_and_neither_applies() {
    let first = r#"/* @intlify { "description": "First" } */"#;
    let second = r#"/* @intlify { "description": "Second" } */"#;
    let text = format!("{PRELUDE}{first}\n{second}\nintent('Pay now')\nintent('Other')\n");
    let analysis = analyze(&text);
    assert_eq!(
        diagnostics(&analysis),
        [
            invalid(&text, first, detail::metadata_repeated()),
            invalid(&text, second, detail::metadata_repeated()),
        ]
    );
    let established: Vec<(u64, u64)> = carried(&analysis)
        .into_iter()
        .map(|(_, range, _, _)| range)
        .collect();
    assert_eq!(established, [at(&text, "'Other'")]);
}

#[test]
fn a_use_of_a_shared_declaration_cannot_redefine_its_metadata() {
    let own = r#"/* @intlify { "description": "Greeting" } */"#;
    let elsewhere = r#"/* @intlify { "description": "Elsewhere" } */"#;
    let text = format!(
        "{PRELUDE}{own}\nconst greeting = mf2`Hello`\n{elsewhere}\nheading.textContent = intent(greeting)\n"
    );
    let analysis = analyze(&text);
    assert_eq!(
        diagnostics(&analysis),
        [invalid(
            &text,
            elsewhere,
            detail::metadata_target_reference()
        )]
    );
    assert_eq!(
        carried(&analysis),
        [(
            OccurrenceRole::Mf2Declaration,
            at(&text, "mf2`Hello`"),
            "checkout".to_owned(),
            Some("Greeting".to_owned())
        )]
    );
    // The use itself is sound; only the attempt to describe it is reported.
    assert_eq!(analysis.inspection_facts().references().len(), 1);
}

#[test]
fn an_occurrence_already_reported_gets_no_second_record() {
    let text = format!(
        "{PRELUDE}export function render() {{\n  let pay = document.querySelector('#pay')\n  \
         {NOTE}\n  pay.textContent = 'Pay now'\n}}\n"
    );
    assert_eq!(
        diagnostics(&analyze_dom(&text)),
        [expect(
            ReasonFamily::AuthoringFormUnsupported,
            detail::receiver_binding_unsupported(),
            at(&text, "pay.textContent = 'Pay now'")
        )]
    );
    let text = format!("{PRELUDE}{NOTE}\nintent(computed())\n");
    assert_eq!(
        diagnostics(&analyze(&text)),
        [expect(
            ReasonFamily::AuthoringSourceDynamic,
            detail::message_dynamic(),
            at(&text, "computed()")
        )]
    );
}

#[test]
fn an_annotated_class_comes_before_the_invocation_default() {
    let nav = r#"/* @intlify { "surfaceClass": "nav" } */"#;
    let unknown = r#"/* @intlify { "surfaceClass": "footer" } */"#;
    let text = format!("{PRELUDE}{nav}\nintent('Nav')\nintent('Plain')\n");
    let classes: Vec<String> = carried(&analyze(&text))
        .into_iter()
        .map(|(_, _, class, _)| class)
        .collect();
    assert_eq!(classes, ["nav", "checkout"]);

    // A class outside the vocabulary is the shared crate's to report, at the
    // declaration, whatever supplied it.
    let text = format!("{PRELUDE}{unknown}\nintent('Pay now')\n");
    assert_eq!(
        diagnostics(&analyze(&text)),
        [expect(
            ReasonFamily::AuthoringSurfaceClassInvalid,
            shared_detail::surface_class_unknown(),
            at(&text, "'Pay now'")
        )]
    );

    // With no invocation default, only an annotated declaration has a class.
    let bare = context_builder().build().expect("a context");
    let text = format!("{PRELUDE}{nav}\nintent('Nav')\nintent('Plain')\n");
    let analysis = try_analyze_in(&bare, Grammar::JsModule, &text, &profile(), &limits()).unwrap();
    assert_eq!(
        diagnostics(&analysis),
        [expect(
            ReasonFamily::AuthoringSurfaceClassInvalid,
            shared_detail::surface_class_absent(),
            at(&text, "'Plain'")
        )]
    );
    assert_eq!(
        carried(&analysis),
        [(
            OccurrenceRole::IntentLiteral,
            at(&text, "'Nav'"),
            "nav".to_owned(),
            None
        )]
    );
}

#[test]
fn an_explicit_locale_reaching_the_default_gives_the_same_revision() {
    let context = context_builder()
        .default_surface_class("checkout")
        .rule(LocaleRule::canonical("EN", "en"))
        .build()
        .expect("a context");
    let read = |text: &str| {
        try_analyze_in(&context, Grammar::JsModule, text, &profile(), &limits()).unwrap()
    };
    let annotated = read(&format!(
        "{PRELUDE}/* @intlify {{ \"sourceLocale\": \"EN\" }} */\nintent('Pay now')\n"
    ));
    let plain = read(&format!("{PRELUDE}intent('Pay now')\n"));
    assert_eq!(diagnostics(&annotated), []);
    let annotated = &annotated.inspection_facts().declarations()[0];
    let plain = &plain.inspection_facts().declarations()[0];
    assert_eq!(annotated.source_locale_basis(), SourceLocaleBasis::Explicit);
    assert_eq!(
        plain.source_locale_basis(),
        SourceLocaleBasis::ContextDefault
    );
    assert_eq!(annotated.projection().source_locale.as_str(), "en");
    assert_eq!(
        intent_revision(annotated.projection()).unwrap(),
        intent_revision(plain.projection()).unwrap()
    );
}

#[test]
fn an_annotation_is_bounded_exactly_by_its_bytes() {
    let text = format!("{PRELUDE}{NOTE}\nintent('Pay now')\n");
    let mut bounded = limits();
    bounded.annotation_bytes = NOTE.len() as u64;
    let analysis = try_analyze(Grammar::JsModule, &text, &profile(), &bounded).unwrap();
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(descriptions(&analysis), [Some("Pay".to_owned())]);

    bounded.annotation_bytes -= 1;
    let analysis = try_analyze(Grammar::JsModule, &text, &profile(), &bounded).unwrap();
    assert_eq!(
        diagnostics(&analysis),
        [expect(
            ReasonFamily::AuthoringResourceLimit,
            detail::limit(JsLimitKind::AnnotationBytes),
            at(&text, NOTE)
        )]
    );
    assert_eq!(analysis.outcome(), UnitOutcome::Blocked);
    assert_eq!(carried(&analysis), []);
}

#[test]
fn a_profile_that_declares_only_by_dom_still_reads_annotations() {
    let text = format!(
        "{NOTE}\nconst pay = document.querySelector('#pay')\n{NOTE}\npay.textContent = 'Pay now'\n"
    );
    let analysis = try_analyze(
        Grammar::JsModule,
        &text,
        &JsAuthoringProfile::new().with_dom_globals([DomGlobal::Document]),
        &limits(),
    )
    .unwrap();
    // The first annotation comes before a statement declaring nothing.
    let (first, _) = text.match_indices(NOTE).next().unwrap();
    assert_eq!(
        diagnostics(&analysis),
        [expect(
            ReasonFamily::AuthoringMetadataInvalid,
            detail::metadata_target_absent(),
            (first as u64, (first + NOTE.len()) as u64)
        )]
    );
    assert_eq!(descriptions(&analysis), [Some("Pay".to_owned())]);
}
