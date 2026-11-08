import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { errorMessage, translate, type Session } from "./i18n";

export function TextBadge({ session, disabled, refresh, onPending }: {
  session: Session;
  disabled: boolean;
  refresh: () => Promise<Session | null>;
  onPending: (pending: boolean) => void;
}) {
  const [text, setText] = useState(session.entry.text ?? "");
  const [working, setWorking] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const latest = useRef({ session, refresh, onPending });
  latest.current = { session, refresh, onPending };
  const queue = useRef<{ text: string; composing: boolean } | null>(null);
  const active = useRef(false);
  const sending = useRef(false);
  const composing = useRef(false);
  const enqueue = (text: string, composition: boolean) => {
    setText(text);
    setError(null);
    queue.current = { text, composing: composition };
    latest.current.onPending(true);
    if (sending.current) return;
    sending.current = true;
    setWorking(true);
    void (async () => {
      let context = latest.current.session;
      try {
        while (queue.current && active.current) {
          const input = queue.current;
          queue.current = null;
          await invoke("set_text_badge", { ...input, expectedEditRevision: context.entry.editRevision,
            operationId: context.operation.progress?.operationId ?? null });
          const next = await latest.current.refresh();
          if (!next || !active.current) break;
          if (next.entry.editRevision !== context.entry.editRevision + 1) { queue.current = null; break; }
          context = next;
        }
      } catch (error) {
        queue.current = null;
        if (active.current) setError(error);
      } finally {
        sending.current = false;
        if (active.current) { setWorking(false); latest.current.onPending(false); }
      }
    })();
  };
  useEffect(() => {
    active.current = true;
    // A reload ends the old WebView's composition; resume its in-memory draft.
    if (latest.current.session.entry.textPending) enqueue(latest.current.session.entry.text ?? "", false);
    return () => { active.current = false; queue.current = null; };
  }, []);
  useEffect(() => {
    if (!working) setText(session.entry.text ?? "");
  }, [session.entry.text, working]);
  useEffect(() => { setError(null); }, [session.entry.badge.id]);
  const t = (key: string) => translate(session.locale, key);
  return <div className="text-badge">
    <label htmlFor="badge-text">{t("text.label")}</label>
    <textarea id="badge-text" rows={1} value={text} disabled={disabled}
      aria-describedby="badge-text-hint badge-text-count" aria-invalid={!!session.entry.badgeError || !!error}
      onCompositionStart={() => { composing.current = true; }}
      onCompositionEnd={event => { composing.current = false; enqueue(event.currentTarget.value, false); }}
      onChange={event => enqueue(event.target.value, composing.current)} />
    <p id="badge-text-hint" className="muted">{t("text.hint")}</p>
    <p id="badge-text-count" className="muted" aria-live="polite">{!working && session.entry.text !== null && session.entry.textCount !== null
      ? translate(session.locale, "text.count", { count: session.entry.textCount }) : ""}</p>
    {error !== null && <p role="alert" className="error">{errorMessage(session.locale, error)}</p>}
  </div>;
}
