// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The shared MF2 analysis every authoring form goes through.
//!
//! Literal and explicitly authored messages differ only in how their MF2 source
//! is obtained. Once obtained, both take exactly the same path, so the two
//! forms cannot acquire divergent message semantics.

use ox_mf2_parser::{
    build_semantic_model, parse_message, validate_semantics, DeclarationKind, DiagnosticSeverity,
    SemanticModel,
};

use super::literal::{self, ExtractionSegment, LiteralFailure};
use crate::diagnostic::{Diagnostic, DiagnosticOrigin, MessageRange, Severity, Stage};
use crate::limits::{AuthoringLimits, LimitKind};
use crate::primitives::{ByteRange, Occurrence};
use crate::projection::build::{Builder, ProjectionFailure};
use crate::projection::MessageProjection;
use crate::workspace::AnalysisWorkspace;

/// One already host-decoded message, in the form its authoring site used.
///
/// Host escape decoding has already happened. An inline literal and a reusable
/// tagged declaration that cook to the same content arrive here identically;
/// their different host spellings are source evidence, not different messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageInput<'a> {
    /// Displayed text with literal semantics, such as recognized UI text.
    Literal(&'a str),
    /// Explicitly authored MF2 source.
    Mf2(&'a str),
}

impl<'a> MessageInput<'a> {
    /// Borrow the supplied text, whichever form it was authored in.
    #[must_use]
    pub const fn text(self) -> &'a str {
        match self {
            Self::Literal(text) | Self::Mf2(text) => text,
        }
    }
}

/// Complete failure of a message analysis.
///
/// These are operational and input failures, not ordinary diagnostics. A
/// malformed authored message is reported through diagnostics instead, because
/// the caller can act on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageFailure {
    /// A named bound was exhausted.
    Limit(LimitKind),
    /// Displayed text contains a scalar MF2 pattern text cannot represent.
    UnrepresentableScalar { offset: u64 },
    /// The parser could not run within its own resources.
    ParserResource,
    /// A parser invariant failed, or literal encoding produced a message the
    /// parser rejected. Neither is an ordinary malformed-message diagnostic.
    ParserInvariant,
    /// Literal encoding did not round-trip to the exact displayed text.
    LiteralRoundTrip,
}

impl From<ProjectionFailure> for MessageFailure {
    fn from(failure: ProjectionFailure) -> Self {
        match failure {
            ProjectionFailure::Limit(kind) => Self::Limit(kind),
            // The parse was diagnostic-free and semantically valid, so a shape
            // the projection walker cannot read is a parser change or a defect
            // here, never an authoring mistake the caller could fix.
            ProjectionFailure::UnsupportedStructure => Self::ParserInvariant,
        }
    }
}

impl From<LiteralFailure> for MessageFailure {
    fn from(failure: LiteralFailure) -> Self {
        match failure {
            LiteralFailure::Limit(kind) => Self::Limit(kind),
            LiteralFailure::UnrepresentableScalar { offset } => {
                Self::UnrepresentableScalar { offset }
            }
        }
    }
}

/// Parser-validated facts for one message.
///
/// Facts exist only when parsing and parser-owned semantic validation both
/// succeeded. A message with syntax or semantic diagnostics has no facts, so
/// dependent work cannot silently proceed from a partially understood message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageFacts {
    parameters: Box<[String]>,
    message: MessageProjection,
}

impl MessageFacts {
    /// Return the external parameter names this message requires.
    ///
    /// The set is duplicate-free and sorted by unsigned UTF-8 bytes. Names are
    /// the parser's semantic names without sigils. MF2-local declarations are
    /// not caller parameters and are absent.
    #[must_use]
    pub fn parameters(&self) -> &[String] {
        &self.parameters
    }

    /// Borrow the structured message this projection is computed from.
    ///
    /// This is the message half of an Intent projection. Combining it with the
    /// resolved source locale, usage, and description produces the complete
    /// projection a revision is taken over.
    #[must_use]
    pub const fn message(&self) -> &MessageProjection {
        &self.message
    }
}

/// The complete result of analyzing one message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageAnalysis {
    mf2_source: String,
    extraction_map: Box<[ExtractionSegment]>,
    facts: Option<MessageFacts>,
    diagnostics: Box<[Diagnostic]>,
}

impl MessageAnalysis {
    /// Borrow the exact MF2 source that was analyzed.
    ///
    /// For displayed text this is the encoded quoted pattern; for authored MF2
    /// it is the supplied source unchanged.
    #[must_use]
    pub fn mf2_source(&self) -> &str {
        &self.mf2_source
    }

    /// Return the mapping from emitted MF2 bytes back to the supplied text.
    #[must_use]
    pub fn extraction_map(&self) -> &[ExtractionSegment] {
        &self.extraction_map
    }

    /// Return the parser-validated facts, when the message was understood.
    #[must_use]
    pub const fn facts(&self) -> Option<&MessageFacts> {
        self.facts.as_ref()
    }

    /// Return the diagnostics in deterministic reporting order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Return whether any retained diagnostic blocks a checked result.
    #[must_use]
    pub fn is_blocked(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_blocking)
    }
}

/// Analyze one message under explicit bounds, reusing `workspace` scratch.
///
/// `occurrence` locates the declaration the message belongs to and is used for
/// diagnostics; this function does not verify it against actual source bytes.
pub fn analyze_message(
    input: MessageInput<'_>,
    occurrence: &Occurrence,
    limits: &AuthoringLimits,
    workspace: &mut AnalysisWorkspace,
) -> Result<MessageAnalysis, MessageFailure> {
    workspace.clear();

    let literal_text = match input {
        MessageInput::Literal(text) => Some(text),
        MessageInput::Mf2(_) => None,
    };
    let mf2_source = match input {
        MessageInput::Literal(text) => literal::encode(text, limits, &mut workspace.segments)?,
        MessageInput::Mf2(text) => {
            if text.len() as u64 > limits.message_text_bytes {
                return Err(MessageFailure::Limit(LimitKind::MessageTextBytes));
            }
            if text.len() as u64 > limits.emitted_mf2_bytes {
                return Err(MessageFailure::Limit(LimitKind::EmittedMf2Bytes));
            }
            // Authored MF2 is already its own extracted form, so the mapping
            // is one identity segment rather than an empty map. An empty map
            // would claim that no emitted byte has a source, and every later
            // reader requires the segments to cover the emitted message.
            workspace.segments.clear();
            if limits.extraction_segments == 0 {
                return Err(MessageFailure::Limit(LimitKind::ExtractionSegments));
            }
            workspace
                .segments
                .push(literal::identity_segment(text.len() as u64)?);
            let mut owned = String::new();
            owned.reserve(text.len());
            owned.push_str(text);
            owned
        }
    };

    if let Some(text) = literal_text {
        // The encoder must be exactly invertible before anything depends on it.
        if literal::decode_round_trip(&mf2_source).as_deref() != Some(text) {
            return Err(MessageFailure::LiteralRoundTrip);
        }
    }

    let parsed = parse_message(&mf2_source).map_err(|_| MessageFailure::ParserResource)?;
    let result = parsed.result();

    if !result.diagnostics.is_empty() {
        // Encoded displayed text is generated, so the parser rejecting it is an
        // encoder defect rather than an authoring mistake the caller can fix.
        if literal_text.is_some() {
            return Err(MessageFailure::ParserInvariant);
        }
        for record in &result.diagnostics {
            push_parser_diagnostic(
                workspace,
                limits,
                occurrence,
                DiagnosticOrigin::Mf2Syntax(record.code.json_code()),
                record.severity,
                record.span,
            )?;
        }
        return Ok(finish(mf2_source, None, workspace));
    }

    let model = build_semantic_model(parsed.sources(), result)
        .map_err(|_| MessageFailure::ParserInvariant)?;
    let semantic = validate_semantics(&model).map_err(|_| MessageFailure::ParserInvariant)?;

    if !semantic.is_empty() {
        if literal_text.is_some() {
            return Err(MessageFailure::ParserInvariant);
        }
        for record in &semantic {
            push_parser_diagnostic(
                workspace,
                limits,
                occurrence,
                DiagnosticOrigin::Mf2Semantic(record.code().json_code()),
                record.severity(),
                record.span(),
            )?;
        }
        return Ok(finish(mf2_source, None, workspace));
    }

    collect_external_parameters(&model, limits, workspace)?;
    let parameters: Box<[String]> = workspace.names.drain(..).collect();
    let view = ox_mf2_parser::CstView::new(parsed.sources(), result.source, &result.cst);
    let message = Builder::new(view, limits).message()?;
    Ok(finish(
        mf2_source,
        Some(MessageFacts {
            parameters,
            message,
        }),
        workspace,
    ))
}

fn finish(
    mf2_source: String,
    facts: Option<MessageFacts>,
    workspace: &mut AnalysisWorkspace,
) -> MessageAnalysis {
    workspace.diagnostics.sort_by(Diagnostic::reporting_cmp);
    MessageAnalysis {
        mf2_source,
        // Draining transfers ownership to the result and keeps the workspace
        // capacity, so no retained value borrows resettable storage.
        extraction_map: workspace.segments.drain(..).collect(),
        facts,
        diagnostics: workspace.diagnostics.drain(..).collect(),
    }
}

fn push_parser_diagnostic(
    workspace: &mut AnalysisWorkspace,
    limits: &AuthoringLimits,
    occurrence: &Occurrence,
    origin: DiagnosticOrigin,
    severity: DiagnosticSeverity,
    span: ox_mf2_parser::Span,
) -> Result<(), MessageFailure> {
    if workspace.diagnostics.len() as u64 >= limits.diagnostics {
        return Err(MessageFailure::Limit(LimitKind::Diagnostics));
    }
    let severity = match severity {
        DiagnosticSeverity::Error => Severity::Error,
        DiagnosticSeverity::Warning => Severity::Warning,
        DiagnosticSeverity::Information | DiagnosticSeverity::Hint => Severity::Information,
    };
    let mut record = Diagnostic::new(Stage::MessageAnalysis, origin, severity, occurrence.clone());
    if let Ok(range) = ByteRange::new(u64::from(span.start), u64::from(span.end)) {
        record = record.with_message_range(MessageRange::Emitted(range));
    }
    workspace.diagnostics.push(record);
    Ok(())
}

/// Collect the external parameter names one message requires.
///
/// A name is external when the message declares it with `.input`, or when a
/// reference resolves to no declaration at all. A reference that resolves to an
/// MF2-local declaration is not a caller parameter.
fn collect_external_parameters(
    model: &SemanticModel,
    limits: &AuthoringLimits,
    workspace: &mut AnalysisWorkspace,
) -> Result<(), MessageFailure> {
    workspace.names.clear();
    let push = |name: &str, workspace: &mut AnalysisWorkspace| -> Result<(), MessageFailure> {
        if name.len() as u64 > limits.parameter_name_bytes {
            return Err(MessageFailure::Limit(LimitKind::ParameterNameBytes));
        }
        if workspace.names.iter().all(|existing| existing != name) {
            if workspace.names.len() as u64 >= limits.parameter_names {
                return Err(MessageFailure::Limit(LimitKind::ParameterNames));
            }
            workspace.names.push(name.to_owned());
        }
        Ok(())
    };

    for declaration in model.semantic_declarations() {
        if declaration.kind() == DeclarationKind::Input {
            push(declaration.name(), workspace)?;
        }
    }
    for reference in model.semantic_references() {
        if reference.resolved_declaration().is_none() {
            push(reference.name(), workspace)?;
        }
    }
    workspace
        .names
        .sort_unstable_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::tests::generous;
    use crate::primitives::{OccurrenceRole, OwnerIdentity, OwnerKind, SourceSnapshot};
    use intlify_shared_json::token::VersionedIdentity;

    fn occurrence(role: OccurrenceRole) -> Occurrence {
        let source = SourceSnapshot::new(
            OwnerIdentity::new(OwnerKind::Application, "storefront").unwrap(),
            "checkout",
            "1",
            VersionedIdentity::literal("intlify-js-grammar", "0"),
            4096,
            &format!("sha256:{}", "0".repeat(64)),
        )
        .unwrap();
        Occurrence::new(source, ByteRange::new(0, 16).unwrap(), role).unwrap()
    }

    fn analyze(input: MessageInput<'_>) -> Result<MessageAnalysis, MessageFailure> {
        let mut workspace = AnalysisWorkspace::new();
        analyze_message(
            input,
            &occurrence(OccurrenceRole::UiLiteral),
            &generous(),
            &mut workspace,
        )
    }

    fn parameters(input: MessageInput<'_>) -> Vec<String> {
        analyze(input)
            .unwrap()
            .facts()
            .expect("understood message")
            .parameters()
            .to_vec()
    }

    #[test]
    fn displayed_braces_stay_literal_instead_of_becoming_interpolation() {
        let analysis = analyze(MessageInput::Literal("Total: {amount}")).unwrap();
        assert_eq!(analysis.mf2_source(), "{{Total: \\{amount\\}}}");
        assert!(analysis.diagnostics().is_empty());
        assert_eq!(
            analysis.facts().expect("understood message").parameters(),
            [] as [String; 0]
        );
    }

    #[test]
    fn authored_mf2_keeps_its_source_and_declares_its_external_inputs() {
        assert_eq!(parameters(MessageInput::Mf2("Hello {$name}!")), ["name"]);
        assert_eq!(
            parameters(MessageInput::Mf2(
                ".input {$count :number}\n.match $count\none {{one}}\n* {{many}}"
            )),
            ["count"]
        );
        // Several uses of one name collapse to a single requirement.
        assert_eq!(
            parameters(MessageInput::Mf2("{$name} and {$name}")),
            ["name"]
        );
        // The set is sorted by unsigned UTF-8 bytes, not by appearance.
        assert_eq!(
            parameters(MessageInput::Mf2("{$zulu} {$alpha} {$Mike}")),
            ["Mike", "alpha", "zulu"]
        );
    }

    #[test]
    fn mf2_local_declarations_are_not_caller_parameters() {
        let source = ".local $greeting = {|hello|}\n{{{$greeting}}}";
        assert_eq!(parameters(MessageInput::Mf2(source)), [] as [String; 0]);
        // A local initialized from an external value still requires that value.
        let mixed = ".local $shown = {$raw}\n{{{$shown}}}";
        assert_eq!(parameters(MessageInput::Mf2(mixed)), ["raw"]);
    }

    #[test]
    fn authored_mf2_maps_to_itself_so_the_segments_cover_the_emitted_message() {
        let analysis = analyze(MessageInput::Mf2("Hello {$name}!")).unwrap();
        let segments = analysis.extraction_map();
        assert_eq!(segments.len(), 1);
        let only = segments[0];
        assert_eq!(only.extracted().start(), 0);
        assert_eq!(only.extracted().end(), analysis.mf2_source().len() as u64);
        assert_eq!(only.source(), only.extracted());

        // An empty authored message still maps its zero emitted bytes.
        let empty = analyze(MessageInput::Mf2("")).unwrap();
        assert_eq!(empty.extraction_map().len(), 1);
        assert!(empty.extraction_map()[0].extracted().is_empty());
    }

    #[test]
    fn the_quoted_pattern_covers_strings_a_simple_message_would_special_case() {
        // A leading period starts a declaration, so displayed text beginning
        // with one cannot be emitted as a bare simple message.
        let as_simple = analyze(MessageInput::Mf2(".input looks like a keyword")).unwrap();
        assert!(as_simple.is_blocked());

        // The same text as displayed content encodes and is understood.
        let displayed = analyze(MessageInput::Literal(".input looks like a keyword")).unwrap();
        assert!(displayed.facts().is_some());
        assert_eq!(displayed.mf2_source(), "{{.input looks like a keyword}}");

        // An empty simple message is valid MF2, because the grammar makes the
        // whole production optional. The encoder still uses the quoted form so
        // that one encoding covers every displayed string.
        assert!(analyze(MessageInput::Mf2("")).unwrap().facts().is_some());
        assert_eq!(
            analyze(MessageInput::Literal("")).unwrap().mf2_source(),
            "{{}}"
        );
    }

    #[test]
    fn identical_cooked_content_analyses_identically_across_authoring_forms() {
        let literal = analyze(MessageInput::Literal("Pay now")).unwrap();
        let authored = analyze(MessageInput::Mf2("{{Pay now}}")).unwrap();
        assert_eq!(literal.mf2_source(), authored.mf2_source());
        assert_eq!(literal.facts(), authored.facts());
    }

    #[test]
    fn syntax_failure_suppresses_dependent_semantic_work_but_keeps_its_own_code() {
        let analysis = analyze(MessageInput::Mf2("Hello {$name")).unwrap();
        assert!(
            analysis.facts().is_none(),
            "no facts from a malformed message"
        );
        assert!(analysis.is_blocked());
        let codes: Vec<&str> = analysis
            .diagnostics()
            .iter()
            .map(|record| record.origin().code())
            .collect();
        assert!(!codes.is_empty());
        assert!(
            codes.iter().all(|code| !code.starts_with("authoring-")),
            "parser codes must not be relabelled into an authoring family: {codes:?}"
        );
    }

    #[test]
    fn a_reported_range_addresses_real_bytes_and_resolves_through_the_segments() {
        let analysis = analyze(MessageInput::Mf2("Hello {$name")).unwrap();
        let reported = analysis
            .diagnostics()
            .iter()
            .find_map(Diagnostic::message_range)
            .expect("a syntax diagnostic carries the range it concerns");

        // The range addresses the analyzed MF2 bytes, on scalar boundaries.
        let source = analysis.mf2_source();
        let reported = reported.range();
        assert!(reported.end() <= source.len() as u64);
        assert!(source.is_char_boundary(reported.start() as usize));
        assert!(source.is_char_boundary(reported.end() as usize));

        // A caller resolves it back to the supplied text through the segments,
        // which is the reason the range is exposed at all.
        let containing = analysis
            .extraction_map()
            .iter()
            .find(|segment| {
                segment.extracted().start() <= reported.start()
                    && reported.start() < segment.extracted().end()
            })
            .expect("every emitted byte is covered by a segment");
        assert!(containing.source().end() >= containing.source().start());
    }

    #[test]
    fn semantic_failure_is_reported_with_its_parser_owned_code() {
        let duplicate = ".input {$a}\n.input {$a}\n{{text}}";
        let analysis = analyze(MessageInput::Mf2(duplicate)).unwrap();
        assert!(analysis.facts().is_none());
        assert!(analysis.diagnostics().iter().any(|record| matches!(
            record.origin(),
            DiagnosticOrigin::Mf2Semantic("duplicate-declaration")
        )));
    }

    #[test]
    fn a_generated_literal_that_the_parser_rejects_is_an_operational_failure() {
        // U+0000 has no pattern representation, so encoding stops before the
        // parser ever sees a message that could not be produced correctly.
        assert_eq!(
            analyze(MessageInput::Literal("a\0b")),
            Err(MessageFailure::UnrepresentableScalar { offset: 1 })
        );
    }

    #[test]
    fn fresh_and_reused_workspaces_agree_after_success_and_failure() {
        let occurrence = occurrence(OccurrenceRole::IntentLiteral);
        let limits = generous();
        let inputs = [
            MessageInput::Literal("Pay now"),
            MessageInput::Mf2("Hello {$name}"),
            MessageInput::Mf2("broken {$name"),
            MessageInput::Literal("Total: {amount}"),
        ];

        let fresh: Vec<_> = inputs
            .iter()
            .map(|input| {
                let mut workspace = AnalysisWorkspace::new();
                analyze_message(*input, &occurrence, &limits, &mut workspace)
            })
            .collect();

        let mut shared = AnalysisWorkspace::new();
        // Exercise a failing analysis first so a reset defect would show up.
        let _ = analyze_message(
            MessageInput::Literal("a\0b"),
            &occurrence,
            &limits,
            &mut shared,
        );
        let reused: Vec<_> = inputs
            .iter()
            .map(|input| analyze_message(*input, &occurrence, &limits, &mut shared))
            .collect();

        assert_eq!(fresh, reused);
        assert!(
            shared.capacities().segments > 0,
            "capacity must be retained"
        );
    }

    #[test]
    fn parameter_bounds_report_the_exact_exhausted_limit() {
        let mut workspace = AnalysisWorkspace::new();
        let occurrence = occurrence(OccurrenceRole::IntentLiteral);

        let mut names = generous();
        names.parameter_names = 2;
        assert!(analyze_message(
            MessageInput::Mf2("{$a} {$b}"),
            &occurrence,
            &names,
            &mut workspace
        )
        .is_ok());
        assert_eq!(
            analyze_message(
                MessageInput::Mf2("{$a} {$b} {$c}"),
                &occurrence,
                &names,
                &mut workspace
            ),
            Err(MessageFailure::Limit(LimitKind::ParameterNames))
        );

        let mut bytes = generous();
        bytes.parameter_name_bytes = 3;
        assert!(analyze_message(
            MessageInput::Mf2("{$abc}"),
            &occurrence,
            &bytes,
            &mut workspace
        )
        .is_ok());
        assert_eq!(
            analyze_message(
                MessageInput::Mf2("{$abcd}"),
                &occurrence,
                &bytes,
                &mut workspace
            ),
            Err(MessageFailure::Limit(LimitKind::ParameterNameBytes))
        );
    }
}
