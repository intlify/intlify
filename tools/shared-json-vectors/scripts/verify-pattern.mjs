/**
 * Check the committed pattern vectors against JavaScript's own regular expressions.
 *
 * The Rust crate compiles Draft 7 `pattern` regular expressions under revision
 * "0"'s subset of ECMA-262 with the `u` flag, and writes the patterns it
 * compiles with the strings each matches and does not, the decisions of more
 * patterns on every short string over a small alphabet, and the patterns it
 * refuses with their failures. This script shares no code with it: it compiles
 * every pattern with `new RegExp(pattern, 'u')`, decides every string with that
 * engine, enumerates the short strings on its own, and reads the nesting and
 * the constructs of refused patterns with its own scanner.
 *
 * ECMA-262's RegExpBuiltinExec tries a match at each code point boundary of the
 * string, advancing past a surrogate pair at once under the `u` flag. V8 also
 * tries the position inside a pair, where `\B` holds between the two
 * surrogates, so the script steps through the boundaries itself and asks the
 * engine to match at each one.
 */

import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const vectorsPath = resolve(here, '../../../crates/intlify_config/fixtures/pattern/vectors.json')

/** The deepest that groups may nest under the subset. */
const MAX_NESTING = 64

/** The failures the subset reports, in the order its checks run. */
const FAILURES = ['nesting', 'syntax', 'unsupported', 'size']

/**
 * The constructs the subset leaves out, each with a reading of the source that
 * finds it. The vectors spell each construct plainly, so a lexical reading is
 * enough.
 */
const CONSTRUCTS = {
  backreference: /\\[1-9]|\\k</,
  lookaround: /\(\?<?[=!]/,
  namedGroup: /\(\?<[^=!]/,
  modifiers: /\(\?(?=[ims-])[ims]*(?:-[ims]*)?:/,
  propertyEscape: /\\p\{/i
}

const problems = []

/**
 * Record one disagreement.
 *
 * @param message - What disagrees.
 */
function problem(message) {
  problems.push(message)
}

/**
 * Compile a pattern as Draft 7 reads it under the subset: ECMA-262 with the `u`
 * flag and no other.
 *
 * @param source - The pattern's text.
 * @returns The compiled expression.
 */
function compile(source) {
  return new RegExp(source, 'u')
}

/**
 * Whether a pattern matches any part of a string, trying it at each code point
 * boundary as RegExpBuiltinExec does. The sticky flag makes the engine match at
 * the given position only, and it changes neither the syntax nor `^`.
 *
 * @param source - The pattern's text.
 * @param string - The string to search.
 * @returns Whether a match starts at some boundary.
 */
function matches(source, string) {
  const expression = new RegExp(source, 'uy')
  let boundary = 0
  for (const codePoint of string) {
    expression.lastIndex = boundary
    if (expression.test(string)) {
      return true
    }
    boundary += codePoint.length
  }
  expression.lastIndex = boundary
  return expression.test(string)
}

/**
 * Every string of up to `longest` code points of `alphabet`: the shorter first,
 * and strings of one length in the alphabet's order, the first code point
 * varying slowest.
 *
 * @param alphabet - The code points, one string each.
 * @param longest - The most code points a string holds.
 * @returns The strings in order.
 */
function strings(alphabet, longest) {
  const every = ['']
  let previous = ['']
  for (let length = 1; length <= longest; length += 1) {
    const next = []
    for (const prefix of previous) {
      for (const codePoint of alphabet) {
        next.push(prefix + codePoint)
      }
    }
    every.push(...next)
    previous = next
  }
  return every
}

/**
 * How deep the groups of a pattern nest: a backslash hides the code point
 * after it, and a parenthesis in a class is literal.
 *
 * @param source - The pattern's text.
 * @returns The deepest nesting.
 */
function nesting(source) {
  let depth = 0
  let deepest = 0
  let inClass = false
  const codePoints = [...source]
  for (let index = 0; index < codePoints.length; index += 1) {
    const codePoint = codePoints[index]
    if (codePoint === '\\') {
      index += 1
    } else if (codePoint === '[') {
      inClass = true
    } else if (codePoint === ']') {
      inClass = false
    } else if (codePoint === '(' && !inClass) {
      depth += 1
      deepest = Math.max(deepest, depth)
    } else if (codePoint === ')' && !inClass) {
      depth = Math.max(0, depth - 1)
    }
  }
  return deepest
}

const vectors = JSON.parse(readFileSync(vectorsPath, 'utf8'))

let decided = 0
for (const vector of vectors.chosen) {
  try {
    compile(vector.pattern)
  } catch (error) {
    problem(`${vector.id}: JavaScript refuses ${JSON.stringify(vector.pattern)}: ${error.message}`)
    continue
  }
  for (const [strings, expected] of [
    [vector.matching, true],
    [vector.notMatching, false]
  ]) {
    for (const string of strings) {
      decided += 1
      if (matches(vector.pattern, string) !== expected) {
        problem(`${vector.id}: ${JSON.stringify(string)} should ${expected ? '' : 'not '}match`)
      }
    }
  }
}

const every = strings(vectors.alphabet, vectors.longest)
for (const vector of vectors.exhaustive) {
  try {
    compile(vector.pattern)
  } catch (error) {
    problem(`JavaScript refuses ${JSON.stringify(vector.pattern)}: ${error.message}`)
    continue
  }
  if (vector.decisions.length !== every.length) {
    problem(
      `${JSON.stringify(vector.pattern)}: ${vector.decisions.length} decisions for ${every.length} strings`
    )
    continue
  }
  every.forEach((string, index) => {
    decided += 1
    const expected = vector.decisions[index] === '1'
    if (matches(vector.pattern, string) !== expected) {
      problem(
        `${JSON.stringify(vector.pattern)}: ${JSON.stringify(string)} should ${expected ? '' : 'not '}match`
      )
    }
  })
}

const failures = new Set()
for (const vector of vectors.refused) {
  failures.add(vector.failure)
  const name = JSON.stringify(
    vector.pattern.length > 40 ? `${vector.pattern.slice(0, 40)}...` : vector.pattern
  )
  if (!FAILURES.includes(vector.failure)) {
    problem(`${name}: unknown failure ${vector.failure}`)
    continue
  }
  let refusal
  try {
    compile(vector.pattern)
  } catch (error) {
    refusal = error
  }
  if (vector.failure === 'syntax') {
    if (!(refusal instanceof SyntaxError)) {
      problem(`${name}: JavaScript accepts a pattern refused as malformed`)
    }
    continue
  }
  // The other failures are of valid ECMA-262.
  if (refusal !== undefined) {
    problem(
      `${name}: JavaScript refuses a pattern refused as ${vector.failure}: ${refusal.message}`
    )
  }
  if (vector.failure === 'nesting' && nesting(vector.pattern) <= MAX_NESTING) {
    problem(`${name}: nests ${nesting(vector.pattern)} deep`)
  }
  if (vector.failure !== 'nesting' && nesting(vector.pattern) > MAX_NESTING) {
    problem(`${name}: nests deeper than ${MAX_NESTING} but is refused as ${vector.failure}`)
  }
  if (vector.failure === 'unsupported') {
    const reading = CONSTRUCTS[vector.construct]
    if (reading === undefined) {
      problem(`${name}: unknown construct ${vector.construct}`)
    } else if (!reading.test(vector.pattern)) {
      problem(`${name}: does not use ${vector.construct}`)
    }
  } else if (Object.values(CONSTRUCTS).some(reading => reading.test(vector.pattern))) {
    problem(`${name}: uses a construct outside the subset but is refused as ${vector.failure}`)
  }
}
for (const failure of FAILURES) {
  if (!failures.has(failure)) {
    problem(`no refused pattern fails with ${failure}`)
  }
}

if (problems.length > 0) {
  for (const message of problems) {
    console.error(message)
  }
  console.error(`${problems.length} disagreements`)
  process.exit(1)
}

console.log(
  `pattern vectors agree: ${vectors.chosen.length} chosen and ${vectors.exhaustive.length} exhaustive patterns, ` +
    `${decided} decisions, ${vectors.refused.length} refused patterns`
)
