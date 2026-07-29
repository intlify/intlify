// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::sync::atomic::{AtomicUsize, Ordering};

use intlify_contract::{
    ArtifactNamespace, CatalogKeyDomain, CatalogScopeId, CatalogScopeName, MessageSelector,
    PortablePathSegment, PortableRelativePath, SourceDocumentIdentity,
};
use intlify_producer_js::{
    scan_source, scan_source_with_cancellation, JsGrammarProfile, JsKeySyntax, JsProducerError,
    JsProducerFailureReason, JsProducerFailureStage, JsRecognizerBinding, JsRecognizerCallKind,
    JsRecognizerConfigurationError, JsRecognizerSet, JsSelectedSourceGoal, JsSourceGoal,
    JsSourceProfile,
};

fn source(path: &str) -> SourceDocumentIdentity {
    let segments = path
        .split('/')
        .map(|segment| PortablePathSegment::try_new(segment).unwrap())
        .collect();
    SourceDocumentIdentity::new(
        ArtifactNamespace::Project,
        PortableRelativePath::try_new(segments).unwrap(),
    )
}

fn scope(name: &str) -> CatalogScopeId {
    CatalogScopeId::new(
        ArtifactNamespace::Project,
        CatalogScopeName::try_new(name).unwrap(),
    )
}

fn recognizer(
    callee: &str,
    kind: JsRecognizerCallKind,
    key_syntax: JsKeySyntax,
) -> JsRecognizerBinding {
    JsRecognizerBinding::try_new(
        callee,
        kind,
        scope("app"),
        CatalogKeyDomain::JsonPointer,
        key_syntax,
    )
    .unwrap()
}

fn recognizers(bindings: Vec<JsRecognizerBinding>) -> JsRecognizerSet {
    JsRecognizerSet::try_new(bindings).unwrap()
}

fn one_lookup(callee: &str, key_syntax: JsKeySyntax) -> JsRecognizerSet {
    recognizers(vec![recognizer(
        callee,
        JsRecognizerCallKind::Lookup,
        key_syntax,
    )])
}

fn failed(error: JsProducerError) -> intlify_producer_js::JsProducerFailure {
    match error {
        JsProducerError::Failed(failure) => failure,
        other => panic!("expected producer failure, got {other:?}"),
    }
}

#[test]
fn source_profiles_use_the_exact_longest_suffix_matrix() {
    let cases = [
        (
            "src/a.js",
            JsGrammarProfile::JavaScript,
            JsSourceGoal::BoundedUnambiguous,
        ),
        (
            "src/a.jsx",
            JsGrammarProfile::JavaScriptJsx,
            JsSourceGoal::BoundedUnambiguous,
        ),
        (
            "src/a.mjs",
            JsGrammarProfile::JavaScript,
            JsSourceGoal::Module,
        ),
        (
            "src/a.cjs",
            JsGrammarProfile::JavaScript,
            JsSourceGoal::CommonJs,
        ),
        (
            "src/a.ts",
            JsGrammarProfile::TypeScript,
            JsSourceGoal::BoundedUnambiguous,
        ),
        (
            "src/a.tsx",
            JsGrammarProfile::TypeScriptJsx,
            JsSourceGoal::BoundedUnambiguous,
        ),
        (
            "src/a.mts",
            JsGrammarProfile::TypeScript,
            JsSourceGoal::Module,
        ),
        (
            "src/a.cts",
            JsGrammarProfile::TypeScript,
            JsSourceGoal::CommonJs,
        ),
        (
            "src/a.d.ts",
            JsGrammarProfile::TypeScriptDeclaration,
            JsSourceGoal::BoundedUnambiguous,
        ),
        (
            "src/a.d.mts",
            JsGrammarProfile::TypeScriptDeclaration,
            JsSourceGoal::Module,
        ),
        (
            "src/a.d.cts",
            JsGrammarProfile::TypeScriptDeclaration,
            JsSourceGoal::CommonJs,
        ),
    ];

    for (path, grammar, goal) in cases {
        let profile = JsSourceProfile::from_source(&source(path)).unwrap();
        assert_eq!(profile.grammar(), grammar, "{path}");
        assert_eq!(profile.goal(), goal, "{path}");
    }

    for path in [
        "src/a",
        "src/a.JS",
        "src/a.Ts",
        "src/a.vue",
        "src/a.mjsx",
        "src/a.cjsx",
    ] {
        assert!(
            JsSourceProfile::from_source(&source(path)).is_err(),
            "{path}"
        );
        let failure =
            failed(scan_source(&source(path), b"", &JsRecognizerSet::empty()).unwrap_err());
        assert_eq!(failure.stage(), JsProducerFailureStage::Profile);
        assert_eq!(
            failure.reason(),
            JsProducerFailureReason::UnsupportedSourceSuffix
        );
    }
}

#[test]
fn supported_grammar_fixtures_produce_only_explicitly_recognized_calls() {
    let fixtures = [
        (
            "fixtures/exact.js",
            include_bytes!("../fixtures/js/exact.js").as_slice(),
            1,
        ),
        (
            "fixtures/view.jsx",
            include_bytes!("../fixtures/jsx/view.jsx").as_slice(),
            1,
        ),
        (
            "fixtures/exact.ts",
            include_bytes!("../fixtures/ts/exact.ts").as_slice(),
            1,
        ),
        (
            "fixtures/view.tsx",
            include_bytes!("../fixtures/tsx/view.tsx").as_slice(),
            1,
        ),
        (
            "fixtures/types.d.ts",
            include_bytes!("../fixtures/ts/types.d.ts").as_slice(),
            0,
        ),
    ];
    let configured = one_lookup("t", JsKeySyntax::DotPath);

    for (path, bytes, expected) in fixtures {
        let scan = scan_source(&source(path), bytes, &configured).unwrap();
        assert_eq!(scan.references().len(), expected, "{path}");
    }

    let unconfigured = scan_source(
        &source("src/unconfigured.ts"),
        b"t('checkout.title'); i18n.t('checkout.title')",
        &JsRecognizerSet::empty(),
    )
    .unwrap();
    assert!(unconfigured.references().is_empty());
}

#[test]
fn bounded_unambiguous_sources_prefer_module_and_retry_only_as_script() {
    let configured = one_lookup("t", JsKeySyntax::Literal);

    let module = scan_source(
        &source("src/module.js"),
        b"export const value = t('module')",
        &configured,
    )
    .unwrap();
    assert_eq!(module.selected_goal(), JsSelectedSourceGoal::Module);

    let script = scan_source(
        &source("src/legacy.js"),
        b"var await = t('legacy')",
        &configured,
    )
    .unwrap();
    assert_eq!(script.selected_goal(), JsSelectedSourceGoal::Script);
    assert_eq!(script.references().len(), 1);

    let fixed_module = failed(
        scan_source(
            &source("src/legacy.mjs"),
            b"var await = t('legacy')",
            &configured,
        )
        .unwrap_err(),
    );
    assert_eq!(fixed_module.stage(), JsProducerFailureStage::Parse);
    assert_eq!(
        fixed_module.reason(),
        JsProducerFailureReason::SyntaxInvalid
    );

    let fixed_commonjs = scan_source(
        &source("src/legacy.cjs"),
        b"with ({}) { t('legacy') }",
        &configured,
    )
    .unwrap();
    assert_eq!(fixed_commonjs.selected_goal(), JsSelectedSourceGoal::Script);
}

#[test]
fn recovered_parser_state_never_emits_partial_references() {
    let error = failed(
        scan_source(
            &source("src/broken.ts"),
            b"t('before'); const = ; t('after')",
            &one_lookup("t", JsKeySyntax::Literal),
        )
        .unwrap_err(),
    );
    assert_eq!(error.stage(), JsProducerFailureStage::Parse);
    assert_eq!(error.reason(), JsProducerFailureReason::SyntaxInvalid);
}

#[test]
fn source_snapshot_preserves_bom_offsets_and_rejects_invalid_utf8() {
    let mut source_bytes = vec![0xef, 0xbb, 0xbf];
    source_bytes.extend_from_slice(b"t('title')");
    let scan = scan_source(
        &source("src/bom.js"),
        &source_bytes,
        &one_lookup("t", JsKeySyntax::Literal),
    )
    .unwrap();
    let span = scan.references()[0].origin().unwrap().span();
    assert_eq!(
        &source_bytes[span.start() as usize..span.end() as usize],
        b"'title'"
    );
    assert_eq!(span.start(), 5);

    let invalid = failed(
        scan_source(
            &source("src/invalid.js"),
            &[0xff],
            &one_lookup("t", JsKeySyntax::Literal),
        )
        .unwrap_err(),
    );
    assert_eq!(invalid.stage(), JsProducerFailureStage::Snapshot);
    assert_eq!(invalid.reason(), JsProducerFailureReason::InvalidUtf8);
}

#[test]
fn recognizer_configuration_is_checked_canonical_and_duplicate_free() {
    let first = recognizer("i18n.t", JsRecognizerCallKind::Lookup, JsKeySyntax::Literal);
    let second = recognizer("t", JsRecognizerCallKind::Lookup, JsKeySyntax::Literal);
    let set = recognizers(vec![second, first]);
    assert_eq!(set.bindings()[0].callee(), "i18n.t");
    assert_eq!(set.bindings()[1].callee(), "t");

    assert_eq!(
        JsRecognizerBinding::try_new(
            "await",
            JsRecognizerCallKind::Lookup,
            scope("app"),
            CatalogKeyDomain::JsonPointer,
            JsKeySyntax::Literal,
        )
        .unwrap_err(),
        JsRecognizerConfigurationError::ReservedCalleeRoot
    );
    assert_eq!(
        JsRecognizerBinding::try_new(
            "i18n..t",
            JsRecognizerCallKind::Lookup,
            scope("app"),
            CatalogKeyDomain::JsonPointer,
            JsKeySyntax::Literal,
        )
        .unwrap_err(),
        JsRecognizerConfigurationError::InvalidCallee
    );
    assert_eq!(
        JsRecognizerBinding::try_new(
            "t",
            JsRecognizerCallKind::Lookup,
            scope("app"),
            CatalogKeyDomain::Xliff2,
            JsKeySyntax::DotPath,
        )
        .unwrap_err(),
        JsRecognizerConfigurationError::UnsupportedKeySyntaxDomain
    );

    let duplicate = recognizer("t", JsRecognizerCallKind::Set, JsKeySyntax::Canonical);
    assert_eq!(
        JsRecognizerSet::try_new(vec![
            recognizer("t", JsRecognizerCallKind::Lookup, JsKeySyntax::Literal),
            duplicate,
        ])
        .unwrap_err(),
        JsRecognizerConfigurationError::DuplicateCallee
    );
}

#[test]
fn matching_accepts_only_direct_static_non_optional_callee_chains() {
    let configured = recognizers(vec![
        recognizer("t", JsRecognizerCallKind::Lookup, JsKeySyntax::Literal),
        recognizer(
            "i18n.global.t",
            JsRecognizerCallKind::Lookup,
            JsKeySyntax::Literal,
        ),
        recognizer(
            "this.$t",
            JsRecognizerCallKind::Lookup,
            JsKeySyntax::Literal,
        ),
    ]);
    let source_text = r#"
        t("direct");
        i18n.global.t("member");
        this.$t("this");
        i18n["global"].t("computed");
        i18n?.global.t("optional");
        getI18n().global.t("call-root");
        (t)("parenthesized");
        function shadow(t) { t("shadowed") }
    "#;
    let scan = scan_source(
        &source("src/shapes.js"),
        source_text.as_bytes(),
        &configured,
    )
    .unwrap();
    let keys: Vec<_> = scan
        .references()
        .iter()
        .map(|reference| match reference.selector() {
            MessageSelector::Exact(key) => key.as_str(),
            other => panic!("unexpected selector: {other:?}"),
        })
        .collect();
    assert_eq!(keys, ["/direct", "/member", "/this", "/shadowed"]);
}

#[test]
fn first_argument_contract_fails_complete_and_ignores_later_arguments() {
    let configured = one_lookup("t", JsKeySyntax::Literal);

    let missing = failed(scan_source(&source("src/missing.js"), b"t()", &configured).unwrap_err());
    assert_eq!(missing.stage(), JsProducerFailureStage::Recognizer);
    assert_eq!(
        missing.reason(),
        JsProducerFailureReason::SelectorArgumentMissing
    );

    let spread =
        failed(scan_source(&source("src/spread.js"), b"t(...keys)", &configured).unwrap_err());
    assert_eq!(spread.stage(), JsProducerFailureStage::Recognizer);
    assert_eq!(
        spread.reason(),
        JsProducerFailureReason::SelectorArgumentSpread
    );

    let later = scan_source(
        &source("src/later.js"),
        b"t('title', ...options)",
        &configured,
    )
    .unwrap();
    assert_eq!(later.references().len(), 1);
}

#[test]
fn static_evaluation_is_expression_local_and_preserves_outer_argument_spans() {
    let configured = one_lookup("t", JsKeySyntax::Literal);
    let source_text = r#"
        t("plain");
        t(`template`);
        t(("wrapped"));
        t("asserted" as const);
        t("checked" satisfies string);
        t("nonnull"!);
        t(<string>"typed");
        t(KEY);
        t("prefix-" + suffix);
        t(`prefix-${suffix}`);
    "#;
    let scan = scan_source(
        &source("src/static.ts"),
        source_text.as_bytes(),
        &configured,
    )
    .unwrap();
    assert_eq!(scan.references().len(), 10);

    for reference in &scan.references()[..7] {
        assert!(matches!(reference.selector(), MessageSelector::Exact(_)));
        assert!(reference.reason().is_none());
    }
    for reference in &scan.references()[7..] {
        assert_eq!(reference.selector(), &MessageSelector::UnboundedDynamic);
        assert_eq!(
            reference.reason().unwrap().as_str(),
            "lookup argument is not statically known"
        );
    }

    let asserted = &scan.references()[3];
    let span = asserted.origin().unwrap().span();
    assert_eq!(
        &source_text[span.start() as usize..span.end() as usize],
        r#""asserted" as const"#
    );
}

#[test]
fn key_syntax_and_call_kind_select_exact_or_pattern_without_inference() {
    let configured = recognizers(vec![
        recognizer(
            "canonical",
            JsRecognizerCallKind::Lookup,
            JsKeySyntax::Canonical,
        ),
        recognizer(
            "literal",
            JsRecognizerCallKind::Lookup,
            JsKeySyntax::Literal,
        ),
        recognizer("dot", JsRecognizerCallKind::Lookup, JsKeySyntax::DotPath),
        recognizer("set", JsRecognizerCallKind::Set, JsKeySyntax::DotPath),
    ]);
    let scan = scan_source(
        &source("src/keys.ts"),
        br#"
            canonical("/checkout/title");
            literal("checkout.title");
            dot("checkout.title");
            dot("errors.*");
            set("errors.*");
            set("errors.literal\\.*");
        "#,
        &configured,
    )
    .unwrap();

    let selectors: Vec<_> = scan
        .references()
        .iter()
        .map(intlify_contract::MessageReference::selector)
        .collect();
    assert!(matches!(
        selectors[0],
        MessageSelector::Exact(key) if key.as_str() == "/checkout/title"
    ));
    assert!(matches!(
        selectors[1],
        MessageSelector::Exact(key) if key.as_str() == "/checkout.title"
    ));
    assert!(matches!(
        selectors[2],
        MessageSelector::Exact(key) if key.as_str() == "/checkout/title"
    ));
    assert!(matches!(
        selectors[3],
        MessageSelector::Exact(key) if key.as_str() == "/errors/*"
    ));
    assert!(matches!(
        selectors[4],
        MessageSelector::Pattern(pattern) if pattern.as_str() == "/errors/*"
    ));
    assert!(
        matches!(
            selectors[5],
            MessageSelector::Pattern(pattern) if pattern.as_str() == "/errors/literal.~2"
        ),
        "{:?}",
        selectors[5]
    );
    assert_eq!(
        scan.references()[4].reason().unwrap().as_str(),
        "bounded set declared by configured recognizer"
    );
}

#[test]
fn known_invalid_and_dynamic_set_selectors_have_distinct_failures() {
    let lookup = one_lookup("t", JsKeySyntax::DotPath);
    let invalid_lookup =
        failed(scan_source(&source("src/lookup.ts"), b"t('bad..key')", &lookup).unwrap_err());
    assert_eq!(invalid_lookup.stage(), JsProducerFailureStage::Selector);
    assert_eq!(
        invalid_lookup.reason(),
        JsProducerFailureReason::LookupSelectorInvalid
    );

    let set = recognizers(vec![recognizer(
        "useMessageSet",
        JsRecognizerCallKind::Set,
        JsKeySyntax::DotPath,
    )]);
    let dynamic_set = failed(
        scan_source(
            &source("src/set-dynamic.ts"),
            b"useMessageSet(PATTERN)",
            &set,
        )
        .unwrap_err(),
    );
    assert_eq!(
        dynamic_set.reason(),
        JsProducerFailureReason::SetSelectorDynamic
    );

    let invalid_set = failed(
        scan_source(
            &source("src/set-invalid.ts"),
            b"useMessageSet('bad..*')",
            &set,
        )
        .unwrap_err(),
    );
    assert_eq!(
        invalid_set.reason(),
        JsProducerFailureReason::SetSelectorInvalid
    );
}

#[test]
fn cancellation_after_a_rejected_module_attempt_does_not_retry_as_script() {
    let probes = AtomicUsize::new(0);
    let cancellation = || probes.fetch_add(1, Ordering::SeqCst) >= 1;
    let error = scan_source_with_cancellation(
        &source("src/cancel.js"),
        b"var await = t('legacy')",
        &one_lookup("t", JsKeySyntax::Literal),
        &cancellation,
    )
    .unwrap_err();
    assert_eq!(error, JsProducerError::Cancelled);
}

#[test]
fn scans_are_reentrant_and_safe_to_run_from_independent_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<JsSourceProfile>();
    assert_send_sync::<JsRecognizerSet>();
    assert_send_sync::<intlify_producer_js::JsSourceScan>();

    let handles: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(|| {
                scan_source(
                    &source("src/thread.ts"),
                    b"t('thread.key')",
                    &one_lookup("t", JsKeySyntax::DotPath),
                )
                .unwrap()
            })
        })
        .collect();

    let scans: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert!(scans.windows(2).all(|pair| pair[0] == pair[1]));
}
