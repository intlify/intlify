import { expect } from 'vite-plus/test'

import type { FormatCheckResult, FormatFailure, FormatResult } from '../src/index.ts'

/**
 * Assert that a formatter result is a failed result and return the narrowed value.
 *
 * @param result - Formatter result under test.
 * @returns The same result narrowed to a failed formatter result.
 */
export function expectFormatFailure(result: FormatResult | FormatCheckResult): FormatFailure {
  expect(result.ok).toBe(false)
  return result as FormatFailure
}
