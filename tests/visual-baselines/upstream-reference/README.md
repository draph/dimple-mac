# Upstream reference for T14 native-blue calibration

These PNGs are test-only reference outputs generated from the pinned upstream
`folderify` commit. They are compared with the committed `svg-native-*` goldens
to close the T14 requirement that native blue be checked against the fixed
upstream implementation. They are not shipped in the application and are not a
zero-difference golden set.

## Provenance

- Upstream repository: `lgarron/folderify`
- Pinned commit: `16de35dd0ee94be4eae046dde709949b5d779e48`
- Input mask: `tests/fixtures/badges/visual-masks/svg.png`
- Input SHA-256: `2803ef63eb559ed3619e056de6a12cc46db1210c031f1e9426a513053347d01a`
- Toolchain used for the reference run: `rustc 1.99.1` / `cargo 1.99.1`
- Image processing: ImageMagick `7.1.2-35 Q16-HDRI aarch64` (`magick` and `identify`)
- Folder style: Big Sur light (`--macOS 11 --color-scheme light`)
- Mask handling: `--no-trim` (the input is 384x384, matching the upstream
  384-pixel mask grid)

## Generation

The reference was generated before the upstream submodule was removed. If it
must be regenerated after removal, check out the pinned commit into a temporary
directory and substitute that path for `upstream/folderify` below. Keep
`CARGO_TARGET_DIR` outside the checkout so the reference source stays clean.

```sh
CARGO_TARGET_DIR=/tmp/folderify-upstream-target \
  cargo run --manifest-path upstream/folderify/Cargo.toml --locked -- \
  --macOS 11 --color-scheme light --no-trim --no-progress \
  --output-iconset /tmp/t14-upstream/svg.iconset \
  --output-icns /tmp/t14-upstream/svg.icns \
  tests/fixtures/badges/visual-masks/svg.png
```

Select the seven sizes using the same mapping as `src/Preview.tsx:72-76` and
name them `svg-native-{size}.png` for direct comparison with the goldens. Only
the iconset outputs from `/tmp` are used; the command does not set an icon on
any folder.

The mask is a `graya` image with transparent background and opaque artwork. The
generated output keeps the expected (non-inverted) badge shape, so the upstream
alpha handling is consistent with the input. If a future mask uses a different
channel convention, verify the output before recording new metrics.

## Reference file hashes

| Size | SHA-256 |
| ---: | :--- |
| 16 | `9a4638de6134cf5b6e36ab3c6525df872de733d099957c5d7b46b696efc55ef9` |
| 32 | `cadca431eab7302bff25e0b40433a4f7dbfc8795133c05581de7314d5a94fac7` |
| 64 | `6b33466ee4160ead70ede57ea83b696ca7fbc2310d0319bc40c3f1301b913e99` |
| 128 | `7f0c6b32a0b200d1b37a9374e50741166fea804f56c6da5a551a8f81f0255e3e` |
| 256 | `b10325a1a654d38da439f67fee66d5616a3d01a93b08e4d513d399627ad4e40b` |
| 512 | `fe6d620ed54a456ecc589b885c9e72969a04039ba30939e343b92ecc07e351ad` |
| 1024 | `521238d2195870e11d2aee8922bccc1ea645f1ac4e2917625fe75fbf8e909292` |

## Comparison with the committed goldens

Metrics are informational. They are not a pass/fail threshold and do not
change `manifest.json`'s zero-difference policy for the project's own goldens.
`different_pixels` counts pixels where ImageMagick's alpha-composited RGBA
difference is nonzero. It is informational and is not the same definition as
the 70-image checker's raw-RGBA `different_pixels`.

| Size | Different pixels | Total pixels | Alpha-different pixels | Peak channel delta (8-bit) | RMSE | MAE |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 16 | 62 | 256 | 2 | 71 | 3.51% | 0.874% |
| 32 | 228 | 1,024 | 2 | 131 | 4.57% | 0.904% |
| 64 | 580 | 4,096 | 0 | 132 | 3.48% | 0.654% |
| 128 | 1,839 | 16,384 | 0 | 94 | 2.65% | 0.538% |
| 256 | 6,297 | 65,536 | 0 | 133 | 2.80% | 0.519% |
| 512 | 23,415 | 262,144 | 0 | 133 | 2.69% | 0.502% |
| 1024 | 89,394 | 1,048,576 | 0 | 135 | 2.35% | 0.379% |

`Peak channel delta` is the maximum absolute RGBA channel difference on an
8-bit scale (the normalized PAE value multiplied by 255, rounded). It is
informational like the other metrics and does not change the zero-difference
policy for the project's own goldens.

The metrics were produced with ImageMagick:

```sh
magick compare -metric RMSE -alpha on REFERENCE GOLDEN null:
magick compare -metric MAE  -alpha on REFERENCE GOLDEN null:
magick compare -metric PAE  -alpha on REFERENCE GOLDEN null:
magick REFERENCE GOLDEN -compose difference -composite \
  -channel RGBA -separate -evaluate-sequence max -threshold 0 \
  -format '%[fx:mean*w*h]' info:
magick REFERENCE GOLDEN -compose difference -composite -alpha extract \
  -threshold 0 -format '%[fx:mean*w*h]' info:
```

The final command counts pixels whose alpha channel differs; the preceding
command counts pixels whose alpha-composited RGBA difference is nonzero.

## Review notes

- Badge alpha geometry matches closely: two alpha-different pixels at 16 and
  32, none at 64 and above. No clipping, obvious offset, or layer inversion was
  observed at 16, 32, 64, 128, or 1024.
- The remaining RGB differences are concentrated on the badge bevel and
  highlight. The project's Canvas renderer draws a stronger bottom/right
  highlight and a gradient; upstream uses its ImageMagick fill/top-bezel/
  bottom-bezel composition. This is a deliberate rendering-style difference,
  not a defect in placement or dimensions.
- The 16/32 outputs remain legible; no detail is lost at the sizes used in
  Finder list view.
- Upstream has no equivalent arbitrary theme-color API. The seven theme
  goldens remain self-consistency checks; this reference only covers native
  blue, as required by the T14 upstream-comparison finding.

## License

These reference images are derived from the pinned upstream template and the
same test mask. They are not packaged with the application. The template's
accepted license basis and its limitation (no separately verified Apple
authorization) are recorded in `THIRD_PARTY_NOTICES.md`; this test reference
does not broaden that basis.
