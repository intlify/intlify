// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The fixed fixtures this owner measures against.
//!
//! Every fixture, its expectation, and the order they run in are fixed before
//! any capture begins. Nothing here is generated from a duration, a machine, or
//! an environment variable, and a fixture that fails to prepare stays in the
//! run as a case that produced no measurement.

use std::cell::RefCell;

use crate::context::test_context::{LocaleRule, TestContext};
use crate::context::SurfaceVocabulary;
use crate::declaration::measured::{self, ContextFacts};
use crate::declaration::{DeclarationInput, DeclarationMetadata, ParameterBinding};
use crate::limits::AuthoringLimits;
use crate::message::{analyze_message, ExtractionSegment, MessageAnalysis, MessageInput};
use crate::primitives::{
    ByteRange, Occurrence, OccurrenceRole, OwnerIdentity, OwnerKind, SourceSnapshot,
};
use crate::workspace::AnalysisWorkspace;
use intlify_shared_json::token::VersionedIdentity;

use super::operation::Operation;

/// What one fixture supplies to its operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Input {
    /// Displayed text, before it is encoded as a quoted pattern.
    Literal(&'static str),
    /// Authored MF2 source.
    Mf2(&'static str),
}

/// Which path a fixture is declared to take.
///
/// A fixture states its expected result, so a change that silently moves it
/// onto another path — a fast failure instead of the work it claims to
/// measure — fails a test rather than producing plausible samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Expected {
    /// The operation produces its complete result.
    Complete,
    /// The operation reports why it cannot, which is also measured.
    Blocked,
}

/// One fixed case this owner measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Fixture {
    pub(super) operation: Operation,
    pub(super) name: &'static str,
    pub(super) input: Input,
    /// The locale the declaration explicitly names, when it names one.
    pub(super) source_locale: Option<&'static str>,
    /// The surface class the declaration explicitly names, when it names one.
    pub(super) surface_class: Option<&'static str>,
    /// The external parameters the use site supplies, by name.
    pub(super) parameters: &'static [&'static str],
    /// The path this fixture is declared to take.
    pub(super) expected: Expected,
}

const fn message(
    operation: Operation,
    name: &'static str,
    input: Input,
    expected: Expected,
) -> Fixture {
    Fixture {
        operation,
        name,
        input,
        source_locale: None,
        surface_class: None,
        parameters: &[],
        expected,
    }
}

const fn declaration(
    operation: Operation,
    name: &'static str,
    input: Input,
    source_locale: Option<&'static str>,
    surface_class: Option<&'static str>,
    parameters: &'static [&'static str],
    expected: Expected,
) -> Fixture {
    Fixture {
        operation,
        name,
        input,
        source_locale,
        surface_class,
        parameters,
        expected,
    }
}

/// The complete inventory, in the order it is attempted.
pub(super) const FIXTURES: [Fixture; 12] = [
    message(
        Operation::LiteralEncode,
        "plain",
        Input::Literal("Pay now"),
        Expected::Complete,
    ),
    message(
        Operation::LiteralEncode,
        "escapes",
        Input::Literal("Use {braces} and a backslash \\ here"),
        Expected::Complete,
    ),
    message(
        Operation::LiteralEncode,
        "multibyte",
        Input::Literal("日本語のテキストと絵文字"),
        Expected::Complete,
    ),
    message(
        Operation::Mf2ParseAndSemanticFacts,
        "simple",
        Input::Mf2("Hello {$name}"),
        Expected::Complete,
    ),
    message(
        Operation::Mf2ParseAndSemanticFacts,
        "selector",
        Input::Mf2(
            ".input {$count :number}\n.match $count\none {{One item}}\n* {{{$count} items}}",
        ),
        Expected::Complete,
    ),
    message(
        Operation::Mf2ParseAndSemanticFacts,
        "malformed",
        Input::Mf2("Hello {$name"),
        Expected::Blocked,
    ),
    declaration(
        Operation::SourceLocaleAndSurfaceClass,
        "explicit-locale",
        Input::Literal("Pay now"),
        Some("EN-us"),
        Some("checkout"),
        &[],
        Expected::Complete,
    ),
    declaration(
        Operation::SourceLocaleAndSurfaceClass,
        "context-default",
        Input::Literal("Pay now"),
        None,
        Some("checkout"),
        &[],
        Expected::Complete,
    ),
    declaration(
        Operation::SourceLocaleAndSurfaceClass,
        "unknown-class",
        Input::Literal("Pay now"),
        None,
        Some("billing"),
        &[],
        Expected::Blocked,
    ),
    declaration(
        Operation::DeclarationFacts,
        "literal",
        Input::Literal("Pay now"),
        Some("EN-us"),
        Some("checkout"),
        &[],
        Expected::Complete,
    ),
    declaration(
        Operation::DeclarationFacts,
        "multibyte",
        Input::Literal("日本語のテキスト"),
        None,
        Some("nav"),
        &[],
        Expected::Complete,
    ),
    // The message requires one external parameter, so the use site supplies
    // it. Without the binding this fixture would measure parameter validation
    // failing rather than the facts and revision it claims to measure.
    declaration(
        Operation::DeclarationFacts,
        "authored-mf2",
        Input::Mf2("Hello {$name}"),
        None,
        Some("nav"),
        &["name"],
        Expected::Complete,
    ),
];

/// The bounds every measured case runs under.
pub(super) fn limits() -> AuthoringLimits {
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

/// The context every measured declaration resolves against.
pub(super) fn context() -> Result<TestContext, PreparationFailure> {
    TestContext::builder(
        OwnerIdentity::new(OwnerKind::Application, "intlify-authoring-smoke")
            .map_err(|_| PreparationFailure::Fixture)?,
        SurfaceVocabulary::new(["checkout", "nav"]).map_err(|_| PreparationFailure::Fixture)?,
    )
    .default_source_locale("en")
    .rule(LocaleRule::canonical("EN-us", "en-US"))
    .rule(LocaleRule::canonical("ja", "ja"))
    .rule(LocaleRule::invalid("en_US"))
    .usage_profile(VersionedIdentity::literal(
        "intlify-authoring-smoke-usage",
        "0",
    ))
    .build()
    .map_err(|_| PreparationFailure::Fixture)
}

fn occurrence() -> Result<Occurrence, PreparationFailure> {
    let source = SourceSnapshot::new(
        OwnerIdentity::new(OwnerKind::Application, "intlify-authoring-smoke")
            .map_err(|_| PreparationFailure::Fixture)?,
        "smoke",
        "1",
        VersionedIdentity::literal("intlify-authoring-smoke-grammar", "0"),
        4096,
        &format!("sha256:{}", "0".repeat(64)),
    )
    .map_err(|_| PreparationFailure::Fixture)?;
    Occurrence::new(
        source,
        ByteRange::new(0, 8).map_err(|_| PreparationFailure::Fixture)?,
        OccurrenceRole::UiLiteral,
    )
    .map_err(|_| PreparationFailure::Fixture)
}

/// A fixture could not be prepared, so its case produced no measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum PreparationFailure {
    /// The fixed inputs this harness declares are not themselves admissible.
    Fixture,
    /// The prior stage a measured boundary starts from did not complete.
    PriorStage,
}

/// Everything one case needs, prepared before any interval opens.
pub(super) struct Prepared {
    pub(super) fixture: Fixture,
    pub(super) limits: AuthoringLimits,
    pub(super) occurrence: Occurrence,
    /// The parameters the use site supplies, owned so the input can borrow them.
    pub(super) parameters: Vec<ParameterBinding>,
    pub(super) segments: RefCell<Vec<ExtractionSegment>>,
    pub(super) workspace: RefCell<AnalysisWorkspace>,
    pub(super) context: TestContext,
    /// The analysis a facts case starts from, established outside the interval.
    pub(super) analysis: Option<MessageAnalysis>,
    /// The context facts a facts case starts from, likewise established first.
    pub(super) context_facts: Option<ContextFacts>,
}

impl Prepared {
    /// Build one declaration input over this case's owned fixture values.
    pub(super) fn declaration(&self) -> DeclarationInput<'_> {
        DeclarationInput {
            occurrence: self.occurrence.clone(),
            message: match self.fixture.input {
                Input::Literal(text) => MessageInput::Literal(text),
                Input::Mf2(text) => MessageInput::Mf2(text),
            },
            metadata: DeclarationMetadata {
                source_locale: self.fixture.source_locale,
                surface_class: self.fixture.surface_class,
                description: Some("a fixed smoke fixture"),
            },
            usage: None,
            parameters: &self.parameters,
        }
    }
}

/// Prepare one case completely, before any measurement begins.
pub(super) fn prepare(fixture: Fixture) -> Result<Prepared, PreparationFailure> {
    let limits = limits();
    let occurrence = occurrence()?;
    let context = context()?;
    let parameters = fixture
        .parameters
        .iter()
        .map(|name| ParameterBinding::new(name, occurrence.clone()))
        .collect();
    let mut prepared = Prepared {
        fixture,
        limits,
        occurrence,
        parameters,
        segments: RefCell::new(Vec::new()),
        workspace: RefCell::new(AnalysisWorkspace::new()),
        context,
        analysis: None,
        context_facts: None,
    };
    if fixture.operation == Operation::DeclarationFacts {
        // The facts boundary starts from an admitted message and admitted
        // context facts, so both are established here rather than measured.
        let declaration = prepared.declaration();
        let analysis = analyze_message(
            declaration.message,
            &declaration.occurrence,
            &prepared.limits,
            &mut prepared.workspace.borrow_mut(),
        )
        .map_err(|_| PreparationFailure::PriorStage)?;
        let facts = measured::context_facts(&prepared.context, &declaration, &prepared.limits)
            .map_err(|_| PreparationFailure::PriorStage)?
            .map_err(|_| PreparationFailure::PriorStage)?;
        drop(declaration);
        prepared.analysis = Some(analysis);
        prepared.context_facts = Some(facts);
    }
    Ok(prepared)
}

/// Run one prepared case once, outside measurement, and report its path.
///
/// This is the same invocation the harness uses to establish its expectation,
/// so a fixture cannot declare one path and measure another.
pub(super) fn path_of(prepared: &Prepared) -> Expected {
    use super::operation;
    let complete = match prepared.fixture.operation {
        Operation::LiteralEncode => {
            let Input::Literal(text) = prepared.fixture.input else {
                return Expected::Blocked;
            };
            operation::invoke_encode((text, &prepared.limits, &prepared.segments)).is_ok()
        }
        Operation::Mf2ParseAndSemanticFacts => {
            let Input::Mf2(source) = prepared.fixture.input else {
                return Expected::Blocked;
            };
            operation::invoke_mf2((
                source,
                &prepared.occurrence,
                &prepared.limits,
                &prepared.workspace,
            ))
            .is_some_and(|analysis| analysis.facts().is_some())
        }
        Operation::SourceLocaleAndSurfaceClass => {
            let declaration = prepared.declaration();
            matches!(
                operation::invoke_context((&prepared.context, &declaration, &prepared.limits)),
                Some(Ok(_))
            )
        }
        Operation::DeclarationFacts => {
            let (Some(analysis), Some(facts)) = (&prepared.analysis, &prepared.context_facts)
            else {
                return Expected::Blocked;
            };
            let declaration = prepared.declaration();
            matches!(
                operation::invoke_facts((&declaration, analysis, facts)),
                Some(Ok(_))
            )
        }
    };
    if complete {
        Expected::Complete
    } else {
        Expected::Blocked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_operation_is_covered_and_no_fixture_name_repeats_within_one() {
        for operation in Operation::ALL {
            let names = FIXTURES
                .iter()
                .filter(|fixture| fixture.operation == operation)
                .map(|fixture| fixture.name)
                .collect::<Vec<_>>();
            assert!(!names.is_empty(), "{operation:?} has no fixture");
            let mut unique = names.clone();
            unique.sort_unstable();
            unique.dedup();
            assert_eq!(unique.len(), names.len(), "{operation:?} repeats a name");
        }
    }

    #[test]
    fn every_fixture_prepares_from_the_fixed_inputs_alone() {
        for fixture in FIXTURES {
            assert!(prepare(fixture).is_ok(), "{} did not prepare", fixture.name);
        }
    }

    #[test]
    fn every_fixture_takes_the_path_it_declares() {
        // A fixture that quietly moves onto a failure path measures that
        // failure instead of the work it names, and its samples still look
        // perfectly consistent. Only its declared expectation catches that.
        for fixture in FIXTURES {
            let prepared = prepare(fixture).unwrap();
            assert_eq!(
                path_of(&prepared),
                fixture.expected,
                "{} took the other path",
                fixture.name
            );
        }
    }

    #[test]
    fn a_message_requiring_a_parameter_is_given_exactly_that_parameter() {
        // The use site supplies exactly the names the message requires, so the
        // facts boundary measures building them rather than rejecting them.
        for fixture in FIXTURES
            .iter()
            .filter(|fixture| fixture.operation == Operation::DeclarationFacts)
        {
            let prepared = prepare(*fixture).unwrap();
            let required = prepared
                .analysis
                .as_ref()
                .and_then(MessageAnalysis::facts)
                .map(|facts| facts.parameters().to_vec())
                .unwrap_or_default();
            let supplied = prepared
                .parameters
                .iter()
                .map(|binding| binding.name().to_owned())
                .collect::<Vec<_>>();
            assert_eq!(supplied, required, "{}", fixture.name);
        }
    }
}
