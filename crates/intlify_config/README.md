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
- Internal configuration validation, named-profile selection, and normalization of the source, requested, and default locales.
- Optional developer measurements for the implemented configuration operations.

The project-profile path does not yet produce a public `LocalizationProjectProfile` or replace the CLI's existing configuration workflow. Locale normalization currently uses a finite test provider, not a production locale-data implementation. Full policy resolution, fallback and negotiation, and output-target settings remain follow-up work.

## Verification

For contributors, run these commands from the repository root.

Run the crate's tests, including its optional measurement support:

```sh
cargo test -p intlify_config --all-targets --all-features
```

Check that the generated project-profile JSON Schema is up to date:

```sh
cargo run -p intlify_config --example generate_config_schema -- --check
```

On Linux or macOS, run the measurement smoke check:

```sh
vp run bench:config:smoke
```

The smoke check exercises the implemented operations, saves measurement records to a new temporary directory, and validates the saved records. It prints the output location. It checks the measurement workflow, not whether the crate meets a performance budget. Measurement support is disabled in normal library builds.

For configuration test scenarios, see the [minimum fixture index](./fixtures/minimum/README.md).

## Further reading

- [Intlify overview](../../design/000-intlify-overview-design.md): the source-first localization concept and overall architecture.
- [Project profile and locale policy design](../../design/015-intlify-project-profile-and-locale-policy-design.md): configuration semantics, the intended profile resolver, and implementation scope.
- [Conformance and measurement design](../../design/026-intlify-conformance-and-measurement-design.md): shared correctness and performance-verification requirements.

## License

MIT
