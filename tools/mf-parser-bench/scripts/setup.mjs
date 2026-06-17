import { execFileSync } from 'node:child_process'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { resolve } from 'node:path'
import { cpus, machine, platform, release } from 'node:os'

const rootDir = resolve(import.meta.dirname, '..')
const repoRoot = resolve(rootDir, '../..')
const resultsDir = resolve(rootDir, 'results')
const tmpDir = resolve(rootDir, '.tmp')

await Promise.all([
  mkdir(resolve(resultsDir, 'raw'), { recursive: true }),
  mkdir(resolve(resultsDir, 'normalized'), { recursive: true }),
  mkdir(resolve(resultsDir, 'reports'), { recursive: true }),
  mkdir(tmpDir, { recursive: true })
])

checkSubmodules()
buildRustRunner()
await writeEnvironmentMetadata()

console.log('mf-parser-bench setup complete')

function checkSubmodules() {
  const required = [
    'refers/messageformat/mf2/messageformat/package.json',
    'refers/formatjs/packages/icu-messageformat-parser/index.ts',
    'refers/ox-content/crates/ox_content_i18n/Cargo.toml',
    'refers/mf2-tools/parser/Cargo.toml'
  ]

  for (const file of required) {
    try {
      execFileSync('test', ['-f', resolve(repoRoot, file)])
    } catch {
      throw new Error(`Missing ${file}. Run: git submodule update --init --depth 1 --recursive`)
    }
  }
}

function buildRustRunner() {
  execFileSync(
    'cargo',
    ['build', '--release', '--manifest-path', resolve(rootDir, 'rs/Cargo.toml')],
    { stdio: 'inherit' }
  )
}

async function writeEnvironmentMetadata() {
  const metadata = {
    generatedAt: new Date().toISOString(),
    os: {
      platform: platform(),
      release: release(),
      arch: process.arch,
      machine: machine()
    },
    cpu: cpus()[0]
      ? {
          model: cpus()[0].model,
          speed: cpus()[0].speed,
          count: cpus().length
        }
      : null,
    node: process.version,
    rustc: commandVersion('rustc', ['--version']),
    cargo: commandVersion('cargo', ['--version']),
    hyperfine: commandVersion('hyperfine', ['--version']),
    submodules: submoduleStatus(),
    npmPackages: await npmPackageVersions()
  }

  await writeFile(resolve(tmpDir, 'environment.json'), `${JSON.stringify(metadata, null, 2)}\n`)
}

function commandVersion(command, args) {
  try {
    return execFileSync(command, args, {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore']
    }).trim()
  } catch {
    return null
  }
}

function submoduleStatus() {
  try {
    return execFileSync('git', ['submodule', 'status', '--recursive'], {
      cwd: repoRoot,
      encoding: 'utf8'
    })
      .trim()
      .split('\n')
      .filter(Boolean)
  } catch {
    return []
  }
}

async function npmPackageVersions() {
  const packages = {}
  for (const packageName of ['messageformat', '@formatjs/icu-messageformat-parser']) {
    try {
      const packageJsonPath = resolve(rootDir, 'node_modules', packageName, 'package.json')
      const packageJson = JSON.parse(await readFile(packageJsonPath, 'utf8'))
      packages[packageName] = packageJson.version
    } catch {
      packages[packageName] = null
    }
  }
  return packages
}
