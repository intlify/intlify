import {
  createMessageAccessor,
  type MessageKey,
  type MessageRuntime
} from './accessor-app.ts'

const runtime: MessageRuntime<string> = {
  format(_scope, ...call) {
    return call[0][1]
  }
}

const t = createMessageAccessor(runtime)

t(['json-pointer', '/hello'], {})
t(['json-pointer', '/total'], { count: 1 })

// @ts-expect-error /total requires count.
t(['json-pointer', '/total'], {})

// @ts-expect-error /hello does not accept /total's argument shape.
t(['json-pointer', '/hello'], { count: 1 })

function callWithWidenedKey(key: MessageKey) {
  // @ts-expect-error A widened key is not correlated with one argument branch.
  return t(key, {})
}

void callWithWidenedKey
