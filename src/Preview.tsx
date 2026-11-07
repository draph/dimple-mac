import { useEffect, useRef, useState } from "react";
import templateUrl from "../upstream/folderify/src/resources/folders/GenericFolderIcon.BigSur.iconset/icon_512x512@2x.png";
import badgeUrl from "./assets/code.svg";
import { translate, type Locale } from "./i18n";

// shortcut: preview uses candidate engraving values; calibrate all sizes in T14.
function render(canvas: HTMLCanvasElement, template: HTMLImageElement, badge: HTMLImageElement, size = 1024) {
  const ctx = canvas.getContext("2d");
  const layer = document.createElement("canvas");
  layer.width = layer.height = 1024;
  const ink = layer.getContext("2d");
  if (!ctx || !ink) throw new Error("E_PREVIEW_CONTEXT");
  canvas.width = canvas.height = size;
  ctx.scale(size / 1024, size / 1024);
  const width = 384;
  const height = width * badge.naturalHeight / badge.naturalWidth;
  const x = (1024 - width) / 2;
  const y = 548 - height / 2;
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
  edge(3, "rgba(235,250,255,.65)");
  ink.clearRect(0, 0, 1024, 1024);
  ink.globalCompositeOperation = "source-over";
  mask(0);
  ink.globalCompositeOperation = "source-in";
  const fill = ink.createLinearGradient(0, y, 0, y + height);
  fill.addColorStop(0, "rgba(16,94,155,.65)");
  fill.addColorStop(1, "rgba(38,127,186,.5)");
  ink.fillStyle = fill;
  ink.fillRect(0, 0, 1024, 1024);
  ctx.drawImage(layer, 0, 0);
  ink.clearRect(0, 0, 1024, 1024);
  ink.globalCompositeOperation = "source-over";
  mask(0);
  ink.globalCompositeOperation = "destination-out";
  mask(3);
  ink.globalCompositeOperation = "source-in";
  ink.fillStyle = "rgba(10,65,110,.4)";
  ink.fillRect(0, 0, 1024, 1024);
  ctx.drawImage(layer, 0, 0);
}

const templates = import.meta.glob<string>("../upstream/folderify/src/resources/folders/GenericFolderIcon.BigSur.iconset/*.png", { eager: true, query: "?url", import: "default" });
async function load(url: string): Promise<HTMLImageElement> {
  const image = new Image(); image.src = url; await image.decode(); return image;
}
export async function exportImages() {
  const badge = await load(badgeUrl);
  const images = [];
  for (const [size, name] of [[16,"16x16"],[32,"16x16@2x"],[64,"32x32@2x"],[128,"128x128"],[256,"128x128@2x"],[512,"256x256@2x"],[1024,"512x512@2x"]] as const) {
    const template = await load(templates[`../upstream/folderify/src/resources/folders/GenericFolderIcon.BigSur.iconset/icon_${name}.png`]);
    const canvas = document.createElement("canvas");
    render(canvas, template, badge, size);
    images.push({ size, png: canvas.toDataURL("image/png").split(",")[1] });
  }
  return images;
}

export function Preview({ locale, onReady }: { locale: Locale; onReady: (ready: boolean) => void }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [status, setStatus] = useState<"loading" | "ready" | "failed">("loading");
  useEffect(() => {
    let active = true;
    const template = new Image();
    const badge = new Image();
    template.src = templateUrl;
    badge.src = badgeUrl;
    void Promise.all([template.decode(), badge.decode()]).then(() => {
      if (!active || !canvas.current) return;
      render(canvas.current, template, badge);
      setStatus("ready");
      onReady(true);
    }).catch(() => { if (active) { setStatus("failed"); onReady(false); } });
    return () => { active = false; };
  }, [onReady]);
  return <div className="preview">
    <canvas ref={canvas} hidden={status !== "ready"} role="img" aria-label={translate(locale, "preview.description")} />
    {status !== "ready" && <p role={status === "failed" ? "alert" : "status"}>{translate(locale, `preview.${status}`)}</p>}
  </div>;
}
