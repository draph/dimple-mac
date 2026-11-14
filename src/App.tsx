import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { errorMessage, translate, type Session } from "./i18n";
import { Preview, exportImages } from "./Preview";
import "./App.css";
import { icons, categories, searchIcons } from "./icons";

const thumbnails = import.meta.glob<string>("./assets/*.svg", { query: "?url", import: "default", eager: true });
const badgeTabs = ["icon", "text", "image"] as const;

import { acceptSession, sameContext, sameDraft, settingsBlocked } from "./session";
import type { Effect, PreparedOperation, PrepareOperation, RenderParams } from "./contracts";
import { TextBadge } from "./TextBadge";
import { Adjustments } from "./Adjustments";
import { ShortcutSettings } from "./ShortcutSettings";
import { FolderSelectionMenu } from "./FolderSelectionMenu";
import { defaultParams } from "./renderParams";
import { operationSummary } from "./operationSummary";
import type { AppearanceController } from "./appearance";
import { acceptLanguage, type LanguagePreference, type LanguageStatus } from "./language";
export default function App({ appearance }: { appearance: AppearanceController }) {
  const [session, setSession] = useState<Session | null>(null);
  const current = useRef<Session | null>(null);
  const epoch = useRef(0);
  const [language, setLanguage] = useState<LanguageStatus | null>(null);
  const [sessionLoaded, setSessionLoaded] = useState(false);
  const [languageLoaded, setLanguageLoaded] = useState(false);
  const languageRef = useRef<LanguageStatus | null>(null);
  const applyLanguage = useCallback((value: LanguageStatus) => {
    const accepted = acceptLanguage(languageRef.current, value);
    if (accepted === languageRef.current) return;
    languageRef.current = accepted;
    setLanguage(accepted);
  }, []);
  const [failure, setFailure] = useState<{ error: unknown } | null>(null);
  const [localParams, setLocalParams] = useState<RenderParams | null>(null);
  const [paramsPending, setParamsPending] = useState(false);
  const paramsEpoch = useRef(0);
  const onParamsChange = useCallback((params: RenderParams | null, pending: boolean) => {
    ++paramsEpoch.current; setLocalParams(params); setParamsPending(pending);
  }, []);
  const [readyBadgeId, setReadyBadgeId] = useState<string | null>(null);
  const [imported, setImported] = useState<{ id: string; url: string } | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [adjustmentsOpen, setAdjustmentsOpen] = useState(false);
  const settingsButton = useRef<HTMLButtonElement>(null);
  const [badgeTab, setBadgeTab] = useState<(typeof badgeTabs)[number]>("icon");
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
    setSession(current.current);
    return current.current;
  }, []);
  const locale = language?.locale ?? session?.locale ?? "en";
  useEffect(() => { document.documentElement.lang = locale; }, [locale]);
  const matchingIcons = searchIcons(query, category);
  const entry = session?.entry;
  const badgeId = entry?.badge.id ?? "code";
  const badgeSource = imported?.id === badgeId ? imported.url : null;
  const imageBadge = !!entry && entry.badge.text == null && !icons.some(icon => icon.id === badgeId);
  const renderParams = localParams ?? entry?.params ?? defaultParams;
  const previewReady = !paramsPending && readyBadgeId === JSON.stringify([badgeId, renderParams]) && !!badgeSource && !entry?.badgeError && !entry?.importing && !textQueued;
  const textBadgeError = entry?.badgeError?.code === "text_error" ? entry.badgeError : null;
  const imageBadgeError = entry?.badgeError && entry.badgeError.code !== "text_error" ? entry.badgeError : null;
  const errorTab = entry?.badgeError ? (entry.badgeError.code === "text_error" ? "text" : "image") : null;
  const textErrorRecoverable = ["missing", "render", "timeout"].includes(String(textBadgeError?.params.reason ?? ""));
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
      }).finally(() => {
        if (generation === epoch.current) setSessionLoaded(true);
      });
    };
    const listener = listen("entry-changed", refresh);
    void listener.then(refresh).catch((error: unknown) => {
      if (generation === epoch.current) { setFailure({ error }); setSessionLoaded(true); }
    });
    return () => { ++epoch.current; void listener.then(unlisten => unlisten()).catch(() => {}); };
  }, [refreshSession]);
  useEffect(() => {
    let active = true;
    const listener = listen<LanguageStatus>("language-changed", event => {
      if (active) applyLanguage(event.payload);
    });
    void listener.then(unlisten => {
      if (!active) { unlisten(); return; }
      return invoke<LanguageStatus>("get_language").then(value => {
        if (active) applyLanguage(value);
      });
    }).catch((error: unknown) => {
      if (active) setFailure({ error });
    }).finally(() => {
      if (active) setLanguageLoaded(true);
    });
    return () => { active = false; void listener.then(unlisten => unlisten()).catch(() => {}); };
  }, [applyLanguage]);
  useEffect(() => {
    if (sessionLoaded && languageLoaded) {
      void invoke("panel_ready").catch(error => setFailure({ error }));
    }
  }, [sessionLoaded, languageLoaded]);
  const selectLanguage = useCallback(async (preference: LanguagePreference) => {
    applyLanguage(await invoke<LanguageStatus>("set_language", { preference }));
  }, [applyLanguage]);
  const prepared = operation?.prepared;
  const progress = operation?.progress;
  const running = !!progress && progress.phase !== "completed";
  useEffect(() => {
    if (prepared?.confirmationToken && !dialog.current?.open) dialog.current?.showModal();
    if (!prepared?.confirmationToken && dialog.current?.open) dialog.current.close();
  }, [prepared]);
  const busy = submitting || !!prepared || running || !entry || !session || entry.choosing || entry.reading || entry.checking || entry.preparing;
  const blockSettings = settingsBlocked(session, submitting);
  const showSettings = settingsOpen && !blockSettings;
  useEffect(() => { if (showSettings) setAdjustmentsOpen(false); }, [showSettings]);
  const wasSettingsOpen = useRef(false);
  useEffect(() => {
    if (wasSettingsOpen.current && !showSettings) settingsButton.current?.focus();
    wasSettingsOpen.current = showSettings;
  }, [showSettings]);
  const command = (name: string, args?: Record<string, unknown>) => {
    const generation = epoch.current;
    const before = current.current;
    setFailure(null);
    void invoke(name, args).then(() => refreshSession(generation)).catch(error => {
      if (generation === epoch.current && sameContext(before, current.current)) setFailure({ error });
    });
  };
  const recoverBadge = () => {
    if (!entry || busy || textQueued || paramsPending) return;
    command("select_badge", { expectedEditRevision: entry.editRevision, operationId: progress?.operationId ?? null, badgeId });
  };
  const effectGuard = useRef(false);
  const changeEffect = async (effect: Effect) => {
    if (effectGuard.current || busy || paramsPending || !entry) return;
    effectGuard.current = true;
    const before = current.current;
    const generation = epoch.current;
    onParamsChange({ ...entry.params, effect }, true);
    try {
      await invoke("set_render_params", { params: { ...entry.params, effect },
        expectedEditRevision: entry.editRevision, operationId: progress?.operationId ?? null });
      await refreshSession(generation);
    } catch (error) {
      await refreshSession(generation).catch(() => null);
      if (generation === epoch.current && sameContext(before, current.current)) setFailure({ error });
    } finally {
      effectGuard.current = false;
      onParamsChange(null, false);
    }
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
      const action: PrepareOperation["action"] = restore ? { type: "restore" } : { type: "apply", templateId: "big-sur-light-blue", badgeId, params: frozenParams, images: await exportImages(badgeSource!, frozenParams, entry.badge.opaque) };
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
      <header className="panel-header" hidden={showSettings}>
        <h1>{t("app.name")}</h1>
        <button ref={settingsButton} className="quiet settings-button" disabled={blockSettings} aria-label={t("settings.title")} onClick={() => command("open_settings")}>
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" aria-hidden="true"><path d="m9 3-1 3-3 1-2 5 2 5 3 1 1 3h6l1-3 3-1 2-5-2-5-3-1-1-3Z" /><circle cx="12" cy="12" r="3" /></svg>
        </button>
      </header>
      <ShortcutSettings locale={locale} version={session?.version ?? ""} blocked={!!session && blockSettings} onOpenChange={setSettingsOpen} appearance={appearance} language={language} onSelectLanguage={selectLanguage} />
      <div className="editor" hidden={showSettings}>
      <div className="editor-scroll">
      <section className="target card" aria-labelledby="target-heading" aria-busy={entry?.choosing || entry?.reading || entry?.checking || entry?.preparing}>
        <div className="target-overview">
        <div className="target-preview"><Preview locale={locale} source={badgeSource} badgeId={badgeId} params={renderParams} cover={entry?.badge.opaque ?? false} onReady={onPreviewReady} /><span className="sr-only">{t("preview.heading")}</span></div>
        <div className="target-info">
        <h2 id="target-heading">{t("entry.heading")}</h2>
        {entry?.items.length ? <>
          <p className="target-summary" title={entry.items.length === 1 ? entry.items[0].path : undefined}>
            {entry.items.length === 1 ? name(entry.items[0].path) : translate(locale, "entry.count", { count: entry.items.length })}
          </p>
          {entry.items.length === 1 ? <p className="target-path" title={entry.items[0].path}>{entry.items[0].path}</p> : <p className="muted">{t("preview.batch")}</p>}

        </> : <p className="empty">{t("entry.empty")}</p>}
        <div className="target-actions">
          <button disabled={busy || paramsPending} onClick={() => command("choose_targets")}>{t("entry.choose")}</button>
          <FolderSelectionMenu locale={locale} disabled={busy || paramsPending || showSettings} onSelect={() => command("finder_selection")} />
        </div>
        </div>
        </div>
        <div className="target-details">
        {!!entry?.items.length && <>
          <details>
            <summary>{t("entry.details")}</summary>
            {entry.items.length > 1 && <div className="target-list-actions"><button className="quiet" disabled={busy || paramsPending}
              onClick={() => command("clear_targets", { expectedRevision: entry.revision })}>{t("entry.removeAll")}</button></div>}
            <ul className="target-list">{entry.items.map((item) => <li key={item.id}>
              <div><strong>{name(item.path)}</strong><p className="path">{item.path}</p>{item.linked && <><p className="path">{translate(locale, "entry.actualPath", { path: item.actualPath })}</p><p>{t("entry.linkConfirmation")}</p></>}</div>
              <button className="quiet" disabled={busy || paramsPending} aria-label={translate(locale, "entry.removeName", { name: name(item.path) })}
                onClick={() => command("remove_target", { expectedRevision: entry.revision, targetId: item.id })}>{t("entry.remove")}</button>
            </li>)}</ul>
          </details>
        </>}
        {entry?.reading && <p role="status" className="muted">{t("entry.reading")}</p>}
        {entry?.checking && <p role="status" className="muted">{t("entry.checking")}</p>}
        {entry?.error && <p role="alert" className="error">{errorMessage(locale, entry.error)}</p>}
        {!!entry?.issues.length && <ul aria-label={t("entry.invalidItems")}>{entry.issues.map((issue, index) => <li key={`${index}:${issue.path}`}><span className="path">{issue.path}</span><p role="alert">{errorMessage(locale, issue.error)}</p></li>)}</ul>}
        </div>
      </section>
      <section className="badge-section" aria-labelledby="badge-heading">
        <div className="badge-toolbar">
        <h2 id="badge-heading" className="sr-only">{t("badge.heading")}</h2>
        <div className="badge-tabs" role="tablist" aria-label={t("badge.heading")}>
          {badgeTabs.map((tab, index) => <button key={tab} id={`badge-tab-${tab}`} role="tab" aria-selected={badgeTab === tab} aria-controls={`badge-panel-${tab}`} tabIndex={badgeTab === tab ? 0 : -1}
            onClick={() => { setAdjustmentsOpen(false); setBadgeTab(tab); }} onKeyDown={event => {
              if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
              event.preventDefault();
              const next = badgeTabs[event.key === "Home" ? 0 : event.key === "End" ? badgeTabs.length - 1 : (index + (event.key === "ArrowRight" ? 1 : -1) + badgeTabs.length) % badgeTabs.length];
              setAdjustmentsOpen(false); setBadgeTab(next); document.getElementById(`badge-tab-${next}`)?.focus();
            }}>{t(`badge.tab.${tab}`)}{errorTab === tab && <><span className="badge-tab-error" aria-hidden="true" /><span className="sr-only">{t("badge.hasError")}</span></>}</button>)}
        </div>
        {session && <Adjustments locale={locale} session={session} disabled={busy || textQueued || !!entry?.importing} refresh={refreshSession} onChange={onParamsChange} open={adjustmentsOpen} onOpenChange={setAdjustmentsOpen} />}
        </div>
        <div id="badge-panel-icon" role="tabpanel" aria-labelledby="badge-tab-icon" hidden={badgeTab !== "icon"}>
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
        </div>
        <div id="badge-panel-text" role="tabpanel" aria-labelledby="badge-tab-text" hidden={badgeTab !== "text"}>
        {session && <TextBadge locale={locale} session={session} disabled={busy || paramsPending} refresh={refreshSession} onPending={onTextPending} />}
        {textBadgeError && <div className="badge-error">
          <p role="alert" className="error">{errorMessage(locale, textBadgeError)}</p>
          {textErrorRecoverable && <button disabled={busy || textQueued || paramsPending} onClick={recoverBadge}>{t("badge.recover")}</button>}
        </div>}
        {entry?.badge.text != null && <p className="muted">{translate(locale, "text.active", { text: entry.badge.text })}</p>}
        </div>
        <div id="badge-panel-image" className="image-badge" role="tabpanel" aria-labelledby="badge-tab-image" hidden={badgeTab !== "image"}>
          <div className="image-import">
            {imageBadge && badgeSource && <img className="image-thumbnail" src={badgeSource} alt={t("badge.imported")} />}
            <button aria-describedby="badge-image-hint" disabled={busy || textQueued || paramsPending || entry?.importing} onClick={() => command("choose_badge")}>{t(imageBadge ? "badge.replace" : "badge.import")}</button>
          </div>
          <p id="badge-image-hint" className="muted">{t("badge.importHint")}</p>
          <fieldset className="badge-effects" disabled={!imageBadge || busy || textQueued || paramsPending || entry?.importing}>
            <legend>{t("badge.effect")}</legend>
            <label><input type="radio" name="badge-effect" value="engraved" checked={renderParams.effect === "engraved"} disabled={!entry?.badge.effects.includes("engraved")} aria-describedby={entry?.badge.opaque ? "badge-opaque-hint" : undefined} onChange={() => void changeEffect("engraved")} />{t("badge.effect.engraved")}</label>
            <label><input type="radio" name="badge-effect" value="original" checked={renderParams.effect === "original"} disabled={!entry?.badge.effects.includes("original")} onChange={() => void changeEffect("original")} />{t("badge.effect.original")}</label>
            <p className="muted">{t(renderParams.effect === "original" ? "badge.originalHint" : "badge.engravedHint")}</p>
          </fieldset>
          {entry?.badge.opaque && <p id="badge-opaque-hint" role="status" className="muted">{t("badge.opaque")}</p>}
          {imageBadgeError && <div className="badge-error">
            <p role="alert" className="error">{errorMessage(locale, imageBadgeError)}</p>
            <button disabled={busy || textQueued || paramsPending} onClick={recoverBadge}>{t("badge.recover")}</button>
          </div>}
        </div>
        {entry?.importing && <p role="status" className="muted">{t("badge.loading")}</p>}
      </section>
      </div>
      {(failure || progress || (prepared && !prepared.confirmationToken)) && <div className="operation-feedback">
      {failure && <p role="alert" className="error">{errorMessage(locale, failure.error)}</p>}
      {progress && <section className="card" aria-live="polite" aria-atomic="true">
        <p>{operationSummary(locale, progress)}</p>
        {progress.phase === "checking" && <p>{t("operation.checking")}</p>}
        {progress.slow && <p>{t("operation.slow")}</p>}
        {progress.items.some(item => item.status === "unknown") && <p role="alert">{t(progress.phase === "running" ? "operation.waiting" : "operation.uncertain")}</p>}
        {progress.stopped && <p>{t("operation.stopped")}</p>}
        {progress.items.filter(item => item.status === "running" || item.status === "unknown").map(item => <p className="path" key={item.targetId}>{entry?.items.find(t => t.id === item.targetId)?.path}</p>)}
        <details><summary>{t("operation.details")}</summary><ul className="operation-results">{progress.items.map(item => <li key={item.targetId}>
          <p className="path">{entry?.items.find(t => t.id === item.targetId)?.path}</p><p>{t(`operation.${item.status}`)}</p>
          {item.error && <p className="error">{errorMessage(locale, item.error)}</p>}
        </li>)}</ul></details>
      </section>}
      {prepared && !prepared.confirmationToken && <button disabled={submitting} onClick={() => void confirm(true)}>{t("operation.cancel")}</button>}
      </div>}
      <footer className="action-bar">
        {operation?.canStop && <button disabled={submitting} onClick={() => command("stop_operation", { operationId: progress!.operationId })}>{t("operation.stop")}</button>}
        {operation?.canRetry && <button disabled={busy || textQueued || paramsPending} onClick={() => void recover("retry_failed")}>{t("operation.retry")}</button>}
        {operation?.canContinue && <button disabled={busy || textQueued || paramsPending} onClick={() => void recover("continue_pending")}>{t("operation.continue")}</button>}
        {!progress && <>
        <button className="quiet" disabled={busy || textQueued || paramsPending || entry?.importing || !entry?.items.length || !!progress} onClick={() => void start(true)}>{t("operation.restore")}</button>
        <button className="primary" disabled={busy || !entry?.items.length || !previewReady || !!progress} onClick={() => void start(false)}>{t("apply.action")}</button>
        </>}
      </footer>
      </div>
      <dialog ref={dialog} aria-labelledby="confirm-title" aria-describedby="confirm-warning confirm-count" onCancel={event => { event.preventDefault(); void confirm(true); }}>
        <h2 id="confirm-title">{t(prepared?.restore ? "operation.confirmRestore" : "operation.confirmApply")}</h2>
        <div id="confirm-warning">{(prepared?.restore || !!prepared?.overwriteTargetIds.length) && <p>{t(prepared?.restore ? "operation.restoreWarning" : "operation.overwriteWarning")}</p>}</div>
        <p id="confirm-count">{translate(locale, "operation.confirmCount", { total: prepared?.targetIds.length ?? 0 })}</p>
        <ul className="confirmation-targets">{prepared?.targetIds.map(id => {
          const target = entry?.items.find(item => item.id === id);
          return <li key={id}><p className="path">{target?.path}</p>
            {!prepared.restore && prepared.overwriteTargetIds.includes(id) && <p>{t("operation.overwrite")}</p>}
            {target && target.path !== target.actualPath && <p className="path">{translate(locale, "operation.actualPath", { path: target.actualPath })}</p>}
          </li>;
        })}</ul>
        {failure && <p role="alert" className="error">{errorMessage(locale, failure.error)}</p>}
        <div className="target-actions"><button autoFocus disabled={submitting} onClick={() => void confirm(true)}>{t("operation.cancel")}</button><button className="primary" disabled={submitting} onClick={() => void confirm(false)}>{t("operation.confirm")}</button></div>
      </dialog>
    </main>
  );
}
