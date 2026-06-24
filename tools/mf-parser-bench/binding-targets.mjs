/** Node.js binding runtimes measured by the binding benchmark harness. */
export const BINDING_RUNTIMES = [
  {
    name: 'napi',
    packageName: '@intlify/ox-mf2-napi',
    description: 'Node.js N-API binding'
  },
  {
    name: 'wasm',
    packageName: '@intlify/ox-mf2-wasm',
    description: 'Node.js WASM binding'
  }
]

/**
 * Binding benchmark operations aligned with design/004 phase names.
 *
 * @see design/004-ox-mf2-phase-2-language-bindings-design.md
 */
export const BINDING_OPERATIONS = [
  {
    name: 'parse-message',
    phase: 'parse_message_binding',
    description: 'parseMessage(source)'
  },
  {
    name: 'parse-batch',
    phase: 'parse_batch_binding',
    description: 'parseBatch(items)'
  },
  {
    name: 'decode-snapshot',
    phase: 'decode_snapshot_binding',
    description: 'decodeSnapshot(bytes)'
  },
  {
    name: 'snapshot-to-bytes-copy',
    phase: 'snapshot_to_bytes_copy_binding',
    description: 'snapshot.toBytes()'
  }
]

/** Binding operations exercised in the browser WASM benchmark harness. */
export const BROWSER_BINDING_OPERATIONS = BINDING_OPERATIONS.filter(
  operation => operation.name !== 'snapshot-to-bytes-copy'
)

/**
 * Resolve a binding runtime by name.
 *
 * @param name - Runtime name (`napi` or `wasm`).
 * @returns Runtime metadata.
 */
export function getBindingRuntime(name) {
  const runtime = BINDING_RUNTIMES.find(item => item.name === name)
  if (!runtime) {
    throw new Error(`Unknown binding runtime: ${name}`)
  }
  return runtime
}

/**
 * Resolve a binding operation by name.
 *
 * @param name - Operation name.
 * @returns Operation metadata.
 */
export function getBindingOperation(name) {
  const operation = BINDING_OPERATIONS.find(item => item.name === name)
  if (!operation) {
    throw new Error(`Unknown binding operation: ${name}`)
  }
  return operation
}
