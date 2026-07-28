# Message definition artifact conformance fixtures

These fixtures pin the mutable v0.1 JSON contract implemented by `intlify_contract`.

- `v0.1/canonical.json` exercises a canonical primary source, one logical alias, the exact structural fingerprint envelope, an empty root key and message, opaque MF2 source text, and raw-entry definition order.
- `v0.1/zero-definitions.json` proves that one successful zero-entry source is still a complete artifact.
- `v0.1/noncanonical-input.json` uses non-semantic object-member order, whitespace, and an equivalent Unicode escape. Decoding it and encoding the resulting value must produce `zero-definitions.json`.

Fixture files carry one repository framing newline. Canonical artifact payload bytes are the file contents without that final newline because the wire writer emits no insignificant whitespace or final newline.

The JSON Schema documents structural interoperability. The Rust codec remains authoritative for decoded-byte limits, domain-specific key grammar, canonical alias ordering, occurrence continuity, deterministic error precedence, and bounded reader behavior that JSON Schema cannot express.
