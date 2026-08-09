# Message Linker CLI Example

This example runs the complete `intlify messages emit` workflow against a small JavaScript/TypeScript-style application:

1. `src/app.ts` supplies a static `t('title')` message reference.
2. The English and Japanese JSON catalogs supply matching definitions.
3. The message linker resolves the reference for both production locales.
4. The ESM exporter writes the selected locale modules and loader under `generated/messages`.

## Project Layout

```text
examples/message-linker-cli
├── intlify.config.json
├── locales
│   ├── en.json
│   └── ja.json
└── src
    └── app.ts
```

The paths in `intlify.config.json` start with `examples/message-linker-cli/` because the CLI detects the repository Git root as the project root. In a standalone application, the equivalent paths would normally be `locales/*.json`, `src/**/*.ts`, and `generated/messages`.

## Run the Example

From the repository root, build the native CLI:

```sh
vp run build:cli
```

Generate the message delivery artifacts:

```sh
./target/release/intlify messages emit \
  --config examples/message-linker-cli/intlify.config.json
```

The generated directory contains deterministic ESM locale artifacts, `loader.mjs`, a typed accessor for the baseline scope, and `.intlify-output-manifest.json`.

Verify that the checked-in inputs still produce the existing output without rewriting it:

```sh
./target/release/intlify messages emit \
  --config examples/message-linker-cli/intlify.config.json \
  --check
```

The check exits with `0` when the output matches. Edit a catalog or `src/app.ts` and run the check again to observe a difference, then rerun write mode to update the generated artifacts.

Use `--reporter=json` with either command to inspect the stable machine-readable result.

> [!NOTE] Message delivery write mode is not currently supported on Windows because durable directory flushing is unavailable. Read-only `--check` mode remains supported.
