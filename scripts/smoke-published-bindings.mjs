import { spawnSync } from 'node:child_process'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const tag = process.argv[2] ?? process.env.TAG ?? process.env.GITHUB_REF_NAME
if (!tag?.startsWith('v')) {
  throw new Error(`release tag must start with "v": ${String(tag)}`)
}

const version = tag.slice(1)
const packages = ['@intlify/ox-mf2-napi', '@intlify/ox-mf2-wasm']

for (const packageName of packages) {
  await waitForPackage(packageName, version)
}

const tempDir = await mkdtemp(join(tmpdir(), 'ox-mf2-published-smoke-'))

try {
  run('npm', ['init', '-y'], { cwd: tempDir })
  run(
    'npm',
    ['install', '--ignore-scripts', ...packages.map(packageName => `${packageName}@${version}`)],
    { cwd: tempDir }
  )

  run(
    'node',
    [
      '--input-type=module',
      '-e',
      `
        import { parseMessage } from '@intlify/ox-mf2-napi'
        const result = parseMessage('hello')
        if (result.snapshot.rootCount() < 1) throw new Error('N-API smoke failed')
        console.log('@intlify/ox-mf2-napi smoke ok')
      `
    ],
    { cwd: tempDir }
  )

  run(
    'node',
    [
      '--input-type=module',
      '-e',
      `
        import { init, parseMessage } from '@intlify/ox-mf2-wasm'
        await init()
        const result = parseMessage('hello')
        if (result.snapshot.rootCount() < 1) throw new Error('WASM smoke failed')
        console.log('@intlify/ox-mf2-wasm smoke ok')
      `
    ],
    { cwd: tempDir }
  )
} finally {
  await rm(tempDir, { force: true, recursive: true })
}

async function waitForPackage(packageName, packageVersion) {
  for (let attempt = 1; attempt <= 18; attempt++) {
    const result = spawnSync('npm', ['view', `${packageName}@${packageVersion}`, 'version'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore']
    })
    if (result.status === 0 && result.stdout.trim() === packageVersion) {
      console.log(`${packageName}@${packageVersion} is available on npm`)
      return
    }
    console.log(`Waiting for ${packageName}@${packageVersion} on npm (${attempt}/18)`)
    await new Promise(resolve => setTimeout(resolve, 10_000))
  }
  throw new Error(`${packageName}@${packageVersion} is not available on npm`)
}

function run(commandName, commandArgs, options) {
  const result = spawnSync(commandName, commandArgs, {
    ...options,
    stdio: 'inherit',
    shell: process.platform === 'win32'
  })
  if (result.status !== 0) {
    throw new Error(`${commandName} ${commandArgs.join(' ')} failed`)
  }
}
