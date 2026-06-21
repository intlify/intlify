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

// Join hyperfine wall-clock with the normalized runner output so we can
// report µs/parse, which is the only fair comparison across targets that
// use different iteration counts.
//
// Key is the normalized file basename (`<target>__<corpus>[.<variant>]`)
// NOT just `<target>__<corpus>`: when the harness has been re-run across
// multiple feature branches the same `(target, corpus)` pair has one entry
// per variant (e.g. `…__mf2-app.json` plus `…__mf2-app.source-copy-review.json`),
// and indexing by `<target>__<corpus>` alone would silently let one variant
// overwrite another in the Map — picking up an iteration count that does
// not match the hyperfine row and producing wildly wrong µs/parse values.
const normalizedByTarget = new Map(
  normalized.map(file => [file.name.replace(/\.json$/, ''), file.data])
)

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
    'Hyperfine wall-clock per invocation. Each invocation runs',
    '`calibration.iterations × corpus.cases` parses, so the `mean ms`',
    'column is NOT directly comparable across targets — use the',
    '`µs/parse` column instead (computed as `mean / total_parses`).',
    '',
    table(
      ['suite', 'target', 'mean ms', 'stddev ms', 'µs/parse', 'relative (µs/parse)'],
      withRelativePerParse(rawResults).map(result => [
        result.suite,
        result.command,
        ms(result.mean),
        ms(result.stddev),
        result.perParseUs == null ? '?' : result.perParseUs.toFixed(3),
        result.relative == null ? '?' : `${result.relative.toFixed(2)}x`
      ])
    ),
    '',
    '## Per-Parse Comparison',
    '',
    'Sorted by suite then by µs/parse, with relative cost vs the fastest',
    'parser in the suite.',
    '',
    perParseRanking(rawResults),
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

function corpusForSuite(suite) {
  // Suites are named `<corpus>-<runtime>` (e.g. mf2-common-rust, mf1-icu-js)
  // optionally followed by `.<variant>` for side-by-side branch experiments
  // (e.g. `mf2-app-rust.feature-phase-1`). Strip the variant first, then
  // drop the trailing runtime token to get the corpus name.
  const base = suite.split('.', 1)[0]
  const parts = base.split('-')
  if (parts.length <= 1) {
    return base
  }
  parts.pop()
  return parts.join('-')
}

function variantForSuite(suite) {
  // Returns the leading `.` so callers can append the suffix directly to
  // a normalized-file key (`<target>__<corpus>` + variant). Empty string
  // for plain suites such as `mf2-common-rust`.
  const dot = suite.indexOf('.')
  return dot === -1 ? '' : suite.slice(dot)
}

function perParseUs(result) {
  // Key must include the suite variant so a `(target, corpus)` pair with
  // multiple normalized files (one per branch experiment) maps to the
  // iteration count that actually produced this hyperfine row.
  const corpus = corpusForSuite(result.suite)
  const variant = variantForSuite(result.suite)
  const data = normalizedByTarget.get(`${result.command}__${corpus}${variant}`)
  if (!data || !data.totalParses) {
    return null
  }
  return (result.mean * 1000 * 1000) / data.totalParses
}

function withRelativePerParse(results) {
  const enriched = results.map(result => ({ ...result, perParseUs: perParseUs(result) }))
  const minBySuite = new Map()
  for (const r of enriched) {
    if (r.perParseUs == null) {
      continue
    }
    const m = minBySuite.get(r.suite)
    if (m == null || r.perParseUs < m) {
      minBySuite.set(r.suite, r.perParseUs)
    }
  }
  return enriched.map(r => ({
    ...r,
    relative: r.perParseUs == null ? null : r.perParseUs / minBySuite.get(r.suite)
  }))
}

function perParseRanking(results) {
  const enriched = withRelativePerParse(results).filter(r => r.perParseUs != null)
  const groups = new Map()
  for (const r of enriched) {
    if (!groups.has(r.suite)) {
      groups.set(r.suite, [])
    }
    groups.get(r.suite).push(r)
  }
  const out = []
  for (const [suite, items] of [...groups.entries()].sort()) {
    items.sort((a, b) => a.perParseUs - b.perParseUs)
    out.push(`### ${suite}`, '')
    out.push(
      table(
        ['rank', 'target', 'µs/parse', 'vs fastest'],
        items.map((item, index) => [
          String(index + 1),
          item.command,
          item.perParseUs.toFixed(3),
          `${item.relative.toFixed(2)}x`
        ])
      )
    )
    out.push('')
  }
  return out.join('\n')
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
