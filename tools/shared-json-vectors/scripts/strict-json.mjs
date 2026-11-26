/**
 * Strict reading of JSON text, which `JSON.parse` alone does not give.
 *
 * Design 017 has every decoder refuse a repeated member, while `JSON.parse`
 * keeps the last value and says nothing. The conformance suite's checkers
 * read their artifacts with this module so that a repeated member is a
 * disagreement, as it is for the Rust runner.
 */

/**
 * Whether a JSON text repeats a member name within one object. The text is
 * valid JSON, and names compare as decoded strings.
 *
 * @param text - The JSON text.
 * @returns Whether a member repeats.
 */
export function repeatsMember(text) {
  const open = []
  let name = false
  for (let at = 0; at < text.length; at += 1) {
    const character = text[at]
    if (character === '"') {
      let end = at + 1
      while (text[end] !== '"') {
        end += text[end] === '\\' ? 2 : 1
      }
      const names = open.at(-1)
      if (name && names instanceof Set) {
        const decoded = JSON.parse(text.slice(at, end + 1))
        if (names.has(decoded)) {
          return true
        }
        names.add(decoded)
        name = false
      }
      at = end
    } else if (character === '{') {
      open.push(new Set())
      name = true
    } else if (character === '[') {
      open.push(null)
      name = false
    } else if (character === '}' || character === ']') {
      open.pop()
      name = false
    } else if (character === ',') {
      name = open.at(-1) instanceof Set
    }
  }
  return false
}
