import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { errorMessage, translate, type Locale } from "./i18n";
import { checkForUpdate, fetchReleases, type UpdateCheck } from "./update";

type View =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "error" }
  | UpdateCheck;

export function AboutSettings({
  locale,
  version,
  blocked,
  checkRequest,
  onCheckRequestHandled,
}: {
  locale: Locale;
  version: string;
  blocked: boolean;
  checkRequest: boolean;
  onCheckRequestHandled: () => void;
}) {
  const [view, setView] = useState<View>({ kind: "idle" });
  const [failure, setFailure] = useState<unknown>(null);
  const section = useRef<HTMLElement>(null);
  const mounted = useRef(false);
  const checking = useRef(false);
  const request = useRef(0);
  const controller = useRef<AbortController | null>(null);
  const handled = useRef(false);
  const t = (key: string) => translate(locale, key);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      // Delay the abort so StrictMode's immediate test unmount does not cancel the first check.
      setTimeout(() => {
        if (!mounted.current) {
          controller.current?.abort();
          controller.current = null;
        }
      }, 0);
    };
  }, []);
  const check = useCallback(async () => {
    if (blocked || checking.current) return;
    checking.current = true;
    const id = ++request.current;
    controller.current?.abort();
    const next = new AbortController();
    controller.current = next;
    const timer = setTimeout(() => next.abort(), 10_000);
    setView({ kind: "checking" });
    setFailure(null);
    try {
      const releases = await fetchReleases(next.signal);
      if (id !== request.current || !mounted.current) return;
      const result = checkForUpdate(version, releases);
      setView(result.kind === "none" ? { kind: "error" } : result);
    } catch {
      if (id === request.current && mounted.current) setView({ kind: "error" });
    } finally {
      clearTimeout(timer);
      if (controller.current === next) controller.current = null;
      checking.current = false;
    }
  }, [blocked, version]);
  useEffect(() => {
    if (!checkRequest) {
      handled.current = false;
      return;
    }
    if (handled.current) return;
    handled.current = true;
    section.current?.scrollIntoView({ block: "center" });
    requestAnimationFrame(() => section.current?.focus({ preventScroll: true }));
    void check();
    onCheckRequestHandled();
  }, [checkRequest, check, onCheckRequestHandled]);
  const openReleases = (tag: string | null) => {
    setFailure(null);
    void invoke("open_releases", { tag }).catch(setFailure);
  };
  return (
    <section
      ref={section}
      tabIndex={-1}
      className="settings-card about"
      aria-labelledby="about-title"
    >
      <div className="settings-card-heading">
        <span className="settings-icon about-icon" aria-hidden="true">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round">
            <circle cx="12" cy="12" r="9" />
            <path d="M12 11v6m0-10v.1" />
          </svg>
        </span>
        <h2 id="about-title" className="settings-card-title">{t("about.title")}</h2>
      </div>
      <div className="about-row">
        <span className="about-version">{translate(locale, "about.version", { version })}</span>
        {view.kind === "checking" && (
          <span className="about-status muted" role="status">{t("about.checking")}</span>
        )}
        {view.kind === "up-to-date" && (
          <span className="about-status muted" role="status">{t("about.upToDate")}</span>
        )}
        {view.kind === "available" && (
          <span className="about-status" role="status">
            {translate(
              locale,
              view.latest.prerelease ? "about.availablePrerelease" : "about.available",
              { version: view.latest.version },
            )}
          </span>
        )}
        {(view.kind === "error" || view.kind === "invalid") && (
          <span className="about-status error" role="alert">{t("about.checkFailed")}</span>
        )}
        {view.kind === "available" ? (
          <button
            type="button"
            className="primary about-download"
            onClick={() => openReleases(view.latest.tag)}
          >
            {t("about.download")} <span aria-hidden="true">↗</span>
            <span className="sr-only">{t("about.external")}</span>
          </button>
        ) : (
          <button
            type="button"
            className="check-updates"
            disabled={blocked || view.kind === "checking"}
            onClick={() => void check()}
          >
            {t("about.update")}
          </button>
        )}
      </div>
      <div className="about-row">
        <span>{t("about.github")}</span>
        <a
          className="about-link"
          href="https://github.com/draph/dimple-mac"
          onClick={event => {
            event.preventDefault();
            setFailure(null);
            void invoke("open_project_github").catch(setFailure);
          }}
        >
          {t("about.source")} <span aria-hidden="true">↗</span>
          <span className="sr-only">{t("about.external")}</span>
        </a>
      </div>
      {failure != null && <p role="alert" className="error">{errorMessage(locale, failure)}</p>}
    </section>
  );
}
