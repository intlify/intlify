import { existsSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { beforeAll, expect, test } from 'vite-plus/test'
import { loadNativeBinding } from '../src/loader.ts'
import {
  decodeSnapshot as decodeNapiSnapshot,
  normalizeResult,
  normalizeRoot,
  parseBatch as parseNapiBatch,
  parseMessage as parseNapiMessage
} from '../src/index.ts'
import {
  decodeSnapshot as decodeWasmSnapshot,
  init as initWasm,
  parseBatch as parseWasmBatch,
  parseMessage as parseWasmMessage
} from '../../ox-mf2-wasm/src/index.ts'

const hasNativeBinding = loadNativeBinding().binding !== null
const hasWasmArtifact = existsSync(
  fileURLToPath(new URL('../../ox-mf2-wasm/dist/ox_mf2_wasm.js', import.meta.url))
)
const canRunParity = hasNativeBinding && hasWasmArtifact

beforeAll(async () => {
  if (canRunParity) {
    await initWasm()
  }
})

test.runIf(canRunParity)('parseMessage parity between N-API and WASM', () => {
  const input = {
    source: 'Hello {$name}',
    locale: 'en',
    messageId: 'hello'
  }
  const options = { includeSourceText: true }
  const napi = parseNapiMessage(input, options)
  const wasm = parseWasmMessage(input, options)

  expect(normalizeResult(wasm)).toEqual(normalizeResult(napi))
  expect(normalizeRoot(wasm.root)).toEqual(normalizeRoot(napi.root))
})

test.runIf(canRunParity)('parseBatch parity between N-API and WASM', () => {
  const input = [
    { source: 'One {$name}', messageId: 'one' },
    { source: 'Two {$name}', messageId: 'two' }
  ]
  const napi = parseNapiBatch(input)
  const wasm = parseWasmBatch(input)

  expect(normalizeResult(wasm)).toEqual(normalizeResult(napi))
})

test.runIf(canRunParity)('decodeSnapshot parity between N-API and WASM', () => {
  const parsed = parseNapiMessage('Hello {$name}')
  const bytes = parsed.snapshot.toBytes()
  const napi = decodeNapiSnapshot(bytes).withSources(['Hello {$name}'])
  const wasm = decodeWasmSnapshot(bytes).withSources(['Hello {$name}'])

  expect(normalizeResult(wasm)).toEqual(normalizeResult(napi))
  expect(normalizeRoot(wasm.roots[0]!)).toEqual(normalizeRoot(napi.roots[0]!))
})
