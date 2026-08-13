export const kind = 'fixture'
export const revision = 'fixture-v1'

const jaBySourceText = new Map([
  ['Hello, {$name}!', '{$name}さん、こんにちは！'],
  ['Pay now', '支払う']
])

/**
 * Resolve deterministic Japanese candidates for the demo messages.
 *
 * @param requests - Canonically ordered target-locale requests.
 * @returns Candidates for every source text known by this fixture.
 */
export async function localize(requests) {
  return requests.flatMap(request => {
    const message = jaBySourceText.get(request.sourceText)

    return message === undefined
      ? []
      : [{ intentId: request.intentId, locale: request.targetLocale, message }]
  })
}
