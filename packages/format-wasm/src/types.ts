/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import type { DiagnosticView } from '@intlify/ox-mf2-shared'

/** Formatter mode accepted by programmatic formatter APIs. */
export type FormatMode = 'standard' | 'preserve'

/** Formatter options accepted by WASM formatter APIs. */
export type FormatOptions = {
  /** Formatting strategy. Defaults to `"standard"`. */
  readonly mode?: FormatMode
}

/** Formatter operational error returned in failed formatter results. */
export type FormatterOperationalError = {
  /** Broad formatter error class. */
  readonly kind: string
  /** Stable formatter error code. */
  readonly code: string
  /** Human-readable error message. */
  readonly message: string
  /** Optional file path or external input identifier. */
  readonly path?: string
  /** Stable details for machine-readable consumers. */
  readonly details?: Record<string, unknown>
}

/** Result returned by APIs that produce formatted text. */
export type FormatResult =
  | {
      readonly ok: true
      readonly code: string
      readonly changed: boolean
    }
  | FormatFailure

/** Result returned by APIs that only report whether formatting would change. */
export type FormatCheckResult =
  | {
      readonly ok: true
      readonly changed: boolean
    }
  | FormatFailure

/** Failed formatter result shared by format and check APIs. */
export type FormatFailure = {
  /** Discriminant for failed formatter results. */
  readonly ok: false
  /** Parser diagnostics that prevented formatting. */
  readonly diagnostics: DiagnosticView[]
  /** Operational errors that prevented formatting. */
  readonly errors: FormatterOperationalError[]
}

export type { DiagnosticView } from '@intlify/ox-mf2-shared'
