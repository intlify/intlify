import { mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'

import { afterEach, describe, expect, test } from 'vite-plus/test'

import { emitArtifacts } from '../src/emitter.mjs'
import { transformJavaScript } from '../src/frontend.mjs'
import { runLocalizationProvider } from '../src/provider.mjs'

const temporaryDirectories = []

afterEach(async () => {
  await Promise.all(
    temporaryDirectories.splice(0).map(directory => rm(directory, { recursive: true, force: true }))
  )
})

const transform = (sourceText, options = {}) =>
  transformJavaScript({
    sourceText,
    relativePath: options.relativePath ?? 'demo/src/app.js',
    hashPayload: options.hashPayload
  })

describe('Intent Frontend', () => {
  test.each([
    ["heading.textContent = 'Welcome'", 'Welcome'],
    ['heading.textContent = "Welcome"', 'Welcome'],
    ['heading.textContent = `Welcome`', 'Welcome']
  ])('extracts static text from %s', (source, expectedText) => {
    const result = transform(source)

    expect(result.ok).toBe(true)
    expect(result.intents).toHaveLength(1)
    expect(result.intents[0]).toMatchObject({
      sourceLocale: 'en',
      sourceText: expectedText,
      surface: 'dom-text-content',
      origin: { path: 'demo/src/app.js' }
    })
    expect(result.intents[0].id).toMatch(/^m_[0-9a-f]{32}$/)
    expect(result.transformedSource).toContain(
      `heading.textContent = __intlify_message(${JSON.stringify(result.intents[0].id)})`
    )
  })

  test('uses cooked literal values', () => {
    const result = transform("heading.textContent = 'Hello\\nworld'")

    expect(result.ok).toBe(true)
    expect(result.intents[0].sourceText).toBe('Hello\nworld')
  })

  test('leaves clear operations and unrelated syntax unchanged', () => {
    const source = [
      "first.textContent = ''",
      "second.textContent = '   '",
      "third['textContent'] = 'computed'",
      "fourth.innerText = 'inner text'",
      "fifth.innerHTML = '<strong>Hello</strong>'",
      "const data = 'not UI text'"
    ].join('\n')
    const result = transform(source)

    expect(result).toMatchObject({
      ok: true,
      diagnostics: [],
      intents: [],
      runtimeAlias: undefined,
      transformedSource: source
    })
  })

  test('warns about dynamic text and preserves it', () => {
    const source = [
      'first.textContent = user.name',
      'second.textContent = `Hello ${user.name}`'
    ].join('\n')
    const result = transform(source)

    expect(result.ok).toBe(true)
    expect(result.intents).toEqual([])
    expect(result.transformedSource).toBe(source)
    expect(result.diagnostics).toEqual([
      expect.objectContaining({
        severity: 'warning',
        code: 'LC001',
        name: 'unsupported_dynamic_ui_text',
        line: 1,
        column: 21
      }),
      expect.objectContaining({
        severity: 'warning',
        code: 'LC001',
        name: 'unsupported_dynamic_ui_text',
        line: 2,
        column: 22
      })
    ])
  })

  test('rejects source imports', () => {
    const result = transform("import value from './value.js'\nheading.textContent = 'Welcome'")

    expect(result).toEqual({
      ok: false,
      diagnostics: [
        {
          severity: 'error',
          code: 'LC002',
          name: 'unsupported_module_import',
          message: 'The PoC entry must be a self-contained browser module.',
          path: 'demo/src/app.js',
          line: 1,
          column: 1
        }
      ]
    })
  })

  test('rejects JavaScript parse errors', () => {
    const result = transform('const =')

    expect(result.ok).toBe(false)
    expect(result.diagnostics[0]).toMatchObject({
      severity: 'error',
      code: 'LC005',
      name: 'invalid_source',
      path: 'demo/src/app.js',
      line: 1
    })
  })

  test('generates deterministic occurrence-aware Intent IDs', () => {
    const source = [
      "first.textContent = 'Same'",
      "second.textContent = 'Same'",
      "third.textContent = 'Different'"
    ].join('\n')
    const first = transform(source)
    const second = transform(source)

    expect(first.ok).toBe(true)
    expect(second.ok).toBe(true)
    expect(first.intents.map(intent => intent.id)).toEqual(second.intents.map(intent => intent.id))
    expect(new Set(first.intents.map(intent => intent.id)).size).toBe(3)
  })

  test('normalizes path separators before hashing and recording origins', () => {
    const windowsPath = transform("heading.textContent = 'Welcome'", {
      relativePath: 'demo\\src\\app.js'
    })
    const posixPath = transform("heading.textContent = 'Welcome'")

    expect(windowsPath.ok).toBe(true)
    expect(windowsPath.intents[0].origin.path).toBe('demo/src/app.js')
    expect(windowsPath.intents[0].id).toBe(posixPath.intents[0].id)
  })

  test('rejects different payloads that collide', () => {
    const result = transform("first.textContent = 'First'\nsecond.textContent = 'Second'", {
      hashPayload: () => '0'.repeat(32)
    })

    expect(result.ok).toBe(false)
    expect(result.diagnostics.at(-1)).toMatchObject({
      severity: 'error',
      code: 'LC006',
      name: 'intent_id_collision',
      line: 2
    })
  })

  test('chooses a source-wide non-conflicting runtime alias', () => {
    const source = [
      'const __intlify_message = "application value"',
      'function render() {',
      '  const __intlify_message_1 = "nested value"',
      "  heading.textContent = 'Welcome'",
      '}'
    ].join('\n')
    const result = transform(source)

    expect(result.ok).toBe(true)
    expect(result.runtimeAlias).toBe('__intlify_message_2')
    expect(result.transformedSource).toContain(
      'import { message as __intlify_message_2 } from "./intlify-runtime.mjs"'
    )
    expect(result.transformedSource).toContain(
      `heading.textContent = __intlify_message_2(${JSON.stringify(result.intents[0].id)})`
    )
  })

  test.each([
    ['LF', '\n'],
    ['CRLF', '\r\n']
  ])('preserves surrounding source and uses the first %s newline', (_name, newline) => {
    const source =
      `// before${newline}` +
      `heading.textContent = 'Welcome' // after${newline}` +
      '// final comment'
    const result = transform(source)
    const id = result.intents[0].id

    expect(result.ok).toBe(true)
    expect(result.transformedSource).toBe(
      `import { message as __intlify_message } from "./intlify-runtime.mjs"${newline}${newline}` +
        `// before${newline}` +
        `heading.textContent = __intlify_message(${JSON.stringify(id)}) // after${newline}` +
        '// final comment'
    )
  })

  test('uses LF for the import when the source has no newline', () => {
    const result = transform("heading.textContent = 'Welcome'")

    expect(result.ok).toBe(true)
    expect(result.transformedSource).toMatch(
      /^import \{ message as __intlify_message \} from "\.\/intlify-runtime\.mjs"\n\n/
    )
  })
})

describe('Localization Provider contract', () => {
  const intents = [
    {
      id: 'm_b',
      sourceLocale: 'en',
      sourceText: 'Second',
      surface: 'dom-text-content'
    },
    {
      id: 'm_a',
      sourceLocale: 'en',
      sourceText: 'First',
      surface: 'dom-text-content'
    }
  ]

  test('loads named exports and canonicalizes requests and candidates', async () => {
    const fixture = await createProvider(`
export const kind = 'fixture'
export const revision = 'fixture-v1'
export let received
export let returned
export async function localize(requests) {
  received = requests.map(request => ({ ...request }))
  returned = requests.map(request => ({
    intentId: request.intentId,
    locale: request.targetLocale,
    message: request.sourceText + ' ja',
    ignored: true
  })).reverse()
  return returned
}
`)

    const result = await runLocalizationProvider({
      providerPath: fixture.providerPath,
      cwd: fixture.cwd,
      intents
    })
    const providerModule = await import(pathToFileURL(fixture.providerPath).href)

    expect(result).toEqual({
      ok: true,
      provider: { kind: 'fixture', revision: 'fixture-v1' },
      requests: [
        {
          intentId: 'm_a',
          sourceLocale: 'en',
          targetLocale: 'ja',
          sourceText: 'First',
          surface: 'dom-text-content'
        },
        {
          intentId: 'm_b',
          sourceLocale: 'en',
          targetLocale: 'ja',
          sourceText: 'Second',
          surface: 'dom-text-content'
        }
      ],
      candidates: [
        { intentId: 'm_a', locale: 'ja', message: 'First ja' },
        { intentId: 'm_b', locale: 'ja', message: 'Second ja' }
      ]
    })
    expect(providerModule.received.map(request => request.intentId)).toEqual(['m_a', 'm_b'])
    expect(providerModule.returned.map(candidate => candidate.intentId)).toEqual(['m_b', 'm_a'])
    expect(providerModule.returned[0].ignored).toBe(true)
  })

  test('does not call localize for an empty request batch', async () => {
    const fixture = await createProvider(`
export const kind = 'fixture'
export const revision = 'fixture-v1'
export let callCount = 0
export async function localize() {
  callCount += 1
  return []
}
`)

    const result = await runLocalizationProvider({
      providerPath: fixture.providerPath,
      cwd: fixture.cwd,
      intents: []
    })
    const providerModule = await import(pathToFileURL(fixture.providerPath).href)

    expect(result).toEqual({
      ok: true,
      provider: { kind: 'fixture', revision: 'fixture-v1' },
      requests: [],
      candidates: []
    })
    expect(providerModule.callCount).toBe(0)
  })

  test('isolates Compiler expectations from Provider request mutation', async () => {
    const fixture = await createProvider(`
export const kind = 'fixture'
export const revision = 'fixture-v1'
export async function localize(requests) {
  const candidates = requests.map(request => ({
    intentId: request.intentId,
    locale: request.targetLocale,
    message: request.sourceText
  }))
  requests.reverse()
  requests[0].intentId = 'mutated'
  return candidates
}
`)

    const result = await runLocalizationProvider({
      providerPath: fixture.providerPath,
      cwd: fixture.cwd,
      intents
    })

    expect(result.ok).toBe(true)
    expect(result.requests.map(request => request.intentId)).toEqual(['m_a', 'm_b'])
  })

  test.each([
    ['missing kind', "export const revision = 'v1'; export function localize() {}"],
    [
      'empty kind',
      "export const kind = ' '; export const revision = 'v1'; export function localize() {}"
    ],
    ['missing revision', "export const kind = 'fixture'; export function localize() {}"],
    [
      'empty revision',
      "export const kind = 'fixture'; export const revision = ''; export function localize() {}"
    ],
    ['missing localize', "export const kind = 'fixture'; export const revision = 'v1'"]
  ])('rejects a Provider with %s', async (_name, source) => {
    const fixture = await createProvider(source)
    const result = await runLocalizationProvider({
      providerPath: fixture.providerPath,
      cwd: fixture.cwd,
      intents
    })

    expect(result).toEqual(providerDiagnostic('LC003', 'invalid_localization_provider'))
  })

  test('rejects a Provider module that cannot be loaded', async () => {
    const fixture = await createProvider('export const =')
    const result = await runLocalizationProvider({
      providerPath: fixture.providerPath,
      cwd: fixture.cwd,
      intents
    })

    expect(result).toEqual(
      providerDiagnostic(
        'LC003',
        'invalid_localization_provider',
        'The provider module could not be loaded.'
      )
    )
  })

  test.each([
    ['throws', "throw new Error('failed')"],
    ['rejects', "return Promise.reject(new Error('failed'))"]
  ])('reports LC007 when localize %s', async (_name, body) => {
    const fixture = await createProvider(
      validProvider(`export async function localize() { ${body} }`)
    )
    const result = await runLocalizationProvider({
      providerPath: fixture.providerPath,
      cwd: fixture.cwd,
      intents
    })

    expect(result).toEqual(
      providerDiagnostic(
        'LC007',
        'localization_provider_failed',
        'The provider failed while localizing messages.'
      )
    )
  })

  test.each([
    ['a non-array', 'return {}'],
    ['a missing candidate', 'return []'],
    [
      'a duplicate candidate',
      'return [candidate(requests[0]), candidate(requests[0]), candidate(requests[1])]'
    ],
    [
      'an unsolicited candidate',
      "return [...requests.map(candidate), { intentId: 'unknown', locale: 'ja', message: 'Unknown' }]"
    ],
    [
      'a mismatched locale',
      "return requests.map(request => ({ ...candidate(request), locale: 'fr' }))"
    ],
    [
      'an invalid field type',
      'return requests.map(request => ({ ...candidate(request), intentId: 1 }))'
    ],
    [
      'an empty message',
      "return requests.map(request => ({ ...candidate(request), message: '   ' }))"
    ]
  ])('rejects %s with LC004', async (_name, body) => {
    const fixture = await createProvider(
      validProvider(`
const candidate = request => ({
  intentId: request.intentId,
  locale: request.targetLocale,
  message: request.sourceText
})
export async function localize(requests) { ${body} }
`)
    )
    const result = await runLocalizationProvider({
      providerPath: fixture.providerPath,
      cwd: fixture.cwd,
      intents
    })

    expect(result).toEqual(providerDiagnostic('LC004', 'invalid_localization_result'))
  })

  test('preserves valid message bytes including whitespace, Unicode, and newlines', async () => {
    const message = '  日本語\nsecond line  '
    const fixture = await createProvider(
      validProvider(`
export async function localize(requests) {
  return requests.map(request => ({
    intentId: request.intentId,
    locale: request.targetLocale,
    message: ${JSON.stringify(message)},
    ignored: 'metadata'
  }))
}
`)
    )
    const result = await runLocalizationProvider({
      providerPath: fixture.providerPath,
      cwd: fixture.cwd,
      intents
    })

    expect(result.ok).toBe(true)
    expect(result.candidates.every(candidate => candidate.message === message)).toBe(true)
    expect(result.candidates.every(candidate => !Object.hasOwn(candidate, 'ignored'))).toBe(true)
  })

  test('accepts a target message equal to its source text', async () => {
    const fixture = await createProvider(
      validProvider(`
export async function localize(requests) {
  return requests.map(request => ({
    intentId: request.intentId,
    locale: request.targetLocale,
    message: request.sourceText
  }))
}
`)
    )
    const result = await runLocalizationProvider({
      providerPath: fixture.providerPath,
      cwd: fixture.cwd,
      intents
    })

    expect(result.ok).toBe(true)
    expect(result.candidates.map(candidate => candidate.message)).toEqual(['First', 'Second'])
  })
})

describe('Artifact emitter', () => {
  const intents = [
    {
      id: 'm_b',
      sourceLocale: 'en',
      sourceText: 'Quote " and slash \\ and\nnewline',
      surface: 'dom-text-content'
    },
    {
      id: 'm_a',
      sourceLocale: 'en',
      sourceText: 'Welcome',
      surface: 'dom-text-content'
    }
  ]
  const candidates = [
    { intentId: 'm_b', locale: 'ja', message: '  日本語\nsecond line  ' },
    { intentId: 'm_a', locale: 'ja', message: '<strong>Hello</strong>' }
  ]
  const provider = { kind: 'fixture', revision: 'fixture-v1' }

  test('emits five artifacts in canonical order', () => {
    const artifacts = emitArtifacts({
      transformedSource: 'const unchanged = true',
      intents,
      candidates,
      provider
    })

    expect([...artifacts.keys()]).toEqual([
      'app.js',
      'intlify-runtime.mjs',
      'locale-en.mjs',
      'locale-ja.mjs',
      'localization-report.json'
    ])
    expect(artifacts.get('app.js')).toBe('const unchanged = true')
  })

  test('sorts messages by Intent ID and safely serializes their exact values', async () => {
    const artifacts = emitArtifacts({
      transformedSource: '',
      intents,
      candidates,
      provider
    })
    const directory = await createTemporaryDirectory('locale-compiler-emitter-')
    await writeFile(join(directory, 'locale-en.mjs'), artifacts.get('locale-en.mjs'), 'utf8')
    await writeFile(join(directory, 'locale-ja.mjs'), artifacts.get('locale-ja.mjs'), 'utf8')

    const en = await import(pathToFileURL(join(directory, 'locale-en.mjs')).href)
    const ja = await import(pathToFileURL(join(directory, 'locale-ja.mjs')).href)

    expect(Object.keys(en.messages)).toEqual(['m_a', 'm_b'])
    expect(en.messages).toEqual({
      m_a: 'Welcome',
      m_b: 'Quote " and slash \\ and\nnewline'
    })
    expect(Object.keys(ja.messages)).toEqual(['m_a', 'm_b'])
    expect(ja.messages).toEqual({
      m_a: '<strong>Hello</strong>',
      m_b: '  日本語\nsecond line  '
    })
  })

  test('emits a runtime with default, target, and error behavior', async () => {
    const artifacts = emitArtifacts({ transformedSource: '', intents, candidates, provider })
    const directory = await createTemporaryDirectory('locale-compiler-runtime-')
    for (const filename of ['intlify-runtime.mjs', 'locale-en.mjs', 'locale-ja.mjs']) {
      await writeFile(join(directory, filename), artifacts.get(filename), 'utf8')
    }

    const runtime = await import(pathToFileURL(join(directory, 'intlify-runtime.mjs')).href)
    const hadLocale = Object.hasOwn(globalThis, '__INTLIFY_LOCALE__')
    const previousLocale = globalThis.__INTLIFY_LOCALE__

    try {
      delete globalThis.__INTLIFY_LOCALE__
      expect(runtime.message('m_a')).toBe('Welcome')

      globalThis.__INTLIFY_LOCALE__ = 'ja'
      expect(runtime.message('m_a')).toBe('<strong>Hello</strong>')

      globalThis.__INTLIFY_LOCALE__ = 'fr'
      expect(() => runtime.message('m_a')).toThrow('unsupported locale: fr')

      globalThis.__INTLIFY_LOCALE__ = 'en'
      expect(() => runtime.message('missing')).toThrow('missing localized message: en/missing')
    } finally {
      restoreGlobal('__INTLIFY_LOCALE__', hadLocale, previousLocale)
    }
  })

  test('emits the exact versioned report schema without nondeterministic metadata', () => {
    const artifacts = emitArtifacts({ transformedSource: '', intents, candidates, provider })
    const reportText = artifacts.get('localization-report.json')

    expect(reportText).toBe(
      `${JSON.stringify(
        {
          schemaVersion: 1,
          sourceLocale: 'en',
          targetLocales: ['ja'],
          validationLevel: 'poc-contract-only',
          intentCount: 2,
          messageCountByLocale: { en: 2, ja: 2 },
          provider: { kind: 'fixture', revision: 'fixture-v1' }
        },
        null,
        2
      )}\n`
    )
    expect(reportText).not.toContain('timestamp')
    expect(reportText).not.toContain('diagnostic')
    expect(reportText).not.toContain(process.cwd())
  })

  test('uses LF and one trailing LF for fully generated artifacts', () => {
    const artifacts = emitArtifacts({ transformedSource: '', intents, candidates, provider })

    for (const filename of [
      'intlify-runtime.mjs',
      'locale-en.mjs',
      'locale-ja.mjs',
      'localization-report.json'
    ]) {
      const text = artifacts.get(filename)
      expect(text).not.toContain('\r')
      expect(text.endsWith('\n')).toBe(true)
      expect(text.endsWith('\n\n')).toBe(false)
    }
  })

  test('emits deterministic bytes regardless of candidate input order', () => {
    const first = emitArtifacts({ transformedSource: '', intents, candidates, provider })
    const second = emitArtifacts({
      transformedSource: '',
      intents,
      candidates: [...candidates].reverse(),
      provider
    })

    expect([...first]).toEqual([...second])
  })

  test('emits all five no-op artifacts when there are no Intents', () => {
    const source = 'const untouched = true'
    const artifacts = emitArtifacts({
      transformedSource: source,
      intents: [],
      candidates: [],
      provider
    })

    expect([...artifacts.keys()]).toHaveLength(5)
    expect(artifacts.get('app.js')).toBe(source)
    expect(artifacts.get('locale-en.mjs')).toContain('export const messages = {}')
    expect(artifacts.get('locale-ja.mjs')).toContain('export const messages = {}')
    expect(JSON.parse(artifacts.get('localization-report.json'))).toMatchObject({
      intentCount: 0,
      messageCountByLocale: { en: 0, ja: 0 }
    })
  })
})

async function createProvider(source) {
  const cwd = await createTemporaryDirectory('locale-compiler-provider-')
  const providerPath = join(cwd, 'provider.mjs')
  await writeFile(providerPath, source, 'utf8')
  return { cwd, providerPath }
}

async function createTemporaryDirectory(prefix) {
  const directory = await mkdtemp(join(tmpdir(), prefix))
  temporaryDirectories.push(directory)
  return directory
}

function restoreGlobal(name, existed, value) {
  if (existed) {
    globalThis[name] = value
  } else {
    delete globalThis[name]
  }
}

function validProvider(body) {
  return `
export const kind = 'fixture'
export const revision = 'fixture-v1'
${body}
`
}

function providerDiagnostic(code, name, message) {
  const messages = {
    LC003: 'The provider must export kind, revision, and localize.',
    LC004:
      'The provider result must contain exactly one non-empty message for every requested intent and locale.'
  }
  return {
    ok: false,
    diagnostics: [
      {
        severity: 'error',
        code,
        name,
        message: message ?? messages[code],
        path: 'provider.mjs'
      }
    ]
  }
}
