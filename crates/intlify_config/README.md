# intlify_config

Shared configuration support for [Intlify](../../design/000-intlify-overview-design.md), a toolchain for compiling localization from application source.

> [!IMPORTANT]
>
> This crate is not yet an application-facing configuration API. Its project-profile resolver is crate-private, and no `LocalizationProjectProfile` API is available yet. See [Current status](#current-status) for the implemented scope and remaining work.

`intlify_config` is the Rust crate that brings Intlify's project-configuration rules into one place. Its purpose is to give the CLI and compiler stages a consistent understanding of a project's languages and localization settings, so each tool does not need its own configuration parser or defaults.

## Role in Intlify

The configuration design separates two things:

- `intlify.config.json`: the configuration maintained in an application repository.
- `LocalizationProjectProfile`: the checked, in-memory settings that compiler stages are intended to consume.

A profile describes one application's localization project: which languages its messages are written in, which locales the application should support, and which localization policies and output targets apply. One repository configuration can declare several named profiles, for example for different applications in a monorepo.

The CLI or another host is responsible for finding and reading the configuration file. This crate is responsible for interpreting and validating the supplied configuration. The intended result is one checked profile that downstream tools can share.

This crate handles configuration, not message translation or runtime formatting. The profile resolver described above exists inside the crate, but no host calls it yet.

## Current status

This is an unpublished, workspace-internal crate. It is not yet an application-facing configuration API.

The current implementation includes:

- Shared JSON decoding, error locations, and JSON Schema utilities already used by the existing Intlify CLI.
- A generated [JSON Schema for project-profile configuration](./schema/project-profile-config-v0.schema.json).
- Internal configuration validation, named-profile selection, and the semantic resolution of the selected profile: the specification input, the Resource Limit Policy bootstrap that confirms the selection, project identity, every locale occurrence, locale negotiation aliases, message fallback, coverage with its surface-class vocabulary, the admission of the submitted artifact collection with its exact references and bindings, delivery placement, each target's locale subset, effective default, and Target Profile directions, the exact partition of the targets into Deployment Compatibility Groups, and their hydration relations between the roles each Target Profile declares. The result is crate-private semantic state, not a profile.
- Internal resolver construction under the built-in Resolver Construction Root Package, with a bounded Draft 7 validator, the closed pre-invocation boundaries, and an invocation whose checked or blocked outcome carries its Resolver Construction Identity and Resolution Evidence, and, when checked, the profile view and the bindings. None of it is public.
- An optional locale provider over the ICU4X 2.2.0 reference adapter, [`intlify_locale_icu4x`](../intlify_locale_icu4x/README.md), behind the non-default `icu4x` feature. It admits a locale canonicalization data artifact from bytes its caller supplies, and the boundary binds it only when it carries the one supported specification, revision `"0"`, with its CLDR 48.2.0 dataset.
- Optional developer measurements for the implemented configuration operations, including locale-policy resolution, the resource policy bootstrap, collection admission, binding construction, target and group resolution, hydration validation, resolver construction with its identity, the Shared projection, Entry projection, and outcome finalization stages of the Evidence materialization over a file entry's invocation, a file entry's complete invocation, from the admission of what a binding submits through the outcome, on a resolver constructed for each call, on one reused across calls, and on one reused right after a blocked outcome, and the admission of an already constructed programmatic Snapshot through the document of its value, and a programmatic entry's complete invocation in the same three states and its Evidence materialization stages, whose Snapshot of a file's value shares that file's Shared outcome, and the replay of a file invocation's Materialized Resolution Input Set. Locale canonicalization and locale-policy resolution are measured through both the finite test provider and the ICU4X provider, which runs once with an engine reused across calls and once with a new engine for each call.

With the ICU4X provider and its measurements, Phase 2 of design 015, the locale canonicalization boundary, is complete. The [Phase 2 fixtures](./fixtures/phase2/README.md) map its coverage to the tests that check it and name what later phases own.

Phase 3, the semantic profile resolver, is complete up to that crate-private state, and the [Phase 3 fixtures](./fixtures/phase3/README.md) map its coverage the same way. It reads design 018's Resource Limit Policy and Trust Policy bodies and admits the bootstrap candidate by the exact pins of 018's Verification Input, which tests construct until Phase 5 reads it at the pre-invocation boundary.

Phase 4, targets and compatibility groups, extends that state with target resolution, the group partition, and hydration validation, and fixes the [Group ID selection vectors](./conformance/README.md) design 020 verifies. The [Phase 4 fixtures](./fixtures/phase4/README.md) map its coverage. It is complete: each Target Profile declares its hydration roles in its body and has a direction for each locale its targets request, and two Target Profiles are compatible by design 024's closed rule.

Phase 5, resolver construction, invocation, and Evidence, constructs one immutable resolver under the built-in Resolver Construction Root Package, admits each input of an invocation at its closed boundary, and returns a checked or blocked outcome with its Resolver Construction Identity, Findings, Resolution Evidence under the reporting bounds, the profile view and its semantic digest, and the inputs a replay reads. The [Phase 5 fixtures](./fixtures/phase5/README.md) map its coverage. It is complete inside the crate: the resolver, its boundaries, and its outcomes are crate-private, and only the tests, the measurements, and the developer example construct a resolver.

The project-profile path does not yet produce a public `LocalizationProjectProfile` or replace the CLI's existing configuration workflow. No binding or CLI calls the resolver yet, so no configuration path uses the ICU4X provider: locale normalization in the example still uses a finite test provider, and only the tests and the measurements run the ICU4X provider. The conformance suite (Phase 6) and product integration (Phase 7) remain follow-up work.

## Try it from the terminal

Run the developer example with an editable sample configuration:

```sh
cargo run -p intlify_config --features dev-example --example resolve_config -- \
  crates/intlify_config/examples/resolve_config/intlify.config.json
```

Run this from the repository root. The example prints normalized locales, suggested spelling corrections, or configuration errors without modifying the file. It is separate from the existing CLI and uses the finite test locale provider, not production locale data.

See [the example guide](./examples/resolve_config/README.md) for changes to try, profile selection, JSON output, and limitations.

## Verification

For contributors, run these commands from the repository root.

Run the crate's tests, including its optional ICU4X provider and measurement support:

```sh
cargo test -p intlify_config --all-targets --all-features
```

Check that the generated project-profile JSON Schema is up to date:

```sh
cargo run -p intlify_config --example generate_config_schema -- --check
```

Check the Intent Surface-Class Vocabulary Artifact's schema, and its vectors against an independent implementation:

```sh
vp run schema:vocabulary:check
vp run vectors:vocabulary:check
```

Check the Resource Limit Policy Verification Input vectors, and the outcomes of its pre-invocation boundary, against an independent implementation:

```sh
vp run vectors:verification:check
```

Check the artifact envelope vectors against an independent implementation:

```sh
vp run vectors:envelope:check
```

Check design 015's record framing and Resolver Construction Identity text against an independent implementation:

```sh
vp run vectors:framing:check
```

Check design 015's bound values and the capability bound inventory against an independent implementation:

```sh
vp run vectors:bounds:check
```

Check that the committed Resolver Construction Root Package, construction authorities, and Registry Package members are the ones this crate writes and pins, against an independent implementation:

```sh
vp run construction:check
```

Check the regular expressions of Draft 7's `pattern` against JavaScript's own engine:

```sh
vp run vectors:pattern:check
```

Check how Draft 7's keywords read numbers, compare JSON values, resolve URI references, and follow JSON Pointers against an independent implementation:

```sh
vp run vectors:validator-values:check
```

Check how Draft 7 schema documents are read, counted, and resolved against an independent implementation:

```sh
vp run vectors:validator-schemas:check
```

Validating an instance against a read schema is checked by the library's unit tests, against the JSON Schema Test Suite's Draft 7 tests and the `jsonschema` crate:

```sh
cargo test -p intlify_config --lib construction::validator
```

How construction checks the authorities it is given, row by row of the construction-failure precedence, is checked by the library's unit tests:

```sh
cargo test -p intlify_config --lib construction
```

Check the profile view of a checked profile and its profile semantic digest against the resolver and an independent implementation:

```sh
vp run vectors:profile-view:check
```

On Linux or macOS, run the measurement smoke check:

```sh
vp run bench:config:smoke
```

The smoke check runs two measurements. The duration run times the implemented operations. The memory run observes the peak live memory of resolver construction and of each entry's complete invocation in a separate binary, [`intlify_config_memory_bench`](../intlify_config_memory_bench/README.md), because the allocator observer it counts with installs its own global allocator. Each run saves its measurement records to a new temporary directory, validates the saved records, and prints the output location. The smoke check checks the measurement workflow, not whether the crate meets a performance budget. Measurement support is disabled in normal library builds. When enabled, it also enables the `icu4x` feature and compiles in the adapter's committed locale canonicalization data as measurement input.

For configuration test scenarios, see the [minimum fixture index](./fixtures/minimum/README.md). For locale canonicalization, see the [Phase 2 fixtures](./fixtures/phase2/README.md). For the vocabulary artifact, see the [vocabulary fixtures](./fixtures/vocabulary/README.md). For the Resource Limit Policy Verification Input, see the [verification input fixtures](./fixtures/verification/README.md). For the members of the artifact collection, see the [envelope fixtures](./fixtures/envelope/README.md). For 015's record framing, see the [framing fixtures](./fixtures/framing/README.md). For 015's bound values and capability bounds, see the [bound fixtures](./fixtures/bounds/README.md). For the built-in Resolver Construction Root Package, see the [construction artifacts](./conformance/construction/README.md). For the Profile Specification, the Configuration Schema Authority Set, and the Resolver Capability Specification that construction is given, and how construction checks them, see the [construction authorities](./conformance/authorities/README.md). For the Registry Package's members, see the [registries](./conformance/registries/README.md). For the regular expressions of Draft 7's `pattern`, see the [pattern fixtures](./fixtures/pattern/README.md). For the numbers, JSON values, URI references, and JSON Pointers Draft 7's keywords read, see the [validator value fixtures](./fixtures/validator-values/README.md). For how Draft 7 schema documents are read, see the [validator schema fixtures](./fixtures/validator-schemas/README.md), and for how an instance is validated against them, the [validation fixtures](./fixtures/json-schema-test-suite/README.md). For the Group ID selection vectors handed to design 020, see the [conformance suite](./conformance/README.md), which holds them as Handoff Vectors. For the profile view of a checked profile and its semantic digest, see the [profile view vectors](./fixtures/profile-view/README.md).

## Further reading

- [Intlify overview](../../design/000-intlify-overview-design.md): the source-first localization concept and overall architecture.
- [Project profile and locale policy design](../../design/015-intlify-project-profile-and-locale-policy-design.md): configuration semantics, the intended profile resolver, and implementation scope.
- [Conformance and measurement design](../../design/026-intlify-conformance-and-measurement-design.md): shared correctness and performance-verification requirements.

## License

MIT
