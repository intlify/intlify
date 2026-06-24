import { execFile } from 'node:child_process'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { resolve } from 'node:path'
import { promisify } from 'node:util'

import { calibrateBindings } from './binding-calibration.mjs'
import { TARGETS, benchmarkCorporaForTarget } from '../targets.mjs'

const execFileAsync = promisify(execFile)

const rootDir = resolve(import.meta.dirname, '..')
const outputDir = resolve(rootDir, '.tmp')
const calibrationRunDir = resolve(outputDir, 'calibration-runs')
const rustBinary = resolve(rootDir, 'rs/target/release/mf-parser-bench-rs')
const candidates = [1, 10, 100, 1000, 5000, 10000, 25000, 50000, 100000]
const targetMs = Number(process.env.MF_PARSER_BENCH_CALIBRATE_MS ?? '200')

await mkdir(calibrationRunDir, { recursive: true })

const results = {}

for (const target of TARGETS) {
  for (const corpus of benchmarkCorporaForTarget(target)) {
    const key = `${target.name}__${corpus.name}`
    results[key] = await calibrate(target, corpus.name)
    console.log(
      `${target.name} / ${corpus.name}: iterations=${results[key].iterations}, elapsedMs=${results[key].elapsedMs.toFixed(2)}`
    )
  }
}

const bindings = await calibrateBindings({ rootDir, targetMs })

await writeFile(
  resolve(outputDir, 'calibration.json'),
  `${JSON.stringify({ targetMs, results, bindings }, null, 2)}\n`
)

async function calibrate(target, corpus) {
  let last = null
  for (const iterations of candidates) {
    const elapsedMs = await runTarget(target, corpus, iterations)
    last = { iterations, elapsedMs }
    if (elapsedMs >= targetMs) {
      return last
    }
  }
  return last
}

async function runTarget(target, corpus, iterations) {
  const summaryJson = resolve(calibrationRunDir, `${target.name}__${corpus}.json`)

  if (target.runtime === 'js') {
    await execFileAsync('node', [
      resolve(rootDir, 'js/run-parser.mjs'),
      '--target',
      target.name,
      '--corpus',
      corpus,
      '--iterations',
      String(iterations),
      '--summary-json',
      summaryJson
    ])
    return readElapsedMs(summaryJson)
  }

  await execFileAsync(rustBinary, [
    '--target',
    target.name,
    '--corpus',
    corpus,
    '--iterations',
    String(iterations),
    '--summary-json',
    summaryJson
  ])
  return readElapsedMs(summaryJson)
}

async function readElapsedMs(summaryJson) {
  const summary = JSON.parse(await readFile(summaryJson, 'utf8'))
  return summary.elapsedMs
}
