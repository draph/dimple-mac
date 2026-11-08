import en from "./locales/en.json" with { type: "json" };
import zh from "./locales/zh-Hans.json" with { type: "json" };

export type Locale = "en" | "zh-Hans";
export type AppError = {
  code: string;
  params: Record<string, unknown>;
  targetId?: string;
};
export type Session = { version: string; locale: Locale; entry: import("./contracts").Entry; operation: import("./contracts").OperationState };

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
  if (typeof error === "object" && error !== null && "code" in error && "params" in error && typeof error.params === "object" && error.params !== null && !Array.isArray(error.params)) {
    const params = error.params as Record<string, unknown>;
    const reason = params.reason;
    if (error.code === "text_error" && typeof reason === "string" && ["budget", "line", "empty", "length", "control", "missing", "render", "timeout"].includes(reason)) return translate(locale, `text.error.${reason}`);
    if (error.code === "entry_error" && reason === "budget") return translate(locale, "error.budget");
    if (error.code === "asset_error" && typeof reason === "string" && ["unavailable", "type", "size", "dimensions", "damaged", "unsupported", "empty", "reference", "complexity", "timeout"].includes(reason)) return translate(locale, `asset.error.${reason}`);
    const entryReasons = ["busy", "limit", "invalid", "finderInactive", "finderFailed", "finderDenied", "finderTimeout", "shortcutFailed", "checkFailed", "changed", "badgeUnavailable", "multipleBadges", "mixed", "empty"];
    if (error.code === "entry_error" && typeof reason === "string" && entryReasons.includes(reason)) return translate(locale, `entry.${reason}`);
    if (error.code === "operation_error" && typeof reason === "string") {
      if (["fingerprint", "write", "image", "coordination", "confirmation", "uncertain", "edit"].includes(reason)) return translate(locale, `operation.error.${reason}`);
      if (reason === "busy") return translate(locale, "entry.busy");
      if (reason === "operation") return translate(locale, "error.operation");
    }
    if (["invalid_input", "permission_denied", "target_changed", "busy", "timeout"].includes(String(error.code)) && typeof reason === "string") {
      if (["changed"].includes(reason)) return translate(locale, "entry.changed");
      if (reason === "busy") return translate(locale, "entry.busy");
      if (reason === "check") return translate(locale, "entry.checkFailed");
      if (reason === "mixed") return translate(locale, "entry.mixed");
      if (reason === "multiple_badges") return translate(locale, "entry.multipleBadges");
      if (reason === "empty") return translate(locale, "entry.empty");
      if (reason === "limit") return translate(locale, "entry.limit");
      if (["unavailable", "path", "protected", "application", "unwritable", "read_only", "budget", "type", "payload", "images", "params", "asset", "duplicate", "operation", "id"].includes(reason)) return translate(locale, `error.${reason}`);
    }
  }
  return translate(locale, "error.generic");
}
