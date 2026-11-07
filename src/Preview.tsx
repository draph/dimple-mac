import { useEffect, useRef, useState } from "react";
import templateUrl from "../upstream/folderify/src/resources/folders/GenericFolderIcon.BigSur.iconset/icon_512x512@2x.png";
import badgeUrl from "./assets/code.svg";
import { translate, type Locale } from "./i18n";

// shortcut: preview uses candidate engraving values; calibrate all sizes in T14.
function render(canvas: HTMLCanvasElement, template: HTMLImageElement, badge: HTMLImageElement) {
  const ctx = canvas.getContext("2d");
  const layer = document.createElement("canvas");
  layer.width = layer.height = 1024;
  const ink = layer.getContext("2d");
  if (!ctx || !ink) throw new Error("E_PREVIEW_CONTEXT");
  canvas.width = canvas.height = 1024;
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

export function Preview({ locale }: { locale: Locale }) {
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
    }).catch(() => { if (active) setStatus("failed"); });
    return () => { active = false; };
  }, []);
  return <div className="preview">
    <canvas ref={canvas} hidden={status !== "ready"} role="img" aria-label={translate(locale, "preview.description")} />
    {status !== "ready" && <p role={status === "failed" ? "alert" : "status"}>{translate(locale, `preview.${status}`)}</p>}
  </div>;
}
