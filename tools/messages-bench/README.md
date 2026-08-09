# Message Linker Benchmarks

Local-first benchmark tooling for project inventory, JavaScript/TypeScript reference production, artifact codecs, definition projection, semantic linking, typed-key model construction, locale-fallback linking, ESM export preparation and rendering, output registration, `messages emit`, artifact payload size, allocator-observed peak live memory, and the complete in-process project workflows.

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

`fixture-selection.json` owns the four link-core workload shapes, typed-key-model and locale-fallback workloads, the ESM export and registration workloads, the `messages emit` project workload, and the representative project fixture. Every generated shape has at least three increasing scales. `benchmark-profile.mjs` owns the exact required-case matrix and one-to-one E2E/resource-extraction companion mappings. `benchmark-phases.mjs` owns the active phase/cost table, interval boundaries, boundary-free artifact-size metric, and allowed overlap topology while importing the exact resource-owned extraction descriptor unchanged.

Warmups complete before measurement and contribute to no elapsed value, count, checksum, or state transition. Stateful output cases reconstruct their absent, unchanged, matched, or different snapshot before every warmup and measured repetition. Every measured repetition must produce the same semantic checksum and structural counts. Duration, allocator, and payload-size values are observations only: CI validates build success, required-case coverage, result shape, artifact fingerprints and bucket reconciliation, boundary topology, companion integrity, and within-invocation determinism, but applies no performance threshold and uses no committed machine baseline.

The active profile measures coverage-baseline selection and checked typed-key model construction separately at 16, 64, and 256 keys. It also measures fallback-chain construction, locale-aware resolution, and locale-finding materialization separately for 16, 64, and 256 non-baseline production locales.

The ESM profile compares ordinary linked output with a benchmark-only full-retention outcome at 4, 16, and 64 keys. It records preparation and rendering intervals, canonical artifact observations, exporter-supplied locale and delivery-unit associations, eager-load reachability, and complete-set, initial-load, locale, delivery-unit, and artifact-kind payload buckets. Registration and `messages emit` use the same scales and keep write-absent, write-unchanged, check-matched, and check-different cases distinct. Fresh-process command timing, lint integration, additional exporters, and numeric regression gates remain outside the active profile.
