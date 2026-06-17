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

Run measurements with `hyperfine`.

```sh
vp run bench:mf-parser
```

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
