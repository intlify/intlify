// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Write the registry history vectors an independent implementation checks.
//!
//! The vectors are one chain: a genesis and three updates, each planned from a
//! real inventory of real source text. Between them they hold every decision
//! kind and a lineage link. Every artifact is complete and sealed, so the Node
//! checker can re-derive each digest, resolve each reference to the artifact
//! it names, and replay each source edit over the retained bytes, without
//! sharing any code with this crate.
//!
//! Each snapshot after the genesis is written out by hand from its base and
//! its update, rather than produced by applying one. That is what makes it an
//! expectation a later replay can be checked against.
//!
//! The Intent ID and registry identity values are fixed fixture values. They
//! stand in for what a host draws from operating-system randomness; nothing
//! here or in this crate generates one.

use std::path::PathBuf;
use std::process::ExitCode;

use intlify_authoring::test_context::TestContext;
use intlify_authoring::{
    resolve_declarations, AnalysisWorkspace, AuthoringArtifact, AuthoringContext, AuthoringLimits,
    ByteRange, Completeness, DeclarationFacts, DeclarationInput, DeclarationMetadata, InputSegment,
    InventoryArtifact, InventoryBuilder, MessageInput, MessageIntentId, Occurrence, OccurrenceRole,
    OwnerIdentity, OwnerKind, ReferenceFacts, SourceSnapshot, SurfaceVocabulary, UnitOutcome,
    UnitResult, VersionedIdentity, ARTIFACT_INTEGRITY_DOMAIN,
};
use intlify_authoring_identity::{
    AllocationBasis, ContinuationBasis, EntryState, ExplicitBasis, IdentityDecision,
    IntentRegistrySnapshot, IntentRegistryUpdate, LineageKind, LineageLink, RegistryArtifact,
    RegistryEntry, RegistryIdentity, RegistryUpdateArtifact, Replacement, SourceEdit,
};
use intlify_shared_json::encoding::digest_bytes;
use intlify_shared_json::token::IntegrityDigest;
use serde_json::{json, Value};

const ARTIFACT: &str = "fixtures/phase3/registry-vectors.json";

const SCOPE: &str = "storefront-web";

/// The verifier profile the edits claim. It names the edit-replay rules of
/// plan interpretation #3, which the continuity phase implements.
const EDIT_PROFILE: (&str, &str) = ("intlify-continuity-edit-replay", "0");

const REGISTRY: &str = "f55ca0b3224c28776d729daf805177d7";
const PAY: &str = "495c5869254a66216d162abaff700528";
const CANCEL: &str = "90389776033b3b9459b46b618e9cc041";
const PAY_LATER: &str = "9df4f5956d01f3abac8b34bb29314486";
const HOME: &str = "6ad022577a9ed7bb6d5f141f06e99df9";

const NAV: &str = "const home = intent('Home')\n";
const HEADER_LINE: &str = "// checkout actions\n";
const PAY_LINE: &str = "const pay = intent('Pay now')\n";
const PAY_LATER_LINE: &str = "const payLater = intent('Pay now')\n";
const CANCEL_LINE: &str = "const cancel = intent('Cancel')\n";

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

fn id(value: &str) -> MessageIntentId {
    MessageIntentId::retained(owner(), value).expect("a fixture value")
}

/// One unit revision and its exact text.
struct Source {
    unit: &'static str,
    revision: &'static str,
    text: String,
}

impl Source {
    fn new(unit: &'static str, revision: &'static str, text: String) -> Self {
        Self {
            unit,
            revision,
            text,
        }
    }

    fn snapshot(&self) -> SourceSnapshot {
        SourceSnapshot::new(
            owner(),
            self.unit,
            self.revision,
            VersionedIdentity::literal("intlify-grammar-js-module", "0"),
            self.text.len() as u64,
            IntegrityDigest::from_hash(digest_bytes(self.text.as_bytes())).as_str(),
        )
        .expect("checked snapshot")
    }
}

/// One `intent('…')` call: the whole call, the quoted literal, and its
/// content.
struct Call {
    whole: ByteRange,
    literal: ByteRange,
    content: ByteRange,
}

/// Find every `intent('…')` call in a fixture text, in source order.
fn calls(text: &str) -> Vec<Call> {
    let range =
        |start: usize, end: usize| ByteRange::new(start as u64, end as u64).expect("ordered");
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(offset) = text[from..].find("intent('") {
        let start = from + offset;
        let quote = start + "intent(".len();
        let close = quote + 1 + text[quote + 1..].find('\'').expect("the literal closes");
        found.push(Call {
            whole: range(start, close + 2),
            literal: range(quote, close + 1),
            content: range(quote + 1, close),
        });
        from = close;
    }
    found
}

/// Build one complete inventory of checked units, as a Producer would.
fn inventory(sources: &[&Source], context: &TestContext) -> InventoryArtifact {
    let mut occurrences = Vec::new();
    let mut maps = Vec::new();
    let mut messages = Vec::new();
    let mut references = Vec::new();
    for source in sources {
        let snapshot = source.snapshot();
        for call in calls(&source.text) {
            let literal = Occurrence::new(
                snapshot.clone(),
                call.literal,
                OccurrenceRole::IntentLiteral,
            )
            .expect("inside the unit");
            let content = &source.text[call.content.start() as usize..call.content.end() as usize];
            maps.push([InputSegment::new(
                ByteRange::new(0, content.len() as u64).expect("ordered"),
                call.content,
            )]);
            messages.push(content.to_owned());
            references.push(ReferenceFacts::new(
                Occurrence::new(snapshot.clone(), call.whole, OccurrenceRole::Reference)
                    .expect("inside the unit"),
                vec![literal.clone()],
                vec![],
            ));
            occurrences.push(literal);
        }
    }
    let inputs: Vec<DeclarationInput<'_>> = occurrences
        .iter()
        .zip(&maps)
        .zip(&messages)
        .map(|((occurrence, map), message)| DeclarationInput {
            occurrence: occurrence.clone(),
            message: MessageInput::Mf2(message),
            input_map: Some(map),
            metadata: DeclarationMetadata::default(),
            usage: None,
            parameters: Some(&[]),
        })
        .collect();
    let result = resolve_declarations(context, &inputs, &limits(), &mut AnalysisWorkspace::new())
        .expect("a complete invocation");

    let mut builder = InventoryBuilder::new(
        owner(),
        SCOPE,
        context.basis().clone(),
        Completeness::Complete,
    )
    .expect("checked scope");
    for source in sources {
        builder.unit(UnitResult::new(source.snapshot(), UnitOutcome::Checked));
    }
    builder.declarations(result.checked().expect("checked").to_vec());
    for reference in references {
        builder.reference(reference);
    }
    InventoryArtifact::seal(builder.finish().expect("well formed")).expect("sealable")
}

/// Find the declaration an inventory records for the `nth` call in a unit.
fn declaration(inventory: &InventoryArtifact, source: &Source, nth: usize) -> Occurrence {
    let literal = calls(&source.text)[nth].literal;
    inventory
        .body()
        .declarations()
        .iter()
        .map(DeclarationFacts::occurrence)
        .find(|occurrence| {
            occurrence.source().unit().as_str() == source.unit
                && occurrence.source().revision().as_str() == source.revision
                && occurrence.range() == literal
        })
        .expect("the inventory records the call")
        .clone()
}

fn range(start: usize, end: usize) -> ByteRange {
    ByteRange::new(start as u64, end as u64).expect("ordered")
}

fn verified(changes: Vec<SourceEdit>) -> ContinuationBasis {
    ContinuationBasis::verified_edit(
        VersionedIdentity::literal(EDIT_PROFILE.0, EDIT_PROFILE.1),
        changes,
    )
}

fn active(value: &str, declaration: Occurrence) -> RegistryEntry {
    RegistryEntry::new(id(value), EntryState::Active, declaration)
}

fn entry(id: &str, note: &str, artifact: &impl serde::Serialize) -> Value {
    json!({ "id": id, "note": note, "artifact": artifact })
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

    let nav = Source::new("nav", "1", NAV.to_owned());
    let first = Source::new("checkout", "1", format!("{PAY_LINE}{CANCEL_LINE}"));
    let second = Source::new(
        "checkout",
        "2",
        format!("{HEADER_LINE}{PAY_LINE}{PAY_LATER_LINE}"),
    );
    let third = Source::new(
        "checkout",
        "3",
        format!("{HEADER_LINE}{PAY_LINE}{PAY_LATER_LINE}{CANCEL_LINE}"),
    );

    // The header goes in above the first line, and the cancel line becomes a
    // second pay line. Positions are in the first revision's bytes.
    let header_and_copy = SourceEdit::new(
        Some(first.snapshot()),
        Some(second.snapshot()),
        vec![
            Replacement::new(range(0, 0), HEADER_LINE),
            Replacement::new(
                range(PAY_LINE.len(), PAY_LINE.len() + CANCEL_LINE.len()),
                PAY_LATER_LINE,
            ),
        ],
    );
    // The cancel line comes back at the end.
    let cancel_again = SourceEdit::new(
        Some(second.snapshot()),
        Some(third.snapshot()),
        vec![Replacement::new(
            range(second.text.len(), second.text.len()),
            CANCEL_LINE,
        )],
    );

    let first_inventory = inventory(&[&first, &nav], &context);
    let second_inventory = inventory(&[&second, &nav], &context);
    let third_inventory = inventory(&[&third, &nav], &context);

    let genesis = RegistryArtifact::seal(
        IntentRegistrySnapshot::genesis(
            owner(),
            SCOPE,
            RegistryIdentity::retained(REGISTRY).expect("a fixture value"),
        )
        .expect("checked scope"),
    )
    .expect("sealable");

    // Update 1: nothing has history yet, so every declaration is new.
    let pay_1 = declaration(&first_inventory, &first, 0);
    let cancel_1 = declaration(&first_inventory, &first, 1);
    let home = declaration(&first_inventory, &nav, 0);
    let allocate = RegistryUpdateArtifact::seal(
        IntentRegistryUpdate::new(
            owner(),
            genesis.reference(),
            first_inventory.reference(),
            vec![
                IdentityDecision::allocation(
                    id(PAY),
                    pay_1.clone(),
                    AllocationBasis::confirmed_new(),
                ),
                IdentityDecision::allocation(
                    id(CANCEL),
                    cancel_1.clone(),
                    AllocationBasis::confirmed_new(),
                ),
                IdentityDecision::allocation(
                    id(HOME),
                    home.clone(),
                    AllocationBasis::confirmed_new(),
                ),
            ],
            vec![],
        )
        .expect("a well-formed update"),
    )
    .expect("sealable");
    let allocated = RegistryArtifact::seal(
        IntentRegistrySnapshot::successor(
            &genesis,
            &allocate,
            vec![
                active(PAY, pay_1.clone()),
                active(CANCEL, cancel_1.clone()),
                active(HOME, home.clone()),
            ],
        )
        .expect("a well-formed snapshot"),
    )
    .expect("sealable");

    // Update 2: pay moves below the header, cancel is gone, and its line now
    // holds a copy of pay. Home's unit did not change, so it needs no decision.
    let pay_2 = declaration(&second_inventory, &second, 0);
    let pay_later_2 = declaration(&second_inventory, &second, 1);
    let edit = RegistryUpdateArtifact::seal(
        IntentRegistryUpdate::new(
            owner(),
            allocated.reference(),
            second_inventory.reference(),
            vec![
                IdentityDecision::continuation(
                    id(PAY),
                    pay_1,
                    pay_2.clone(),
                    verified(vec![header_and_copy]),
                ),
                IdentityDecision::retirement(id(CANCEL), cancel_1.clone()),
                IdentityDecision::allocation(
                    id(PAY_LATER),
                    pay_later_2.clone(),
                    AllocationBasis::confirmed_new(),
                ),
            ],
            vec![LineageLink::new(
                LineageKind::Copy,
                vec![id(PAY)],
                vec![id(PAY_LATER)],
            )],
        )
        .expect("a well-formed update"),
    )
    .expect("sealable");
    let edited = RegistryArtifact::seal(
        IntentRegistrySnapshot::successor(
            &allocated,
            &edit,
            vec![
                active(PAY, pay_2.clone()),
                RegistryEntry::new(id(CANCEL), EntryState::Retired, cancel_1.clone()),
                active(PAY_LATER, pay_later_2.clone()),
                active(HOME, home.clone()),
            ],
        )
        .expect("a well-formed snapshot"),
    )
    .expect("sealable");

    // Update 3: cancel comes back, and the author chooses to restore its old
    // ID rather than give it a new one. That choice is explicit.
    let pay_3 = declaration(&third_inventory, &third, 0);
    let pay_later_3 = declaration(&third_inventory, &third, 1);
    let cancel_3 = declaration(&third_inventory, &third, 2);
    let restore = RegistryUpdateArtifact::seal(
        IntentRegistryUpdate::new(
            owner(),
            edited.reference(),
            third_inventory.reference(),
            vec![
                IdentityDecision::continuation(
                    id(PAY),
                    pay_2,
                    pay_3.clone(),
                    verified(vec![cancel_again.clone()]),
                ),
                IdentityDecision::continuation(
                    id(PAY_LATER),
                    pay_later_2,
                    pay_later_3.clone(),
                    verified(vec![cancel_again]),
                ),
                IdentityDecision::restoration(
                    id(CANCEL),
                    cancel_1,
                    cancel_3.clone(),
                    ExplicitBasis::new("The cancel action came back with the same meaning.")
                        .expect("a reason"),
                ),
            ],
            vec![],
        )
        .expect("a well-formed update"),
    )
    .expect("sealable");
    let restored = RegistryArtifact::seal(
        IntentRegistrySnapshot::successor(
            &edited,
            &restore,
            vec![
                active(PAY, pay_3),
                active(CANCEL, cancel_3),
                active(PAY_LATER, pay_later_3),
                active(HOME, home),
            ],
        )
        .expect("a well-formed snapshot"),
    )
    .expect("sealable");

    let sources: Vec<Value> = [&first, &second, &third, &nav]
        .iter()
        .map(|source| {
            json!({ "unit": source.unit, "revision": source.revision, "text": source.text })
        })
        .collect();
    let artifacts = json!([
        entry(
            "inventory-1",
            "Checkout revision 1 with pay and cancel, and the navigation unit with home.",
            &first_inventory,
        ),
        entry(
            "registry-0",
            "The genesis: no history and no entries.",
            &genesis,
        ),
        entry(
            "update-1",
            "Three allocations against the genesis. The base has no history, so every declaration is new.",
            &allocate,
        ),
        entry(
            "registry-1",
            "Three active entries.",
            &allocated,
        ),
        entry(
            "inventory-2",
            "Checkout revision 2: a header above pay, and cancel's line replaced by a copy of pay.",
            &second_inventory,
        ),
        entry(
            "update-2",
            "Pay continues across a verified edit, cancel is retired, and the copy gets a new ID linked to pay.",
            &edit,
        ),
        entry(
            "registry-2",
            "Cancel is retired and keeps its last declaration; home's entry is copied unchanged.",
            &edited,
        ),
        entry(
            "inventory-3",
            "Checkout revision 3: cancel's line added back at the end.",
            &third_inventory,
        ),
        entry(
            "update-3",
            "Pay and its copy continue across a verified edit; cancel's old ID is restored by explicit choice.",
            &restore,
        ),
        entry(
            "registry-3",
            "Four active entries.",
            &restored,
        ),
    ]);
    let document = json!({
        "note": "One registry chain: a genesis and three updates, each planned from a real inventory of the source texts below. An independent implementation removes each artifact's top-level integrityDigest, reframes the rest and re-hashes it under the domain below. It also resolves every reference to the earlier artifact it names, checks that a chain keeps its owner, scope and registry identity, checks every snapshot against the text it names, and replays every source edit over that text.",
        "domain": ARTIFACT_INTEGRITY_DOMAIN,
        "sources": sources,
        "artifacts": artifacts,
    });
    let mut rendered = serde_json::to_string_pretty(&document).expect("serializable document");
    rendered.push('\n');

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ARTIFACT);
    let committed = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    if !write {
        if committed.as_ref() == Some(&document) {
            println!("{} is fresh", path.display());
            return ExitCode::SUCCESS;
        }
        eprintln!("{} is stale; rerun with --write", path.display());
        return ExitCode::FAILURE;
    }
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
    match std::fs::write(&path, rendered) {
        Ok(()) => {
            println!("wrote {}", path.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("could not write {}: {error}", path.display());
            ExitCode::FAILURE
        }
    }
}
