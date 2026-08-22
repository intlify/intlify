import { mkdir, mkdtemp, readFile, rm, symlink, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'

import { afterEach, describe, expect, test } from 'vite-plus/test'

import { main } from '../src/cli.mjs'
import { compile } from '../src/compiler.mjs'
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
    expect(result.intents[0].id).toMatch(/^m_[\w-]{16}$/)
    expect(result.intents[0].parameters).toEqual([])
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

  test('rewrites an explicit parameterized Intent alongside automatic extraction', () => {
    const newline = '\r\n'
    const source = [
      '// marker comment',
      "import { intent } from '@intlify/locale'",
      '',
      'const name = getName()',
      'heading.textContent = intent(',
      "  'Hello, {$name}!',",
      '  /* evaluated once */ { name }',
      ')',
      "button.textContent = 'Pay now'"
    ].join(newline)
    const result = transform(source)

    expect(result.ok).toBe(true)
    expect(result.diagnostics).toEqual([])
    expect(result.intents).toHaveLength(2)
    expect(result.intents[0]).toMatchObject({
      sourceText: 'Hello, {$name}!',
      surface: 'explicit-intent',
      parameters: [{ name: 'name', type: 'string-or-number' }]
    })
    expect(result.intents[1]).toMatchObject({
      sourceText: 'Pay now',
      surface: 'dom-text-content',
      parameters: []
    })
    expect(result.intents.every(intent => /^m_[\w-]{16}$/.test(intent.id))).toBe(true)
    expect(result.transformedSource).toContain(
      `import { message as __intlify_message } from "./intlify-runtime.mjs"${newline}`
    )
    expect(result.transformedSource).toContain(
      `heading.textContent = __intlify_message(${newline}` +
        `  ${JSON.stringify(result.intents[0].id)},${newline}` +
        `  /* evaluated once */ { name }${newline}` +
        ')'
    )
    expect(result.transformedSource).toContain(
      `button.textContent = __intlify_message(${JSON.stringify(result.intents[1].id)})`
    )
  })

  test('removes an unused marker import without adding the runtime', () => {
    const source = [
      '// before',
      "import { intent } from '@intlify/locale'",
      'const untouched = true'
    ].join('\n')
    const result = transform(source)

    expect(result).toMatchObject({
      ok: true,
      diagnostics: [],
      intents: [],
      runtimeAlias: undefined,
      transformedSource: '// before\n\nconst untouched = true'
    })
  })

  test('preserves a marker semicolon that separates a same-line statement', () => {
    const result = transform(
      "import { intent } from '@intlify/locale';const value = intent('Welcome')"
    )

    expect(result.ok).toBe(true)
    expect(result.transformedSource).toBe(
      `import { message as __intlify_message } from "./intlify-runtime.mjs";` +
        `const value = __intlify_message(${JSON.stringify(result.intents[0].id)})`
    )
  })

  test.each([
    ['single-quoted string', "intent('Welcome')"],
    ['double-quoted string', 'intent("Welcome")'],
    ['no-substitution template', 'intent(`Welcome`)']
  ])('accepts an explicit source as a %s', (_name, declaration) => {
    const result = transform(
      ["import { intent } from '@intlify/locale'", `const value = ${declaration}`].join('\n')
    )

    expect(result.ok).toBe(true)
    expect(result.intents[0]).toMatchObject({
      sourceText: 'Welcome',
      surface: 'explicit-intent',
      parameters: []
    })
  })

  test('accepts cooked explicit messages and normalizes parameters by placeholder order', () => {
    const result = transform(
      [
        "import { intent } from '@intlify/locale'",
        "const value = intent(' {$first}\\n{$last} {$first} ', { last, first })"
      ].join('\n')
    )

    expect(result.ok).toBe(true)
    expect(result.intents[0]).toMatchObject({
      sourceText: ' {$first}\n{$last} {$first} ',
      parameters: [
        { name: 'first', type: 'string-or-number' },
        { name: 'last', type: 'string-or-number' }
      ]
    })
  })

  test.each([
    ['no arguments', 'intent()'],
    ['too many arguments', "intent('Welcome', {}, 'extra')"],
    ['a dynamic source', 'intent(message)'],
    ['a substitution template', 'intent(`Hello, ${name}!`, { name })'],
    ['an empty source', "intent('')"],
    ['a whitespace-only source', "intent('   ')"],
    ['a malformed placeholder', "intent('Hello, {$name!', { name })"],
    ['whitespace inside a placeholder', "intent('Hello, {$ name}!', { name })"],
    ['whitespace before the dollar', "intent('Hello, { $name}!', { name })"],
    ['an invalid placeholder identifier', "intent('Hello, {$user-name}!', { user_name })"],
    ['a forbidden placeholder identifier', "intent('Hello, {$__proto__}!', { value })"],
    ['missing parameters', "intent('Hello, {$name}!')"],
    ['unneeded parameters', "intent('Welcome', {})"],
    ['a non-literal parameters object', "intent('Hello, {$name}!', parameters)"],
    ['a quoted property', "intent('Hello, {$name}!', { 'name': name })"],
    ['a computed property', "intent('Hello, {$name}!', { ['name']: name })"],
    ['a spread property', "intent('Hello, {$name}!', { ...parameters })"],
    ['a method property', "intent('Hello, {$name}!', { name() {} })"],
    ['an accessor property', "intent('Hello, {$name}!', { get name() { return 'Ada' } })"],
    ['a duplicate property', "intent('Hello, {$name}!', { name, name: fallback })"],
    ['a forbidden property', "intent('Hello, {$name}!', { __proto__: name })"],
    ['a missing property', "intent('{$first} {$last}', { first })"],
    ['an extra property', "intent('Hello, {$name}!', { name, unused })"],
    ['a nested call', "intent('Outer: {$inner}', { inner: intent('Inner') })"]
  ])('reports one LC010 for %s', (_name, declaration) => {
    const result = transform(
      ["import { intent } from '@intlify/locale'", `const value = ${declaration}`].join('\n')
    )

    expect(result.ok).toBe(false)
    expect(result.diagnostics.filter(diagnostic => diagnostic.code === 'LC010')).toHaveLength(1)
    expect(result.diagnostics.at(-1)).toMatchObject({
      severity: 'error',
      code: 'LC010',
      name: 'invalid_intent_declaration'
    })
  })

  test('allows ordinary braces and repeated placeholders', () => {
    const result = transform(
      [
        "import { intent } from '@intlify/locale'",
        "const first = intent('Use { and } carefully')",
        "const second = intent('{$name}, {$name}!', { name })"
      ].join('\n')
    )

    expect(result.ok).toBe(true)
    expect(result.intents.map(intent => intent.parameters)).toEqual([
      [],
      [{ name: 'name', type: 'string-or-number' }]
    ])
  })

  test('accepts adjacent placeholders and normal property assignments', () => {
    const result = transform(
      [
        "import { intent } from '@intlify/locale'",
        "const value = intent('{$first}{$last}', { last: user.last, first: user.first })"
      ].join('\n')
    )

    expect(result.ok).toBe(true)
    expect(result.intents[0].parameters).toEqual([
      { name: 'first', type: 'string-or-number' },
      { name: 'last', type: 'string-or-number' }
    ])
    expect(result.transformedSource).toContain('{ last: user.last, first: user.first }')
  })

  test.each([
    ["import { intent as localize } from '@intlify/locale'"],
    ["import locale from '@intlify/locale'"],
    ["import * as locale from '@intlify/locale'"],
    ["import { intent, other } from '@intlify/locale'"],
    ["import { intent } from '@intlify/locale' with { type: 'json' }"],
    [
      ["import { intent } from '@intlify/locale'", "import { intent } from '@intlify/locale'"].join(
        '\n'
      )
    ]
  ])('rejects a non-exact marker import: %s', source => {
    const result = transform(source)

    expect(result.ok).toBe(false)
    expect(result.diagnostics).toEqual([
      expect.objectContaining({ code: 'LC010', name: 'invalid_intent_declaration' })
    ])
  })

  test('collects all unsupported non-marker imports in source order', () => {
    const result = transform("import './first.js'\nimport value from './second.js'")

    expect(result.ok).toBe(false)
    expect(result.diagnostics.map(diagnostic => [diagnostic.code, diagnostic.line])).toEqual([
      ['LC002', 1],
      ['LC002', 2]
    ])
  })

  test('rejects dynamic imports and re-exports as unsupported module graph edges', () => {
    const result = transform(
      ["const first = import('./first.js')", "export * from './second.js'"].join('\n')
    )

    expect(result.ok).toBe(false)
    expect(result.diagnostics.map(diagnostic => [diagnostic.code, diagnostic.line])).toEqual([
      ['LC002', 1],
      ['LC002', 2]
    ])
  })

  test('does not recognize an unimported application intent function', () => {
    const source = [
      'function intent(value) { return value }',
      "const message = intent('Welcome')"
    ].join('\n')
    const result = transform(source)

    expect(result).toMatchObject({
      ok: true,
      diagnostics: [],
      intents: [],
      transformedSource: source
    })
  })

  test.each([
    ['an assigned reference', 'const localize = intent'],
    ['property-call indirection', "intent.call(null, 'Welcome')"],
    ['construction', "const value = new intent('Welcome')"],
    ['a parenthesized callee', "const value = (intent)('Welcome')"],
    ['an optional call', "const value = intent?.('Welcome')"],
    ['a tagged template', 'const value = intent`Welcome`'],
    ['an exported reference', 'export { intent }']
  ])('rejects %s of the marker binding', (_name, usage) => {
    const result = transform(["import { intent } from '@intlify/locale'", usage].join('\n'))

    expect(result.ok).toBe(false)
    expect(result.diagnostics).toEqual([expect.objectContaining({ code: 'LC010' })])
  })

  test('rejects marker binding shadowing without misclassifying a valid outer call', () => {
    const result = transform(
      [
        "import { intent } from '@intlify/locale'",
        'function render(intent) { return intent }',
        "const title = intent('Welcome')"
      ].join('\n')
    )

    expect(result.ok).toBe(false)
    expect(result.diagnostics).toEqual([
      expect.objectContaining({
        code: 'LC010',
        line: 2,
        message: expect.stringContaining('shadowed')
      })
    ])
  })

  test('extracts explicit Intents from unreachable source', () => {
    const result = transform(
      [
        "import { intent } from '@intlify/locale'",
        'if (false) {',
        "  console.log(intent('Never shown'))",
        '}'
      ].join('\n')
    )

    expect(result.ok).toBe(true)
    expect(result.intents[0].sourceText).toBe('Never shown')
  })

  test('warns only when an explicit Intent is part of a larger textContent expression', () => {
    const direct = transform(
      [
        "import { intent } from '@intlify/locale'",
        "heading.textContent = intent('Hello, {$name}!', { name })"
      ].join('\n')
    )
    const compound = transform(
      [
        "import { intent } from '@intlify/locale'",
        "heading.textContent = prefix + intent('Hello')"
      ].join('\n')
    )

    expect(direct.ok).toBe(true)
    expect(direct.diagnostics).toEqual([])
    expect(compound.ok).toBe(true)
    expect(compound.diagnostics).toEqual([expect.objectContaining({ code: 'LC001' })])
    expect(compound.transformedSource).toContain('__intlify_message(')
  })

  test('collects LC001 and independent LC010 diagnostics in source order', () => {
    const result = transform(
      [
        "import { intent } from '@intlify/locale'",
        'heading.textContent = dynamicValue',
        'const first = intent(message)',
        "const second = intent('Hello, {$user-name}!', { user_name })"
      ].join('\n')
    )

    expect(result.ok).toBe(false)
    expect(result.diagnostics.map(diagnostic => [diagnostic.code, diagnostic.line])).toEqual([
      ['LC001', 2],
      ['LC010', 3],
      ['LC010', 4]
    ])
  })

  test('assigns occurrence-aware IDs to repeated explicit call sites', () => {
    const source = [
      "import { intent } from '@intlify/locale'",
      "const first = intent('Open')",
      "const second = intent('Open')"
    ].join('\n')
    const first = transform(source)
    const second = transform(source)

    expect(first.ok).toBe(true)
    expect(first.intents.map(intent => intent.id)).toEqual(second.intents.map(intent => intent.id))
    expect(new Set(first.intents.map(intent => intent.id)).size).toBe(2)
  })

  test('detects ID collisions across explicit and automatic Intents', () => {
    const result = transform(
      [
        "import { intent } from '@intlify/locale'",
        "const title = intent('Welcome')",
        "heading.textContent = 'Welcome'"
      ].join('\n'),
      { hashPayload: () => 'A'.repeat(16) }
    )

    expect(result.ok).toBe(false)
    expect(result.diagnostics.at(-1)).toMatchObject({ code: 'LC006', line: 3 })
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
  const parameterizedIntents = [
    {
      id: 'm_parameterized',
      sourceLocale: 'en',
      sourceText: 'Hello, {$first} {$last}!',
      surface: 'explicit-intent',
      parameters: [
        { name: 'first', type: 'string-or-number' },
        { name: 'last', type: 'string-or-number' }
      ]
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
          surface: 'dom-text-content',
          parameters: []
        },
        {
          intentId: 'm_b',
          sourceLocale: 'en',
          targetLocale: 'ja',
          sourceText: 'Second',
          surface: 'dom-text-content',
          parameters: []
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

  test('deep-copies parameter contracts before calling the Provider', async () => {
    const fixture = await createProvider(`
export const kind = 'fixture'
export const revision = 'fixture-v1'
export let received
export async function localize(requests) {
  received = requests
  requests[0].parameters[0].name = 'mutated'
  requests[0].parameters.push({ name: 'extra', type: 'string-or-number' })
  return [{
    intentId: requests[0].intentId,
    locale: requests[0].targetLocale,
    message: '{$last}、{$first}！'
  }]
}
`)

    const result = await runLocalizationProvider({
      providerPath: fixture.providerPath,
      cwd: fixture.cwd,
      intents: parameterizedIntents
    })

    expect(result.ok).toBe(true)
    expect(result.requests[0].parameters).toEqual([
      { name: 'first', type: 'string-or-number' },
      { name: 'last', type: 'string-or-number' }
    ])
  })

  test('passes parameter contracts in source-message order', async () => {
    const fixture = await createProvider(`
export const kind = 'fixture'
export const revision = 'fixture-v1'
export let received
export async function localize(requests) {
  received = requests
  return requests.map(request => ({
    intentId: request.intentId,
    locale: request.targetLocale,
    message: '{$last}、{$first}、{$first}！'
  }))
}
`)

    const result = await runLocalizationProvider({
      providerPath: fixture.providerPath,
      cwd: fixture.cwd,
      intents: parameterizedIntents
    })
    const providerModule = await import(pathToFileURL(fixture.providerPath).href)

    expect(result.ok).toBe(true)
    expect(providerModule.received[0]).toMatchObject({
      surface: 'explicit-intent',
      parameters: [
        { name: 'first', type: 'string-or-number' },
        { name: 'last', type: 'string-or-number' }
      ]
    })
    expect(result.candidates[0].message).toBe('{$last}、{$first}、{$first}！')
  })

  test.each([
    ['a missing placeholder', 'こんにちは！'],
    ['an extra placeholder', '{$first} {$last} {$extra}'],
    ['a malformed placeholder', '{$first} {$last']
  ])('rejects a localized message with %s', async (_name, message) => {
    const fixture = await createProvider(
      validProvider(`
export async function localize(requests) {
  return requests.map(request => ({
    intentId: request.intentId,
    locale: request.targetLocale,
    message: ${JSON.stringify(message)}
  }))
}
`)
    )

    const result = await runLocalizationProvider({
      providerPath: fixture.providerPath,
      cwd: fixture.cwd,
      intents: parameterizedIntents
    })

    expect(result).toEqual(providerDiagnostic('LC004', 'invalid_localization_result'))
  })

  test('does not parse placeholder-shaped text for an automatically extracted Intent', async () => {
    const fixture = await createProvider(
      validProvider(`
export async function localize(requests) {
  return requests.map(request => ({
    intentId: request.intentId,
    locale: request.targetLocale,
    message: '{$broken'
  }))
}
`)
    )
    const result = await runLocalizationProvider({
      providerPath: fixture.providerPath,
      cwd: fixture.cwd,
      intents: [
        {
          id: 'm_plain',
          sourceLocale: 'en',
          sourceText: 'Example: {$name}',
          surface: 'dom-text-content',
          parameters: []
        }
      ]
    })

    expect(result.ok).toBe(true)
    expect(result.candidates[0].message).toBe('{$broken')
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

  test('interpolates validated strings and finite numbers in locale-specific order', async () => {
    const parameterizedIntents = [
      {
        id: 'm_greeting',
        sourceLocale: 'en',
        sourceText: '{$first} {$last}: {$count} / {$first}',
        surface: 'explicit-intent',
        parameters: [
          { name: 'first', type: 'string-or-number' },
          { name: 'last', type: 'string-or-number' },
          { name: 'count', type: 'string-or-number' }
        ]
      },
      {
        id: 'm_value',
        sourceLocale: 'en',
        sourceText: 'Value: {$value}',
        surface: 'explicit-intent',
        parameters: [{ name: 'value', type: 'string-or-number' }]
      },
      {
        id: 'm_plain',
        sourceLocale: 'en',
        sourceText: 'Example: {$name}',
        surface: 'dom-text-content',
        parameters: []
      }
    ]
    const parameterizedCandidates = [
      {
        intentId: 'm_greeting',
        locale: 'ja',
        message: '{$last}、{$first}: {$count} / {$first}'
      },
      { intentId: 'm_value', locale: 'ja', message: '値: {$value}' },
      { intentId: 'm_plain', locale: 'ja', message: '例: {$name}' }
    ]
    const artifacts = emitArtifacts({
      transformedSource: '',
      intents: parameterizedIntents,
      candidates: parameterizedCandidates,
      provider
    })
    const directory = await createTemporaryDirectory('locale-compiler-parameter-runtime-')
    for (const filename of ['intlify-runtime.mjs', 'locale-en.mjs', 'locale-ja.mjs']) {
      await writeFile(join(directory, filename), artifacts.get(filename), 'utf8')
    }

    const runtime = await import(pathToFileURL(join(directory, 'intlify-runtime.mjs')).href)
    const hadLocale = Object.hasOwn(globalThis, '__INTLIFY_LOCALE__')
    const previousLocale = globalThis.__INTLIFY_LOCALE__

    try {
      delete globalThis.__INTLIFY_LOCALE__
      expect(runtime.message('m_greeting', { last: 'Lovelace', first: 'Ada', count: 2 })).toBe(
        'Ada Lovelace: 2 / Ada'
      )

      globalThis.__INTLIFY_LOCALE__ = 'ja'
      expect(runtime.message('m_greeting', { count: 2, first: 'Ada', last: 'Lovelace' })).toBe(
        'Lovelace、Ada: 2 / Ada'
      )
      expect(runtime.message('m_value', { value: '{$other}' })).toBe('値: {$other}')
      expect(runtime.message('m_plain')).toBe('例: {$name}')
    } finally {
      restoreGlobal('__INTLIFY_LOCALE__', hadLocale, previousLocale)
    }
  })

  test('rejects missing, unexpected, and unsafe runtime parameter values without leaking them', async () => {
    const runtimeIntents = [
      {
        id: 'm_parameters',
        sourceLocale: 'en',
        sourceText: '{$first} {$last}',
        surface: 'explicit-intent',
        parameters: [
          { name: 'first', type: 'string-or-number' },
          { name: 'last', type: 'string-or-number' }
        ]
      }
    ]
    const runtimeCandidates = [
      { intentId: 'm_parameters', locale: 'ja', message: '{$last} {$first}' }
    ]
    const artifacts = emitArtifacts({
      transformedSource: '',
      intents: runtimeIntents,
      candidates: runtimeCandidates,
      provider
    })
    const directory = await createTemporaryDirectory('locale-compiler-invalid-parameters-')
    for (const filename of ['intlify-runtime.mjs', 'locale-en.mjs', 'locale-ja.mjs']) {
      await writeFile(join(directory, filename), artifacts.get(filename), 'utf8')
    }
    const runtime = await import(pathToFileURL(join(directory, 'intlify-runtime.mjs')).href)

    expect(() => runtime.message('m_parameters', { last: 'Lovelace' })).toThrow(
      'missing message parameter: m_parameters/first'
    )
    expect(() =>
      runtime.message('m_parameters', { first: 'Ada', last: 'Lovelace', unused: 'secret' })
    ).toThrow('unexpected message parameter: m_parameters/unused')

    for (const invalid of [null, undefined, true, {}, 1n, Number.NaN, Infinity, -Infinity]) {
      let error
      try {
        runtime.message('m_parameters', { first: invalid, last: 'private-value' })
      } catch (caught) {
        error = caught
      }
      expect(error).toBeInstanceOf(TypeError)
      expect(error.message).toBe(
        'invalid message parameter: m_parameters/first must be a string or finite number'
      )
      expect(error.message).not.toContain('private-value')
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

describe('Compiler orchestration', () => {
  test('writes the same five artifacts returned by a successful compile', async () => {
    const fixture = await createCompileFixture()
    const result = await compile(fixture)

    expect(result.ok).toBe(true)
    expect([...result.artifacts.keys()]).toEqual([
      'app.js',
      'intlify-runtime.mjs',
      'locale-en.mjs',
      'locale-ja.mjs',
      'localization-report.json'
    ])

    for (const [filename, text] of result.artifacts) {
      expect(await readFile(join(fixture.outDir, filename), 'utf8')).toBe(text)
    }
  })

  test('returns LC001 as a successful no-op compile warning', async () => {
    const fixture = await createCompileFixture({ source: 'heading.textContent = user.name' })
    const result = await compile(fixture)

    expect(result.ok).toBe(true)
    expect(result.diagnostics).toEqual([
      expect.objectContaining({ code: 'LC001', severity: 'warning' })
    ])
    expect(result.artifacts.get('app.js')).toBe('heading.textContent = user.name')
    expect(JSON.parse(result.artifacts.get('localization-report.json'))).toMatchObject({
      intentCount: 0,
      messageCountByLocale: { en: 0, ja: 0 }
    })
  })

  test('stops before loading the Provider after a Frontend error', async () => {
    const fixture = await createCompileFixture({
      source: "import value from './value.js'",
      providerSource: 'export const ='
    })
    const result = await compile(fixture)

    expect(result).toEqual({
      ok: false,
      diagnostics: [expect.objectContaining({ code: 'LC002' })]
    })
    await expectPathMissing(fixture.outDir)
  })

  test('stops before loading the Provider after an invalid explicit Intent', async () => {
    const fixture = await createCompileFixture({
      source: [
        "import { intent } from '@intlify/locale'",
        'const message = intent(dynamicSource)'
      ].join('\n'),
      providerSource: 'export const ='
    })
    const result = await compile(fixture)

    expect(result).toEqual({
      ok: false,
      diagnostics: [expect.objectContaining({ code: 'LC010', line: 2 })]
    })
    await expectPathMissing(fixture.outDir)
  })

  test('rejects a localized parameter mismatch before writing artifacts', async () => {
    const fixture = await createCompileFixture({
      source: [
        "import { intent } from '@intlify/locale'",
        "const message = intent('Hello, {$name}!', { name: 'Ada' })"
      ].join('\n'),
      providerSource: validProvider(`
export async function localize(requests) {
  return requests.map(request => ({
    intentId: request.intentId,
    locale: request.targetLocale,
    message: 'こんにちは！'
  }))
}
`)
    })
    const result = await compile(fixture)

    expect(result).toEqual({
      ok: false,
      diagnostics: [expect.objectContaining({ code: 'LC004' })]
    })
    await expectPathMissing(fixture.outDir)
  })

  test.each([
    ['a non-js extension', { extension: '.txt' }],
    ['a missing entry', { writeEntry: false }],
    ['a UTF-8 BOM', { sourceBytes: Buffer.from([0xef, 0xbb, 0xbf, 0x61]) }],
    ['invalid UTF-8', { sourceBytes: Buffer.from([0xff]) }]
  ])('reports LC005 for %s', async (_name, options) => {
    const fixture = await createCompileFixture(options)
    const result = await compile(fixture)

    expect(result).toEqual({
      ok: false,
      diagnostics: [expect.objectContaining({ code: 'LC005', name: 'invalid_source' })]
    })
    await expectPathMissing(fixture.outDir)
  })

  test('retains warnings when Provider validation fails and leaves output untouched', async () => {
    const fixture = await createCompileFixture({
      source: [
        "heading.textContent = 'Welcome'",
        'description.textContent = user.description'
      ].join('\n'),
      providerSource: validProvider('export async function localize() { return [] }')
    })
    await mkdir(fixture.outDir)
    await writeFile(join(fixture.outDir, 'app.js'), 'existing output', 'utf8')

    const result = await compile(fixture)

    expect(result.ok).toBe(false)
    expect(result.diagnostics.map(diagnostic => diagnostic.code)).toEqual(['LC001', 'LC004'])
    expect(await readFile(join(fixture.outDir, 'app.js'), 'utf8')).toBe('existing output')
    await expectPathMissing(join(fixture.outDir, 'locale-en.mjs'))
  })

  test('rejects output that contains the entry or Provider', async () => {
    const fixture = await createCompileFixture()

    const entryResult = await compile({ ...fixture, outDir: fixture.cwd })
    expect(entryResult).toEqual({
      ok: false,
      diagnostics: [expect.objectContaining({ code: 'LC009', path: '.' })]
    })

    const nestedOutput = join(fixture.cwd, 'nested-output')
    await mkdir(nestedOutput)
    const nestedProvider = join(nestedOutput, 'provider.mjs')
    await writeFile(nestedProvider, defaultProviderSource(), 'utf8')
    const providerResult = await compile({
      ...fixture,
      outDir: nestedOutput,
      providerPath: nestedProvider
    })
    expect(providerResult).toEqual({
      ok: false,
      diagnostics: [expect.objectContaining({ code: 'LC009', path: 'nested-output' })]
    })
  })

  test('rejects an output directory symbolic link', async () => {
    const fixture = await createCompileFixture()
    const realOutput = join(fixture.cwd, 'real-output')
    await mkdir(realOutput)
    await symlink(realOutput, fixture.outDir, 'dir')

    const result = await compile(fixture)

    expect(result).toEqual({
      ok: false,
      diagnostics: [expect.objectContaining({ code: 'LC009', path: 'dist' })]
    })
  })

  test('rejects a generated artifact symbolic link', async () => {
    const fixture = await createCompileFixture()
    await mkdir(fixture.outDir)
    const target = join(fixture.cwd, 'outside-app.js')
    await writeFile(target, 'outside', 'utf8')
    await symlink(target, join(fixture.outDir, 'app.js'))

    const result = await compile(fixture)

    expect(result).toEqual({
      ok: false,
      diagnostics: [expect.objectContaining({ code: 'LC009', path: 'dist/app.js' })]
    })
    expect(await readFile(target, 'utf8')).toBe('outside')
  })

  test('preserves unknown files and overwrites only known artifact files', async () => {
    const fixture = await createCompileFixture()
    await mkdir(fixture.outDir)
    await writeFile(join(fixture.outDir, 'unknown.txt'), 'keep me', 'utf8')
    await writeFile(join(fixture.outDir, 'app.js'), 'replace me', 'utf8')

    const result = await compile(fixture)

    expect(result.ok).toBe(true)
    expect(await readFile(join(fixture.outDir, 'unknown.txt'), 'utf8')).toBe('keep me')
    expect(await readFile(join(fixture.outDir, 'app.js'), 'utf8')).toBe(
      result.artifacts.get('app.js')
    )
  })

  test('reports LC008 when the output cannot be created as a directory', async () => {
    const fixture = await createCompileFixture()
    await writeFile(fixture.outDir, 'not a directory', 'utf8')

    const result = await compile(fixture)

    expect(result).toEqual({
      ok: false,
      diagnostics: [expect.objectContaining({ code: 'LC008', path: 'dist' })]
    })
  })

  test('reproduces byte-identical output over an existing build', async () => {
    const fixture = await createCompileFixture()
    const first = await compile(fixture)
    const second = await compile(fixture)

    expect(first.ok).toBe(true)
    expect(second.ok).toBe(true)
    expect([...second.artifacts]).toEqual([...first.artifacts])
  })
})

describe('CLI', () => {
  test('resolves the three required paths from the injected working directory', async () => {
    const fixture = await createCompileFixture()
    const output = createIo(fixture.cwd)

    const exitCode = await main(
      ['--entry', 'app.js', '--out', 'dist', '--provider', 'provider.mjs'],
      output.io
    )

    expect(exitCode).toBe(0)
    expect(output.stdout()).toBe('')
    expect(output.stderr()).toBe('')
    expect(await readFile(join(fixture.outDir, 'app.js'), 'utf8')).toContain('intlify-runtime.mjs')
  })

  test.each([
    ['a missing option', ['--entry', 'app.js', '--out', 'dist']],
    [
      'an unknown option',
      ['--entry', 'app.js', '--out', 'dist', '--provider', 'provider.mjs', '--unknown', 'value']
    ],
    [
      'a positional argument',
      ['--entry', 'app.js', '--out', 'dist', '--provider', 'provider.mjs', 'positional']
    ]
  ])('returns exit code 2 for %s', async (_name, argv) => {
    const fixture = await createCompileFixture()
    const output = createIo(fixture.cwd)

    const exitCode = await main(argv, output.io)

    expect(exitCode).toBe(2)
    expect(output.stdout()).toBe('')
    expect(output.stderr()).toContain('Usage: node src/cli.mjs')
    expect(output.stderr()).not.toContain('LC00')
    await expectPathMissing(fixture.outDir)
  })

  test('prints a source warning and still returns exit code 0', async () => {
    const fixture = await createCompileFixture({ source: 'heading.textContent = user.name' })
    const output = createIo(fixture.cwd)

    const exitCode = await main(cliArguments(), output.io)

    expect(exitCode).toBe(0)
    expect(output.stderr()).toMatch(
      /^warning LC001 unsupported_dynamic_ui_text\napp\.js:1:\d+\nDynamic textContent was left unchanged\.\n$/
    )
  })

  test('prints a blocking source diagnostic and returns exit code 1', async () => {
    const fixture = await createCompileFixture({ source: "import value from './value.js'" })
    const output = createIo(fixture.cwd)

    const exitCode = await main(cliArguments(), output.io)

    expect(exitCode).toBe(1)
    expect(output.stderr()).toBe(
      'error LC002 unsupported_module_import\n' +
        'app.js:1:1\n' +
        'The PoC entry must be a self-contained browser module.\n'
    )
  })

  test('prints LC010 for an invalid explicit Intent and returns exit code 1', async () => {
    const fixture = await createCompileFixture({
      source: [
        "import { intent } from '@intlify/locale'",
        "const message = intent('Hello, {$name}!')"
      ].join('\n')
    })
    const output = createIo(fixture.cwd)

    const exitCode = await main(cliArguments(), output.io)

    expect(exitCode).toBe(1)
    expect(output.stderr()).toMatch(
      /^error LC010 invalid_intent_declaration\napp\.js:2:\d+\nThe intent parameters must exactly match the message placeholders\.\n$/
    )
  })

  test('prints a non-source diagnostic with a path but no line or column', async () => {
    const fixture = await createCompileFixture({
      providerSource: "export const kind = 'fixture'"
    })
    const output = createIo(fixture.cwd)

    const exitCode = await main(cliArguments(), output.io)

    expect(exitCode).toBe(1)
    expect(output.stderr()).toBe(
      'error LC003 invalid_localization_provider\n' +
        'provider.mjs\n' +
        'The provider must export kind, revision, and localize.\n'
    )
  })
})

describe('end-to-end generated application', () => {
  test.each([
    ['the default locale', '', 'Hello, Ada!', 'Pay now'],
    ['English', '?locale=en', 'Hello, Ada!', 'Pay now'],
    ['Japanese', '?locale=ja', 'Adaさん、こんにちは！', '支払う']
  ])('renders %s through bootstrap and the runtime', async (_name, query, heading, button) => {
    const rendered = await runCompiledDemo({ query })

    expect(rendered.get('h1').textContent).toBe(heading)
    expect(rendered.get('#pay').textContent).toBe(button)
  })

  test('rejects an unsupported locale through bootstrap', async () => {
    await expect(runCompiledDemo({ query: '?locale=fr' })).rejects.toThrow('unsupported locale: fr')
  })

  test('assigns an HTML-shaped Provider message as plain textContent', async () => {
    const rendered = await runCompiledDemo({
      source: "const heading = document.querySelector('h1')\nheading.textContent = 'Welcome'",
      providerSource: validProvider(`
export async function localize(requests) {
  return requests.map(request => ({
    intentId: request.intentId,
    locale: request.targetLocale,
    message: '<strong>Hello</strong>'
  }))
}
`),
      query: '?locale=ja'
    })

    expect(rendered.get('h1')).toEqual({ textContent: '<strong>Hello</strong>' })
  })

  test('rejects an unsafe explicit runtime parameter value', async () => {
    await expect(
      runCompiledDemo({
        source: [
          "import { intent } from '@intlify/locale'",
          "const heading = document.querySelector('h1')",
          "heading.textContent = intent('Hello, {$name}!', { name: null })"
        ].join('\n'),
        query: '?locale=ja'
      })
    ).rejects.toThrow(/invalid message parameter: m_[\w-]{16}\/name/)
  })

  test('evaluates each explicit parameter expression once for repeated placeholders', async () => {
    try {
      const rendered = await runCompiledDemo({
        source: [
          "import { intent } from '@intlify/locale'",
          'globalThis.__intentParameterEvaluations = 0',
          'function getName() {',
          '  globalThis.__intentParameterEvaluations += 1',
          "  return 'Ada'",
          '}',
          "const heading = document.querySelector('h1')",
          "heading.textContent = intent('{$name}, {$name}!', { name: getName() })"
        ].join('\n'),
        providerSource: validProvider(`
export async function localize(requests) {
  return requests.map(request => ({
    intentId: request.intentId,
    locale: request.targetLocale,
    message: request.sourceText
  }))
}
`)
      })

      expect(rendered.get('h1').textContent).toBe('Ada, Ada!')
      expect(globalThis.__intentParameterEvaluations).toBe(1)
    } finally {
      delete globalThis.__intentParameterEvaluations
    }
  })

  test('executes an Intent-free generated application without importing the runtime', async () => {
    const rendered = await runCompiledDemo({ source: 'globalThis.__pocExecuted = true' })

    expect(globalThis.__pocExecuted).toBe(true)
    expect(rendered.get('h1').textContent).toBe('')
    delete globalThis.__pocExecuted
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

async function createCompileFixture(options = {}) {
  const cwd = await createTemporaryDirectory('locale-compiler-compile-')
  const extension = options.extension ?? '.js'
  const entryPath = join(cwd, `app${extension}`)
  const outDir = join(cwd, 'dist')
  const providerPath = join(cwd, 'provider.mjs')

  if (options.writeEntry !== false) {
    await writeFile(
      entryPath,
      options.sourceBytes ??
        Buffer.from(options.source ?? "heading.textContent = 'Welcome'", 'utf8')
    )
  }
  await writeFile(providerPath, options.providerSource ?? defaultProviderSource(), 'utf8')

  return { cwd, entryPath, outDir, providerPath }
}

function defaultProviderSource() {
  return validProvider(`
export async function localize(requests) {
  return requests.map(request => ({
    intentId: request.intentId,
    locale: request.targetLocale,
    message: request.sourceText + ' ja'
  }))
}
`)
}

async function expectPathMissing(path) {
  await expect(readFile(path)).rejects.toMatchObject({ code: 'ENOENT' })
}

function restoreGlobal(name, existed, value) {
  if (existed) {
    globalThis[name] = value
  } else {
    delete globalThis[name]
  }
}

function createIo(cwd) {
  let stdout = ''
  let stderr = ''
  return {
    io: {
      cwd,
      stdout: { write: text => (stdout += text) },
      stderr: { write: text => (stderr += text) }
    },
    stdout: () => stdout,
    stderr: () => stderr
  }
}

function cliArguments() {
  return ['--entry', 'app.js', '--out', 'dist', '--provider', 'provider.mjs']
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

async function runCompiledDemo(options = {}) {
  const fixture = await createCompileFixture({
    source:
      options.source ??
      [
        "import { intent } from '@intlify/locale'",
        "const heading = document.querySelector('h1')",
        "const name = 'Ada'",
        "heading.textContent = intent('Hello, {$name}!', { name })",
        "const button = document.querySelector('#pay')",
        "button.textContent = 'Pay now'"
      ].join('\n'),
    providerSource: options.providerSource ?? demoProviderSource()
  })
  const result = await compile(fixture)
  expect(result.ok).toBe(true)

  const bootstrapSource = await readFile(join(import.meta.dirname, '../demo/bootstrap.mjs'), 'utf8')
  const bootstrapPath = join(fixture.cwd, 'bootstrap.mjs')
  await writeFile(bootstrapPath, bootstrapSource, 'utf8')

  const elements = new Map([
    ['h1', { textContent: '' }],
    ['#pay', { textContent: '' }]
  ])
  const previousGlobals = new Map(
    ['location', 'document', '__INTLIFY_LOCALE__'].map(name => [
      name,
      { existed: Object.hasOwn(globalThis, name), value: globalThis[name] }
    ])
  )

  try {
    globalThis.location = { href: `https://example.test/index.html${options.query ?? ''}` }
    globalThis.document = {
      querySelector: selector => elements.get(selector) ?? null
    }
    await import(pathToFileURL(bootstrapPath).href)
    return elements
  } finally {
    for (const [name, previous] of previousGlobals) {
      restoreGlobal(name, previous.existed, previous.value)
    }
  }
}

function demoProviderSource() {
  return validProvider(`
const messages = new Map([
  ['Hello, {$name}!', '{$name}さん、こんにちは！'],
  ['Pay now', '支払う']
])
export async function localize(requests) {
  return requests.flatMap(request => {
    const message = messages.get(request.sourceText)
    return message === undefined
      ? []
      : [{ intentId: request.intentId, locale: request.targetLocale, message }]
  })
}
`)
}
