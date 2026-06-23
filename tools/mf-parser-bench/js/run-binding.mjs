import { mkdir, writeFile } from 'node:fs/promises'
import { dirname } from 'node:path'

import { getBindingOperation, getBindingRuntime } from '../binding-targets.mjs'

const args = parseArgs(process.argv.slice(2))
const runtime = getBindingRuntime(requiredArg(args, 'runtime'))
const operation = getBindingOperation(requiredArg(args, 'operation'))
const iterations = Number(args.iterations ?? process.env.OX_MF2_BINDING_BENCH_ITERATIONS ?? '100')

if (!Number.isInteger(iterations) || iterations < 1) {
  throw new Error(`--iterations must be a positive integer: ${args.iterations}`)
}

const api = await loadApi(runtime.name)
const message = 'Hello {$name}'
const batch = [
  { source: 'Hello {$name}', messageId: 'hello' },
  { source: 'Welcome {$name}', messageId: 'welcome' }
]
const parsed = api.parseMessage(message)
const bytes = parsed.snapshot.toBytes()
const rootNode = parsed.root.node()

const started = process.hrtime.bigint()

for (let index = 0; index < iterations; index++) {
  switch (operation.name) {
    case 'parse-message':
      api.parseMessage(message)
      break
    case 'parse-batch':
      api.parseBatch(batch)
      break
    case 'decode-snapshot':
      api.decodeSnapshot(bytes)
      break
    case 'snapshot-to-bytes-copy':
      parsed.snapshot.toBytes()
      break
    default:
      throw new Error(`unsupported binding operation: ${operation.name}`)
  }
}

const elapsedNs = Number(process.hrtime.bigint() - started)
const summary = {
  runtime: runtime.name,
  operation: operation.name,
  iterations,
  checksum: bytes.length + rootNode.kind(),
  elapsedMs: elapsedNs / 1_000_000
}

if (args['summary-json']) {
  await mkdir(dirname(args['summary-json']), { recursive: true })
  await writeFile(args['summary-json'], `${JSON.stringify(summary, null, 2)}\n`)
}

console.log(`checksum=${summary.checksum}`)

/**
 * Load a built binding package for benchmarking.
 *
 * @param runtimeName - Binding runtime name.
 * @returns Binding API surface.
 */
async function loadApi(runtimeName) {
  if (runtimeName === 'napi') {
    const api = await import('@intlify/ox-mf2-napi')
    api.parseMessage('warmup')
    return api
  }

  if (runtimeName === 'wasm') {
    const api = await import('@intlify/ox-mf2-wasm')
    await api.init()
    return api
  }

  throw new Error(`unsupported binding runtime: ${runtimeName}`)
}

function requiredArg(values, name) {
  const value = values[name]
  if (!value) {
    throw new Error(`Missing required option --${name}`)
  }
  return value
}

function parseArgs(values) {
  const args = {}
  for (let index = 0; index < values.length; index++) {
    const value = values[index]
    if (!value.startsWith('--')) {
      throw new Error(`Unexpected argument: ${value}`)
    }
    const key = value.slice(2)
    const next = values[index + 1]
    if (!next || next.startsWith('--')) {
      args[key] = 'true'
    } else {
      args[key] = next
      index++
    }
  }
  return args
}
