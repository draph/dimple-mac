import type { Session } from "./i18n.ts";

export function acceptSession(previous: Session | null, next: Session): Session {
  if (!previous) return next;
  if (next.entry.revision < previous.entry.revision) return previous;
  const old = previous.operation.progress;
  const incoming = next.operation.progress;
  if (old && incoming?.operationId === old.operationId && incoming.sequence < old.sequence) return previous;
  return next;
}

export function sameContext(previous: Session | null, latest: Session | null): boolean {
  return previous?.entry.editRevision === latest?.entry.editRevision
    && previous?.operation.progress?.operationId === latest?.operation.progress?.operationId
    && previous?.operation.prepared?.operationId === latest?.operation.prepared?.operationId;
}

export function sameDraft(draft: Session, latest: Session | null): boolean {
  return !!latest && draft.entry.editRevision === latest.entry.editRevision
    && draft.entry.items.length === latest.entry.items.length
    && draft.entry.items.every((item, index) => item.id === latest.entry.items[index].id)
    && !latest.operation.prepared && !latest.operation.progress
    && !latest.entry.choosing && !latest.entry.reading && !latest.entry.checking && !latest.entry.preparing;
}
