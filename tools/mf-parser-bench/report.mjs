import { readdir, readFile, writeFile } from 'node:fs/promises'
import { resolve } from 'node:path'

const rootDir = import.meta.dirname
const rawDir = resolve(rootDir, 'results/raw')
const normalizedDir = resolve(rootDir, 'results/normalized')
const reportDir = resolve(rootDir, 'results/reports')
const tmpDir = resolve(rootDir, '.tmp')

const rawResults = await readRawResults()
const normalized = await readNormalizedResults()
const environment = await readOptionalJson(resolve(tmpDir, 'environment.json'))
const preflight = await readOptionalJson(resolve(tmpDir, 'preflight.json'))
const calibration = await readOptionalJson(resolve(tmpDir, 'calibration.json'))

const report = renderReport({
  rawResults,
  normalized,
  environment,
  preflight,
  calibration
})

await writeFile(resolve(reportDir, 'latest.md'), report)
console.log(resolve(reportDir, 'latest.md'))

async function readRawResults() {
  const files = await readJsonFiles(rawDir)
  return files.flatMap(file =>
    file.data.results.map(result => ({
      suite: file.name.replace(/\.json$/, ''),
      command: result.command,
      mean: result.mean,
      stddev: result.stddev,
      min: result.min,
      max: result.max,
      median: result.median,
      user: result.user,
      system: result.system
    }))
  )
}

async function readNormalizedResults() {
  return readJsonFiles(normalizedDir)
}

async function readJsonFiles(dir) {
  let names
  try {
    names = await readdir(dir)
  } catch {
    return []
  }

  const files = []
  for (const name of names.filter(name => name.endsWith('.json')).sort()) {
    files.push({
      name,
      data: JSON.parse(await readFile(resolve(dir, name), 'utf8'))
    })
  }
  return files
}

async function readOptionalJson(path) {
  try {
    return JSON.parse(await readFile(path, 'utf8'))
  } catch {
    return null
  }
}

function renderReport({ rawResults, normalized, environment, preflight, calibration }) {
  const lines = [
    '# MF Parser Benchmark Report',
    '',
    `Generated at: ${new Date().toISOString()}`,
    '',
    '## Environment',
    '',
    environment
      ? `- Node: ${environment.node}
- rustc: ${environment.rustc ?? 'n/a'}
- cargo: ${environment.cargo ?? 'n/a'}
- hyperfine: ${environment.hyperfine ?? 'n/a'}
- OS: ${environment.os.platform} ${environment.os.release} ${environment.os.arch}
- CPU: ${environment.cpu?.model ?? 'n/a'} (${environment.cpu?.count ?? 'n/a'} cores)`
      : '- Environment metadata was not generated.',
    '',
    '## Hyperfine Results',
    '',
    table(
      ['suite', 'target', 'mean ms', 'stddev ms', 'relative in suite'],
      withRelative(rawResults).map(result => [
        result.suite,
        result.command,
        ms(result.mean),
        ms(result.stddev),
        `${result.relative.toFixed(2)}x`
      ])
    ),
    '',
    '## Workload Summaries',
    '',
    table(
      ['target', 'corpus', 'cases', 'iterations', 'total parses', 'input bytes', 'checksum'],
      normalized.map(file => [
        file.data.target,
        file.data.corpus,
        String(file.data.caseCount),
        String(file.data.iterations),
        String(file.data.totalParses),
        String(file.data.inputBytes),
        String(file.data.checksum)
      ])
    ),
    '',
    '## Preflight Summary',
    '',
    preflight
      ? `- parse-ok: ${preflight.summary.parseOk}
- parse-error: ${preflight.summary.parseError}
- unsupported: ${preflight.summary.unsupported}
- unexpected: ${preflight.summary.unexpected}`
      : '- Preflight was not generated.',
    '',
    '## Calibration',
    '',
    calibration
      ? table(
          ['target / corpus', 'iterations', 'elapsed ms'],
          Object.entries(calibration.results).map(([key, value]) => [
            key,
            String(value.iterations),
            value.elapsedMs.toFixed(2)
          ])
        )
      : '- Calibration was not generated.',
    ''
  ]

  return `${lines.join('\n')}\n`
}

function withRelative(results) {
  const minBySuite = new Map()
  for (const result of results) {
    const min = minBySuite.get(result.suite)
    if (min == null || result.mean < min) {
      minBySuite.set(result.suite, result.mean)
    }
  }
  return results.map(result => ({
    ...result,
    relative: result.mean / minBySuite.get(result.suite)
  }))
}

function table(headers, rows) {
  if (rows.length === 0) {
    return '_No data._'
  }
  const separator = headers.map(() => '---')
  return [headers, separator, ...rows].map(row => `| ${row.join(' | ')} |`).join('\n')
}

function ms(seconds) {
  return (seconds * 1000).toFixed(3)
}
