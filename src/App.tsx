import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { errorMessage, translate, type Session } from "./i18n";
import { Preview, exportImages } from "./Preview";
import "./App.css";
import { icons, categories, searchIcons } from "./icons";

const thumbnails = import.meta.glob<string>("./assets/*.svg", { query: "?url", import: "default", eager: true });

import { acceptSession, sameContext, sameDraft } from "./session";
import type { PreparedOperation, PrepareOperation, RenderParams } from "./contracts";
import { TextBadge } from "./TextBadge";
import { Adjustments } from "./Adjustments";
import { defaultParams } from "./renderParams";
export default function App() {
  const [session, setSession] = useState<Session | null>(null);
  const current = useRef<Session | null>(null);
  const epoch = useRef(0);
  const [failure, setFailure] = useState<{ error: unknown } | null>(null);
  const [localParams, setLocalParams] = useState<RenderParams | null>(null);
  const [paramsPending, setParamsPending] = useState(false);
  const paramsEpoch = useRef(0);
  const onParamsChange = useCallback((params: RenderParams | null, pending: boolean) => {
    ++paramsEpoch.current; setLocalParams(params); setParamsPending(pending);
  }, []);
  const [readyBadgeId, setReadyBadgeId] = useState<string | null>(null);
  const [imported, setImported] = useState<{ id: string; url: string } | null>(null);
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState("");
  const onPreviewReady = useCallback((id: string, ready: boolean) => setReadyBadgeId(ready ? id : null), []);
  const [submitting, setSubmitting] = useState(false);
  const [textQueued, setTextQueued] = useState(false);
  const textEpoch = useRef(0);
  const onTextPending = useCallback((pending: boolean) => { ++textEpoch.current; setTextQueued(pending); }, []);
  const submitGuard = useRef(false);
  const dialog = useRef<HTMLDialogElement>(null);
  const refreshSession = useCallback(async (generation = epoch.current) => {
    const value = await invoke<Session>("get_session");
    if (generation !== epoch.current) return null;
    const next = acceptSession(current.current, value);
    if (!sameContext(current.current, next)) setFailure(null);
    current.current = next;
    document.documentElement.lang = current.current.locale;
    setSession(current.current);
    return current.current;
  }, []);
  const locale = session?.locale ?? "en";
  const matchingIcons = searchIcons(query, category);
  const entry = session?.entry;
  const badgeId = entry?.badge.id ?? "code";
  const badgeSource = imported?.id === badgeId ? imported.url : null;
  const renderParams = localParams ?? entry?.params ?? defaultParams;
  const previewReady = !paramsPending && readyBadgeId === JSON.stringify([badgeId, renderParams]) && !!badgeSource && !entry?.badgeError && !entry?.importing && !textQueued;
  useEffect(() => {
    let active = true;
    let url: string | undefined;
    setImported(null);
    if (entry) {
      void invoke<string>("get_badge_mask", { badgeId }).then(png => {
        if (!active) return;
        const bytes = Uint8Array.from(atob(png), c => c.charCodeAt(0));
        url = URL.createObjectURL(new Blob([bytes], { type: "image/png" }));
        setImported({ id: badgeId, url });
      }).catch(error => { if (active) setFailure({ error }); });
    }
    return () => { active = false; if (url) URL.revokeObjectURL(url); };
  }, [badgeId, !!entry]);
  const operation = session?.operation;
  const t = (key: string) => translate(locale, key);
  useEffect(() => {
    const generation = ++epoch.current;
    const refresh = () => {
      const before = current.current;
      return refreshSession(generation).catch((error: unknown) => {
        if (generation === epoch.current && current.current === before) setFailure({ error });
      });
    };
    const listener = listen("entry-changed", refresh);
    void listener.then(refresh).catch((error: unknown) => {
      if (generation === epoch.current) setFailure({ error });
    });
    return () => { ++epoch.current; void listener.then(unlisten => unlisten()).catch(() => {}); };
  }, [refreshSession]);
  const prepared = operation?.prepared;
  const progress = operation?.progress;
  const running = !!progress && progress.phase !== "completed";
  useEffect(() => {
    if (prepared?.confirmationToken && !dialog.current?.open) dialog.current?.showModal();
    if (!prepared?.confirmationToken && dialog.current?.open) dialog.current.close();
  }, [prepared]);
  const busy = submitting || !!prepared || running || !entry || !session || entry.choosing || entry.reading || entry.checking || entry.preparing;
  const command = (name: string, args?: Record<string, unknown>) => {
    const generation = epoch.current;
    const before = current.current;
    setFailure(null);
    void invoke(name, args).then(() => refreshSession(generation)).catch(error => {
      if (generation === epoch.current && sameContext(before, current.current)) setFailure({ error });
    });
  };
  const commit = async (p: PreparedOperation, generation: number) => {
    if (generation !== epoch.current) return;
    await invoke("commit_operation", { operationId: p.operationId, confirmationToken: p.confirmationToken });
    await refreshSession(generation);
  };
  const start = async (restore: boolean) => {
    if (submitGuard.current || busy || textQueued || paramsPending || !entry?.items.length || progress || (!restore && !previewReady) || !session) return;
    submitGuard.current = true; setSubmitting(true); setFailure(null);
    const generation = epoch.current;
    const draft = session;
    const textVersion = textEpoch.current;
    const paramsVersion = paramsEpoch.current;
    const frozenParams = { ...renderParams };
    const targetIds = entry.items.map(item => item.id);
    try {
      const action: PrepareOperation["action"] = restore ? { type: "restore" } : { type: "apply", templateId: "big-sur-light-blue", badgeId, params: frozenParams, images: await exportImages(badgeSource!, frozenParams) };
      const latest = await refreshSession(generation);
      if (paramsVersion !== paramsEpoch.current || textVersion !== textEpoch.current || !sameDraft(draft, latest, restore)) return;
      const p = await invoke<PreparedOperation>("prepare_operation", { request: JSON.stringify({ revision: latest!.entry.revision, targetIds, action }) });
      if (generation !== epoch.current) return;
      if (p.confirmationToken) await refreshSession(generation);
      else await commit(p, generation);
    } catch (error) {
      if (generation === epoch.current && sameContext(draft, current.current)) setFailure({ error });
      await refreshSession(generation).catch(() => {});
    } finally {
      if (generation === epoch.current) { submitGuard.current = false; setSubmitting(false); }
    }
  };
  const recover = async (mode: "retry_failed" | "continue_pending") => {
    if (submitGuard.current || busy || textQueued || paramsPending || !progress || !session) return;
    const generation = epoch.current;
    const before = current.current;
    submitGuard.current = true; setSubmitting(true); setFailure(null);
    try {
      const p = await invoke<PreparedOperation>("prepare_recovery", { operationId: progress.operationId, expectedRevision: session.entry.revision, mode });
      if (generation !== epoch.current) return;
      if (p.confirmationToken) await refreshSession(generation);
      else await commit(p, generation);
    } catch (error) {
      if (generation === epoch.current && sameContext(before, current.current)) setFailure({ error });
      await refreshSession(generation).catch(() => {});
    } finally {
      if (generation === epoch.current) { submitGuard.current = false; setSubmitting(false); }
    }
  };
  const confirm = async (cancel: boolean) => {
    if (!prepared || submitGuard.current) return;
    const generation = epoch.current;
    const before = current.current;
    submitGuard.current = true; setSubmitting(true); setFailure(null);
    try {
      if (cancel) { await invoke("cancel_prepared", { operationId: prepared.operationId }); await refreshSession(generation); }
      else await commit(prepared, generation);
    } catch (error) { if (generation === epoch.current && sameContext(before, current.current)) setFailure({ error }); }
    finally { if (generation === epoch.current) { submitGuard.current = false; setSubmitting(false); } }
  };
  const name = (path: string) => path.substring(path.lastIndexOf("/") + 1) || path;
  return (
    <main className="panel">
      <header className="panel-header">
        <h1>{t("app.name")}</h1>
        <button className="quiet" disabled={!session} aria-pressed={entry?.pinned ?? false} onClick={() => command("set_pinned", { pinned: !entry?.pinned })}>{t("window.pin")}</button>
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
              <button className="quiet" disabled={busy || paramsPending} aria-label={translate(locale, "entry.removeName", { name: name(item.path) })}
                onClick={() => command("remove_target", { expectedRevision: entry.revision, targetId: item.id })}>{t("entry.remove")}</button>
            </li>)}</ul>
          </details>
        </> : <p className="empty">{t("entry.empty")}</p>}
        <div className="target-actions">
          <button disabled={busy || paramsPending} onClick={() => command("choose_targets")}>{t("entry.choose")}</button>
          <button className="quiet" disabled={busy || paramsPending} onClick={() => command("finder_selection")}>{t("entry.finder")}</button>
        </div>
        {entry?.reading && <p role="status" className="muted">{t("entry.reading")}</p>}
        {entry?.checking && <p role="status" className="muted">{t("entry.checking")}</p>}
        {entry?.error && <p role="alert" className="error">{errorMessage(locale, entry.error)}</p>}
        {!!entry?.issues.length && <ul aria-label={t("entry.invalidItems")}>{entry.issues.map((issue, index) => <li key={`${index}:${issue.path}`}><span className="path">{issue.path}</span><p role="alert">{errorMessage(locale, issue.error)}</p></li>)}</ul>}
      </section>
      <section className="badge-section" aria-labelledby="badge-heading">
        <h2 id="badge-heading">{t("badge.heading")}</h2>
        <div className="icon-filters">
          <input type="search" aria-label={t("icons.search")} placeholder={t("icons.search")} value={query} onChange={event => setQuery(event.target.value)} />
          <select aria-label={t("icons.category")} value={category} onChange={event => setCategory(event.target.value)}>
            <option value="">{t("icons.all")}</option>
            {categories.map(id => <option key={id} value={id}>{t(`category.${id}`)}</option>)}
          </select>
        </div>
        <div className="icon-grid" role="group" aria-label={t("badge.heading")}>
          {matchingIcons.map(icon => <button key={icon.id} disabled={busy || textQueued || paramsPending} className="badge-choice" aria-pressed={badgeId === icon.id}
            onClick={() => command("select_badge", { expectedEditRevision: entry!.editRevision, operationId: progress?.operationId ?? null, badgeId: icon.id })}>
            <img src={thumbnails[`./assets/${icon.id}.svg`]} alt="" />{t(icon.name)}
          </button>)}
        </div>
        {matchingIcons.length === 0 && <p role="status" className="empty">{t("icons.empty")}</p>}
        <div className="target-actions">
          <button disabled={busy || textQueued || paramsPending || entry?.importing} onClick={() => command("choose_badge")}>{t("badge.import")}</button>
        </div>
        {session && <TextBadge session={session} disabled={busy || paramsPending} refresh={refreshSession} onPending={onTextPending} />}
        {entry?.badge.text != null && <p className="muted">{translate(locale, "text.active", { text: entry.badge.text })}</p>}
        {!icons.some(icon => icon.id === badgeId) && entry?.badge.text == null && <p className="muted">{t("badge.imported")}</p>}
        {entry?.importing && <p role="status" className="muted">{t("badge.loading")}</p>}
        {entry?.badge.opaque && <p role="status" className="muted">{t("badge.opaque")}</p>}
        {entry?.badgeError && <><p role="alert" className="error">{errorMessage(locale, entry.badgeError)}</p><button disabled={busy || textQueued || paramsPending} onClick={() => command("select_badge", { expectedEditRevision: entry.editRevision, operationId: progress?.operationId ?? null, badgeId })}>{t("badge.recover")}</button></>}
      </section>
      <section aria-labelledby="preview-heading">
        <h2 id="preview-heading">{t("preview.heading")}</h2>
        <Preview locale={locale} source={badgeSource} badgeId={badgeId} params={renderParams} onReady={onPreviewReady} />
        {session && <Adjustments session={session} disabled={busy || textQueued || !!entry?.importing} refresh={refreshSession} onChange={onParamsChange} />}
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
        {operation?.canStop && <button disabled={submitting} onClick={() => command("stop_operation", { operationId: progress!.operationId })}>{t("operation.stop")}</button>}
        {operation?.canRetry && <button disabled={busy || textQueued || paramsPending} onClick={() => void recover("retry_failed")}>{t("operation.retry")}</button>}
        {operation?.canContinue && <button disabled={busy || textQueued || paramsPending} onClick={() => void recover("continue_pending")}>{t("operation.continue")}</button>}
        {!progress && <>
        <button className="quiet" disabled={busy || textQueued || paramsPending || entry?.importing || !entry?.items.length || !!progress} onClick={() => void start(true)}>{t("operation.restore")}</button>
        <button className="primary" disabled={busy || !entry?.items.length || !previewReady || !!progress} onClick={() => void start(false)}>{t("apply.action")}</button>
        </>}
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
