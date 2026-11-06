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
