// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use allocation_counter::{measure, AllocationInfo};
use intlify_cli::messages::benchmark::{
    benchmark_project_link, BenchmarkProjectLinkExecution, MessageBenchmarkStage,
};
use intlify_cli::messages::validate_messages_config;
use intlify_contract::{
    decode_definition_artifact, decode_reference_artifact, encode_definition_artifact,
    encode_reference_artifact, ArtifactNamespace, ArtifactVersion, CatalogKey, CatalogKeyDomain,
    CatalogKeyPrefix, CatalogScopeId, CatalogScopeName, DeliveryUnitId, EntryReference,
    EntryStructuralPath, FingerprintAlgorithm, FingerprintDigest, InputFingerprint, LinkLimits,
    Locale, MessageDefinition, MessageDefinitionArtifact, MessagePayload, MessageReference,
    MessageReferenceArtifact, MessageSelector, PortablePathSegment, PortableRelativePath,
    ProducerId, ProducerIdentity, ProducerRevision, ReferenceArtifactIdentity,
    ReferenceArtifactSegment, SourceDocumentIdentity,
};
use intlify_linker::{
    link, DeliveryUnitGraph, DynamicReferenceMode, InputCompleteness, LinkPolicy, LinkRequest,
    PlacementPolicy, ScopeCompleteness, ScopeCompletenessTable, ScopeMappingTable,
};
use intlify_producer_js::benchmark::{
    benchmark_produce_reference_artifacts_with_cache, BenchmarkJsStage,
};
use intlify_producer_js::{
    JsKeySyntax, JsPhysicalSourceGroup, JsProducerCache, JsProducerCacheIoError,
    JsProducerCacheKey, JsRecognizerBinding, JsRecognizerCallKind, JsRecognizerSet,
};
use intlify_resource::{HostFormatRegistry, ResourcesConfig};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tempfile::TempDir;

const RUNTIME: &str = "intlify-messages-bench-rs";
const EXECUTION_MODEL: &str = "in_process";
const DOMAIN: CatalogKeyDomain = CatalogKeyDomain::JsonPointer;
const ZERO_REFERENCE_ARTIFACT: &[u8] = include_bytes!(
    "../../../../crates/intlify_contract/fixtures/reference/v0.1/zero-references.json"
);

fn main() {
    if let Err(error) = run() {
        eprintln!("intlify-messages-bench: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let options = Options::parse()?;
    let selection: FixtureSelection =
        serde_json::from_slice(&fs::read(&options.fixture_selection).map_err(|error| {
            format!(
                "failed to read {}: {error}",
                options.fixture_selection.display()
            )
        })?)
        .map_err(|error| format!("fixture selection is invalid: {error}"))?;
    selection.validate()?;

    let (mut results, definitions, references) =
        measure_project_workflow(&selection.project_fixture, &options)?;
    results.extend(measure_artifact_codecs(
        &selection.project_fixture,
        &definitions,
        &references,
        &options,
    )?);
    results.push(measure_js_cache_hit(&selection, &options)?);
    results.extend(measure_link_peak_memory(
        &selection,
        options.warmup_iterations,
    )?);

    let output = CoreOutput { results };
    println!(
        "{}",
        serde_json::to_string(&output)
            .map_err(|error| format!("failed to encode benchmark output: {error}"))?
    );
    Ok(())
}

#[derive(Debug)]
struct Options {
    fixture_selection: PathBuf,
    iterations: u64,
    warmup_iterations: u64,
}

impl Options {
    fn parse() -> Result<Self, String> {
        let mut fixture_selection = None;
        let mut iterations = 5_u64;
        let mut warmup_iterations = 1_u64;
        let mut args = env::args().skip(1);
        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--fixture-selection" => {
                    fixture_selection = Some(PathBuf::from(
                        args.next().ok_or("--fixture-selection requires a path")?,
                    ));
                }
                "--iterations" => {
                    iterations = positive_u64(
                        &args.next().ok_or("--iterations requires a value")?,
                        "--iterations",
                    )?;
                }
                "--warmup-iterations" => {
                    warmup_iterations = args
                        .next()
                        .ok_or("--warmup-iterations requires a value")?
                        .parse()
                        .map_err(|_| "--warmup-iterations must be a non-negative integer")?;
                }
                unknown => return Err(format!("unknown argument: {unknown}")),
            }
        }
        Ok(Self {
            fixture_selection: fixture_selection.ok_or("--fixture-selection is required")?,
            iterations,
            warmup_iterations,
        })
    }
}

fn positive_u64(value: &str, option: &str) -> Result<u64, String> {
    let value = value
        .parse::<u64>()
        .map_err(|_| format!("{option} must be a positive integer"))?;
    if value == 0 {
        return Err(format!("{option} must be a positive integer"));
    }
    Ok(value)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureSelection {
    revision: u32,
    profiles: Vec<FixtureProfile>,
    project_fixture: ProjectFixture,
}

impl FixtureSelection {
    fn validate(&self) -> Result<(), String> {
        if self.revision == 0 || self.project_fixture.revision == 0 {
            return Err("fixture revisions must be positive".to_owned());
        }
        let expected = [
            FixtureShape::ExactReferenceDense,
            FixtureShape::DefinitionDenseSparseReference,
            FixtureShape::BoundedSelector,
            FixtureShape::FindingDense,
        ];
        for shape in expected {
            let profile = self
                .profiles
                .iter()
                .find(|profile| profile.shape == shape)
                .ok_or_else(|| format!("fixture selection is missing {shape:?}"))?;
            if profile.revision == 0
                || profile.generator.kind != FixtureGeneratorKind::Rust
                || profile.generator.id != shape
                || profile.scales.len() < 3
            {
                return Err(format!(
                    "{} must have its exact Rust generator, a positive revision, and at least three scales",
                    profile.name
                ));
            }
            if profile.scales.contains(&0)
                || profile.scales.windows(2).any(|pair| pair[0] >= pair[1])
            {
                return Err(format!(
                    "{} scales must be positive and strictly increasing",
                    profile.name
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureProfile {
    name: String,
    revision: u32,
    shape: FixtureShape,
    generator: FixtureGenerator,
    scales: Vec<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureGenerator {
    kind: FixtureGeneratorKind,
    id: FixtureShape,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum FixtureGeneratorKind {
    Rust,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum FixtureShape {
    ExactReferenceDense,
    DefinitionDenseSparseReference,
    BoundedSelector,
    FindingDense,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectFixture {
    name: String,
    revision: u32,
}

#[derive(Debug, Serialize)]
struct CoreOutput {
    results: Vec<Measurement>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Measurement {
    status: &'static str,
    phase: &'static str,
    cost: &'static str,
    boundary_id: &'static str,
    fixture: String,
    fixture_revision: u32,
    variant: String,
    scale: usize,
    runtime: &'static str,
    execution_model: &'static str,
    operation: &'static str,
    metric: &'static str,
    input_count: u64,
    output_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    physical_group_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    host_byte_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    entry_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    iterations: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    elapsed_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    checksum: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    peak_live_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retained_live_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    allocation_count: Option<u64>,
}

#[derive(Debug)]
struct StageIteration {
    identities: Vec<Box<str>>,
    elapsed: Duration,
    checksum: u32,
}

#[derive(Debug)]
struct StageAggregate {
    identities: Vec<Box<str>>,
    elapsed: Duration,
    per_iteration_checksum: u32,
    repeated_checksum: u32,
}

fn measure_project_workflow(
    fixture: &ProjectFixture,
    options: &Options,
) -> Result<ProjectMeasurementOutput, String> {
    let project = RepresentativeProject::new()?;
    let prepared = project.execute()?;
    let definitions = prepared.definition_artifacts().to_vec();
    let references = prepared.reference_artifacts().to_vec();
    let resource_iteration_checksum = project.resource_extraction_checksum()?;

    for _ in 0..options.warmup_iterations {
        black_box(project.execute()?);
    }

    let mut aggregates = BTreeMap::<MessageBenchmarkStage, StageAggregate>::new();
    for _ in 0..options.iterations {
        let execution = project.execute()?;
        for (stage, iteration) in summarize_project_iteration(&execution) {
            let aggregate = aggregates.entry(stage).or_insert_with(|| StageAggregate {
                identities: iteration.identities.clone(),
                elapsed: Duration::ZERO,
                per_iteration_checksum: iteration.checksum,
                repeated_checksum: 0,
            });
            if aggregate.identities != iteration.identities
                || aggregate.per_iteration_checksum != iteration.checksum
            {
                return Err(format!(
                    "{}/{} changed its semantic observation across repetitions",
                    stage.phase(),
                    stage.cost()
                ));
            }
            aggregate.elapsed += iteration.elapsed;
            aggregate.repeated_checksum =
                aggregate.repeated_checksum.wrapping_add(iteration.checksum);
        }
        if execution.definition_artifacts() != definitions
            || execution.reference_artifacts() != references
        {
            return Err("project workflow changed its admitted artifacts".to_owned());
        }
    }

    let definition_count = u64::try_from(definitions.len()).map_err(|_| "definition count")?;
    let reference_count = u64::try_from(references.len()).map_err(|_| "reference count")?;
    let output_count = definition_count.saturating_add(reference_count);
    let mut results = Vec::new();
    for (stage, aggregate) in aggregates {
        if matches!(
            stage,
            MessageBenchmarkStage::ReferenceArtifactDecode
                | MessageBenchmarkStage::ReferenceArtifactEncode
                | MessageBenchmarkStage::DefinitionArtifactDecode
                | MessageBenchmarkStage::DefinitionArtifactEncode
        ) {
            continue;
        }
        let resource = stage == MessageBenchmarkStage::ResourceHostParseAndEntryExtraction;
        results.push(duration_record(DurationRecord {
            stage,
            fixture: &fixture.name,
            fixture_revision: fixture.revision,
            variant: if is_js_stage(stage) {
                "cache_miss"
            } else {
                "representative_project"
            },
            scale: 1,
            operation: stage.boundary_id(),
            input_count: definition_count.saturating_add(reference_count),
            output_count,
            physical_group_count: resource.then(|| {
                u64::try_from(aggregate.identities.len())
                    .expect("supported physical-group counts fit u64")
            }),
            host_byte_count: resource.then_some(
                u64::try_from(RepresentativeProject::CATALOG.len())
                    .expect("catalog fixture length fits u64"),
            ),
            entry_count: resource.then_some(1),
            iterations: options.iterations,
            elapsed: aggregate.elapsed,
            checksum: if resource {
                resource_iteration_checksum.wrapping_mul(options.iterations as u32)
            } else {
                aggregate.repeated_checksum
            },
        }));
    }
    Ok((results, definitions, references))
}

fn summarize_project_iteration(
    execution: &BenchmarkProjectLinkExecution,
) -> BTreeMap<MessageBenchmarkStage, StageIteration> {
    let mut by_stage = BTreeMap::<
        MessageBenchmarkStage,
        Vec<&intlify_cli::messages::benchmark::BenchmarkMessageMeasurement>,
    >::new();
    for measurement in execution.measurements() {
        by_stage
            .entry(measurement.stage())
            .or_default()
            .push(measurement);
    }
    by_stage
        .into_iter()
        .map(|(stage, measurements)| {
            let identities = measurements
                .iter()
                .map(|measurement| Box::<str>::from(measurement.identity()))
                .collect::<Vec<_>>();
            let elapsed = measurements
                .iter()
                .map(|measurement| measurement.elapsed())
                .sum();
            let checksum = checksum_occurrence_sequence(stage, &measurements);
            (
                stage,
                StageIteration {
                    identities,
                    elapsed,
                    checksum,
                },
            )
        })
        .collect()
}

fn checksum_occurrence_sequence(
    stage: MessageBenchmarkStage,
    measurements: &[&intlify_cli::messages::benchmark::BenchmarkMessageMeasurement],
) -> u32 {
    checksum_observation_sequence(
        stage.cost(),
        measurements
            .iter()
            .map(|measurement| (measurement.identity(), measurement.checksum())),
    )
}

fn checksum_observation_sequence<'a>(
    domain: &str,
    occurrences: impl ExactSizeIterator<Item = (&'a str, u32)>,
) -> u32 {
    let mut checksum = Checksum::new(domain.as_bytes());
    checksum.write_u64(
        0x01,
        u64::try_from(occurrences.len()).expect("supported occurrence counts fit u64"),
    );
    for (identity, semantic_checksum) in occurrences {
        checksum.write_str(0x02, identity);
        checksum.write_u32(0x03, semantic_checksum);
    }
    checksum.finish()
}

struct RepresentativeProject {
    root: TempDir,
    resources: intlify_resource::ResolvedResources,
    messages: intlify_cli::messages::ResolvedMessagesConfig,
    registry: HostFormatRegistry,
    limits: LinkLimits,
}

impl RepresentativeProject {
    const CATALOG: &'static str = r#"{"hello":"Hello"}"#;

    fn new() -> Result<Self, String> {
        let root = tempfile::tempdir().map_err(|error| error.to_string())?;
        write_file(root.path(), "locales/en.json", Self::CATALOG.as_bytes())?;
        write_file(
            root.path(),
            "src/app.mts",
            b"export const hello = t('/hello');",
        )?;
        write_file(
            root.path(),
            "artifacts/external.json",
            ZERO_REFERENCE_ARTIFACT,
        )?;

        let resources = ResourcesConfig::validate(Some(&json!({
            "catalogs": [{
                "scope": "app",
                "include": ["locales/*.json"],
                "locale": { "from": "fixed", "value": "en" }
            }]
        })))
        .map_err(|error| format!("resource fixture config is invalid: {error:?}"))?
        .resolve();
        let messages = validate_messages_config(
            Some(&json!({
                "locales": ["en"],
                "producers": {
                    "js": {
                        "include": ["src/**/*.mts"],
                        "recognizers": {
                            "t": {
                                "kind": "lookup",
                                "scope": "app",
                                "domain": "json-pointer",
                                "keySyntax": "literal"
                            }
                        }
                    },
                    "artifacts": ["artifacts/external.json"]
                }
            })),
            &resources,
        )
        .map_err(|error| format!("message fixture config is invalid: {error:?}"))?
        .ok_or("message fixture config was omitted")?
        .1;
        Ok(Self {
            root,
            resources,
            messages,
            registry: HostFormatRegistry::new(),
            limits: LinkLimits::default(),
        })
    }

    fn execute(&self) -> Result<BenchmarkProjectLinkExecution, String> {
        benchmark_project_link(
            self.root.path(),
            None,
            &self.resources,
            &self.messages,
            &self.registry,
            &self.limits,
        )
        .map_err(|error| error.to_string())
    }

    fn resource_extraction_checksum(&self) -> Result<u32, String> {
        let source = Self::CATALOG;
        let resolved = self
            .registry
            .resolve_direct_extension(".json")
            .ok_or("JSON resource adapter is unavailable")?;
        let catalog = self
            .registry
            .extract(resolved, std::sync::Arc::from(source))
            .map_err(|error| error.to_string())?;
        resource_extraction_record_checksum(
            &[&["locales", "en.json"]],
            "json",
            source.as_bytes(),
            &catalog,
        )
    }
}

fn write_file(root: &Path, relative: &str, bytes: &[u8]) -> Result<(), String> {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().ok_or("fixture path has no parent")?)
        .map_err(|error| error.to_string())?;
    fs::write(path, bytes).map_err(|error| error.to_string())
}

fn resource_extraction_record_checksum(
    physical_group_paths: &[&[&str]],
    host_format: &str,
    source: &[u8],
    catalog: &intlify_resource::ExtractedCatalog,
) -> Result<u32, String> {
    let selected_source_checksum = fnv_domain_bytes(b"intlify-resource-selected-source-v1", source);
    let mut group = Checksum::empty();
    group.write_field(0x01, &path_sequence_payload(physical_group_paths));
    group.write_str(0x02, host_format);
    group.write_u32(0x03, selected_source_checksum);
    let entries = catalog
        .entries()
        .iter()
        .map(resource_entry_payload)
        .collect::<Result<Vec<_>, _>>()?;
    group.write_field(0x04, &sequence_payload(&entries));
    let group_checksum = group.finish();

    let mut record = Checksum::new(b"intlify-resource-extraction-record-v1");
    record.write_u64(
        0x01,
        u64::try_from(physical_group_paths.len())
            .map_err(|_| "resource group count does not fit u64")?,
    );
    record.write_u64(
        0x02,
        u64::try_from(source.len()).map_err(|_| "resource source length does not fit u64")?,
    );
    record.write_u64(
        0x03,
        u64::try_from(catalog.entries().len())
            .map_err(|_| "resource entry count does not fit u64")?,
    );
    let mut group_record = Vec::new();
    group_record.extend_from_slice(&path_sequence_payload(physical_group_paths));
    group_record.extend_from_slice(&group_checksum.to_be_bytes());
    record.write_field(0x04, &sequence_payload(&[group_record]));
    Ok(record.finish())
}

fn resource_entry_payload(entry: &intlify_resource::MessageEntry) -> Result<Vec<u8>, String> {
    let mut entry_record = Vec::new();
    push_field(
        &mut entry_record,
        0x01,
        entry.key().structural_path().as_str().as_bytes(),
    );
    push_field(
        &mut entry_record,
        0x02,
        &entry.key().occurrence().to_be_bytes(),
    );
    push_field(
        &mut entry_record,
        0x03,
        resource_catalog_domain_token(entry.catalog_key_domain())?.as_bytes(),
    );
    push_field(
        &mut entry_record,
        0x04,
        entry.catalog_key().as_str().as_bytes(),
    );
    let mut display_key = vec![u8::from(entry.display_key().is_some())];
    if let Some(value) = entry.display_key() {
        display_key.extend_from_slice(value.as_bytes());
    }
    push_field(&mut entry_record, 0x05, &display_key);
    push_field(&mut entry_record, 0x06, entry.message_text().as_bytes());
    let raw_span = entry.raw_value_span();
    let mut raw_span_payload = Vec::with_capacity(8);
    raw_span_payload.extend_from_slice(&raw_span.start().to_be_bytes());
    raw_span_payload.extend_from_slice(&raw_span.end().to_be_bytes());
    push_field(&mut entry_record, 0x07, &raw_span_payload);

    let message_len = u32::try_from(entry.message_text().len())
        .map_err(|_| "resource message length does not fit u32")?;
    let mut offset_observations = Vec::new();
    for position in 0..=message_len {
        let mapped = entry
            .offset_map()
            .map_span(intlify_resource::Utf8ByteSpan::new(position, position))
            .map_err(|error| format!("resource offset observation failed: {error:?}"))?;
        offset_observations.push(mapped_span_payload(mapped));
    }
    for position in 0..message_len {
        let mapped = entry
            .offset_map()
            .map_span(intlify_resource::Utf8ByteSpan::new(position, position + 1))
            .map_err(|error| format!("resource offset observation failed: {error:?}"))?;
        offset_observations.push(mapped_span_payload(mapped));
    }
    push_field(
        &mut entry_record,
        0x08,
        &sequence_payload(&offset_observations),
    );
    push_field(&mut entry_record, 0x09, &[u8::from(entry.is_read_only())]);
    Ok(entry_record)
}

fn resource_catalog_domain_token(
    domain: intlify_resource::CatalogKeyDomain,
) -> Result<&'static str, String> {
    match domain {
        intlify_resource::CatalogKeyDomain::JsonPointer => Ok("json-pointer"),
        unsupported => Err(format!(
            "resource benchmark schema revision 0 does not admit {unsupported:?}"
        )),
    }
}

fn mapped_span_payload(span: intlify_resource::Utf8ByteSpan) -> Vec<u8> {
    [span.start().to_be_bytes(), span.end().to_be_bytes()].concat()
}

fn path_sequence_payload(paths: &[&[&str]]) -> Vec<u8> {
    let path_payloads = paths
        .iter()
        .map(|segments| {
            let segment_payloads = segments
                .iter()
                .map(|segment| segment.as_bytes().to_vec())
                .collect::<Vec<_>>();
            sequence_payload(&segment_payloads)
        })
        .collect::<Vec<_>>();
    sequence_payload(&path_payloads)
}

fn sequence_payload(items: &[Vec<u8>]) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(
        &u64::try_from(items.len())
            .expect("supported checksum sequences fit u64")
            .to_be_bytes(),
    );
    for item in items {
        payload.extend_from_slice(
            &u64::try_from(item.len())
                .expect("supported checksum items fit u64")
                .to_be_bytes(),
        );
        payload.extend_from_slice(item);
    }
    payload
}

fn push_field(output: &mut Vec<u8>, tag: u8, value: &[u8]) {
    output.push(tag);
    output.extend_from_slice(
        &u64::try_from(value.len())
            .expect("supported checksum fields fit u64")
            .to_be_bytes(),
    );
    output.extend_from_slice(value);
}

fn fnv_domain_bytes(domain: &[u8], bytes: &[u8]) -> u32 {
    let mut checksum = Checksum::new(domain);
    checksum.write_raw(bytes);
    checksum.finish()
}

fn measure_artifact_codecs(
    fixture: &ProjectFixture,
    definitions: &[MessageDefinitionArtifact],
    references: &[MessageReferenceArtifact],
    options: &Options,
) -> Result<Vec<Measurement>, String> {
    let limits = LinkLimits::default();
    let encoded_definitions = definitions
        .iter()
        .map(|artifact| encode_definition_artifact(artifact, &limits))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let encoded_references = references
        .iter()
        .map(|artifact| encode_reference_artifact(artifact, &limits))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    for _ in 0..options.warmup_iterations {
        for artifact in definitions {
            black_box(
                encode_definition_artifact(artifact, &limits).map_err(|error| error.to_string())?,
            );
        }
        for bytes in &encoded_definitions {
            black_box(
                decode_definition_artifact(bytes, &limits).map_err(|error| error.to_string())?,
            );
        }
        for artifact in references {
            black_box(
                encode_reference_artifact(artifact, &limits).map_err(|error| error.to_string())?,
            );
        }
        for bytes in &encoded_references {
            black_box(
                decode_reference_artifact(bytes, &limits).map_err(|error| error.to_string())?,
            );
        }
    }

    let definition_encode = measure_codec_repetitions(
        MessageBenchmarkStage::DefinitionArtifactEncode,
        options.iterations,
        || encode_definition_sequence(definitions, &limits),
    )?;
    let definition_decode = measure_codec_repetitions(
        MessageBenchmarkStage::DefinitionArtifactDecode,
        options.iterations,
        || decode_definition_sequence(&encoded_definitions, &limits),
    )?;
    let reference_encode = measure_codec_repetitions(
        MessageBenchmarkStage::ReferenceArtifactEncode,
        options.iterations,
        || encode_reference_sequence(references, &limits),
    )?;
    let reference_decode = measure_codec_repetitions(
        MessageBenchmarkStage::ReferenceArtifactDecode,
        options.iterations,
        || decode_reference_sequence(&encoded_references, &limits),
    )?;

    Ok([
        (definition_encode, definitions.len(), definitions.len()),
        (definition_decode, definitions.len(), definitions.len()),
        (reference_encode, references.len(), references.len()),
        (reference_decode, references.len(), references.len()),
    ]
    .into_iter()
    .map(|(aggregate, input, output)| {
        duration_record(DurationRecord {
            stage: aggregate.stage,
            fixture: &fixture.name,
            fixture_revision: fixture.revision,
            variant: "canonical_json",
            scale: 1,
            operation: aggregate.stage.boundary_id(),
            input_count: u64::try_from(input).expect("supported artifact counts fit u64"),
            output_count: u64::try_from(output).expect("supported artifact counts fit u64"),
            physical_group_count: None,
            host_byte_count: None,
            entry_count: None,
            iterations: options.iterations,
            elapsed: aggregate.elapsed,
            checksum: aggregate.repeated_checksum,
        })
    })
    .collect())
}

struct TimedAggregate {
    stage: MessageBenchmarkStage,
    elapsed: Duration,
    repeated_checksum: u32,
}

fn measure_codec_repetitions<F>(
    stage: MessageBenchmarkStage,
    iterations: u64,
    mut operation: F,
) -> Result<TimedAggregate, String>
where
    F: FnMut() -> Result<(Duration, u32), String>,
{
    let mut elapsed = Duration::ZERO;
    let mut expected = None;
    let mut repeated_checksum = 0_u32;
    for _ in 0..iterations {
        let (iteration_elapsed, checksum) = operation()?;
        if expected
            .replace(checksum)
            .is_some_and(|value| value != checksum)
        {
            return Err(format!(
                "{}/{} changed across repetitions",
                stage.phase(),
                stage.cost()
            ));
        }
        elapsed += iteration_elapsed;
        repeated_checksum = repeated_checksum.wrapping_add(checksum);
    }
    Ok(TimedAggregate {
        stage,
        elapsed,
        repeated_checksum,
    })
}

fn encode_definition_sequence(
    artifacts: &[MessageDefinitionArtifact],
    limits: &LinkLimits,
) -> Result<(Duration, u32), String> {
    let mut elapsed = Duration::ZERO;
    let mut checksum = Checksum::new(b"definition-artifact-encode");
    for artifact in artifacts {
        let started = Instant::now();
        let bytes =
            encode_definition_artifact(artifact, limits).map_err(|error| error.to_string())?;
        elapsed += started.elapsed();
        checksum.write_bytes(0x01, &bytes);
    }
    Ok((elapsed, checksum.finish()))
}

fn decode_definition_sequence(
    encoded: &[Box<[u8]>],
    limits: &LinkLimits,
) -> Result<(Duration, u32), String> {
    let mut elapsed = Duration::ZERO;
    let mut checksum = Checksum::new(b"definition-artifact-decode");
    for bytes in encoded {
        let started = Instant::now();
        let artifact =
            decode_definition_artifact(bytes, limits).map_err(|error| error.to_string())?;
        elapsed += started.elapsed();
        let canonical =
            encode_definition_artifact(&artifact, limits).map_err(|error| error.to_string())?;
        checksum.write_bytes(0x01, &canonical);
    }
    Ok((elapsed, checksum.finish()))
}

fn encode_reference_sequence(
    artifacts: &[MessageReferenceArtifact],
    limits: &LinkLimits,
) -> Result<(Duration, u32), String> {
    let mut elapsed = Duration::ZERO;
    let mut checksum = Checksum::new(b"reference-artifact-encode");
    for artifact in artifacts {
        let started = Instant::now();
        let bytes =
            encode_reference_artifact(artifact, limits).map_err(|error| error.to_string())?;
        elapsed += started.elapsed();
        checksum.write_bytes(0x01, &bytes);
    }
    Ok((elapsed, checksum.finish()))
}

fn decode_reference_sequence(
    encoded: &[Box<[u8]>],
    limits: &LinkLimits,
) -> Result<(Duration, u32), String> {
    let mut elapsed = Duration::ZERO;
    let mut checksum = Checksum::new(b"reference-artifact-decode");
    for bytes in encoded {
        let started = Instant::now();
        let artifact =
            decode_reference_artifact(bytes, limits).map_err(|error| error.to_string())?;
        elapsed += started.elapsed();
        let canonical =
            encode_reference_artifact(&artifact, limits).map_err(|error| error.to_string())?;
        checksum.write_bytes(0x01, &canonical);
    }
    Ok((elapsed, checksum.finish()))
}

#[derive(Default)]
struct MemoryJsCache {
    entries: Mutex<HashMap<[u8; 32], Box<[u8]>>>,
}

impl JsProducerCache for MemoryJsCache {
    fn load(&self, key: &JsProducerCacheKey) -> Result<Option<Box<[u8]>>, JsProducerCacheIoError> {
        Ok(self
            .entries
            .lock()
            .map_err(|_| "benchmark cache lock poisoned")?
            .get(key.lookup_digest())
            .cloned())
    }

    fn store(&self, key: &JsProducerCacheKey, value: &[u8]) -> Result<(), JsProducerCacheIoError> {
        self.entries
            .lock()
            .map_err(|_| "benchmark cache lock poisoned")?
            .insert(*key.lookup_digest(), value.to_vec().into_boxed_slice());
        Ok(())
    }
}

fn measure_js_cache_hit(
    selection: &FixtureSelection,
    options: &Options,
) -> Result<Measurement, String> {
    let profile = selection
        .profiles
        .iter()
        .find(|profile| profile.shape == FixtureShape::ExactReferenceDense)
        .ok_or("exact-reference-dense profile is missing")?;
    let scale = profile.scales[0];
    let mut source = String::with_capacity(scale.saturating_mul(20));
    for index in 0..scale {
        write!(&mut source, "t('/key/{index:08}');")
            .expect("writing benchmark source to a string cannot fail");
    }
    let selected = || {
        JsPhysicalSourceGroup::try_new(
            vec![source_identity(&["src", "cache.mts"])],
            source.as_bytes().to_vec(),
        )
        .map(|group| vec![group])
        .map_err(|error| format!("cache fixture group is invalid: {error:?}"))
    };
    let recognizers = JsRecognizerSet::try_new(vec![JsRecognizerBinding::try_new(
        "t",
        JsRecognizerCallKind::Lookup,
        app_scope(),
        DOMAIN,
        JsKeySyntax::Literal,
    )
    .map_err(|error| format!("cache fixture recognizer is invalid: {error:?}"))?])
    .map_err(|error| format!("cache fixture recognizer set is invalid: {error:?}"))?;
    let limits = LinkLimits::default();
    let cache = MemoryJsCache::default();
    black_box(
        benchmark_produce_reference_artifacts_with_cache(
            selected()?,
            &recognizers,
            &limits,
            &cache,
        )
        .map_err(|error| error.to_string())?,
    );
    for _ in 0..options.warmup_iterations {
        black_box(
            benchmark_produce_reference_artifacts_with_cache(
                selected()?,
                &recognizers,
                &limits,
                &cache,
            )
            .map_err(|error| error.to_string())?,
        );
    }

    let mut elapsed = Duration::ZERO;
    let mut expected = None;
    let mut repeated_checksum = 0_u32;
    for _ in 0..options.iterations {
        let execution = benchmark_produce_reference_artifacts_with_cache(
            selected()?,
            &recognizers,
            &limits,
            &cache,
        )
        .map_err(|error| error.to_string())?;
        let stages = execution.stages();
        if stages.len() != 1 || stages[0].stage() != BenchmarkJsStage::CacheHitValidationAndAccess {
            return Err("prepared JS cache hit executed an unexpected producer stage".to_owned());
        }
        let checksum = stages[0].checksum();
        if expected
            .replace(checksum)
            .is_some_and(|value| value != checksum)
        {
            return Err("JS cache-hit checksum changed across repetitions".to_owned());
        }
        elapsed += stages[0].elapsed();
        repeated_checksum = repeated_checksum.wrapping_add(checksum);
        black_box(execution);
    }

    Ok(duration_record(DurationRecord {
        stage: MessageBenchmarkStage::CacheHitValidationAndAccess,
        fixture: &profile.name,
        fixture_revision: profile.revision,
        variant: "cache_hit",
        scale,
        operation: MessageBenchmarkStage::CacheHitValidationAndAccess.boundary_id(),
        input_count: u64::try_from(scale).expect("supported fixture scale fits u64"),
        output_count: 1,
        physical_group_count: None,
        host_byte_count: None,
        entry_count: None,
        iterations: options.iterations,
        elapsed,
        checksum: repeated_checksum,
    }))
}

fn measure_link_peak_memory(
    selection: &FixtureSelection,
    warmup_iterations: u64,
) -> Result<Vec<Measurement>, String> {
    let mut results = Vec::new();
    for profile in &selection.profiles {
        for &scale in &profile.scales {
            let fixture = GeneratedLinkFixture::new(profile.shape, scale)?;
            for _ in 0..warmup_iterations {
                black_box(fixture.link()?);
            }
            let request = fixture.request()?;
            let mut outcome = None;
            let allocation = measure(|| {
                outcome = Some(link(black_box(&request)));
            });
            let execution = outcome
                .ok_or("link memory measurement produced no result")?
                .map_err(|error| error.to_string())?;
            let output_count = u64::try_from(
                execution.findings().len() + execution.bundle_plans().map_or(0, <[_]>::len),
            )
            .map_err(|_| "link output count does not fit u64")?;
            black_box(execution);
            results.push(memory_record(
                profile,
                scale,
                fixture.input_count(),
                output_count,
                allocation,
            )?);
        }
    }
    Ok(results)
}

struct GeneratedLinkFixture {
    references: Vec<MessageReferenceArtifact>,
    definitions: Vec<MessageDefinitionArtifact>,
    policy: LinkPolicy,
    mappings: ScopeMappingTable,
    completeness: ScopeCompletenessTable,
    graph: DeliveryUnitGraph,
    limits: LinkLimits,
}

impl GeneratedLinkFixture {
    fn new(shape: FixtureShape, scale: usize) -> Result<Self, String> {
        let scope = app_scope();
        let definitions = match shape {
            FixtureShape::FindingDense => Vec::new(),
            _ => (0..scale)
                .map(|index| definition(scope.clone(), &definition_key(shape, index), index))
                .collect::<Result<Vec<_>, _>>()?,
        };
        let references = match shape {
            FixtureShape::ExactReferenceDense => (0..scale)
                .map(|index| {
                    reference(
                        scope.clone(),
                        MessageSelector::Exact(catalog_key(&definition_key(shape, index))?),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
            FixtureShape::DefinitionDenseSparseReference => vec![reference(
                scope.clone(),
                MessageSelector::Exact(catalog_key(&definition_key(shape, 0))?),
            )?],
            FixtureShape::BoundedSelector => vec![reference(
                scope.clone(),
                MessageSelector::Prefix(
                    CatalogKeyPrefix::try_new(DOMAIN, "/group")
                        .map_err(|error| format!("selector prefix is invalid: {error:?}"))?,
                ),
            )?],
            FixtureShape::FindingDense => (0..scale)
                .map(|index| {
                    reference(
                        scope.clone(),
                        MessageSelector::Exact(catalog_key(&format!("/missing/{index:08}"))?),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        let limits = LinkLimits::default();
        let definition_artifacts = vec![definition_artifact(definitions, &limits)?];
        let reference_artifacts = vec![reference_artifact(references, &limits)?];
        let policy = LinkPolicy::try_new(
            vec![Locale::try_new("en").map_err(|error| error.to_string())?],
            Vec::new(),
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &limits,
        )
        .map_err(|error| error.to_string())?;
        let mappings = ScopeMappingTable::empty(std::slice::from_ref(&scope), &limits)
            .map_err(|error| error.to_string())?;
        let completeness = ScopeCompletenessTable::try_new(
            std::slice::from_ref(&scope),
            vec![ScopeCompleteness::try_new(
                scope.clone(),
                InputCompleteness::Closed,
                InputCompleteness::Closed,
            )
            .map_err(|error| error.to_string())?],
            &limits,
        )
        .map_err(|error| error.to_string())?;
        let graph = DeliveryUnitGraph::single_main(&limits).map_err(|error| error.to_string())?;
        Ok(Self {
            references: reference_artifacts,
            definitions: definition_artifacts,
            policy,
            mappings,
            completeness,
            graph,
            limits,
        })
    }

    fn request(&self) -> Result<LinkRequest<'_>, String> {
        LinkRequest::try_new(
            &self.references,
            &self.definitions,
            &self.policy,
            &self.mappings,
            &self.completeness,
            &self.graph,
            &self.limits,
        )
        .map_err(|error| error.to_string())
    }

    fn link(&self) -> Result<intlify_linker::LinkOutcome, String> {
        link(&self.request()?).map_err(|error| error.to_string())
    }

    fn input_count(&self) -> u64 {
        let definitions = self.definitions[0].definitions().len();
        let references = self.references[0].references().len();
        u64::try_from(definitions + references).expect("supported generated counts fit u64")
    }
}

type ProjectMeasurementOutput = (
    Vec<Measurement>,
    Vec<MessageDefinitionArtifact>,
    Vec<MessageReferenceArtifact>,
);

fn definition_key(shape: FixtureShape, index: usize) -> String {
    match shape {
        FixtureShape::BoundedSelector => format!("/group/{index:08}"),
        _ => format!("/key/{index:08}"),
    }
}

fn app_scope() -> CatalogScopeId {
    CatalogScopeId::new(
        ArtifactNamespace::Project,
        CatalogScopeName::try_new("app").expect("fixture scope is valid"),
    )
}

fn catalog_key(value: &str) -> Result<CatalogKey, String> {
    CatalogKey::try_new(DOMAIN, value).map_err(|error| error.to_string())
}

fn definition(
    scope: CatalogScopeId,
    key_value: &str,
    index: usize,
) -> Result<MessageDefinition, String> {
    MessageDefinition::try_new(
        scope,
        DOMAIN,
        catalog_key(key_value)?,
        Locale::try_new("en").map_err(|error| error.to_string())?,
        MessagePayload::try_new(format!("Message {index}")).map_err(|error| error.to_string())?,
        EntryReference::new(
            EntryStructuralPath::try_new(key_value).map_err(|error| error.to_string())?,
            0,
        ),
    )
    .map_err(|error| error.to_string())
}

fn reference(scope: CatalogScopeId, selector: MessageSelector) -> Result<MessageReference, String> {
    MessageReference::try_new(scope, DOMAIN, selector, None, None)
        .map_err(|error| error.to_string())
}

fn producer_identity() -> ProducerIdentity {
    ProducerIdentity::new(
        ProducerId::try_new("dev.intlify/messages-bench").expect("fixture producer ID is valid"),
        ProducerRevision::try_new("1").expect("fixture producer revision is valid"),
    )
}

fn definition_artifact(
    definitions: Vec<MessageDefinition>,
    limits: &LinkLimits,
) -> Result<MessageDefinitionArtifact, String> {
    MessageDefinitionArtifact::try_new(
        ArtifactVersion::DRAFT_V0_1,
        producer_identity(),
        source_identity(&["generated", "definitions.json"]),
        Vec::new(),
        InputFingerprint::new(
            FingerprintAlgorithm::Blake3_256,
            FingerprintDigest::new([0; 32]),
        ),
        definitions,
        limits,
    )
    .map_err(|error| error.to_string())
}

fn reference_artifact(
    references: Vec<MessageReference>,
    limits: &LinkLimits,
) -> Result<MessageReferenceArtifact, String> {
    MessageReferenceArtifact::try_new(
        ArtifactVersion::DRAFT_V0_1,
        producer_identity(),
        ReferenceArtifactIdentity::try_new(
            ArtifactNamespace::Project,
            vec![
                ReferenceArtifactSegment::try_new("generated")
                    .expect("fixture identity segment is valid"),
                ReferenceArtifactSegment::try_new("references")
                    .expect("fixture identity segment is valid"),
            ],
        )
        .map_err(|error| error.to_string())?,
        DeliveryUnitId::main(),
        references,
        limits,
    )
    .map_err(|error| error.to_string())
}

fn source_identity(path: &[&str]) -> SourceDocumentIdentity {
    SourceDocumentIdentity::new(
        ArtifactNamespace::Project,
        PortableRelativePath::try_new(
            path.iter()
                .map(|segment| {
                    PortablePathSegment::try_new(*segment)
                        .expect("fixture path segment is contract-valid")
                })
                .collect(),
        )
        .expect("fixture path is contract-valid"),
    )
}

struct DurationRecord<'a> {
    stage: MessageBenchmarkStage,
    fixture: &'a str,
    fixture_revision: u32,
    variant: &'a str,
    scale: usize,
    operation: &'static str,
    input_count: u64,
    output_count: u64,
    physical_group_count: Option<u64>,
    host_byte_count: Option<u64>,
    entry_count: Option<u64>,
    iterations: u64,
    elapsed: Duration,
    checksum: u32,
}

fn duration_record(options: DurationRecord<'_>) -> Measurement {
    Measurement {
        status: "measured",
        phase: options.stage.phase(),
        cost: options.stage.cost(),
        boundary_id: options.stage.boundary_id(),
        fixture: options.fixture.to_owned(),
        fixture_revision: options.fixture_revision,
        variant: options.variant.to_owned(),
        scale: options.scale,
        runtime: RUNTIME,
        execution_model: EXECUTION_MODEL,
        operation: options.operation,
        metric: "duration",
        input_count: options.input_count,
        output_count: options.output_count,
        physical_group_count: options.physical_group_count,
        host_byte_count: options.host_byte_count,
        entry_count: options.entry_count,
        iterations: Some(options.iterations),
        elapsed_ms: Some(options.elapsed.as_secs_f64() * 1_000.0),
        checksum: Some(options.checksum),
        peak_live_bytes: None,
        retained_live_bytes: None,
        allocation_count: None,
    }
}

fn memory_record(
    profile: &FixtureProfile,
    scale: usize,
    input_count: u64,
    output_count: u64,
    allocation: AllocationInfo,
) -> Result<Measurement, String> {
    let retained_live_bytes = u64::try_from(allocation.bytes_current).map_err(|_| {
        format!(
            "link memory interval released more bytes than it allocated: {}",
            allocation.bytes_current
        )
    })?;
    Ok(Measurement {
        status: "measured",
        phase: MessageBenchmarkStage::LinkCorePeakLiveMemory.phase(),
        cost: MessageBenchmarkStage::LinkCorePeakLiveMemory.cost(),
        boundary_id: MessageBenchmarkStage::LinkCorePeakLiveMemory.boundary_id(),
        fixture: profile.name.clone(),
        fixture_revision: profile.revision,
        variant: fixture_variant(profile.shape).to_owned(),
        scale,
        runtime: RUNTIME,
        execution_model: EXECUTION_MODEL,
        operation: MessageBenchmarkStage::LinkCorePeakLiveMemory.boundary_id(),
        metric: "peak_live_memory",
        input_count,
        output_count,
        physical_group_count: None,
        host_byte_count: None,
        entry_count: None,
        iterations: None,
        elapsed_ms: None,
        checksum: None,
        peak_live_bytes: Some(allocation.bytes_max),
        retained_live_bytes: Some(retained_live_bytes),
        allocation_count: Some(allocation.count_total),
    })
}

const fn fixture_variant(shape: FixtureShape) -> &'static str {
    match shape {
        FixtureShape::ExactReferenceDense => "exact_reference_dense",
        FixtureShape::DefinitionDenseSparseReference => "definition_dense_sparse_reference",
        FixtureShape::BoundedSelector => "bounded_selector",
        FixtureShape::FindingDense => "finding_dense",
    }
}

const fn is_js_stage(stage: MessageBenchmarkStage) -> bool {
    matches!(
        stage,
        MessageBenchmarkStage::SourceParse
            | MessageBenchmarkStage::RecognizerMatchAndStaticEvaluation
            | MessageBenchmarkStage::KeyAndProvenanceConstruction
            | MessageBenchmarkStage::ReferenceArtifactConstruction
            | MessageBenchmarkStage::CacheMissProduction
    )
}

struct Checksum {
    state: u32,
}

impl Checksum {
    const fn empty() -> Self {
        Self {
            state: 2_166_136_261,
        }
    }

    fn new(domain: &[u8]) -> Self {
        let mut checksum = Self::empty();
        checksum.write_raw(domain);
        checksum.write_raw(&[0]);
        checksum
    }

    fn write_u64(&mut self, tag: u8, value: u64) {
        self.write_field(tag, &value.to_be_bytes());
    }

    fn write_u32(&mut self, tag: u8, value: u32) {
        self.write_field(tag, &value.to_be_bytes());
    }

    fn write_str(&mut self, tag: u8, value: &str) {
        self.write_field(tag, value.as_bytes());
    }

    fn write_bytes(&mut self, tag: u8, value: &[u8]) {
        self.write_field(tag, value);
    }

    fn write_field(&mut self, tag: u8, value: &[u8]) {
        self.write_raw(&[tag]);
        self.write_raw(
            &u64::try_from(value.len())
                .expect("supported checksum fields fit u64")
                .to_be_bytes(),
        );
        self.write_raw(value);
    }

    fn write_raw(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= u32::from(*byte);
            self.state = self.state.wrapping_mul(16_777_619);
        }
    }

    const fn finish(self) -> u32 {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize)]
    struct ChecksumVectors {
        revision: u32,
        vectors: Vec<ChecksumVector>,
    }

    #[derive(Debug, Deserialize)]
    struct ChecksumVector {
        name: String,
        domain: String,
        occurrences: Vec<ChecksumOccurrence>,
        expected: u32,
    }

    #[derive(Debug, Deserialize)]
    struct ChecksumOccurrence {
        identity: String,
        checksum: u32,
    }

    #[test]
    fn occurrence_checksum_matches_shared_vectors() {
        let vectors: ChecksumVectors =
            serde_json::from_slice(include_bytes!("../../checksum-vectors.json")).unwrap();
        assert_eq!(vectors.revision, 1);
        for vector in vectors.vectors {
            let actual = checksum_observation_sequence(
                &vector.domain,
                vector
                    .occurrences
                    .iter()
                    .map(|occurrence| (occurrence.identity.as_str(), occurrence.checksum)),
            );
            assert_eq!(actual, vector.expected, "{}", vector.name);
        }
    }
}
