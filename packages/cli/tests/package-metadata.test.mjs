import { readFile } from 'node:fs/promises'

import { expect, test } from 'vite-plus/test'

import { NATIVE_TARGETS } from '../bin/intlify.mjs'

const packageVersion = '0.14.0'
const repositoryUrl = 'git+https://github.com/intlify/intlify.git'
const nativePackageMetadata = [
  {
    directory: 'packages/cli-darwin-x64',
    name: '@intlify/cli-darwin-x64',
    packageDirectory: 'cli-darwin-x64',
    platform: 'darwin',
    arch: 'x64',
    os: ['darwin'],
    cpu: ['x64'],
    files: ['intlify', 'README.md', 'package.json']
  },
  {
    directory: 'packages/cli-darwin-arm64',
    name: '@intlify/cli-darwin-arm64',
    packageDirectory: 'cli-darwin-arm64',
    platform: 'darwin',
    arch: 'arm64',
    os: ['darwin'],
    cpu: ['arm64'],
    files: ['intlify', 'README.md', 'package.json']
  },
  {
    directory: 'packages/cli-linux-x64-gnu',
    name: '@intlify/cli-linux-x64-gnu',
    packageDirectory: 'cli-linux-x64-gnu',
    platform: 'linux',
    arch: 'x64',
    libc: 'glibc',
    os: ['linux'],
    cpu: ['x64'],
    packageLibc: ['glibc'],
    files: ['intlify', 'README.md', 'package.json']
  },
  {
    directory: 'packages/cli-linux-arm64-gnu',
    name: '@intlify/cli-linux-arm64-gnu',
    packageDirectory: 'cli-linux-arm64-gnu',
    platform: 'linux',
    arch: 'arm64',
    libc: 'glibc',
    os: ['linux'],
    cpu: ['arm64'],
    packageLibc: ['glibc'],
    files: ['intlify', 'README.md', 'package.json']
  },
  {
    directory: 'packages/cli-linux-x64-musl',
    name: '@intlify/cli-linux-x64-musl',
    packageDirectory: 'cli-linux-x64-musl',
    platform: 'linux',
    arch: 'x64',
    libc: 'musl',
    os: ['linux'],
    cpu: ['x64'],
    packageLibc: ['musl'],
    files: ['intlify', 'README.md', 'package.json']
  },
  {
    directory: 'packages/cli-win32-x64-msvc',
    name: '@intlify/cli-win32-x64-msvc',
    packageDirectory: 'cli-win32-x64-msvc',
    platform: 'win32',
    arch: 'x64',
    os: ['win32'],
    cpu: ['x64'],
    files: ['intlify.exe', 'README.md', 'package.json']
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
  expect(pkg.keywords).toEqual(expect.arrayContaining(['intlify', 'messageformat', 'mf2', 'cli']))

  for (const metadata of nativePackageMetadata) {
    expect(pkg.optionalDependencies[metadata.name]).toBe(packageVersion)
  }
  expect(Object.keys(pkg.optionalDependencies).sort()).toEqual(
    nativePackageMetadata.map(metadata => metadata.name).sort()
  )
})

test('native package metadata matches wrapper resolution table', async () => {
  expect(
    NATIVE_TARGETS.map(target => ({
      name: target.packageName,
      platform: target.platform,
      arch: target.arch,
      libc: target.libc,
      packageDirectory: target.packageDirectory,
      binaryName: target.binaryName
    }))
  ).toEqual(
    nativePackageMetadata.map(metadata => ({
      name: metadata.name,
      platform: metadata.platform,
      arch: metadata.arch,
      libc: metadata.libc,
      packageDirectory: metadata.packageDirectory,
      binaryName: metadata.files[0]
    }))
  )

  for (const metadata of nativePackageMetadata) {
    const pkg = await readJson(`../../../${metadata.directory}/package.json`)

    expect(pkg).toMatchObject({
      name: metadata.name,
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
        directory: metadata.directory
      },
      files: metadata.files,
      os: metadata.os,
      cpu: metadata.cpu,
      publishConfig: {
        access: 'public'
      }
    })
    expect(pkg.libc).toEqual(metadata.packageLibc)
    expect(pkg.bin).toBeUndefined()
    expect(pkg.engines).toBeUndefined()
    expect(pkg.exports).toBeUndefined()
  }
})

async function readJson(relativePath) {
  return JSON.parse(await readFile(new URL(relativePath, import.meta.url), 'utf8'))
}
