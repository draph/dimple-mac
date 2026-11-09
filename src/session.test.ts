import assert from "node:assert/strict";
import test from "node:test";
import { acceptSession, sameContext, sameDraft } from "./session.ts";
import type { Session } from "./i18n.ts";

function session(revision = 1): Session {
  return {
    version: "0.1.0", locale: "en",
    entry: { params: {scale: 1, offsetX: 0, offsetY: 0, theme: "native"}, badge: { id: "code", width: 384, height: 256, opaque: false, text: null }, badgeError: null, importing: false, text: null, textCount: null, textPending: false, revision, editRevision: 1, pinned: false, source: "picker", items: [{ id: "one", path: "/tmp/one", actualPath: "/tmp/one", linked: false, kind: "directory", error: null }], issues: [], error: null, choosing: false, reading: false, checking: false, preparing: false },
    operation: { canStop: false, canRetry: false, canContinue: false, revision, prepared: null, progress: null },
  };
}

test("atomic snapshots reject stale results and same-operation sequence regressions", () => {
  const current = session(2);
  const stale = session(1);
  assert.equal(acceptSession(current, stale), current);
  current.operation.progress = { operationId: "old", sequence: 2, phase: "running", slow: false, stopped: false, items: [] };
  const next = structuredClone(current);
  next.entry.revision = next.operation.revision = 3;
  next.operation.progress!.sequence = 1;
  assert.equal(acceptSession(current, next), current);
  next.operation.progress!.operationId = "new";
  assert.equal(acceptSession(current, next), next);
  assert.equal(acceptSession(null, next), next);
});

test("late operation snapshots cannot resurrect results cleared by a new edit", () => {
  const result = session();
  result.operation.progress = { operationId: "old", sequence: 99, phase: "completed", slow: false, stopped: false, items: [] };
  const edited = session(2);
  edited.entry.editRevision = 2;
  assert.equal(acceptSession(edited, result), edited);
});

test("exports survive notifications but reject accepted edits and target replacement", () => {
  const draft = session();
  const updated = session(2);
  updated.entry.pinned = true;
  assert.ok(sameDraft(draft, updated));
  assert.ok(!sameDraft(draft, null));
  updated.entry.editRevision++;
  assert.ok(!sameDraft(draft, updated));
  updated.entry.editRevision--;
  updated.entry.items[0].id = "new";
  assert.ok(!sameDraft(draft, updated));
  updated.entry.items = [];
  assert.ok(!sameDraft(draft, updated));
});

test("exports cannot enter preparation while another input or operation owns the session", () => {
  const draft = session();
  for (const key of ["choosing", "reading", "checking", "preparing", "importing"] as const) {
    const updated = session();
    updated.entry[key] = true;
    assert.ok(!sameDraft(draft, updated));
  }
  const updated = session();
  updated.operation.progress = { operationId: "in-flight", sequence: 0, phase: "running", slow: false, stopped: false, items: [] };
  assert.ok(!sameDraft(draft, updated));
});

test("late errors belong only to the same edit and operation context", () => {
  const before = session();
  assert.ok(sameContext(before, session(2)));
  const after = session(2);
  after.entry.editRevision++;
  assert.ok(!sameContext(before, after));
  after.entry.editRevision--;
  after.operation.progress = { operationId: "new", sequence: 1, phase: "running", slow: false, stopped: false, items: [] };
  assert.ok(!sameContext(before, after));
  assert.ok(!sameContext(null, before));
});

test("invalid imports block old exports but allow restore without discarding the draft", () => {
  const draft = session();
  const failed = session(2);
  failed.entry.badgeError = { code: "asset_error", params: { reason: "empty" } };
  assert.ok(!sameDraft(draft, failed));
  assert.ok(sameDraft(draft, failed, true));
  assert.ok(sameContext(draft, failed));
  failed.entry.badge.id = "replacement";
  assert.ok(!sameDraft(draft, failed, true));
});
