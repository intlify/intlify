/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

export {
  CORE_SECTION_KINDS,
  DiagnosticCode,
  DiagnosticSeverity,
  NONE_U32,
  SectionKind,
  SyntaxKind,
  diagnosticCodeName,
  diagnosticSeverityName,
  isCoreSectionKind,
  sectionKindName,
  syntaxKindName
} from './constants.ts'
export { OxMf2ErrorCode, oxMf2ErrorCodeName } from './error-codes.ts'
export {
  OxMf2InitializationError,
  OxMf2ParseError,
  OxMf2SnapshotError,
  OxMf2SourceTextError
} from './errors.ts'
export {
  normalizeDiagnostic,
  normalizeNode,
  normalizeResult,
  normalizeRoot,
  normalizeSections,
  normalizeSnapshotCounts,
  normalizeSource,
  normalizeSpan,
  normalizeToken,
  normalizeTrivia
} from './normalize.ts'
export {
  normalizeParseBatchInput,
  normalizeParseMessageInput,
  validateDecodeSnapshotInput,
  validateParseBatchOptions,
  validateParseMessageOptions,
  validateWithSourcesInput
} from './options.ts'
export {
  assertNoUnpairedSurrogates,
  assertUtf8Span,
  decodeUtf8Slice,
  encodeUtf8Source,
  hasUnpairedSurrogate,
  isUtf8Boundary,
  utf8ByteLength
} from './source-text.ts'
export {
  BinarySnapshotAccessor,
  createDecodedSnapshotResult,
  createDecodedSnapshotResultFromAccessor,
  createParseBatchResult,
  createParseMessageResult
} from './snapshot.ts'

export type {
  DiagnosticCodeValue,
  DiagnosticSeverityValue,
  SectionKindValue,
  SyntaxKindValue
} from './constants.ts'
export type { OxMf2ErrorCodeValue } from './error-codes.ts'
export type {
  BatchExecution,
  ChildHandle,
  DecodedSnapshotResult,
  DiagnosticLabelView,
  DiagnosticView,
  NormalizedParseBatchOptions,
  NormalizedParseInputObject,
  NormalizedParseMessageOptions,
  NodeHandle,
  ParseBatchOptions,
  ParseBatchResult,
  ParseInputObject,
  ParseMessageOptions,
  ParseMessageResult,
  RootHandle,
  SectionMetadata,
  SnapshotAccessor,
  SourceLocation,
  SourceView,
  Span,
  TokenHandle,
  TriviaHandle,
  U32,
  WasmInitInput
} from './types.ts'
export type { OxMf2ErrorShape } from './errors.ts'
export type { NormalizedDiagnostic, NormalizedResult } from './normalize.ts'
