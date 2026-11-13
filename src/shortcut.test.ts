import assert from "node:assert/strict";
import test from "node:test";
import { captureShortcut, displayShortcut } from "./shortcut.ts";

test("capture uses physical codes, including Option-produced characters, and macOS symbols", () => {
  const event = { code: "KeyF", altKey: true, ctrlKey: false, metaKey: false, shiftKey: false, isComposing: false };
  assert.equal(captureShortcut(event), "alt+KeyF");
  assert.equal(displayShortcut("shift+control+alt+super+KeyF"), "⇧⌃⌥⌘F");
  assert.equal(displayShortcut("alt+ArrowUp"), "⌥↑");
  for (const code of ["AltLeft", "Escape", "Unidentified", "NumpadEnter"]) assert.equal(captureShortcut({ ...event, code }), null);
  assert.equal(captureShortcut({ ...event, isComposing: true }), null);
});
