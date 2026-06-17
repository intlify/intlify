import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { getTarget } from '../targets.mjs'
import { checksumValue, runJsParserTarget } from './parser-targets.mjs'

const rootDir = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const casesDir = resolve(rootDir, 'cases')

const args = parseArgs(process.argv.slice(2))
const target = getTarget(requiredArg(args, 'target'))

if (target.runtime !== 'js') {
  throw new Error(`Target is not a JS target: ${target.name}`)
}

const corpusName = requiredArg(args, 'corpus')
const iterations = Number(args.iterations ?? '1')
if (!Number.isInteger(iterations) || iterations < 1) {
  throw new Error(`--iterations must be a positive integer: ${args.iterations}`)
}

const corpus = await readCorpus(corpusName)
if (corpus.format !== target.format) {
  throw new Error(
    `Corpus ${corpus.name} has format ${corpus.format}, but ${target.name} expects ${target.format}`
  )
}

const cases = selectBenchmarkCases(corpus, target.name)
if (cases.length === 0) {
  throw new Error(`No benchmark cases for ${target.name} / ${corpus.name}`)
}

let checksum = 0
const started = process.hrtime.bigint()

for (let iteration = 0; iteration < iterations; iteration++) {
  for (const testCase of cases) {
    const value = runJsParserTarget(target.name, testCase.source)
    checksum = (checksum + checksumValue(value) + testCase.source.length) >>> 0
  }
}

const elapsedNs = Number(process.hrtime.bigint() - started)
const summary = {
  target: target.name,
  runtime: target.runtime,
  format: target.format,
  corpus: corpus.name,
  caseCount: cases.length,
  inputBytes: Buffer.byteLength(cases.map(testCase => testCase.source).join('')),
  iterations,
  totalParses: cases.length * iterations,
  checksum,
  elapsedMs: elapsedNs / 1_000_000
}

if (args['summary-json']) {
  const output = resolve(rootDir, args['summary-json'])
  await mkdir(dirname(output), { recursive: true })
  await writeFile(`${output}.tmp`, `${JSON.stringify(summary, null, 2)}\n`)
  await writeFile(output, `${JSON.stringify(summary, null, 2)}\n`)
}

console.log(`checksum=${checksum}`)

async function readCorpus(name) {
  const file = resolve(casesDir, `${name}.json`)
  return JSON.parse(await readFile(file, 'utf8'))
}

function selectBenchmarkCases(corpus, targetName) {
  return corpus.cases.filter(
    testCase => testCase.expected === 'parse-ok' && !testCase.unsupportedBy?.includes(targetName)
  )
}

function requiredArg(values, name) {
  const value = values[name]
  if (!value) {
    throw new Error(`Missing required option --${name}`)
  }
  return value
}

function parseArgs(values) {
  const args = {}
  for (let index = 0; index < values.length; index++) {
    const value = values[index]
    if (!value.startsWith('--')) {
      throw new Error(`Unexpected argument: ${value}`)
    }
    const key = value.slice(2)
    const next = values[index + 1]
    if (!next || next.startsWith('--')) {
      args[key] = 'true'
    } else {
      args[key] = next
      index++
    }
  }
  return args
}
