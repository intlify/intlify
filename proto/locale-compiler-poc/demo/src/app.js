/**
 * @license MIT
 * @author kazuya kawaguchi (a.k.a. kazupon)
 */

import { intent } from '@intlify/locale'

const heading = document.querySelector('h1')
const name = 'Ada'
heading.textContent = intent('Hello, {$name}!', { name })

const button = document.querySelector('#pay')
button.textContent = 'Pay now'
