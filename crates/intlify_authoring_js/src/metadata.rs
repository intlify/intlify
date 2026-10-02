// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! `@intlify` metadata annotations.
//!
//! An annotation is a block comment whose text starts with `@intlify`,
//! followed by one JSON object. It supplies a declaration's source locale,
//! surface class and description without adding an argument to any authoring
//! form, so the same comment means the same thing above ordinary UI text, an
//! `intent()` message and an `mf2` declaration.
//!
//! Reading one has two halves, and each reports its own mistakes. The comment
//! is first checked on its own: it has to fit its bound, decode as one JSON
//! object naming no member twice, name only `sourceLocale`, `surfaceClass`
//! and `description`, and give each a nonempty string. What a value means,
//! such as whether a locale canonicalizes or a class is in the vocabulary, is
//! decided by `intlify_authoring`, as it is for a value from anywhere else.
//!
//! Then it is placed. An annotation belongs to the statement right after it
//! in the same statement list, and that statement has to declare exactly one
//! message of its own. A declaration is a statement's own when that statement
//! is the innermost list member around it and no function or class lies in
//! between. So an annotation never describes a whole function or block, and
//! never looks past the next statement for something to describe. An
//! annotation that cannot be placed, or one of two before the same
//! statement, describes nothing.
//!
//! A reported annotation that does belong to one declaration keeps that
//! declaration from being established. What the author wrote about it is not
//! known, and a default is no stand-in for it.

use std::collections::{BTreeMap, BTreeSet};

use intlify_authoring::{Detail, Occurrence, ReasonFamily};
use intlify_shared_json::json::{decode_unique_json, JsonDecodeErrorKind};
use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{
    ArrowFunctionExpression, BlockStatement, Class, Directive, Function, FunctionBody, Program,
    Statement, StaticBlock, SwitchCase, TSModuleBlock, TSModuleDeclaration,
};
use oxc_ast::{AstKind, Comment};
use oxc_ast_visit::{walk, Visit};
use oxc_semantic::ScopeFlags;
use oxc_span::{GetSpan, Span};
use serde_json::Value;

use crate::detail;
use crate::explicit::Recognized;
use crate::failure::ProducerFailure;
use crate::limits::{JsAuthoringLimits, JsLimitKind};
use crate::parse::Parsed;
use crate::report::Reporter;

/// How many nodes the walk enters between two cancellation probes.
const PROBE_INTERVAL: u32 = 1024;

/// The word an annotation starts with.
const MARKER: &str = "@intlify";

type Key = (u32, u32);

const fn key(span: Span) -> Key {
    (span.start, span.end)
}

/// The values one valid annotation supplies, each still as written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Values {
    pub(crate) source_locale: Option<String>,
    pub(crate) surface_class: Option<String>,
    pub(crate) description: Option<String>,
}

/// What an annotation says about one declaration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum Annotation {
    /// No annotation belongs to it.
    #[default]
    Absent,
    /// The one annotation that belongs to it is valid.
    Valid(Values),
    /// An annotation belongs to it but was reported, so it is not
    /// established.
    Rejected,
}

/// What one comment is to this profile.
#[derive(Debug, PartialEq, Eq)]
enum Reading<'t> {
    /// An annotation, with the text that has to be one JSON object.
    Annotation(&'t str),
    /// A comment naming `@intlify` in a form this profile does not read.
    Misspelled,
    Ordinary,
}

/// Classify a comment by its text between the delimiters.
///
/// Only a block comment starting with the marker is an annotation. A line
/// comment or a documentation comment that would be one once its `//` or
/// leading `*` were removed is a misspelling, which is reported rather than
/// lost.
fn classify(content: &str, block: bool) -> Reading<'_> {
    if block {
        if let Some(rest) = marked(content.trim_start()) {
            return Reading::Annotation(rest);
        }
    }
    let unadorned = content.trim_start_matches(|c: char| c.is_whitespace() || c == '*' || c == '/');
    if marked(unadorned).is_some() {
        Reading::Misspelled
    } else {
        Reading::Ordinary
    }
}

/// Return what follows the marker, when `text` starts with it as a word.
fn marked(text: &str) -> Option<&str> {
    let rest = text.strip_prefix(MARKER)?;
    match rest.chars().next() {
        Some(next) if !next.is_whitespace() && next != '{' => None,
        _ => Some(rest),
    }
}

fn read<'t>(comment: &Comment, text: &'t str) -> Reading<'t> {
    let content = comment.content_span();
    classify(
        &text[content.start as usize..content.end as usize],
        comment.is_block(),
    )
}

/// Decode and check what follows the marker.
///
/// Members are checked by name, so which problem is reported first does not
/// depend on how the JSON decoder orders an object.
fn values(json: &str) -> Result<Values, Detail> {
    let decoded = decode_unique_json(json).map_err(|failure| match failure.kind() {
        JsonDecodeErrorKind::DuplicateObjectMember => detail::metadata_member_duplicate(),
        JsonDecodeErrorKind::Syntax => detail::metadata_json_malformed(),
    })?;
    let Value::Object(members) = decoded else {
        return Err(detail::metadata_not_object());
    };
    let members: BTreeMap<&str, &Value> = members
        .iter()
        .map(|(name, value)| (name.as_str(), value))
        .collect();
    let mut found = Values::default();
    for (name, value) in members {
        let slot = match name {
            "sourceLocale" => &mut found.source_locale,
            "surfaceClass" => &mut found.surface_class,
            "description" => &mut found.description,
            _ => return Err(detail::metadata_member_unknown()),
        };
        let Value::String(value) = value else {
            return Err(detail::metadata_value_not_string());
        };
        if value.is_empty() {
            return Err(detail::metadata_value_empty());
        }
        *slot = Some(value.clone());
    }
    Ok(found)
}

/// Read the unit's annotations and attach each to the declaration it
/// describes.
///
/// It runs after every recognizer, since an annotation describes what they
/// declared. When the statement after an annotation declares nothing because
/// what it holds was already reported, no second record says so.
pub(crate) fn attach<C>(
    parsed: &Parsed<'_>,
    text: &str,
    recognized: &mut Recognized,
    limits: &JsAuthoringLimits,
    reporter: &mut Reporter,
    cancelled: &C,
) -> Result<(), ProducerFailure>
where
    C: Fn() -> bool + ?Sized,
{
    let reported = reporter.ranges();
    let mut annotations = Vec::new();
    for comment in &parsed.program.comments {
        match read(comment, text) {
            Reading::Ordinary => {}
            Reading::Misspelled => reporter.at(
                ReasonFamily::AuthoringMetadataInvalid,
                detail::metadata_comment_form(),
                comment.span,
            )?,
            Reading::Annotation(json) => annotations.push((comment.span, json)),
        }
    }
    if annotations.is_empty() {
        return Ok(());
    }

    let mut checked = Vec::with_capacity(annotations.len());
    for &(span, json) in &annotations {
        if u64::from(span.size()) > limits.annotation_bytes {
            reporter.at(
                ReasonFamily::AuthoringResourceLimit,
                detail::limit(JsLimitKind::AnnotationBytes),
                span,
            )?;
            checked.push(None);
            continue;
        }
        match values(json) {
            Ok(values) => checked.push(Some(values)),
            Err(detail) => {
                reporter.at(ReasonFamily::AuthoringMetadataInvalid, detail, span)?;
                checked.push(None);
            }
        }
    }

    let declarations = by_range(
        recognized
            .declarations
            .iter()
            .map(|declared| &declared.occurrence),
    );
    // Only a use of a shared declaration is a use of something declared
    // elsewhere; an inline use is its own declaration's.
    let shared = by_range(
        recognized
            .uses
            .iter()
            .filter(|used| used.shared)
            .map(|used| &used.occurrence),
    );
    let mut placer = Placer {
        openings: Vec::new(),
        frames: Vec::new(),
        gaps: Vec::new(),
        declarations: &declarations,
        shared: &shared,
        declared_in: vec![None; recognized.declarations.len()],
        used_in: Vec::new(),
        visited: 0,
        cancelled,
        failure: None,
    };
    placer.visit_program(parsed.program);
    if let Some(failure) = placer.failure {
        return Err(failure);
    }
    let mut gaps = placer.gaps;
    gaps.sort_unstable();

    let mut before: BTreeMap<Key, Vec<usize>> = BTreeMap::new();
    for (index, &(span, _)) in annotations.iter().enumerate() {
        let gap = gaps
            .partition_point(|gap| gap.0 <= span.start)
            .checked_sub(1)
            .map(|at| gaps[at])
            .filter(|gap| span.end <= gap.1);
        match gap {
            Some((_, _, statement)) => before.entry(statement).or_default().push(index),
            None => reporter.at(
                ReasonFamily::AuthoringMetadataInvalid,
                detail::metadata_misplaced(),
                span,
            )?,
        }
    }

    let mut own: BTreeMap<Key, Vec<usize>> = BTreeMap::new();
    for (index, statement) in placer.declared_in.iter().enumerate() {
        if let Some(statement) = statement {
            own.entry(*statement).or_default().push(index);
        }
    }
    let using: BTreeSet<Key> = placer.used_in.into_iter().collect();
    for (statement, group) in before {
        let candidates = own.get(&statement).map_or(&[][..], Vec::as_slice);
        if let [index] = group[..] {
            let span = annotations[index].0;
            let failed = match candidates {
                [] if using.contains(&statement) => Some(detail::metadata_target_reference()),
                [] if reported_within(&reported, statement) => None,
                [] => Some(detail::metadata_target_absent()),
                [only] => {
                    recognized.declarations[*only].annotation = match checked[index].take() {
                        Some(values) => Annotation::Valid(values),
                        None => Annotation::Rejected,
                    };
                    None
                }
                _ => Some(detail::metadata_target_ambiguous()),
            };
            if let Some(detail) = failed {
                reporter.at(ReasonFamily::AuthoringMetadataInvalid, detail, span)?;
            }
            continue;
        }
        for &index in &group {
            reporter.at(
                ReasonFamily::AuthoringMetadataInvalid,
                detail::metadata_repeated(),
                annotations[index].0,
            )?;
        }
        if let [only] = candidates {
            recognized.declarations[*only].annotation = Annotation::Rejected;
        }
    }
    Ok(())
}

/// Index occurrences by the span they were made from.
fn by_range<'o>(occurrences: impl Iterator<Item = &'o Occurrence>) -> BTreeMap<Key, usize> {
    occurrences
        .enumerate()
        .filter_map(|(index, occurrence)| {
            let range = occurrence.range();
            let start = u32::try_from(range.start()).ok()?;
            let end = u32::try_from(range.end()).ok()?;
            Some(((start, end), index))
        })
        .collect()
}

/// Return whether some earlier record points inside `statement`.
fn reported_within(reported: &[(u64, u64)], statement: Key) -> bool {
    reported
        .iter()
        .any(|&(start, end)| u64::from(statement.0) <= start && end <= u64::from(statement.1))
}

/// Where one statement list starts, and the directives it opens with.
struct Opening {
    start: u32,
    directives: Vec<Key>,
}

/// What the walk is inside of.
#[derive(Debug, Clone, Copy)]
enum Frame {
    /// A member of a statement list.
    Member(Key),
    /// A function, class or namespace, which is a scope of its own.
    Barrier,
}

/// Finds where annotations can be and which statement owns each
/// declaration.
struct Placer<'p, C: ?Sized> {
    /// The opening of each statement list about to be read.
    ///
    /// An arrow's expression body is a list of one statement that spans the
    /// expression, so no annotation can come before it.
    openings: Vec<Opening>,
    frames: Vec<Frame>,
    /// Each place an annotation can be: the stretch of source before a list
    /// member, after the one before it, and that member.
    gaps: Vec<(u32, u32, Key)>,
    declarations: &'p BTreeMap<Key, usize>,
    shared: &'p BTreeMap<Key, usize>,
    /// The statement that owns each declaration, if any does.
    declared_in: Vec<Option<Key>>,
    /// The statements that own a use of a shared declaration.
    used_in: Vec<Key>,
    visited: u32,
    cancelled: &'p C,
    failure: Option<ProducerFailure>,
}

impl<C> Placer<'_, C>
where
    C: Fn() -> bool + ?Sized,
{
    fn open(&mut self, start: u32, directives: &[Directive<'_>]) {
        self.openings.push(Opening {
            start,
            directives: directives.iter().map(|found| key(found.span)).collect(),
        });
    }

    fn within(&mut self, walk: impl FnOnce(&mut Self)) {
        self.frames.push(Frame::Barrier);
        walk(self);
        self.frames.pop();
    }
}

impl<'a, C> Visit<'a> for Placer<'_, C>
where
    C: Fn() -> bool + ?Sized,
{
    fn enter_node(&mut self, kind: AstKind<'a>) {
        self.visited += 1;
        if self.visited.is_multiple_of(PROBE_INTERVAL)
            && self.failure.is_none()
            && (self.cancelled)()
        {
            self.failure = Some(ProducerFailure::Cancelled);
        }
        let at = key(kind.span());
        let owner = match self.frames.last() {
            Some(Frame::Member(statement)) => Some(*statement),
            _ => None,
        };
        if let Some(&index) = self.declarations.get(&at) {
            self.declared_in[index] = owner;
        }
        if let (Some(_), Some(statement)) = (self.shared.get(&at), owner) {
            self.used_in.push(statement);
        }
    }

    fn visit_program(&mut self, program: &Program<'a>) {
        // No comment can come before a hashbang, so where the program starts
        // is where its first gap does.
        self.open(program.span.start, &program.directives);
        walk::walk_program(self, program);
    }

    fn visit_block_statement(&mut self, block: &BlockStatement<'a>) {
        self.open(block.span.start, &[]);
        walk::walk_block_statement(self, block);
    }

    fn visit_function_body(&mut self, body: &FunctionBody<'a>) {
        self.open(body.span.start, &body.directives);
        walk::walk_function_body(self, body);
    }

    fn visit_static_block(&mut self, block: &StaticBlock<'a>) {
        self.open(block.span.start, &[]);
        walk::walk_static_block(self, block);
    }

    fn visit_switch_case(&mut self, case: &SwitchCase<'a>) {
        let start = case
            .test
            .as_ref()
            .map_or(case.span.start, |test| test.span().end);
        self.open(start, &[]);
        walk::walk_switch_case(self, case);
    }

    fn visit_ts_module_block(&mut self, block: &TSModuleBlock<'a>) {
        self.open(block.span.start, &block.directives);
        walk::walk_ts_module_block(self, block);
    }

    fn visit_statements(&mut self, statements: &ArenaVec<'a, Statement<'a>>) {
        let Some(opening) = self.openings.pop() else {
            walk::walk_statements(self, statements);
            return;
        };
        let mut previous = opening.start;
        for directive in opening.directives {
            self.gaps.push((previous, directive.0, directive));
            previous = directive.1;
        }
        for statement in statements {
            if self.failure.is_some() {
                return;
            }
            let at = key(statement.span());
            self.gaps.push((previous, at.0, at));
            previous = at.1;
            self.frames.push(Frame::Member(at));
            self.visit_statement(statement);
            self.frames.pop();
        }
    }

    fn visit_function(&mut self, function: &Function<'a>, flags: ScopeFlags) {
        self.within(|placer| walk::walk_function(placer, function, flags));
    }

    fn visit_arrow_function_expression(&mut self, arrow: &ArrowFunctionExpression<'a>) {
        self.within(|placer| walk::walk_arrow_function_expression(placer, arrow));
    }

    fn visit_class(&mut self, class: &Class<'a>) {
        self.within(|placer| walk::walk_class(placer, class));
    }

    fn visit_ts_module_declaration(&mut self, module: &TSModuleDeclaration<'a>) {
        self.within(|placer| walk::walk_ts_module_declaration(placer, module));
    }
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;

    use super::*;
    use crate::grammar::Grammar;
    use crate::limits::tests::generous;
    use crate::test_support::{parse, reporter};

    #[test]
    fn a_probe_asking_to_stop_stops_the_placement() {
        // Each empty statement is a node the walk enters, so it reaches a
        // probe long before the end.
        let text = format!("/* @intlify {{}} */\n{}", ";".repeat(2048));
        let allocator = Allocator::default();
        let parsed = parse(&allocator, &text, Grammar::JsModule);
        assert_eq!(
            attach(
                &parsed,
                &text,
                &mut Recognized::default(),
                &generous(),
                &mut reporter(&text),
                &|| true,
            ),
            Err(ProducerFailure::Cancelled)
        );
        // With no annotation there is nothing to place, and nothing is walked.
        let quiet = ";".repeat(2048);
        let parsed = parse(&allocator, &quiet, Grammar::JsModule);
        assert_eq!(
            attach(
                &parsed,
                &quiet,
                &mut Recognized::default(),
                &generous(),
                &mut reporter(&quiet),
                &|| true,
            ),
            Ok(())
        );
    }

    #[test]
    fn only_a_block_comment_starting_with_the_marker_is_an_annotation() {
        for (content, block, expected) in [
            (" @intlify {} ", true, Reading::Annotation(" {} ")),
            ("@intlify{}", true, Reading::Annotation("{}")),
            (
                "\n  @intlify\n  {}\n",
                true,
                Reading::Annotation("\n  {}\n"),
            ),
            (" @intlify", true, Reading::Annotation("")),
            (" @intlify {} ", false, Reading::Misspelled),
            ("/ @intlify {}", false, Reading::Misspelled),
            ("* @intlify {} ", true, Reading::Misspelled),
            ("*\n * @intlify {}\n ", true, Reading::Misspelled),
            (" @intlifyish {} ", true, Reading::Ordinary),
            (" see @intlify ", true, Reading::Ordinary),
            (" an ordinary note", false, Reading::Ordinary),
            ("", true, Reading::Ordinary),
        ] {
            assert_eq!(classify(content, block), expected, "{content:?}");
        }
    }

    #[test]
    fn an_annotation_is_one_object_of_nonempty_strings_under_three_names() {
        assert_eq!(
            values(r#" { "sourceLocale": "en", "surfaceClass": "nav", "description": "Pay" } "#),
            Ok(Values {
                source_locale: Some("en".to_owned()),
                surface_class: Some("nav".to_owned()),
                description: Some("Pay".to_owned()),
            })
        );
        assert_eq!(values("{}"), Ok(Values::default()));
        for (json, expected) in [
            ("", detail::metadata_json_malformed()),
            (r#"{ "description": }"#, detail::metadata_json_malformed()),
            (
                r#"{ "description": "Pay" } more"#,
                detail::metadata_json_malformed(),
            ),
            (
                "{ \"description\": \"\x5cuD800\" }",
                detail::metadata_json_malformed(),
            ),
            (r#"["Pay"]"#, detail::metadata_not_object()),
            (r#""Pay""#, detail::metadata_not_object()),
            (
                r#"{ "description": "a", "description": "b" }"#,
                detail::metadata_member_duplicate(),
            ),
            (
                r#"{ "descripton": "Pay" }"#,
                detail::metadata_member_unknown(),
            ),
            (
                r#"{ "description": 1 }"#,
                detail::metadata_value_not_string(),
            ),
            (
                r#"{ "description": null }"#,
                detail::metadata_value_not_string(),
            ),
            (
                r#"{ "description": ["Pay"] }"#,
                detail::metadata_value_not_string(),
            ),
            (r#"{ "surfaceClass": "" }"#, detail::metadata_value_empty()),
        ] {
            assert_eq!(values(json), Err(expected), "{json}");
        }
        // Problems are taken by member name, whatever order they are written
        // in, so the same annotation always reports the same one.
        for json in [
            r#"{ "zzz": "x", "description": "" }"#,
            r#"{ "description": "", "zzz": "x" }"#,
        ] {
            assert_eq!(values(json), Err(detail::metadata_value_empty()), "{json}");
        }
    }
}
