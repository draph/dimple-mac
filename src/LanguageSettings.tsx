import { useState } from "react";
import { errorMessage, translate, type Locale } from "./i18n";
import type { LanguagePreference, LanguageStatus } from "./language";

const preferences = ["system", "zh-Hans", "en"] as const;

export function LanguageSettings({ locale, status, disabled, onSelect }: {
  locale: Locale;
  status: LanguageStatus | null;
  disabled: boolean;
  onSelect: (preference: LanguagePreference) => Promise<void>;
}) {
  const [saving, setSaving] = useState(false);
  const [failure, setFailure] = useState<unknown>(null);
  const t = (key: string) => translate(locale, key);
  const choose = async (preference: LanguagePreference) => {
    if (saving || !status || preference === status.preference) return;
    setSaving(true); setFailure(null);
    try { await onSelect(preference); }
    catch (error) { setFailure(error); }
    finally { setSaving(false); }
  };
  return <section className="settings-card language" aria-labelledby="language-title">
    <div className="settings-card-heading">
      <span className="settings-icon" aria-hidden="true"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round"><circle cx="12" cy="12" r="9" /><path d="M3 12h18M12 3c2.5 2.5 4 5.6 4 9s-1.5 6.5-4 9c-2.5-2.5-4-5.6-4-9s1.5-6.5 4-9Z" /></svg></span>
      <h2 id="language-title" className="settings-card-title">{t("language.title")}</h2>
    </div>
    <div className="appearance-options" role="group" aria-labelledby="language-title">
      {preferences.map(value => <label key={value}>
        <input type="radio" name="language" value={value} checked={status?.preference === value}
          disabled={disabled || saving || !status} onChange={() => void choose(value)} />
        <span>{value === "system" ? t("language.system") : value === "zh-Hans" ? "简体中文" : "English"}</span>
      </label>)}
    </div>
    {status?.error != null && <p role="alert" className="error">{errorMessage(locale, status.error)}</p>}
    {failure != null && <p role="alert" className="error">{errorMessage(locale, failure)}</p>}
  </section>;
}
