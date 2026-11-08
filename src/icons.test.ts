import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { icons, categories, searchIcons } from "./icons.ts";
import { translate } from "./i18n.ts";

test("the offline catalog has five licensed icons per category and localized names", () => {
  assert.equal(new Set(icons.map(icon => icon.id)).size, icons.length);
  for (const category of categories) assert.ok(icons.filter(icon => icon.category === category).length >= 5);
  for (const icon of icons) {
    assert.ok(categories.includes(icon.category));
    assert.equal(icon.license, "MIT");
    assert.equal(icon.source, `src/assets/${icon.id}.svg`);
    const svg = readFileSync(new URL(`../${icon.source}`, import.meta.url), "utf8");
    assert.ok(svg.startsWith('<svg xmlns="http://www.w3.org/2000/svg"'));
    assert.ok(!/<(?:script|image|text)|href=/.test(svg));
    for (const locale of ["en", "zh-Hans"] as const) {
      const name = translate(locale, icon.name);
      assert.notEqual(name, translate(locale, "error.generic"));
      assert.ok(searchIcons(name).some(found => found.id === icon.id));
    }
    for (const keyword of icon.keywords) assert.ok(searchIcons(keyword).some(found => found.id === icon.id));
  }
});

test("search matches bilingual names, categories and keywords with case and whitespace normalization", () => {
  assert.deepEqual(searchIcons(""), icons);
  assert.deepEqual(searchIcons(" \n\t "), icons);
  assert.deepEqual(searchIcons("  TeRmInAl \n").map(icon => icon.id), ["terminal"]);
  assert.ok(searchIcons("命令行").some(icon => icon.id === "terminal"));
  for (const category of categories) {
    const expected = icons.filter(icon => icon.category === category);
    assert.deepEqual(searchIcons("", category), expected);
    for (const locale of ["en", "zh-Hans"] as const) {
      assert.deepEqual(searchIcons(translate(locale, `category.${category}`)), expected);
    }
  }
  assert.deepEqual(searchIcons("terminal", "media"), []);
  assert.deepEqual(searchIcons("no-such-icon"), []);
});
