import { describe, expect, test } from 'vite-plus/test'

import { transformJavaScript } from '../src/frontend.mjs'

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
