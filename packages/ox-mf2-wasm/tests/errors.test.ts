import { expect, test } from 'vite-plus/test'
import { OxMf2ErrorCode, OxMf2InitializationError } from '../src/index.ts'

test('initialization error shape is exported from the WASM package', () => {
  const error = new OxMf2InitializationError({
    code: OxMf2ErrorCode.InitializationWasmNotInitialized,
    message: 'wasm not initialized'
  })
  expect(error.name).toBe('OxMf2InitializationError')
  expect(error.code).toBe(OxMf2ErrorCode.InitializationWasmNotInitialized)
})
