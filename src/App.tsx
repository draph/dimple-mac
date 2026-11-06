import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { errorMessage, translate, type Locale, type Session } from "./i18n";
import "./App.css";

export default function App() {
  const [locale, setLocale] = useState<Locale>("en");
  const [session, setSession] = useState<Session | null>(null);
  const [failure, setFailure] = useState<{ error: unknown } | null>(null);
  const t = (key: string) => translate(locale, key);
  useEffect(() => {
    let active = true;
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
    return () => { active = false; };
  }, []);
  return (
    <main className="mx-auto flex min-h-screen max-w-lg flex-col gap-8 px-8 py-10">
      <header>
        <p className="eyebrow">{t("app.brand")}</p>
        <h1>{t("app.heading")}</h1>
        <p className="muted">{t("app.description")}</p>
      </header>
      <section className="empty" aria-labelledby="target-heading">
        <span aria-hidden="true" className="folder">▱</span>
        <h2 id="target-heading">{t("prototype.heading")}</h2>
        <p className="muted">{t("prototype.description")}</p>
      </section>
      <p role="status" className="muted text-sm">{failure ? errorMessage(locale, failure.error) : session
        ? translate(locale, "connection.connected", { version: session.version })
        : t("connection.connecting")}</p>
    </main>
  );
}
