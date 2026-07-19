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

The command builds the formatter N-API package, formatter WASM package, parser N-API package, and native CLI binary in release mode when needed. Results are written to:

```text
tools/format-bench/results/latest.json
```

By default, the WASM build keeps wasm-pack's downloaded tools under `node_modules/.cache/wasm-pack`. This makes the first build writable in restricted environments and completes toolchain preparation before benchmark timing starts. Set `WASM_PACK_CACHE` to override the cache location.

## Measurement model

The result schema separates startup observations from steady-state measurements:

- `coldStartResults` records module loading, WASM runtime initialization, the first API call, and the first CLI process separately.
- `results` contains only warm measurements. Each operation runs three unmeasured warm-up iterations by default before its timer starts.
- API measurements use `executionModel: "in_process"`. CLI measurements use `executionModel: "fresh_process"` because every iteration launches the CLI again; its warm-up removes first-run host cache effects but does not remove process startup.

Use `--warmup-iterations <count>` to change the unmeasured warm-up count. The first selected fixture supplies the representative first-call and first-process startup records. Startup records are diagnostic observations and are not added to warm timing.

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

`syntax_traversal_layout_construction` is one category because the Phase 3B formatter core builds Layout IR while traversing parser syntax. Timing thresholds are not enforced; startup and warm results are observational only.

## Validation

Validate a generated result without comparing timing values:

```sh
vp run format-bench#validate
```

Benchmark jobs and issue-comment reporting in GitHub Actions are intentionally left as follow-up work.
