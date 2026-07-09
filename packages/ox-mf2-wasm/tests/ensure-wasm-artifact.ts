import { execFile } from 'node:child_process'
import { existsSync } from 'node:fs'
import { open, readFile, rm, unlink } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { promisify } from 'node:util'

const execFileAsync = promisify(execFile)
const packageDir = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const crateDir = resolve(packageDir, '../../crates/ox_mf2_wasm')
const distDir = resolve(packageDir, 'dist')
const lockPath = resolve(packageDir, '.wasm-build.lock')
export const wasmArtifactTimeoutMs = 5 * 60 * 1000
const waitIntervalMs = 250

let ensurePromise: Promise<void> | null = null

/**
 * Ensure wasm-pack artifacts exist before WASM runtime tests run.
 *
 * @returns Promise that resolves when `dist/ox_mf2_wasm.js` is available.
 */
export function ensureWasmArtifacts(): Promise<void> {
  ensurePromise ??= buildIfNeeded()
  return ensurePromise
}

async function buildIfNeeded(): Promise<void> {
  if (hasWasmArtifacts()) {
    return
  }

  const deadline = Date.now() + wasmArtifactTimeoutMs

  while (Date.now() < deadline) {
    if (hasWasmArtifacts()) {
      return
    }

    const acquired = await tryAcquireLock()
    if (acquired) {
      try {
        if (hasWasmArtifacts()) {
          return
        }

        await runWasmPackBuild()

        if (!hasWasmArtifacts()) {
          throw new Error('WASM artifacts are missing after wasm-pack build')
        }

        return
      } finally {
        await releaseLock()
      }
    }

    await sleep(waitIntervalMs)
  }

  throw new Error('Timed out waiting for WASM artifacts')
}

async function runWasmPackBuild(): Promise<void> {
  await rm(distDir, { recursive: true, force: true })

  try {
    await execFileAsync(
      'wasm-pack',
      [
        'build',
        crateDir,
        '--target',
        'web',
        '--no-pack',
        '--out-dir',
        distDir,
        '--out-name',
        'ox_mf2_wasm'
      ],
      {
        cwd: packageDir,
        maxBuffer: 10 * 1024 * 1024,
        timeout: wasmArtifactTimeoutMs,
        killSignal: 'SIGKILL'
      }
    )
  } catch (error) {
    throw new Error(formatExecError('wasm-pack build', error))
  }

  try {
    await unlink(resolve(distDir, '.gitignore'))
  } catch (error) {
    if (!isMissingFileError(error)) {
      throw error
    }
  }
}

async function tryAcquireLock(): Promise<boolean> {
  try {
    const handle = await open(lockPath, 'wx')
    await handle.writeFile(String(process.pid))
    await handle.close()
    return true
  } catch (error) {
    if (isAlreadyExistsError(error)) {
      if (await removeStaleLock()) {
        return tryAcquireLock()
      }
      return false
    }
    throw error
  }
}

async function removeStaleLock(): Promise<boolean> {
  let ownerPid = 0
  try {
    ownerPid = Number((await readFile(lockPath, 'utf8')).trim())
  } catch (error) {
    if (!isMissingFileError(error)) {
      await releaseLock()
      return true
    }
  }

  if (Number.isInteger(ownerPid) && ownerPid > 0 && isProcessAlive(ownerPid)) {
    return false
  }

  await releaseLock()
  return true
}

async function releaseLock(): Promise<void> {
  try {
    await unlink(lockPath)
  } catch (error) {
    if (!isMissingFileError(error)) {
      throw error
    }
  }
}

function hasWasmArtifacts(): boolean {
  return (
    existsSync(resolve(distDir, 'ox_mf2_wasm.js')) &&
    existsSync(resolve(distDir, 'ox_mf2_wasm_bg.wasm'))
  )
}

function formatExecError(command: string, error: unknown): string {
  if (!(error instanceof Error)) {
    return `${command} failed: ${String(error)}`
  }

  const execError = error as Error & { stdout?: string; stderr?: string }
  const details = [execError.stderr, execError.stdout].filter(Boolean).join('\n').trim()
  return details.length > 0
    ? `${command} failed:\n${details}`
    : `${command} failed: ${execError.message}`
}

function sleep(ms: number): Promise<void> {
  return new Promise(resolve => {
    setTimeout(resolve, ms)
  })
}

function isAlreadyExistsError(error: unknown): boolean {
  return error instanceof Error && 'code' in error && error.code === 'EEXIST'
}

function isMissingFileError(error: unknown): boolean {
  return error instanceof Error && 'code' in error && error.code === 'ENOENT'
}

function isProcessAlive(pid: number): boolean {
  try {
    process.kill(pid, 0)
    return true
  } catch (error) {
    return error instanceof Error && 'code' in error && error.code === 'EPERM'
  }
}
