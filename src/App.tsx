import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { errorMessage, translate, type Locale, type Session } from "./i18n";
import { Preview, exportImages } from "./Preview";
import "./App.css";

import type { Entry, OperationState, PreparedOperation, PrepareOperation } from "./contracts";
export default function App() {
  const [locale, setLocale] = useState<Locale>("en");
  const [session, setSession] = useState<Session | null>(null);
  const [failure, setFailure] = useState<{ error: unknown } | null>(null);
  const [entry, setEntry] = useState<Entry | null>(null);
  const [operation, setOperation] = useState<OperationState>({ revision: -1, prepared: null, progress: null });
  const [previewReady, setPreviewReady] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const submitGuard = useRef(false);
  const dialog = useRef<HTMLDialogElement>(null);
  const refreshOperation = async () => {
    const state = await invoke<OperationState>("get_operation");
    setOperation(old => {
      if (state.revision < old.revision) return old;
      if (old.progress && state.progress?.operationId === old.progress.operationId && state.progress.sequence < old.progress.sequence) return old;
      return state;
    });
  };
  const t = (key: string) => translate(locale, key);
  useEffect(() => {
    let active = true;
    const refresh = () => Promise.all([refreshOperation(), invoke<Entry>("get_entry").then(value => {
      if (active) setEntry(old => !old || value.revision >= old.revision ? value : old);
    })]).catch((error: unknown) => active && setFailure({ error }));
    const listener = listen("entry-changed", refresh);
    listener.then(refresh).catch((error: unknown) => active && setFailure({ error }));
    invoke<Session>("get_session").then(
      (value) => {
        if (active) {
          document.documentElement.lang = value.locale;
          setLocale(value.locale);
          setSession(value);
        }
      },
      (error: unknown) => active && setFailure({ error }),
    );
    return () => { active = false; void listener.then(unlisten => unlisten()).catch(() => {}); };
  }, []);
  const prepared = operation.prepared;
  const progress = operation.progress;
  const running = !!progress && progress.phase !== "completed";
  useEffect(() => {
    if (prepared?.confirmationToken && !dialog.current?.open) dialog.current?.showModal();
    if (!prepared?.confirmationToken && dialog.current?.open) dialog.current.close();
  }, [prepared]);
  const busy = submitting || !!prepared || running || !entry || !session || entry.choosing || entry.reading || entry.checking || entry.preparing;
  const command = (name: string, args?: Record<string, unknown>) => {
    setFailure(null);
    void invoke(name, args).catch(error => setFailure({ error }));
  };
  const commit = async (p: PreparedOperation) => {
    await invoke("commit_operation", { operationId: p.operationId, confirmationToken: p.confirmationToken });
    await refreshOperation();
  };
  const start = async (restore: boolean) => {
    if (submitGuard.current || busy || !entry || !entry.items.length || progress || (!restore && !previewReady)) return;
    submitGuard.current = true; setSubmitting(true); setFailure(null);
    const revision = entry.revision;
    const targetIds = entry.items.map(item => item.id);
    try {
      const action: PrepareOperation["action"] = restore ? { type: "restore" } : { type: "apply", templateId: "big-sur-light-blue", badgeId: "code", params: { scale: 1, offsetX: 0, offsetY: 0 }, images: await exportImages() };
      const p = await invoke<PreparedOperation>("prepare_operation", { request: JSON.stringify({ revision, targetIds, action }) });
      if (p.confirmationToken) await refreshOperation();
      else await commit(p);
    } catch (error) { setFailure({ error }); await refreshOperation().catch(() => {}); }
    finally { submitGuard.current = false; setSubmitting(false); }
  };
  const confirm = async (cancel: boolean) => {
    if (!prepared || submitGuard.current) return;
    submitGuard.current = true; setSubmitting(true); setFailure(null);
    try {
      if (cancel) { await invoke("cancel_prepared", { operationId: prepared.operationId }); await refreshOperation(); }
      else await commit(prepared);
    } catch (error) { setFailure({ error }); }
    finally { submitGuard.current = false; setSubmitting(false); }
  };
  const name = (path: string) => path.substring(path.lastIndexOf("/") + 1) || path;
  return (
    <main className="panel">
      <header className="panel-header">
        <h1>{t("app.name")}</h1>
        <button className="quiet" onClick={() => command("quit_app")}>{t("menu.quit")}</button>
      </header>
      <section className="target card" aria-labelledby="target-heading" aria-busy={entry?.choosing || entry?.reading || entry?.checking || entry?.preparing}>
        <h2 id="target-heading">{t("entry.heading")}</h2>
        {entry?.items.length ? <>
          <p className="target-summary" title={entry.items.length === 1 ? entry.items[0].path : undefined}>
            {entry.items.length === 1 ? name(entry.items[0].path) : translate(locale, "entry.count", { count: entry.items.length })}
          </p>
          <details>
            <summary>{t("entry.details")}</summary>
            <ul className="target-list">{entry.items.map((item) => <li key={item.id}>
              <div><strong>{name(item.path)}</strong><p className="path">{item.path}</p>{item.linked && <><p className="path">{translate(locale, "entry.actualPath", { path: item.actualPath })}</p><p>{t("entry.linkConfirmation")}</p></>}</div>
              <button className="quiet" disabled={busy} aria-label={translate(locale, "entry.removeName", { name: name(item.path) })}
                onClick={() => command("remove_target", { expectedRevision: entry.revision, targetId: item.id })}>{t("entry.remove")}</button>
            </li>)}</ul>
          </details>
        </> : <p className="empty">{t("entry.empty")}</p>}
        <div className="target-actions">
          <button disabled={busy} onClick={() => command("choose_targets")}>{t("entry.choose")}</button>
          <button className="quiet" disabled={busy} onClick={() => command("finder_selection")}>{t("entry.finder")}</button>
        </div>
        {entry?.reading && <p role="status" className="muted">{t("entry.reading")}</p>}
        {entry?.checking && <p role="status" className="muted">{t("entry.checking")}</p>}
        {entry?.error && <p role="alert" className="error">{errorMessage(locale, entry.error)}</p>}
        {!!entry?.issues.length && <ul aria-label={t("entry.invalidItems")}>{entry.issues.map((issue, index) => <li key={`${index}:${issue.path}`}><span className="path">{issue.path}</span><p role="alert">{errorMessage(locale, issue.error)}</p></li>)}</ul>}
      </section>
      <section className="badge-section" aria-labelledby="badge-heading">
        <h2 id="badge-heading">{t("badge.heading")}</h2>
        <button disabled={busy} className="badge-choice" aria-pressed="true"><span aria-hidden="true">&lt;/&gt;</span>{t("badge.code")}</button>
      </section>
      <section aria-labelledby="preview-heading">
        <h2 id="preview-heading">{t("preview.heading")}</h2>
        <Preview locale={locale} onReady={setPreviewReady} />
      </section>
      {failure && <p role="alert" className="error">{errorMessage(locale, failure.error)}</p>}
      {progress && <section className="card" aria-live="polite" aria-atomic="true">
        <p>{translate(locale, progress.phase === "completed" ? "operation.result" : "operation.progress", {
          completed: progress.items.filter(item => item.status === "succeeded" || item.status === "failed").length,
          total: progress.items.length,
          succeeded: progress.items.filter(item => item.status === "succeeded").length,
          failed: progress.items.filter(item => item.status === "failed").length,
          pending: progress.items.filter(item => item.status === "pending").length,
          unknown: progress.items.filter(item => item.status === "unknown").length,
        })}</p>
        {progress.phase === "checking" && <p>{t("operation.checking")}</p>}
        {progress.slow && <p>{t("operation.slow")}</p>}
        {progress.items.some(item => item.status === "unknown") && <p role="alert">{t("operation.uncertain")}</p>}
        {progress.stopped && <p>{t("operation.stopped")}</p>}
        {progress.items.filter(item => item.status === "running" || item.status === "unknown").map(item => <p className="path" key={item.targetId}>{entry?.items.find(t => t.id === item.targetId)?.path}</p>)}
        <details><summary>{t("operation.details")}</summary><ul className="operation-results">{progress.items.map(item => <li key={item.targetId}>
          <p className="path">{entry?.items.find(t => t.id === item.targetId)?.path}</p><p>{t(`operation.${item.status}`)}</p>
          {item.error && <p className="error">{errorMessage(locale, item.error)}</p>}
        </li>)}</ul></details>
      </section>}
      {prepared && !prepared.confirmationToken && <button disabled={submitting} onClick={() => void confirm(true)}>{t("operation.cancel")}</button>}
      <footer>
        <button className="quiet" disabled={busy || !entry?.items.length || !!progress} onClick={() => void start(true)}>{t("operation.restore")}</button>
        <button className="primary" disabled={busy || !entry?.items.length || !previewReady || !!progress} onClick={() => void start(false)}>{t("apply.action")}</button>
      </footer>
      <dialog ref={dialog} aria-labelledby="confirm-title" aria-describedby="confirm-warning" onCancel={event => { event.preventDefault(); void confirm(true); }}>
        <h2 id="confirm-title">{t(prepared?.restore ? "operation.confirmRestore" : "operation.confirmApply")}</h2>
        <p id="confirm-warning">{t(prepared?.restore ? "operation.restoreWarning" : "operation.overwriteWarning")}</p>
        <p>{translate(locale, "operation.confirmCount", { total: prepared?.targetIds.length ?? 0, overwrite: prepared?.overwriteTargetIds.length ?? 0, linked: prepared?.linkedTargetIds.length ?? 0 })}</p>
        <ul className="confirmation-targets">{prepared?.targetIds.map(id => {
          const target = entry?.items.find(item => item.id === id);
          return <li key={id}><p className="path">{target?.path}</p>
            {prepared.overwriteTargetIds.includes(id) && <p>{t("operation.overwrite")}</p>}
            {target?.linked && <p className="path">{translate(locale, "entry.actualPath", { path: target.actualPath })}</p>}
          </li>;
        })}</ul>
        {failure && <p role="alert" className="error">{errorMessage(locale, failure.error)}</p>}
        <div className="target-actions"><button autoFocus disabled={submitting} onClick={() => void confirm(true)}>{t("operation.cancel")}</button><button className="primary" disabled={submitting} onClick={() => void confirm(false)}>{t("operation.confirm")}</button></div>
      </dialog>
    </main>
  );
}
