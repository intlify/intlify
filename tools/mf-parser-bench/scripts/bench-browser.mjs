import { spawn } from 'node:child_process'
import { createReadStream, existsSync } from 'node:fs'
import { stat } from 'node:fs/promises'
import { createServer } from 'node:http'
import { extname, resolve } from 'node:path'
import { BROWSER_BINDING_OPERATIONS } from '../binding-targets.mjs'
import { readBindingIterations } from './binding-calibration.mjs'

const benchDir = resolve(import.meta.dirname, '..')
const repoRoot = resolve(benchDir, '../..')
const fixturesDir = resolve(benchDir, 'fixtures')
const wasmDistDir = resolve(repoRoot, 'packages/ox-mf2-wasm/dist')
const wasmGluePath = resolve(wasmDistDir, 'ox_mf2_wasm.js')
const targets = BROWSER_BINDING_OPERATIONS.map(operation => operation.name)
const session = `ox-mf2-wasm-bench-${process.pid}`

if (!existsSync(wasmGluePath)) {
  throw new Error(
    'WASM artifact is missing. Run `vp run mf-parser-bench#bench:bindings:browser` or `node scripts/setup-bindings.mjs` first.'
  )
}

const server = createStaticServer({
  fixturesDir,
  wasmDistDir
})
await new Promise(resolveListen => server.listen(0, '127.0.0.1', resolveListen))

try {
  const address = server.address()
  if (!address || typeof address === 'string') {
    throw new Error('failed to allocate local browser benchmark port')
  }
  const url = `http://127.0.0.1:${address.port}/fixtures/browser-bench.html`
  await runPlaywright(['open', url, '--raw'])
  for (const target of targets) {
    const iterations = await resolveBrowserIterations(target)
    await runPlaywright([
      'eval',
      `() => window.__OX_MF2_BROWSER_BENCH__.ready.then(() => window.__OX_MF2_BROWSER_BENCH__.run('${target}', ${iterations}))`,
      '--raw'
    ])
  }
} finally {
  await runPlaywright(['close'], { allowFailure: true })
  await new Promise(resolveClose => server.close(resolveClose))
}

async function resolveBrowserIterations(operation) {
  if (process.env.OX_MF2_BROWSER_BENCH_ITERATIONS) {
    return Number(process.env.OX_MF2_BROWSER_BENCH_ITERATIONS)
  }

  return readBindingIterations(benchDir, 'wasm', operation)
}

function runPlaywright(args, options = {}) {
  const child = spawn(
    'npx',
    ['--package', '@playwright/cli', 'playwright-cli', `-s=${session}`, ...args],
    {
      cwd: benchDir,
      stdio: 'inherit',
      shell: process.platform === 'win32'
    }
  )
  return new Promise((resolve, reject) => {
    child.on('error', reject)
    child.on('exit', status => {
      if (!options.allowFailure && status !== 0) {
        reject(new Error(`playwright-cli exited with ${status ?? 1}`))
        return
      }
      resolve()
    })
  })
}

function createStaticServer({ fixturesDir, wasmDistDir }) {
  const mounts = [
    { prefix: '/fixtures', root: fixturesDir },
    { prefix: '/wasm', root: wasmDistDir }
  ]

  return createServer(async (request, response) => {
    const url = new URL(request.url ?? '/', 'http://127.0.0.1')
    const pathname = decodeURIComponent(url.pathname)

    if (pathname === '/' || pathname === '') {
      response.writeHead(302, { location: '/fixtures/browser-bench.html' })
      response.end()
      return
    }

    for (const mount of mounts) {
      if (pathname === mount.prefix || pathname.startsWith(`${mount.prefix}/`)) {
        const relativePath = pathname.slice(mount.prefix.length).replace(/^\//, '')
        await serveFile(resolve(mount.root, relativePath), mount.root, response)
        return
      }
    }

    response.writeHead(404)
    response.end('not found')
  })
}

async function serveFile(filePath, root, response) {
  if (!filePath.startsWith(root)) {
    response.writeHead(403)
    response.end('forbidden')
    return
  }

  try {
    const info = await stat(filePath)
    if (!info.isFile()) {
      response.writeHead(404)
      response.end('not found')
      return
    }
    response.writeHead(200, {
      'content-type': contentType(filePath)
    })
    createReadStream(filePath).pipe(response)
  } catch {
    response.writeHead(404)
    response.end('not found')
  }
}

function contentType(filePath) {
  switch (extname(filePath)) {
    case '.html':
      return 'text/html; charset=utf-8'
    case '.js':
      return 'text/javascript; charset=utf-8'
    case '.wasm':
      return 'application/wasm'
    default:
      return 'application/octet-stream'
  }
}
