# ox_mf2_parser

High performance MessageFormat 2 parser core for the intlify ox-mf2 packages.

This crate parses Unicode MessageFormat 2 messages into a recovering, lossless concrete syntax tree (CST). It also exposes diagnostics, explicit source-backed semantic construction and validation, and a Binary AST snapshot format used by the N-API and WASM bindings.

## Installation

```toml
[dependencies]
ox_mf2_parser = "0.1"
```

## Quick Start

```rust
use ox_mf2_parser::parse_message;

let parsed = parse_message("Hello, {$name}!").expect("valid message should parse");
let result = parsed.result();

assert!(result.diagnostics.is_empty());
assert!(result.cst.node_count() > 0);
```

`parse_message` is the one-shot convenience API. Its `StandaloneParseResult` keeps the syntax result paired with the private source owner needed by source-backed consumers.

## Diagnostics

Malformed input is parsed with recovery enabled by default. Diagnostics carry stable codes, severity, source spans, and messages.

```rust
use ox_mf2_parser::{parse_message, DiagnosticSeverity};

let parsed = parse_message("Hello, {$name").expect("malformed message should recover");
let result = parsed.result();

assert!(!result.diagnostics.is_empty());
assert_eq!(result.diagnostics[0].severity, DiagnosticSeverity::Error);
```

## Semantic Model

Semantic construction is an explicit phase after a diagnostic-free parse.

```rust
use ox_mf2_parser::{
    build_semantic_model, parse_source, validate_semantics, ParseOptions,
    SourceFileInput, SourceStore,
};

let mut sources = SourceStore::new();
let source = sources.add(SourceFileInput {
    source: "Hello, {$name}!",
    path: Some("message.mf2"),
    locale: Some("en"),
    message_id: Some("greeting"),
    base_offset: None,
});

let result = parse_source(&sources, source, ParseOptions::default())
    .expect("source should parse");
assert!(result.diagnostics.is_empty());

let model = build_semantic_model(&sources, &result)
    .expect("semantic model should build");
let diagnostics = validate_semantics(&model)
    .expect("semantic model invariants should hold");
assert!(diagnostics.is_empty());
```

Use `parse_source_session` with `ParseWorkspace` when repeated parsing should reuse allocations.

## Binary AST Snapshot

The `snapshot` module encodes parse results into a compact binary format and decodes that format into zero-copy views.

```rust
use ox_mf2_parser::{snapshot, ParseOptions};

let snapshot = snapshot::parse_message_to_snapshot(
    "Hello, {$name}!",
    None,
    ParseOptions::default(),
    snapshot::SnapshotOptions::default(),
)
.unwrap();

let view = snapshot::decode_snapshot(&snapshot.bytes).unwrap();
assert_eq!(view.root_count(), 1);
```

The snapshot format is intended for language bindings, persistence, and cross-process transfer. For in-process Rust usage, prefer `ParseResult` and `CstView`.

## Relationship to npm packages

This crate is the Rust parser core used by:

- `@intlify/ox-mf2-napi`
- `@intlify/ox-mf2-wasm`

Most JavaScript users should install one of those npm packages instead of using this crate directly.

## Stability

`ox_mf2_parser` is pre-1.0. Public APIs are documented and intended to be usable, but minor releases may still refine names, shape, and snapshot details while MessageFormat 2 integration work continues.

## License

MIT
