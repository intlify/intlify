import { expect, test } from 'vite-plus/test'
import { SectionKind, normalizeResult } from '../src/index.ts'

import type {
  DiagnosticView,
  NodeHandle,
  ParseMessageResult,
  RootHandle,
  SectionMetadata,
  SnapshotAccessor,
  SourceView
} from '../src/index.ts'

test('normalizeResult extracts stable parity fields', () => {
  const diagnostic: DiagnosticView = {
    rootId: 0,
    sourceId: 0,
    severity: 0,
    code: 1,
    message: 'unexpected end of input',
    span: { start: 0, end: 1 },
    location: { line: 1, column: 0 },
    labels: []
  }
  const source: SourceView = {
    id: 0,
    path: () => null,
    locale: () => 'en',
    messageId: () => 'hello',
    baseOffset: () => 0,
    sourceSlice: () => 'x'
  }
  const node: NodeHandle = {
    id: 0,
    kind: () => 1,
    span: () => ({ start: 0, end: 1 }),
    childCount: () => 0,
    childAt: () => {
      throw new RangeError('out of range')
    },
    children: () => []
  }
  const root: RootHandle = {
    id: 0,
    node: () => node,
    diagnostics: () => [diagnostic]
  }
  const section: SectionMetadata = {
    kind: SectionKind.Roots,
    offset: 32,
    byteLength: 16,
    recordSize: 16,
    count: 1,
    required: true
  }
  const snapshot: SnapshotAccessor = {
    root: () => root,
    node: () => node,
    token: () => {
      throw new RangeError('out of range')
    },
    trivia: () => {
      throw new RangeError('out of range')
    },
    source: () => source,
    diagnostic: () => diagnostic,
    rootCount: () => 1,
    nodeCount: () => 1,
    tokenCount: () => 0,
    triviaCount: () => 0,
    sourceCount: () => 1,
    diagnosticCount: () => 1,
    section: kind => (kind === SectionKind.Roots ? section : null),
    toBytes: () => new Uint8Array()
  }
  const result: ParseMessageResult = {
    snapshot,
    roots: [root],
    root,
    sources: [source],
    source,
    diagnostics: [diagnostic]
  }

  expect(normalizeResult(result)).toEqual({
    rootIds: [0],
    sourceIds: [0],
    diagnostics: [
      {
        rootId: 0,
        sourceId: 0,
        severity: 0,
        code: 1,
        message: 'unexpected end of input',
        span: { start: 0, end: 1 },
        location: { line: 1, column: 0 },
        labels: []
      }
    ],
    counts: {
      roots: 1,
      sources: 1,
      nodes: 1,
      tokens: 0,
      trivia: 0,
      diagnostics: 1
    },
    sections: [section]
  })
})
