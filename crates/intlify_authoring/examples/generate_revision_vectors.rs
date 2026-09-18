// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Write the Intent revision vectors an independent implementation checks.
//!
//! Each vector holds the exact digest preimage and the digest this crate
//! computed from it. Publishing the preimage lets the Node checker verify the
//! framing separately from the hash, so a disagreement names which half is
//! wrong instead of only reporting different hex.

use std::path::PathBuf;
use std::process::ExitCode;

use intlify_authoring::{
    analyze_message, intent_projection, intent_revision, revision_preimage, AnalysisWorkspace,
    AuthoringLimits, ByteRange, MessageInput, Occurrence, OccurrenceRole, OwnerIdentity, OwnerKind,
    SourceSnapshot, VersionedIdentity,
};
use serde_json::json;

const ARTIFACT: &str = "fixtures/phase1/revision-vectors.json";

/// One vector: a message, its context, and whether it must equal another.
struct Case {
    id: &'static str,
    literal: bool,
    source: &'static str,
    locale: &'static str,
    description: Option<&'static str>,
    equal_to: &'static [&'static str],
}

const CASES: &[Case] = &[
    Case {
        id: "plain-literal",
        literal: true,
        source: "Pay now",
        locale: "en",
        description: None,
        equal_to: &["equivalent-quoted-pattern"],
    },
    Case {
        id: "equivalent-quoted-pattern",
        literal: false,
        source: "{{Pay now}}",
        locale: "en",
        description: None,
        equal_to: &["plain-literal"],
    },
    Case {
        id: "trailing-space-differs",
        literal: true,
        source: "Pay now ",
        locale: "en",
        description: None,
        equal_to: &[],
    },
    Case {
        id: "decomposed-literal-stays-distinct",
        literal: true,
        source: "cafe\u{301}",
        locale: "en",
        description: None,
        equal_to: &[],
    },
    Case {
        id: "precomposed-literal",
        literal: true,
        source: "caf\u{e9}",
        locale: "en",
        description: None,
        equal_to: &[],
    },
    Case {
        id: "variable-and-function",
        literal: false,
        source: "{$amount :number style=percent}",
        locale: "en",
        description: None,
        equal_to: &[],
    },
    Case {
        id: "matcher",
        literal: false,
        source: ".input {$count :number}\n.match $count\none {{one}}\n* {{many}}",
        locale: "en",
        description: None,
        equal_to: &[],
    },
    Case {
        id: "local-declaration",
        literal: false,
        source: ".local $greeting = {|hello|}\n{{{$greeting}}}",
        locale: "en",
        description: None,
        equal_to: &[],
    },
    Case {
        id: "markup",
        literal: false,
        source: "{#b}bold{/b}{#img /}",
        locale: "en",
        description: None,
        equal_to: &[],
    },
    Case {
        id: "other-locale",
        literal: true,
        source: "Pay now",
        locale: "ja",
        description: None,
        equal_to: &[],
    },
    Case {
        id: "with-description",
        literal: true,
        source: "Pay now",
        locale: "en",
        description: Some("Primary payment action"),
        equal_to: &[],
    },
];

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
        ByteRange::new(0, 16).unwrap(),
        OccurrenceRole::UiLiteral,
    )
    .expect("range inside the snapshot")
}

fn main() -> ExitCode {
    let write = matches!(std::env::args().nth(1).as_deref(), Some("--write"));
    let limits = limits();
    let occurrence = occurrence();
    let mut workspace = AnalysisWorkspace::new();
    let mut vectors = Vec::new();

    for case in CASES {
        let input = if case.literal {
            MessageInput::Literal(case.source)
        } else {
            MessageInput::Mf2(case.source)
        };
        let Ok(analysis) = analyze_message(input, &occurrence, &limits, &mut workspace) else {
            eprintln!("{}: analysis failed", case.id);
            return ExitCode::FAILURE;
        };
        let Some(facts) = analysis.facts() else {
            eprintln!("{}: message not understood", case.id);
            return ExitCode::FAILURE;
        };
        let Ok(projection) = intent_projection(
            facts.message().clone(),
            facts.parameters().to_vec().into_boxed_slice(),
            case.locale,
            None,
            case.description,
        ) else {
            eprintln!("{}: context rejected", case.id);
            return ExitCode::FAILURE;
        };
        let (Ok(preimage), Ok(revision)) =
            (revision_preimage(&projection), intent_revision(&projection))
        else {
            eprintln!("{}: revision could not be computed", case.id);
            return ExitCode::FAILURE;
        };
        vectors.push(json!({
            "id": case.id,
            "mf2Source": analysis.mf2_source(),
            "equalTo": case.equal_to,
            "preimage": preimage,
            "revision": revision,
        }));
    }

    let document = json!({
        "note": "Each vector is the exact digest preimage and the revision taken over it. \
                 An independent implementation reframes the preimage and re-hashes it, so a \
                 disagreement identifies whether the framing or the hash is wrong.",
        "domain": "intent-semantic-revision",
        "projectionSpecification": intlify_authoring::projection_specification(),
        "vectors": vectors,
    });
    let mut rendered = serde_json::to_string_pretty(&document).expect("serializable document");
    rendered.push('\n');

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ARTIFACT);
    if write {
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
    match std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    {
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
