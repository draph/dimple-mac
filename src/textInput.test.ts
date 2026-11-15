import assert from "node:assert/strict";
import test from "node:test";
import { countText, draftText, limitText, syncedText } from "./textInput.ts";

test("counts user-visible characters instead of UTF-16 units", () => {
  for (const [text, count] of [
    ["", 0],
    ["中文", 2],
    ["a\u{301}", 1],
    ["👨‍👩‍👧‍👦", 1],
    ["🇨🇳", 1],
    ["A中12", 4],
  ] as const) {
    assert.equal(countText(text), count);
  }
});

test("keeps composition drafts whole and ignores lagging backend text", () => {
  assert.equal(draftText("gongju", true), "gongju");
  assert.equal(draftText("gongju", false), "gong");
  assert.equal(draftText("工具", false), "工具");
  assert.equal(syncedText("gong", true), null);
  assert.equal(syncedText("gong", false), "gong");
  assert.equal(syncedText(null, false), "");
});

test("limits text without splitting grapheme clusters", () => {
  assert.equal(limitText("12345"), "1234");
  assert.equal(limitText("中文字符多"), "中文字符");
  assert.equal(limitText("1234"), "1234");
  assert.equal(limitText(""), "");
  assert.equal(limitText("👨‍👩‍👧‍👦".repeat(5)), "👨‍👩‍👧‍👦".repeat(4));
  assert.equal(limitText("e\u{301}".repeat(5)), "e\u{301}".repeat(4));
});
