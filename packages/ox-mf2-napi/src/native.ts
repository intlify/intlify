/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import { OxMf2ErrorCode, OxMf2InitializationError } from '@intlify/ox-mf2-shared'
import { loadNativeBinding } from './loader.ts'

import type { NativeBinding } from './loader.ts'

/**
 * Return the loaded native binding or raise an initialization error.
 *
 * @returns Native binding for the current platform.
 */
export function getNativeBinding(): NativeBinding {
  const result = loadNativeBinding()
  if (result.binding) {
    return result.binding
  }
  throw new OxMf2InitializationError({
    code: OxMf2ErrorCode.InitializationNativeBindingUnavailable,
    message: 'ox-mf2 native binding is unavailable for this environment',
    cause: result.error
  })
}
