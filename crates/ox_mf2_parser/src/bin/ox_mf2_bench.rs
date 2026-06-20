//! `ox-mf2-bench` — benchmark harness CLI.
//!
//! Designed for use with `hyperfine` and the `tools/mf-parser-bench`
//! orchestrator. Each invocation does ONE thing so that benchmark numbers
//! never mix unrelated phases:
//!
//! - `--phase parse_message_owned` — convenience parse path that materialises
//!   an owned `ParseResult`.
//! - `--phase parse_cst` — `parse_source_session` with workspace reuse,
//!   borrowed result, default `collect_trivia = true`.
//! - `--phase parse_cst_no_trivia` — same but `collect_trivia = false`.
//! - `--phase lower_semantic` — `parse_source_session` with
//!   `parse_semantic = true`.
//! - `--phase cst_view_traversal` — parse once, then iterate the full CST
//!   `--iterations` times to isolate traversal cost.
//! - `--phase diagnostics` — re-emits diagnostics through `DiagnosticView`.
//! - `--phase source_mapping` — converts every diagnostic span to
//!   line/column via `SourceStore`.
//! - `--phase parse_batch_sequential` — runs `parse_batch` over a corpus.
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
    clippy::manual_let_else
)]

use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use ox_mf2_parser::{
    parse_batch, parse_message, parse_source, parse_source_session, BatchParseOptions, CstChild,
    CstNodeView, CstView, DiagnosticView, ParseInput, ParseOptions, ParseWorkspace,
    SourceFileInput, SourceStore,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    ParseMessageOwned,
    ParseCst,
    ParseCstNoTrivia,
    LowerSemantic,
    CstViewTraversal,
    Diagnostics,
    SourceMapping,
    ParseBatchSequential,
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
        "cst_view_traversal" => Phase::CstViewTraversal,
        "diagnostics" => Phase::Diagnostics,
        "source_mapping" => Phase::SourceMapping,
        "parse_batch_sequential" => Phase::ParseBatchSequential,
        other => return Err(format!("unknown phase: {other}")),
    })
}

fn print_help() {
    println!("ox-mf2-bench — Phase 1 ox_mf2_parser benchmark CLI");
    println!();
    println!("Usage: ox-mf2-bench --phase <PHASE> [options]");
    println!();
    println!("Phases:");
    println!("  parse_message_owned          parse_message → owned ParseResult");
    println!("  parse_cst                    parse_source_session (workspace + borrowed view)");
    println!("  parse_cst_no_trivia          parse_cst with --no-collect-trivia");
    println!("  lower_semantic               parse_source_session with --parse-semantic");
    println!("  cst_view_traversal           parse once, traverse CST N times");
    println!("  diagnostics                  parse once, iterate DiagnosticView N times");
    println!("  source_mapping               parse once, resolve every diagnostic span to line/col");
    println!("  parse_batch_sequential       parse_batch over --corpus <dir>/*.mf2");
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
    io::stdin().read_to_string(&mut s).map_err(|e| e.to_string())?;
    Ok(s)
}

fn read_corpus(dir: &PathBuf) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    let entries = fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "mf2") {
            let text = fs::read_to_string(&path)
                .map_err(|e| format!("read {}: {e}", path.display()))?;
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
        Phase::CstViewTraversal => run_cst_view_traversal(&args),
        Phase::Diagnostics => run_diagnostics(&args),
        Phase::SourceMapping => run_source_mapping(&args),
        Phase::ParseBatchSequential => run_parse_batch_sequential(&args),
    };
    match result {
        Ok(summary) => {
            if args.print_result {
                println!(
                    "{} iterations: {} elapsed: {:?}",
                    summary.iterations, summary.work_units, start.elapsed()
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
    // If the input is valid, fall back to mapping every token's span so the
    // phase always does meaningful work and isn't an empty loop.
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
        units += result.items.iter().map(|i| i.result.cst.node_count()).sum::<usize>();
    }
    Ok(PhaseSummary {
        iterations: iters,
        work_units: units,
    })
}
