import { readdirSync } from 'node:fs'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const workspaceDir = fileURLToPath(new URL('..', import.meta.url))
const packDir = await mkdtemp(join(tmpdir(), 'ox-mf2-pack-'))

try {
  const napiPackage = packPackage('packages/ox-mf2-napi')
  const wasmPackage = packPackage('packages/ox-mf2-wasm')
  const formatNapiPackage = packPackage('packages/format-napi')
  const formatWasmPackage = packPackage('packages/format-wasm')
  const napiContents = listPackage(napiPackage)
  const wasmContents = listPackage(wasmPackage)
  const formatNapiContents = listPackage(formatNapiPackage)
  const formatWasmContents = listPackage(formatWasmPackage)

  assertIncludes(
    napiContents,
    entry => entry.startsWith('package/dist/') && entry.endsWith('.node'),
    'N-API .node artifact'
  )
  assertIncludes(napiContents, entry => entry === 'package/dist/index.js', 'N-API JS entry')
  assertIncludes(napiContents, entry => entry === 'package/dist/index.d.ts', 'N-API types entry')
  assertIncludes(
    napiContents,
    entry => entry === 'package/dist/native-binding.js',
    'N-API native binding JS glue'
  )
  assertIncludes(
    napiContents,
    entry => entry === 'package/dist/native-binding.d.ts',
    'N-API native binding types'
  )
  assertIncludes(wasmContents, entry => entry === 'package/dist/index.js', 'WASM JS entry')
  assertIncludes(wasmContents, entry => entry === 'package/dist/index.d.ts', 'WASM types entry')
  assertIncludes(
    wasmContents,
    entry => entry === 'package/dist/ox_mf2_wasm_bg.wasm',
    'WASM binary artifact'
  )
  assertIncludes(
    wasmContents,
    entry => entry === 'package/dist/ox_mf2_wasm.js',
    'WASM JS glue artifact'
  )
  assertIncludes(
    formatNapiContents,
    entry => entry.startsWith('package/dist/') && entry.endsWith('.node'),
    'formatter N-API .node artifact'
  )
  assertIncludes(
    formatNapiContents,
    entry => entry === 'package/dist/index.js',
    'formatter N-API JS entry'
  )
  assertIncludes(
    formatNapiContents,
    entry => entry === 'package/dist/index.d.ts',
    'formatter N-API types entry'
  )
  assertIncludes(
    formatNapiContents,
    entry => entry === 'package/dist/native-binding.js',
    'formatter N-API native binding JS glue'
  )
  assertIncludes(
    formatNapiContents,
    entry => entry === 'package/dist/native-binding.d.ts',
    'formatter N-API native binding types'
  )
  assertIncludes(
    formatWasmContents,
    entry => entry === 'package/dist/index.js',
    'formatter WASM JS entry'
  )
  assertIncludes(
    formatWasmContents,
    entry => entry === 'package/dist/index.d.ts',
    'formatter WASM types entry'
  )
  assertIncludes(
    formatWasmContents,
    entry => entry === 'package/dist/intlify_format_wasm_bg.wasm',
    'formatter WASM binary artifact'
  )
  assertIncludes(
    formatWasmContents,
    entry => entry === 'package/dist/intlify_format_wasm.js',
    'formatter WASM JS glue artifact'
  )
} finally {
  await rm(packDir, { force: true, recursive: true })
}

function packPackage(packagePath) {
  const before = new Set(readdirSync(packDir))
  const result = spawnSync('pnpm', ['--dir', packagePath, 'pack', '--pack-destination', packDir], {
    cwd: workspaceDir,
    encoding: 'utf8',
    shell: process.platform === 'win32'
  })
  if (result.status !== 0) {
    throw new Error(result.stderr || result.stdout || `failed to pack ${packagePath}`)
  }
  const created = readdirSync(packDir).find(name => name.endsWith('.tgz') && !before.has(name))
  if (!created) {
    throw new Error(`failed to locate tarball path for ${packagePath}`)
  }
  return join(packDir, created)
}

function listPackage(packageFile) {
  const result = spawnSync('tar', ['-tzf', packageFile], {
    cwd: workspaceDir,
    encoding: 'utf8',
    shell: process.platform === 'win32'
  })
  if (result.status !== 0) {
    throw new Error(result.stderr || `failed to inspect ${packageFile}`)
  }
  return result.stdout.trim().split('\n')
}

function assertIncludes(entries, predicate, label) {
  if (!entries.some(predicate)) {
    throw new Error(`package is missing ${label}`)
  }
}
