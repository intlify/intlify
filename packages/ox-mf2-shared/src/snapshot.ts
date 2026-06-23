/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import {
  DIAGNOSTIC_LABEL_RECORD_SIZE,
  DIAGNOSTIC_RECORD_SIZE,
  EDGE_KIND_TOKEN,
  EDGE_RECORD_SIZE,
  HEADER_SIZE,
  NODE_RECORD_SIZE,
  NONE_U32,
  ROOT_RECORD_SIZE,
  SECTION_ALIGNMENT,
  SECTION_FLAG_REQUIRED,
  SECTION_RECORD_SIZE,
  SOURCE_RECORD_SIZE,
  SNAPSHOT_FEATURE_FLAGS,
  SNAPSHOT_MAGIC,
  SNAPSHOT_MAJOR_VERSION,
  SNAPSHOT_MINOR_VERSION,
  SectionKind,
  TOKEN_RECORD_SIZE,
  TRIVIA_RECORD_SIZE,
  isCoreSectionKind,
  sectionRecordSize
} from './constants.ts'
import { OxMf2ErrorCode } from './error-codes.ts'
import { OxMf2SnapshotError, OxMf2SourceTextError } from './errors.ts'
import { decodeUtf8Slice, encodeUtf8Source, isUtf8Boundary } from './source-text.ts'

import type {
  DiagnosticCodeValue,
  DiagnosticSeverityValue,
  SectionKindValue,
  SyntaxKindValue
} from './constants.ts'
import type { OxMf2ErrorCodeValue } from './error-codes.ts'
import type {
  BatchExecution,
  ChildHandle,
  DecodedSnapshotResult,
  DiagnosticLabelView,
  DiagnosticView,
  NodeHandle,
  ParseBatchResult,
  ParseMessageResult,
  RootHandle,
  SectionMetadata,
  SnapshotAccessor,
  SourceLocation,
  SourceView,
  Span,
  TokenHandle,
  TriviaHandle
} from './types.ts'

type SectionSlice = {
  readonly kind: number
  readonly flags: number
  readonly offset: number
  readonly byteLength: number
  readonly count: number
  readonly recordSize: number
  readonly alignment: number
}

type SnapshotResultInit = {
  readonly bytes: Uint8Array
  readonly externalSources?: readonly string[]
}

const TEXT_DECODER = new TextDecoder('utf-8', { fatal: true })

/**
 * Decode raw snapshot bytes into a decoded snapshot result.
 *
 * @param bytes - Binary AST snapshot bytes.
 * @returns Decoded snapshot result with lazy accessors.
 */
export function createDecodedSnapshotResult(bytes: Uint8Array): DecodedSnapshotResult {
  return createDecodedSnapshotResultFromAccessor(new BinarySnapshotAccessor(bytes.slice()))
}

/**
 * Create a single-message parse result from snapshot bytes.
 *
 * @param init - Snapshot bytes and optional external source texts.
 * @returns Single-message parse result.
 */
export function createParseMessageResult(init: SnapshotResultInit): ParseMessageResult {
  const snapshot = new BinarySnapshotAccessor(init.bytes, init.externalSources)
  const roots = collectRoots(snapshot)
  const sources = collectSources(snapshot)
  const diagnostics = collectDiagnostics(snapshot)
  const root = roots[0]
  const source = sources[0]
  if (!root || !source) {
    throw snapshotError(
      OxMf2ErrorCode.DecodeMissingRequiredSection,
      'snapshot does not contain a root/source pair'
    )
  }
  return { snapshot, roots, root, sources, source, diagnostics }
}

/**
 * Create a batch parse result from snapshot bytes.
 *
 * @param init - Snapshot bytes, optional sources, and batch execution metadata.
 * @returns Batch parse result.
 */
export function createParseBatchResult(
  init: SnapshotResultInit & {
    /** Effective batch execution mode reported by the binding. */
    readonly execution: BatchExecution
    /** Whether requested parallel execution degraded to sequential execution. */
    readonly degraded: boolean
  }
): ParseBatchResult {
  const snapshot = new BinarySnapshotAccessor(init.bytes, init.externalSources)
  return {
    snapshot,
    roots: collectRoots(snapshot),
    sources: collectSources(snapshot),
    diagnostics: collectDiagnostics(snapshot),
    execution: init.execution,
    degraded: init.degraded
  }
}

/**
 * Create a decoded snapshot result from an existing accessor.
 *
 * @param snapshot - Snapshot accessor to wrap.
 * @returns Decoded snapshot result.
 */
export function createDecodedSnapshotResultFromAccessor(
  snapshot: BinarySnapshotAccessor
): DecodedSnapshotResult {
  return {
    snapshot,
    roots: collectRoots(snapshot),
    sources: collectSources(snapshot),
    diagnostics: collectDiagnostics(snapshot),
    withSources(sources: string[]): DecodedSnapshotResult {
      return createDecodedSnapshotResultFromAccessor(snapshot.withSources(sources))
    }
  }
}

/** Lazy decoder and accessor for the ox-mf2 Binary AST snapshot format. */
export class BinarySnapshotAccessor implements SnapshotAccessor {
  readonly #bytes: Uint8Array
  readonly #view: DataView
  readonly #sections: Map<number, SectionSlice>
  readonly #externalSources: readonly Uint8Array[] | null

  /**
   * Create an accessor from snapshot bytes.
   *
   * @param bytes - Binary AST snapshot bytes.
   * @param externalSources - Optional source texts ordered by source id.
   */
  constructor(bytes: Uint8Array, externalSources?: readonly string[]) {
    this.#bytes = bytes
    this.#view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
    this.#sections = decodeSections(bytes, this.#view)
    this.#externalSources =
      externalSources?.map((source, index) => encodeUtf8Source(source, `source ${index}`)) ?? null
  }

  /**
   * Return a root handle by id.
   *
   * @param id - Zero-based root id.
   * @returns Root handle for the requested id.
   */
  root(id: number): RootHandle {
    assertIndex(id, this.rootCount(), 'root')
    return new BinaryRootHandle(this, id)
  }

  /**
   * Return a node handle by id.
   *
   * @param id - Zero-based node id.
   * @returns Node handle for the requested id.
   */
  node(id: number): NodeHandle {
    assertIndex(id, this.nodeCount(), 'node')
    return new BinaryNodeHandle(this, id)
  }

  /**
   * Return a token handle by id.
   *
   * @param id - Zero-based token id.
   * @returns Token handle for the requested id.
   */
  token(id: number): TokenHandle {
    assertIndex(id, this.tokenCount(), 'token')
    return new BinaryTokenHandle(this, id)
  }

  /**
   * Return a trivia handle by id.
   *
   * @param id - Zero-based trivia id.
   * @returns Trivia handle for the requested id.
   */
  trivia(id: number): TriviaHandle {
    assertIndex(id, this.triviaCount(), 'trivia')
    return new BinaryTriviaHandle(this, id)
  }

  /**
   * Return a source view by id.
   *
   * @param id - Zero-based source id.
   * @returns Source view for the requested id.
   */
  source(id: number): SourceView {
    assertIndex(id, this.sourceCount(), 'source')
    return new BinarySourceView(this, id)
  }

  /**
   * Return a diagnostic view by index.
   *
   * @param index - Zero-based diagnostic index.
   * @returns Diagnostic view for the requested index.
   */
  diagnostic(index: number): DiagnosticView {
    assertIndex(index, this.diagnosticCount(), 'diagnostic')
    return this.#diagnosticView(index)
  }

  /**
   * Return the number of root records.
   *
   * @returns Root record count.
   */
  rootCount(): number {
    return this.#requiredSection(SectionKind.Roots).count
  }

  /**
   * Return the number of node records.
   *
   * @returns Node record count.
   */
  nodeCount(): number {
    return this.#requiredSection(SectionKind.Nodes).count
  }

  /**
   * Return the number of token records.
   *
   * @returns Token record count.
   */
  tokenCount(): number {
    return this.#requiredSection(SectionKind.Tokens).count
  }

  /**
   * Return the number of trivia records.
   *
   * @returns Trivia record count.
   */
  triviaCount(): number {
    return this.#sections.get(SectionKind.Trivia)?.count ?? 0
  }

  /**
   * Return the number of source records.
   *
   * @returns Source record count.
   */
  sourceCount(): number {
    return this.#requiredSection(SectionKind.Sources).count
  }

  /**
   * Return the number of diagnostic records.
   *
   * @returns Diagnostic record count.
   */
  diagnosticCount(): number {
    return this.#sections.get(SectionKind.Diagnostics)?.count ?? 0
  }

  /**
   * Return metadata for one section kind.
   *
   * @param kind - Numeric section kind.
   * @returns Section metadata, or null when absent.
   */
  section(kind: number): SectionMetadata | null {
    const section = this.#sections.get(kind)
    if (!section) {
      return null
    }
    return {
      kind: kind as SectionKindValue,
      offset: section.offset,
      byteLength: section.byteLength,
      recordSize: section.recordSize,
      count: section.count,
      required: (section.flags & SECTION_FLAG_REQUIRED) !== 0
    }
  }

  /**
   * Copy the complete snapshot byte buffer.
   *
   * @returns New byte array with the snapshot contents.
   */
  toBytes(): Uint8Array {
    return this.#bytes.slice()
  }

  /**
   * Return a new accessor with external source texts attached.
   *
   * @param sources - Source texts ordered by source id.
   * @returns New accessor backed by copied snapshot bytes.
   */
  withSources(sources: readonly string[]): BinarySnapshotAccessor {
    if (sources.length !== this.sourceCount()) {
      throw new OxMf2SourceTextError({
        code: OxMf2ErrorCode.SourceTextCountMismatch,
        message: 'source text count does not match snapshot source count'
      })
    }
    for (let i = 0; i < this.sourceCount(); i++) {
      if (this.#sourceEmbeddedTextBytes(i) !== null) {
        throw new OxMf2SourceTextError({
          code: OxMf2ErrorCode.SourceTextNotIncluded,
          message: 'snapshot already includes source text'
        })
      }
    }
    return new BinarySnapshotAccessor(this.#bytes.slice(), sources)
  }

  /**
   * Return the root node id stored in a root record.
   *
   * @param rootId - Zero-based root id.
   * @returns Node id referenced by the root.
   */
  rootNodeId(rootId: number): number {
    return this.#readU32(this.#recordOffset(SectionKind.Roots, rootId, ROOT_RECORD_SIZE))
  }

  /**
   * Return the diagnostic range stored in a root record.
   *
   * @param rootId - Zero-based root id.
   * @returns Start index and count for root diagnostics.
   */
  rootDiagnosticRange(rootId: number): [number, number] {
    const offset = this.#recordOffset(SectionKind.Roots, rootId, ROOT_RECORD_SIZE)
    return [this.#readU32(offset + 8), this.#readU32(offset + 12)]
  }

  /**
   * Return the syntax kind stored in a node record.
   *
   * @param nodeId - Zero-based node id.
   * @returns Numeric syntax kind.
   */
  nodeKind(nodeId: number): SyntaxKindValue {
    return this.#readU16(
      this.#recordOffset(SectionKind.Nodes, nodeId, NODE_RECORD_SIZE)
    ) as SyntaxKindValue
  }

  /**
   * Return the source span stored in a node record.
   *
   * @param nodeId - Zero-based node id.
   * @returns Half-open UTF-8 byte span.
   */
  nodeSpan(nodeId: number): Span {
    const offset = this.#recordOffset(SectionKind.Nodes, nodeId, NODE_RECORD_SIZE)
    return { start: this.#readU32(offset + 4), end: this.#readU32(offset + 8) }
  }

  /**
   * Return the child edge count stored in a node record.
   *
   * @param nodeId - Zero-based node id.
   * @returns Child edge count.
   */
  nodeChildCount(nodeId: number): number {
    return this.#readU32(this.#recordOffset(SectionKind.Nodes, nodeId, NODE_RECORD_SIZE) + 16)
  }

  /**
   * Return a child handle from a node edge range.
   *
   * @param nodeId - Zero-based node id.
   * @param index - Zero-based child edge index.
   * @returns Child node or token handle.
   */
  nodeChildAt(nodeId: number, index: number): ChildHandle {
    const childCount = this.nodeChildCount(nodeId)
    assertIndex(index, childCount, 'child')
    const nodeOffset = this.#recordOffset(SectionKind.Nodes, nodeId, NODE_RECORD_SIZE)
    const edgeStart = this.#readU32(nodeOffset + 12)
    const edgeOffset = this.#recordOffset(SectionKind.Edges, edgeStart + index, EDGE_RECORD_SIZE)
    const edgeKind = this.#readU16(edgeOffset)
    const refId = this.#readU32(edgeOffset + 4)
    return edgeKind === EDGE_KIND_TOKEN ? this.token(refId) : this.node(refId)
  }

  /**
   * Return the syntax kind stored in a token record.
   *
   * @param tokenId - Zero-based token id.
   * @returns Numeric syntax kind.
   */
  tokenKind(tokenId: number): SyntaxKindValue {
    return this.#readU16(
      this.#recordOffset(SectionKind.Tokens, tokenId, TOKEN_RECORD_SIZE)
    ) as SyntaxKindValue
  }

  /**
   * Return the source span stored in a token record.
   *
   * @param tokenId - Zero-based token id.
   * @returns Half-open UTF-8 byte span.
   */
  tokenSpan(tokenId: number): Span {
    const offset = this.#recordOffset(SectionKind.Tokens, tokenId, TOKEN_RECORD_SIZE)
    return { start: this.#readU32(offset + 4), end: this.#readU32(offset + 8) }
  }

  /**
   * Return trivia handles attached to one side of a token.
   *
   * @param tokenId - Zero-based token id.
   * @param side - Token side to read.
   * @returns Trivia handles in source order.
   */
  tokenTrivia(tokenId: number, side: 'leading' | 'trailing'): TriviaHandle[] {
    const offset = this.#recordOffset(SectionKind.Tokens, tokenId, TOKEN_RECORD_SIZE)
    const rangeOffset = side === 'leading' ? 16 : 24
    const start = this.#readU32(offset + rangeOffset)
    const count = this.#readU32(offset + rangeOffset + 4)
    return Array.from({ length: count }, (_, index) => this.trivia(start + index))
  }

  /**
   * Return the syntax kind stored in a trivia record.
   *
   * @param triviaId - Zero-based trivia id.
   * @returns Numeric syntax kind.
   */
  triviaKind(triviaId: number): SyntaxKindValue {
    return this.#readU16(
      this.#recordOffset(SectionKind.Trivia, triviaId, TRIVIA_RECORD_SIZE)
    ) as SyntaxKindValue
  }

  /**
   * Return the source span stored in a trivia record.
   *
   * @param triviaId - Zero-based trivia id.
   * @returns Half-open UTF-8 byte span.
   */
  triviaSpan(triviaId: number): Span {
    const offset = this.#recordOffset(SectionKind.Trivia, triviaId, TRIVIA_RECORD_SIZE)
    return { start: this.#readU32(offset + 4), end: this.#readU32(offset + 8) }
  }

  /**
   * Return the source path string.
   *
   * @param sourceId - Zero-based source id.
   * @returns Source path, or null when absent.
   */
  sourcePath(sourceId: number): string | null {
    return this.#sourceString(sourceId, 4)
  }

  /**
   * Return the source locale string.
   *
   * @param sourceId - Zero-based source id.
   * @returns Locale identifier, or null when absent.
   */
  sourceLocale(sourceId: number): string | null {
    return this.#sourceString(sourceId, 8)
  }

  /**
   * Return the source message id string.
   *
   * @param sourceId - Zero-based source id.
   * @returns Message id, or null when absent.
   */
  sourceMessageId(sourceId: number): string | null {
    return this.#sourceString(sourceId, 12)
  }

  /**
   * Return the base offset stored in a source record.
   *
   * @param sourceId - Zero-based source id.
   * @returns Base UTF-8 byte offset.
   */
  sourceBaseOffset(sourceId: number): number {
    return this.#readU32(this.#recordOffset(SectionKind.Sources, sourceId, SOURCE_RECORD_SIZE) + 16)
  }

  /**
   * Decode a slice of source text for one source record.
   *
   * @param sourceId - Zero-based source id.
   * @param span - Half-open UTF-8 byte span to decode.
   * @returns Decoded source slice.
   */
  sourceSlice(sourceId: number, span: Span): string {
    const bytes = this.#sourceTextBytes(sourceId)
    if (!bytes) {
      throw new OxMf2SourceTextError({
        code: OxMf2ErrorCode.SourceTextNotIncluded,
        message: 'source text is not available'
      })
    }
    try {
      return decodeUtf8Slice(bytes, span)
    } catch (cause) {
      throw new OxMf2SourceTextError({
        code: OxMf2ErrorCode.SourceTextSpanOutOfBounds,
        message: 'source text span is out of bounds or not UTF-8 aligned',
        cause
      })
    }
  }

  /**
   * Return diagnostic labels attached to one diagnostic.
   *
   * @param index - Zero-based diagnostic index.
   * @returns Diagnostic label views in snapshot order.
   */
  diagnosticLabels(index: number): DiagnosticLabelView[] {
    const offset = this.#recordOffset(SectionKind.Diagnostics, index, DIAGNOSTIC_RECORD_SIZE)
    const start = this.#readU32(offset + 20)
    const count = this.#readU32(offset + 24)
    return Array.from({ length: count }, (_, labelIndex) => {
      const labelOffset = this.#recordOffset(
        SectionKind.DiagnosticLabels,
        start + labelIndex,
        DIAGNOSTIC_LABEL_RECORD_SIZE
      )
      return {
        span: {
          start: this.#readU32(labelOffset + 4),
          end: this.#readU32(labelOffset + 8)
        },
        message: this.#string(this.#readU32(labelOffset + 12))
      }
    })
  }

  #diagnosticView(index: number): DiagnosticView {
    const offset = this.#recordOffset(SectionKind.Diagnostics, index, DIAGNOSTIC_RECORD_SIZE)
    const sourceId = this.#readU32(offset)
    const span = { start: this.#readU32(offset + 4), end: this.#readU32(offset + 8) }
    return {
      rootId: this.#rootIdForDiagnostic(index),
      sourceId,
      severity: this.#readU8(offset + 12) as DiagnosticSeverityValue,
      code: this.#readU16(offset + 14) as DiagnosticCodeValue,
      message: this.#string(this.#readU32(offset + 16)),
      span,
      location: this.#sourceLocation(sourceId, span),
      labels: this.diagnosticLabels(index)
    }
  }

  #sourceLocation(sourceId: number, span: Span): SourceLocation | null {
    const bytes = this.#sourceTextBytes(sourceId)
    if (!bytes || span.start > bytes.byteLength || !isUtf8Boundary(bytes, span.start)) {
      return null
    }
    let line = 1
    let lineStart = 0
    for (let i = 0; i < span.start; i++) {
      const byte = bytes[i]
      if (byte === 0x0a) {
        line++
        lineStart = i + 1
      } else if (byte === 0x0d) {
        line++
        lineStart = i + 1
        if (bytes[i + 1] === 0x0a && i + 1 < span.start) {
          i++
          lineStart = i + 1
        }
      }
    }
    return { line, column: span.start - lineStart }
  }

  #rootIdForDiagnostic(index: number): number {
    for (let rootId = 0; rootId < this.rootCount(); rootId++) {
      const [start, count] = this.rootDiagnosticRange(rootId)
      if (index >= start && index < start + count) {
        return rootId
      }
    }
    return 0
  }

  #sourceString(sourceId: number, fieldOffset: number): string | null {
    const sourceOffset = this.#recordOffset(SectionKind.Sources, sourceId, SOURCE_RECORD_SIZE)
    return this.#string(this.#readU32(sourceOffset + fieldOffset))
  }

  #sourceTextBytes(sourceId: number): Uint8Array | null {
    return this.#sourceEmbeddedTextBytes(sourceId) ?? this.#externalSources?.[sourceId] ?? null
  }

  #sourceEmbeddedTextBytes(sourceId: number): Uint8Array | null {
    const sourceOffset = this.#recordOffset(SectionKind.Sources, sourceId, SOURCE_RECORD_SIZE)
    const textSource = this.#readU32(sourceOffset + 20)
    if (textSource === NONE_U32) {
      return null
    }
    const section = this.#sections.get(SectionKind.SourceTextData)
    if (!section) {
      return null
    }
    const offset = this.#readU32(sourceOffset + 24)
    const byteLength = this.#readU32(sourceOffset + 28)
    const start = section.offset + offset
    return this.#bytes.subarray(start, start + byteLength)
  }

  #string(id: number): string | null {
    if (id === NONE_U32) {
      return null
    }
    const offsets = this.#requiredSection(SectionKind.StringOffsets)
    const data = this.#requiredSection(SectionKind.StringData)
    if (id >= offsets.count) {
      return null
    }
    const offset = offsets.offset + id * 8
    const start = data.offset + this.#readU32(offset)
    const end = start + this.#readU32(offset + 4)
    return TEXT_DECODER.decode(this.#bytes.subarray(start, end))
  }

  #recordOffset(kind: number, index: number, expectedRecordSize: number): number {
    const section = this.#requiredSection(kind)
    assertIndex(index, section.count, 'record')
    if (section.recordSize !== expectedRecordSize) {
      throw snapshotError(OxMf2ErrorCode.DecodeInvalidRecordSize, `invalid ${kind} record size`)
    }
    return section.offset + index * expectedRecordSize
  }

  #requiredSection(kind: number): SectionSlice {
    const section = this.#sections.get(kind)
    if (!section) {
      throw snapshotError(
        OxMf2ErrorCode.DecodeMissingRequiredSection,
        `missing required section ${kind}`
      )
    }
    return section
  }

  #readU8(offset: number): number {
    return this.#view.getUint8(offset)
  }

  #readU16(offset: number): number {
    return this.#view.getUint16(offset, true)
  }

  #readU32(offset: number): number {
    return this.#view.getUint32(offset, true)
  }
}

class BinaryRootHandle implements RootHandle {
  constructor(
    private readonly snapshot: BinarySnapshotAccessor,
    readonly id: number
  ) {}

  node(): NodeHandle {
    return this.snapshot.node(this.snapshot.rootNodeId(this.id))
  }

  diagnostics(): DiagnosticView[] {
    const [start, count] = this.snapshot.rootDiagnosticRange(this.id)
    return Array.from({ length: count }, (_, index) => this.snapshot.diagnostic(start + index))
  }
}

class BinaryNodeHandle implements NodeHandle {
  constructor(
    private readonly snapshot: BinarySnapshotAccessor,
    readonly id: number
  ) {}

  kind(): SyntaxKindValue {
    return this.snapshot.nodeKind(this.id)
  }

  span(): Span {
    return this.snapshot.nodeSpan(this.id)
  }

  childCount(): number {
    return this.snapshot.nodeChildCount(this.id)
  }

  childAt(index: number): ChildHandle {
    return this.snapshot.nodeChildAt(this.id, index)
  }

  children(): ChildHandle[] {
    return Array.from({ length: this.childCount() }, (_, index) => this.childAt(index))
  }
}

class BinaryTokenHandle implements TokenHandle {
  constructor(
    private readonly snapshot: BinarySnapshotAccessor,
    readonly id: number
  ) {}

  kind(): SyntaxKindValue {
    return this.snapshot.tokenKind(this.id)
  }

  span(): Span {
    return this.snapshot.tokenSpan(this.id)
  }

  leadingTrivia(): TriviaHandle[] {
    return this.snapshot.tokenTrivia(this.id, 'leading')
  }

  trailingTrivia(): TriviaHandle[] {
    return this.snapshot.tokenTrivia(this.id, 'trailing')
  }
}

class BinaryTriviaHandle implements TriviaHandle {
  constructor(
    private readonly snapshot: BinarySnapshotAccessor,
    readonly id: number
  ) {}

  kind(): SyntaxKindValue {
    return this.snapshot.triviaKind(this.id)
  }

  span(): Span {
    return this.snapshot.triviaSpan(this.id)
  }
}

class BinarySourceView implements SourceView {
  constructor(
    private readonly snapshot: BinarySnapshotAccessor,
    readonly id: number
  ) {}

  path(): string | null {
    return this.snapshot.sourcePath(this.id)
  }

  locale(): string | null {
    return this.snapshot.sourceLocale(this.id)
  }

  messageId(): string | null {
    return this.snapshot.sourceMessageId(this.id)
  }

  baseOffset(): number {
    return this.snapshot.sourceBaseOffset(this.id)
  }

  sourceSlice(span: Span): string {
    return this.snapshot.sourceSlice(this.id, span)
  }
}

function collectRoots(snapshot: BinarySnapshotAccessor): RootHandle[] {
  return Array.from({ length: snapshot.rootCount() }, (_, index) => snapshot.root(index))
}

function collectSources(snapshot: BinarySnapshotAccessor): SourceView[] {
  return Array.from({ length: snapshot.sourceCount() }, (_, index) => snapshot.source(index))
}

function collectDiagnostics(snapshot: BinarySnapshotAccessor): DiagnosticView[] {
  return Array.from({ length: snapshot.diagnosticCount() }, (_, index) =>
    snapshot.diagnostic(index)
  )
}

function decodeSections(bytes: Uint8Array, view: DataView): Map<number, SectionSlice> {
  if (bytes.byteLength < HEADER_SIZE) {
    throw snapshotError(OxMf2ErrorCode.DecodeBufferTooShort, 'snapshot header is too short')
  }
  if (readMagic(bytes) !== SNAPSHOT_MAGIC) {
    throw snapshotError(OxMf2ErrorCode.DecodeInvalidMagic, 'invalid snapshot magic')
  }
  if (view.getUint16(8, true) !== SNAPSHOT_MAJOR_VERSION) {
    throw snapshotError(
      OxMf2ErrorCode.DecodeUnsupportedMajorVersion,
      'unsupported snapshot major version'
    )
  }
  if (view.getUint16(10, true) !== SNAPSHOT_MINOR_VERSION) {
    throw snapshotError(
      OxMf2ErrorCode.DecodeUnsupportedMinorVersion,
      'unsupported snapshot minor version'
    )
  }
  if (view.getUint32(12, true) !== SNAPSHOT_FEATURE_FLAGS) {
    throw snapshotError(OxMf2ErrorCode.DecodeInvalidFeatureFlags, 'invalid feature flags')
  }
  if (view.getUint32(16, true) !== HEADER_SIZE || view.getUint32(20, true) !== HEADER_SIZE) {
    throw snapshotError(OxMf2ErrorCode.DecodeInvalidHeaderLength, 'invalid header length')
  }
  const sectionCount = view.getUint16(24, true)
  const tableEnd = HEADER_SIZE + sectionCount * SECTION_RECORD_SIZE
  if (tableEnd > bytes.byteLength) {
    throw snapshotError(
      OxMf2ErrorCode.DecodeSectionTableOutOfBounds,
      'section table is out of bounds'
    )
  }

  const sections = new Map<number, SectionSlice>()
  for (let i = 0; i < sectionCount; i++) {
    const offset = HEADER_SIZE + i * SECTION_RECORD_SIZE
    const kind = view.getUint16(offset, true)
    const flags = view.getUint16(offset + 2, true)
    const sectionOffset = view.getUint32(offset + 4, true)
    const byteLength = view.getUint32(offset + 8, true)
    const count = view.getUint32(offset + 12, true)
    const recordSize = view.getUint16(offset + 16, true)
    const alignment = view.getUint8(offset + 18)
    if (sections.has(kind)) {
      throw snapshotError(OxMf2ErrorCode.DecodeDuplicateSection, 'duplicate section')
    }
    if (sectionOffset + byteLength > bytes.byteLength) {
      throw snapshotError(
        OxMf2ErrorCode.DecodeInvalidSectionBounds,
        'section bounds exceed snapshot byte length'
      )
    }
    if (alignment !== SECTION_ALIGNMENT || sectionOffset % SECTION_ALIGNMENT !== 0) {
      throw snapshotError(
        OxMf2ErrorCode.DecodeInvalidSectionAlignment,
        'section alignment is invalid'
      )
    }
    if (!isKnownSectionKind(kind)) {
      throw snapshotError(OxMf2ErrorCode.DecodeUnknownSection, 'unknown section kind')
    }
    const expectedRecordSize = sectionRecordSize(kind)
    if (recordSize !== expectedRecordSize) {
      throw snapshotError(OxMf2ErrorCode.DecodeInvalidRecordSize, 'section record size is invalid')
    }
    if (recordSize !== 0 && count * recordSize !== byteLength) {
      throw snapshotError(OxMf2ErrorCode.DecodeInvalidSectionCount, 'section count is invalid')
    }
    sections.set(kind, {
      kind,
      flags,
      offset: sectionOffset,
      byteLength,
      count,
      recordSize,
      alignment
    })
  }

  for (const kind of [
    SectionKind.Roots,
    SectionKind.Sources,
    SectionKind.Nodes,
    SectionKind.Edges,
    SectionKind.Tokens,
    SectionKind.StringOffsets,
    SectionKind.StringData
  ]) {
    const section = sections.get(kind)
    if (!section) {
      throw snapshotError(OxMf2ErrorCode.DecodeMissingRequiredSection, 'missing core section')
    }
    if ((section.flags & SECTION_FLAG_REQUIRED) === 0 || !isCoreSectionKind(kind)) {
      throw snapshotError(OxMf2ErrorCode.DecodeInvalidSectionFlags, 'invalid core section flags')
    }
  }
  return sections
}

function readMagic(bytes: Uint8Array): string {
  return String.fromCharCode(...bytes.subarray(0, 8))
}

function assertIndex(index: number, count: number, label: string): void {
  if (!Number.isInteger(index) || index < 0 || index >= count) {
    throw new RangeError(`${label} index is out of range`)
  }
}

function isKnownSectionKind(kind: number): kind is SectionKindValue {
  return Object.values(SectionKind).includes(kind as SectionKindValue)
}

function snapshotError(code: OxMf2ErrorCodeValue, message: string): OxMf2SnapshotError {
  return new OxMf2SnapshotError({
    code,
    message
  })
}
