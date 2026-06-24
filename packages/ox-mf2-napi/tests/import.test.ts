import { expect, test } from 'vite-plus/test'
import { OxMf2InitializationError, parseMessage, syntaxKindName } from '../src/index.ts'

test('module import succeeds without a native binary', () => {
  expect(syntaxKindName(1)).toBe('Root')
})

test('first API call reports native binding unavailability', () => {
  process.env.OX_MF2_NAPI_FORCE_MISSING = '1'
  try {
    expect(() => parseMessage('Hello {$name}')).toThrow(OxMf2InitializationError)
  } finally {
    delete process.env.OX_MF2_NAPI_FORCE_MISSING
  }
})
