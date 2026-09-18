// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 016's equality and change vectors for the Intent revision.
//!
//! Each case names two messages and whether their revisions must match. The
//! pairs come from the change table in 016 rather than from observing this
//! implementation, so a projection that silently starts merging or splitting
//! revisions fails here instead of passing a snapshot update.

use intlify_authoring::{
    analyze_message, intent_projection, intent_revision, AnalysisWorkspace, AuthoringLimits,
    ByteRange, MessageInput, Occurrence, OccurrenceRole, OwnerIdentity, OwnerKind, SourceSnapshot,
    VersionedIdentity,
};

fn limits() -> AuthoringLimits {
    AuthoringLimits {
        declarations: 1024,
        message_text_bytes: 64 * 1024,
        emitted_mf2_bytes: 128 * 1024,
        extraction_segments: 4096,
        parameter_names: 64,
        parameter_name_bytes: 256,
        metadata_value_bytes: 4096,
        vocabulary_members: 256,
        projection_nodes: 4096,
        projection_depth: 32,
        diagnostics: 256,
    }
    .validate()
    .expect("satisfiable bounds")
}

fn occurrence() -> Occurrence {
    let source = SourceSnapshot::new(
        OwnerIdentity::new(OwnerKind::Application, "storefront").expect("checked project id"),
        "checkout",
        "1",
        VersionedIdentity::literal("intlify-fixture-grammar", "0"),
        4096,
        &format!("sha256:{}", "0".repeat(64)),
    )
    .expect("checked snapshot");
    Occurrence::new(
        source,
        ByteRange::new(0, 16).expect("forward range"),
        OccurrenceRole::UiLiteral,
    )
    .expect("range inside the snapshot")
}

/// Compute the revision of one message under an explicit context.
fn revision_of(input: MessageInput<'_>, locale: &str, description: Option<&str>) -> String {
    let mut workspace = AnalysisWorkspace::new();
    let analysis = analyze_message(input, &occurrence(), &limits(), &mut workspace)
        .unwrap_or_else(|error| panic!("analysis failed: {error:?}"));
    let facts = analysis
        .facts()
        .unwrap_or_else(|| panic!("message not understood: {:?}", analysis.diagnostics()));
    let projection = intent_projection(
        facts.message().clone(),
        facts.parameters().to_vec().into_boxed_slice(),
        locale,
        None,
        description,
    )
    .expect("checked context");
    intent_revision(&projection)
        .expect("canonical encoding")
        .as_str()
        .to_owned()
}

fn mf2(source: &str) -> String {
    revision_of(MessageInput::Mf2(source), "en", None)
}

fn literal(text: &str) -> String {
    revision_of(MessageInput::Literal(text), "en", None)
}

#[test]
fn host_spelling_and_wrappers_do_not_change_a_revision() {
    // A simple message and a quoted pattern with equal content are one message.
    assert_eq!(mf2("Pay now"), mf2("{{Pay now}}"));
    // Displayed text reaches the same projection as the equivalent MF2.
    assert_eq!(literal("Pay now"), mf2("{{Pay now}}"));
    // A quoted and an unquoted literal with equal decoded values are equal.
    assert_eq!(mf2("{|one| :string}"), mf2("{one :string}"));
    // How the parser happened to split text runs is not message content.
    assert_eq!(mf2("{{a\\{b}}"), literal("a{b"));
}

#[test]
fn literal_content_is_compared_exactly() {
    assert_ne!(literal("Save"), literal("Save "));
    assert_ne!(literal("Save"), literal("save"));
    assert_ne!(literal("one\ntwo"), literal("one two"));
    assert_ne!(literal("a  b"), literal("a b"));
    // Canonically equivalent but byte-distinct Unicode stays distinct: the
    // projection must not normalize literal content.
    assert_ne!(literal("caf\u{e9}"), literal("cafe\u{301}"));
    assert_eq!(literal("caf\u{e9}"), literal("caf\u{e9}"));
}

#[test]
fn message_structure_changes_the_revision() {
    let base = ".input {$count :number}\n.match $count\none {{one}}\n* {{many}}";
    // Reordering variants is a different message under a conservative
    // comparison, even where the rendered output might coincide.
    let reordered = ".input {$count :number}\n.match $count\n* {{many}}\none {{one}}";
    assert_ne!(mf2(base), mf2(reordered));
    // Adding a branch changes it.
    let extra = ".input {$count :number}\n.match $count\none {{one}}\ntwo {{two}}\n* {{many}}";
    assert_ne!(mf2(base), mf2(extra));
    // Changing a variant body changes it.
    let edited = ".input {$count :number}\n.match $count\none {{ONE}}\n* {{many}}";
    assert_ne!(mf2(base), mf2(edited));
    // An identical rewrite does not.
    assert_eq!(mf2(base), mf2(base));
}

#[test]
fn parameter_and_function_requirements_change_the_revision() {
    assert_ne!(mf2("Hello {$name}"), mf2("Hello {$other}"));
    assert_ne!(mf2("{$amount :number}"), mf2("{$amount :integer}"));
    assert_ne!(
        mf2("{$amount :number}"),
        mf2("{$amount :number style=percent}")
    );
    assert_ne!(
        mf2("{$amount :number style=percent}"),
        mf2("{$amount :number style=currency}")
    );
    // Option order is preserved, so it participates in the comparison.
    assert_ne!(mf2("{$a :fn one=1 two=2}"), mf2("{$a :fn two=2 one=1}"));
}

#[test]
fn local_declarations_participate_without_alpha_renaming() {
    let base = ".local $greeting = {|hello|}\n{{{$greeting}}}";
    let renamed = ".local $welcome = {|hello|}\n{{{$welcome}}}";
    // The projection is deliberately conservative: it does not alpha-rename.
    assert_ne!(mf2(base), mf2(renamed));

    let reinitialized = ".local $greeting = {|hi|}\n{{{$greeting}}}";
    assert_ne!(mf2(base), mf2(reinitialized));

    let reordered = ".local $a = {|x|}\n.local $b = {|y|}\n{{{$a}{$b}}}";
    let swapped = ".local $b = {|y|}\n.local $a = {|x|}\n{{{$a}{$b}}}";
    assert_ne!(mf2(reordered), mf2(swapped));
}

#[test]
fn context_changes_the_revision_and_other_dependencies_do_not() {
    let source = "Pay now";
    assert_ne!(
        revision_of(MessageInput::Literal(source), "en", None),
        revision_of(MessageInput::Literal(source), "ja", None)
    );
    assert_ne!(
        revision_of(MessageInput::Literal(source), "en", None),
        revision_of(MessageInput::Literal(source), "en", Some("Primary action"))
    );
    assert_ne!(
        revision_of(MessageInput::Literal(source), "en", Some("Primary action")),
        revision_of(
            MessageInput::Literal(source),
            "en",
            Some("Secondary action")
        )
    );
    // The same context twice is the same revision, so the digest is stable
    // across runs rather than carrying anything acquired at analysis time.
    assert_eq!(
        revision_of(MessageInput::Literal(source), "en", Some("Primary action")),
        revision_of(MessageInput::Literal(source), "en", Some("Primary action"))
    );
}

#[test]
fn markup_and_attributes_participate() {
    assert_ne!(mf2("{#b}x{/b}"), mf2("{#i}x{/i}"));
    assert_ne!(mf2("{#b}x{/b}"), mf2("{#b /}x"));
    assert_ne!(mf2("{$a :fn}"), mf2("{$a :fn @note=one}"));
    assert_ne!(mf2("{$a :fn @note=one}"), mf2("{$a :fn @note=two}"));
}
