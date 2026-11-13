import assert from "node:assert/strict";
import test from "node:test";
import { defaultParams, engravingOffset, hsl, outputSizes, rgb, themes, tint, validParams } from "./renderParams.ts";

test("render parameters reject unsafe values and accept inclusive boundaries", () => {
  for (const scale of [.25, 1, 2]) for (const offsetX of [-100, 0, 100]) {
    assert.ok(validParams({ ...defaultParams, scale, offsetX, offsetY: -offsetX }));
  }
  for (const patch of [{scale: NaN}, {scale: Infinity}, {scale: .24}, {scale: 2.01}, {offsetX: 101}, {offsetY: -101}, {theme: "red"}, {theme: "#FFF"}, {theme: "#12345G"}]) {
    assert.equal(validParams({ ...defaultParams, ...patch }), false);
  }
  assert.ok(validParams({ ...defaultParams, effect: "original" }));
  assert.equal(validParams({ ...defaultParams, effect: "unknown" } as never), false);
  for (const [, theme] of themes) assert.ok(validParams({ ...defaultParams, theme }));
});

test("theme tint preserves alpha and HSL lightness, including achromatic inputs", () => {
  for (const [, color] of themes.slice(1)) {
    const pixels = new Uint8ClampedArray([20, 100, 200, 17, 240, 250, 255, 255, 0, 0, 0, 0]);
    tint(pixels, color);
    assert.equal(pixels[3], 17); assert.equal(pixels[7], 255); assert.equal(pixels[11], 0);
    assert.ok(Math.abs(Math.max(...pixels.slice(0, 3)) + Math.min(...pixels.slice(0, 3)) - 220) <= 1);
    assert.ok(Math.abs(Math.max(...pixels.slice(4, 7)) + Math.min(...pixels.slice(4, 7)) - 495) <= 1);
  }
  const gray = (color: string) => { const data = new Uint8ClampedArray([20,100,200,17]); tint(data, color); return [...data]; };
  assert.deepEqual(gray("#000000"), gray("#FFFFFF"));
  assert.deepEqual(gray("#808080"), [110,110,110,17]);
  for (const color of ["#EF4444", "#00FF00", "#0000FF", "#000000", "#FFFFFF", "#808080"]) {
    assert.deepEqual(rgb(...hsl(color)), [1,3,5].map(i => parseInt(color.slice(i,i+2),16)));
  }
});

test("engraving offsets are calibrated for every output size", () => {
  assert.deepEqual(outputSizes.map(engravingOffset), [.5, .5, .75, 1, 1.5, 2, 3]);
  assert.throws(() => engravingOffset(48), { message: "E_RENDER_SIZE" });
});
