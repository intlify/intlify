import { EventEmitter } from 'node:events'
import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { expect, test } from 'vite-plus/test'

import {
  buildErrorEnvelope,
  detectLinuxLibc,
  NATIVE_PACKAGE_NAME,
  resolveNativeBinary,
  resolveNativeTarget,
  runWrapper,
  slashNormalizePath,
  usesJsonReporter
} from '../bin/intlify.mjs'

test('wrapper resolves a supported host native package', () => {
  const platform = process.platform
  const arch = process.arch
  const libc = platform === 'linux' ? 'glibc' : undefined
  const target = resolveNativeTarget({ platform, arch, libc })

  expect(target).toBeTruthy()

  const resolution = resolveNativeBinary({
    platform,
    arch,
    libc,
    resolvePackageJson: packageName => {
      expect(packageName).toBe(NATIVE_PACKAGE_NAME)
      return fixturePackageJsonPath(packageName)
    },
    existsSync: path => path.endsWith(target.binaryName)
  })

  expect(resolution).toMatchObject({
    ok: true,
    target
  })
  expect(slashNormalizePath(resolution.binaryPath)).toContain(
    `${NATIVE_PACKAGE_NAME.replace('/', '-')}/bin/${target.rustTarget}/${target.binaryName}`
  )
})

test('wrapper forwards args env stdio and native exit code', () => {
  const child = new EventEmitter()
  child.kill = () => true
  const env = { INTLIFY_WRAPPER_TEST: '1' }
  const calls = []
  let exitCode

  runWrapper({
    argv: ['fmt', '--reporter=json'],
    env,
    stdio: 'inherit',
    platform: 'darwin',
    arch: 'arm64',
    resolvePackageJson: packageName => fixturePackageJsonPath(packageName),
    existsSync: path => path.endsWith('intlify'),
    spawn(binaryPath, argv, options) {
      calls.push({ binaryPath, argv, options })
      return child
    },
    exit(code) {
      exitCode = code
    },
    forwardSignals: false
  })

  child.emit('exit', 7, null)

  expect(calls).toHaveLength(1)
  expect(calls[0].argv).toEqual(['fmt', '--reporter=json'])
  expect(calls[0].options).toEqual({ env, stdio: 'inherit' })
  expect(
    calls[0].binaryPath.endsWith(
      join('@intlify-cli-napi', 'bin', 'aarch64-apple-darwin', 'intlify')
    )
  ).toBe(true)
  expect(exitCode).toBe(7)
})

test('wrapper reports unsupported platform as JSON envelope when requested', () => {
  const output = captureWrapperRun({
    argv: ['--reporter', 'json', '--version'],
    platform: 'freebsd',
    arch: 'x64',
    cwd: '/tmp/intlify-wrapper'
  })

  expect(output.exitCode).toBe(2)
  expect(output.stderr).toBe('')
  expect(JSON.parse(output.stdout)).toEqual(
    buildErrorEnvelope(
      {
        kind: 'io',
        code: 'native_platform_unsupported',
        message: 'No intlify native CLI package is available for this platform.',
        details: {
          projectRootSource: 'cwd-fallback',
          platform: 'freebsd',
          arch: 'x64'
        }
      },
      { cwd: '/tmp/intlify-wrapper' }
    )
  )
})

test('wrapper reports non-json reporter failures as human-readable stderr', () => {
  const output = captureWrapperRun({
    argv: ['--reporter', 'xml'],
    platform: 'darwin',
    arch: 'arm64',
    resolvePackageJson() {
      throw new Error('missing package')
    }
  })

  expect(output.exitCode).toBe(2)
  expect(output.stdout).toBe('')
  expect(output.stderr).toContain('error: Native package @intlify/cli-napi')
})

test('wrapper does not implement help or version shortcuts', () => {
  const output = captureWrapperRun({
    argv: ['--version'],
    platform: 'freebsd',
    arch: 'x64'
  })

  expect(output.exitCode).toBe(2)
  expect(output.stdout).toBe('')
  expect(output.stderr).toContain('No intlify native CLI package is available')
  expect(output.stderr).not.toBe('0.14.0\n')
})

test('wrapper distinguishes missing package missing binary and spawn failures', () => {
  const missingPackage = resolveNativeBinary({
    platform: 'darwin',
    arch: 'arm64',
    resolvePackageJson() {
      throw new Error('missing package')
    }
  })
  expect(missingPackage.error.code).toBe('native_package_not_found')
  expect(missingPackage.error.details.packageName).toBe(NATIVE_PACKAGE_NAME)

  const missingBinary = resolveNativeBinary({
    platform: 'darwin',
    arch: 'arm64',
    resolvePackageJson: packageName => fixturePackageJsonPath(packageName),
    existsSync: () => false
  })
  expect(missingBinary.error.code).toBe('native_binary_not_found')
  expect(missingBinary.error.details.binaryPath).toContain('intlify')

  const output = captureWrapperRun({
    argv: ['--reporter=json'],
    platform: 'darwin',
    arch: 'arm64',
    resolvePackageJson: packageName => fixturePackageJsonPath(packageName),
    existsSync: () => true,
    spawn() {
      throw new Error('permission denied')
    }
  })
  const json = JSON.parse(output.stdout)
  expect(json.errors[0].code).toBe('native_binary_failed')
  expect(json.errors[0].details.cause).toBe('permission denied')
})

test('wrapper can resolve source-tree cli-napi package', () => {
  const resolution = resolveNativeBinary({
    platform: 'darwin',
    arch: 'arm64',
    existsSync: path => path.endsWith('package.json')
  })

  expect(resolution.error.code).toBe('native_binary_not_found')
  expect(resolution.error.details.packageName).toBe(NATIVE_PACKAGE_NAME)
  expect(resolution.error.details.binaryPath).toContain(
    'packages/cli-napi/bin/aarch64-apple-darwin/intlify'
  )
})

test('wrapper json reporter parser only recognizes json forms', () => {
  expect(usesJsonReporter(['--reporter', 'json'])).toBe(true)
  expect(usesJsonReporter(['fmt', '--reporter=json'])).toBe(true)
  expect(usesJsonReporter(['--reporter', 'xml'])).toBe(false)
  expect(usesJsonReporter(['--reporter'])).toBe(false)
})

test('linux libc detection uses runtime report instead of guessing without one', () => {
  expect(
    detectLinuxLibc({
      getReport: () => ({ header: { glibcVersionRuntime: '2.39' } })
    })
  ).toBe('glibc')
  expect(detectLinuxLibc({ getReport: () => ({ header: {} }) })).toBe('musl')
  expect(detectLinuxLibc({ getReport: () => undefined })).toBeUndefined()
})

function captureWrapperRun(options) {
  let stdout = ''
  let stderr = ''
  let exitCode
  runWrapper({
    cwd: '/tmp/intlify-wrapper',
    stdout: {
      write(chunk) {
        stdout += chunk
      }
    },
    stderr: {
      write(chunk) {
        stderr += chunk
      }
    },
    exit(code) {
      exitCode = code
    },
    spawn() {
      throw new Error('spawn should not be called')
    },
    forwardSignals: false,
    ...options
  })
  return { stdout, stderr, exitCode }
}

function fixturePackageJsonPath(packageName) {
  const directory = join(
    mkdtempSync(join(tmpdir(), 'intlify-wrapper-test-')),
    packageName.replace('/', '-')
  )
  mkdirSync(directory)
  writeFileSync(join(directory, 'package.json'), '{}')
  return join(directory, 'package.json')
}
