# Release Runbook

## Formatter Packages

The formatter release flow publishes generated N-API native packages before the wrapper package, then publishes the WASM package:

1. `@intlify/format-napi-*` platform packages
2. `@intlify/format-napi`
3. `@intlify/format-wasm`

New `@intlify/format-*` npm packages may need token-based bootstrap publishing for the first release because npm Trusted Publishing can only be configured after the packages exist. After bootstrap, configure the `npm-release` trusted publisher for this repository workflow and use the normal trusted publishing release path.

Published release smoke tests install `@intlify/format-napi` and `@intlify/format-wasm` for the release tag and verify `formatMessage` / `checkFormat` behavior.
