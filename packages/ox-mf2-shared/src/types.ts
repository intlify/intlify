/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import type {
  DiagnosticCodeValue,
  DiagnosticSeverityValue,
  SectionKindValue,
  SyntaxKindValue
} from './constants.ts'

/** Unsigned 32-bit integer value used by the snapshot wire format. */
export type U32 = number

type SyntaxKind = SyntaxKindValue
type DiagnosticSeverity = DiagnosticSeverityValue
type DiagnosticCode = DiagnosticCodeValue
type SectionKind = SectionKindValue

/** Batch parser execution mode reported by language bindings. */
export type BatchExecution = 'sequential' | 'parallel'

/** Half-open byte span in UTF-8 source text. */
export type Span = {
  /** Inclusive UTF-8 byte start offset. */
  readonly start: U32
  /** Exclusive UTF-8 byte end offset. */
  readonly end: U32
}

/** Human-readable source location derived from UTF-8 byte offsets. */
export type SourceLocation = {
  /** One-based line number. */
  readonly line: U32
  /** Zero-based UTF-8 byte column within the line. */
  readonly column: U32
}

/** Input object accepted by parse APIs when source metadata is available. */
export type ParseInputObject = {
  /** MessageFormat source text to parse. */
  readonly source: string
  /** Optional source path used for diagnostics and tooling metadata. */
  readonly path?: string
  /** Optional locale identifier associated with this source. */
  readonly locale?: string
  /** Optional message identifier associated with this source. */
  readonly messageId?: string
  /** Optional base byte offset used when this source is embedded in a larger file. */
  readonly baseOffset?: U32
}

/** Parse input object after binding-side validation and defaulting. */
export type NormalizedParseInputObject = {
  /** Validated MessageFormat source text. */
  readonly source: string
  /** Source path, or null when it was not provided. */
  readonly path: string | null
  /** Locale identifier, or null when it was not provided. */
  readonly locale: string | null
  /** Message identifier, or null when it was not provided. */
  readonly messageId: string | null
  /** Base UTF-8 byte offset for embedded source text. */
  readonly baseOffset: U32
}

/** Options shared by single-message parse APIs. */
export type ParseMessageOptions = {
  /** Whether the parser should collect trivia while parsing. */
  readonly collectTrivia?: boolean
  /** Whether collected trivia should be written into the snapshot. */
  readonly includeTrivia?: boolean
  /** Whether diagnostics should be written into the snapshot. */
  readonly includeDiagnostics?: boolean
  /** Whether source text bytes should be embedded in the snapshot. */
  readonly includeSourceText?: boolean
}

/** Single-message parse options after validation and defaulting. */
export type NormalizedParseMessageOptions = {
  /** Validated trivia collection flag. */
  readonly collectTrivia: boolean
  /** Validated snapshot trivia inclusion flag. */
  readonly includeTrivia: boolean
  /** Validated diagnostic inclusion flag. */
  readonly includeDiagnostics: boolean
  /** Validated source text embedding flag. */
  readonly includeSourceText: boolean
}

/** Options accepted by batch parse APIs. */
export type ParseBatchOptions = ParseMessageOptions & {
  /** Requested batch execution mode. */
  readonly batchExecution?: BatchExecution
}

/** Batch parse options after validation and defaulting. */
export type NormalizedParseBatchOptions = NormalizedParseMessageOptions & {
  /** Effective batch execution mode. */
  readonly batchExecution: BatchExecution
}

/** Metadata for a section in a decoded Binary AST snapshot. */
export type SectionMetadata = {
  /** Numeric section kind from the snapshot section table. */
  readonly kind: SectionKind
  /** Byte offset of the section payload from the beginning of the snapshot. */
  readonly offset: U32
  /** Byte length of the section payload. */
  readonly byteLength: U32
  /** Fixed record size for record-based sections, or zero for byte payload sections. */
  readonly recordSize: number
  /** Number of records in the section. */
  readonly count: U32
  /** Whether the section is marked as required in the section table. */
  readonly required: boolean
}

/** Lazy accessor over a decoded Binary AST snapshot. */
export interface SnapshotAccessor {
  /**
   * Return a root handle by id.
   *
   * @param id - Zero-based root id.
   * @returns Root handle for the requested id.
   */
  root(id: number): RootHandle

  /**
   * Return a CST node handle by id.
   *
   * @param id - Zero-based node id.
   * @returns Node handle for the requested id.
   */
  node(id: number): NodeHandle

  /**
   * Return a token handle by id.
   *
   * @param id - Zero-based token id.
   * @returns Token handle for the requested id.
   */
  token(id: number): TokenHandle

  /**
   * Return a trivia handle by id.
   *
   * @param id - Zero-based trivia id.
   * @returns Trivia handle for the requested id.
   */
  trivia(id: number): TriviaHandle

  /**
   * Return a source metadata view by id.
   *
   * @param id - Zero-based source id.
   * @returns Source view for the requested id.
   */
  source(id: number): SourceView

  /**
   * Return a diagnostic view by index.
   *
   * @param index - Zero-based diagnostic index.
   * @returns Diagnostic view for the requested index.
   */
  diagnostic(index: number): DiagnosticView

  /**
   * Return the number of root records.
   *
   * @returns Root record count.
   */
  rootCount(): number

  /**
   * Return the number of node records.
   *
   * @returns Node record count.
   */
  nodeCount(): number

  /**
   * Return the number of token records.
   *
   * @returns Token record count.
   */
  tokenCount(): number

  /**
   * Return the number of trivia records.
   *
   * @returns Trivia record count.
   */
  triviaCount(): number

  /**
   * Return the number of source records.
   *
   * @returns Source record count.
   */
  sourceCount(): number

  /**
   * Return the number of diagnostic records.
   *
   * @returns Diagnostic record count.
   */
  diagnosticCount(): number

  /**
   * Return metadata for a snapshot section.
   *
   * @param kind - Section kind to inspect.
   * @returns Section metadata, or null when the section is absent.
   */
  section(kind: SectionKind): SectionMetadata | null

  /**
   * Copy the underlying snapshot bytes.
   *
   * @returns New byte array containing the complete snapshot.
   */
  toBytes(): Uint8Array
}

/** Handle for one snapshot root record. */
export interface RootHandle {
  /** Zero-based root id. */
  readonly id: U32

  /**
   * Return the root node.
   *
   * @returns Node handle referenced by this root.
   */
  node(): NodeHandle

  /**
   * Return diagnostics attached to this root.
   *
   * @returns Diagnostic views in snapshot order.
   */
  diagnostics(): DiagnosticView[]
}

/** Child handle returned from node edge traversal. */
export type ChildHandle = NodeHandle | TokenHandle

/** Handle for one CST node record. */
export interface NodeHandle {
  /** Zero-based node id. */
  readonly id: U32

  /**
   * Return the syntax kind of this node.
   *
   * @returns Numeric syntax kind.
   */
  kind(): SyntaxKind

  /**
   * Return the source span of this node.
   *
   * @returns Half-open UTF-8 byte span.
   */
  span(): Span

  /**
   * Return the number of child edges.
   *
   * @returns Child edge count.
   */
  childCount(): number

  /**
   * Return one child handle by edge index.
   *
   * @param index - Zero-based child edge index.
   * @returns Child node or token handle.
   */
  childAt(index: number): ChildHandle

  /**
   * Materialize all child handles.
   *
   * @returns Child handles in edge order.
   */
  children(): ChildHandle[]
}

/** Handle for one token record. */
export interface TokenHandle {
  /** Zero-based token id. */
  readonly id: U32

  /**
   * Return the syntax kind of this token.
   *
   * @returns Numeric syntax kind.
   */
  kind(): SyntaxKind

  /**
   * Return the source span of this token.
   *
   * @returns Half-open UTF-8 byte span.
   */
  span(): Span

  /**
   * Return trivia that appears before this token.
   *
   * @returns Leading trivia handles in source order.
   */
  leadingTrivia(): TriviaHandle[]

  /**
   * Return trivia that appears after this token.
   *
   * @returns Trailing trivia handles in source order.
   */
  trailingTrivia(): TriviaHandle[]
}

/** Handle for one trivia record. */
export interface TriviaHandle {
  /** Zero-based trivia id. */
  readonly id: U32

  /**
   * Return the syntax kind of this trivia item.
   *
   * @returns Numeric syntax kind.
   */
  kind(): SyntaxKind

  /**
   * Return the source span of this trivia item.
   *
   * @returns Half-open UTF-8 byte span.
   */
  span(): Span
}

/** View over one source record and its optional source text. */
export interface SourceView {
  /** Zero-based source id. */
  readonly id: U32

  /**
   * Return the source path.
   *
   * @returns Source path, or null when absent.
   */
  path(): string | null

  /**
   * Return the source locale.
   *
   * @returns Locale identifier, or null when absent.
   */
  locale(): string | null

  /**
   * Return the source message id.
   *
   * @returns Message id, or null when absent.
   */
  messageId(): string | null

  /**
   * Return the base offset for embedded source text.
   *
   * @returns Base UTF-8 byte offset.
   */
  baseOffset(): U32

  /**
   * Decode a UTF-8 source slice.
   *
   * @param span - Half-open UTF-8 byte span to decode.
   * @returns Decoded source text slice.
   */
  sourceSlice(span: Span): string
}

/** Diagnostic label attached to a diagnostic. */
export type DiagnosticLabelView = {
  /** Source span covered by the label. */
  readonly span: Span
  /** Optional label message. */
  readonly message: string | null
}

/** Diagnostic view decoded from snapshot records. */
export type DiagnosticView = {
  /** Root id that owns this diagnostic. */
  readonly rootId: U32
  /** Source id that owns this diagnostic span. */
  readonly sourceId: U32
  /** Diagnostic severity enum value. */
  readonly severity: DiagnosticSeverity
  /** Diagnostic code enum value. */
  readonly code: DiagnosticCode
  /** Human-readable diagnostic message. */
  readonly message: string | null
  /** Primary source span for the diagnostic. */
  readonly span: Span
  /** Optional human-readable source location. */
  readonly location: SourceLocation | null
  /** Additional labels associated with the diagnostic. */
  readonly labels: DiagnosticLabelView[]
}

/** Result returned by single-message parse APIs. */
export type ParseMessageResult = {
  /** Snapshot accessor for lazy traversal. */
  readonly snapshot: SnapshotAccessor
  /** Roots decoded from the snapshot. */
  readonly roots: RootHandle[]
  /** Primary root for the parsed message. */
  readonly root: RootHandle
  /** Sources decoded from the snapshot. */
  readonly sources: SourceView[]
  /** Primary source for the parsed message. */
  readonly source: SourceView
  /** Diagnostics decoded from the snapshot. */
  readonly diagnostics: DiagnosticView[]
}

/** Result returned by batch parse APIs. */
export type ParseBatchResult = {
  /** Snapshot accessor for lazy traversal. */
  readonly snapshot: SnapshotAccessor
  /** Roots decoded from the snapshot. */
  readonly roots: RootHandle[]
  /** Sources decoded from the snapshot. */
  readonly sources: SourceView[]
  /** Diagnostics decoded from the snapshot. */
  readonly diagnostics: DiagnosticView[]
  /** Effective batch execution mode reported by the binding. */
  readonly execution: BatchExecution
  /** Whether the binding degraded from the requested execution mode. */
  readonly degraded: boolean
}

/** Result returned by snapshot decode APIs. */
export type DecodedSnapshotResult = {
  /** Snapshot accessor for lazy traversal. */
  readonly snapshot: SnapshotAccessor
  /** Roots decoded from the snapshot. */
  readonly roots: RootHandle[]
  /** Sources decoded from the snapshot. */
  readonly sources: SourceView[]
  /** Diagnostics decoded from the snapshot. */
  readonly diagnostics: DiagnosticView[]

  /**
   * Attach external source texts to a snapshot that does not embed them.
   *
   * @param sources - Source texts ordered by source id.
   * @returns New decoded snapshot result backed by the same snapshot bytes.
   */
  withSources(sources: string[]): DecodedSnapshotResult
}

/** Input accepted by the WASM package initializer. */
export type WasmInitInput = URL | RequestInfo | WebAssembly.Module | ArrayBuffer | Uint8Array
