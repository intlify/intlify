import { expect, test } from 'vite-plus/test'
import { loadNativeBinding } from '../src/loader.ts'
import {
  OxMf2SourceTextError,
  SectionKind,
  decodeSnapshot,
  parseBatch,
  parseMessage
} from '../src/index.ts'

const hasNativeBinding = loadNativeBinding().binding !== null

test.runIf(hasNativeBinding)('parseMessage returns a snapshot-backed result', () => {
  const result = parseMessage('Hello {$name}')

  expect(result.roots).toHaveLength(1)
  expect(result.sources).toHaveLength(1)
  expect(result.snapshot.rootCount()).toBe(1)
  expect(result.snapshot.sourceCount()).toBe(1)
  expect(result.snapshot.nodeCount()).toBeGreaterThan(0)
  expect(result.snapshot.tokenCount()).toBeGreaterThan(0)
  expect(result.source.sourceSlice({ start: 0, end: 5 })).toBe('Hello')
  expect(result.snapshot.toBytes().byteLength).toBeGreaterThan(0)
})

test.runIf(hasNativeBinding)('decodeSnapshot can reattach external source text', () => {
  const result = parseMessage('Hello {$name}')
  const decoded = decodeSnapshot(result.snapshot.toBytes())
  const withSources = decoded.withSources(['Hello {$name}'])

  expect(() => decoded.sources[0]?.sourceSlice({ start: 0, end: 5 })).toThrow(OxMf2SourceTextError)
  expect(withSources.sources[0]?.sourceSlice({ start: 6, end: 13 })).toBe('{$name}')
})

test.runIf(hasNativeBinding)('parseBatch preserves root and source order', () => {
  const result = parseBatch([
    { source: 'One', messageId: 'one' },
    { source: 'Two', messageId: 'two' }
  ])

  expect(result.execution).toBe('sequential')
  expect(result.degraded).toBe(false)
  expect(result.roots).toHaveLength(2)
  expect(result.sources.map(source => source.messageId())).toEqual(['one', 'two'])
  expect(result.sources[0]?.sourceSlice({ start: 0, end: 3 })).toBe('One')
  expect(result.sources[1]?.sourceSlice({ start: 0, end: 3 })).toBe('Two')
})

test.runIf(hasNativeBinding)('parallel batch request degrades to sequential in Phase 2', () => {
  const result = parseBatch([{ source: 'One' }, { source: 'Two' }], {
    batchExecution: 'parallel'
  })

  expect(result.execution).toBe('sequential')
  expect(result.degraded).toBe(true)
})

test.runIf(hasNativeBinding)('snapshot.toBytes returns a defensive copy', () => {
  const result = parseMessage('Hello')
  const bytes = result.snapshot.toBytes()
  const originalFirstByte = bytes[0]

  bytes[0] = 0

  expect(result.snapshot.rootCount()).toBe(1)
  expect(result.snapshot.toBytes()[0]).toBe(originalFirstByte)
})

test.runIf(hasNativeBinding)('optional trivia section is absent when trivia is disabled', () => {
  const result = parseMessage('Hello {$name}', {
    collectTrivia: false,
    includeTrivia: false
  })

  expect(result.snapshot.triviaCount()).toBe(0)
  expect(result.snapshot.section(SectionKind.Trivia)).toBeNull()
  expect(() => result.snapshot.trivia(0)).toThrow(RangeError)
})

test.runIf(hasNativeBinding)('sourceSlice rejects non UTF-8 boundary spans', () => {
  const result = parseMessage('é')

  expect(() => result.source.sourceSlice({ start: 1, end: 2 })).toThrow(OxMf2SourceTextError)
})

test.runIf(hasNativeBinding)('diagnostics are materialized with root and source ids', () => {
  const result = parseMessage('Hello {')

  expect(result.diagnostics.length).toBeGreaterThan(0)
  expect(result.root.diagnostics()).toHaveLength(result.diagnostics.length)
  expect(result.diagnostics[0]?.rootId).toBe(0)
  expect(result.diagnostics[0]?.sourceId).toBe(0)
  expect(result.diagnostics[0]?.location?.line).toBe(1)
})

test.runIf(hasNativeBinding)('withSources rejects embedded source text snapshots', () => {
  const result = parseMessage('Hello', { includeSourceText: true })
  const decoded = decodeSnapshot(result.snapshot.toBytes())

  expect(() => decoded.withSources(['Hello'])).toThrow(OxMf2SourceTextError)
})
