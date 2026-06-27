import { expect, test } from 'vite-plus/test'
import {
  OxMf2InitializationError,
  parseMessage,
  syntaxKindName,
  utf16OffsetToUtf8ByteOffset,
  utf8ByteOffsetToUtf16Offset
} from '../src/index.ts'

test('module import succeeds without a native binary', () => {
  expect(syntaxKindName(1)).toBe('Root')
})

test('module re-exports source offset helpers', () => {
  expect(utf16OffsetToUtf8ByteOffset('aあ', 2)).toBe(4)
  expect(utf8ByteOffsetToUtf16Offset('aあ', 4)).toBe(2)
})

test('first API call reports native binding unavailability', () => {
  process.env.OX_MF2_NAPI_FORCE_MISSING = '1'
  try {
    expect(() => parseMessage('Hello {$name}')).toThrow(OxMf2InitializationError)
  } finally {
    delete process.env.OX_MF2_NAPI_FORCE_MISSING
  }
})
