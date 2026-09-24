// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use intlify_authoring::{
    validate_input_map, ByteRange, InputSegment, IntegrityDigest, Occurrence, OccurrenceRole,
    OwnerIdentity, OwnerKind, SourceSnapshot, VersionedIdentity,
};
use intlify_shared_json::encoding::digest_bytes;
use oxc_allocator::Allocator;
use oxc_ast::ast::{StringLiteral, TemplateElement};
use oxc_ast_visit::Visit;
use oxc_parser::Parser;
use serde::Deserialize;

use super::*;
use crate::grammar::Grammar;
use crate::limits::tests::generous;

/// One literal found in a source, and what cooking it gave.
#[derive(Debug)]
struct Found {
    span: (u32, u32),
    result: Result<Cooked, CookFailure>,
    parser_lone_surrogates: bool,
}

struct Collector<'s> {
    source: &'s str,
    limits: &'s JsAuthoringLimits,
    found: Vec<Found>,
}

impl<'a> Visit<'a> for Collector<'_> {
    fn visit_string_literal(&mut self, literal: &StringLiteral<'a>) {
        self.found.push(Found {
            span: (literal.span.start, literal.span.end),
            result: cook_string(self.source, literal, self.limits),
            parser_lone_surrogates: literal.lone_surrogates,
        });
    }

    fn visit_template_element(&mut self, element: &TemplateElement<'a>) {
        self.found.push(Found {
            span: (element.span.start, element.span.end),
            result: cook_template(self.source, element, self.limits),
            parser_lone_surrogates: element.lone_surrogates,
        });
    }
}

/// Cook every string literal and template element in `source`, in order.
fn cook_all(source: &str, grammar: Grammar, limits: &JsAuthoringLimits) -> Vec<Found> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, grammar.source_type()).parse();
    assert!(
        parsed.diagnostics.is_empty() && !parsed.panicked,
        "{source:?} parses under {grammar:?}"
    );
    let mut collector = Collector {
        source,
        limits,
        found: Vec::new(),
    };
    collector.visit_program(&parsed.program);
    collector.found.sort_by_key(|found| found.span);
    collector.found
}

/// Cook the only literal in `source`, which is read as a module.
fn cook_one(source: &str) -> Result<Cooked, CookFailure> {
    cook_one_as(source, Grammar::JsModule)
}

fn cook_one_as(source: &str, grammar: Grammar) -> Result<Cooked, CookFailure> {
    let mut found = cook_all(source, grammar, &generous());
    let literal = found.pop().expect("one literal");
    assert!(found.is_empty(), "{source:?} holds exactly one literal");
    literal.result
}

fn runs(segments: &[(u64, u64, u64, u64)]) -> Vec<InputSegment> {
    segments
        .iter()
        .map(|&(decoded_start, decoded_end, source_start, source_end)| {
            InputSegment::new(
                ByteRange::new(decoded_start, decoded_end).unwrap(),
                ByteRange::new(source_start, source_end).unwrap(),
            )
        })
        .collect()
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "it builds what a successful cook returns"
)]
fn cooked(text: &str, segments: &[(u64, u64, u64, u64)]) -> Result<Cooked, CookFailure> {
    Ok(Cooked {
        text: text.to_owned(),
        input_map: runs(segments),
    })
}

fn unsupported(form: Unsupported, start: u64, end: u64) -> Result<Cooked, CookFailure> {
    Err(CookFailure::Unsupported(
        form,
        ByteRange::new(start, end).unwrap(),
    ))
}

/// Check that a map is one `intlify_authoring` accepts for the literal.
///
/// The shared crate is the map's consumer, so its own check is the one that
/// decides whether the decoder produced something usable.
fn assert_accepted(source: &str, span: (u32, u32), cooked: &Cooked) {
    let snapshot = SourceSnapshot::new(
        OwnerIdentity::new(OwnerKind::Application, "storefront").unwrap(),
        "checkout",
        "1",
        Grammar::JsModule.identity(),
        source.len() as u64,
        IntegrityDigest::from_hash(digest_bytes(source.as_bytes())).as_str(),
    )
    .unwrap();
    let occurrence = Occurrence::new(
        snapshot,
        ByteRange::new(u64::from(span.0), u64::from(span.1)).unwrap(),
        OccurrenceRole::IntentLiteral,
    )
    .unwrap();
    assert_eq!(
        validate_input_map(&cooked.input_map, &cooked.text, &occurrence),
        Ok(()),
        "the map for {span:?} in {source:?} describes its text"
    );
}

#[test]
fn verbatim_text_is_one_positional_run() {
    assert_eq!(
        cook_one("f('Pay now')"),
        cooked("Pay now", &[(0, 7, 3, 10)])
    );
    assert_eq!(
        cook_one("f('日本語')"),
        cooked("日本語", &[(0, 9, 3, 12)]),
        "multi-byte text is still one byte for one byte"
    );
}

#[test]
fn each_escape_is_a_run_of_its_own() {
    assert_eq!(
        cook_one("f('Hi\\tthere')"),
        cooked("Hi\tthere", &[(0, 2, 3, 5), (2, 3, 5, 7), (3, 8, 7, 12)])
    );
    // Every single-character escape, with the three that name themselves.
    let expected: Vec<(u64, u64, u64, u64)> =
        (0..10).map(|i| (i, i + 1, 3 + 2 * i, 5 + 2 * i)).collect();
    assert_eq!(
        cook_one(r#"f('\b\f\n\r\t\v\0\'\"\\')"#),
        cooked("\u{8}\u{c}\n\r\t\u{b}\0'\"\\", &expected)
    );
}

#[test]
fn numeric_escapes_answer_for_their_whole_spelling() {
    assert_eq!(
        cook_one(r"f('\x41\u00e9')"),
        cooked("Aé", &[(0, 1, 3, 7), (1, 3, 7, 13)])
    );
    assert_eq!(cook_one(r"f('\u{1F600}')"), cooked("😀", &[(0, 4, 3, 12)]));
    assert_eq!(
        cook_one(r"f('\u{0000041}')"),
        cooked("A", &[(0, 1, 3, 14)]),
        "leading zeros are part of the escape"
    );
    assert_eq!(
        cook_one(r"f('\uD83D\uDE00')"),
        cooked("😀", &[(0, 4, 3, 15)]),
        "a pair is one scalar, answering for both escapes"
    );
}

#[test]
fn an_escaped_ordinary_character_is_that_character() {
    assert_eq!(
        cook_one(r"f('\a\é')"),
        cooked("aé", &[(0, 1, 3, 5), (1, 3, 5, 8)])
    );
}

#[test]
fn a_line_continuation_decodes_to_nothing_but_keeps_its_source() {
    assert_eq!(
        cook_one("f('a\\\nb')"),
        cooked("ab", &[(0, 1, 3, 4), (1, 1, 4, 6), (1, 2, 6, 7)])
    );
    assert_eq!(
        cook_one("f('a\\\r\nb')"),
        cooked("ab", &[(0, 1, 3, 4), (1, 1, 4, 7), (1, 2, 7, 8)])
    );
    assert_eq!(
        cook_one("f('a\\\rb')"),
        cooked("ab", &[(0, 1, 3, 4), (1, 1, 4, 6), (1, 2, 6, 7)])
    );
    assert_eq!(
        cook_one("f('a\\\u{2028}b')"),
        cooked("ab", &[(0, 1, 3, 4), (1, 1, 4, 8), (1, 2, 8, 9)]),
        "a line separator is a three-byte terminator"
    );
    assert_eq!(
        cook_one("f('\\\n')"),
        cooked("", &[(0, 0, 3, 5)]),
        "a literal holding only a continuation still maps its one run"
    );
}

#[test]
fn a_template_reads_every_line_ending_as_a_line_feed() {
    assert_eq!(
        cook_one("f(`a\r\nb`)"),
        cooked("a\nb", &[(0, 1, 3, 4), (1, 2, 4, 6), (2, 3, 6, 7)]),
        "CRLF is two source bytes for one decoded byte"
    );
    assert_eq!(
        cook_one("f(`a\rb`)"),
        cooked("a\nb", &[(0, 3, 3, 6)]),
        "a lone CR still answers for one byte, so the run goes on"
    );
    assert_eq!(cook_one("f(`a\nb`)"), cooked("a\nb", &[(0, 3, 3, 6)]));
    assert_eq!(
        cook_one("f(`\r\r\n\n`)"),
        cooked("\n\n\n", &[(0, 1, 3, 4), (1, 2, 4, 6), (2, 3, 6, 7)]),
        "a CR before a CRLF is a lone CR"
    );
    assert_eq!(
        cook_one(r"f(`a\r\nb`)"),
        cooked(
            "a\r\nb",
            &[(0, 1, 3, 4), (1, 2, 4, 6), (2, 3, 6, 8), (3, 4, 8, 9)]
        ),
        "escaped line endings are not normalized"
    );
    assert_eq!(
        cook_one("f(`a\\\r\nb`)"),
        cooked("ab", &[(0, 1, 3, 4), (1, 1, 4, 7), (1, 2, 7, 8)])
    );
    assert_eq!(
        cook_one("f(`a\u{2028}b`)"),
        cooked("a\u{2028}b", &[(0, 5, 3, 8)]),
        "a line separator is not a line ending a template rewrites"
    );
}

#[test]
fn template_delimiters_escape_to_themselves() {
    assert_eq!(
        cook_one(r"f(`\`\$`)"),
        cooked("`$", &[(0, 1, 3, 5), (1, 2, 5, 7)])
    );
}

#[test]
fn each_template_element_is_decoded_on_its_own() {
    let found = cook_all("f(`a${x}b`)", Grammar::JsModule, &generous());
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].result, cooked("a", &[(0, 1, 3, 4)]));
    assert_eq!(found[1].result, cooked("b", &[(0, 1, 8, 9)]));
}

#[test]
fn an_empty_literal_still_says_where_its_text_came_from() {
    assert_eq!(cook_one("f('')"), cooked("", &[(0, 0, 3, 3)]));
    assert_eq!(cook_one("f(``)"), cooked("", &[(0, 0, 3, 3)]));
}

#[test]
fn a_surrogate_is_accepted_only_as_two_adjacent_four_digit_escapes() {
    for (source, start, end) in [
        (r"f('\uD800')", 3, 9),
        (r"f('\uDC00')", 3, 9),
        (r"f('x\uD83Dy')", 4, 10),
        (r"f('\uDE00\uD83D')", 3, 9),
        (r"f('\uD83D\uD83D\uDE00')", 3, 9),
        (r"f('\u{D83D}')", 3, 11),
        (r"f(`\uDC00`)", 3, 9),
    ] {
        assert_eq!(
            cook_one(source),
            unsupported(Unsupported::SurrogateEscape, start, end),
            "{source}"
        );
    }
}

#[test]
fn the_parser_does_not_pair_the_spellings_the_profile_refuses() {
    // JavaScript pairs these, and the pinned parser does not. The profile
    // refuses them for that reason, so if a parser upgrade starts pairing
    // them this fails and the refusal is reconsidered rather than kept by
    // accident.
    for source in [
        r"f('\uD83D\u{DE00}')",
        r"f('\u{D83D}\uDE00')",
        "f('\\uD83D\\\n\\uDE00')",
    ] {
        let found = cook_all(source, Grammar::JsModule, &generous());
        assert_eq!(found.len(), 1);
        assert!(found[0].parser_lone_surrogates, "{source}");
        assert!(
            matches!(
                found[0].result,
                Err(CookFailure::Unsupported(Unsupported::SurrogateEscape, _))
            ),
            "{source}"
        );
    }
}

#[test]
fn legacy_escapes_are_refused_where_sloppy_code_admits_them() {
    for (source, start, end) in [
        (r"f('\101')", 3, 7),
        (r"f('\12x')", 3, 6),
        (r"f('\1')", 3, 5),
        (r"f('\47')", 3, 6),
        (r"f('\400')", 3, 6),
        (r"f('\08')", 3, 5),
        (r"f('\8')", 3, 5),
        (r"f('\9')", 3, 5),
    ] {
        assert_eq!(
            cook_one_as(source, Grammar::JsScript),
            unsupported(Unsupported::LegacyEscape, start, end),
            "{source}"
        );
    }
    assert_eq!(
        cook_one_as(r"f('a\0b')", Grammar::JsScript),
        cooked("a\0b", &[(0, 1, 3, 4), (1, 2, 4, 6), (2, 3, 6, 7)]),
        "a null escape before a non-digit is not legacy"
    );
}

#[test]
fn a_tagged_template_keeps_invalid_escapes_and_they_are_refused() {
    for (source, start, end) in [
        (r"t`\u{`", 2, 5),
        (r"t`\u{41`", 2, 7),
        (r"t`\u{110000}`", 2, 11),
        (r"t`\u12`", 2, 6),
        (r"t`\ux`", 2, 4),
        (r"t`\x4`", 2, 5),
        (r"t`\xg`", 2, 4),
        (r"t`\01`", 2, 5),
        (r"t`\1`", 2, 4),
    ] {
        assert_eq!(
            cook_one(source),
            unsupported(Unsupported::TemplateEscapeInvalid, start, end),
            "{source}"
        );
    }
}

#[test]
fn the_first_refused_form_in_source_order_is_the_one_reported() {
    assert_eq!(
        cook_one_as(r"f('\101\uD800')", Grammar::JsScript),
        unsupported(Unsupported::LegacyEscape, 3, 7)
    );
}

#[test]
fn input_segments_are_bounded_exactly() {
    // `a`, the escape, and `b` are three runs.
    let mut limits = generous();
    limits.input_segments = 3;
    let at_limit = cook_all("f('a\\nb')", Grammar::JsModule, &limits);
    assert!(at_limit[0].result.is_ok());
    limits.input_segments = 2;
    let over = cook_all("f('a\\nb')", Grammar::JsModule, &limits);
    assert_eq!(
        over[0].result,
        Err(CookFailure::Limit(JsLimitKind::InputSegments))
    );
    // The empty literal's one run counts too.
    limits.input_segments = 1;
    assert!(cook_all("f('')", Grammar::JsModule, &limits)[0]
        .result
        .is_ok());
}

/// Copies of every literal node, so a test can alter what the parser said.
struct Nodes<'a> {
    strings: Vec<StringLiteral<'a>>,
    elements: Vec<TemplateElement<'a>>,
}

impl<'a> Visit<'a> for Nodes<'a> {
    fn visit_string_literal(&mut self, literal: &StringLiteral<'a>) {
        self.strings.push(literal.clone());
    }

    fn visit_template_element(&mut self, element: &TemplateElement<'a>) {
        self.elements.push(element.clone());
    }
}

#[test]
fn a_disagreement_with_the_parser_stops_instead_of_choosing() {
    let allocator = Allocator::default();
    let source = "f('Pay now'); t`Pay now`; t`\\u{`";
    let parsed = Parser::new(&allocator, source, Grammar::JsModule.source_type()).parse();
    assert!(parsed.diagnostics.is_empty());

    let mut nodes = Nodes {
        strings: Vec::new(),
        elements: Vec::new(),
    };
    nodes.visit_program(&parsed.program);
    let limits = generous();
    let literal = &nodes.strings[0];
    assert!(cook_string(source, literal, &limits).is_ok());

    let mut tampered = literal.clone();
    tampered.value = "Pay later".into();
    assert_eq!(
        cook_string(source, &tampered, &limits),
        Err(CookFailure::Disagreement),
        "a different cooked value"
    );
    let mut tampered = literal.clone();
    tampered.lone_surrogates = true;
    assert_eq!(
        cook_string(source, &tampered, &limits),
        Err(CookFailure::Disagreement),
        "a lone surrogate the decoder did not find"
    );
    let mut tampered = literal.clone();
    tampered.span.start += 1;
    assert_eq!(
        cook_string(source, &tampered, &limits),
        Err(CookFailure::Disagreement),
        "a span that is not a whole quoted token"
    );
    // The same span over bytes a string cannot hold: a conforming parser
    // never accepts `\xg`, so meeting one is a disagreement, not a form.
    assert_eq!(
        cook_string("f('\\xg now')", literal, &limits),
        Err(CookFailure::Disagreement)
    );

    let element = &nodes.elements[0];
    assert!(cook_template(source, element, &limits).is_ok());
    let mut tampered = element.clone();
    tampered.value.cooked = None;
    assert_eq!(
        cook_template(source, &tampered, &limits),
        Err(CookFailure::Disagreement),
        "no cooked value for text the decoder could read"
    );
    let invalid = &nodes.elements[1];
    assert!(matches!(
        cook_template(source, invalid, &limits),
        Err(CookFailure::Unsupported(
            Unsupported::TemplateEscapeInvalid,
            _
        ))
    ));
    let mut tampered = invalid.clone();
    tampered.value.cooked = Some("u{".into());
    assert_eq!(
        cook_template(source, &tampered, &limits),
        Err(CookFailure::Disagreement),
        "a cooked value for an escape the language does not define"
    );
}

#[test]
fn every_decoded_map_is_one_the_shared_crate_accepts() {
    for source in [
        "f('Pay now')",
        "f('a\\\r\nb')",
        "f(`\r\r\n\n`)",
        r"f('\x41\u00e9\uD83D\uDE00\u{1F600}')",
        "f('')",
        "f('\\\n')",
        "f(`a${x}b`)",
        "mf2`one\r\ntwo\rthree\nfour`",
    ] {
        for found in cook_all(source, Grammar::JsModule, &generous()) {
            let cooked = found.result.expect("a decodable literal");
            assert_accepted(source, found.span, &cooked);
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    #[allow(dead_code)]
    note: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Case {
    id: String,
    file: String,
    grammar: String,
    byte_length: u64,
    utf8_digest: String,
    literals: Vec<Expected>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Expected {
    span: (u32, u32),
    text: Option<String>,
    input_map: Option<Vec<(u64, u64, u64, u64)>>,
    unsupported: Option<String>,
    at: Option<(u64, u64)>,
}

#[test]
fn extraction_fixtures_decode_to_their_hand_derived_text_and_map() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/phase2");
    let manifest: Manifest = serde_json::from_str(
        &std::fs::read_to_string(root.join("extraction.json")).expect("the manifest"),
    )
    .expect("a well-formed manifest");
    assert!(!manifest.cases.is_empty());

    for case in manifest.cases {
        let bytes = std::fs::read(root.join(&case.file)).expect("the fixture file");
        let grammar = Grammar::from_identity(&VersionedIdentity::new(&case.grammar, "0").unwrap())
            .expect("a registered grammar");
        // A rewritten line ending or byte order mark changes the digest, so
        // the case fails here instead of passing against other bytes.
        let snapshot = SourceSnapshot::new(
            OwnerIdentity::new(OwnerKind::Application, "storefront").unwrap(),
            &case.id,
            "1",
            grammar.identity(),
            case.byte_length,
            &case.utf8_digest,
        )
        .unwrap();
        let source = snapshot.verify(&bytes).unwrap_or_else(|mismatch| {
            panic!(
                "{}: the file is not the declared bytes ({mismatch:?})",
                case.id
            )
        });

        let found = cook_all(source, grammar, &generous());
        assert_eq!(
            found.len(),
            case.literals.len(),
            "{}: every literal is listed",
            case.id
        );
        for (found, expected) in found.iter().zip(&case.literals) {
            assert_eq!(found.span, expected.span, "{}: literals in order", case.id);
            let want = match (
                &expected.text,
                &expected.input_map,
                &expected.unsupported,
                expected.at,
            ) {
                (Some(text), Some(map), None, None) => cooked(text, map),
                (None, None, Some(form), Some((start, end))) => {
                    let form = [
                        Unsupported::SurrogateEscape,
                        Unsupported::TemplateEscapeInvalid,
                        Unsupported::LegacyEscape,
                    ]
                    .into_iter()
                    .find(|candidate| candidate.as_str() == form)
                    .expect("a known form");
                    unsupported(form, start, end)
                }
                _ => panic!("{}: a literal is either decoded or refused", case.id),
            };
            assert_eq!(
                found.result, want,
                "{}: literal at {:?}",
                case.id, found.span
            );
            if let Ok(cooked) = &found.result {
                assert_accepted(source, found.span, cooked);
            }
        }
    }
}
