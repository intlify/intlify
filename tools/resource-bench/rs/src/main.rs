// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::fmt::Write as _;
use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use allocation_counter::{measure, AllocationInfo};
use intlify_format::{format_message, FormatOptions};
use intlify_resource::{
    ExtractedCatalog, FormattedEntry, HostFormatRegistry, WriteBackBenchmarkProfile,
    WriteBackOutcome,
};
use serde::{Deserialize, Serialize};

const EXPANDING_MESSAGE: &str =
    ".match   $count  $gender\n0 {{One key}}\n10 female extra {{Three keys}}\n* * {{Two keys}}";
const FORMATTED_MESSAGE: &str = ".match $count $gender\n0                  {{One key}}\n10  female  extra  {{Three keys}}\n*   *              {{Two keys}}";
const NEAR_LINEAR_GROWTH_LIMIT: f64 = 2.5;

fn main() {
    if let Err(error) = run() {
        eprintln!("intlify-resource-bench: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let options = Options::parse()?;
    let selection = serde_json::from_str::<FixtureSelection>(
        &fs::read_to_string(&options.fixture_selection).map_err(|error| {
            format!(
                "failed to read {}: {error}",
                options.fixture_selection.display()
            )
        })?,
    )
    .map_err(|error| format!("fixture selection is invalid: {error}"))?;
    validate_selection(&selection)?;

    let mut results = Vec::new();
    let mut memory_samples = Vec::new();

    for profile in &selection.profiles {
        for &scale in &profile.scales {
            let original = generate_catalog(profile.shape, scale, CatalogVariant::Original)?;
            let candidate = generate_catalog(profile.shape, scale, CatalogVariant::Candidate)?;
            let entry_count = expected_entry_count(profile.shape, scale);

            results.push(measure_extraction_timing(
                profile,
                scale,
                entry_count,
                &original,
                options.iterations,
            )?);

            for (variant, source, cost) in [
                ("original", original.as_str(), "original_extraction"),
                ("candidate", candidate.as_str(), "candidate_reextraction"),
            ] {
                let allocation = measure_extraction_peak(source)?;
                let record = memory_record(
                    "resource_extract_peak_memory",
                    cost,
                    &profile.name,
                    variant,
                    scale,
                    source.len(),
                    entry_count,
                    allocation,
                )?;
                memory_samples.push(MemorySample {
                    fixture: profile.name.clone(),
                    variant: variant.to_owned(),
                    input_bytes: source.len(),
                    peak_live_bytes: record.peak_live_bytes.expect("memory record has a peak"),
                });
                results.push(record);
            }
        }

        if profile.shape == FixtureShape::MessageDense {
            let scale = *profile
                .scales
                .last()
                .expect("validated profiles contain scales");
            measure_write_back(profile, scale, options.iterations, &mut results)?;
            measure_formatter_admission(profile, scale, options.iterations, &mut results)?;
        }
    }

    let memory_growth_checks = validate_memory_growth(&selection.profiles, &memory_samples)?;
    let output = CoreOutput {
        results,
        memory_growth_checks,
    };
    println!(
        "{}",
        serde_json::to_string(&output).map_err(|error| error.to_string())?
    );
    Ok(())
}

#[derive(Debug)]
struct Options {
    fixture_selection: PathBuf,
    iterations: u64,
}

impl Options {
    fn parse() -> Result<Self, String> {
        let mut fixture_selection = None;
        let mut iterations = None;
        let mut args = std::env::args().skip(1);
        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--fixture-selection" => {
                    fixture_selection = Some(PathBuf::from(
                        args.next().ok_or("--fixture-selection requires a path")?,
                    ));
                }
                "--iterations" => {
                    let raw = args.next().ok_or("--iterations requires a value")?;
                    let value = raw
                        .parse::<u64>()
                        .map_err(|_| format!("--iterations must be a positive integer: {raw}"))?;
                    if value == 0 {
                        return Err("--iterations must be a positive integer".to_owned());
                    }
                    iterations = Some(value);
                }
                _ => return Err(format!("unknown argument: {argument}")),
            }
        }

        Ok(Self {
            fixture_selection: fixture_selection.ok_or("--fixture-selection is required")?,
            iterations: iterations.unwrap_or(5),
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FixtureSelection {
    profiles: Vec<FixtureProfile>,
    catalog_fixture: CatalogFixture,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureProfile {
    name: String,
    shape: FixtureShape,
    scales: Vec<usize>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum FixtureShape {
    MessageDense,
    StructurallyDenseFewMessage,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogFixture {
    name: String,
    path: String,
}

fn validate_selection(selection: &FixtureSelection) -> Result<(), String> {
    if selection.profiles.is_empty() {
        return Err("fixture selection must contain profiles".to_owned());
    }
    if selection.catalog_fixture.name.is_empty() || selection.catalog_fixture.path.is_empty() {
        return Err("catalogFixture must contain name and path".to_owned());
    }
    for profile in &selection.profiles {
        if profile.name.is_empty() {
            return Err("fixture profile name must not be empty".to_owned());
        }
        if profile.scales.len() < 3
            || profile.scales.contains(&0)
            || !profile.scales.windows(2).all(|pair| pair[0] < pair[1])
        {
            return Err(format!(
                "fixture profile {} must contain at least three increasing positive scales",
                profile.name
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum CatalogVariant {
    Original,
    Candidate,
}

fn generate_catalog(
    shape: FixtureShape,
    scale: usize,
    variant: CatalogVariant,
) -> Result<String, String> {
    match shape {
        FixtureShape::MessageDense => generate_message_dense(scale, variant),
        FixtureShape::StructurallyDenseFewMessage => generate_structurally_dense(scale, variant),
    }
}

fn generate_message_dense(scale: usize, variant: CatalogVariant) -> Result<String, String> {
    let message = match variant {
        CatalogVariant::Original => EXPANDING_MESSAGE,
        CatalogVariant::Candidate => FORMATTED_MESSAGE,
    };
    let encoded = serde_json::to_string(message).map_err(|error| error.to_string())?;
    let mut source = String::with_capacity(scale.saturating_mul(encoded.len() + 18));
    source.push('{');
    for index in 0..scale {
        if index > 0 {
            source.push(',');
        }
        write!(source, "\"message-{index:08}\":{encoded}").map_err(|error| error.to_string())?;
    }
    source.push('}');
    Ok(source)
}

fn generate_structurally_dense(scale: usize, variant: CatalogVariant) -> Result<String, String> {
    let message = match variant {
        CatalogVariant::Original => EXPANDING_MESSAGE,
        CatalogVariant::Candidate => FORMATTED_MESSAGE,
    };
    let encoded = serde_json::to_string(message).map_err(|error| error.to_string())?;
    let mut source = String::with_capacity(scale.saturating_mul(2) + encoded.len() + 32);
    write!(source, "{{\"message\":{encoded},\"payload\":[").map_err(|error| error.to_string())?;
    for index in 0..scale {
        if index > 0 {
            source.push(',');
        }
        source.push(
            if index == 0 && matches!(variant, CatalogVariant::Candidate) {
                '1'
            } else {
                '0'
            },
        );
    }
    source.push_str("]}");
    Ok(source)
}

const fn expected_entry_count(shape: FixtureShape, scale: usize) -> usize {
    match shape {
        FixtureShape::MessageDense => scale,
        FixtureShape::StructurallyDenseFewMessage => 1,
    }
}

fn extract(source: &str) -> Result<ExtractedCatalog, String> {
    let registry = HostFormatRegistry::new();
    let resolved = registry
        .resolve_direct_extension(".json")
        .ok_or("JSON adapter is unavailable")?;
    registry
        .extract(resolved, Arc::from(source))
        .map_err(|error| error.to_string())
}

fn measure_extraction_timing(
    profile: &FixtureProfile,
    scale: usize,
    entry_count: usize,
    source: &str,
    iterations: u64,
) -> Result<Measurement, String> {
    let started = Instant::now();
    let mut checksum = 0_u64;
    for _ in 0..iterations {
        let catalog = extract(black_box(source))?;
        checksum = checksum.wrapping_add(catalog.entries().len() as u64);
        checksum = checksum.wrapping_add(catalog.source().len() as u64);
        black_box(&catalog);
    }
    Ok(duration_record(
        "resource_extract",
        "host_parse_and_entry_extraction",
        &profile.name,
        "original",
        scale,
        source.len(),
        entry_count,
        iterations,
        started.elapsed(),
        checksum,
    ))
}

fn measure_extraction_peak(source: &str) -> Result<AllocationInfo, String> {
    let registry = HostFormatRegistry::new();
    let resolved = registry
        .resolve_direct_extension(".json")
        .ok_or("JSON adapter is unavailable")?;
    let mut outcome = None;
    let allocation = measure(|| {
        outcome = Some(registry.extract(resolved, Arc::from(black_box(source))));
    });
    let catalog = outcome
        .expect("measured extraction stores its result")
        .map_err(|error| error.to_string())?;
    black_box(catalog.entries().len());
    drop(catalog);
    Ok(allocation)
}

fn measure_write_back(
    profile: &FixtureProfile,
    scale: usize,
    iterations: u64,
    output: &mut Vec<Measurement>,
) -> Result<(), String> {
    let source = generate_catalog(profile.shape, scale, CatalogVariant::Original)?;
    let catalog = extract(&source)?;
    let formatted_messages = catalog
        .entries()
        .iter()
        .map(|_| FORMATTED_MESSAGE.to_owned())
        .collect::<Vec<_>>();
    let formatted_entries = catalog
        .entries()
        .iter()
        .zip(&formatted_messages)
        .map(|(entry, formatted_message)| FormattedEntry {
            entry: entry.handle(),
            formatted_message,
        })
        .collect::<Vec<_>>();

    let mut measurement = Duration::ZERO;
    let mut materialization = Duration::ZERO;
    let mut validation = Duration::ZERO;
    let mut checksum = 0_u64;
    for _ in 0..iterations {
        let profiled = catalog
            .benchmark_build_and_validate_write_back(black_box(&formatted_entries))
            .map_err(|error| error.to_string())?;
        let (outcome, profile) = profiled.into_parts();
        accumulate_write_back_profile(
            profile,
            &mut measurement,
            &mut materialization,
            &mut validation,
        );
        let WriteBackOutcome::Changed(write_back) = outcome else {
            return Err("write-back benchmark unexpectedly returned Unchanged".to_owned());
        };
        checksum = checksum.wrapping_add(checksum_bytes(write_back.candidate().source()));
        black_box(write_back);
    }

    for (cost, elapsed) in [
        ("reescape_measurement", measurement),
        ("raw_materialization_and_edit_composition", materialization),
        ("candidate_reparse_and_validation", validation),
    ] {
        output.push(duration_record(
            "resource_write_back",
            cost,
            &profile.name,
            "changed",
            scale,
            source.len(),
            catalog.entries().len(),
            iterations,
            elapsed,
            checksum,
        ));
    }
    Ok(())
}

fn accumulate_write_back_profile(
    profile: WriteBackBenchmarkProfile,
    measurement: &mut Duration,
    materialization: &mut Duration,
    validation: &mut Duration,
) {
    *measurement += profile.measurement();
    *materialization += profile.materialization_and_edit_composition();
    *validation += profile.candidate_reparse_and_validation();
}

fn measure_formatter_admission(
    profile: &FixtureProfile,
    scale: usize,
    iterations: u64,
    output: &mut Vec<Measurement>,
) -> Result<(), String> {
    for (variant, catalog_variant, expected_changed) in [
        ("expanding", CatalogVariant::Original, true),
        ("byte_identical", CatalogVariant::Candidate, false),
    ] {
        let source = generate_catalog(profile.shape, scale, catalog_variant)?;
        let catalog = extract(&source)?;
        let mut formatter_elapsed = Duration::ZERO;
        let mut admission_elapsed = Duration::ZERO;
        let mut checksum = 0_u64;
        for _ in 0..iterations {
            let run = execute_formatter_admission(&catalog, expected_changed)?;
            formatter_elapsed += run.formatter_elapsed;
            admission_elapsed += run.admission_elapsed;
            checksum = checksum.wrapping_add(run.checksum);
            black_box(run.outputs);
        }

        output.push(duration_record(
            "fmt_catalog_output_admission_peak_memory",
            "message_parse_and_format",
            &profile.name,
            variant,
            scale,
            source.len(),
            catalog.entries().len(),
            iterations,
            formatter_elapsed,
            checksum,
        ));
        output.push(duration_record(
            "fmt_catalog_output_admission_peak_memory",
            "raw_order_candidate_admission",
            &profile.name,
            variant,
            scale,
            source.len(),
            catalog.entries().len(),
            iterations,
            admission_elapsed,
            checksum,
        ));

        let mut measured_run = None;
        let allocation = measure(|| {
            measured_run = Some(execute_formatter_admission(&catalog, expected_changed));
        });
        let measured_run = measured_run.expect("measured admission stores its result")?;
        black_box(&measured_run.outputs);
        output.push(memory_record(
            "fmt_catalog_output_admission_peak_memory",
            "combined_formatter_output_and_admission",
            &profile.name,
            variant,
            scale,
            source.len(),
            catalog.entries().len(),
            allocation,
        )?);
    }
    Ok(())
}

struct FormatterAdmissionRun {
    outputs: Vec<String>,
    formatter_elapsed: Duration,
    admission_elapsed: Duration,
    checksum: u64,
}

fn execute_formatter_admission(
    catalog: &ExtractedCatalog,
    expected_changed: bool,
) -> Result<FormatterAdmissionRun, String> {
    let mut admission = catalog.begin_candidate_message_admission();
    let mut outputs = Vec::with_capacity(catalog.entries().len());
    let mut formatter_elapsed = Duration::ZERO;
    let mut admission_elapsed = Duration::ZERO;
    let mut checksum = 0_u64;

    for entry in catalog.entries() {
        let started = Instant::now();
        let formatted = format_message(black_box(entry.message_text()), FormatOptions::default())
            .map_err(|failure| format!("formatter failed: {failure:?}"))?;
        if formatted.changed != expected_changed
            || (expected_changed && formatted.code.len() <= entry.message_text().len())
        {
            return Err(format!(
                "formatter admission fixture did not satisfy its {} contract",
                if expected_changed {
                    "expanding"
                } else {
                    "byte-identical"
                }
            ));
        }
        formatter_elapsed += started.elapsed();

        let started = Instant::now();
        admission
            .admit_formatted_bytes(
                entry.handle(),
                u64::try_from(formatted.code.len())
                    .map_err(|_| "formatted output length does not fit u64")?,
            )
            .map_err(|error| error.to_string())?;
        admission_elapsed += started.elapsed();
        checksum = checksum.wrapping_add(checksum_bytes(&formatted.code));
        outputs.push(formatted.code);
    }
    admission.finish().map_err(|error| error.to_string())?;

    Ok(FormatterAdmissionRun {
        outputs,
        formatter_elapsed,
        admission_elapsed,
        checksum,
    })
}

fn checksum_bytes(value: &str) -> u64 {
    u64::from(value.as_bytes().iter().fold(0_u32, |checksum, byte| {
        checksum
            .wrapping_mul(16_777_619)
            .wrapping_add(u32::from(*byte))
    }))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CoreOutput {
    results: Vec<Measurement>,
    memory_growth_checks: Vec<MemoryGrowthCheck>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Measurement {
    status: &'static str,
    phase: &'static str,
    cost: &'static str,
    fixture: String,
    variant: String,
    scale: usize,
    input_bytes: usize,
    entry_count: usize,
    metric: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    iterations: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    elapsed_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    checksum: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    peak_live_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retained_live_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    allocation_count: Option<u64>,
}

#[allow(clippy::too_many_arguments)]
fn duration_record(
    phase: &'static str,
    cost: &'static str,
    fixture: &str,
    variant: &str,
    scale: usize,
    input_bytes: usize,
    entry_count: usize,
    iterations: u64,
    elapsed: Duration,
    checksum: u64,
) -> Measurement {
    Measurement {
        status: "measured",
        phase,
        cost,
        fixture: fixture.to_owned(),
        variant: variant.to_owned(),
        scale,
        input_bytes,
        entry_count,
        metric: "duration",
        iterations: Some(iterations),
        elapsed_ms: Some(elapsed.as_secs_f64() * 1_000.0),
        checksum: Some(checksum),
        peak_live_bytes: None,
        retained_live_bytes: None,
        allocation_count: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn memory_record(
    phase: &'static str,
    cost: &'static str,
    fixture: &str,
    variant: &str,
    scale: usize,
    input_bytes: usize,
    entry_count: usize,
    allocation: AllocationInfo,
) -> Result<Measurement, String> {
    let retained_live_bytes = u64::try_from(allocation.bytes_current).map_err(|_| {
        format!(
            "{phase}/{cost} released more measured bytes than it allocated: {}",
            allocation.bytes_current
        )
    })?;
    Ok(Measurement {
        status: "measured",
        phase,
        cost,
        fixture: fixture.to_owned(),
        variant: variant.to_owned(),
        scale,
        input_bytes,
        entry_count,
        metric: "peak_live_memory",
        iterations: None,
        elapsed_ms: None,
        checksum: None,
        peak_live_bytes: Some(allocation.bytes_max),
        retained_live_bytes: Some(retained_live_bytes),
        allocation_count: Some(allocation.count_total),
    })
}

struct MemorySample {
    fixture: String,
    variant: String,
    input_bytes: usize,
    peak_live_bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MemoryGrowthCheck {
    fixture: String,
    variant: String,
    sample_count: usize,
    max_normalized_step_growth: f64,
    limit: f64,
    status: &'static str,
}

fn validate_memory_growth(
    profiles: &[FixtureProfile],
    samples: &[MemorySample],
) -> Result<Vec<MemoryGrowthCheck>, String> {
    let mut checks = Vec::new();
    for profile in profiles {
        for variant in ["original", "candidate"] {
            let matching = samples
                .iter()
                .filter(|sample| sample.fixture == profile.name && sample.variant == variant)
                .collect::<Vec<_>>();
            if matching.len() != profile.scales.len() {
                return Err(format!(
                    "missing memory samples for {}/{}",
                    profile.name, variant
                ));
            }
            let mut max_normalized_step_growth = 0.0_f64;
            for pair in matching.windows(2) {
                if pair[0].input_bytes >= pair[1].input_bytes
                    || pair[0].peak_live_bytes == 0
                    || pair[1].peak_live_bytes == 0
                {
                    return Err(format!(
                        "invalid increasing memory samples for {}/{}",
                        profile.name, variant
                    ));
                }
                let input_growth = pair[1].input_bytes as f64 / pair[0].input_bytes as f64;
                let peak_growth = pair[1].peak_live_bytes as f64 / pair[0].peak_live_bytes as f64;
                max_normalized_step_growth =
                    max_normalized_step_growth.max(peak_growth / input_growth);
            }
            if max_normalized_step_growth > NEAR_LINEAR_GROWTH_LIMIT {
                return Err(format!(
                    "peak live memory is not near-linear for {}/{}: normalized step growth {:.3} exceeds {:.3}",
                    profile.name,
                    variant,
                    max_normalized_step_growth,
                    NEAR_LINEAR_GROWTH_LIMIT
                ));
            }
            checks.push(MemoryGrowthCheck {
                fixture: profile.name.clone(),
                variant: variant.to_owned(),
                sample_count: matching.len(),
                max_normalized_step_growth,
                limit: NEAR_LINEAR_GROWTH_LIMIT,
                status: "passed",
            });
        }
    }
    Ok(checks)
}
