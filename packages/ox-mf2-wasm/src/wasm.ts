/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import { OxMf2ErrorCode, OxMf2InitializationError } from '@intlify/ox-mf2-shared'

/** Generated wasm-bindgen functions used by the JavaScript wrapper. */
export type WasmBinding = {
  /**
   * Parse one normalized input into snapshot bytes.
   *
   * @param input - Normalized source object.
   * @param options - Normalized parse options.
   * @returns Snapshot bytes.
   */
  parseMessageToSnapshot(input: unknown, options: unknown): Uint8Array

  /**
   * Parse normalized inputs into one batch snapshot.
   *
   * @param items - Normalized source objects.
   * @param options - Normalized batch parse options.
   * @returns Snapshot bytes.
   */
  parseBatchToSnapshot(items: unknown[], options: unknown): Uint8Array

  /**
   * Validate and normalize existing snapshot bytes.
   *
   * @param bytes - Snapshot bytes to decode.
   * @returns Snapshot bytes copied from the WASM boundary.
   */
  decodeSnapshotBytes(bytes: Uint8Array): Uint8Array
}

let binding: WasmBinding | null = null

/**
 * Store the generated WASM binding after initialization.
 *
 * @param nextBinding - Generated WASM binding module.
 */
export function setWasmBinding(nextBinding: WasmBinding): void {
  binding = nextBinding
}

/**
 * Return the initialized WASM binding or raise an initialization error.
 *
 * @returns Initialized WASM binding.
 */
export function getWasmBinding(): WasmBinding {
  if (binding) {
    return binding
  }
  throw new OxMf2InitializationError({
    code: OxMf2ErrorCode.InitializationWasmNotInitialized,
    message: 'ox-mf2 WASM runtime has not been initialized'
  })
}
