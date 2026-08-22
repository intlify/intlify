#!/usr/bin/env node
/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import { resolve } from 'node:path'
import { pathToFileURL } from 'node:url'
import { parseArgs } from 'node:util'

import { compile } from './compiler.mjs'

const USAGE = 'Usage: node src/cli.mjs --entry <app.js> --out <directory> --provider <provider.mjs>'

/**
 * Run the Locale Compiler CLI without terminating the current process.
 *
 * @param argv - Command-line arguments excluding the executable and script path.
 * @param io - Working directory and writable stdout/stderr streams.
 * @returns The process exit code for this invocation.
 */
export async function main(argv, io) {
  let values

  try {
    ;({ values } = parseArgs({
      args: argv,
      options: {
        entry: { type: 'string' },
        out: { type: 'string' },
        provider: { type: 'string' }
      },
      strict: true,
      allowPositionals: false
    }))
  } catch (error) {
    writeUsageError(io.stderr, error.message)
    return 2
  }

  const missingOptions = ['entry', 'out', 'provider'].filter(
    option => typeof values[option] !== 'string' || values[option].trim().length === 0
  )
  if (missingOptions.length > 0) {
    writeUsageError(io.stderr, `Missing required option(s): ${missingOptions.join(', ')}`)
    return 2
  }

  const result = await compile({
    entryPath: resolve(io.cwd, values.entry),
    outDir: resolve(io.cwd, values.out),
    providerPath: resolve(io.cwd, values.provider),
    cwd: io.cwd
  })

  for (const diagnostic of result.diagnostics) {
    io.stderr.write(`${formatDiagnostic(diagnostic)}\n`)
  }

  return result.ok ? 0 : 1
}

function formatDiagnostic(diagnostic) {
  const lines = [`${diagnostic.severity} ${diagnostic.code} ${diagnostic.name}`]

  if (diagnostic.path !== undefined) {
    const location =
      diagnostic.line === undefined || diagnostic.column === undefined
        ? diagnostic.path
        : `${diagnostic.path}:${diagnostic.line}:${diagnostic.column}`
    lines.push(location)
  }

  lines.push(diagnostic.message)
  return lines.join('\n')
}

function writeUsageError(stderr, reason) {
  stderr.write(`${reason}\n${USAGE}\n`)
}

function isMainModule() {
  return process.argv[1] !== undefined && pathToFileURL(process.argv[1]).href === import.meta.url
}

if (isMainModule()) {
  process.exitCode = await main(process.argv.slice(2), {
    cwd: process.cwd(),
    stdout: process.stdout,
    stderr: process.stderr
  })
}
