# MF Parser Benchmark

Local benchmark tooling for measuring MF2 / MF1 parser throughput and compatibility.

## Targets

MF2:

- `messageformat-parse-message`
- `messageformat-parse-cst`
- `messageformat-cst-to-message`
- `messageformat-constructor`
- `ox-content-parse`
- `ox-content-parse-and-validate`
- `mf2-tools-parse`
- `mf2-tools-parse-and-analyze`
- `ox-mf2-parse`
- `ox-mf2-parse-session`
- `ox-mf2-parse-session-no-trivia`
- `ox-mf2-parse-and-lower`

MF1 / ICU:

- `formatjs-icu-parse`

## Setup

Install dependencies from the repository root.

```sh
vp install
```

If reference repositories are not initialized yet, initialize the submodules first.

```sh
git submodule update --init --depth 1 --recursive
```

Prepare benchmark artifacts and environment metadata.

```sh
vp run mf-parser-bench#setup
```

## Preflight

Before measuring performance, check whether each parser can parse each corpus.

```sh
vp run mf-parser-bench#preflight
```

The result is written to `tools/mf-parser-bench/.tmp/preflight.json`. Targets listed in `unsupportedBy` are treated as unsupported and excluded from performance measurements.

## Calibration

Calibration chooses the internal iteration count for each target / corpus pair so that `hyperfine` process startup overhead does not dominate the measurement.

```sh
vp run mf-parser-bench#calibrate
```

The result is written to `tools/mf-parser-bench/.tmp/calibration.json`.

## Benchmark

Run the full benchmark suite (parser corpora, Node.js bindings, and browser WASM) from the repository root:

```sh
vp run bench
```

Short smoke run:

```sh
vp run bench:smoke
```

Parser-only measurements with `hyperfine`:

```sh
vp run bench:mf-parser
```

## Binding benchmarks

Node.js N-API and WASM binding throughput is measured separately from parser corpora so binding cost stays visible.

Operations:

- `parse-message`
- `parse-batch`
- `decode-snapshot`
- `snapshot-to-bytes-copy`

Runtimes:

- `ox-mf2-napi`
- `ox-mf2-wasm`

Setup builds `@intlify/ox-mf2-napi` and `@intlify/ox-mf2-wasm` when `dist/` artifacts are missing.

Binding benchmarks reuse the parser calibration policy: each runtime / operation pair picks an iteration count so one invocation spends about `MF_PARSER_BENCH_CALIBRATE_MS` (default `200` ms). Results are stored in `tools/mf-parser-bench/.tmp/calibration.json` under `bindings`.

```sh
vp run bench:bindings
```

Node.js binding throughput only (after setup):

```sh
vp run mf-parser-bench#bench:bindings:run
```

Short Node.js binding smoke run from the repository root:

```sh
vp run bench:bindings:smoke
```

Browser WASM benchmarks run through Playwright and share the same binding setup step.

```sh
vp run bench:bindings:browser
```

Short browser WASM smoke run from the repository root:

```sh
vp run bench:bindings:e2e:smoke
```

Browser WASM benchmarks use the calibrated `wasm / <operation>` iteration counts by default. Override all browser targets with `OX_MF2_BROWSER_BENCH_ITERATIONS` when needed.

`WARMUP` and `RUNS` can be overridden with environment variables.

```sh
bash tools/mf-parser-bench/scripts/mf-parser-hyperfine.sh --warmup 1 --runs 3
```

Use the dedicated script for a short smoke test through Vite+.

```sh
vp run bench:mf-parser:smoke
```

## Results

Generated artifacts are not tracked by git.

- `results/raw/*.json`: `hyperfine --export-json`
- `results/raw/*.md`: `hyperfine --export-markdown`
- `results/normalized/*.json`: workload summaries emitted by the runners
- `results/reports/latest.md`: aggregated report

## Build profile

The bench harness builds with `lto = "fat"` and `codegen-units = 1` by default (see `rs/Cargo.toml`). That matches what a production deployment of any of the benched parsers would use.

For local "how fast can it go on _this_ machine?" measurements you can layer `target-cpu=native` on top — it gives the compiler permission to emit instructions that only run on the host CPU (e.g. M1 SIMD):

```sh
RUSTFLAGS="-C target-cpu=native" vp run mf-parser-bench#setup
RUSTFLAGS="-C target-cpu=native" vp run mf-parser-bench#bench
```

Do NOT commit the resulting binary or quote those numbers as production figures — they are not portable across CPUs. PGO (profile-guided optimisation) was evaluated and gave no measurable improvement on top of fat LTO for this workload, so it is not part of the default build.

## Validation

After changing the implementation, run the normal Vite+ validation from the repository root.

```sh
vp check
vp test
```

If setup, runtime, or package-manager behavior looks wrong, check the environment with:

```sh
vp env doctor
```

## License Notes

This tooling references the following external parser implementations.

- `messageformat`: Apache-2.0
- `formatjs`: MIT
- `ox-content`: MIT
- `mf2-tools`: GPL-3.0-or-later

`mf2-tools` is GPL-3.0-or-later. For now, this is treated as local benchmark tooling and is not included in any published package.
