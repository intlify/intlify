# Formatter Benchmark

Local-first benchmark tooling for the Phase 3B MF2 formatter.

Run the full local formatter benchmark from the repository root:

```sh
vp run bench:format
```

Short smoke run:

```sh
vp run bench:format:smoke
```

The command builds the formatter N-API package, formatter WASM package, parser N-API package, and native CLI binary when needed. Results are written to:

```text
tools/format-bench/results/latest.json
```

## Phases

The benchmark result uses the Phase 3B phase names:

- `format_standard`
- `format_preserve`
- `format_check_cli_e2e`
- `format_check_json`
- `e2e_format`

The result schema also records the cost category for each measurement:

- `parse`
- `snapshot_encode`
- `snapshot_decode_access`
- `syntax_traversal_layout_construction`
- `rendering`
- `napi_binding_call`
- `wasm_binding_call`
- `cli_e2e`
- `cli_json_reporter`

`syntax_traversal_layout_construction` is one category because the Phase 3B formatter core builds Layout IR while traversing parser syntax. Timing thresholds are not enforced; slow results are observational only.

## Validation

Validate a generated result without comparing timing values:

```sh
vp run format-bench#validate
```

Benchmark jobs and issue-comment reporting in GitHub Actions are intentionally left as follow-up work.
