import * as eager0 from "./locales/locale-en.mjs";

export const formatVersion = [0, 1];
export const locales = ["en", "fr"];
export const fallbacks = [
  ["fr", ["en"]]
];

export function loadLocale(locale) {
  switch (locale) {
    case "en":
      return Promise.resolve(eager0);
    case "fr":
      return import("./locales/locale-fr.mjs");
    default:
      return Promise.reject(new RangeError("unsupported locale"));
  }
}
