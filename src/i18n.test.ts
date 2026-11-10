import assert from "node:assert/strict";
import test from "node:test";
import en from "./locales/en.json" with { type: "json" };
import zh from "./locales/zh-Hans.json" with { type: "json" };
import { errorMessage, translate } from "./i18n.ts";

test("catalogs have matching keys and named parameters", () => {
  assert.deepEqual(Object.keys(zh).sort(), Object.keys(en).sort());
  for (const key of Object.keys(en) as (keyof typeof en)[]) {
    assert.ok(en[key].trim());
    assert.ok(zh[key].trim());
    assert.deepEqual(zh[key].match(/\{\w+\}/g), en[key].match(/\{\w+\}/g));
  }
});

test("named values are inserted once without translating user content", () => {
  assert.equal(translate("zh-Hans", "connection.connected", { version: "中文{version}" }),
    "原生服务已连接 · v中文{version}");
  assert.equal(translate("en", "connection.connected", { version: "0.1.0" }),
    "Native service connected · v0.1.0");
  assert.equal(translate("en", "connection.connected"), en["error.generic"]);
});

test("missing Chinese messages fall back to English and unknown keys stay private", () => {
  const catalog: Record<string, string> = zh;
  const saved = catalog["app.heading"];
  try {
    delete catalog["app.heading"];
    assert.equal(translate("zh-Hans", "app.heading"), en["app.heading"]);
  } finally { catalog["app.heading"] = saved; }
  for (const key of ["private/path", "toString", "__proto__"]) {
    assert.equal(translate("zh-Hans", key), zh["error.generic"]);
  }
});

test("only registered structured errors produce specific messages", () => {
  for (const locale of ["en", "zh-Hans"] as const) {
    assert.equal(errorMessage(locale, { code: "native_connection_failed", params: {} }),
      translate(locale, "error.nativeConnection"));
    for (const error of [null, "/private/path", { code: "unknown", params: {} },
      { code: "native_connection_failed" }, { code: "native_connection_failed", params: [] },
      { code: "native_connection_failed", params: { path: "/private/path" } }]) {
      assert.equal(errorMessage(locale, error), translate(locale, "error.generic"));
    }
  }
});

test("target and operation errors use bounded reason mappings", () => {
  for (const locale of ["en", "zh-Hans"] as const) {
    assert.equal(errorMessage(locale, {code: "invalid_input", params: {reason: "file_url", index: 3}}), translate(locale, "error.fileUrl", {index: 3}));
    for (const index of [undefined, 0, -1, "private/path"]) assert.equal(errorMessage(locale, {code: "invalid_input", params: {reason: "file_url", index}}), translate(locale, "error.generic"));
    assert.equal(errorMessage(locale, {code: "target_changed", params: {reason: "changed"}}), translate(locale, "entry.changed"));
    assert.equal(errorMessage(locale, {code: "permission_denied", params: {reason: "unwritable"}}), translate(locale, "error.unwritable"));
    assert.equal(errorMessage(locale, {code: "invalid_input", params: {reason: "images"}}), translate(locale, "error.images"));
    assert.equal(errorMessage(locale, {code: "entry_error", params: {reason: "budget"}}), translate(locale, "error.budget"));
    assert.equal(errorMessage(locale, {code: "entry_error", params: {reason: "/private/unknown"}}), translate(locale, "error.generic"));
  }
});

test("asset errors have bounded localized reasons", () => {
  for (const locale of ["en", "zh-Hans"] as const) {
    for (const reason of ["size", "damaged", "unsupported", "empty", "reference", "complexity", "timeout"]) {
      assert.equal(errorMessage(locale, {code: "asset_error", params: {reason}}), translate(locale, `asset.error.${reason}`));
    }
    assert.equal(errorMessage(locale, {code: "asset_error", params: {reason: "/private/path"}}), translate(locale, "error.generic"));
  }
});


test("text errors have bounded bilingual mappings", () => {
  for (const locale of ["en", "zh-Hans"] as const) {
    for (const reason of ["budget", "line", "empty", "length", "control", "missing", "render", "timeout"]) {
      assert.equal(errorMessage(locale, {code: "text_error", params: {reason}}), translate(locale, `text.error.${reason}`));
    }
    assert.equal(errorMessage(locale, {code: "text_error", params: {reason: "private"}}), translate(locale, "error.generic"));
  }
});

test("shortcut failures have bounded bilingual messages", () => {
  for (const locale of ["en", "zh-Hans"] as const) {
    for (const reason of ["invalid", "register", "unregister", "save", "rollback", "damaged", "restart", "capture"]) {
      assert.equal(errorMessage(locale, { code: "shortcut_error", params: { reason } }), translate(locale, `settings.error.${reason}`));
    }
    assert.equal(errorMessage(locale, { code: "shortcut_error", params: { reason: "/private/path" } }), translate(locale, "error.generic"));
  }
});
