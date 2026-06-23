import { expect, test } from 'vite-plus/test'
import { OxMf2ErrorCode, oxMf2ErrorCodeName } from '../src/error-codes.ts'

test('oxMf2ErrorCodeName resolves every exported constant', () => {
  for (const [name, code] of Object.entries(OxMf2ErrorCode)) {
    expect(oxMf2ErrorCodeName(code)).toBe(name)
  }
})

test('oxMf2ErrorCodeName returns unknown for unrecognized codes', () => {
  expect(oxMf2ErrorCodeName(999)).toBe('unknown')
})
