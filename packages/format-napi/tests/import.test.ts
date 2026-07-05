import { expect, test } from 'vite-plus/test'
import { OxMf2InitializationError, formatMessage } from '../src/index.ts'
import { expectFormatFailure } from './helpers.ts'

test('module import succeeds without loading a native binary', () => {
  const result = formatMessage(42 as never)
  const failure = expectFormatFailure(result)

  expect(failure.errors[0]?.code).toBe('invalid_options')
})

test('first API call reports native binding unavailability', () => {
  process.env.INTLIFY_FORMAT_NAPI_FORCE_MISSING = '1'
  try {
    expect(() => formatMessage('Hello {$name}')).toThrow(OxMf2InitializationError)
  } finally {
    delete process.env.INTLIFY_FORMAT_NAPI_FORCE_MISSING
  }
})
