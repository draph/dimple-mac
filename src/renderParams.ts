import type { RenderParams } from "./contracts.ts";

export const defaultParams: RenderParams = { scale: 1, offsetX: 0, offsetY: 0, theme: "native" };
export const themes = [
  ["native", "native"], ["red", "#EF4444"], ["orange", "#F97316"], ["yellow", "#EAB308"],
  ["green", "#22C55E"], ["purple", "#A855F7"], ["gray", "#808080"],
] as const;
export const outputSizes = [16, 32, 64, 128, 256, 512, 1024] as const;
const engravingOffsets: Record<(typeof outputSizes)[number], number> = {
  16: .5, 32: .5, 64: .75, 128: 1, 256: 1.5, 512: 2, 1024: 3,
};
export function engravingOffset(size: number): number {
  if (!outputSizes.includes(size as (typeof outputSizes)[number])) throw new Error("E_RENDER_SIZE");
  return engravingOffsets[size as (typeof outputSizes)[number]];
}
export function validParams(p: RenderParams): boolean {
  return Number.isFinite(p.scale) && p.scale >= .25 && p.scale <= 2
    && [p.offsetX, p.offsetY].every(v => Number.isFinite(v) && v >= -100 && v <= 100)
    && (p.theme === "native" || /^#[\da-f]{6}$/i.test(p.theme));
}
export function hsl(color: string): [number, number, number] {
  const [r, g, b] = [1, 3, 5].map(i => parseInt(color.slice(i, i + 2), 16) / 255);
  const max = Math.max(r, g, b), min = Math.min(r, g, b), d = max - min, l = (max + min) / 2;
  const h = !d ? 0 : max === r ? ((g - b) / d + 6) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return [h * 60, d ? d / (1 - Math.abs(2 * l - 1)) : 0, l];
}
export function rgb(h: number, s: number, l: number): number[] {
  const a = s * Math.min(l, 1 - l);
  return [0, 8, 4].map(n => {
    const k = (n + h / 30) % 12;
    return Math.round(255 * (l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1))));
  });
}
export function tint(data: Uint8ClampedArray, color: string): void {
  const [h, s] = hsl(color);
  for (let i = 0; i < data.length; i += 4) {
    const l = (Math.max(data[i], data[i + 1], data[i + 2]) + Math.min(data[i], data[i + 1], data[i + 2])) / 510;
    const result = rgb(h, s, l);
    data[i] = result[0]; data[i + 1] = result[1]; data[i + 2] = result[2];
  }
}
