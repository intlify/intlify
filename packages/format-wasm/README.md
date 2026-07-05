# @intlify/format-wasm

Browser-first WASM formatter package for MessageFormat 2 messages.

```ts
import { formatMessage, init } from '@intlify/format-wasm'

await init()

const result = formatMessage('.input   {$count   :number}\n{{Value {$count}}}')
```

## API

Call `init(input?)` before using the synchronous formatter APIs. The optional input matches the `@intlify/ox-mf2-wasm` initialization input shape and may be a WASM module, URL, request info, response, or bytes.

- `formatMessage(source, options?)`
- `checkFormat(source, options?)`
- `formatSnapshot(snapshot, source, options?)`
- `checkSnapshot(snapshot, source, options?)`

The advanced snapshot APIs accept serialized Binary AST snapshot bytes plus the complete source text that produced the snapshot. This package does not depend on the parser WASM package at runtime.

## Limitations

Phase 3B formats direct `.mf2` message text only. Resource/catalog file adapters, line wrapping, formatter ignore directives, range formatting, and `.editorconfig` loading are not implemented.

Node.js tooling should prefer `@intlify/format-napi`; this WASM package is intended for browser, worker, playground, and other non-native runtime use cases.
