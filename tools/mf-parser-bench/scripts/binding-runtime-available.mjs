import { existsSync } from 'node:fs'
import { readdirSync } from 'node:fs'
import { resolve } from 'node:path'

const runtime = process.argv[2]
const repoRoot = resolve(import.meta.dirname, '../../..')

if (runtime === 'napi') {
  const distDir = resolve(repoRoot, 'packages/ox-mf2-napi/dist')
  const available =
    existsSync(resolve(distDir, 'index.js')) &&
    existsSync(distDir) &&
    readdirSync(distDir).some(name => name.endsWith('.node'))
  process.exit(available ? 0 : 1)
}

if (runtime === 'wasm') {
  const distDir = resolve(repoRoot, 'packages/ox-mf2-wasm/dist')
  const available =
    existsSync(resolve(distDir, 'index.js')) &&
    existsSync(resolve(distDir, 'ox_mf2_wasm.js')) &&
    existsSync(resolve(distDir, 'ox_mf2_wasm_bg.wasm'))
  process.exit(available ? 0 : 1)
}

console.error(`Unknown binding runtime: ${runtime}`)
process.exit(1)
