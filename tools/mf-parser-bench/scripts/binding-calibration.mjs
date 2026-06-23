import { execFile } from 'node:child_process'
import { existsSync, readdirSync } from 'node:fs'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { resolve } from 'node:path'
import { promisify } from 'node:util'

import { BINDING_OPERATIONS, BINDING_RUNTIMES } from '../binding-targets.mjs'

const execFileAsync = promisify(execFile)

/** Iteration candidates shared with parser benchmark calibration. */
export const BINDING_CALIBRATION_CANDIDATES = [1, 10, 100, 1000, 5000, 10000, 25000, 50000, 100000]

/**
 * Target wall-clock budget per calibration probe.
 *
 * @returns Calibration target in milliseconds.
 */
export function bindingCalibrationTargetMs() {
  return Number(process.env.MF_PARSER_BENCH_CALIBRATE_MS ?? '200')
}

/**
 * Build the calibration key for a binding runtime / operation pair.
 *
 * @param runtime - Binding runtime name.
 * @param operation - Binding operation name.
 * @returns Calibration map key.
 */
export function bindingCalibrationKey(runtime, operation) {
  return `${runtime}__${operation}`
}

/**
 * Calibrate binding benchmark iteration counts to match parser benchmarks.
 *
 * @param options - Calibration options.
 * @param options.rootDir - mf-parser-bench root directory.
 * @param options.targetMs - Target elapsed milliseconds per probe.
 * @returns Calibrated iteration counts keyed by runtime / operation.
 */
export async function calibrateBindings({ rootDir, targetMs = bindingCalibrationTargetMs() }) {
  const calibrationRunDir = resolve(rootDir, '.tmp/calibration-runs')
  await mkdir(calibrationRunDir, { recursive: true })

  const results = {}

  for (const runtime of BINDING_RUNTIMES) {
    if (!isRuntimeAvailable(rootDir, runtime.name)) {
      continue
    }

    for (const operation of BINDING_OPERATIONS) {
      const key = bindingCalibrationKey(runtime.name, operation.name)
      results[key] = await calibrateBinding({
        rootDir,
        calibrationRunDir,
        runtime: runtime.name,
        operation: operation.name,
        targetMs
      })
      console.log(
        `${runtime.name} / ${operation.name}: iterations=${results[key].iterations}, elapsedMs=${results[key].elapsedMs.toFixed(2)}`
      )
    }
  }

  return results
}

/**
 * Persist binding calibration results into `.tmp/calibration.json`.
 *
 * @param rootDir - mf-parser-bench root directory.
 * @param bindings - Binding calibration results.
 */
export async function writeBindingCalibration(rootDir, bindings) {
  const calibrationPath = resolve(rootDir, '.tmp/calibration.json')
  let calibration = {
    targetMs: bindingCalibrationTargetMs(),
    results: {},
    bindings: {}
  }

  try {
    calibration = JSON.parse(await readFile(calibrationPath, 'utf8'))
  } catch {
    // Keep defaults when parser calibration has not run yet.
  }

  calibration.bindings = bindings
  await mkdir(resolve(rootDir, '.tmp'), { recursive: true })
  await writeFile(calibrationPath, `${JSON.stringify(calibration, null, 2)}\n`)
}

/**
 * Read a calibrated binding iteration count.
 *
 * @param rootDir - mf-parser-bench root directory.
 * @param runtime - Binding runtime name.
 * @param operation - Binding operation name.
 * @returns Positive iteration count.
 */
export async function readBindingIterations(rootDir, runtime, operation) {
  const fallback = 1000

  try {
    const calibration = JSON.parse(
      await readFile(resolve(rootDir, '.tmp/calibration.json'), 'utf8')
    )
    const value = calibration.bindings?.[bindingCalibrationKey(runtime, operation)]?.iterations
    if (Number.isInteger(value) && value > 0) {
      return value
    }
  } catch {
    // Fall through to the parser-benchmark default.
  }

  return fallback
}

async function calibrateBinding({ rootDir, calibrationRunDir, runtime, operation, targetMs }) {
  let last = null

  for (const iterations of BINDING_CALIBRATION_CANDIDATES) {
    const elapsedMs = await runBinding({
      rootDir,
      calibrationRunDir,
      runtime,
      operation,
      iterations
    })
    last = { iterations, elapsedMs }
    if (elapsedMs >= targetMs) {
      return last
    }
  }

  return last
}

async function runBinding({ rootDir, calibrationRunDir, runtime, operation, iterations }) {
  const summaryJson = resolve(calibrationRunDir, `binding-${runtime}__${operation}.json`)
  await execFileAsync(
    'node',
    [
      resolve(rootDir, 'js/run-binding.mjs'),
      '--runtime',
      runtime,
      '--operation',
      operation,
      '--iterations',
      String(iterations),
      '--summary-json',
      summaryJson
    ],
    { cwd: rootDir }
  )

  const summary = JSON.parse(await readFile(summaryJson, 'utf8'))
  return summary.elapsedMs
}

function isRuntimeAvailable(rootDir, runtime) {
  const repoRoot = resolve(rootDir, '../..')

  if (runtime === 'napi') {
    const distDir = resolve(repoRoot, 'packages/ox-mf2-napi/dist')
    return (
      existsSync(resolve(distDir, 'index.js')) &&
      readdirSync(distDir).some(name => name.endsWith('.node'))
    )
  }

  if (runtime === 'wasm') {
    const distDir = resolve(repoRoot, 'packages/ox-mf2-wasm/dist')
    return (
      existsSync(resolve(distDir, 'index.js')) &&
      existsSync(resolve(distDir, 'ox_mf2_wasm.js')) &&
      existsSync(resolve(distDir, 'ox_mf2_wasm_bg.wasm'))
    )
  }

  return false
}
