// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! `ox-mf2-bench` — benchmark harness CLI.
//!
//! Designed for use with `hyperfine` and the `tools/mf-parser-bench`
//! orchestrator. Each invocation does ONE thing so that benchmark numbers
//! never mix unrelated phases. Phases are grouped by what they measure so
//! external comparisons can pick the right baseline:
//!
//! Parser-core baselines (compare against other parsers):
//!
//! - `--phase parse_cst_no_trivia` — `parse_source_session` with workspace
//!   reuse, borrowed result, `collect_trivia = false`. Closest to a pure
//!   "tokenise + build CST" parser benchmark.
//! - `--phase parse_cst` — same but `collect_trivia = true`. Adds the cost
//!   of pushing trivia records to the workspace.
//!
//! Optional / downstream cost (always include alongside a baseline):
//!
//! - `--phase lower_semantic` — `parse_source_session` with
//!   `parse_semantic = true`. Measures parser-core + `SemanticModel` lowering.
//! - `--phase owned_materialize` — parses once with workspace reuse and
//!   then measures the batch-style cost of cloning `CstTables` and
//!   materialising diagnostics into an owned `ParseResult`. One-shot
//!   `parse_source` moves the final tables instead of cloning them.
//!
//! Convenience APIs (NOT parser-core baselines — they include fresh workspace
//! setup and owned result construction; malformed inputs may also build a
//! temporary source store for diagnostic locations):
//!
//! - `--phase parse_message_owned` — convenience `parse_message` call,
//!   freshly allocating a workspace and constructing an owned `ParseResult`
//!   every iteration.
//!
//! View / diagnostic / batch phases:
//!
//! - `--phase cst_view_traversal` — parse once, then iterate the full CST
//!   `--iterations` times to isolate traversal cost.
//! - `--phase diagnostics` — parse once, then iterate `DiagnosticView`
//!   N times (requires a malformed input).
//! - `--phase source_mapping` — parse once, then resolve every diagnostic
//!   span to line/column N times (requires a malformed input).
//! - `--phase parse_batch_session` — parser-core baseline: one
//!   `SourceStore`, one reused `ParseWorkspace`, `parse_source_session`
//!   over the corpus. NO owned materialisation. Use this when comparing
//!   parser-core throughput across many inputs.
//! - `--phase parse_batch_sequential` — `parse_batch` over a corpus with
//!   owned `ParseResult` per item (clone + diagnostic materialise). This
//!   is the owned batch API cost, not parser-core.
//!
//! Switches that change parser semantics or measured cost:
//!
//! - `--iterations N` — repeats the inner work N times in-process.
//! - `--reuse-workspace` / `--no-reuse-workspace`
//! - `--reserve` / `--no-reserve` (calls `reserve_for_source_len` first)
//! - `--collect-trivia` / `--no-collect-trivia`
//! - `--parse-semantic`
//! - `--input <path>` / `--input-text <text>` / stdin
//! - `--corpus <dir>` — for `parse_batch_sequential`.

#![allow(
    clippy::struct_excessive_bools,
    clippy::field_reassign_with_default,
    clippy::if_same_then_else,
    clippy::single_match_else,
    clippy::manual_let_else,
    // run_allocations reports per-iteration averages; the precision loss
    // converting `usize` counts to `f64` is acceptable for a display value.
    clippy::cast_precision_loss
)]

use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use ox_mf2_parser::snapshot::view::ChildView;
use ox_mf2_parser::snapshot::{
    decode_snapshot, decode_snapshot_owned, parse_batch_result_to_snapshot,
    parse_result_to_snapshot, SnapshotOptions,
};
use ox_mf2_parser::{
    parse_batch, parse_message, parse_source, parse_source_session, BatchParseOptions, CstChild,
    CstNodeView, CstView, DiagnosticView, NodeId, ParseInput, ParseOptions, ParseWorkspace,
    SourceFileInput, SourceStore,
};

// `--phase allocations` routes every allocation in this binary through
// `stats_alloc` so the runner can report per-iteration alloc count / bytes
// for any parse path. Gated behind `bench-alloc` because wrapping the
// allocator is a non-trivial overhead for all the wall-clock phases.
#[cfg(feature = "bench-alloc")]
use stats_alloc::{Region, StatsAlloc, INSTRUMENTED_SYSTEM};
#[cfg(feature = "bench-alloc")]
use std::alloc::System;

#[cfg(feature = "bench-alloc")]
#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    ParseMessageOwned,
    ParseCst,
    ParseCstNoTrivia,
    LowerSemantic,
    OwnedMaterialize,
    CstViewTraversal,
    Diagnostics,
    SourceMapping,
    ParseBatchSession,
    ParseBatchSequential,
    Allocations,
    EncodeSnapshot,
    ParseCstAndEncodeSnapshot,
    DecodeSnapshot,
    DecodeSnapshotOwned,
    TraverseNodes,
    TraverseTokens,
    TraverseDiagnostics,
    ParseBatchToSnapshot,
    ParseBatchResultToSnapshot,
    SnapshotToBytesCopy,
}

#[derive(Debug, Default)]
struct Args {
    phase: Option<Phase>,
    iterations: usize,
    reuse_workspace: bool,
    reserve: bool,
    collect_trivia: Option<bool>,
    parse_semantic: bool,
    input_path: Option<PathBuf>,
    input_text: Option<String>,
    corpus_dir: Option<PathBuf>,
    print_result: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        iterations: 1,
        reuse_workspace: true,
        reserve: true,
        ..Args::default()
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--phase" => {
                let v = it.next().ok_or("missing --phase value")?;
                a.phase = Some(parse_phase(&v)?);
            }
            "--iterations" => {
                let v = it.next().ok_or("missing --iterations value")?;
                a.iterations = v.parse().map_err(|e| format!("iterations: {e}"))?;
            }
            "--reuse-workspace" => a.reuse_workspace = true,
            "--no-reuse-workspace" => a.reuse_workspace = false,
            "--reserve" => a.reserve = true,
            "--no-reserve" => a.reserve = false,
            "--collect-trivia" => a.collect_trivia = Some(true),
            "--no-collect-trivia" => a.collect_trivia = Some(false),
            "--parse-semantic" => a.parse_semantic = true,
            "--input" => a.input_path = Some(it.next().ok_or("missing --input value")?.into()),
            "--input-text" => a.input_text = Some(it.next().ok_or("missing --input-text value")?),
            "--corpus" => a.corpus_dir = Some(it.next().ok_or("missing --corpus value")?.into()),
            "--print-result" => a.print_result = true,
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(a)
}

fn parse_phase(name: &str) -> Result<Phase, String> {
    Ok(match name {
        "parse_message_owned" => Phase::ParseMessageOwned,
        "parse_cst" => Phase::ParseCst,
        "parse_cst_no_trivia" => Phase::ParseCstNoTrivia,
        "lower_semantic" => Phase::LowerSemantic,
        "owned_materialize" => Phase::OwnedMaterialize,
        "cst_view_traversal" => Phase::CstViewTraversal,
        "diagnostics" => Phase::Diagnostics,
        "source_mapping" => Phase::SourceMapping,
        "parse_batch_session" => Phase::ParseBatchSession,
        "parse_batch_sequential" => Phase::ParseBatchSequential,
        "allocations" => Phase::Allocations,
        "encode_snapshot" => Phase::EncodeSnapshot,
        "parse_cst_and_encode_snapshot" => Phase::ParseCstAndEncodeSnapshot,
        "decode_snapshot" => Phase::DecodeSnapshot,
        "decode_snapshot_owned" => Phase::DecodeSnapshotOwned,
        "traverse_nodes" => Phase::TraverseNodes,
        "traverse_tokens" => Phase::TraverseTokens,
        "traverse_diagnostics" => Phase::TraverseDiagnostics,
        "parse_batch_to_snapshot" => Phase::ParseBatchToSnapshot,
        "parse_batch_result_to_snapshot" => Phase::ParseBatchResultToSnapshot,
        "snapshot_to_bytes_copy" => Phase::SnapshotToBytesCopy,
        other => return Err(format!("unknown phase: {other}")),
    })
}

fn print_help() {
    println!("ox-mf2-bench — Phase 1 ox_mf2_parser benchmark CLI");
    println!();
    println!("Usage: ox-mf2-bench --phase <PHASE> [options]");
    println!();
    println!("Phases (parser-core baselines — compare against other parsers):");
    println!("  parse_cst_no_trivia          parser-core, borrowed result, trivia disabled");
    println!("  parse_cst                    parser-core, borrowed result, trivia enabled");
    println!();
    println!("Phases (optional / downstream cost — include alongside a baseline):");
    println!("  lower_semantic               parser-core + SemanticModel lowering");
    println!(
        "  owned_materialize            batch-style CstTables.clone + diagnostic materialise only"
    );
    println!();
    println!("Phases (convenience APIs — NOT parser-core, include extra setup):");
    println!("  parse_message_owned          parse_message → owned ParseResult (fresh sources/workspace)");
    println!();
    println!("Phases (view / diagnostic / batch):");
    println!("  cst_view_traversal           parse once, traverse CST N times");
    println!("  diagnostics                  parse once, iterate DiagnosticView N times");
    println!(
        "  source_mapping               parse once, resolve every diagnostic span to line/col"
    );
    println!("  parse_batch_session          parser-core: one SourceStore + one ParseWorkspace,");
    println!("                                borrowed parse_source_session over --corpus");
    println!(
        "  parse_batch_sequential       owned parse_batch over --corpus (clone + materialise)"
    );
    println!();
    println!("Phases (Phase 2 Binary AST snapshot):");
    println!("  encode_snapshot              parse once, encode snapshot N times");
    println!("  parse_cst_and_encode_snapshot parse + encode each iteration (combined)");
    println!("  decode_snapshot              encode once, borrowed decode N times");
    println!("  decode_snapshot_owned        encode once, owned (Arc) decode N times");
    println!("  traverse_nodes               decode once, walk every node N times");
    println!("  traverse_tokens              decode once, walk every token N times");
    println!(
        "  traverse_diagnostics         decode once, iterate diagnostics N times (malformed input)"
    );
    println!("  parse_batch_to_snapshot      parse + shared batch snapshot over --corpus");
    println!(
        "  parse_batch_result_to_snapshot batch parse once, snapshot N times over --corpus"
    );
    println!("  snapshot_to_bytes_copy       encode once, copy bytes into Arc<[u8]> N times");
    println!();
    println!("Phases (allocator inspection — requires `--features bench-alloc` build):");
    println!("  allocations                  report alloc count + bytes per parse iteration");
    println!();
    println!("Options:");
    println!("  --iterations N               inner repeat count (default 1)");
    println!("  --reuse-workspace            keep one workspace across iterations (default)");
    println!("  --no-reuse-workspace");
    println!("  --reserve                    pre-reserve via reserve_for_source_len (default)");
    println!("  --no-reserve");
    println!("  --collect-trivia             override default trivia collection (default true)");
    println!("  --no-collect-trivia");
    println!("  --parse-semantic             enable optional semantic lowering");
    println!("  --input <path>               read input from file");
    println!("  --input-text <str>           inline input");
    println!("  --corpus <dir>               directory of .mf2 files for batch phase");
    println!("  --print-result               print summary numbers to stdout");
}

fn read_input(args: &Args) -> Result<String, String> {
    if let Some(text) = &args.input_text {
        return Ok(text.clone());
    }
    if let Some(path) = &args.input_path {
        return fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()));
    }
    let mut s = String::new();
    io::stdin()
        .read_to_string(&mut s)
        .map_err(|e| e.to_string())?;
    Ok(s)
}

fn read_corpus(dir: &PathBuf) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    let entries = fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "mf2") {
            let text =
                fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
            out.push((path.display().to_string(), text));
        }
    }
    Ok(out)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("ox-mf2-bench: {e}\nTry --help.");
            return ExitCode::from(2);
        }
    };
    let phase = match args.phase {
        Some(p) => p,
        None => {
            eprintln!("ox-mf2-bench: --phase is required");
            return ExitCode::from(2);
        }
    };
    let start = Instant::now();
    let result = match phase {
        Phase::ParseMessageOwned => run_parse_message_owned(&args),
        Phase::ParseCst => run_parse_cst(&args, true),
        Phase::ParseCstNoTrivia => run_parse_cst(&args, false),
        Phase::LowerSemantic => run_lower_semantic(&args),
        Phase::OwnedMaterialize => run_owned_materialize(&args),
        Phase::CstViewTraversal => run_cst_view_traversal(&args),
        Phase::Diagnostics => run_diagnostics(&args),
        Phase::SourceMapping => run_source_mapping(&args),
        Phase::ParseBatchSession => run_parse_batch_session(&args),
        Phase::ParseBatchSequential => run_parse_batch_sequential(&args),
        Phase::Allocations => run_allocations(&args),
        Phase::EncodeSnapshot => run_encode_snapshot(&args),
        Phase::ParseCstAndEncodeSnapshot => run_parse_cst_and_encode_snapshot(&args),
        Phase::DecodeSnapshot => run_decode_snapshot(&args),
        Phase::DecodeSnapshotOwned => run_decode_snapshot_owned(&args),
        Phase::TraverseNodes => run_traverse_nodes(&args),
        Phase::TraverseTokens => run_traverse_tokens(&args),
        Phase::TraverseDiagnostics => run_traverse_diagnostics(&args),
        Phase::ParseBatchToSnapshot => run_parse_batch_to_snapshot(&args),
        Phase::ParseBatchResultToSnapshot => run_parse_batch_result_to_snapshot(&args),
        Phase::SnapshotToBytesCopy => run_snapshot_to_bytes_copy(&args),
    };
    match result {
        Ok(summary) => {
            if args.print_result {
                println!(
                    "{} iterations: {} elapsed: {:?}",
                    summary.iterations,
                    summary.work_units,
                    start.elapsed()
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("ox-mf2-bench: {e}");
            ExitCode::from(1)
        }
    }
}

struct PhaseSummary {
    iterations: usize,
    work_units: usize,
}

fn run_parse_message_owned(args: &Args) -> Result<PhaseSummary, String> {
    let input = read_input(args)?;
    let iters = args.iterations.max(1);
    let mut count = 0usize;
    for _ in 0..iters {
        let r = parse_message(&input);
        count += r.cst.node_count();
    }
    Ok(PhaseSummary {
        iterations: iters,
        work_units: count,
    })
}

fn options_for(args: &Args, default_trivia: bool, parse_semantic: bool) -> ParseOptions {
    let mut o = ParseOptions::default();
    o.collect_trivia = args.collect_trivia.unwrap_or(default_trivia);
    o.parse_semantic = parse_semantic || args.parse_semantic;
    o
}

fn run_parse_cst(args: &Args, default_trivia: bool) -> Result<PhaseSummary, String> {
    let input = read_input(args)?;
    let iters = args.iterations.max(1);
    let options = options_for(args, default_trivia, false);
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source: &input,
        ..Default::default()
    });

    let mut workspace = if args.reuse_workspace {
        ParseWorkspace::new()
    } else {
        ParseWorkspace::new()
    };
    if args.reserve {
        workspace.reserve_for_source_len(input.len());
    }

    let mut total_nodes = 0usize;
    for _ in 0..iters {
        if !args.reuse_workspace {
            workspace = ParseWorkspace::new();
            if args.reserve {
                workspace.reserve_for_source_len(input.len());
            }
        }
        let session = parse_source_session(&sources, id, &mut workspace, options);
        total_nodes += session.cst.tables().node_count();
    }
    Ok(PhaseSummary {
        iterations: iters,
        work_units: total_nodes,
    })
}

fn run_lower_semantic(args: &Args) -> Result<PhaseSummary, String> {
    let input = read_input(args)?;
    let iters = args.iterations.max(1);
    let options = options_for(args, true, true);
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source: &input,
        ..Default::default()
    });
    let mut workspace = ParseWorkspace::new();
    if args.reserve {
        workspace.reserve_for_source_len(input.len());
    }
    let mut units = 0usize;
    for _ in 0..iters {
        let session = parse_source_session(&sources, id, &mut workspace, options);
        if let Some(s) = session.semantic {
            units += s.references().len() + s.expressions().len();
        }
    }
    Ok(PhaseSummary {
        iterations: iters,
        work_units: units,
    })
}

/// Measure the batch-style cost of cloning `CstTables` and materialising
/// diagnostics into an owned `ParseResult`, with the parse itself amortised
/// outside the loop.
///
/// One-shot `parse_source` / `parse_message` move the final CST tables out of
/// their disposable workspace. `parse_batch` still keeps one workspace alive
/// across items, so it must clone table data into each owned result.
fn run_owned_materialize(args: &Args) -> Result<PhaseSummary, String> {
    let input = read_input(args)?;
    let iters = args.iterations.max(1);
    let options = options_for(args, true, false);
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source: &input,
        ..Default::default()
    });
    let mut workspace = ParseWorkspace::new();
    if args.reserve {
        workspace.reserve_for_source_len(input.len());
    }
    let session = parse_source_session(&sources, id, &mut workspace, options);
    // Clone what the reusable-workspace owned path materialises into a
    // `ParseResult` once, outside the measured loop.
    let baseline_cst = session.cst.tables().clone();
    let baseline_diags: Vec<_> = session.diagnostics.iter().collect();
    let mut units = 0usize;
    // Per iteration: re-clone the tables and re-materialise the diagnostic
    // list. This is the cost that reusable-workspace owned APIs pay on top
    // of the borrowed-session path.
    for _ in 0..iters {
        let cst = baseline_cst.clone();
        let diags = baseline_diags.clone();
        units += cst.node_count() + diags.len();
    }
    Ok(PhaseSummary {
        iterations: iters,
        work_units: units,
    })
}

fn run_cst_view_traversal(args: &Args) -> Result<PhaseSummary, String> {
    let input = read_input(args)?;
    let iters = args.iterations.max(1);
    let options = options_for(args, true, false);
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source: &input,
        ..Default::default()
    });
    let result = parse_source(&sources, id, options);
    let view = CstView::new(&sources, id, &result.cst);
    let mut nodes = 0usize;
    for _ in 0..iters {
        if let Some(root) = view.root() {
            walk(root, &mut nodes);
        }
    }
    Ok(PhaseSummary {
        iterations: iters,
        work_units: nodes,
    })
}

fn walk(node: CstNodeView<'_>, count: &mut usize) {
    *count += 1;
    for child in node.children() {
        if let CstChild::Node(n) = child {
            walk(n, count);
        }
    }
}

fn run_diagnostics(args: &Args) -> Result<PhaseSummary, String> {
    let input = read_input(args)?;
    let iters = args.iterations.max(1);
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source: &input,
        ..Default::default()
    });
    // Parse ONCE so the loop measures DiagnosticView iteration only — not
    // parse cost. Use a borrowed session so the same workspace backs the
    // view across iterations.
    let mut workspace = ParseWorkspace::new();
    workspace.reserve_for_source_len(input.len());
    let session = parse_source_session(&sources, id, &mut workspace, ParseOptions::default());
    if session.diagnostics.is_empty() {
        return Err("diagnostics phase requires an input that produces \
                    at least one diagnostic; pass a malformed --input-text \
                    such as 'Hello, {$name' or '{$x:number}'"
            .to_string());
    }
    let view: DiagnosticView<'_> = session.diagnostics;
    let mut units = 0usize;
    for _ in 0..iters {
        // Iterate every record AND materialise the public Diagnostic for
        // each one so message + location resolution is part of the
        // measurement.
        for d in view.iter() {
            units += d.location.line as usize + d.location.column as usize;
        }
    }
    if units == 0 {
        return Err("diagnostics phase ran zero work units; check the input".to_string());
    }
    Ok(PhaseSummary {
        iterations: iters,
        work_units: units,
    })
}

fn run_source_mapping(args: &Args) -> Result<PhaseSummary, String> {
    let input = read_input(args)?;
    let iters = args.iterations.max(1);
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source: &input,
        ..Default::default()
    });
    let result = parse_source(&sources, id, ParseOptions::default());
    // The phase measures `SourceStore::location` on diagnostic spans. A valid
    // input has no diagnostics, so the inner loop would do zero work and the
    // hyperfine number would be meaningless — refuse the run instead.
    if result.diagnostics.is_empty() {
        return Err("source_mapping phase requires at least one diagnostic; \
                    pass a malformed --input-text such as 'Hello, {$name'"
            .to_string());
    }
    let mut units = 0usize;
    for _ in 0..iters {
        for d in &result.diagnostics {
            let loc = sources.location(d.source, d.span);
            units += loc.line as usize + loc.column as usize;
        }
    }
    if units == 0 {
        return Err("source_mapping phase ran zero work units; check the input".to_string());
    }
    Ok(PhaseSummary {
        iterations: iters,
        work_units: units,
    })
}

/// Allocator-focused measurement. Parses the input N times with workspace
/// reuse and reports both the aggregate alloc count / byte total and the
/// per-iteration averages, so it is obvious whether the steady-state cost
/// of one parse is zero allocations (P3-P8 ideal) or some small number.
///
/// Honours `--collect-trivia` / `--no-collect-trivia` / `--parse-semantic`
/// the same way the timing phases do: any combination is measurable so
/// regression checks can target the exact parse path that changed (for
/// example "did adding a semantic-lowering optimisation introduce a hidden
/// allocation per call?").
///
/// Requires the `bench-alloc` feature so the global allocator is wrapped
/// in `stats_alloc::INSTRUMENTED_SYSTEM`. Without that feature the phase
/// returns a descriptive error explaining how to rebuild.
#[cfg(feature = "bench-alloc")]
fn run_allocations(args: &Args) -> Result<PhaseSummary, String> {
    let input = read_input(args)?;
    let iters = args.iterations.max(1);
    // Same plumbing as the parser-core timing phases so the measured
    // allocations match the path under test, not a hard-coded default.
    let options = options_for(args, true, false);
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source: &input,
        ..Default::default()
    });
    let mut workspace = ParseWorkspace::new();
    if args.reserve {
        workspace.reserve_for_source_len(input.len());
    }
    // Warm the workspace so the first iteration's lazy growth is not
    // attributed to the steady state.
    let _ = parse_source_session(&sources, id, &mut workspace, options);

    let region = Region::new(GLOBAL);
    let mut total_nodes = 0usize;
    for _ in 0..iters {
        let session = parse_source_session(&sources, id, &mut workspace, options);
        total_nodes += session.cst.tables().node_count();
    }
    let stats = region.change();
    // Net = allocations - deallocations gives the steady-state retained
    // bytes (should hover at 0 when workspace reuse is healthy). Bytes
    // allocated is the gross throughput through the allocator.
    let net_bytes = stats.bytes_allocated as i64 - stats.bytes_deallocated as i64;
    eprintln!(
        "allocations: iters={iters} collect_trivia={ct} parse_semantic={ps} \
         alloc_calls={alloc} dealloc_calls={dealloc} \
         bytes_allocated={bytes_alloc} bytes_deallocated={bytes_dealloc} \
         net_bytes={net} alloc_per_iter={apk:.2} bytes_per_iter={bpk:.2}",
        ct = options.collect_trivia,
        ps = options.parse_semantic,
        alloc = stats.allocations,
        dealloc = stats.deallocations,
        bytes_alloc = stats.bytes_allocated,
        bytes_dealloc = stats.bytes_deallocated,
        net = net_bytes,
        apk = stats.allocations as f64 / iters as f64,
        bpk = stats.bytes_allocated as f64 / iters as f64,
    );
    Ok(PhaseSummary {
        iterations: iters,
        work_units: total_nodes,
    })
}

#[cfg(not(feature = "bench-alloc"))]
fn run_allocations(_args: &Args) -> Result<PhaseSummary, String> {
    Err(
        "allocations phase requires `--features bench-alloc`; rebuild with \
         `cargo build --release -p ox_mf2_parser --bin ox-mf2-bench --features bench-alloc`"
            .to_string(),
    )
}

/// Parser-core batch baseline. Builds one [`SourceStore`] from `--corpus`,
/// keeps a single [`ParseWorkspace`] alive for the entire run, and walks
/// each input through [`parse_source_session`] — the same primitive that
/// `parse_batch` calls internally, but WITHOUT the owned `ParseResult`
/// clone + diagnostic materialisation that turns it into the public batch
/// API. Use this when comparing ox-mf2 parser-core throughput against
/// other parsers' minimal-cost paths over many small messages.
fn run_parse_batch_session(args: &Args) -> Result<PhaseSummary, String> {
    let dir = args
        .corpus_dir
        .as_ref()
        .ok_or("parse_batch_session requires --corpus <dir>")?;
    let corpus = read_corpus(dir)?;
    let iters = args.iterations.max(1);
    let options = options_for(args, true, false);
    let mut sources = SourceStore::new();
    let mut ids = Vec::with_capacity(corpus.len());
    let mut max_source_len = 0usize;
    for (path, text) in &corpus {
        let id = sources.add(SourceFileInput {
            source: text,
            path: Some(path),
            ..Default::default()
        });
        ids.push(id);
        if text.len() > max_source_len {
            max_source_len = text.len();
        }
    }
    let mut workspace = ParseWorkspace::new();
    if args.reserve {
        // Reserve once for the largest input in the corpus so the workspace
        // does not regrow across rounds.
        workspace.reserve_for_source_len(max_source_len);
    }
    let mut units = 0usize;
    for _ in 0..iters {
        for &id in &ids {
            let session = parse_source_session(&sources, id, &mut workspace, options);
            units += session.cst.tables().node_count();
        }
    }
    Ok(PhaseSummary {
        iterations: iters,
        work_units: units,
    })
}

fn run_parse_batch_sequential(args: &Args) -> Result<PhaseSummary, String> {
    let dir = args
        .corpus_dir
        .as_ref()
        .ok_or("parse_batch_sequential requires --corpus <dir>")?;
    let corpus = read_corpus(dir)?;
    let iters = args.iterations.max(1);
    let inputs: Vec<_> = corpus
        .iter()
        .map(|(path, text)| ParseInput {
            source: text,
            path: Some(path),
            ..Default::default()
        })
        .collect();
    let mut units = 0usize;
    for _ in 0..iters {
        let result = parse_batch(&inputs, BatchParseOptions::default());
        units += result
            .items
            .iter()
            .map(|i| i.result.cst.node_count())
            .sum::<usize>();
    }
    Ok(PhaseSummary {
        iterations: iters,
        work_units: units,
    })
}

// ── Phase 2 Binary AST snapshot phases ───────────────────────────────

fn snapshot_options(args: &Args) -> SnapshotOptions {
    let mut opts = SnapshotOptions::default();
    opts.include_trivia = args.collect_trivia.unwrap_or(true);
    opts
}

fn parse_for_snapshot(args: &Args) -> Result<(SourceStore, ox_mf2_parser::ParseResult), String> {
    let input = read_input(args)?;
    let mut sources = SourceStore::new();
    let id = sources.add(SourceFileInput {
        source: &input,
        ..Default::default()
    });
    let options = options_for(args, true, false);
    let result = parse_source(&sources, id, options);
    Ok((sources, result))
}

fn run_encode_snapshot(args: &Args) -> Result<PhaseSummary, String> {
    let (sources, result) = parse_for_snapshot(args)?;
    let snap_opts = snapshot_options(args);
    let iters = args.iterations.max(1);
    let mut bytes_total = 0usize;
    for _ in 0..iters {
        let snap = parse_result_to_snapshot(&sources, &result, snap_opts)
            .map_err(|e| format!("snapshot encode failed: {e}"))?;
        bytes_total += snap.bytes.len();
    }
    Ok(PhaseSummary {
        iterations: iters,
        work_units: bytes_total,
    })
}

fn run_parse_cst_and_encode_snapshot(args: &Args) -> Result<PhaseSummary, String> {
    let input = read_input(args)?;
    let snap_opts = snapshot_options(args);
    let parse_opts = options_for(args, true, false);
    let iters = args.iterations.max(1);
    let mut bytes_total = 0usize;
    for _ in 0..iters {
        let mut sources = SourceStore::new();
        let id = sources.add(SourceFileInput {
            source: &input,
            ..Default::default()
        });
        let result = parse_source(&sources, id, parse_opts);
        let snap = parse_result_to_snapshot(&sources, &result, snap_opts)
            .map_err(|e| format!("snapshot encode failed: {e}"))?;
        bytes_total += snap.bytes.len();
    }
    Ok(PhaseSummary {
        iterations: iters,
        work_units: bytes_total,
    })
}

fn run_decode_snapshot(args: &Args) -> Result<PhaseSummary, String> {
    let (sources, result) = parse_for_snapshot(args)?;
    let snap = parse_result_to_snapshot(&sources, &result, snapshot_options(args))
        .map_err(|e| format!("snapshot encode failed: {e}"))?;
    let iters = args.iterations.max(1);
    let mut sections = 0usize;
    for _ in 0..iters {
        let view = decode_snapshot(&snap.bytes).map_err(|e| format!("decode failed: {e}"))?;
        sections += view.section_count_for_bench();
    }
    Ok(PhaseSummary {
        iterations: iters,
        work_units: sections,
    })
}

fn run_decode_snapshot_owned(args: &Args) -> Result<PhaseSummary, String> {
    let (sources, result) = parse_for_snapshot(args)?;
    let snap = parse_result_to_snapshot(&sources, &result, snapshot_options(args))
        .map_err(|e| format!("snapshot encode failed: {e}"))?;
    let arc: std::sync::Arc<[u8]> = snap.bytes.into();
    let iters = args.iterations.max(1);
    let mut sections = 0usize;
    for _ in 0..iters {
        let view =
            decode_snapshot_owned(arc.clone()).map_err(|e| format!("decode failed: {e}"))?;
        sections += view.view().section_count_for_bench();
    }
    Ok(PhaseSummary {
        iterations: iters,
        work_units: sections,
    })
}

fn run_traverse_nodes(args: &Args) -> Result<PhaseSummary, String> {
    let (sources, result) = parse_for_snapshot(args)?;
    let snap = parse_result_to_snapshot(&sources, &result, snapshot_options(args))
        .map_err(|e| format!("snapshot encode failed: {e}"))?;
    let view = decode_snapshot(&snap.bytes).map_err(|e| format!("decode failed: {e}"))?;
    let iters = args.iterations.max(1);
    let mut units = 0usize;
    for _ in 0..iters {
        for i in 0..view.node_count() {
            let node = view.node(NodeId::new(i)).expect("valid node");
            // Touch span + kind so the loop is not eliminated.
            units += node.span().len() as usize + node.kind() as u16 as usize;
            for child in node.children() {
                match child {
                    ChildView::Node(n) => units += n.id().raw() as usize,
                    ChildView::Token(t) => units += t.id().raw() as usize,
                }
            }
        }
    }
    Ok(PhaseSummary {
        iterations: iters,
        work_units: units,
    })
}

fn run_traverse_tokens(args: &Args) -> Result<PhaseSummary, String> {
    let (sources, result) = parse_for_snapshot(args)?;
    let snap = parse_result_to_snapshot(&sources, &result, snapshot_options(args))
        .map_err(|e| format!("snapshot encode failed: {e}"))?;
    let view = decode_snapshot(&snap.bytes).map_err(|e| format!("decode failed: {e}"))?;
    let iters = args.iterations.max(1);
    let mut units = 0usize;
    for _ in 0..iters {
        for i in 0..view.token_count() {
            let token = view
                .token(ox_mf2_parser::TokenId::new(i))
                .expect("valid token");
            units +=
                token.span().len() as usize + token.leading_trivia().count() + token.trailing_trivia().count();
        }
    }
    Ok(PhaseSummary {
        iterations: iters,
        work_units: units,
    })
}

fn run_traverse_diagnostics(args: &Args) -> Result<PhaseSummary, String> {
    let (sources, result) = parse_for_snapshot(args)?;
    let snap = parse_result_to_snapshot(&sources, &result, snapshot_options(args))
        .map_err(|e| format!("snapshot encode failed: {e}"))?;
    let view = decode_snapshot(&snap.bytes).map_err(|e| format!("decode failed: {e}"))?;
    let iters = args.iterations.max(1);
    if view.diagnostic_count() == 0 {
        eprintln!(
            "ox-mf2-bench: traverse_diagnostics measured 0 diagnostics — supply a malformed --input"
        );
    }
    let mut units = 0usize;
    for _ in 0..iters {
        for i in 0..view.diagnostic_count() {
            let d = view.diagnostic(i).expect("valid diag");
            units += d.span().len() as usize + d.labels().count();
        }
    }
    Ok(PhaseSummary {
        iterations: iters,
        work_units: units,
    })
}

fn run_parse_batch_to_snapshot(args: &Args) -> Result<PhaseSummary, String> {
    let dir = args
        .corpus_dir
        .as_ref()
        .ok_or("parse_batch_to_snapshot requires --corpus <dir>")?;
    let corpus = read_corpus(dir)?;
    let snap_opts = snapshot_options(args);
    let inputs: Vec<_> = corpus
        .iter()
        .map(|(path, text)| ParseInput {
            source: text,
            path: Some(path),
            ..Default::default()
        })
        .collect();
    let iters = args.iterations.max(1);
    let mut bytes_total = 0usize;
    for _ in 0..iters {
        let snap = ox_mf2_parser::snapshot::parse_batch_to_snapshot(
            &inputs,
            BatchParseOptions::default(),
            snap_opts,
        )
        .map_err(|e| format!("batch snapshot failed: {e}"))?;
        bytes_total += snap.bytes.len();
    }
    Ok(PhaseSummary {
        iterations: iters,
        work_units: bytes_total,
    })
}

fn run_parse_batch_result_to_snapshot(args: &Args) -> Result<PhaseSummary, String> {
    let dir = args
        .corpus_dir
        .as_ref()
        .ok_or("parse_batch_result_to_snapshot requires --corpus <dir>")?;
    let corpus = read_corpus(dir)?;
    let snap_opts = snapshot_options(args);
    let inputs: Vec<_> = corpus
        .iter()
        .map(|(path, text)| ParseInput {
            source: text,
            path: Some(path),
            ..Default::default()
        })
        .collect();
    let batch = parse_batch(&inputs, BatchParseOptions::default());
    let iters = args.iterations.max(1);
    let mut bytes_total = 0usize;
    for _ in 0..iters {
        let snap = parse_batch_result_to_snapshot(&batch, snap_opts)
            .map_err(|e| format!("batch snapshot failed: {e}"))?;
        bytes_total += snap.bytes.len();
    }
    Ok(PhaseSummary {
        iterations: iters,
        work_units: bytes_total,
    })
}

fn run_snapshot_to_bytes_copy(args: &Args) -> Result<PhaseSummary, String> {
    let (sources, result) = parse_for_snapshot(args)?;
    let snap = parse_result_to_snapshot(&sources, &result, snapshot_options(args))
        .map_err(|e| format!("snapshot encode failed: {e}"))?;
    let arc: std::sync::Arc<[u8]> = snap.bytes.into();
    let iters = args.iterations.max(1);
    let mut bytes_total = 0usize;
    for _ in 0..iters {
        // Realistic transfer / persistence path: deep copy the snapshot
        // bytes into a fresh `Arc<[u8]>` so the inner Vec is freed.
        let copy: std::sync::Arc<[u8]> = (*arc).to_vec().into();
        bytes_total += copy.len();
    }
    Ok(PhaseSummary {
        iterations: iters,
        work_units: bytes_total,
    })
}

// Tiny helper so the bench can report a section count without
// exposing it as a stable public API on `SnapshotView` (the field
// list lives on `SectionIndex`).
trait BenchSectionExt {
    fn section_count_for_bench(&self) -> usize;
}

impl BenchSectionExt for ox_mf2_parser::SnapshotView<'_> {
    fn section_count_for_bench(&self) -> usize {
        let s = self.sections();
        let mut n = 7;
        n += usize::from(s.trivia.is_some());
        n += usize::from(s.diagnostics.is_some());
        n += usize::from(s.diagnostic_labels.is_some());
        n += usize::from(s.source_text_data.is_some());
        n += usize::from(s.extended_data.is_some());
        n
    }
}
