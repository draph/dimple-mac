import assert from "node:assert/strict";
import test from "node:test";
import { createAppearance } from "./appearance.ts";

function setup(saved: string | null = null, dark = false) {
  const media = Object.assign(new EventTarget(), { matches: dark });
  const root = { dataset: {} as DOMStringMap };
  const values = new Map<string, string>();
  if (saved !== null) values.set("appearance", saved);
  const storage = {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value); },
  };
  const controller = createAppearance(() => storage, media, root);
  return { media, root, storage, controller };
}

test("missing and invalid preferences follow the system without rewriting storage", () => {
  for (const saved of [null, "invalid", "", '"dark"']) {
    const { controller, root, storage, media } = setup(saved);
    assert.equal(controller.getSnapshot().preference, "system");
    assert.equal(root.dataset.theme, "light");
    media.matches = true;
    media.dispatchEvent(new Event("change"));
    assert.equal(root.dataset.theme, "dark");
    assert.equal(storage.getItem("appearance"), saved);
  }
});

test("manual preferences override system changes and are restored on reload", () => {
  for (const preference of ["dark", "light"] as const) {
    const { controller, root, media, storage } = setup(null, preference === "light");
    controller.select(preference);
    assert.equal(root.dataset.theme, preference);
    assert.equal(storage.getItem("appearance"), preference);
    media.matches = !media.matches;
    media.dispatchEvent(new Event("change"));
    assert.equal(root.dataset.theme, preference);
    const reloaded = { dataset: {} as DOMStringMap };
    createAppearance(() => storage, media, reloaded);
    assert.equal(reloaded.dataset.theme, preference);
    controller.select("system");
    assert.equal(root.dataset.theme, media.matches ? "dark" : "light");
    media.matches = !media.matches;
    media.dispatchEvent(new Event("change"));
    assert.equal(root.dataset.theme, media.matches ? "dark" : "light");
  }
});

test("unavailable storage falls back safely and failed writes keep the current appearance", () => {
  const media = Object.assign(new EventTarget(), { matches: true });
  const root = { dataset: {} as DOMStringMap };
  const controller = createAppearance(() => { throw new Error("Storage unavailable"); }, media, root);
  assert.equal(controller.getSnapshot().preference, "system");
  assert.equal(root.dataset.theme, "dark");
  controller.select("light");
  assert.deepEqual(controller.getSnapshot(), { preference: "light", saveFailed: true });
  assert.equal(root.dataset.theme, "light");
});

test("subscribers receive saved changes and failures clear after a successful retry", () => {
  const { controller, storage } = setup();
  let updates = 0;
  const unsubscribe = controller.subscribe(() => { ++updates; });
  const write = storage.setItem;
  storage.setItem = () => { throw new Error("Storage full"); };
  controller.select("dark");
  assert.equal(controller.getSnapshot().saveFailed, true);
  storage.setItem = write;
  controller.select("dark");
  assert.deepEqual(controller.getSnapshot(), { preference: "dark", saveFailed: false });
  assert.equal(updates, 2);
  unsubscribe();
  controller.select("light");
  assert.equal(updates, 2);
});
