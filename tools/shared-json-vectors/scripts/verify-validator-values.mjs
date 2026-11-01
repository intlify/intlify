/**
 * Check the committed validator value vectors with an implementation of their own.
 *
 * The Rust crate reads JSON numbers as exact decimals, keys JSON values for
 * equality, splits, resolves, and normalizes URI references under RFC 3986,
 * and reads the JSON Pointers that URI fragments spell under RFC 6901, and it
 * writes its readings of chosen inputs. This script shares no code with it: it
 * compares numbers as BigInt fractions, decides `multipleOf` by modular
 * exponentiation, compares JSON values on its own, reads RFC 3986's grammar
 * with its own expressions and Node's `net.isIPv6`, resolves by RFC 3986's
 * algorithm as the RFC writes it, and decodes fragments with
 * `decodeURIComponent`. It also carries its own transcription of the examples
 * RFC 3986 section 5.4 and RFC 6901 list, so a reading both get wrong in the
 * same way is still caught where the RFCs speak.
 */

import { readFileSync } from 'node:fs'
import { isIPv6 } from 'node:net'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const vectorsPath = resolve(
  here,
  '../../../crates/intlify_config/fixtures/validator-values/vectors.json'
)

/** The most significant digits a `multipleOf` divisor may have. */
const MAX_DIVISOR_DIGITS = 19

/** The largest count, which every larger count reads as. */
const MAX_COUNT = (1n << 64n) - 1n

/** Past this many places apart, two numbers are compared by their leading places. */
const SCALE_LIMIT = 100_000n

/** RFC 3986 section 5.4's base. */
const RFC_3986_BASE = 'http://a/b/c/d;p?q'

/** RFC 3986 sections 5.4.1 and 5.4.2: each reference with its target, strict. */
const RFC_3986_EXAMPLES = [
  ['g:h', 'g:h'],
  ['g', 'http://a/b/c/g'],
  ['./g', 'http://a/b/c/g'],
  ['g/', 'http://a/b/c/g/'],
  ['/g', 'http://a/g'],
  ['//g', 'http://g'],
  ['?y', 'http://a/b/c/d;p?y'],
  ['g?y', 'http://a/b/c/g?y'],
  ['#s', 'http://a/b/c/d;p?q#s'],
  ['g#s', 'http://a/b/c/g#s'],
  ['g?y#s', 'http://a/b/c/g?y#s'],
  [';x', 'http://a/b/c/;x'],
  ['g;x', 'http://a/b/c/g;x'],
  ['g;x?y#s', 'http://a/b/c/g;x?y#s'],
  ['', 'http://a/b/c/d;p?q'],
  ['.', 'http://a/b/c/'],
  ['./', 'http://a/b/c/'],
  ['..', 'http://a/b/'],
  ['../', 'http://a/b/'],
  ['../g', 'http://a/b/g'],
  ['../..', 'http://a/'],
  ['../../', 'http://a/'],
  ['../../g', 'http://a/g'],
  ['../../../g', 'http://a/g'],
  ['../../../../g', 'http://a/g'],
  ['/./g', 'http://a/g'],
  ['/../g', 'http://a/g'],
  ['g.', 'http://a/b/c/g.'],
  ['.g', 'http://a/b/c/.g'],
  ['g..', 'http://a/b/c/g..'],
  ['..g', 'http://a/b/c/..g'],
  ['./../g', 'http://a/b/g'],
  ['./g/.', 'http://a/b/c/g/'],
  ['g/./h', 'http://a/b/c/g/h'],
  ['g/../h', 'http://a/b/c/h'],
  ['g;x=1/./y', 'http://a/b/c/g;x=1/y'],
  ['g;x=1/../y', 'http://a/b/c/y'],
  ['g?y/./x', 'http://a/b/c/g?y/./x'],
  ['g?y/../x', 'http://a/b/c/g?y/../x'],
  ['g#s/./x', 'http://a/b/c/g#s/./x'],
  ['g#s/../x', 'http://a/b/c/g#s/../x'],
  ['http:g', 'http:g']
]

/** RFC 6901 section 5's document. */
const RFC_6901_DOCUMENT = {
  foo: ['bar', 'baz'],
  '': 0,
  'a/b': 1,
  'c%d': 2,
  'e^f': 3,
  'g|h': 4,
  'i\\j': 5,
  'k"l': 6,
  ' ': 7,
  'm~n': 8
}

/** RFC 6901 section 6: each fragment with the value it names. */
const RFC_6901_FRAGMENTS = [
  ['', RFC_6901_DOCUMENT],
  ['/foo', ['bar', 'baz']],
  ['/foo/0', 'bar'],
  ['/', 0],
  ['/a~1b', 1],
  ['/c%25d', 2],
  ['/e%5Ef', 3],
  ['/g%7Ch', 4],
  ['/i%5Cj', 5],
  ['/k%22l', 6],
  ['/%20', 7],
  ['/m~0n', 8]
]

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
 * Read a JSON number as `sign * digits * 10^exponent`, the digits without a
 * leading or trailing zero.
 *
 * @param text - The number's text.
 * @returns The number, or null when the text is not a JSON number.
 */
function readNumber(text) {
  const match = /^(-?)(0|[1-9]\d*)(?:\.(\d+))?(?:e([+-]?\d+))?$/i.exec(text)
  if (match === null) {
    return null
  }
  const [, minus, whole, fraction = '', written = '0'] = match
  const significant = (whole + fraction).replace(/^0+/, '')
  if (significant === '') {
    return { sign: 0, digits: '', exponent: 0n }
  }
  const digits = significant.replace(/0+$/, '')
  return {
    sign: minus === '-' ? -1 : 1,
    digits,
    exponent: BigInt(written) - BigInt(fraction.length) + BigInt(significant.length - digits.length)
  }
}

/**
 * The canonical text of a number: the sign, the digits, and the exponent when
 * it is not zero.
 *
 * @param number - A number as `readNumber` reads it.
 * @returns The text.
 */
function canonical(number) {
  if (number.sign === 0) {
    return '0'
  }
  const sign = number.sign < 0 ? '-' : ''
  return number.exponent === 0n
    ? `${sign}${number.digits}`
    : `${sign}${number.digits}e${number.exponent}`
}

/**
 * Compare two numbers as fractions of BigInts, or, when they are too many
 * places apart to scale, by their leading places.
 *
 * @param left - A number.
 * @param right - Another.
 * @returns -1, 0, or 1.
 */
function compare(left, right) {
  if (left.sign !== right.sign) {
    return Math.sign(left.sign - right.sign)
  }
  if (left.sign === 0) {
    return 0
  }
  const shift = left.exponent - right.exponent
  let magnitude
  if (shift > SCALE_LIMIT || shift < -SCALE_LIMIT) {
    const leftTop = left.exponent + BigInt(left.digits.length)
    const rightTop = right.exponent + BigInt(right.digits.length)
    if (leftTop === rightTop) {
      throw new Error(`cannot compare ${canonical(left)} with ${canonical(right)}`)
    }
    magnitude = leftTop > rightTop ? 1 : -1
  } else {
    const scaledLeft = BigInt(left.digits) * 10n ** (shift > 0n ? shift : 0n)
    const scaledRight = BigInt(right.digits) * 10n ** (shift < 0n ? -shift : 0n)
    magnitude = scaledLeft === scaledRight ? 0 : scaledLeft > scaledRight ? 1 : -1
  }
  return left.sign * magnitude
}

/**
 * Whether a number has no fractional part: its digits are divisible by the
 * power of ten its exponent divides them by.
 *
 * @param number - A number.
 * @returns Whether it is an integer.
 */
function isInteger(number) {
  if (number.sign === 0 || number.exponent >= 0n) {
    return true
  }
  if (-number.exponent > BigInt(number.digits.length)) {
    return false
  }
  return BigInt(number.digits) % 10n ** -number.exponent === 0n
}

/**
 * A number as a count of the length keywords.
 *
 * @param number - A number.
 * @returns The count as decimal text, at most 2^64 - 1, or null when the
 *   number is not a non-negative integer.
 */
function count(number) {
  if (number.sign < 0 || !isInteger(number)) {
    return null
  }
  if (number.sign === 0) {
    return '0'
  }
  if (number.exponent > 20n) {
    return MAX_COUNT.toString()
  }
  const value = BigInt(number.digits) * 10n ** number.exponent
  return (value > MAX_COUNT ? MAX_COUNT : value).toString()
}

/**
 * Why a number is not a `multipleOf` divisor.
 *
 * @param number - A number.
 * @returns The failure, or null for a divisor.
 */
function divisorFailure(number) {
  if (number.sign <= 0) {
    return 'notPositive'
  }
  return number.digits.length > MAX_DIVISOR_DIGITS ? 'digits' : null
}

/**
 * `base^exponent mod modulus` by repeated squaring.
 *
 * @param base - The base.
 * @param exponent - A non-negative exponent of any size.
 * @param modulus - A positive modulus.
 * @returns The power's remainder.
 */
function power(base, exponent, modulus) {
  let result = 1n % modulus
  let square = base % modulus
  for (let rest = exponent; rest > 0n; rest >>= 1n) {
    if ((rest & 1n) === 1n) {
      result = (result * square) % modulus
    }
    square = (square * square) % modulus
  }
  return result
}

/**
 * Whether dividing a number by a divisor leaves an integer.
 *
 * @param number - A number.
 * @param divisor - A positive number.
 * @returns Whether the quotient is an integer.
 */
function isMultiple(number, divisor) {
  if (number.sign === 0) {
    return true
  }
  const digits = BigInt(number.digits)
  const by = BigInt(divisor.digits)
  const shift = number.exponent - divisor.exponent
  if (shift >= 0n) {
    return ((digits % by) * power(10n, shift, by)) % by === 0n
  }
  // The quotient is digits / (by * 10^-shift), and digits is below 10^length.
  if (-shift > BigInt(number.digits.length)) {
    return false
  }
  return digits % (by * 10n ** -shift) === 0n
}

/** A JSON number as its text spelled it. */
class JsonNumber {
  /**
   * Read a number's text.
   *
   * @param source - The number's text.
   */
  constructor(source) {
    this.number = readNumber(source)
  }
}

/**
 * Parse JSON text, keeping each number's text.
 *
 * @param text - The JSON text.
 * @returns The value, with each number a `JsonNumber`.
 */
function parseJson(text) {
  return JSON.parse(text, (_name, value, context) => {
    if (typeof value !== 'number') {
      return value
    }
    if (context?.source === undefined) {
      throw new Error('JSON.parse does not give the source text of numbers')
    }
    return new JsonNumber(context.source)
  })
}

/**
 * Whether two JSON values are equal: numbers by value, arrays item by item,
 * objects by their members in any order, and the rest as they are.
 *
 * @param left - A value.
 * @param right - Another.
 * @returns Whether they are equal.
 */
function equal(left, right) {
  if (left instanceof JsonNumber || right instanceof JsonNumber) {
    return (
      left instanceof JsonNumber &&
      right instanceof JsonNumber &&
      compare(left.number, right.number) === 0
    )
  }
  if (Array.isArray(left) || Array.isArray(right)) {
    return (
      Array.isArray(left) &&
      Array.isArray(right) &&
      left.length === right.length &&
      left.every((item, index) => equal(item, right[index]))
    )
  }
  if (left !== null && right !== null && typeof left === 'object' && typeof right === 'object') {
    const names = Object.keys(left)
    return (
      names.length === Object.keys(right).length &&
      names.every(name => Object.hasOwn(right, name) && equal(left[name], right[name]))
    )
  }
  return left === right
}

// RFC 3986's character sets, for classes read with the `i` flag: `\w` is the
// letters, the digits, and `_`, which with `.`, `~`, and `-` are unreserved.
const UNRESERVED = '\\w.~\\-'
const SUB_DELIMITERS = "!$&'()*+,;="
const ESCAPE = '%[\\da-f]{2}'
const SCHEME = /^[a-z][\d+.a-z-]*$/i
const USER_INFORMATION = new RegExp(`^(?:[${UNRESERVED}${SUB_DELIMITERS}:]|${ESCAPE})*$`, 'i')
const REGISTERED_NAME = new RegExp(`^(?:[${UNRESERVED}${SUB_DELIMITERS}]|${ESCAPE})*$`, 'i')
const PATH = new RegExp(`^(?:[${UNRESERVED}${SUB_DELIMITERS}:@/]|${ESCAPE})*$`, 'i')
const QUERY = new RegExp(`^(?:[${UNRESERVED}${SUB_DELIMITERS}:@/?]|${ESCAPE})*$`, 'i')
const FUTURE_ADDRESS = new RegExp(`^v[\\da-f]+\\.[${UNRESERVED}${SUB_DELIMITERS}:]+$`, 'i')

/**
 * RFC 3986 Appendix B's expression, which splits any text. The path after an
 * authority is matched apart: it starts with `/`, or is empty.
 */
const SPLIT = /^(?:([^#/:?]+):)?(?:\/\/([^#/?]*)(\/[^#?]*)?|([^#?]*))(?:\?([^#]*))?(?:#(.*))?$/s

/**
 * Whether an authority is `[ userinfo "@" ] host [ ":" port ]`.
 *
 * @param authority - The authority.
 * @returns Whether it is one.
 */
function isAuthority(authority) {
  const at = authority.lastIndexOf('@')
  if (at >= 0 && !USER_INFORMATION.test(authority.slice(0, at))) {
    return false
  }
  const hostAndPort = authority.slice(at + 1)
  const literal = /^\[([^\]]*)\](?::\d*)?$/.exec(hostAndPort)
  if (literal !== null) {
    const address = literal[1]
    return FUTURE_ADDRESS.test(address) || (!address.includes('%') && isIPv6(address))
  }
  const named = /^([^:]*)(?::\d*)?$/.exec(hostAndPort)
  return named !== null && REGISTERED_NAME.test(named[1])
}

/**
 * Read a URI reference into its five components, an absent one undefined.
 *
 * @param text - The reference.
 * @returns The components, or null when the text is outside the grammar.
 */
function parseReference(text) {
  const [, scheme, authority, pathAfterAuthority = '', pathAlone, query, fragment] =
    SPLIT.exec(text)
  const path = authority === undefined ? pathAlone : pathAfterAuthority
  const valid =
    (scheme === undefined || SCHEME.test(scheme)) &&
    (authority === undefined || isAuthority(authority)) &&
    PATH.test(path) &&
    (query === undefined || QUERY.test(query)) &&
    (fragment === undefined || QUERY.test(fragment)) &&
    // A relative reference's first segment holds no colon.
    !(scheme === undefined && authority === undefined && path.split('/')[0].includes(':'))
  return valid ? { scheme, authority, path, query, fragment } : null
}

/**
 * RFC 3986 section 5.2.4, with the input and output buffers it describes.
 *
 * @param path - A path.
 * @returns The path without dot segments.
 */
function removeDotSegments(path) {
  let input = path
  let output = ''
  const dropLast = () => {
    output = output.slice(0, Math.max(0, output.lastIndexOf('/')))
  }
  while (input.length > 0) {
    if (input.startsWith('../')) {
      input = input.slice(3)
    } else if (input.startsWith('./')) {
      input = input.slice(2)
    } else if (input.startsWith('/./')) {
      input = `/${input.slice(3)}`
    } else if (input === '/.') {
      input = '/'
    } else if (input.startsWith('/../')) {
      input = `/${input.slice(4)}`
      dropLast()
    } else if (input === '/..') {
      input = '/'
      dropLast()
    } else if (input === '.' || input === '..') {
      input = ''
    } else {
      const segment = /^\/?[^/]*/.exec(input)[0]
      output += segment
      input = input.slice(segment.length)
    }
  }
  return output
}

/**
 * RFC 3986 section 5.2.2's strict algorithm, with 5.2.3's merge.
 *
 * @param reference - The reference's components.
 * @param base - The base's components, with a scheme.
 * @returns The target's components.
 */
function resolveReference(reference, base) {
  if (reference.scheme !== undefined) {
    return { ...reference, path: removeDotSegments(reference.path) }
  }
  const target = { scheme: base.scheme, fragment: reference.fragment }
  if (reference.authority !== undefined) {
    target.authority = reference.authority
    target.path = removeDotSegments(reference.path)
    target.query = reference.query
  } else if (reference.path === '') {
    target.authority = base.authority
    target.path = base.path
    target.query = reference.query ?? base.query
  } else {
    target.authority = base.authority
    target.query = reference.query
    let merged = reference.path
    if (!merged.startsWith('/')) {
      merged =
        base.authority !== undefined && base.path === ''
          ? `/${merged}`
          : base.path.slice(0, base.path.lastIndexOf('/') + 1) + merged
    }
    target.path = removeDotSegments(merged)
  }
  return target
}

/**
 * RFC 3986 section 5.3's recomposition.
 *
 * @param components - The five components.
 * @returns The reference's text.
 */
function recompose({ scheme, authority, path, query, fragment }) {
  return (
    (scheme === undefined ? '' : `${scheme}:`) +
    (authority === undefined ? '' : `//${authority}`) +
    path +
    (query === undefined ? '' : `?${query}`) +
    (fragment === undefined ? '' : `#${fragment}`)
  )
}

/**
 * Decode the escapes of unreserved characters and uppercase the others, and
 * lowercase everything else when asked.
 *
 * @param text - A component.
 * @param lowercase - Whether the component is case-insensitive.
 * @returns The normalized component.
 */
function normalizeText(text, lowercase) {
  const fold = character => (lowercase ? character.toLowerCase() : character)
  return text.replaceAll(/%([\da-f]{2})|./gis, (match, hex) => {
    if (hex === undefined) {
      return fold(match)
    }
    const character = String.fromCodePoint(Number.parseInt(hex, 16))
    return /^[\w.~-]$/.test(character) ? fold(character) : `%${hex.toUpperCase()}`
  })
}

/**
 * RFC 3986 section 6.2.2's syntax-based normalization.
 *
 * @param reference - The reference's components.
 * @returns The normalized components.
 */
function normalizeReference(reference) {
  let authority
  if (reference.authority !== undefined) {
    const at = reference.authority.indexOf('@')
    authority =
      at < 0
        ? normalizeText(reference.authority, true)
        : `${normalizeText(reference.authority.slice(0, at), false)}@${normalizeText(
            reference.authority.slice(at + 1),
            true
          )}`
  }
  const path = normalizeText(reference.path, false)
  return {
    scheme: reference.scheme?.toLowerCase(),
    authority,
    path: reference.scheme === undefined ? path : removeDotSegments(path),
    query: reference.query === undefined ? undefined : normalizeText(reference.query, false),
    fragment:
      reference.fragment === undefined ? undefined : normalizeText(reference.fragment, false)
  }
}

/**
 * Read the JSON Pointer a fragment spells.
 *
 * @param fragment - The fragment as the reference writes it.
 * @returns The reference tokens, or null for a plain name or a malformed pointer.
 */
function readFragment(fragment) {
  if (fragment !== '' && !fragment.startsWith('/')) {
    return null
  }
  let decoded
  try {
    decoded = decodeURIComponent(fragment)
  } catch {
    return null
  }
  if (decoded === '') {
    return []
  }
  const tokens = decoded.slice(1).split('/')
  if (tokens.some(token => /~(?![01])/.test(token))) {
    return null
  }
  // RFC 6901 section 4's order: `~1` first, then `~0`.
  return tokens.map(token => token.replaceAll('~1', '/').replaceAll('~0', '~'))
}

/**
 * Follow reference tokens into a value.
 *
 * @param document - The value.
 * @param tokens - The tokens.
 * @returns Whether a value is there, and the value.
 */
function evaluate(document, tokens) {
  let value = document
  for (const token of tokens) {
    if (Array.isArray(value)) {
      if (!/^(?:0|[1-9]\d*)$/.test(token) || BigInt(token) >= BigInt(value.length)) {
        return { found: false }
      }
      value = value[Number(token)]
    } else if (value !== null && typeof value === 'object' && Object.hasOwn(value, token)) {
      value = value[token]
    } else {
      return { found: false }
    }
  }
  return { found: true, value }
}

const vectors = JSON.parse(readFileSync(vectorsPath, 'utf8'))

// Numbers.
const { numbers } = vectors
for (const vector of numbers.parsed) {
  const number = readNumber(vector.text)
  const expected = number === null ? null : canonical(number)
  if (vector.decimal !== expected) {
    problem(`${JSON.stringify(vector.text)} reads to ${expected}, not ${vector.decimal}`)
  }
}
const values = numbers.values.map(text => readNumber(text))
values.forEach((value, index) => {
  if (value === null) {
    problem(`${JSON.stringify(numbers.values[index])} is not a JSON number`)
  }
})
const marks = { '-1': '<', 0: '=', 1: '>' }
if (numbers.order.length !== values.length ** 2) {
  problem(`${numbers.order.length} orderings for ${values.length} numbers`)
}
values.forEach((left, i) => {
  values.forEach((right, j) => {
    const mark = marks[compare(left, right)]
    if (numbers.order[i * values.length + j] !== mark) {
      problem(`${numbers.values[i]} ${mark} ${numbers.values[j]}`)
    }
  })
})
values.forEach((value, index) => {
  const text = numbers.values[index]
  if ((numbers.integers[index] === '1') !== isInteger(value)) {
    problem(`${text} is ${isInteger(value) ? '' : 'not '}an integer`)
  }
  if (numbers.counts[index] !== count(value)) {
    problem(`${text} counts ${count(value)}, not ${numbers.counts[index]}`)
  }
})
const divisors = []
for (const vector of numbers.divisors) {
  const divisor = readNumber(vector.text)
  const failure = divisorFailure(divisor)
  if (vector.failure !== failure) {
    problem(`${vector.text} as a divisor fails with ${failure}, not ${vector.failure}`)
  }
  if (failure === null) {
    divisors.push([vector.text, divisor])
  }
}
if (numbers.multiples.length !== values.length * divisors.length) {
  problem(`${numbers.multiples.length} multiples for ${values.length * divisors.length} pairs`)
}
values.forEach((value, i) => {
  divisors.forEach(([text, divisor], j) => {
    const multiple = isMultiple(value, divisor)
    if ((numbers.multiples[i * divisors.length + j] === '1') !== multiple) {
      problem(`${numbers.values[i]} is ${multiple ? '' : 'not '}a multiple of ${text}`)
    }
  })
})

// Equality.
const classes = vectors.equality.classes.map(members => members.map(text => parseJson(text)))
classes.forEach((members, i) => {
  members.forEach((member, k) => {
    if (!equal(members[0], member)) {
      problem(`${vectors.equality.classes[i][k]} is not equal to ${vectors.equality.classes[i][0]}`)
    }
  })
  classes.slice(i + 1).forEach((others, offset) => {
    if (equal(members[0], others[0])) {
      problem(
        `${vectors.equality.classes[i][0]} equals ${vectors.equality.classes[i + 1 + offset][0]}`
      )
    }
  })
})
for (const vector of vectors.equality.uniqueItems) {
  const items = parseJson(vector.items)
  const unique = items.every((item, i) => items.slice(i + 1).every(other => !equal(item, other)))
  if (vector.unique !== unique) {
    problem(`${vector.items} is ${unique ? '' : 'not '}unique`)
  }
}

// URI references.
const { references } = vectors
const FIELDS = ['scheme', 'authority', 'path', 'query', 'fragment']
for (const vector of references.parsed) {
  const parsed = parseReference(vector.text)
  const name = JSON.stringify(vector.text)
  if ((parsed === null) !== (vector.components === null)) {
    problem(`${name} is ${parsed === null ? 'not ' : ''}a URI reference`)
  } else if (parsed !== null) {
    for (const field of FIELDS) {
      if ((parsed[field] ?? null) !== vector.components[field]) {
        problem(`${name}: the ${field} is ${parsed[field]}, not ${vector.components[field]}`)
      }
    }
  }
}
const rfcBase = parseReference(RFC_3986_BASE)
for (const [text, target] of RFC_3986_EXAMPLES) {
  const resolved = recompose(resolveReference(parseReference(text), rfcBase))
  if (resolved !== target) {
    problem(
      `this checker resolves ${JSON.stringify(text)} to ${resolved}, not RFC 3986's ${target}`
    )
  }
}
if (references.exampleBase !== RFC_3986_BASE) {
  problem(`the examples resolve against ${references.exampleBase}`)
}
if (references.examples.length !== RFC_3986_EXAMPLES.length) {
  problem(`${references.examples.length} examples, not ${RFC_3986_EXAMPLES.length}`)
}
references.examples.forEach((vector, index) => {
  const [text, target] = RFC_3986_EXAMPLES[index] ?? []
  if (vector.reference !== text || vector.target !== target) {
    problem(`example ${index}: ${vector.reference} resolves to ${vector.target}, not ${target}`)
  }
})
references.bases.forEach((baseText, i) => {
  const base = parseReference(baseText)
  references.references.forEach((text, j) => {
    const target = recompose(resolveReference(parseReference(text), base))
    if (references.targets[i]?.[j] !== target) {
      problem(
        `${JSON.stringify(text)} against ${baseText} is ${target}, not ${references.targets[i]?.[j]}`
      )
    }
  })
})
for (const vector of references.normalized) {
  const normalized = recompose(normalizeReference(parseReference(vector.text)))
  if (vector.normalized !== normalized) {
    problem(`${JSON.stringify(vector.text)} normalizes to ${normalized}, not ${vector.normalized}`)
  }
}

// JSON Pointers.
const { pointers } = vectors
for (const [name, value] of Object.entries(RFC_6901_DOCUMENT)) {
  if (!equal(pointers.document[name], value)) {
    problem(`the document's ${JSON.stringify(name)} is not RFC 6901's`)
  }
}
for (const [fragment, value] of RFC_6901_FRAGMENTS) {
  const vector = pointers.fragments.find(candidate => candidate.fragment === fragment)
  const named =
    fragment === ''
      ? Object.entries(RFC_6901_DOCUMENT).every(([member, item]) =>
          equal(vector?.value?.[member], item)
        )
      : equal(vector?.value, value)
  if (vector?.found !== true || !named) {
    problem(`RFC 6901's ${JSON.stringify(fragment)} does not name what the RFC lists`)
  }
}
for (const vector of pointers.fragments) {
  const name = JSON.stringify(vector.fragment)
  const tokens = readFragment(vector.fragment)
  if (tokens === null || vector.tokens === null) {
    if (tokens !== vector.tokens) {
      problem(`${name} reads to ${JSON.stringify(tokens)}, not ${JSON.stringify(vector.tokens)}`)
    }
    continue
  }
  if (!equal(tokens, vector.tokens)) {
    problem(`${name} reads to ${JSON.stringify(tokens)}, not ${JSON.stringify(vector.tokens)}`)
  }
  const { found, value } = evaluate(pointers.document, tokens)
  if (vector.found !== found || (found && !equal(vector.value, value))) {
    problem(`${name} names ${found ? JSON.stringify(value) : 'nothing'}`)
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
  `validator value vectors agree: ${numbers.parsed.length} number texts, ` +
    `${values.length ** 2} orderings, ${values.length * divisors.length} multiples, ` +
    `${classes.length} equality classes, ${vectors.equality.uniqueItems.length} arrays, ` +
    `${references.parsed.length} reference texts, ${references.examples.length} RFC 3986 examples, ` +
    `${references.bases.length * references.references.length} resolutions, ` +
    `${references.normalized.length} normalizations, ${pointers.fragments.length} fragments`
)
