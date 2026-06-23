import { expect, test } from 'vite-plus/test'
import {
  normalizeParseBatchInput,
  normalizeParseMessageInput,
  validateDecodeSnapshotInput,
  validateParseBatchOptions,
  validateParseMessageOptions,
  validateWithSourcesInput
} from '../src/index.ts'

test('parseMessage options normalize defaults', () => {
  expect(validateParseMessageOptions()).toEqual({
    collectTrivia: true,
    includeTrivia: true,
    includeDiagnostics: true,
    includeSourceText: false
  })
})

test('parseMessage rejects batch-only and unknown options', () => {
  expect(() => validateParseMessageOptions({ batchExecution: 'parallel' } as never)).toThrow(
    TypeError
  )
  expect(() => validateParseMessageOptions({ typo: true } as never)).toThrow(TypeError)
})

test('parse options reject impossible trivia combination', () => {
  expect(() =>
    validateParseMessageOptions({
      collectTrivia: false,
      includeTrivia: true
    })
  ).toThrow(TypeError)
})

test('parseBatch options accept execution mode', () => {
  expect(validateParseBatchOptions({ batchExecution: 'parallel' }).batchExecution).toBe('parallel')
})

test('parse input object normalizes metadata', () => {
  expect(
    normalizeParseMessageInput({
      source: 'Hello {$name}',
      locale: 'en',
      messageId: 'hello',
      baseOffset: 4
    })
  ).toEqual({
    source: 'Hello {$name}',
    path: null,
    locale: 'en',
    messageId: 'hello',
    baseOffset: 4
  })
})

test('parse input rejects invalid baseOffset and unpaired surrogates', () => {
  expect(() => normalizeParseMessageInput({ source: 'x', baseOffset: 0x1_0000_0000 })).toThrow(
    RangeError
  )
  expect(() => normalizeParseMessageInput('\ud800')).toThrow(TypeError)
})

test('parseBatch input accepts objects only and must not be empty', () => {
  expect(() => normalizeParseBatchInput([])).toThrow(RangeError)
  expect(() => normalizeParseBatchInput(['Hello'] as never)).toThrow(TypeError)
  expect(normalizeParseBatchInput([{ source: 'Hello' }])).toHaveLength(1)
})

test('decodeSnapshot accepts only Uint8Array', () => {
  const bytes = new Uint8Array([1, 2, 3])
  expect(validateDecodeSnapshotInput(bytes)).toBe(bytes)
  expect(() => validateDecodeSnapshotInput(new ArrayBuffer(1) as never)).toThrow(TypeError)
})

test('withSources validates source strings', () => {
  expect(validateWithSourcesInput(['a', 'b'])).toEqual(['a', 'b'])
  expect(() => validateWithSourcesInput([1] as never)).toThrow(TypeError)
  expect(() => validateWithSourcesInput(['\ud800'])).toThrow(TypeError)
})
