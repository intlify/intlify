import { expect, test } from 'vite-plus/test'

import { decodeCoreProcessResult } from '../core-process-result.mjs'

test('core process decoder permits only unavailable launch failures to be skipped', () => {
  const error = Object.assign(new Error('spawn core ENOENT'), { code: 'ENOENT' })

  expect(decodeCoreProcessResult({ error }, 'core')).toEqual({
    kind: 'unavailable',
    reason: 'spawn core ENOENT'
  })
})

test('core process decoder rejects non-launch execution failures', () => {
  const error = Object.assign(new Error('stdout exceeded maxBuffer'), { code: 'ENOBUFS' })

  expect(() => decodeCoreProcessResult({ error }, 'core')).toThrow(
    'failed to execute core: stdout exceeded maxBuffer'
  )
})

test('core process decoder rejects nonzero exits', () => {
  expect(() =>
    decodeCoreProcessResult({ status: 1, stderr: 'memory growth rejected', stdout: '' }, 'core')
  ).toThrow('memory growth rejected')
})

test('core process decoder rejects malformed and incomplete output', () => {
  expect(() => decodeCoreProcessResult({ status: 0, stderr: '', stdout: '{' }, 'core')).toThrow(
    'resource benchmark core emitted malformed JSON'
  )
  expect(() => decodeCoreProcessResult({ status: 0, stderr: '', stdout: '{}' }, 'core')).toThrow(
    'resource benchmark core output is missing result arrays'
  )
})

test('core process decoder returns complete measured output', () => {
  const output = { results: [], memoryGrowthChecks: [] }

  expect(
    decodeCoreProcessResult({ status: 0, stderr: '', stdout: JSON.stringify(output) }, 'core')
  ).toEqual({ kind: 'measured', output })
})
