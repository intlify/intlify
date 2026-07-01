import { spawnSync } from 'node:child_process'
import { existsSync, readdirSync, statSync } from 'node:fs'
import { mkdtemp, readFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { parseArgs } from 'node:util'

const rootDir = fileURLToPath(new URL('..', import.meta.url))
const { dryRun, explicitTag, npmDir } = parseCliArgs(process.argv.slice(2))

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

  const publishTarget = await preparePublishTarget(packageDir, pkg)
  try {
    const publishArgs = ['publish', ...publishTarget.args, '--access', 'public', '--tag', distTag]
    if (dryRun) {
      publishArgs.push('--dry-run')
    }

    console.log(`${dryRun ? 'Dry-run publishing' : 'Publishing'} ${pkg.name}@${pkg.version}`)
    run('npm', publishArgs, { cwd: packageDir })
  } finally {
    await publishTarget.cleanup()
  }
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

async function preparePublishTarget(packageDir, pkg) {
  if (!requiresPnpmPackedTarball(pkg)) {
    return {
      args: [],
      cleanup: async () => {}
    }
  }

  const packDirectory = await mkdtemp(join(tmpdir(), 'intlify-npm-publish-'))
  try {
    run('pnpm', ['--dir', packageDir, 'pack', '--pack-destination', packDirectory], {
      cwd: rootDir
    })
    const tarballPath = join(packDirectory, packageTarballName(pkg))
    if (!existsSync(tarballPath)) {
      throw new Error(`pnpm pack did not create ${tarballPath}`)
    }

    return {
      args: [tarballPath],
      cleanup: async () => {
        await rm(packDirectory, { recursive: true, force: true })
      }
    }
  } catch (error) {
    await rm(packDirectory, { recursive: true, force: true })
    throw error
  }
}

function requiresPnpmPackedTarball(pkg) {
  return (
    Array.isArray(pkg.publishConfig?.executableFiles) &&
    pkg.publishConfig.executableFiles.length > 0
  )
}

function packageTarballName(pkg) {
  return `${pkg.name.replace(/^@/, '').replace('/', '-')}-${pkg.version}.tgz`
}

function parseCliArgs(args) {
  const { values, positionals } = parseArgs({
    args,
    options: {
      'dry-run': { type: 'boolean' },
      'npm-dir': { type: 'string' },
      tag: { type: 'string' }
    },
    allowPositionals: true
  })

  if (positionals.length > 1) {
    throw new Error(`expected at most one command, received: ${positionals.join(' ')}`)
  }

  const command = positionals[0] ?? 'publish'
  if (!['publish', 'dry-run'].includes(command)) {
    throw new Error(`unsupported command: ${command}`)
  }

  return {
    dryRun: Boolean(values['dry-run']) || command === 'dry-run',
    explicitTag: values.tag,
    npmDir: values['npm-dir'] ?? join(rootDir, 'release-dir', 'ox-mf2-napi')
  }
}

async function readJson(path) {
  return JSON.parse(await readFile(path, 'utf8'))
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
