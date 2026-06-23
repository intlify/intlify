# @intlify/ox-mf2-wasm

Portable WASM runtime package for ox-mf2.

Use this package for browser, bundler, editor extension, edge runtime, and other environments where native addons are unavailable. Node.js users should prefer `@intlify/ox-mf2-napi` when native addons are acceptable.

```ts
import { init, parseMessage } from '@intlify/ox-mf2-wasm'

await init()

const result = parseMessage('Hello {$name}')
const bytes = result.snapshot.toBytes()
```

The public API matches the N-API package where possible. `parseMessage()` and `parseBatch()` return Binary AST snapshot-backed result objects rather than a nested JSON AST. `snapshot.toBytes()` returns a copy, and `decodeSnapshot(bytes)` copies and validates the input bytes.

Decoded snapshots that do not embed source text can reattach it with `withSources()`. Spans and `SourceLocation.column` use UTF-8 byte offsets. JavaScript strings with unpaired surrogates are rejected. Recoverable parser diagnostics are returned on the result and are not thrown.

WASM initialization is explicit. Calling parser APIs before `await init()` throws `OxMf2InitializationError`.

Phase 2 does not provide an `@intlify/ox-mf2` wrapper package, automatic N-API fallback, UTF-16 location helpers, formatter API, linter API, or semantic snapshot exposure.
