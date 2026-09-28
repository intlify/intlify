// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Binding identity: an intrinsic is the binding a name resolves to.
//!
//! Every expected range is found in the source text by searching for the
//! bytes it covers, never read back from the analysis under test.

mod support;

use intlify_authoring::{OccurrenceRole, ReasonFamily, UnitOutcome};
use intlify_authoring_js::{
    detail, BindingError, Grammar, Intrinsic, IntrinsicBinding, JsAuthoringProfile,
};
use support::{
    analyze, analyze_as, at, diagnostics, expect, limits, nth, occurrence, try_analyze, PRELUDE,
};

fn declared(analysis: &intlify_authoring_js::UnitAnalysis) -> Vec<(OccurrenceRole, (u64, u64))> {
    analysis
        .inspection_facts()
        .declarations()
        .iter()
        .map(|facts| occurrence(facts.occurrence()))
        .collect()
}

#[test]
fn a_named_import_and_its_alias_both_bind_the_intrinsic() {
    let text = "import { intent as t, mf2 as message } from 'fixture-authoring'\n\
                t('Pay now')\n\
                const hello = message`Hello`\n\
                t(hello)\n";
    let analysis = analyze(text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(analysis.outcome(), UnitOutcome::Checked);
    assert_eq!(
        declared(&analysis),
        [
            (OccurrenceRole::IntentLiteral, at(text, "'Pay now'")),
            (OccurrenceRole::Mf2Declaration, at(text, "message`Hello`")),
        ]
    );
    assert_eq!(analysis.inspection_facts().references().len(), 2);
}

#[test]
fn a_name_that_does_not_resolve_to_the_import_is_not_the_intrinsic() {
    // A parameter and a block-scoped constant shadow the import, and neither
    // they nor a module that merely has the same export are recognized, and
    // nothing is reported about them.
    let text = format!(
        "{PRELUDE}function shadowed(intent) {{ return intent('Pay now') }}\n\
         {{ const intent = (text) => text; intent('Pay later') }}\n\
         export const unrelated = () => mf2Like('Save')\n"
    );
    let analysis = analyze(&text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(declared(&analysis), []);

    let elsewhere = "import { intent } from 'other-authoring'\nintent('Pay now')\n";
    let analysis = analyze(elsewhere);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(declared(&analysis), []);

    // Without any import, the spelling alone is not evidence.
    let global = "intent('Pay now')\n";
    assert_eq!(declared(&analyze(global)), []);
}

#[test]
fn a_known_intrinsic_used_other_than_directly_is_reported_where_it_is_used() {
    let text = format!(
        "{PRELUDE}const wrapped = intent\n\
         intent.call(null, 'Pay now')\n\
         intent?.('Pay later')\n\
         new intent('Save')\n\
         register(intent)\n\
         (intent)('Close')\n\
         mf2('Open')\n\
         intent`Tagged`\n\
         export {{ intent }}\n"
    );
    let analysis = analyze(&text);
    let unsupported = ReasonFamily::AuthoringFormUnsupported;
    let use_of = |needle: &str, nth_use: usize| {
        expect(
            unsupported,
            detail::intrinsic_use_unsupported(),
            nth(&text, needle, nth_use),
        )
    };
    // The first `intent` and `mf2` are the import itself.
    assert_eq!(
        diagnostics(&analysis),
        [
            use_of("intent", 1),
            use_of("intent", 2),
            use_of("intent", 3),
            use_of("intent", 4),
            use_of("intent", 5),
            use_of("intent", 6),
            use_of("mf2", 1),
            use_of("intent", 7),
            use_of("intent", 8),
        ]
    );
    assert_eq!(declared(&analysis), []);
    assert_eq!(analysis.outcome(), UnitOutcome::Blocked);
}

#[test]
fn a_registered_module_imported_another_way_is_reported_once_where_it_is_imported() {
    let text = "import authoring from 'fixture-authoring'\n\
                import * as all from 'fixture-authoring'\n\
                export { intent } from 'fixture-authoring'\n\
                export * from 'fixture-authoring'\n\
                const later = import('fixture-authoring')\n\
                authoring.intent('Pay now')\n\
                all.intent('Pay later')\n";
    let analysis = analyze(text);
    let unsupported = ReasonFamily::AuthoringFormUnsupported;
    let form = |range| expect(unsupported, detail::import_form_unsupported(), range);
    // A default specifier is its local name alone.
    let (default, _) = at(text, "authoring from");
    assert_eq!(
        diagnostics(&analysis),
        [
            form((default, default + "authoring".len() as u64)),
            form(at(text, "* as all")),
            form(nth(text, "intent", 0)),
            form(at(text, "export * from 'fixture-authoring'")),
            form(at(text, "import('fixture-authoring')")),
        ]
    );
    // The uses through the namespace or default binding are not reported
    // again: the import was.
    assert_eq!(declared(&analysis), []);
}

#[test]
fn a_type_only_import_or_type_position_binds_no_value() {
    let text = "import type { intent } from 'fixture-authoring'\n\
                import { type mf2, noIntent } from 'fixture-authoring'\n\
                type Localize = typeof noIntent\n\
                let use: Localize\n";
    let analysis = analyze_as(Grammar::TsModule, text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(analysis.outcome(), UnitOutcome::Checked);
}

#[test]
fn an_unregistered_name_from_a_registered_module_is_ordinary() {
    let text = "import { format } from 'fixture-authoring'\nformat('Pay now')\n";
    let analysis = analyze(text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(declared(&analysis), []);
}

#[test]
fn a_profile_that_registers_nothing_recognizes_nothing() {
    let text = format!("{PRELUDE}intent('Pay now')\nconst hello = mf2`Hello`\nwrapped(intent)\n");
    let analysis = try_analyze(
        Grammar::JsModule,
        &text,
        &JsAuthoringProfile::new(),
        &limits(),
    )
    .expect("the analysis runs");
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(analysis.outcome(), UnitOutcome::Checked);
    assert_eq!(declared(&analysis), []);
}

#[test]
fn a_binding_set_that_contradicts_itself_builds_no_profile() {
    assert_eq!(
        JsAuthoringProfile::new().with_bindings([
            IntrinsicBinding::new("fixture-authoring", "intent", Intrinsic::Intent),
            IntrinsicBinding::new("fixture-authoring", "intent", Intrinsic::Mf2),
        ]),
        Err(BindingError::Conflict {
            module: "fixture-authoring".into(),
            export: "intent".into(),
        })
    );
    assert_eq!(
        JsAuthoringProfile::new().with_bindings([IntrinsicBinding::new(
            "",
            "intent",
            Intrinsic::Intent
        )]),
        Err(BindingError::EmptyModule)
    );
}

#[test]
fn a_script_has_no_imports_and_so_no_intrinsics() {
    // A classic script cannot import, so no binding in it is an intrinsic,
    // and `require()` is not read as an import.
    let text = "const { intent } = require('fixture-authoring')\nintent('Pay now')\n";
    let analysis = analyze_as(Grammar::JsScript, text);
    assert_eq!(diagnostics(&analysis), []);
    assert_eq!(declared(&analysis), []);
}
