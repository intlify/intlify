export const JS_TARGETS = [
  {
    name: 'messageformat-parse-message',
    runtime: 'js',
    format: 'mf2',
    description: 'messageformat parseMessage(source)'
  },
  {
    name: 'messageformat-parse-cst',
    runtime: 'js',
    format: 'mf2',
    description: 'messageformat/cst parseCST(source)'
  },
  {
    name: 'messageformat-cst-to-message',
    runtime: 'js',
    format: 'mf2',
    description: 'messageformat/cst messageFromCST(parseCST(source))'
  },
  {
    name: 'messageformat-constructor',
    runtime: 'js',
    format: 'mf2',
    description: 'new MessageFormat("en", source)'
  },
  {
    name: 'formatjs-icu-parse',
    runtime: 'js',
    format: 'mf1-icu',
    description: '@formatjs/icu-messageformat-parser parse(source)'
  }
]

export const RUST_TARGETS = [
  {
    name: 'ox-content-parse',
    runtime: 'rust',
    format: 'mf2',
    description: 'ox_content_i18n::mf2::parse(source)'
  },
  {
    name: 'ox-content-parse-and-validate',
    runtime: 'rust',
    format: 'mf2',
    description: 'ox_content_i18n::mf2::parse_and_validate(source)'
  },
  {
    name: 'mf2-tools-parse',
    runtime: 'rust',
    format: 'mf2',
    description: 'mf2_parser::parse(source)'
  },
  {
    name: 'mf2-tools-parse-and-analyze',
    runtime: 'rust',
    format: 'mf2',
    description: 'mf2_parser::parse(source) + analyze_semantics(...)'
  },
  {
    name: 'ox-mf2-parse',
    runtime: 'rust',
    format: 'mf2',
    description: 'ox_mf2_parser::parse_message(source) — Phase 1 CST only'
  },
  {
    name: 'ox-mf2-parse-and-lower',
    runtime: 'rust',
    format: 'mf2',
    description: 'ox_mf2_parser::parse_source(parse_semantic=true) — CST + SemanticModel'
  }
]

export const TARGETS = [...JS_TARGETS, ...RUST_TARGETS]

export const CORPORA = [
  { name: 'mf2-common', format: 'mf2', benchmark: true },
  { name: 'mf2-app', format: 'mf2', benchmark: true },
  { name: 'mf2-full', format: 'mf2', benchmark: false },
  { name: 'mf1-icu', format: 'mf1-icu', benchmark: true }
]

/**
 * Find a parser target by its stable benchmark name.
 *
 * @param name - Parser target name.
 * @returns Target metadata.
 */
export function getTarget(name) {
  const target = TARGETS.find(item => item.name === name)
  if (!target) {
    throw new Error(`Unknown parser target: ${name}`)
  }
  return target
}

/**
 * List benchmark corpora compatible with a parser target.
 *
 * @param target - Parser target metadata.
 * @returns Corpora that should be measured for the target.
 */
export function benchmarkCorporaForTarget(target) {
  return CORPORA.filter(corpus => corpus.benchmark && corpus.format === target.format)
}
