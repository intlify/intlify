import { beforeAll, expect, test } from 'vite-plus/test'
import { OxMf2InitializationError, formatMessage, init } from '../src/index.ts'
import { ensureWasmArtifacts } from './ensure-wasm-artifact.ts'

beforeAll(async () => {
  await ensureWasmArtifacts()
})

test('module import succeeds before WASM init', () => {
  expect(typeof init).toBe('function')
})

test('API before init reports initialization error', () => {
  expect(() => formatMessage('Hello {$name}')).toThrow(OxMf2InitializationError)
})

test('explicit init input is rejected while default init is in flight', async () => {
  const initialization = init()
  await expect(init(new Uint8Array())).rejects.toBeInstanceOf(OxMf2InitializationError)
  await expect(initialization).resolves.toBeUndefined()
})

test('init initializes the generated WASM artifact', async () => {
  await expect(init()).resolves.toBeUndefined()
  await expect(init()).resolves.toBeUndefined()
  await expect(init(new Uint8Array())).rejects.toBeInstanceOf(OxMf2InitializationError)
})
