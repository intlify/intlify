import { expect, test } from 'vite-plus/test'
import { checkFormat, checkSnapshot, formatMessage, formatSnapshot } from '../src/index.ts'
import { expectFormatFailure } from './helpers.ts'

test('null options return invalid_options', () => {
  const result = formatMessage('Hello', null as never)
  const failure = expectFormatFailure(result)

  expect(failure.errors[0]?.code).toBe('invalid_options')
  expect(failure.errors[0]?.details?.pointer).toBe('/options')
})

test('unknown option fields return invalid_options', () => {
  const result = checkFormat('Hello', { lineWidth: 100 } as never)
  const failure = expectFormatFailure(result)

  expect(failure.errors[0]?.code).toBe('invalid_options')
  expect(failure.errors[0]?.details?.pointer).toBe('/lineWidth')
  expect(failure.errors[0]?.details?.reason).toBe('unknown_field')
})

test('invalid mode returns invalid_options', () => {
  const result = formatMessage('Hello', { mode: 'compact' } as never)
  const failure = expectFormatFailure(result)

  expect(failure.errors[0]?.code).toBe('invalid_options')
  expect(failure.errors[0]?.details?.pointer).toBe('/mode')
  expect(failure.errors[0]?.details?.allowedValues).toEqual(['standard', 'preserve'])
})

test('invalid source type returns invalid_options before WASM init', () => {
  const result = checkFormat({ source: 'Hello' } as never)
  const failure = expectFormatFailure(result)

  expect(failure.errors[0]?.code).toBe('invalid_options')
  expect(failure.errors[0]?.details?.pointer).toBe('/source')
})

test('invalid snapshot type returns invalid_options before WASM init', () => {
  const result = formatSnapshot('not bytes' as never, 'Hello')
  const failure = expectFormatFailure(result)

  expect(failure.errors[0]?.code).toBe('invalid_options')
  expect(failure.errors[0]?.details?.pointer).toBe('/snapshot')
})

test('invalid snapshot check input returns invalid_options before WASM init', () => {
  const result = checkSnapshot(new Uint8Array(), 42 as never)
  const failure = expectFormatFailure(result)

  expect(failure.errors[0]?.code).toBe('invalid_options')
  expect(failure.errors[0]?.details?.pointer).toBe('/source')
})
