import { beforeAll, expect, test } from 'vite-plus/test'
import { checkFormat, checkSnapshot, formatMessage, formatSnapshot, init } from '../src/index.ts'
import { ensureWasmArtifacts, wasmArtifactTimeoutMs } from './ensure-wasm-artifact.ts'
import { FORMAT_SNAPSHOT_SOURCE, formatSnapshotBytes } from './fixtures.ts'
import { expectFormatFailure } from './helpers.ts'

beforeAll(async () => {
  await ensureWasmArtifacts()
  await init()
}, wasmArtifactTimeoutMs)

test('formatMessage formats standard mode output', () => {
  const result = formatMessage('.input   {$count   :number}\n{{Value {$count   :number}}}')

  expect(result).toEqual({
    ok: true,
    changed: true,
    code: '.input {$count :number}\n{{Value {$count :number}}}'
  })
})

test('formatMessage formats preserve mode output', () => {
  const result = formatMessage('.input   {$name   :string} {{Hello {$name}}}', {
    mode: 'preserve'
  })

  expect(result).toEqual({
    ok: true,
    changed: true,
    code: '.input {$name :string} {{Hello {$name}}}'
  })
})

test('checkFormat reports changed and unchanged inputs', () => {
  expect(checkFormat('.input   {$count   :number}\n{{Value {$count}}}')).toEqual({
    ok: true,
    changed: true
  })
  expect(checkFormat('Hello {$name}')).toEqual({
    ok: true,
    changed: false
  })
})

test('parser diagnostics return a failed formatter result', () => {
  const result = formatMessage('Hello {$name')
  const failure = expectFormatFailure(result)

  expect(failure.errors).toEqual([])
  expect(failure.diagnostics.length).toBeGreaterThan(0)
  expect(failure.diagnostics[0]?.rootId).toBe(0)
  expect(failure.diagnostics[0]?.sourceId).toBe(0)
  expect(failure.diagnostics[0]?.span.start).toBeGreaterThanOrEqual(0)
  expect(failure.diagnostics[0]?.location?.line).toBe(1)
})

test('corrupt snapshot bytes return invalid_snapshot', () => {
  const result = formatSnapshot(new Uint8Array([0, 1, 2]), 'Hello')
  const failure = expectFormatFailure(result)

  expect(failure.diagnostics).toEqual([])
  expect(failure.errors[0]?.code).toBe('invalid_snapshot')
  expect(failure.errors[0]?.details?.reason).toBe('corrupt')
})

test('unsupported snapshot version reports wire versions', () => {
  const snapshot = formatSnapshotBytes()
  snapshot[10] = 3
  snapshot[11] = 0
  const failure = expectFormatFailure(formatSnapshot(snapshot, FORMAT_SNAPSHOT_SOURCE))

  expect(failure.errors[0]?.details).toMatchObject({
    reason: 'unsupported_version',
    version: { major: 0, minor: 3 },
    supportedVersions: [{ major: 0, minor: 1 }]
  })
})

test('checkSnapshot validates corrupt snapshot bytes', () => {
  const result = checkSnapshot(new Uint8Array([0, 1, 2]), 'Hello')
  const failure = expectFormatFailure(result)

  expect(failure.diagnostics).toEqual([])
  expect(failure.errors[0]?.code).toBe('invalid_snapshot')
})

test('formatSnapshot formats a valid serialized snapshot', () => {
  const result = formatSnapshot(formatSnapshotBytes(), FORMAT_SNAPSHOT_SOURCE)

  expect(result).toEqual({
    ok: true,
    changed: true,
    code: '.input {$count :number}\n{{Value {$count :number}}}'
  })
})

test('checkSnapshot reports changed state for a valid snapshot', () => {
  const result = checkSnapshot(formatSnapshotBytes(), FORMAT_SNAPSHOT_SOURCE)

  expect(result).toEqual({
    ok: true,
    changed: true
  })
})
