# Message Linker Benchmarks

Local-first benchmark tooling for project inventory, JavaScript/TypeScript reference production, artifact codecs, definition projection, semantic linking, typed-key model construction, locale-fallback linking, allocator-observed peak live memory, and the complete in-process project-link workflow.

Run the release-profile benchmark from the repository root:

```sh
vp run bench:messages
```

Run the one-repetition acceptance smoke:

```sh
vp run bench:messages:smoke
```

Results are written to `tools/messages-bench/results/latest.json`. Validate an existing result without rerunning:

```sh
vp run bench:messages:validate
```

The non-published Rust runner is the only supported caller of the non-default `benchmark` features. Product CLI and package builds do not enable those features or expose a message-linker command.

`fixture-selection.json` owns the four link-core workload shapes, the typed-key-model workload, the locale-fallback-expansion workload, and the representative project fixture. Every generated shape has at least three increasing scales. `benchmark-profile.mjs` owns the exact required-case matrix and the one-to-one E2E/resource-extraction companion mapping. `benchmark-phases.mjs` owns the active phase/cost table, messages-owned boundaries, and allowed overlap topology while importing the exact resource-owned extraction descriptor unchanged.

Warmups complete before measurement and contribute to no elapsed value, count, or checksum. Every measured repetition must produce the same semantic checksum and structural counts. Duration and allocator values are observations only: CI validates build success, required-case coverage, result shape, boundary topology, companion integrity, and within-invocation determinism, but applies no timing or memory regression threshold and uses no committed machine baseline.

The active profile measures coverage-baseline selection and checked typed-key model construction separately at 16, 64, and 256 keys. It also measures fallback-chain construction, locale-aware resolution, and locale-finding materialization separately for 16, 64, and 256 non-baseline production locales. It intentionally excludes exporters, artifact-size comparison, fresh-process command timing, lint integration, and user-facing message commands.
