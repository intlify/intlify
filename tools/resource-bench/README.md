# Resource Catalog Benchmark

Local-first acceptance tooling for the Tier 1 JSON resource adapter and catalog formatter integration.

Run the full benchmark from the repository root:

```sh
vp run bench:resource
```

Run one timing iteration while retaining all phase, memory-growth, and determinism checks:

```sh
vp run bench:resource:smoke
```

Results are written to `tools/resource-bench/results/latest.json`. Validate an existing result without rerunning the benchmark:

```sh
vp run bench:resource:validate
```

`INTLIFY_RESOURCE_BENCH_CORE_BINARY` and `INTLIFY_RESOURCE_BENCH_CLI_BINARY` can select explicit prebuilt executables when running `scripts/run.mjs --skip-build`. Relative overrides are resolved from the command's working directory. An existing executable that fails or emits invalid output remains a fatal benchmark error; `--allow-skips` only permits unavailable executables.

## Phases

The result keeps resource, formatter, aggregation, and file-I/O costs observable as separate records:

- `resource_extract`: JSON host parsing and message-entry extraction
- `resource_extract_peak_memory`: original and candidate re-extraction peak live allocations
- `resource_write_back`: re-escaping measurement, raw materialization/edit composition, and full candidate reparse/validation
- `fmt_catalog_output_admission_peak_memory`: ordinary message parsing/formatting, raw-order admission, and their combined peak live allocations
- `fmt_catalog_check_e2e`: catalog read/check CLI pipeline
- `fmt_catalog_write_e2e`: catalog read/write CLI pipeline
- `sequential_physical_group_aggregation`: deterministic multi-file aggregation including a hard-link alias group

Generated profiles cover a message-dense catalog and a structurally dense catalog with one message. Three increasing input sizes are used for each original/candidate extraction path. The core command rejects normalized step growth above the documented `2.5` near-linear tolerance; the JSON result records every sample and the resulting check.

Elapsed timing values are observational and never pass/fail thresholds. The executable command, complete phase/cost vocabulary, result schema, memory-growth contract, catalog CLI fixture, and deterministic aggregation are the acceptance gates.

GitHub Actions trend jobs and issue-comment reporting remain deferred. Normal `vpr check` and `vpr test` do not run the release benchmark.
