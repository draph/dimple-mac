# Visual baseline checks

The golden PNGs in this directory were exported by Safari on macOS 26.6.2
(25G83), using WebKit 605.1.15 at device pixel ratio 2. The manifest records
the input hashes, seven output sizes, engraving offsets, theme colors, ICNS
representations, and exact RGBA thresholds.

The input files are converted to the same cropped RGBA images used by the app. Engraving reads only their Alpha; color channels and PNG encoding do not change the historical engraving goldens.
Regenerate them after changing a source fixture with:

```sh
cargo run --manifest-path src-tauri/Cargo.toml --locked --example prepare_visual_masks
```

To compare a new export on the recorded environment:

1. Run `npm run dev` from the repository root.
2. Open `http://localhost:1420/tests/visual-baseline.html` in Safari.
3. Choose **Generate visual baselines**, then **Download JSON for pixel comparison**.
4. Run `cargo run --manifest-path src-tauri/Cargo.toml --locked --example check_visual_baselines -- /path/to/folderify-visual-baselines.json`.

The checker requires the recorded browser environment and compares all 70 PNGs
against the committed goldens. The threshold is zero changed pixels and zero
channel delta. ICNS compatibility and ARGB alpha preservation are checked on
macOS with `cargo test --manifest-path src-tauri/Cargo.toml --locked system_iconutil_reads`.

## Upstream native-blue reference

`upstream-reference/` contains test-only outputs from the pinned upstream
commit for the same `svg` mask. They close the T14 requirement to compare
native blue with the fixed upstream implementation. They are not part of the
70-image zero-difference checker: the comparison is reviewed with the
informational metrics and notes in
[`upstream-reference/README.md`](upstream-reference/README.md). Regenerating
the reference is a development-only step that requires ImageMagick; neither
ImageMagick nor the reference images are packaged with the application.

Original-color regression images and checks are documented in [color/README.md](color/README.md).
