# intlify_config

Shared configuration support for [Intlify](../../design/000-intlify-overview-design.md), a toolchain for compiling localization from application source.

> [!IMPORTANT]
>
> This crate is currently a minimum implementation. A complete project-profile resolver and an application-facing `LocalizationProjectProfile` API are not yet available. See [Current status](#current-status) for the implemented scope and remaining work.

`intlify_config` is the Rust crate that brings Intlify's project-configuration rules into one place. Its purpose is to give the CLI and compiler stages a consistent understanding of a project's languages and localization settings, so each tool does not need its own configuration parser or defaults.

## Role in Intlify

The configuration design separates two things:

- `intlify.config.json`: the configuration maintained in an application repository.
- `LocalizationProjectProfile`: the checked, in-memory settings that compiler stages are intended to consume.

A profile describes one application's localization project: which languages its messages are written in, which locales the application should support, and which localization policies and output targets apply. One repository configuration can declare several named profiles, for example for different applications in a monorepo.

The CLI or another host is responsible for finding and reading the configuration file. This crate is responsible for interpreting and validating the supplied configuration. The intended result is one checked profile that downstream tools can share.

This crate handles configuration, not message translation or runtime formatting. The complete profile resolver described above is still under development.

## Current status

This is an unpublished, workspace-internal crate. It is not yet an application-facing configuration API.

The current implementation includes:

- Shared JSON decoding, error locations, and JSON Schema utilities already used by the existing Intlify CLI.
- A generated [JSON Schema for project-profile configuration](./schema/project-profile-config-v0.schema.json).
- Internal configuration validation, named-profile selection, and locale resolution: every locale spelling of the selected profile is counted, the spellings outside the coverage rules are canonicalized, requested-locale sets are checked for duplicates, and the source, requested, and default locales are resolved.
- An optional locale provider over the ICU4X 2.2.0 reference adapter, [`intlify_locale_icu4x`](../intlify_locale_icu4x/README.md), behind the non-default `icu4x` feature. It admits a locale canonicalization data artifact from bytes its caller supplies, and the boundary binds it only when it carries the one supported specification, revision `"0"`, with its CLDR 48.2.0 dataset.
- Optional developer measurements for the implemented configuration operations. Locale canonicalization is measured through both the finite test provider and the ICU4X provider, which runs once with an engine reused across calls and once with a new engine for each call.

With the ICU4X provider and its measurements, Phase 2 of design 015, the locale canonicalization boundary, is complete. The [Phase 2 fixtures](./fixtures/phase2/README.md) map its coverage to the tests that check it and name what later phases own.

The project-profile path does not yet produce a public `LocalizationProjectProfile` or replace the CLI's existing configuration workflow. No configuration path uses the ICU4X provider yet: locale normalization in the example still uses a finite test provider, and only the measurements run the ICU4X provider. Full policy resolution, fallback and negotiation, and output-target settings remain follow-up work.

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

On Linux or macOS, run the measurement smoke check:

```sh
vp run bench:config:smoke
```

The smoke check exercises the implemented operations, saves measurement records to a new temporary directory, and validates the saved records. It prints the output location. It checks the measurement workflow, not whether the crate meets a performance budget. Measurement support is disabled in normal library builds. When enabled, it also enables the `icu4x` feature and compiles in the adapter's committed locale canonicalization data as measurement input.

For configuration test scenarios, see the [minimum fixture index](./fixtures/minimum/README.md). For locale canonicalization, see the [Phase 2 fixtures](./fixtures/phase2/README.md). For the vocabulary artifact, see the [vocabulary fixtures](./fixtures/vocabulary/README.md).

## Further reading

- [Intlify overview](../../design/000-intlify-overview-design.md): the source-first localization concept and overall architecture.
- [Project profile and locale policy design](../../design/015-intlify-project-profile-and-locale-policy-design.md): configuration semantics, the intended profile resolver, and implementation scope.
- [Conformance and measurement design](../../design/026-intlify-conformance-and-measurement-design.md): shared correctness and performance-verification requirements.

## License

MIT
