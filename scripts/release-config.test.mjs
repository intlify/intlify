import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'

import { expect, test } from 'vite-plus/test'

import config, { prepareRelease } from '../bump.config.ts'
import { bumpCargoVersion } from './bump-cargo-version.mjs'
import { releaseCargoLockPackages, releaseCargoTomlFiles } from './lib/release-crates.mjs'

test('bumpp config owns the release commit, tag, push, and execute hook', () => {
  expect(config).toMatchObject({
    all: true,
    commit: 'release: v%s',
    push: true,
    tag: true
  })
  expect(config.execute).toBeTypeOf('function')
})

test('Cargo version propagation accepts the version selected by bumpp', async () => {
  const fixtureRoot = await mkdtemp(join(tmpdir(), 'intlify-bump-cargo-'))
  try {
    await writeCargoFixture(fixtureRoot)

    await bumpCargoVersion('1.2.3', { rootDir: fixtureRoot })

    for (const relativePath of releaseCargoTomlFiles) {
      const source = await readFile(join(fixtureRoot, relativePath), 'utf8')
      expect(source).toContain('version = "1.2.3"')
    }

    const lockfile = await readFile(join(fixtureRoot, 'Cargo.lock'), 'utf8')
    for (const packageName of releaseCargoLockPackages) {
      expect(lockfile).toContain(`name = "${packageName}"\nversion = "1.2.3"`)
    }

    const parserSource = await readFile(
      join(fixtureRoot, 'crates/ox_mf2_parser/src/lib.rs'),
      'utf8'
    )
    expect(parserSource).toContain('https://docs.rs/ox_mf2_parser/1.2.3')
  } finally {
    await rm(fixtureRoot, { recursive: true, force: true })
  }
})

test('release preparation awaits Cargo and changelog failures before bumpp commits', async () => {
  const calls = []
  const dependencies = {
    bumpCargoVersion: async version => {
      calls.push(['cargo', version])
    },
    updateChangelog: async options => {
      calls.push(['changelog', options])
    }
  }

  await prepareRelease('1.2.3', dependencies)
  expect(calls).toEqual([
    ['cargo', '1.2.3'],
    [
      'changelog',
      {
        repository: 'intlify/intlify',
        tagName: 'v1.2.3',
        source: 'generated-notes',
        targetCommitish: 'HEAD',
        output: 'CHANGELOG.md'
      }
    ]
  ])

  const changelog = async () => {
    throw new Error('generated notes failed')
  }
  await expect(
    prepareRelease('1.2.3', {
      bumpCargoVersion: dependencies.bumpCargoVersion,
      updateChangelog: changelog
    })
  ).rejects.toThrow('generated notes failed')

  const blockedCalls = []
  await expect(
    prepareRelease('1.2.3', {
      bumpCargoVersion: async () => {
        blockedCalls.push('cargo')
        throw new Error('cargo failed')
      },
      updateChangelog: async () => {
        blockedCalls.push('changelog')
      }
    })
  ).rejects.toThrow('cargo failed')
  expect(blockedCalls).toEqual(['cargo'])
})

async function writeCargoFixture(fixtureRoot) {
  for (const relativePath of releaseCargoTomlFiles) {
    const file = join(fixtureRoot, relativePath)
    await mkdir(dirname(file), { recursive: true })
    await writeFile(file, '[package]\nname = "fixture"\nversion = "0.1.0"\n')
  }

  const parserSource = join(fixtureRoot, 'crates/ox_mf2_parser/src/lib.rs')
  await mkdir(dirname(parserSource), { recursive: true })
  await writeFile(parserSource, '#![doc(html_root_url = "https://docs.rs/ox_mf2_parser/0.1.0")]\n')

  const lockfile = releaseCargoLockPackages
    .map(packageName => `[[package]]\nname = "${packageName}"\nversion = "0.1.0"\n`)
    .join('\n')
  await writeFile(join(fixtureRoot, 'Cargo.lock'), lockfile)
}
