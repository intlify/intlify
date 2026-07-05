# @intlify/format-napi

Native Node.js formatter APIs for ox-mf2.

```js
import { formatMessage } from '@intlify/format-napi'

const result = formatMessage('.input   {$count   :number}\n{{Value {$count}}}')
if (result.ok) {
  console.log(result.code)
}
```

The package loads its platform native binding lazily. Importing the package does not require the native binary to be available; the first formatter API call reports an initialization error if the binary cannot be loaded.

The public APIs are:

- `formatMessage(source, options?)`
- `checkFormat(source, options?)`
- `formatSnapshot(snapshot, source, options?)`
- `checkSnapshot(snapshot, source, options?)`

`options.mode` accepts `"standard"` and `"preserve"`. Invalid options and invalid argument types are returned as `{ ok: false }` formatter results with an `invalid_options` operational error.

## Native Packages

Release automation generates platform packages from this wrapper package:

- `@intlify/format-napi-darwin-arm64`
- `@intlify/format-napi-darwin-x64`
- `@intlify/format-napi-linux-x64-gnu`
- `@intlify/format-napi-linux-x64-musl`
- `@intlify/format-napi-linux-arm64-gnu`
- `@intlify/format-napi-win32-x64-msvc`

Those packages contain only the native `.node` binary and generated N-API glue for their platform. They are published before this wrapper package.

## Limitations

Phase 3B formats direct `.mf2` message text only. Resource/catalog file adapters, line wrapping, formatter ignore directives, range formatting, and `.editorconfig` loading are not implemented.
