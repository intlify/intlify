use std::env;
use std::fs;
use std::hint::black_box;
use std::mem::size_of_val;
use std::path::PathBuf;
use std::process;
use std::time::Instant;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct Corpus {
    name: String,
    format: String,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    id: String,
    source: String,
    expected: String,
    unsupported_by: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Summary {
    target: String,
    runtime: &'static str,
    format: String,
    corpus: String,
    case_count: usize,
    input_bytes: usize,
    iterations: usize,
    total_parses: usize,
    checksum: u64,
    elapsed_ms: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreflightReport {
    target: String,
    runtime: &'static str,
    format: String,
    corpus: String,
    results: Vec<PreflightCase>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PreflightCase {
    id: String,
    expected: String,
    status: String,
    diagnostics: usize,
    error: Option<String>,
}

#[derive(Debug)]
struct TargetResult {
    checksum: u64,
    diagnostics: usize,
}

#[derive(Debug, Default)]
struct Args {
    target: Option<String>,
    corpus: Option<String>,
    cases_dir: Option<PathBuf>,
    iterations: usize,
    summary_json: Option<PathBuf>,
    preflight: bool,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = parse_args(env::args().skip(1))?;
    let target = args
        .target
        .as_deref()
        .ok_or_else(|| "Missing required option --target".to_string())?;
    let corpus_name = args
        .corpus
        .as_deref()
        .ok_or_else(|| "Missing required option --corpus".to_string())?;

    let corpus = read_corpus(args.cases_dir.as_ref(), corpus_name)?;
    if corpus.format != "mf2" {
        return Err(format!(
            "Rust targets only support mf2 corpus, got {}",
            corpus.format
        ));
    }

    if args.preflight {
        let report = run_preflight(target, &corpus);
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
        );
        return Ok(());
    }

    let cases = select_benchmark_cases(&corpus, target);
    if cases.is_empty() {
        return Err(format!("No benchmark cases for {target} / {}", corpus.name));
    }

    let session_inputs = if target == "ox-mf2-parse-session-no-trivia" {
        Some(prepare_ox_mf2_session_inputs(&cases))
    } else {
        None
    };

    let started = Instant::now();
    let checksum = if let Some((sources, source_ids, max_source_len)) = session_inputs {
        run_ox_mf2_parse_session_no_trivia(&sources, &source_ids, max_source_len, args.iterations)
    } else {
        run_generic_target_loop(target, &cases, args.iterations)?
    };

    let input_bytes = cases.iter().map(|case| case.source.len()).sum();
    let summary = Summary {
        target: target.to_string(),
        runtime: "rust",
        format: corpus.format.clone(),
        corpus: corpus.name.clone(),
        case_count: cases.len(),
        input_bytes,
        iterations: args.iterations,
        total_parses: cases.len() * args.iterations,
        checksum,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
    };

    if let Some(path) = args.summary_json {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let json = serde_json::to_string_pretty(&summary).map_err(|error| error.to_string())?;
        fs::write(path, format!("{json}\n")).map_err(|error| error.to_string())?;
    }

    println!("checksum={checksum}");
    Ok(())
}

fn run_preflight(target: &str, corpus: &Corpus) -> PreflightReport {
    let results = corpus
        .cases
        .iter()
        .map(|case| {
            if case.unsupported_by.iter().any(|name| name == target) {
                return PreflightCase {
                    id: case.id.clone(),
                    expected: case.expected.clone(),
                    status: "unsupported".to_string(),
                    diagnostics: 0,
                    error: None,
                };
            }

            match run_target(target, &case.source) {
                Ok(result) if result.diagnostics == 0 => PreflightCase {
                    id: case.id.clone(),
                    expected: case.expected.clone(),
                    status: "parse-ok".to_string(),
                    diagnostics: 0,
                    error: None,
                },
                Ok(result) => PreflightCase {
                    id: case.id.clone(),
                    expected: case.expected.clone(),
                    status: "parse-error".to_string(),
                    diagnostics: result.diagnostics,
                    error: None,
                },
                Err(error) => PreflightCase {
                    id: case.id.clone(),
                    expected: case.expected.clone(),
                    status: "parse-error".to_string(),
                    diagnostics: 1,
                    error: Some(error),
                },
            }
        })
        .collect();

    PreflightReport {
        target: target.to_string(),
        runtime: "rust",
        format: corpus.format.clone(),
        corpus: corpus.name.clone(),
        results,
    }
}

fn run_target(target: &str, source: &str) -> Result<TargetResult, String> {
    match target {
        "ox-content-parse" => {
            let message = ox_content_i18n::mf2::parse(black_box(source))
                .map_err(|error| error.to_string())?;
            Ok(TargetResult {
                checksum: size_of_val(black_box(&message)) as u64,
                diagnostics: 0,
            })
        }
        "ox-content-parse-and-validate" => {
            let (message, errors) = ox_content_i18n::mf2::parse_and_validate(black_box(source))
                .map_err(|error| error.to_string())?;
            Ok(TargetResult {
                checksum: size_of_val(black_box(&message)) as u64,
                diagnostics: errors.len(),
            })
        }
        "mf2-tools-parse" => {
            let (message, diagnostics, source_text_info) = mf2_parser::parse(black_box(source));
            Ok(TargetResult {
                checksum: (size_of_val(black_box(&message))
                    + size_of_val(black_box(&source_text_info))) as u64,
                diagnostics: diagnostics.len(),
            })
        }
        "mf2-tools-parse-and-analyze" => {
            let (message, mut diagnostics, source_text_info) = mf2_parser::parse(black_box(source));
            let scope = mf2_parser::analyze_semantics(&message, &mut diagnostics);
            Ok(TargetResult {
                checksum: (size_of_val(black_box(&message))
                    + size_of_val(black_box(&source_text_info))
                    + size_of_val(black_box(&scope))) as u64,
                diagnostics: diagnostics.len(),
            })
        }
        "ox-mf2-parse" => {
            // Phase 1 parser entry point equivalent to mf2-tools-parse — no
            // semantic lowering, default options.
            let result = ox_mf2_parser::parse_message(black_box(source));
            Ok(TargetResult {
                checksum: (size_of_val(black_box(&result.cst))
                    + result.cst.node_count() as usize
                    + result.cst.token_count() as usize) as u64,
                diagnostics: result.diagnostics.len(),
            })
        }
        "ox-mf2-parse-session-no-trivia" => {
            let mut options = ox_mf2_parser::ParseOptions::default();
            options.collect_trivia = false;
            let mut sources = ox_mf2_parser::SourceStore::new();
            let id = sources.add(ox_mf2_parser::SourceFileInput {
                source: black_box(source),
                ..Default::default()
            });
            let mut workspace = ox_mf2_parser::ParseWorkspace::new();
            workspace.reserve_for_source_len(source.len());
            let result = ox_mf2_parser::parse_source_session(&sources, id, &mut workspace, options);
            let tables = black_box(result.cst.tables());
            Ok(TargetResult {
                checksum: (tables.node_count() + tables.token_count()) as u64,
                diagnostics: result.diagnostics.len(),
            })
        }
        "ox-mf2-parse-and-lower" => {
            let mut options = ox_mf2_parser::ParseOptions::default();
            options.parse_semantic = true;
            let mut sources = ox_mf2_parser::SourceStore::new();
            let id = sources.add(ox_mf2_parser::SourceFileInput {
                source: black_box(source),
                ..Default::default()
            });
            let result = ox_mf2_parser::parse_source(&sources, id, options);
            let semantic_size = result
                .semantic
                .as_ref()
                .map(|s| s.declarations.len() + s.expressions.len() + s.patterns.len())
                .unwrap_or(0);
            Ok(TargetResult {
                checksum: (result.cst.node_count() + semantic_size) as u64,
                diagnostics: result.diagnostics.len(),
            })
        }
        _ => Err(format!("Unknown Rust parser target: {target}")),
    }
}

fn run_generic_target_loop(
    target: &str,
    cases: &[&Case],
    iterations: usize,
) -> Result<u64, String> {
    let mut checksum = 0_u64;

    for _ in 0..iterations {
        for case in cases {
            let result = run_target(target, &case.source)?;
            checksum = checksum
                .wrapping_add(result.checksum)
                .wrapping_add(case.source.len() as u64)
                .wrapping_add(result.diagnostics as u64);
        }
    }

    Ok(checksum)
}

fn prepare_ox_mf2_session_inputs(
    cases: &[&Case],
) -> (
    ox_mf2_parser::SourceStore,
    Vec<(ox_mf2_parser::SourceId, usize)>,
    usize,
) {
    let mut sources = ox_mf2_parser::SourceStore::with_capacity(cases.len());
    let mut source_ids = Vec::with_capacity(cases.len());
    let mut max_source_len = 0;

    for case in cases {
        let source_len = case.source.len();
        max_source_len = max_source_len.max(source_len);
        let id = sources.add(ox_mf2_parser::SourceFileInput {
            source: &case.source,
            ..Default::default()
        });
        source_ids.push((id, source_len));
    }

    (sources, source_ids, max_source_len)
}

fn run_ox_mf2_parse_session_no_trivia(
    sources: &ox_mf2_parser::SourceStore,
    source_ids: &[(ox_mf2_parser::SourceId, usize)],
    max_source_len: usize,
    iterations: usize,
) -> u64 {
    let mut options = ox_mf2_parser::ParseOptions::default();
    options.collect_trivia = false;

    let mut workspace = ox_mf2_parser::ParseWorkspace::new();
    workspace.reserve_for_source_len(max_source_len);

    let mut checksum = 0_u64;
    for _ in 0..iterations {
        for &(source_id, source_len) in source_ids {
            let result =
                ox_mf2_parser::parse_source_session(sources, source_id, &mut workspace, options);
            let tables = black_box(result.cst.tables());
            checksum = checksum
                .wrapping_add((tables.node_count() + tables.token_count()) as u64)
                .wrapping_add(source_len as u64)
                .wrapping_add(result.diagnostics.len() as u64);
        }
    }

    checksum
}

fn select_benchmark_cases<'a>(corpus: &'a Corpus, target: &str) -> Vec<&'a Case> {
    corpus
        .cases
        .iter()
        .filter(|case| {
            case.expected == "parse-ok" && !case.unsupported_by.iter().any(|name| name == target)
        })
        .collect()
}

fn read_corpus(cases_dir: Option<&PathBuf>, name: &str) -> Result<Corpus, String> {
    let default_cases_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../cases");
    let cases_dir = cases_dir.unwrap_or(&default_cases_dir);
    let path = cases_dir.join(format!("{name}.json"));
    let source = fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read {}: {error}", path.display()))?;
    serde_json::from_str(&source)
        .map_err(|error| format!("Failed to parse {}: {error}", path.display()))
}

fn parse_args(values: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut args = Args {
        iterations: 1,
        ..Args::default()
    };
    let mut values = values.peekable();

    while let Some(value) = values.next() {
        match value.as_str() {
            "--target" => args.target = Some(next_arg(&mut values, "--target")?),
            "--corpus" => args.corpus = Some(next_arg(&mut values, "--corpus")?),
            "--cases-dir" => {
                args.cases_dir = Some(PathBuf::from(next_arg(&mut values, "--cases-dir")?))
            }
            "--iterations" => {
                let value = next_arg(&mut values, "--iterations")?;
                args.iterations = value
                    .parse()
                    .map_err(|_| format!("Invalid --iterations value: {value}"))?;
                if args.iterations == 0 {
                    return Err("--iterations must be greater than zero".to_string());
                }
            }
            "--summary-json" => {
                args.summary_json = Some(PathBuf::from(next_arg(&mut values, "--summary-json")?));
            }
            "--preflight" => args.preflight = true,
            other => return Err(format!("Unknown argument: {other}")),
        }
    }

    Ok(args)
}

fn next_arg(
    values: &mut std::iter::Peekable<impl Iterator<Item = String>>,
    name: &str,
) -> Result<String, String> {
    values
        .next()
        .ok_or_else(|| format!("Missing value for {name}"))
}
