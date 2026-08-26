/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

const locale = new URL(globalThis.location.href).searchParams.get('locale') ?? 'en'

globalThis.__INTLIFY_LOCALE__ = locale

await import('./dist/app.js')
