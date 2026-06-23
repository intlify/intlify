import { expect, test } from 'vite-plus/test'
import {
  assertNoUnpairedSurrogates,
  decodeUtf8Slice,
  encodeUtf8Source,
  hasUnpairedSurrogate,
  isUtf8Boundary,
  utf8ByteLength
} from '../src/index.ts'

test('source text rejects unpaired surrogates', () => {
  expect(hasUnpairedSurrogate('abc')).toBe(false)
  expect(hasUnpairedSurrogate('😀')).toBe(false)
  expect(hasUnpairedSurrogate('\ud800')).toBe(true)
  expect(() => assertNoUnpairedSurrogates('\ud800')).toThrow(TypeError)
})

test('UTF-8 byte helpers use byte offsets', () => {
  const bytes = encodeUtf8Source('a😀b')
  expect(utf8ByteLength('a😀b')).toBe(6)
  expect(isUtf8Boundary(bytes, 1)).toBe(true)
  expect(isUtf8Boundary(bytes, 2)).toBe(false)
  expect(decodeUtf8Slice(bytes, { start: 1, end: 5 })).toBe('😀')
  expect(() => decodeUtf8Slice(bytes, { start: 2, end: 5 })).toThrow(RangeError)
})
