import { spawnSync } from 'node:child_process'
import { existsSync, readdirSync, statSync } from 'node:fs'
import { readFile } from 'node:fs/promises'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

const rootDir = fileURLToPath(new URL('..', import.meta.url))
const args = process.argv.slice(2)
const command = args[0] ?? 'publish'
const dryRun = args.includes('--dry-run') || command === 'dry-run'
const npmDir = readOption('--npm-dir') ?? join(rootDir, 'release-dir', 'ox-mf2-napi')
const explicitTag = readOption('--tag')

if (!['publish', 'dry-run'].includes(command)) {
  throw new Error(`unsupported command: ${command}`)
}

const packageDirs = [
  ...collectGeneratedPackageDirs(npmDir),
  join(rootDir, 'packages', 'ox-mf2-wasm'),
  join(rootDir, 'packages', 'ox-mf2-napi')
]

for (const packageDir of packageDirs) {
  await publishPackage(packageDir)
}

async function publishPackage(packageDir) {
  const pkg = await readJson(join(packageDir, 'package.json'))
  const distTag = explicitTag ?? distTagForVersion(pkg.version)

  if (!dryRun && (await isPublished(pkg.name, pkg.version))) {
    console.log(`${pkg.name}@${pkg.version} is already published; skipping`)
    return
  }

  const publishArgs = ['publish', '--access', 'public', '--tag', distTag]
  if (dryRun) {
    publishArgs.push('--dry-run')
  }

  console.log(`${dryRun ? 'Dry-run publishing' : 'Publishing'} ${pkg.name}@${pkg.version}`)
  run('npm', publishArgs, { cwd: packageDir })
}

function collectGeneratedPackageDirs(directory) {
  if (!existsSync(directory)) {
    return []
  }
  return readdirSync(directory)
    .map(name => join(directory, name))
    .filter(path => statSync(path).isDirectory() && existsSync(join(path, 'package.json')))
    .sort((a, b) => a.localeCompare(b))
}

async function isPublished(packageName, version) {
  const result = spawnSync('npm', ['view', `${packageName}@${version}`, 'version'], {
    cwd: rootDir,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'ignore']
  })
  return result.status === 0 && result.stdout.trim() === version
}

function distTagForVersion(version) {
  return version.includes('-') ? 'next' : 'latest'
}

async function readJson(path) {
  return JSON.parse(await readFile(path, 'utf8'))
}

function readOption(name) {
  const index = args.indexOf(name)
  return index === -1 ? undefined : args[index + 1]
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
