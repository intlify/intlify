// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The `authoring-inventory` representation, from facts to sealed bytes and
//! back through admission.
//!
//! Every fixture starts from real source text and real resolution, so the
//! positions, digests and projections an inventory records are the ones an
//! actual analysis produces. Each refusal case then changes exactly one thing,
//! and where that change would also break the digest, the artifact is resealed
//! so that the case reaches the rule it is about rather than stopping at
//! integrity.

use intlify_authoring::test_context::{admit_inventory as admit_test, TestContext};
use intlify_authoring::{
    admit_inventory, intent_revision, resolve_declarations, AdmissionFailure, AnalysisWorkspace,
    ArtifactRelation, AuthoringLimits, ByteRange, Completeness, ContextKind, DeclarationFacts,
    DeclarationInput, DeclarationMetadata, Exclusion, InputSegment, InventoryArtifact,
    InventoryBuilder, InventoryFailure, MessageInput, Occurrence, OccurrenceRole, OwnerIdentity,
    OwnerKind, ParameterBinding, ReferenceFacts, SnapshotMismatch, SourceBytes, SourceSnapshot,
    SurfaceVocabulary, UnitOutcome, UnitResult, VersionedIdentity, ARTIFACT_INTEGRITY_DOMAIN,
};
use intlify_shared_json::encoding::{digest_bytes, hash, Domain};
use intlify_shared_json::token::IntegrityDigest;
use serde_json::{json, Value};

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

fn context() -> TestContext {
    TestContext::builder(
        owner(),
        SurfaceVocabulary::new(["checkout", "nav"]).unwrap(),
    )
    .default_source_locale("en")
    .default_surface_class("checkout")
    .build()
    .expect("checked test context")
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

/// The byte range of the first occurrence of `needle`, offset by `skip`.
fn span(text: &str, needle: &str, skip: usize, trim: usize) -> ByteRange {
    let start = text.find(needle).expect("needle in fixture") + skip;
    let end = start - skip + needle.len() - trim;
    ByteRange::new(start as u64, end as u64).unwrap()
}

fn at(source: &SourceSnapshot, range: ByteRange, role: OccurrenceRole) -> Occurrence {
    Occurrence::new(source.clone(), range, role).expect("range inside the unit")
}

/// A verbatim host run: every decoded byte came from the matching source byte.
fn verbatim(decoded_len: usize, content: ByteRange) -> [InputSegment; 1] {
    [InputSegment::new(
        ByteRange::new(0, decoded_len as u64).unwrap(),
        content,
    )]
}

/// Everything one fixture unit contributes to an inventory.
struct Parts {
    source: SourceSnapshot,
    declarations: Vec<DeclarationFacts>,
    references: Vec<ReferenceFacts>,
    exclusions: Vec<Exclusion>,
}

/// Resolve the fixture's declarations for real and collect the other facts.
fn parts(unit: &str, text: &str, context: &TestContext) -> Parts {
    let source = snapshot(unit, text);
    let pay_literal = span(text, "'Pay now'", 0, 0);
    let pay_content = span(text, "'Pay now'", 1, 1);
    let hello_tag = span(text, "mf2`Hello {$name}!`", 0, 0);
    let hello_content = span(text, "mf2`Hello {$name}!`", 4, 1);

    let pay_map = verbatim("Pay now".len(), pay_content);
    let hello_map = verbatim("Hello {$name}!".len(), hello_content);
    let inputs = [
        DeclarationInput {
            occurrence: at(&source, pay_literal, OccurrenceRole::IntentLiteral),
            message: MessageInput::Mf2("Pay now"),
            input_map: Some(&pay_map),
            metadata: DeclarationMetadata::default(),
            usage: None,
            parameters: Some(&[]),
        },
        DeclarationInput {
            occurrence: at(&source, hello_tag, OccurrenceRole::Mf2Declaration),
            message: MessageInput::Mf2("Hello {$name}!"),
            input_map: Some(&hello_map),
            metadata: DeclarationMetadata::default(),
            usage: None,
            // A reusable declaration has no use site of its own.
            parameters: None,
        },
    ];
    let mut workspace = AnalysisWorkspace::new();
    let result = resolve_declarations(context, &inputs, &limits(), &mut workspace)
        .expect("a complete invocation");
    let declarations = result
        .checked()
        .expect("both declarations resolve")
        .to_vec();

    let name = span(text, "{ name }", 2, 2);
    let references = vec![
        ReferenceFacts::new(
            at(
                &source,
                span(text, "intent('Pay now')", 0, 0),
                OccurrenceRole::Reference,
            ),
            vec![at(&source, pay_literal, OccurrenceRole::IntentLiteral)],
            vec![],
        ),
        ReferenceFacts::new(
            at(
                &source,
                span(text, "intent(hello, { name })", 0, 0),
                OccurrenceRole::Reference,
            ),
            vec![at(&source, hello_tag, OccurrenceRole::Mf2Declaration)],
            vec![ParameterBinding::new(
                "name",
                at(&source, name, OccurrenceRole::ParameterExpression),
            )],
        ),
    ];
    // The exclusion runs from the marker to the end of its call, whatever the
    // excluded value is, so a fixture can change that value freely.
    let start = text.find("noIntent(").expect("an exclusion in the fixture");
    let end = start + text[start..].find(")\n").expect("the call closes") + 1;
    let exclusions = vec![Exclusion::new(
        at(
            &source,
            ByteRange::new(start as u64, end as u64).unwrap(),
            OccurrenceRole::Exclusion,
        ),
        "Product name",
    )
    .unwrap()];
    Parts {
        source,
        declarations,
        references,
        exclusions,
    }
}

fn builder(context: &TestContext, completeness: Completeness) -> InventoryBuilder {
    use intlify_authoring::AuthoringContext;
    InventoryBuilder::new(
        owner(),
        "checkout-ui",
        context.basis().clone(),
        completeness,
    )
    .expect("checked scope token")
}

fn inventory(parts: Parts, outcome: UnitOutcome, context: &TestContext) -> InventoryArtifact {
    let mut builder = builder(context, Completeness::Complete);
    builder.unit(UnitResult::new(parts.source, outcome));
    builder.declarations(parts.declarations);
    for reference in parts.references {
        builder.reference(reference);
    }
    for exclusion in parts.exclusions {
        builder.exclusion(exclusion);
    }
    InventoryArtifact::seal(builder.finish().expect("a well-formed inventory")).expect("sealable")
}

fn sealed() -> (TestContext, InventoryArtifact) {
    let context = context();
    let artifact = inventory(
        parts("checkout", SOURCE, &context),
        UnitOutcome::Checked,
        &context,
    );
    (context, artifact)
}

fn bytes(artifact: &InventoryArtifact) -> Vec<u8> {
    serde_json::to_vec(artifact).expect("serializable")
}

fn value(artifact: &InventoryArtifact) -> Value {
    serde_json::to_value(artifact).expect("serializable")
}

/// Recompute the digest over an edited value, the way a forger with the
/// specification in hand would.
fn reseal(mut value: Value) -> Vec<u8> {
    let object = value.as_object_mut().expect("an artifact object");
    object.remove("integrityDigest");
    let digest = hash(Domain::literal(ARTIFACT_INTEGRITY_DOMAIN), &value).expect("encodable");
    value["integrityDigest"] = json!(IntegrityDigest::from_hash(digest).as_str());
    serde_json::to_vec(&value).expect("serializable")
}

/// One labelled edit to an artifact's JSON, for refusal tables.
type Damage = (&'static str, fn(&mut Value));

fn admit(
    bytes: &[u8],
    context: &TestContext,
) -> Result<intlify_authoring::AdmittedInventory, AdmissionFailure> {
    let source = [SourceBytes {
        unit: "checkout",
        bytes: SOURCE.as_bytes(),
    }];
    admit_test(
        bytes,
        context,
        &source,
        &limits(),
        &mut AnalysisWorkspace::new(),
    )
}

#[test]
fn a_sealed_inventory_is_admitted_from_its_own_bytes() {
    let (context, artifact) = sealed();
    let admitted = admit(&bytes(&artifact), &context).expect("admitted");
    assert_eq!(admitted.artifact(), &artifact);
    assert_eq!(admitted.reference(), artifact.reference());
    assert!(
        admitted.unverified_units().is_empty(),
        "every unit's bytes were supplied and checked"
    );

    let inventory = admitted.inventory();
    assert_eq!(inventory.declarations().len(), 2);
    assert_eq!(inventory.references().len(), 2);
    assert_eq!(inventory.exclusions().len(), 1);
    assert!(inventory.is_complete_checked());
}

#[test]
fn a_unit_without_supplied_bytes_is_admitted_but_named_as_unverified() {
    // Checking ranges against real bytes is a stronger result than checking
    // their shape, and a caller that supplied no bytes gets the weaker one.
    // That has to be visible, not folded into a single "admitted".
    let (context, artifact) = sealed();
    let admitted = admit_test(
        &bytes(&artifact),
        &context,
        &[],
        &limits(),
        &mut AnalysisWorkspace::new(),
    )
    .expect("admitted");
    let unverified: Vec<&str> = admitted
        .unverified_units()
        .iter()
        .map(intlify_authoring::Token::as_str)
        .collect();
    assert_eq!(unverified, ["checkout"]);
}

#[test]
fn the_ordinary_entry_never_admits_a_test_inventory() {
    // An ordinary build cannot construct a test context, and a test inventory
    // must not become production evidence by being read through the
    // production entry.
    let (context, artifact) = sealed();
    assert_eq!(
        admit_inventory(
            &bytes(&artifact),
            &context,
            &[],
            &limits(),
            &mut AnalysisWorkspace::new()
        ),
        Err(AdmissionFailure::ContextNotAdmitted(
            ContextKind::TestContext
        ))
    );
}

#[test]
fn content_changed_after_sealing_is_refused_before_anything_reads_it() {
    let (context, artifact) = sealed();
    let mut edited = value(&artifact);
    edited["body"]["exclusions"][0]["reason"] = json!("Brand name");
    assert_eq!(
        admit(&serde_json::to_vec(&edited).unwrap(), &context).unwrap_err(),
        AdmissionFailure::Integrity
    );
}

#[test]
fn a_tuple_this_reader_does_not_implement_is_unsupported_not_malformed() {
    let (context, artifact) = sealed();
    let original = value(&artifact);
    let cases: [Damage; 4] = [
        ("a registered kind implemented later", |v| {
            v["kind"] = json!("message-intent");
        }),
        ("an unregistered kind", |v| {
            v["kind"] = json!("translation-catalog");
        }),
        ("a later schema revision", |v| {
            v["schemaRevision"] = json!("1");
        }),
        ("a later specification revision", |v| {
            v["authoringSpecification"]["revision"] = json!("1");
        }),
    ];
    for (label, edit) in cases {
        let mut edited = original.clone();
        edit(&mut edited);
        assert_eq!(
            admit(&reseal(edited), &context).unwrap_err(),
            AdmissionFailure::Unsupported,
            "{label}"
        );
    }
}

#[test]
fn bytes_outside_the_shared_encoding_or_the_closed_shape_are_refused_by_name() {
    let (context, artifact) = sealed();
    assert!(matches!(
        admit(b"{\"kind\":", &context),
        Err(AdmissionFailure::Unreadable(_))
    ));
    let mut numbered = value(&artifact);
    numbered["body"]["units"][0]["source"]["byteLength"] = json!(12);
    assert!(
        matches!(
            admit(&serde_json::to_vec(&numbered).unwrap(), &context),
            Err(AdmissionFailure::Unreadable(_))
        ),
        "the shared encoding carries every quantity as a string"
    );

    let mut extra = value(&artifact);
    extra["body"]["notes"] = json!("unknown members are not extension points");
    assert_eq!(
        admit(&reseal(extra), &context).unwrap_err(),
        AdmissionFailure::Shape
    );
}

#[test]
fn a_retained_mf2_source_that_does_not_reproduce_its_projection_is_refused() {
    // The projection a revision is taken over is recomputed from the MF2 that
    // was retained, not read from the record.
    let (context, artifact) = sealed();
    let mut edited = value(&artifact);
    let declarations = edited["body"]["declarations"].as_array_mut().unwrap();
    let pay = declarations
        .iter_mut()
        .find(|facts| facts["mf2Source"] == json!("Pay now"))
        .expect("the literal declaration");
    // Same length, so the extraction map still covers it and the case reaches
    // the projection rather than stopping at the map.
    pay["mf2Source"] = json!("Pay new");
    assert_eq!(
        admit(&reseal(edited), &context).unwrap_err(),
        AdmissionFailure::InconsistentProjection
    );
}

#[test]
fn a_locale_or_class_the_context_would_not_have_produced_is_refused() {
    let (context, artifact) = sealed();
    let mut locale = value(&artifact);
    locale["body"]["declarations"][0]["projection"]["sourceLocale"] = json!("ja");
    assert_eq!(
        admit(&reseal(locale), &context).unwrap_err(),
        AdmissionFailure::InconsistentContext,
        "the basis says context default, and the context's default is en"
    );

    let mut class = value(&artifact);
    class["body"]["declarations"][0]["surfaceClass"] = json!("billing");
    assert_eq!(
        admit(&reseal(class), &context).unwrap_err(),
        AdmissionFailure::InconsistentContext,
        "billing is not a member of the pinned vocabulary"
    );
}

#[test]
fn an_inventory_is_admitted_only_under_the_context_it_was_resolved_against() {
    let (_, artifact) = sealed();
    let other = TestContext::builder(
        owner(),
        SurfaceVocabulary::new(["checkout", "nav"]).unwrap(),
    )
    .default_source_locale("ja")
    .default_surface_class("checkout")
    .build()
    .unwrap();
    assert_eq!(
        admit(&bytes(&artifact), &other).unwrap_err(),
        AdmissionFailure::BasisMismatch
    );
}

#[test]
fn a_checked_unit_cannot_hold_a_reference_that_did_not_resolve() {
    // Removing the parameter leaves a reference that misses what its
    // declaration requires. A checked unit claims everything in it resolved,
    // so the record contradicts itself; a blocked unit makes no such claim.
    let (context, artifact) = sealed();
    let mut edited = value(&artifact);
    for reference in edited["body"]["references"].as_array_mut().unwrap() {
        reference["parameters"] = json!([]);
    }
    assert_eq!(
        admit(&reseal(edited.clone()), &context).unwrap_err(),
        AdmissionFailure::ParameterMismatch
    );

    edited["body"]["units"][0]["outcome"] = json!("blocked");
    let admitted = admit(&reseal(edited), &context).expect("a blocked unit records it");
    assert!(!admitted.inventory().is_complete_checked());
}

#[test]
fn supplied_bytes_are_checked_against_the_unit_they_claim() {
    let (context, artifact) = sealed();
    let encoded = bytes(&artifact);
    let run = |sources: &[SourceBytes<'_>]| {
        admit_test(
            &encoded,
            &context,
            sources,
            &limits(),
            &mut AnalysisWorkspace::new(),
        )
    };

    let mut altered = SOURCE.as_bytes().to_vec();
    altered[6] = b'P';
    assert_eq!(
        run(&[SourceBytes {
            unit: "checkout",
            bytes: &altered,
        }])
        .unwrap_err(),
        AdmissionFailure::Source(SnapshotMismatch::Utf8Digest)
    );
    assert_eq!(
        run(&[SourceBytes {
            unit: "billing",
            bytes: SOURCE.as_bytes(),
        }])
        .unwrap_err(),
        AdmissionFailure::UnknownSourceUnit
    );
    let twice = SourceBytes {
        unit: "checkout",
        bytes: SOURCE.as_bytes(),
    };
    assert_eq!(
        run(&[twice, twice]).unwrap_err(),
        AdmissionFailure::UnknownSourceUnit,
        "one unit's bytes supplied twice is ambiguous, not redundant"
    );
}

#[test]
fn a_recorded_range_that_splits_a_character_of_the_real_source_is_refused() {
    // The unit's shape alone admits any range up to its length. Only the real
    // bytes can show that a range starts in the middle of a character.
    let text = "const pay = intent('Pay now')\n\
const hello = mf2`Hello {$name}!`\n\
greet(intent(hello, { name }))\n\
brand.textContent = noIntent('日本', 'Product name')\n";
    let context = context();
    let artifact = inventory(
        parts("checkout", text, &context),
        UnitOutcome::Checked,
        &context,
    );
    let mut edited = value(&artifact);
    let range = &mut edited["body"]["exclusions"][0]["occurrence"]["range"];
    let start: u64 = range["start"].as_str().unwrap().parse().unwrap();
    let inside = text.find("日本").unwrap() as u64 + 1;
    assert!(inside > start);
    range["start"] = json!(inside.to_string());
    let source = [SourceBytes {
        unit: "checkout",
        bytes: text.as_bytes(),
    }];
    assert_eq!(
        admit_test(
            &reseal(edited),
            &context,
            &source,
            &limits(),
            &mut AnalysisWorkspace::new()
        )
        .unwrap_err(),
        AdmissionFailure::SourceBoundary
    );
}

#[test]
fn moving_source_changes_the_artifact_and_leaves_every_revision_alone() {
    // Source evidence is part of what the artifact records, so a move changes
    // its integrity digest. It is not part of what a message means, so the
    // revision of every declaration stays the same.
    let context = context();
    let before = inventory(
        parts("checkout", SOURCE, &context),
        UnitOutcome::Checked,
        &context,
    );
    let moved = format!("// moved below a new header\n{SOURCE}");
    let after = inventory(
        parts("checkout", &moved, &context),
        UnitOutcome::Checked,
        &context,
    );

    assert_ne!(before.reference(), after.reference());
    assert_eq!(before.relation(&after), ArtifactRelation::Distinct);
    let revisions = |artifact: &InventoryArtifact| -> Vec<_> {
        artifact
            .body()
            .declarations()
            .iter()
            .map(|facts| intent_revision(facts.projection()).unwrap())
            .collect()
    };
    assert_eq!(revisions(&before), revisions(&after));
}

#[test]
fn one_reference_naming_two_contents_is_a_conflict_not_a_duplicate() {
    let (_, artifact) = sealed();
    assert_eq!(artifact.relation(&artifact.clone()), ArtifactRelation::Same);

    // Keep the stored digest and change the content, which is exactly what a
    // digest collision or a tampered copy would look like to a reader that
    // trusted the digest instead of comparing.
    let mut forged = value(&artifact);
    forged["body"]["exclusions"][0]["reason"] = json!("Brand name");
    let forged: InventoryArtifact = serde_json::from_value(forged).unwrap();
    assert_eq!(artifact.relation(&forged), ArtifactRelation::Conflict);
}

#[test]
fn a_complete_scope_with_a_failed_unit_is_a_record_but_not_checked_input() {
    // 016 describes this state directly: a complete inventory with an
    // unsuccessfully analyzed unit cannot produce a complete checked result.
    // The record is legitimate; relabelling it partial would misdescribe what
    // the caller declared.
    let context = context();
    let parts = parts("checkout", SOURCE, &context);
    let mut builder = builder(&context, Completeness::Complete);
    builder.unit(UnitResult::new(parts.source.clone(), UnitOutcome::Checked));
    builder.unit(UnitResult::new(
        snapshot("broken", "const = ;"),
        UnitOutcome::Failed,
    ));
    builder.declarations(parts.declarations);
    let inventory = builder.finish().expect("a legitimate record");
    assert!(!inventory.is_complete_checked());

    let mut partial = builder_for_partial(&context);
    partial.unit(UnitResult::new(parts.source, UnitOutcome::Checked));
    assert!(
        !partial.finish().unwrap().is_complete_checked(),
        "checked facts for a smaller scope are not complete input"
    );
}

fn builder_for_partial(context: &TestContext) -> InventoryBuilder {
    builder(context, Completeness::Partial)
}

#[test]
fn each_structural_rule_is_refused_by_its_own_name() {
    let context = context();
    let base = || parts("checkout", SOURCE, &context);
    let finish = |edit: &dyn Fn(&mut InventoryBuilder, Parts)| {
        let mut builder = builder(&context, Completeness::Complete);
        edit(&mut builder, base());
        builder.finish().unwrap_err()
    };

    assert_eq!(
        finish(&|builder, parts| {
            builder.unit(UnitResult::new(parts.source.clone(), UnitOutcome::Checked));
            builder.unit(UnitResult::new(parts.source, UnitOutcome::Checked));
        }),
        InventoryFailure::DuplicateUnit
    );
    assert_eq!(
        finish(&|builder, parts| {
            builder.unit(UnitResult::new(parts.source, UnitOutcome::Checked));
            builder.declarations([parts.declarations[0].clone(), parts.declarations[0].clone()]);
        }),
        InventoryFailure::DuplicateOccurrence
    );
    assert_eq!(
        finish(&|builder, parts| {
            // The occurrence names a unit this inventory does not list.
            builder.unit(UnitResult::new(
                snapshot("other", SOURCE),
                UnitOutcome::Checked,
            ));
            builder.declarations(parts.declarations);
        }),
        InventoryFailure::UnknownSource
    );
    assert_eq!(
        finish(&|builder, parts| {
            builder.unit(UnitResult::new(parts.source.clone(), UnitOutcome::Failed));
            builder.declarations(parts.declarations);
        }),
        InventoryFailure::FactsFromFailedUnit
    );
    assert_eq!(
        finish(&|builder, parts| {
            builder.unit(UnitResult::new(parts.source, UnitOutcome::Checked));
            // A reference to a declaration this inventory does not hold.
            builder.reference(parts.references[0].clone());
        }),
        InventoryFailure::UnresolvedReference
    );
    assert_eq!(
        finish(&|builder, parts| {
            builder.unit(UnitResult::new(parts.source, UnitOutcome::Checked));
            builder.reference(ReferenceFacts::new(
                parts.references[0].occurrence().clone(),
                vec![],
                vec![],
            ));
        }),
        InventoryFailure::EmptyReference
    );
    assert_eq!(
        finish(&|builder, parts| {
            builder.unit(UnitResult::new(parts.source.clone(), UnitOutcome::Checked));
            builder.declarations(parts.declarations);
            let reference = &parts.references[1];
            let binding = reference.parameters()[0].clone();
            builder.reference(ReferenceFacts::new(
                reference.occurrence().clone(),
                reference.declarations().to_vec(),
                vec![binding.clone(), binding],
            ));
        }),
        InventoryFailure::DuplicateParameter
    );
    assert_eq!(
        finish(&|builder, parts| {
            builder.unit(UnitResult::new(parts.source.clone(), UnitOutcome::Checked));
            // An exclusion carrying a reference role.
            let occurrence = parts.references[0].occurrence().clone();
            builder.exclusion(Exclusion::new(occurrence, "Product name").unwrap());
        }),
        InventoryFailure::RoleMismatch
    );
    assert_eq!(
        finish(&|builder, parts| {
            let foreign = OwnerIdentity::new(OwnerKind::Library, "storefront").unwrap();
            let source = SourceSnapshot::new(
                foreign,
                "checkout",
                "1",
                VersionedIdentity::literal("intlify-grammar-js-module", "0"),
                SOURCE.len() as u64,
                parts.source.utf8_digest().as_str(),
            )
            .unwrap();
            builder.unit(UnitResult::new(source, UnitOutcome::Checked));
        }),
        InventoryFailure::ForeignOwner,
        "the same identity under the library kind is a different owner"
    );
}

#[test]
fn a_decoded_record_is_held_to_the_rules_the_builder_would_have_applied() {
    // Decoding does not run the builder, so admission validates what decoding
    // produced. Two rules are only reachable this way: order the builder
    // would have fixed, and a range the constructor would have refused.
    let context = context();
    let parts = parts("checkout", SOURCE, &context);
    let mut builder = builder(&context, Completeness::Complete);
    builder.unit(UnitResult::new(parts.source, UnitOutcome::Checked));
    builder.unit(UnitResult::new(snapshot("zeta", "x"), UnitOutcome::Checked));
    let artifact = InventoryArtifact::seal(builder.finish().unwrap()).unwrap();

    let mut swapped = value(&artifact);
    swapped["body"]["units"].as_array_mut().unwrap().swap(0, 1);
    assert_eq!(
        admit(&reseal(swapped), &context).unwrap_err(),
        AdmissionFailure::Inventory(InventoryFailure::UnitsUnordered)
    );

    let (context, full) = sealed();
    let mut past = value(&full);
    past["body"]["exclusions"][0]["occurrence"]["range"]["end"] =
        json!((SOURCE.len() + 1).to_string());
    past["body"]["exclusions"][0]["occurrence"]["range"]["start"] =
        json!((SOURCE.len() - 3).to_string());
    assert_eq!(
        admit(&reseal(past), &context).unwrap_err(),
        AdmissionFailure::Inventory(InventoryFailure::RangeOutsideSource)
    );
}

#[test]
fn admission_is_bounded_by_the_caller_limits() {
    let (context, artifact) = sealed();
    let mut tight = limits();
    tight.declarations = 1;
    assert_eq!(
        admit_test(
            &bytes(&artifact),
            &context,
            &[],
            &tight,
            &mut AnalysisWorkspace::new()
        )
        .unwrap_err(),
        AdmissionFailure::Limit(intlify_authoring::LimitKind::Declarations)
    );
    tight.declarations = 2;
    assert!(admit_test(
        &bytes(&artifact),
        &context,
        &[],
        &tight,
        &mut AnalysisWorkspace::new()
    )
    .is_ok());
}

#[test]
fn the_committed_schema_refuses_what_the_reader_refuses() {
    // A schema looser than its reader lets a consumer validate a document the
    // reader would never admit. Every structural refusal Draft 7 can express
    // is therefore checked on both sides.
    let schema: Value =
        serde_json::from_str(include_str!("../schema/authoring-inventory-v0.schema.json")).unwrap();
    let validator = jsonschema::draft7::new(&schema).expect("a valid Draft 7 schema");
    let (context, artifact) = sealed();
    let original = value(&artifact);
    assert!(
        validator.is_valid(&original),
        "a real artifact is valid against its own schema"
    );

    let cases: [Damage; 9] = [
        ("an unknown top-level member", |v| v["extra"] = json!(true)),
        ("a kind this reader does not implement", |v| {
            v["kind"] = json!("message-intent");
        }),
        ("a later schema revision", |v| {
            v["schemaRevision"] = json!("1");
        }),
        ("a later specification revision", |v| {
            v["authoringSpecification"]["revision"] = json!("1");
        }),
        ("an empty surface class", |v| {
            v["body"]["declarations"][0]["surfaceClass"] = json!("");
        }),
        ("an unregistered occurrence role", |v| {
            v["body"]["exclusions"][0]["occurrence"]["role"] = json!("attribute");
        }),
        ("a missing integrity digest", |v| {
            v.as_object_mut().unwrap().remove("integrityDigest");
        }),
        ("a digest in another presentation", |v| {
            v["integrityDigest"] = json!(format!("md5:{}", "0".repeat(32)));
        }),
        ("an offset that is not a decimal string", |v| {
            v["body"]["exclusions"][0]["occurrence"]["range"]["start"] = json!("twelve");
        }),
    ];
    for (label, damage) in cases {
        let mut damaged = original.clone();
        damage(&mut damaged);
        assert!(!validator.is_valid(&damaged), "the schema admitted {label}");
        let encoded = if damaged
            .get("integrityDigest")
            .and_then(Value::as_str)
            .is_some_and(|digest| digest.starts_with("sha256:"))
        {
            reseal(damaged)
        } else {
            serde_json::to_vec(&damaged).unwrap()
        };
        assert!(
            admit(&encoded, &context).is_err(),
            "the reader admitted {label}"
        );
    }
}

#[test]
fn every_committed_vector_is_admitted_from_its_committed_bytes() {
    // The vectors are what a second implementation checks, so they have to be
    // artifacts this reader actually admits, not merely documents that hash.
    let document: Value =
        serde_json::from_str(include_str!("../fixtures/phase2/inventory-vectors.json")).unwrap();
    let context = context();
    for vector in document["vectors"].as_array().unwrap() {
        let id = vector["id"].as_str().unwrap();
        let encoded = serde_json::to_vec(&vector["artifact"]).unwrap();
        let admitted = admit_test(
            &encoded,
            &context,
            &[],
            &limits(),
            &mut AnalysisWorkspace::new(),
        )
        .unwrap_or_else(|failure| panic!("{id} was refused: {failure:?}"));
        let complete_checked = admitted.inventory().is_complete_checked();
        assert_eq!(
            complete_checked,
            id != "complete-with-failed-unit",
            "{id}: only the vector with a failed unit falls short of checked input"
        );
    }
}
