import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { errorMessage, translate, type Locale, type Session } from "./i18n";
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
  return (
    <main className="mx-auto flex min-h-screen max-w-lg flex-col gap-8 px-8 py-10">
      <header>
        <p className="eyebrow">{t("app.brand")}</p>
        <h1>{t("app.heading")}</h1>
        <p className="muted">{t("app.description")}</p>
      </header>
      <section className="empty" aria-labelledby="target-heading">
        <h2 id="target-heading">{t("entry.heading")}</h2>
        <div className="flex flex-wrap gap-3">
          <button disabled={entry?.choosing || entry?.reading || entry?.checking} onClick={() => { void invoke("choose_targets").catch(error => setFailure({ error })); }}>{t("entry.choose")}</button>
          <button disabled={entry?.choosing || entry?.reading || entry?.checking} onClick={() => { void invoke("finder_selection").catch(error => setFailure({ error })); }}>{t("entry.finder")}</button>
        </div>
        {entry?.reading && <p role="status">{t("entry.reading")}</p>}
        {entry?.error && <p role="alert">{t(`entry.${entry.error}`)}</p>}
        {entry?.items.length ? <>
          <p>{t(`entry.${entry.source}`)} · {new Intl.NumberFormat(locale).format(entry.items.length)}</p>
          <ul>{entry.items.map(item => <li key={item.path}>
            <details><summary>{item.path.substring(item.path.lastIndexOf("/") + 1)} · {t(`entry.${item.kind}`)}</summary><p className="break-all">{item.path}</p></details>
          </li>)}</ul>
        </> : <p>{t("entry.empty")}</p>}
      </section>
      <p role="status" className="muted text-sm">{failure ? errorMessage(locale, failure.error) : session
        ? translate(locale, "connection.connected", { version: session.version })
        : t("connection.connecting")}</p>
      <button className="self-end text-sm" onClick={() => { void invoke("quit_app").catch(error => setFailure({ error })); }}>{t("menu.quit")}</button>
    </main>
  );
}
