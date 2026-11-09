#!/usr/bin/env node
// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

import { spawn } from 'node:child_process'
import { mkdir, open, unlink } from 'node:fs/promises'
import { join } from 'node:path'
import { parseArgs } from 'node:util'
import { fileURLToPath } from 'node:url'

const workspaceRoot = fileURLToPath(new URL('..', import.meta.url))
const lockPath = join(workspaceRoot, '.wasm-pack-build.lock')
const wasmPackCachePath =
  process.env.WASM_PACK_CACHE ?? join(workspaceRoot, 'node_modules', '.cache', 'wasm-pack')
const waitTimeoutMs = 10 * 60 * 1000
const waitIntervalMs = 250
// wasm-pack downloads wasm-bindgen and binaryen from GitHub releases when its
// cache lacks them. A failed download is retried after a pause; any other
// failure is not.
const downloadFailure = 'failed to download'
const retryDelaysMs = [15_000, 45_000]

try {
  const options = parseOptions()
  await mkdir(wasmPackCachePath, { recursive: true })
  await withWasmPackLock(() => buildWithRetries(options))
} catch (error) {
  console.error(`error: ${error instanceof Error ? error.message : String(error)}`)
  process.exitCode = 1
}

function parseOptions() {
  const { values } = parseArgs({
    options: {
      crate: { type: 'string' },
      'out-dir': { type: 'string' },
      'out-name': { type: 'string' }
    },
    strict: true
  })

  const crate = values.crate
  const outDir = values['out-dir']
  const outName = values['out-name']

  if (!crate || !outDir || !outName) {
    throw new Error(
      'Usage: build-wasm-package.mjs --crate <path> --out-dir <path> --out-name <name>'
    )
  }

  return { crate, outDir, outName }
}

async function withWasmPackLock(callback) {
  await acquireLock()
  try {
    await callback()
  } finally {
    await releaseLock()
  }
}

async function acquireLock() {
  const deadline = Date.now() + waitTimeoutMs

  while (Date.now() < deadline) {
    try {
      const handle = await open(lockPath, 'wx')
      await handle.writeFile(`${process.pid}\n`)
      await handle.close()
      return
    } catch (error) {
      if (!isAlreadyExistsError(error)) {
        throw error
      }
      await sleep(waitIntervalMs)
    }
  }

  throw new Error('Timed out waiting for another wasm-pack build to finish')
}

async function releaseLock() {
  try {
    await unlink(lockPath)
  } catch (error) {
    if (!isMissingFileError(error)) {
      throw error
    }
  }
}

async function buildWithRetries(options) {
  for (let attempt = 1; ; attempt += 1) {
    const { code, signal, downloadFailed } = await runWasmPackBuild(options)
    if (code === 0) {
      return
    }
    const delay = retryDelaysMs[attempt - 1]
    if (!downloadFailed || delay === undefined) {
      throw new Error(`wasm-pack build failed with ${signal ?? `exit code ${code ?? 'unknown'}`}`)
    }
    console.error(
      `wasm-pack could not download a tool; retrying in ${delay / 1000}s (attempt ${attempt + 1} of ${retryDelaysMs.length + 1})`
    )
    await sleep(delay)
  }
}

function runWasmPackBuild({ crate, outDir, outName }) {
  return new Promise((resolve, reject) => {
    const child = spawn(
      'wasm-pack',
      ['build', crate, '--target', 'web', '--no-pack', '--out-dir', outDir, '--out-name', outName],
      {
        env: { ...process.env, WASM_PACK_CACHE: wasmPackCachePath },
        stdio: ['inherit', 'pipe', 'pipe'],
        shell: process.platform === 'win32'
      }
    )

    let downloadFailed = false
    const forward = (source, sink) => {
      // The tail keeps a marker that one chunk ends and the next begins visible.
      let tail = ''
      source.on('data', chunk => {
        sink.write(chunk)
        const text = tail + chunk.toString()
        downloadFailed ||= text.includes(downloadFailure)
        tail = text.slice(-(downloadFailure.length - 1))
      })
    }
    forward(child.stdout, process.stdout)
    forward(child.stderr, process.stderr)

    child.on('error', reject)
    child.on('close', (code, signal) => {
      resolve({ code, signal, downloadFailed })
    })
  })
}

function sleep(ms) {
  return new Promise(resolve => {
    setTimeout(resolve, ms)
  })
}

function isAlreadyExistsError(error) {
  return error instanceof Error && 'code' in error && error.code === 'EEXIST'
}

function isMissingFileError(error) {
  return error instanceof Error && 'code' in error && error.code === 'ENOENT'
}
