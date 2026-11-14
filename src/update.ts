// shortcut: only the newest 100 releases are scanned; add pagination if the release count grows.
const releasesEndpoint = "https://api.github.com/repos/draph/dimple-mac/releases?per_page=100";

export type Release = {
  version: string;
  tag: string;
  prerelease: boolean;
};

export type UpdateCheck =
  | { kind: "up-to-date"; current: string }
  | { kind: "available"; current: string; latest: Release }
  | { kind: "none" }
  | { kind: "invalid" };

const versionPattern =
  /^v?(0|[1-9]\d{0,9})\.(0|[1-9]\d{0,9})\.(0|[1-9]\d{0,9})$/;

export function parseVersion(value: string): [number, number, number] | null {
  const match = versionPattern.exec(value);
  if (!match) return null;
  const parts = match.slice(1).map(Number);
  if (parts.some(part => !Number.isSafeInteger(part) || part > 4_294_967_295)) return null;
  return [parts[0], parts[1], parts[2]];
}

function compareParsed(
  a: [number, number, number],
  b: [number, number, number],
): number {
  for (let index = 0; index < 3; index++) {
    if (a[index] !== b[index]) return a[index] - b[index];
  }
  return 0;
}

export function compareVersions(a: string, b: string): number | null {
  const left = parseVersion(a);
  const right = parseVersion(b);
  if (!left || !right) return null;
  return compareParsed(left, right);
}

export function parseReleases(payload: unknown): Release[] {
  if (!Array.isArray(payload)) throw new Error("Invalid release payload");
  const releases: Array<{ release: Release; parts: [number, number, number] }> = [];
  for (const item of payload) {
    if (typeof item !== "object" || item === null) continue;
    const record = item as Record<string, unknown>;
    if (record.draft === true || typeof record.tag_name !== "string") continue;
    const parts = parseVersion(record.tag_name);
    if (!parts) continue;
    releases.push({
      release: {
        version: parts.join("."),
        tag: record.tag_name,
        prerelease: record.prerelease === true,
      },
      parts,
    });
  }
  releases.sort((a, b) => compareParsed(b.parts, a.parts));
  return releases.map(({ release }) => release);
}

export function latestRelease(releases: Release[]): Release | null {
  let latest: Release | null = null;
  for (const release of releases) {
    if (!latest) {
      latest = release;
      continue;
    }
    const comparison = compareVersions(release.version, latest.version);
    if (comparison === null) continue;
    if (comparison > 0 || (comparison === 0 && latest.prerelease && !release.prerelease)) {
      latest = release;
    }
  }
  return latest;
}

export function checkForUpdate(current: string, releases: Release[]): UpdateCheck {
  const currentParts = parseVersion(current);
  if (!currentParts) return { kind: "invalid" };
  const latest = latestRelease(releases);
  if (!latest) return { kind: "none" };
  if (compareParsed(currentParts, parseVersion(latest.version) ?? [0, 0, 0]) >= 0) {
    return { kind: "up-to-date", current: currentParts.join(".") };
  }
  return { kind: "available", current: currentParts.join("."), latest };
}

export async function fetchReleases(
  signal?: AbortSignal,
  request: typeof fetch = fetch,
): Promise<Release[]> {
  // A unique query keeps the manual check from reusing a cached release list.
  const response = await request(`${releasesEndpoint}&_=${Date.now()}`, {
    signal,
    credentials: "omit",
    headers: { Accept: "application/vnd.github+json" },
  });
  if (!response.ok) throw new Error(`GitHub releases request failed: ${response.status}`);
  return parseReleases(await response.json());
}
