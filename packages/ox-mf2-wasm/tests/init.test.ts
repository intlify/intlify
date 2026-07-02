import { beforeAll, expect, test } from 'vite-plus/test'
import {
  OxMf2InitializationError,
  init,
  parseMessage,
  utf16OffsetToUtf8ByteOffset,
  utf8ByteOffsetToUtf16Offset
} from '../src/index.ts'
import { ensureWasmArtifacts } from './ensure-wasm-artifact.ts'

beforeAll(async () => {
  await ensureWasmArtifacts()
})

test('module import succeeds before WASM init', () => {
  expect(typeof init).toBe('function')
})

test('module re-exports source offset helpers', () => {
  expect(utf16OffsetToUtf8ByteOffset('aあ', 2)).toBe(4)
  expect(utf8ByteOffsetToUtf16Offset('aあ', 4)).toBe(2)
})

test('API before init reports initialization error', () => {
  expect(() => parseMessage('Hello {$name}')).toThrow(OxMf2InitializationError)
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
