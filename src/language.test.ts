import assert from "node:assert/strict";
import test from "node:test";
import { acceptLanguage, type LanguageStatus } from "./language.ts";

const status = (revision: number, locale: "en" | "zh-Hans" = "en"): LanguageStatus => ({ preference: "system", locale, revision });

test("stale language responses cannot replace a newer selection", () => {
  const current = status(3, "zh-Hans");
  assert.equal(acceptLanguage(current, status(2)), current);
  assert.equal(acceptLanguage(current, status(3)).locale, "en");
  assert.equal(acceptLanguage(null, current), current);
});
