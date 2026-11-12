import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

test("npm, Tauri and Cargo package versions agree", () => {
  const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");
  const npm = JSON.parse(read("../package.json")).version;
  const tauri = JSON.parse(read("../src-tauri/tauri.conf.json")).version;
  const cargoPackage = read("../src-tauri/Cargo.toml").split("[package]")[1]?.split(/^\[/m)[0];
  const cargo = cargoPackage?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  assert.ok(typeof npm === "string" && npm.length > 0);
  assert.equal(tauri, npm);
  assert.equal(cargo, npm);
});
