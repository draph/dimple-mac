# Original color baselines

These 14 images cover the normalized colored SVG (including half-transparent green) and RGB JPEG at 16, 32, 64, 128, 256, 512 and 1024 pixels. `manifest.json` records parameters, hashes and the actual Chromium environment. They are regression artifacts from the app's shared exporter, not an independent rendering reference or Finder acceptance evidence. Existing Safari engraving goldens remain unchanged.

The opaque JPEG sample uses the cover placement: at scale 1 the image covers the folder front inside a thin folder rim and overflow is clipped by the rounded face. The transparent SVG sample keeps the shared 384-grid badge placement used by engraving. `manifest.json` records the placement per sample.

Regenerate normalized fixtures with:

```sh
cargo run --manifest-path src-tauri/Cargo.toml --locked --example prepare_visual_masks
```

Run `npm run dev` and open `/tests/color-render.html` to check original colors, theme independence, transparent source-over composition, dimensions and both transport budgets. The SVG comparison is exact; the independent JPEG reference allows one channel unit away from a one-pixel color/outer-boundary band because Chromium can use different tiny-image interpolation paths. The page also compares these goldens exactly when its recorded browser environment matches; other environments report that comparison as skipped. Inspect all seven sizes visually when refreshing artifacts. No tolerances in the original Safari engraving checker were changed.

Open `/tests/color-effects.html` for actual React/IPC-mock checks of rapid effect clicks, apply blocking, accepted parameter freezing, reset, disabled engraving and failed imports. This does not replace native entry/IPC tests or a packaged WebKit/Finder run.
