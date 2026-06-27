import { expect, test } from 'vite-plus/test'
import {
  assertNoUnpairedSurrogates,
  decodeUtf8Slice,
  encodeUtf8Source,
  hasUnpairedSurrogate,
  isUtf8Boundary,
  utf16OffsetToUtf8ByteOffset,
  utf8ByteOffsetToUtf16Offset,
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

test('offset helpers convert between UTF-8 bytes and UTF-16 code units', () => {
  const source = 'aあ😀b'

  expect(utf16OffsetToUtf8ByteOffset(source, 0)).toBe(0)
  expect(utf16OffsetToUtf8ByteOffset(source, 1)).toBe(1)
  expect(utf16OffsetToUtf8ByteOffset(source, 2)).toBe(4)
  expect(utf16OffsetToUtf8ByteOffset(source, 4)).toBe(8)
  expect(utf16OffsetToUtf8ByteOffset(source, 5)).toBe(9)

  expect(utf8ByteOffsetToUtf16Offset(source, 0)).toBe(0)
  expect(utf8ByteOffsetToUtf16Offset(source, 1)).toBe(1)
  expect(utf8ByteOffsetToUtf16Offset(source, 4)).toBe(2)
  expect(utf8ByteOffsetToUtf16Offset(source, 8)).toBe(4)
  expect(utf8ByteOffsetToUtf16Offset(source, 9)).toBe(5)
})

test('offset helpers floor offsets inside a scalar value', () => {
  const source = 'aあ😀b'

  expect(utf8ByteOffsetToUtf16Offset(source, 2)).toBe(1)
  expect(utf8ByteOffsetToUtf16Offset(source, 6)).toBe(2)
  expect(utf16OffsetToUtf8ByteOffset(source, 3)).toBe(4)
})

test('offset helpers validate bounds and source text', () => {
  expect(() => utf16OffsetToUtf8ByteOffset('abc', -1)).toThrow(RangeError)
  expect(() => utf16OffsetToUtf8ByteOffset('abc', 1.5)).toThrow(RangeError)
  expect(() => utf16OffsetToUtf8ByteOffset('abc', 4)).toThrow(RangeError)
  expect(() => utf16OffsetToUtf8ByteOffset('\ud800', 0)).toThrow(TypeError)
  expect(() => utf8ByteOffsetToUtf16Offset('abc', -1)).toThrow(RangeError)
  expect(() => utf8ByteOffsetToUtf16Offset('abc', 1.5)).toThrow(RangeError)
  expect(() => utf8ByteOffsetToUtf16Offset('abc', 4)).toThrow(RangeError)
  expect(() => utf8ByteOffsetToUtf16Offset('\ud800', 0)).toThrow(TypeError)
})
