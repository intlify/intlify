// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Write the sealed inventory vectors an independent implementation checks.
//!
//! Each vector is a complete sealed `authoring-inventory` artifact. The Node
//! checker removes the one member the preimage excludes, reframes the rest and
//! re-hashes it, so it verifies both the framing and the exclusion rule without
//! sharing any code with this crate.
//!
//! Vectors can also declare that their declarations have the same revisions as
//! another vector's. The checker recomputes every revision itself, which makes
//! "moving source changes the artifact but not what the message means" a claim
//! two implementations agree on rather than one this crate asserts about
//! itself.

use std::path::PathBuf;
use std::process::ExitCode;

use intlify_authoring::test_context::TestContext;
use intlify_authoring::{
    intent_revision, resolve_declarations, AnalysisWorkspace, AuthoringContext, AuthoringLimits,
    ByteRange, Completeness, DeclarationInput, DeclarationMetadata, Exclusion, InputSegment,
    InventoryArtifact, InventoryBuilder, MessageInput, Occurrence, OccurrenceRole, OwnerIdentity,
    OwnerKind, ParameterBinding, ReferenceFacts, SourceSnapshot, SurfaceVocabulary, UnitOutcome,
    UnitResult, VersionedIdentity, ARTIFACT_INTEGRITY_DOMAIN,
};
use intlify_shared_json::encoding::digest_bytes;
use intlify_shared_json::token::IntegrityDigest;
use serde_json::json;

const ARTIFACT: &str = "fixtures/phase2/inventory-vectors.json";

const SOURCE: &str = "const pay = intent('Pay now')\n\
const hello = mf2`Hello {$name}!`\n\
greet(intent(hello, { name }))\n\
brand.textContent = noIntent('Intlify', 'Product name')\n";

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

fn owner() -> OwnerIdentity {
    OwnerIdentity::new(OwnerKind::Application, "storefront").expect("checked project id")
}

fn snapshot(unit: &str, text: &str) -> SourceSnapshot {
    SourceSnapshot::new(
        owner(),
        unit,
        "1",
        VersionedIdentity::literal("intlify-grammar-js-module", "0"),
        text.len() as u64,
        IntegrityDigest::from_hash(digest_bytes(text.as_bytes())).as_str(),
    )
    .expect("checked snapshot")
}

fn span(text: &str, needle: &str, skip: usize, trim: usize) -> ByteRange {
    let start = text.find(needle).expect("needle in fixture") + skip;
    let end = start - skip + needle.len() - trim;
    ByteRange::new(start as u64, end as u64).expect("ordered range")
}

fn at(source: &SourceSnapshot, range: ByteRange, role: OccurrenceRole) -> Occurrence {
    Occurrence::new(source.clone(), range, role).expect("range inside the unit")
}

/// Build one inventory over the fixture text, with an optional failed unit.
fn inventory(text: &str, failed_unit: bool, context: &TestContext) -> InventoryArtifact {
    let source = snapshot("checkout", text);
    let pay = span(text, "'Pay now'", 0, 0);
    let hello = span(text, "mf2`Hello {$name}!`", 0, 0);
    let pay_map = [InputSegment::new(
        ByteRange::new(0, 7).expect("ordered range"),
        span(text, "'Pay now'", 1, 1),
    )];
    let hello_map = [InputSegment::new(
        ByteRange::new(0, 14).expect("ordered range"),
        span(text, "mf2`Hello {$name}!`", 4, 1),
    )];
    let inputs = [
        DeclarationInput {
            occurrence: at(&source, pay, OccurrenceRole::IntentLiteral),
            message: MessageInput::Mf2("Pay now"),
            input_map: Some(&pay_map),
            metadata: DeclarationMetadata::default(),
            usage: None,
            parameters: Some(&[]),
        },
        DeclarationInput {
            occurrence: at(&source, hello, OccurrenceRole::Mf2Declaration),
            message: MessageInput::Mf2("Hello {$name}!"),
            input_map: Some(&hello_map),
            metadata: DeclarationMetadata::default(),
            usage: None,
            parameters: None,
        },
    ];
    let result = resolve_declarations(context, &inputs, &limits(), &mut AnalysisWorkspace::new())
        .expect("a complete invocation");

    let mut builder = InventoryBuilder::new(
        owner(),
        "checkout-ui",
        context.basis().clone(),
        Completeness::Complete,
    )
    .expect("checked scope");
    builder.unit(UnitResult::new(source.clone(), UnitOutcome::Checked));
    if failed_unit {
        builder.unit(UnitResult::new(
            snapshot("broken", "const = ;"),
            UnitOutcome::Failed,
        ));
    }
    builder.declarations(result.checked().expect("checked").to_vec());
    builder.reference(ReferenceFacts::new(
        at(
            &source,
            span(text, "intent('Pay now')", 0, 0),
            OccurrenceRole::Reference,
        ),
        vec![at(&source, pay, OccurrenceRole::IntentLiteral)],
        vec![],
    ));
    builder.reference(ReferenceFacts::new(
        at(
            &source,
            span(text, "intent(hello, { name })", 0, 0),
            OccurrenceRole::Reference,
        ),
        vec![at(&source, hello, OccurrenceRole::Mf2Declaration)],
        vec![ParameterBinding::new(
            "name",
            at(
                &source,
                span(text, "{ name }", 2, 2),
                OccurrenceRole::ParameterExpression,
            ),
        )],
    ));
    let start = text.find("noIntent(").expect("an exclusion");
    let end = start + text[start..].find(")\n").expect("the call closes") + 1;
    builder.exclusion(
        Exclusion::new(
            at(
                &source,
                ByteRange::new(start as u64, end as u64).expect("ordered range"),
                OccurrenceRole::Exclusion,
            ),
            "Product name",
        )
        .expect("a reason"),
    );
    InventoryArtifact::seal(builder.finish().expect("well formed")).expect("sealable")
}

/// One vector: the sealed artifact, and the revision this crate computes for
/// each of its declarations.
///
/// The revisions are written out so the independent implementation compares
/// what it computes with what this crate computes. Comparing its own results
/// across vectors alone would pass for any deterministic function, including a
/// wrong one, because equal projections give equal answers.
fn vector(id: &str, note: &str, same: &[&str], artifact: &InventoryArtifact) -> serde_json::Value {
    let revisions: Vec<String> = artifact
        .body()
        .declarations()
        .iter()
        .map(|facts| {
            intent_revision(facts.projection())
                .expect("a checked projection has a revision")
                .as_str()
                .to_owned()
        })
        .collect();
    json!({
        "id": id,
        "note": note,
        "sameRevisionsAs": same,
        "revisions": revisions,
        "artifact": artifact,
    })
}

fn main() -> ExitCode {
    let write = matches!(std::env::args().nth(1).as_deref(), Some("--write"));
    let context = TestContext::builder(
        owner(),
        SurfaceVocabulary::new(["checkout", "nav"]).expect("a vocabulary"),
    )
    .default_source_locale("en")
    .default_surface_class("checkout")
    .build()
    .expect("checked test context");

    let moved = format!("// moved below a new header\n{SOURCE}");
    let vectors = json!([
        vector(
            "complete-checked",
            "One checked unit with two declarations, two references and an exclusion.",
            &[],
            &inventory(SOURCE, false, &context),
        ),
        vector(
            "moved-source",
            "The same unit with a line added above it. Every position moves, so the artifact changes; no message changes, so no revision does.",
            &["complete-checked"],
            &inventory(&moved, false, &context),
        ),
        vector(
            "complete-with-failed-unit",
            "A complete scope in which one unit could not be read. A legitimate record, and not complete checked input.",
            &["complete-checked"],
            &inventory(SOURCE, true, &context),
        ),
    ]);
    let document = json!({
        "note": "Each vector is a complete sealed authoring-inventory artifact. An independent implementation removes the top-level integrityDigest, reframes the rest and re-hashes it under the domain below. It also recomputes every declaration's revision, and checks that vectors declared to share revisions do while their artifacts differ.",
        "domain": ARTIFACT_INTEGRITY_DOMAIN,
        "revisionDomain": "intent-semantic-revision",
        "projectionSpecification": intlify_authoring::projection_specification(),
        "vectors": vectors,
    });
    let mut rendered = serde_json::to_string_pretty(&document).expect("serializable document");
    rendered.push('\n');

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ARTIFACT);
    let committed = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    if write {
        if committed.as_ref() == Some(&document) {
            println!("{} is already fresh", path.display());
            return ExitCode::SUCCESS;
        }
        if let Some(parent) = path.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                eprintln!("could not create {}: {error}", parent.display());
                return ExitCode::FAILURE;
            }
        }
        return match std::fs::write(&path, rendered) {
            Ok(()) => {
                println!("wrote {}", path.display());
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("could not write {}: {error}", path.display());
                ExitCode::FAILURE
            }
        };
    }
    match committed {
        Some(committed) if committed == document => {
            println!("{} is fresh", path.display());
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{} is stale; rerun with --write", path.display());
            ExitCode::FAILURE
        }
    }
}
