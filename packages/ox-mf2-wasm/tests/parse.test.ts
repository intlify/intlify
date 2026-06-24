import { beforeAll, expect, test } from 'vite-plus/test'
import {
  OxMf2SourceTextError,
  SectionKind,
  decodeSnapshot,
  init,
  parseBatch,
  parseMessage
} from '../src/index.ts'
import { ensureWasmArtifacts } from './ensure-wasm-artifact.ts'

beforeAll(async () => {
  await ensureWasmArtifacts()
  await init()
})

test('parseMessage returns a snapshot-backed result', () => {
  const result = parseMessage('Hello {$name}')

  expect(result.roots).toHaveLength(1)
  expect(result.sources).toHaveLength(1)
  expect(result.snapshot.rootCount()).toBe(1)
  expect(result.snapshot.nodeCount()).toBeGreaterThan(0)
  expect(result.snapshot.tokenCount()).toBeGreaterThan(0)
  expect(result.source.sourceSlice({ start: 0, end: 5 })).toBe('Hello')
})

test('decodeSnapshot can reattach external source text', () => {
  const result = parseMessage('Hello {$name}')
  const decoded = decodeSnapshot(result.snapshot.toBytes())
  const withSources = decoded.withSources(['Hello {$name}'])

  expect(() => decoded.sources[0]?.sourceSlice({ start: 0, end: 5 })).toThrow(OxMf2SourceTextError)
  expect(withSources.sources[0]?.sourceSlice({ start: 6, end: 13 })).toBe('{$name}')
})

test('parseBatch preserves root and source order', () => {
  const result = parseBatch([
    { source: 'One', messageId: 'one' },
    { source: 'Two', messageId: 'two' }
  ])

  expect(result.execution).toBe('sequential')
  expect(result.degraded).toBe(false)
  expect(result.roots).toHaveLength(2)
  expect(result.sources.map(source => source.messageId())).toEqual(['one', 'two'])
})

test('parallel batch request degrades to sequential in Phase 2', () => {
  const result = parseBatch([{ source: 'One' }, { source: 'Two' }], {
    batchExecution: 'parallel'
  })

  expect(result.execution).toBe('sequential')
  expect(result.degraded).toBe(true)
})

test('optional trivia section is absent when trivia is disabled', () => {
  const result = parseMessage('Hello {$name}', {
    collectTrivia: false,
    includeTrivia: false
  })

  expect(result.snapshot.triviaCount()).toBe(0)
  expect(result.snapshot.section(SectionKind.Trivia)).toBeNull()
  expect(() => result.snapshot.trivia(0)).toThrow(RangeError)
})

test('withSources rejects embedded source text snapshots', () => {
  const result = parseMessage('Hello', { includeSourceText: true })
  const decoded = decodeSnapshot(result.snapshot.toBytes())

  expect(() => decoded.withSources(['Hello'])).toThrow(OxMf2SourceTextError)
})
