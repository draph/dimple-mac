import test from "node:test";
import assert from "node:assert/strict";
import {
  checkForUpdate,
  compareVersions,
  fetchReleases,
  latestRelease,
  parseReleases,
  parseVersion,
  type Release,
} from "./update.ts";

const release = (
  tag: string,
  extra: Record<string, unknown> = {},
): Record<string, unknown> => ({
  tag_name: tag,
  draft: false,
  prerelease: false,
  ...extra,
});

test("release version parsing accepts plain numeric versions only", () => {
  assert.deepEqual(parseVersion("1.2.3"), [1, 2, 3]);
  assert.deepEqual(parseVersion("v10.0.12"), [10, 0, 12]);
  for (const value of [
    "",
    "v1.2",
    "1.2.3.4",
    "v01.2.3",
    "1.02.3",
    "1.2.03",
    "1.2.3-beta.1",
    "1.2.3+meta",
    " 1.2.3",
    "1.2.3/../4",
    "99999999999.0.0",
  ]) {
    assert.equal(parseVersion(value), null, value);
  }
});

test("release versions compare numerically", () => {
  assert.equal(compareVersions("1.0.0", "1.0.0"), 0);
  assert.equal(compareVersions("1.0.0", "v1.0.1"), -1);
  assert.equal(compareVersions("2.0.0", "1.9.9"), 1);
  assert.equal(compareVersions("1.10.0", "1.9.0"), 1);
  assert.equal(compareVersions("bad", "1.0.0"), null);
});

test("release payloads drop drafts, invalid tags and malformed items", () => {
  const releases = parseReleases([
    release("v1.0.0"),
    release("1.2.0", { prerelease: true }),
    release("v0.9.0", { draft: true }),
    release("updates"),
    release("v1.1.0-beta.1"),
    null,
    "v3.0.0",
    { draft: false },
  ]);
  assert.deepEqual(releases, [
    { version: "1.2.0", tag: "1.2.0", prerelease: true },
    { version: "1.0.0", tag: "v1.0.0", prerelease: false },
  ]);
  assert.throws(() => parseReleases({ message: "not found" }));
});

test("the highest release wins and stable release beats an equal prerelease", () => {
  const releases: Release[] = [
    { version: "1.0.0", tag: "v1.0.0", prerelease: false },
    { version: "1.2.0", tag: "v1.2.0", prerelease: true },
    { version: "1.1.0", tag: "v1.1.0", prerelease: false },
  ];
  assert.equal(latestRelease(releases)?.tag, "v1.2.0");
  assert.equal(
    latestRelease([
      { version: "2.0.0", tag: "v2.0.0", prerelease: true },
      { version: "2.0.0", tag: "2.0.0", prerelease: false },
    ])?.tag,
    "2.0.0",
  );
  assert.equal(latestRelease([]), null);
});

test("update checks distinguish current, newer, missing and invalid versions", () => {
  const current = [
    { version: "1.0.0", tag: "v1.0.0", prerelease: false },
    { version: "1.0.1", tag: "v1.0.1", prerelease: false },
  ];
  assert.deepEqual(checkForUpdate("v1.0.1", current), {
    kind: "up-to-date",
    current: "1.0.1",
  });
  assert.deepEqual(checkForUpdate("1.0.0", current), {
    kind: "available",
    current: "1.0.0",
    latest: current[1],
  });
  assert.deepEqual(checkForUpdate("1.0.0", [current[0]]), {
    kind: "up-to-date",
    current: "1.0.0",
  });
  assert.deepEqual(checkForUpdate("1.0.0", []), { kind: "none" });
  assert.deepEqual(checkForUpdate("test", current), { kind: "invalid" });
});

test("the release request sends no credentials and rejects API errors", async () => {
  const ok = (async () =>
    new Response(
      JSON.stringify([release("v1.2.3", { prerelease: true })]),
      { status: 200, headers: { "Content-Type": "application/json" } },
    )) as typeof fetch;
  assert.deepEqual(await fetchReleases(undefined, ok), [
    { version: "1.2.3", tag: "v1.2.3", prerelease: true },
  ]);

  const unavailable = (async () =>
    new Response("rate limited", { status: 403 })) as typeof fetch;
  await assert.rejects(fetchReleases(undefined, unavailable));

  const malformed = (async () =>
    new Response(JSON.stringify({ message: "not found" }), {
      status: 200,
    })) as typeof fetch;
  await assert.rejects(fetchReleases(undefined, malformed));
});
