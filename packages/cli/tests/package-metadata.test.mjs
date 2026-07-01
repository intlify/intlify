import { readdir, readFile } from 'node:fs/promises'

import { expect, test } from 'vite-plus/test'

import { NATIVE_PACKAGE_NAME, NATIVE_TARGETS } from '../bin/intlify.mjs'

const packageVersion = '0.14.0'
const repositoryUrl = 'git+https://github.com/intlify/intlify.git'
const nativeTargetMatrix = [
  {
    platform: 'darwin',
    arch: 'x64',
    rustTarget: 'x86_64-apple-darwin',
    binaryName: 'intlify'
  },
  {
    platform: 'darwin',
    arch: 'arm64',
    rustTarget: 'aarch64-apple-darwin',
    binaryName: 'intlify'
  },
  {
    platform: 'linux',
    arch: 'x64',
    libc: 'glibc',
    rustTarget: 'x86_64-unknown-linux-gnu',
    binaryName: 'intlify'
  },
  {
    platform: 'linux',
    arch: 'arm64',
    libc: 'glibc',
    rustTarget: 'aarch64-unknown-linux-gnu',
    binaryName: 'intlify'
  },
  {
    platform: 'linux',
    arch: 'x64',
    libc: 'musl',
    rustTarget: 'x86_64-unknown-linux-musl',
    binaryName: 'intlify'
  },
  {
    platform: 'win32',
    arch: 'x64',
    rustTarget: 'x86_64-pc-windows-msvc',
    binaryName: 'intlify.exe'
  }
]

test('wrapper package metadata matches the public package contract', async () => {
  const pkg = await readJson('../package.json')

  expect(pkg).toMatchObject({
    name: '@intlify/cli',
    version: packageVersion,
    type: 'module',
    bin: {
      intlify: './bin/intlify.mjs'
    },
    files: ['bin', 'schema', 'README.md', 'package.json'],
    dependencies: {
      [NATIVE_PACKAGE_NAME]: 'workspace:*'
    },
    publishConfig: {
      access: 'public'
    },
    engines: {
      node: '>=22.12.0'
    },
    repository: {
      type: 'git',
      url: repositoryUrl,
      directory: 'packages/cli'
    }
  })
  expect(pkg.exports).toEqual({
    './package.json': './package.json',
    './schema/config.schema.json': './schema/config.schema.json'
  })
  expect(pkg.optionalDependencies).toBeUndefined()
  expect(pkg.dependencies[NATIVE_PACKAGE_NAME]).toBe('workspace:*')
  expect(pkg.keywords).toEqual(expect.arrayContaining(['intlify', 'messageformat', 'mf2', 'cli']))
})

test('cli-native package metadata matches the native package source contract', async () => {
  const pkg = await readJson('../../cli-native/package.json')

  expect(pkg).toMatchObject({
    name: NATIVE_PACKAGE_NAME,
    version: packageVersion,
    keywords: expect.arrayContaining(['intlify', 'messageformat', 'mf2', 'cli']),
    homepage: 'https://github.com/intlify/intlify#readme',
    bugs: {
      url: 'https://github.com/intlify/intlify/issues'
    },
    license: 'MIT',
    repository: {
      type: 'git',
      url: repositoryUrl,
      directory: 'packages/cli-native'
    },
    files: ['bin', 'README.md', 'package.json'],
    publishConfig: {
      access: 'public',
      executableFiles: nativeTargetMatrix.map(
        target => `./bin/${target.rustTarget}/${target.binaryName}`
      )
    }
  })
  expect(pkg.bin).toBeUndefined()
  expect(pkg.engines).toBeUndefined()
  expect(pkg.exports).toBeUndefined()
})

test('root package exposes CLI local validation entry points', async () => {
  const pkg = await readJson('../../../package.json')

  expect(pkg.scripts).toMatchObject({
    'build:cli': 'vp run cli#build',
    'schema:cli': 'vp run cli#schema',
    'schema:cli:check': 'vp run cli#schema:check',
    'check:cli-pack': 'vp run cli#pack:check',
    'test:cli-smoke': 'vp run cli#smoke',
    'bench:cli-startup': 'vp run cli#bench:startup'
  })
})

test('legacy per-target native package skeletons are not checked in', async () => {
  const packageEntries = await readdir(new URL('../../', import.meta.url), {
    withFileTypes: true
  })
  const legacyCliPackageDirectories = packageEntries
    .filter(entry => entry.isDirectory())
    .map(entry => entry.name)
    .filter(name => /^cli-(?:darwin|linux|win32)/.test(name))

  expect(legacyCliPackageDirectories).toEqual([])
})

test('wrapper platform table matches the CLI native build matrix', () => {
  expect(NATIVE_TARGETS).toEqual(nativeTargetMatrix)
})

async function readJson(relativePath) {
  return JSON.parse(await readFile(new URL(relativePath, import.meta.url), 'utf8'))
}
