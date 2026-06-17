import { execFile } from 'node:child_process'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { resolve } from 'node:path'
import { promisify } from 'node:util'

import { JS_TARGETS, RUST_TARGETS, TARGETS } from '../targets.mjs'
import { diagnosticsCount, runJsParserTarget } from '../js/parser-targets.mjs'

const execFileAsync = promisify(execFile)

const rootDir = resolve(import.meta.dirname, '..')
const casesDir = resolve(rootDir, 'cases')
const outputDir = resolve(rootDir, '.tmp')
const rustBinary = resolve(rootDir, 'rs/target/release/mf-parser-bench-rs')

await mkdir(outputDir, { recursive: true })

const reports = []

for (const target of JS_TARGETS) {
  for (const corpus of await corporaForFormat(target.format)) {
    reports.push(await runJsPreflight(target, corpus))
  }
}

for (const target of RUST_TARGETS) {
  for (const corpus of await corporaForFormat(target.format)) {
    reports.push(await runRustPreflight(target, corpus))
  }
}

const summary = summarize(reports)
const payload = { reports, summary }

await writeFile(resolve(outputDir, 'preflight.json'), `${JSON.stringify(payload, null, 2)}\n`)

console.log(
  `preflight complete: ${summary.parseOk} ok, ${summary.parseError} parse-error, ${summary.unsupported} unsupported, ${summary.unexpected} unexpected`
)

if (summary.unexpected > 0 && process.argv.includes('--strict')) {
  process.exitCode = 1
}

async function corporaForFormat(format) {
  const names = format === 'mf2' ? ['mf2-common', 'mf2-app', 'mf2-full'] : ['mf1-icu']
  return Promise.all(names.map(name => readCorpus(name)))
}

async function readCorpus(name) {
  return JSON.parse(await readFile(resolve(casesDir, `${name}.json`), 'utf8'))
}

async function runJsPreflight(target, corpus) {
  const results = corpus.cases.map(testCase => {
    if (testCase.unsupportedBy?.includes(target.name)) {
      return caseResult(target, corpus, testCase, 'unsupported', 0)
    }

    try {
      const value = runJsParserTarget(target.name, testCase.source)
      const diagnostics = diagnosticsCount(value)
      return caseResult(
        target,
        corpus,
        testCase,
        diagnostics === 0 ? 'parse-ok' : 'parse-error',
        diagnostics
      )
    } catch (error) {
      return caseResult(target, corpus, testCase, 'parse-error', 1, error)
    }
  })

  return {
    target: target.name,
    runtime: target.runtime,
    format: target.format,
    corpus: corpus.name,
    results
  }
}

async function runRustPreflight(target, corpus) {
  const { stdout } = await execFileAsync(rustBinary, [
    '--target',
    target.name,
    '--corpus',
    corpus.name,
    '--preflight'
  ])
  return JSON.parse(stdout)
}

function caseResult(target, corpus, testCase, status, diagnostics, error) {
  return {
    id: testCase.id,
    expected: testCase.expected,
    status,
    diagnostics,
    error: error ? String(error.message ?? error) : null,
    unexpected: isUnexpected(testCase.expected, status)
  }
}

function summarize(reports) {
  const summary = {
    targets: TARGETS.length,
    reports: reports.length,
    parseOk: 0,
    parseError: 0,
    unsupported: 0,
    unexpected: 0
  }

  for (const report of reports) {
    for (const result of report.results) {
      if (result.status === 'parse-ok') {
        summary.parseOk++
      } else if (result.status === 'parse-error') {
        summary.parseError++
      } else if (result.status === 'unsupported') {
        summary.unsupported++
      }
      if (result.unexpected ?? isUnexpected(result.expected, result.status)) {
        summary.unexpected++
      }
    }
  }

  return summary
}

function isUnexpected(expected, status) {
  if (status === 'unsupported') {
    return false
  }
  return expected !== status
}
