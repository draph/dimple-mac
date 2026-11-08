import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { translate, type Locale } from "./i18n";
import type { RenderParams } from "./contracts";
import { hsl, tint, validParams } from "./renderParams";

// shortcut: preview uses candidate engraving values; calibrate all sizes in T14.
function render(canvas: HTMLCanvasElement, template: CanvasImageSource, badge: HTMLImageElement, params: RenderParams, layer: HTMLCanvasElement, size = 1024) {
  const ctx = canvas.getContext("2d");
  if (!validParams(params)) throw new Error("E_PREVIEW_PARAMS");
  if (layer.width !== 1024) layer.width = layer.height = 1024;
  const ink = layer.getContext("2d");
  if (!ctx || !ink) throw new Error("E_PREVIEW_CONTEXT");
  if (canvas.width !== size || canvas.height !== size) canvas.width = canvas.height = size;
  ctx.setTransform(1, 0, 0, 1, 0, 0); ctx.globalAlpha = 1; ctx.globalCompositeOperation = "source-over";
  ctx.clearRect(0, 0, size, size);
  ink.setTransform(1, 0, 0, 1, 0, 0); ink.globalAlpha = 1; ink.globalCompositeOperation = "source-over";
  ink.clearRect(0, 0, 1024, 1024);
  ctx.scale(size / 1024, size / 1024);
  const factor = 384 * params.scale / Math.max(badge.naturalWidth, badge.naturalHeight);
  const width = badge.naturalWidth * factor;
  const height = badge.naturalHeight * factor;
  const x = (1024 - width) / 2 + params.offsetX * 2;
  const y = 548 - height / 2 + params.offsetY * 2;
  ctx.drawImage(template, 0, 0, 1024, 1024);
  const mask = (offset: number) => ink.drawImage(badge, x, y + offset, width, height);
  const edge = (offset: number, color: string) => {
    ink.clearRect(0, 0, 1024, 1024);
    ink.globalCompositeOperation = "source-over";
    mask(offset);
    ink.globalCompositeOperation = "destination-out";
    mask(0);
    ink.globalCompositeOperation = "source-in";
    ink.fillStyle = color;
    ink.fillRect(0, 0, 1024, 1024);
    ctx.drawImage(layer, 0, 0);
  };
  const color = (lightness: number, alpha: number) => {
    const [h, saturation] = hsl(params.theme);
    return `hsla(${h},${saturation * 100}%,${lightness}%,${alpha})`;
  };
  const native = params.theme === "native";
  edge(3, native ? "rgba(235,250,255,.65)" : color(96, .65));
  ink.clearRect(0, 0, 1024, 1024);
  ink.globalCompositeOperation = "source-over";
  mask(0);
  ink.globalCompositeOperation = "source-in";
  const fill = ink.createLinearGradient(0, y, 0, y + height);
  fill.addColorStop(0, native ? "rgba(16,94,155,.65)" : color(30, .65));
  fill.addColorStop(1, native ? "rgba(38,127,186,.5)" : color(44, .5));
  ink.fillStyle = fill;
  ink.fillRect(0, 0, 1024, 1024);
  ctx.drawImage(layer, 0, 0);
  ink.clearRect(0, 0, 1024, 1024);
  ink.globalCompositeOperation = "source-over";
  mask(0);
  ink.globalCompositeOperation = "destination-out";
  mask(3);
  ink.globalCompositeOperation = "source-in";
  ink.fillStyle = native ? "rgba(10,65,110,.4)" : color(20, .4);
  ink.fillRect(0, 0, 1024, 1024);
  ctx.drawImage(layer, 0, 0);
}

const templates = import.meta.glob<string>("../upstream/folderify/src/resources/folders/GenericFolderIcon.BigSur.iconset/*.png", { eager: true, query: "?url", import: "default" });
async function load(url: string): Promise<HTMLImageElement> {
  const image = new Image(); image.src = url; await image.decode(); return image;
}
// Only packaged templates are retained; current badge and tinted buffers have component/export lifetimes.
const decodedTemplates = new Map<string, Promise<HTMLImageElement>>();
function template(size: number) {
  const names: Record<number, string> = Object.fromEntries([[16,"16x16"],[32,"16x16@2x"],[64,"32x32@2x"],[128,"128x128"],[256,"128x128@2x"],[512,"256x256@2x"],[1024,"512x512@2x"]]);
  const url = templates[`../upstream/folderify/src/resources/folders/GenericFolderIcon.BigSur.iconset/icon_${names[size]}.png`];
  if (!decodedTemplates.has(url)) decodedTemplates.set(url, load(url).catch(error => { decodedTemplates.delete(url); throw error; }));
  return decodedTemplates.get(url)!;
}
function colored(image: HTMLImageElement, theme: string): CanvasImageSource {
  if (theme === "native") return image;
  const canvas = document.createElement("canvas");
  canvas.width = image.naturalWidth; canvas.height = image.naturalHeight;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("E_PREVIEW_CONTEXT");
  ctx.drawImage(image, 0, 0);
  const pixels = ctx.getImageData(0, 0, canvas.width, canvas.height);
  tint(pixels.data, theme); ctx.putImageData(pixels, 0, 0);
  return canvas;
}
export async function exportImages(badgeUrl: string, params: RenderParams) {
  if (!validParams(params)) throw new Error("E_PREVIEW_PARAMS");
  const badge = await load(badgeUrl);
  const images = [];
  const canvas = document.createElement("canvas"), layer = document.createElement("canvas");
  for (const size of [16, 32, 64, 128, 256, 512, 1024]) {
    render(canvas, colored(await template(size), params.theme), badge, params, layer, size);
    images.push({ size, png: canvas.toDataURL("image/png").split(",")[1] });
  }
  return images;
}

export function Preview({ locale, source, badgeId, params, onReady }: { locale: Locale; source: string | null; badgeId: string; params: RenderParams; onReady: (key: string, ready: boolean) => void }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const layer = useRef<HTMLCanvasElement | null>(null);
  const resources = useRef<{ badge: HTMLImageElement; template: CanvasImageSource; theme: string } | null>(null);
  const decodedBadge = useRef<{ source: string; image: Promise<HTMLImageElement> } | null>(null);
  const frame = useRef<number | null>(null);
  const visible = useRef(false);
  const [status, setStatus] = useState<"loading" | "ready" | "failed">("loading");
  const key = JSON.stringify([badgeId, params]);
  const latest = useRef({ params, key, onReady }); latest.current = { params, key, onReady };
  const schedule = useRef(() => {});
  schedule.current = () => {
    if (!visible.current || document.hidden || frame.current !== null || !resources.current) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = null;
      const r = resources.current, next = latest.current;
      if (!visible.current || document.hidden || !r || !canvas.current) return;
      try {
        if (r.theme !== next.params.theme) return;
        layer.current ??= document.createElement("canvas");
        render(canvas.current, r.template, r.badge, next.params, layer.current);
        setStatus("ready"); next.onReady(next.key, true);
      } catch { setStatus("failed"); next.onReady(next.key, false); }
    });
  };
  useEffect(() => {
    let active = true;
    const update = (shown: boolean) => {
      if (!active) return;
      visible.current = shown;
      if (!shown && frame.current !== null) { cancelAnimationFrame(frame.current); frame.current = null; }
      if (shown) schedule.current();
    };
    // Subscribe before reading initial state so reloads do not miss a show/hide transition.
    let changed = false;
    const listener = listen<boolean>("panel-visibility", event => { changed = true; update(event.payload); });
    void listener.then(() => invoke<boolean>("panel_visible")).then(shown => { if (!changed) update(shown); }).catch(() => update(false));
    const change = () => { if (document.hidden) { if (frame.current !== null) cancelAnimationFrame(frame.current); frame.current = null; } else schedule.current(); };
    document.addEventListener("visibilitychange", change);
    return () => { active = false; document.removeEventListener("visibilitychange", change); void listener.then(unlisten => unlisten()).catch(() => {}); if (frame.current !== null) cancelAnimationFrame(frame.current); frame.current = null; };
  }, []);
  useEffect(() => {
    let active = true;
    resources.current = null; setStatus("loading"); onReady(latest.current.key, false);
    if (source && decodedBadge.current?.source !== source) decodedBadge.current = { source, image: load(source) };
    if (!source) decodedBadge.current = null;
    if (source) void Promise.all([template(1024), decodedBadge.current!.image]).then(([image, badge]) => {
      if (!active) return;
      resources.current = { badge, template: colored(image, params.theme), theme: params.theme };
      schedule.current();
    }).catch(() => { if (active) { setStatus("failed"); onReady(latest.current.key, false); } });
    return () => { active = false; resources.current = null; };
  }, [source, badgeId, params.theme, onReady]);
  useEffect(() => { schedule.current(); }, [key]);
  return <div className="preview">
    <canvas ref={canvas} hidden={status !== "ready"} role="img" aria-label={translate(locale, "preview.description")} />
    {status !== "ready" && <p role={status === "failed" ? "alert" : "status"}>{translate(locale, `preview.${status}`)}</p>}
  </div>;
}
