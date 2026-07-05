import { spawnSync } from 'node:child_process'
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

import { expect, test } from 'vite-plus/test'

const workspaceRoot = fileURLToPath(new URL('../../..', import.meta.url))
const checkReleaseVersionScript = join(workspaceRoot, 'scripts', 'check-release-version.mjs')

test('release version check accepts CLI release metadata', async () => {
  await withFixture(async fixtureRoot => {
    const result = runCheckReleaseVersion(fixtureRoot)

    expect(result.status).toBe(0)
  })
})

test('release version check falls back to TAG when positional tag is empty', async () => {
  await withFixture(async fixtureRoot => {
    const result = runCheckReleaseVersion(fixtureRoot, {
      args: [''],
      env: {
        TAG: 'v0.14.0'
      }
    })

    expect(result.status).toBe(0)
  })
})

test('release version check rejects mismatched package versions', async () => {
  await withFixture(async fixtureRoot => {
    await patchJson(join(fixtureRoot, 'packages', 'cli-native', 'package.json'), pkg => {
      pkg.version = '0.14.1'
    })

    const result = runCheckReleaseVersion(fixtureRoot)

    expect(result.status).not.toBe(0)
    expect(result.stderr).toContain('packages/cli-native/package.json version must be 0.14.0')
  })
})

test('release version check rejects CLI native dependency drift', async () => {
  await withFixture(async fixtureRoot => {
    await patchJson(join(fixtureRoot, 'packages', 'cli', 'package.json'), pkg => {
      pkg.dependencies['@intlify/cli-native'] = '0.14.0'
    })

    const result = runCheckReleaseVersion(fixtureRoot)

    expect(result.status).not.toBe(0)
    expect(result.stderr).toContain(
      'packages/cli/package.json dependencies.@intlify/cli-native must be workspace:*'
    )
  })
})

test('release version check rejects public intlify_cli crates.io publishing', async () => {
  await withFixture(async fixtureRoot => {
    const cargoTomlPath = join(fixtureRoot, 'crates', 'intlify_cli', 'Cargo.toml')
    const source = await readFile(cargoTomlPath, 'utf8')
    await writeFile(cargoTomlPath, source.replace('publish = false', 'publish = true'))

    const result = runCheckReleaseVersion(fixtureRoot)

    expect(result.status).not.toBe(0)
    expect(result.stderr).toContain('crates/intlify_cli/Cargo.toml must set publish = false')
  })
})

test('release version check rejects public formatter crates.io publishing', async () => {
  await withFixture(async fixtureRoot => {
    const cargoTomlPath = join(fixtureRoot, 'crates', 'intlify_format', 'Cargo.toml')
    const source = await readFile(cargoTomlPath, 'utf8')
    await writeFile(cargoTomlPath, source.replace('publish = false', 'publish = true'))

    const result = runCheckReleaseVersion(fixtureRoot)

    expect(result.status).not.toBe(0)
    expect(result.stderr).toContain('crates/intlify_format/Cargo.toml must set publish = false')
  })
})

async function withFixture(callback) {
  const fixtureRoot = await mkdtemp(join(tmpdir(), 'intlify-release-version-'))
  try {
    await writeFixture(fixtureRoot)
    await callback(fixtureRoot)
  } finally {
    await rm(fixtureRoot, { recursive: true, force: true })
  }
}

function runCheckReleaseVersion(fixtureRoot, options = {}) {
  return spawnSync(
    process.execPath,
    [checkReleaseVersionScript, ...(options.args ?? ['v0.14.0'])],
    {
      env: {
        ...process.env,
        INTLIFY_RELEASE_ROOT: fixtureRoot,
        ...options.env
      },
      encoding: 'utf8'
    }
  )
}

async function writeFixture(fixtureRoot) {
  await writeJson(join(fixtureRoot, 'package.json'), {
    name: 'intlify-monorepo',
    version: '0.14.0',
    private: true,
    scripts: {
      release:
        'bumpp "package.json" "packages/format-napi/package.json" "packages/format-wasm/package.json" "packages/cli/package.json" "packages/cli-native/package.json"'
    }
  })

  await writePackageJson(fixtureRoot, 'packages/ox-mf2-napi/package.json', {
    name: '@intlify/ox-mf2-napi',
    version: '0.14.0',
    repository: { directory: 'packages/ox-mf2-napi' }
  })
  await writePackageJson(fixtureRoot, 'packages/ox-mf2-wasm/package.json', {
    name: '@intlify/ox-mf2-wasm',
    version: '0.14.0',
    repository: { directory: 'packages/ox-mf2-wasm' }
  })
  await writeJson(join(fixtureRoot, 'packages/ox-mf2-shared/package.json'), {
    name: '@intlify/ox-mf2-shared',
    version: '0.14.0',
    private: true
  })
  await writePackageJson(fixtureRoot, 'packages/format-napi/package.json', {
    name: '@intlify/format-napi',
    version: '0.14.0',
    repository: { directory: 'packages/format-napi' }
  })
  await writePackageJson(fixtureRoot, 'packages/format-wasm/package.json', {
    name: '@intlify/format-wasm',
    version: '0.14.0',
    repository: { directory: 'packages/format-wasm' }
  })
  await writePackageJson(fixtureRoot, 'packages/cli/package.json', {
    name: '@intlify/cli',
    version: '0.14.0',
    repository: { directory: 'packages/cli' },
    dependencies: {
      '@intlify/cli-native': 'workspace:*'
    }
  })
  await writePackageJson(fixtureRoot, 'packages/cli-native/package.json', {
    name: '@intlify/cli-native',
    version: '0.14.0',
    repository: { directory: 'packages/cli-native' }
  })

  await writeCargoToml(fixtureRoot, 'crates/ox_mf2_parser/Cargo.toml', 'ox_mf2_parser')
  await writeCargoToml(fixtureRoot, 'crates/ox_mf2_napi/Cargo.toml', 'ox_mf2_napi')
  await writeCargoToml(fixtureRoot, 'crates/ox_mf2_wasm/Cargo.toml', 'ox_mf2_wasm')
  await writeCargoToml(fixtureRoot, 'crates/intlify_format/Cargo.toml', 'intlify_format', {
    publishFalse: true
  })
  await writeCargoToml(
    fixtureRoot,
    'crates/intlify_format_napi/Cargo.toml',
    'intlify_format_napi',
    {
      publishFalse: true
    }
  )
  await writeCargoToml(
    fixtureRoot,
    'crates/intlify_format_wasm/Cargo.toml',
    'intlify_format_wasm',
    {
      publishFalse: true
    }
  )
  await writeCargoToml(fixtureRoot, 'crates/intlify_cli/Cargo.toml', 'intlify_cli', {
    publishFalse: true
  })
  await writeText(
    join(fixtureRoot, 'crates/ox_mf2_parser/src/lib.rs'),
    '#![doc(html_root_url = "https://docs.rs/ox_mf2_parser/0.14.0")]\n'
  )
  await writeText(
    join(fixtureRoot, 'Cargo.lock'),
    `${cargoLockPackage('ox_mf2_parser')}${cargoLockPackage('ox_mf2_napi')}${cargoLockPackage('ox_mf2_wasm')}${cargoLockPackage('intlify_format')}${cargoLockPackage('intlify_format_napi')}${cargoLockPackage('intlify_format_wasm')}${cargoLockPackage('intlify_cli')}`
  )
}

async function writePackageJson(fixtureRoot, relativePath, overrides) {
  await writeJson(join(fixtureRoot, relativePath), {
    version: '0.14.0',
    publishConfig: {
      access: 'public'
    },
    homepage: 'https://github.com/intlify/intlify#readme',
    bugs: {
      url: 'https://github.com/intlify/intlify/issues'
    },
    ...overrides,
    repository: {
      type: 'git',
      url: 'git+https://github.com/intlify/intlify.git',
      ...overrides.repository
    }
  })
}

async function writeCargoToml(fixtureRoot, relativePath, name, options = {}) {
  await writeText(
    join(fixtureRoot, relativePath),
    `[package]\nname = "${name}"\nversion = "0.14.0"\n${options.publishFalse ? 'publish = false\n' : ''}`
  )
}

function cargoLockPackage(name) {
  return `[[package]]\nname = "${name}"\nversion = "0.14.0"\n\n`
}

async function patchJson(path, patch) {
  const value = JSON.parse(await readFile(path, 'utf8'))
  patch(value)
  await writeJson(path, value)
}

async function writeJson(path, value) {
  await mkdir(dirname(path), { recursive: true })
  await writeFile(path, `${JSON.stringify(value, null, 2)}\n`)
}

async function writeText(path, value) {
  await mkdir(dirname(path), { recursive: true })
  await writeFile(path, value)
}
