import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { errorMessage, translate, type Locale, type Session } from "./i18n";
import { countText, draftText, syncedText } from "./textInput";

export function TextBadge({ locale, session, disabled, refresh, onPending }: {
  locale: Locale;
  session: Session;
  disabled: boolean;
  refresh: () => Promise<Session | null>;
  onPending: (pending: boolean) => void;
}) {
  const [text, setText] = useState(() => syncedText(session.entry.text, false) ?? "");
  const [working, setWorking] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [isComposing, setIsComposing] = useState(false);
  const latest = useRef({ session, refresh, onPending });
  latest.current = { session, refresh, onPending };
  const queue = useRef<{ text: string; composing: boolean } | null>(null);
  const active = useRef(false);
  const sending = useRef(false);
  const composing = useRef(false);
  const committed = useRef(text);
  const currentText = useRef(text);
  const clearing = useRef(false);
  const input = useRef<HTMLTextAreaElement>(null);
  const changeText = (value: string) => { currentText.current = value; setText(value); };
  const enqueue = (value: string, composition: boolean, force = false) => {
    const draft = draftText(value, composition);
    // Never rewrite the DOM during composition; it would end or truncate the IME draft.
    if (!composition && input.current && input.current.value !== draft) input.current.value = draft;
    if (!force && !composition && draft === currentText.current) return;
    changeText(draft);
    setError(null);
    queue.current = { text: draft, composing: composition };
    latest.current.onPending(true);
    if (sending.current) return;
    sending.current = true;
    setWorking(true);
    void (async () => {
      let context = latest.current.session;
      try {
        while (queue.current && active.current) {
          const request = queue.current;
          queue.current = null;
          await invoke("set_text_badge", { ...request, expectedEditRevision: context.entry.editRevision,
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
    if (latest.current.session.entry.textPending) enqueue(latest.current.session.entry.text ?? "", false, true);
    return () => { active.current = false; queue.current = null; };
  }, []);
  useEffect(() => {
    if (working || sending.current || queue.current) return;
    const next = syncedText(session.entry.text, composing.current);
    if (next !== null) changeText(next);
  }, [session.entry.text, working]);
  useEffect(() => { setError(null); }, [session.entry.badge.id]);
  const clear = async () => {
    if (working || clearing.current || sending.current || queue.current) return;
    clearing.current = true;
    const context = latest.current.session;
    setError(null);
    try {
      await invoke("clear_badge", { expectedEditRevision: context.entry.editRevision,
        operationId: context.operation.progress?.operationId ?? null });
      await latest.current.refresh();
    } catch (error) { if (active.current) setError(error); }
    finally { clearing.current = false; }
  };
  const t = (key: string) => translate(locale, key);
  const shownCount = countText(isComposing ? committed.current : text);
  return <div className="text-badge">
    <label htmlFor="badge-text">{t("text.label")}</label>
    <div className="text-badge-input">
      <div className="text-badge-field">
        <textarea ref={input} id="badge-text" rows={1} value={text} disabled={disabled}
          aria-describedby="badge-text-hint badge-text-count" aria-invalid={session.entry.badgeError?.code === "text_error" || !!error}
          onCompositionStart={event => { composing.current = true; committed.current = event.currentTarget.value; setIsComposing(true); }}
          onCompositionEnd={event => { composing.current = false; setIsComposing(false); enqueue(event.currentTarget.value, false, true); }}
          onChange={event => enqueue(event.target.value, composing.current)} />
        {text.trim() !== "" && <button className="quiet" aria-label={t("text.clear")} title={t("text.clear")}
          disabled={disabled || working || session.entry.importing} onClick={() => void clear()}>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18" /></svg>
        </button>}
      </div>
      <span id="badge-text-count" className="muted text-badge-count" aria-live="polite">
        {translate(locale, "text.count", { count: shownCount })}
      </span>
    </div>
    <p id="badge-text-hint" className="muted">{t("text.hint")}</p>
    {error !== null && <p role="alert" className="error">{errorMessage(locale, error)}</p>}
  </div>;
}
