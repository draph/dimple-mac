import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { errorMessage, translate, type Locale } from "./i18n";

export function AboutSettings({ locale, version }: { locale: Locale; version: string }) {
  const [failure, setFailure] = useState<unknown>(null);
  const t = (key: string) => translate(locale, key);
  return <section className="settings-card about" aria-labelledby="about-title">
    <div className="settings-card-heading">
      <span className="settings-icon about-icon" aria-hidden="true"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round"><circle cx="12" cy="12" r="9" /><path d="M12 11v6m0-10v.1" /></svg></span>
      <h2 id="about-title" className="settings-card-title">{t("about.title")}</h2>
    </div>
    <div className="about-row">
      <span>{translate(locale, "about.version", { version })}</span>
      <button type="button" className="check-updates" disabled aria-describedby="update-unavailable">{t("about.update")}</button>
      <span id="update-unavailable" className="sr-only">{t("about.unavailable")}</span>
    </div>
    <div className="about-row">
      <span>{t("about.github")}</span>
      <a className="about-link" href="https://github.com/draph/dimple-mac" onClick={event => {
        event.preventDefault();
        setFailure(null);
        void invoke("open_project_github").catch(setFailure);
      }}>{t("about.source")} <span aria-hidden="true">↗</span><span className="sr-only">{t("about.external")}</span></a>
    </div>
    {failure != null && <p role="alert" className="error">{errorMessage(locale, failure)}</p>}
  </section>;
}
