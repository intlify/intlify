/**
 * RFC 3986's URI references and RFC 6901's JSON Pointers, as the checkers read
 * them on their own.
 *
 * A reference is split by RFC 3986 Appendix B's expression and its components
 * checked against the RFC's grammar, an IPv6 literal with Node's `net.isIPv6`.
 * Resolution and normalization follow the RFC's algorithms as it writes them,
 * and a fragment is decoded with `decodeURIComponent` before it is read as a
 * pointer. The Rust crate reads these with code of its own, so the checkers
 * share this module with each other only.
 */

import { isIPv6 } from 'node:net'

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
export function parseReference(text) {
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
export function removeDotSegments(path) {
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
export function resolveReference(reference, base) {
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
export function recompose({ scheme, authority, path, query, fragment }) {
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
export function normalizeReference(reference) {
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
export function readFragment(fragment) {
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
export function evaluate(document, tokens) {
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
