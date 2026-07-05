#!/usr/bin/env node
// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

import { spawn } from 'node:child_process'
import { open, unlink } from 'node:fs/promises'
import { join } from 'node:path'
import { parseArgs } from 'node:util'
import { fileURLToPath } from 'node:url'

const workspaceRoot = fileURLToPath(new URL('..', import.meta.url))
const lockPath = join(workspaceRoot, '.wasm-pack-build.lock')
const waitTimeoutMs = 10 * 60 * 1000
const waitIntervalMs = 250

try {
  const options = parseOptions()
  await withWasmPackLock(() => runWasmPackBuild(options))
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

function runWasmPackBuild({ crate, outDir, outName }) {
  return new Promise((resolve, reject) => {
    const child = spawn(
      'wasm-pack',
      ['build', crate, '--target', 'web', '--no-pack', '--out-dir', outDir, '--out-name', outName],
      {
        stdio: 'inherit',
        shell: process.platform === 'win32'
      }
    )

    child.on('error', reject)
    child.on('exit', (code, signal) => {
      if (code === 0) {
        resolve()
        return
      }
      reject(new Error(`wasm-pack build failed with ${signal ?? `exit code ${code ?? 'unknown'}`}`))
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
