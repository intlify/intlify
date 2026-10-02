import { intent, mf2, noIntent } from 'fixture-authoring'

/* @intlify { "description": "Greeting addressed to the signed-in user" } */
const greeting = mf2`Hello {$name}!`

export function render(name) {
  const save = document.querySelector('#save')
  const heading = document.querySelector('#heading')
  const first = document.querySelector('#first')
  const second = document.querySelector('#second')
  const brand = document.querySelector('#brand')

  save.textContent = 'Save'
  heading.textContent = intent('Welcome')
  first.textContent = intent(greeting, { name })
  second.textContent = intent(greeting, { name })
  brand.textContent = noIntent('Intlify', 'Product name')
}
