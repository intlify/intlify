import { execFileSync } from 'node:child_process'
import { existsSync, readdirSync, rmSync } from 'node:fs'
import { resolve } from 'node:path'

const rootDir = resolve(import.meta.dirname, '..')
const repoRoot = resolve(rootDir, '../..')
const napiPackageDir = resolve(repoRoot, 'packages/ox-mf2-napi')
const wasmPackageDir = resolve(repoRoot, 'packages/ox-mf2-wasm')
const napiDistDir = resolve(napiPackageDir, 'dist')
const wasmDistDir = resolve(wasmPackageDir, 'dist')
const napiCrateManifest = resolve(repoRoot, 'crates/ox_mf2_napi/Cargo.toml')
const wasmCrateDir = resolve(repoRoot, 'crates/ox_mf2_wasm')

buildNapiIfMissing()
buildWasmIfMissing()

console.log('mf-parser-bench binding setup complete')

function buildNapiIfMissing() {
  if (hasNapiArtifacts()) {
    return
  }

  runCommand(
    'napi',
    [
      'build',
      '--manifest-path',
      napiCrateManifest,
      '--output-dir',
      'dist',
      '--platform',
      '--esm',
      '--no-dts-header',
      '--js',
      'native-binding.js',
      '--dts',
      'native-binding.d.ts'
    ],
    napiPackageDir
  )
  runCommand('vp', ['pack'], napiPackageDir)

  if (!hasNapiArtifacts()) {
    throw new Error('N-API binding artifacts are still missing after build')
  }
}

function buildWasmIfMissing() {
  if (hasWasmArtifacts()) {
    return
  }

  rmSync(wasmDistDir, { recursive: true, force: true })
  runCommand(
    'wasm-pack',
    [
      'build',
      wasmCrateDir,
      '--target',
      'web',
      '--no-pack',
      '--out-dir',
      wasmDistDir,
      '--out-name',
      'ox_mf2_wasm'
    ],
    wasmPackageDir
  )
  rmSync(resolve(wasmDistDir, '.gitignore'), { force: true })
  runCommand('vp', ['pack'], wasmPackageDir)

  if (!hasWasmArtifacts()) {
    throw new Error('WASM binding artifacts are still missing after build')
  }
}

function runCommand(command, args, cwd) {
  try {
    execFileSync(command, args, { cwd, stdio: 'inherit' })
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    throw new Error(`Command failed: ${command} ${args.join(' ')}\n${message}`)
  }
}

function hasNapiArtifacts() {
  if (!existsSync(resolve(napiDistDir, 'index.js'))) {
    return false
  }
  return readdirSync(napiDistDir).some(name => name.endsWith('.node'))
}

function hasWasmArtifacts() {
  return (
    existsSync(resolve(wasmDistDir, 'index.js')) &&
    existsSync(resolve(wasmDistDir, 'ox_mf2_wasm.js')) &&
    existsSync(resolve(wasmDistDir, 'ox_mf2_wasm_bg.wasm'))
  )
}
