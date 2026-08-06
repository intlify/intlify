export const formatVersion = [0, 1] as const;
export const scope = ["project", "app"] as const;

export type MessageKey =
  | readonly ["json-pointer", "/hello"]
  | readonly ["json-pointer", "/total"];

export type MessageArguments<K extends MessageKey> = K extends readonly ["json-pointer", "/hello"]
  ? Readonly<Record<string, never>>
  : K extends readonly ["json-pointer", "/total"]
    ? Readonly<{
        "count": unknown;
      }>
    : never;

type MessageCall =
  | [key: readonly ["json-pointer", "/hello"], args: MessageArguments<readonly ["json-pointer", "/hello"]>]
  | [key: readonly ["json-pointer", "/total"], args: MessageArguments<readonly ["json-pointer", "/total"]>];

export interface MessageRuntime<Result> {
  format(messageScope: typeof scope, ...call: MessageCall): Result;
}

export function createMessageAccessor<Result>(runtime: MessageRuntime<Result>) {
  return function t(...call: MessageCall): Result {
    return runtime.format(scope, ...call);
  };
}
