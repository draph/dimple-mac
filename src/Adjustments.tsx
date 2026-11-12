import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { errorMessage, translate, type Session } from "./i18n";
import type { RenderParams } from "./contracts";
import { defaultParams, themes, validParams } from "./renderParams";
import { sameContext } from "./session";

export function Adjustments({ session, disabled, refresh, onChange }: {
  session: Session; disabled: boolean; refresh: () => Promise<Session | null>;
  onChange: (params: RenderParams | null, pending: boolean) => void;
}) {
  const [params, setParams] = useState(session.entry.params);
  const [color, setColor] = useState(session.entry.params.theme === "native" ? "#808080" : session.entry.params.theme);
  const [error, setError] = useState<unknown>(null);
  const [invalid, setInvalid] = useState(false);
  const active = useRef(false), sending = useRef(false);
  const queue = useRef<RenderParams | null>(null);
  const latest = useRef({ session, refresh, onChange }); latest.current = { session, refresh, onChange };
  const invalidRef = useRef(false);
  const value = useRef(params);
  const enqueue = (next: RenderParams) => {
    if (!validParams(next)) return;
    value.current = next; setParams(next); setError(null); setInvalid(false); invalidRef.current = false;
    setColor(next.theme === "native" ? "#808080" : next.theme);
    queue.current = next;
    latest.current.onChange(next, true);
    if (sending.current) return;
    sending.current = true;
    void (async () => {
      let context = latest.current.session;
      try {
        while (active.current && queue.current) {
          const input = queue.current; queue.current = null;
          await invoke("set_render_params", { params: input, expectedEditRevision: context.entry.editRevision,
            operationId: context.operation.progress?.operationId ?? null });
          const nextSession = await latest.current.refresh();
          if (!nextSession || !active.current) break;
          if (nextSession.entry.editRevision !== context.entry.editRevision + 1) { context = nextSession; queue.current = null; break; }
          context = nextSession;
        }
      } catch (error) {
        queue.current = null;
        if (active.current) {
          const failed = context;
          context = await latest.current.refresh().catch(() => null) ?? latest.current.session;
          if (active.current) setError(sameContext(failed, context) ? error : null);
        }
      } finally {
        sending.current = false;
        if (active.current) {
          const current = (latest.current.session.entry.revision > context.entry.revision ? latest.current.session : context).entry.params;
          value.current = current; setParams(current);
          latest.current.onChange(null, invalidRef.current);
        }
      }
    })();
  };
  useEffect(() => { active.current = true; return () => { active.current = false; queue.current = null; }; }, []);
  useEffect(() => {
    if (!sending.current) { value.current = session.entry.params; setParams(session.entry.params); if (!invalidRef.current) setColor(session.entry.params.theme === "native" ? "#808080" : session.entry.params.theme); }
  }, [session.entry.params]);
  const t = (key: string) => translate(session.locale, key);
  return <details className="adjustments"><summary>{t("adjust.heading")}</summary>
    <fieldset disabled={disabled}>
      {([["scale", .25, 2, .01], ["offsetX", -100, 100, 1], ["offsetY", -100, 100, 1]] as const).map(([name, min, max, step]) =>
        <label className="adjustment-row" key={name}><span>{t(`adjust.${name}`)}</span>
          <input type="range" aria-label={t(`adjust.${name}`)} min={min} max={max} step={step} value={params[name]}
            onChange={event => enqueue({ ...value.current, [name]: Number(event.target.value) })} /><output>{params[name]}</output>
        </label>)}
      <div className="theme-presets" role="group" aria-label={t("adjust.theme")}>
        {themes.map(([name, theme]) => <button key={name} className="theme-swatch" aria-label={t(`adjust.${name}`)} title={t(`adjust.${name}`)} style={{ backgroundColor: theme === "native" ? "#65BDF3" : theme }} aria-pressed={params.theme === theme} onClick={() => enqueue({ ...value.current, theme })}><span className="sr-only">{t(`adjust.${name}`)}</span></button>)}
      </div>
      <label>{t("adjust.custom")} <input type="color" aria-label={t("adjust.picker")} value={/^#[\da-f]{6}$/i.test(color) ? color : "#808080"}
        onChange={event => enqueue({ ...value.current, theme: event.target.value.toUpperCase() })} />
        <input type="text" aria-label={t("adjust.hex")} value={color} aria-invalid={invalid} spellCheck={false}
          onChange={event => {
            const input = event.target.value; setColor(input);
            if (/^#[\da-f]{6}$/i.test(input)) enqueue({ ...value.current, theme: input.toUpperCase() });
            else { invalidRef.current = true; setInvalid(true); latest.current.onChange(null, true); }
          }} />
      </label>
      <p className="muted">{t("adjust.hint")}</p>
      {invalid && <p role="alert" className="error">{t("adjust.invalid")}</p>}
      <button onClick={() => enqueue({ ...defaultParams })}>{t("adjust.reset")}</button>
    </fieldset>
    {error !== null && <p role="alert" className="error">{errorMessage(session.locale, error)}</p>}
  </details>;
}
