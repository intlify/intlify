# @intlify/ox-mf2-napi

Node.js 22+ N-API runtime package for ox-mf2.

This package exposes the Phase 2 snapshot-backed API. `parseMessage()` and `parseBatch()` return result objects backed by a Binary AST snapshot accessor; they do not materialize a nested JSON AST on the hot path.

```ts
import { parseMessage } from '@intlify/ox-mf2-napi'

const result = parseMessage('Hello {$name}')
const root = result.root.node()
const bytes = result.snapshot.toBytes()
```

`snapshot.toBytes()` returns a copy. `decodeSnapshot(bytes)` copies and validates the input bytes before creating a decoded result. Decoded snapshots that do not embed source text can reattach it with `withSources()`.

Spans and `SourceLocation.column` use UTF-8 byte offsets. JavaScript strings with unpaired surrogates are rejected. Recoverable parser diagnostics are returned on the result and are not thrown.

Editor integrations can convert between parser UTF-8 byte offsets and JavaScript UTF-16 code unit offsets with `utf8ByteOffsetToUtf16Offset()` and `utf16OffsetToUtf8ByteOffset()`.

Native binary load failures do not throw during module import. The first API call throws `OxMf2InitializationError` if no native artifact is available.

Phase 2 does not provide an `@intlify/ox-mf2` wrapper package, automatic WASM fallback, formatter API, linter API, or semantic snapshot exposure.
