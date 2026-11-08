# Visual baseline checks

The golden PNGs in this directory were exported by Safari on macOS 26.6.2
(25G83), using WebKit 605.1.15 at device pixel ratio 2. The manifest records
the input hashes, seven output sizes, engraving offsets, theme colors, ICNS
representations, and exact RGBA thresholds.

The input files are converted to the same cropped Alpha masks used by the app.
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
