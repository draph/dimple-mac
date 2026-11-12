import { useSyncExternalStore } from "react";
import { translate, type Locale } from "./i18n";
import type { AppearanceController } from "./appearance";

export function AppearanceSettings({ locale, appearance }: { locale: Locale; appearance: AppearanceController }) {
  const { preference, saveFailed } = useSyncExternalStore(appearance.subscribe, appearance.getSnapshot);
  const t = (key: string) => translate(locale, key);
  return <section className="settings-card appearance" aria-labelledby="appearance-title">
    <div className="settings-card-heading">
      <span className="settings-icon" aria-hidden="true"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7"><circle cx="12" cy="12" r="9" /><path d="M12 3a9 9 0 0 0 0 18Z" fill="currentColor" /></svg></span>
      <h2 id="appearance-title" className="settings-card-title">{t("appearance.title")}</h2>
    </div>
    <div className="appearance-options" role="group" aria-labelledby="appearance-title">
      {(["system", "dark", "light"] as const).map(value => <label key={value}>
        <input type="radio" name="appearance" value={value} checked={preference === value} onChange={() => appearance.select(value)} />
        <span>{t(`appearance.${value}`)}</span>
      </label>)}
    </div>
    {saveFailed && <p className="error" role="alert">{t("appearance.saveFailed")}</p>}
  </section>;
}
