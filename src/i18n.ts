import en from "./locales/en.json" with { type: "json" };
import zh from "./locales/zh-Hans.json" with { type: "json" };

export type Locale = "en" | "zh-Hans";
export type AppError = {
  code: string;
  params: Record<string, unknown>;
  targetId?: string;
};
export type Session = { version: string; locale: Locale };

const resources: Record<Locale, Record<string, string>> = { en, "zh-Hans": zh };

export function translate(locale: Locale, key: string, params: Record<string, string | number> = {}): string {
  const catalog = resources[locale];
  const message = (Object.prototype.hasOwnProperty.call(catalog, key) ? catalog[key] : undefined)
    ?? (Object.prototype.hasOwnProperty.call(en, key) ? (en as Record<string, string>)[key] : catalog["error.generic"] ?? en["error.generic"]);
  let missing = false;
  const result = message.replace(/\{(\w+)\}/g, (_, name: string) => {
    if (!Object.prototype.hasOwnProperty.call(params, name)) { missing = true; return ""; }
    const value = params[name];
    return typeof value === "number" ? new Intl.NumberFormat(locale).format(value) : value;
  });
  return missing ? translate(locale, "error.generic") : result;
}

export function errorMessage(locale: Locale, error: unknown): string {
  if (typeof error === "object" && error !== null && "code" in error
    && error.code === "native_connection_failed" && "params" in error
    && typeof error.params === "object" && error.params !== null
    && !Array.isArray(error.params) && Object.keys(error.params).length === 0) {
    return translate(locale, "error.nativeConnection");
  }
  return translate(locale, "error.generic");
}
