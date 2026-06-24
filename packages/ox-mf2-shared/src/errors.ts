/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import type { SectionKindValue } from './constants.ts'
import type { OxMf2ErrorCodeValue } from './error-codes.ts'

type SectionKind = SectionKindValue
type OxMf2ErrorCode = OxMf2ErrorCodeValue

/** Serializable shape shared by ox-mf2 binding errors. */
export type OxMf2ErrorShape = {
  /** Stable numeric ox-mf2 API error code. */
  readonly code: OxMf2ErrorCode
  /** Human-readable error message. */
  readonly message: string
  /** Optional underlying cause retained on Error instances. */
  readonly cause?: unknown
  /** Optional snapshot section kind associated with the error. */
  readonly sectionKind?: SectionKind
  /** Optional byte offset associated with the error. */
  readonly offset?: number
  /** Optional record index associated with the error. */
  readonly recordIndex?: number
}

abstract class OxMf2BaseError extends Error {
  readonly code: OxMf2ErrorCode
  readonly sectionKind?: SectionKind
  readonly offset?: number
  readonly recordIndex?: number
  readonly cause?: unknown

  constructor(shape: OxMf2ErrorShape) {
    super(shape.message)
    this.name = new.target.name
    this.code = shape.code
    this.sectionKind = shape.sectionKind
    this.offset = shape.offset
    this.recordIndex = shape.recordIndex
    if ('cause' in shape) {
      Object.defineProperty(this, 'cause', {
        configurable: true,
        enumerable: false,
        value: shape.cause,
        writable: true
      })
    }
    Object.setPrototypeOf(this, new.target.prototype)
  }

  toJSON(): OxMf2ErrorShape {
    return {
      code: this.code,
      message: this.message,
      sectionKind: this.sectionKind,
      offset: this.offset,
      recordIndex: this.recordIndex
    }
  }
}

/** Error raised for parse failures surfaced by language bindings. */
export class OxMf2ParseError extends OxMf2BaseError {}

/** Error raised when a Binary AST snapshot cannot be decoded or traversed. */
export class OxMf2SnapshotError extends OxMf2BaseError {}

/** Error raised when source text is unavailable or invalid for a requested span. */
export class OxMf2SourceTextError extends OxMf2BaseError {}

/** Error raised when a binding cannot be initialized or loaded. */
export class OxMf2InitializationError extends OxMf2BaseError {}
