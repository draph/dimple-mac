import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { errorMessage, translate, type Locale } from "./i18n";
import { captureShortcut, displayShortcut, type ShortcutStatus, type ShortcutSettings as Value } from "./shortcut";
import { AppearanceSettings } from "./AppearanceSettings";
import { LanguageSettings } from "./LanguageSettings";
import type { AppearanceController } from "./appearance";
import type { LanguagePreference, LanguageStatus } from "./language";

export function ShortcutSettings({ locale, version, blocked, onOpenChange, appearance, language, onSelectLanguage }: { locale: Locale; version: string; blocked: boolean; onOpenChange: (open: boolean) => void; appearance: AppearanceController; language: LanguageStatus | null; onSelectLanguage: (preference: LanguagePreference) => Promise<void> }) {
  const [status, setStatus] = useState<ShortcutStatus | null>(null);
  const [draft, setDraft] = useState<Value | null>(null);
  const [capturing, setCapturing] = useState(false);
  const [saving, setSaving] = useState(false);
  const [failure, setFailure] = useState<unknown>(null);
  const [saved, setSaved] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const capture = useRef(false);
  const captureEpoch = useRef(0);
  const section = useRef<HTMLElement>(null);
  const previouslyBlocked = useRef(false);
  const t = (key: string) => translate(locale, key);
  useEffect(() => {
    let active = true;
    let request = 0;
    const resetOpen = blocked || previouslyBlocked.current;
    previouslyBlocked.current = blocked;
    const refresh = async () => {
      const generation = ++request;
      const value = await invoke<ShortcutStatus>("get_shortcut_settings");
      if (!active || generation !== request) return;
      if (blocked && value.open) await invoke("close_settings");
      if (active && generation === request) { setStatus({ ...value, open: value.open && !blocked }); setDraft(previous => previous ?? value.settings); }
    };
    const listener = listen("shortcut-settings", () => void refresh().catch(setFailure));
    void listener.then(() => active ? invoke("set_shortcut_capture", { capturing: false }) : undefined)
      .then(() => active && resetOpen ? invoke("close_settings") : undefined)
      .then(() => { if (active) return refresh(); }).catch(setFailure);
    return () => { active = false; void listener.then(fn => fn()).catch(() => {}); };
  }, [blocked]);
  const stopCapture = async () => {
    ++captureEpoch.current;
    capture.current = false; setCapturing(false);
    await invoke("set_shortcut_capture", { capturing: false });
  };
  useEffect(() => {
    const hidden = listen<boolean>("panel-visibility", event => { if (!event.payload && capture.current) void stopCapture().catch(setFailure); });
    return () => { void hidden.then(fn => fn()).catch(() => {}); };
  }, []);
  const save = async () => {
    if (!draft || saving) return;
    setSaving(true); setFailure(null); setSaved(false);
    try {
      await stopCapture();
      const value = await invoke<ShortcutStatus>("save_shortcut_settings", { settings: draft });
      setStatus(previous => ({ ...value, open: previous?.open ?? false }));
      if (value.saved) { setDraft(value.settings); setSaved(true); }
    } catch (error) { setFailure(error); }
    finally { setSaving(false); }
  };
  const open = !!status?.open && !blocked;
  useEffect(() => {
    if (blocked) {
      if (status?.open) void invoke("close_settings").catch(setFailure);
      setStatus(previous => previous && ({ ...previous, open: false }));
    }
  }, [blocked]);
  useEffect(() => { onOpenChange(open); }, [open, onOpenChange]);
  useEffect(() => {
    if (!open) void stopCapture().catch(setFailure);
  }, [open]);
  useEffect(() => {
    if (open) { section.current?.scrollIntoView({ block: "start" }); section.current?.focus(); }
  }, [open]);
  useEffect(() => {
    const listener = listen<string>("shortcut-captured", event => {
      if (capture.current) { setDraft(previous => previous && ({ ...previous, shortcut: event.payload })); setSaved(false); }
    });
    return () => { void listener.then(fn => fn()).catch(() => {}); };
  }, []);
  if (!open || !status || !draft) {
    const error = failure ?? status?.error;
    return error ? <p className="error" role="alert">{errorMessage(locale, error)}</p> : null;
  }
  return <section ref={section} tabIndex={-1} className="settings" aria-labelledby="settings-title">
    <div className="settings-header"><h2 id="settings-title">{t("settings.title")}</h2><button className="quiet settings-back" disabled={saving} onClick={() => {
      void stopCapture().then(() => invoke("close_settings")).then(() => setStatus({ ...status, open: false })).catch(setFailure);
    }}><span aria-hidden="true">‹ </span>{t("settings.close")}</button></div>
    <div className="settings-scroll">
    <AppearanceSettings locale={locale} appearance={appearance} />
    <LanguageSettings locale={locale} status={language} disabled={blocked || capturing} onSelect={onSelectLanguage} />
    <section className="settings-card" aria-labelledby="shortcut-title">
    <div className="settings-card-heading">
      <span className="settings-icon" aria-hidden="true"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round"><rect x="2" y="4" width="20" height="16" rx="3" /><path d="M6 8h1m4 0h1m4 0h1M6 12h1m4 0h1m4 0h1M8 16h8" /></svg></span>
      <h2 id="shortcut-title" className="settings-card-title">{t("settings.section")}</h2>
    </div>
    <div className="settings-toggle"><span><span className="settings-label">{t("settings.enabled")}</span><span className="muted">{t("settings.enabledDescription")}</span></span><input role="switch" aria-label={t("settings.enabled")} type="checkbox" checked={draft.enabled} disabled={saving || capturing} onChange={e => { setDraft({ ...draft, enabled: e.target.checked }); setSaved(false); }} /></div>
    <div className="shortcut-row">
    <label htmlFor="shortcut-input">{t("settings.shortcut")}</label>
    <input id="shortcut-input" ref={input} readOnly value={capturing ? t("settings.recording") : displayShortcut(draft.shortcut)} disabled={saving || !draft.enabled}
      onClick={() => {
        setFailure(null); setSaved(false);
        const generation = ++captureEpoch.current;
        void invoke("set_shortcut_capture", { capturing: true }).then(() => {
          if (generation !== captureEpoch.current) return;
          capture.current = true; setCapturing(true); input.current?.focus();
        }).catch(setFailure);
      }} onBlur={() => { if (capture.current) void stopCapture().catch(setFailure); }}
      onKeyDown={event => {
        if (!capturing) {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault(); event.stopPropagation(); input.current?.click();
          }
          return;
        }
        event.preventDefault(); event.stopPropagation();
        if (event.nativeEvent.isComposing || event.keyCode === 229) return;
        if (event.key === "Escape") { void stopCapture().catch(setFailure); return; }
        const value = captureShortcut(event.nativeEvent);
        if (value) { setDraft({ ...draft, shortcut: value }); setSaved(false); }
      }} />
    <button className="primary" disabled={saving || capturing || status.uncertain.length > 0} onClick={() => void save()}>{t("settings.save")}</button>
    </div>
    <p className="muted settings-active" role="status">{translate(locale, "settings.active", { shortcut: status.active.length ? status.active.map(displayShortcut).join(" / ") : t("settings.none") })}</p>
    <p className="muted settings-hint"><span aria-hidden="true">ⓘ</span>{t("settings.hint")}</p>
    {status.uncertain.length > 0 && <p role="alert">{translate(locale, "settings.uncertain", { shortcut: status.uncertain.map(displayShortcut).join(" / ") })}</p>}
    {status.error != null && <p role="alert" className="error">{errorMessage(locale, status.error)}</p>}
    {failure != null && <p role="alert" className="error">{errorMessage(locale, failure)}</p>}
    {saved && <p role="status">{t("settings.saved")}</p>}
    </section>
    <section className="settings-card about" aria-labelledby="about-title">
      <span className="settings-icon about-icon" aria-hidden="true"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round"><circle cx="12" cy="12" r="9" /><path d="M12 11v6m0-10v.1" /></svg></span>
      <div><p id="about-title" className="settings-card-title">{translate(locale, "about.version", { version })}</p></div>
      <button type="button" className="check-updates" disabled aria-describedby="update-unavailable">{t("about.update")}</button>
      <span id="update-unavailable" className="sr-only">{t("about.unavailable")}</span>
    </section>
    </div>
    <div className="settings-footer"><button className="quiet" disabled={saving || capturing} onClick={() => { void invoke("quit_app").catch(setFailure); }}>{t("menu.quit")}</button></div>
  </section>;
}
