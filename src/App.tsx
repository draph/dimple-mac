import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { errorMessage, translate, type Locale, type Session } from "./i18n";
import { Preview } from "./Preview";
import "./App.css";

type Entry = { revision: number; source: string; items: { path: string; kind: string }[]; error: string | null; choosing: boolean; reading: boolean; checking: boolean };
export default function App() {
  const [locale, setLocale] = useState<Locale>("en");
  const [session, setSession] = useState<Session | null>(null);
  const [failure, setFailure] = useState<{ error: unknown } | null>(null);
  const [entry, setEntry] = useState<Entry | null>(null);
  const t = (key: string) => translate(locale, key);
  useEffect(() => {
    let active = true;
    const refresh = () => invoke<Entry>("get_entry").then(value => {
      if (active) setEntry(old => !old || value.revision >= old.revision ? value : old);
    }).catch((error: unknown) => active && setFailure({ error }));
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
  const busy = !entry || !session || entry.choosing || entry.reading || entry.checking;
  const command = (name: string, args?: Record<string, unknown>) => {
    setFailure(null);
    void invoke(name, args).catch(error => setFailure({ error }));
  };
  const name = (path: string) => path.substring(path.lastIndexOf("/") + 1) || path;
  return (
    <main className="panel">
      <header className="panel-header">
        <h1>{t("app.name")}</h1>
        <button className="quiet" onClick={() => command("quit_app")}>{t("menu.quit")}</button>
      </header>
      <section className="target card" aria-labelledby="target-heading" aria-busy={entry?.choosing || entry?.reading || entry?.checking}>
        <h2 id="target-heading">{t("entry.heading")}</h2>
        {entry?.items.length ? <>
          <p className="target-summary" title={entry.items.length === 1 ? entry.items[0].path : undefined}>
            {entry.items.length === 1 ? name(entry.items[0].path) : translate(locale, "entry.count", { count: entry.items.length })}
          </p>
          <details>
            <summary>{t("entry.details")}</summary>
            <ul className="target-list">{entry.items.map((item, index) => <li key={`${index}:${item.path}`}>
              <div><strong>{name(item.path)}</strong><p className="path">{item.path}</p></div>
              <button className="quiet" disabled={busy} aria-label={translate(locale, "entry.removeName", { name: name(item.path) })}
                onClick={() => command("remove_target", { expectedRevision: entry.revision, index })}>{t("entry.remove")}</button>
            </li>)}</ul>
          </details>
        </> : <p className="empty">{t("entry.empty")}</p>}
        <div className="target-actions">
          <button disabled={busy} onClick={() => command("choose_targets")}>{t("entry.choose")}</button>
          <button className="quiet" disabled={busy} onClick={() => command("finder_selection")}>{t("entry.finder")}</button>
        </div>
        {entry?.reading && <p role="status" className="muted">{t("entry.reading")}</p>}
        {entry?.checking && <p role="status" className="muted">{t("entry.checking")}</p>}
        {entry?.error && <p role="alert" className="error">{t(`entry.${entry.error}`)}</p>}
      </section>
      <section className="badge-section" aria-labelledby="badge-heading">
        <h2 id="badge-heading">{t("badge.heading")}</h2>
        <button className="badge-choice" aria-pressed="true"><span aria-hidden="true">&lt;/&gt;</span>{t("badge.code")}</button>
      </section>
      <section aria-labelledby="preview-heading">
        <h2 id="preview-heading">{t("preview.heading")}</h2>
        <Preview locale={locale} />
      </section>
      {failure && <p role="alert" className="error">{errorMessage(locale, failure.error)}</p>}
      <footer>
        <p className="muted" id="apply-help">{t("apply.unavailable")}</p>
        <button className="primary" disabled aria-describedby="apply-help">{t("apply.action")}</button>
      </footer>
    </main>
  );
}
