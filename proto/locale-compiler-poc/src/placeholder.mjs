/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

const IDENTIFIER_PATTERN = /^[A-Z_]\w*/i

/**
 * Parse the PoC's minimal placeholder syntax and preserve first-use order.
 *
 * @param message - A cooked source or localized message.
 * @returns Ordered parameter names, or an invalid-placeholder result.
 */
export function parsePlaceholders(message) {
  const names = []
  const seen = new Set()

  for (let index = 0; index < message.length; index += 1) {
    if (message[index] !== '{') {
      continue
    }

    let dollarIndex = index + 1
    while (isWhitespace(message[dollarIndex])) {
      dollarIndex += 1
    }
    if (message[dollarIndex] !== '$') {
      continue
    }

    if (dollarIndex !== index + 1) {
      return { ok: false }
    }

    const identifierStart = dollarIndex + 1
    const identifier = IDENTIFIER_PATTERN.exec(message.slice(identifierStart))?.[0]
    if (identifier === undefined || identifier === '__proto__') {
      return { ok: false }
    }

    const end = identifierStart + identifier.length
    if (message[end] !== '}') {
      return { ok: false }
    }

    if (!seen.has(identifier)) {
      seen.add(identifier)
      names.push(identifier)
    }
    index = end
  }

  return { ok: true, names }
}

function isWhitespace(character) {
  return character !== undefined && /\s/.test(character)
}
