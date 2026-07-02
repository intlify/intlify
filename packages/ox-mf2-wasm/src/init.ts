/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import { OxMf2ErrorCode, OxMf2InitializationError } from '@intlify/ox-mf2-shared'
import { setWasmBinding } from './wasm.ts'

import type { WasmInitInput } from '@intlify/ox-mf2-shared'
import type { WasmBinding } from './wasm.ts'

type GeneratedWasmModule = WasmBinding & {
  readonly default: (
    input?: { readonly module_or_path: WasmInitInput | Promise<WasmInitInput> } | WasmInitInput
  ) => Promise<unknown>
}

let initialized = false
let inFlight: Promise<void> | null = null

/**
 * Initialize the WASM runtime before parse or decode APIs are used.
 *
 * @param input - Optional WASM module, URL, request info, or bytes.
 * @returns Promise that resolves after the WASM binding is ready.
 */
export async function init(input?: WasmInitInput): Promise<void> {
  if (initialized) {
    if (input === undefined) {
      return
    }
    throw initializationError('ox-mf2 WASM runtime is already initialized')
  }

  if (inFlight) {
    if (input === undefined) {
      return inFlight
    }
    throw initializationError('ox-mf2 WASM initialization is already in flight')
  }

  inFlight = initializeWasm(input)
  try {
    await inFlight
    initialized = true
  } finally {
    inFlight = null
  }
}

async function initializeWasm(input?: WasmInitInput): Promise<void> {
  let module: GeneratedWasmModule
  try {
    module = (await import(
      /* @vite-ignore */ new URL('../dist/ox_mf2_wasm.js', import.meta.url).href
    )) as GeneratedWasmModule
  } catch (cause) {
    throw initializationError('ox-mf2 WASM artifact is not built in this workspace yet', cause)
  }

  await module.default({ module_or_path: await resolveWasmInput(input) })
  setWasmBinding(module)
}

async function resolveWasmInput(input?: WasmInitInput): Promise<WasmInitInput> {
  if (input !== undefined) {
    if (input instanceof URL && input.protocol === 'file:' && isNodeRuntime()) {
      return readFileUrl(input)
    }
    return input
  }

  const defaultUrl = new URL('../dist/ox_mf2_wasm_bg.wasm', import.meta.url)
  if (isNodeRuntime()) {
    return readFileUrl(defaultUrl)
  }
  return defaultUrl
}

async function readFileUrl(url: URL): Promise<Uint8Array> {
  const [{ readFile }, { fileURLToPath }] = await Promise.all([
    import('node:fs/promises'),
    import('node:url')
  ])
  return new Uint8Array(await readFile(fileURLToPath(url)))
}

function isNodeRuntime(): boolean {
  return typeof process !== 'undefined' && !!process.versions?.node
}

function initializationError(message: string, cause?: unknown): OxMf2InitializationError {
  return new OxMf2InitializationError({
    code: OxMf2ErrorCode.InitializationWasmNotInitialized,
    message,
    cause
  })
}
