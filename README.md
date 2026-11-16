# Dimple — Folder icons for Mac

English | [简体中文](README.zh-CN.md)

A macOS menu bar app for giving folders custom icons with a native recessed look. Select folders, choose a badge, and apply—all in one panel.

Dimple is built with Tauri v2, Rust, React, TypeScript, Vite, and Tailwind.

## Features

- Apply an icon to one folder or up to 100 folders per run; folder contents are never touched.
- Choose a built-in badge, import a PNG, static SVG, or JPG/JPEG, or type 1–4 visible characters.
- Preview the result and optionally adjust scale, position, and folder color.
- Open the panel from the menu bar, the global shortcut (**⌥F** by default), a Finder service, or by dragging folders in.
- Clear the custom icon after confirmation to return to the system default; overwriting and clearing are irreversible.
- Work offline with no account, telemetry, or uploads.

## Installation

Dimple is a Universal DMG for Apple Silicon and Intel Macs and requires macOS 13.3 or later. The project has tested macOS 26.6.2 on Apple Silicon and has verified the Universal build on an Intel Mac; other macOS versions and hardware combinations remain untested.

Download the DMG from [GitHub Releases](https://github.com/draph/dimple-mac/releases), open it, and drag **Dimple** to **Applications**. No Python, ImageMagick, or development tools are required.

The app is ad-hoc signed and not notarized. On first launch, open **System Settings → Privacy & Security**, find the Dimple notice, and choose **Open Anyway**; on older macOS versions, Control-click **Dimple** in Finder and choose **Open**. Do not disable Gatekeeper. Verify the download against the SHA-256 published on the release page.

## Permissions

Reading the Finder selection requests Finder automation permission on first use; if you decline, manual selection and drag and drop still work. Dimple does not elevate privileges: each target must be a writable folder, and system-protected directories and application bundles are rejected.

## Usage

1. Select folders in Finder, then open the panel with the menu bar icon or **⌥F**. You can also drag folders in, use the folder picker, or invoke the Finder service.
2. Choose a built-in badge, import an image, or type text; the preview updates in the target card.
3. Click **Apply**, and confirm if existing custom icons would be overwritten.

Input above 100 folders, mixed folders and badges, or an entirely invalid selection is rejected with a message and the previous targets are kept. Partial batch results offer **Retry Failed**, **Continue Remaining**, and **Stop Remaining**. A write that times out is not cancelled; wait for its result.

**Clear Folder Icon** returns folders to the system icon and does not recover previous custom icons. Import limits and rejected SVG features are reported in the panel, and **Adjustments** controls scale, position, and folder color.

**Settings** holds appearance (System/Dark/Light), language (Follow System, 简体中文, English; resolved at startup), the global shortcut, the installed version, and **Check for updates**, which only runs when clicked.

## Limitations and privacy

Dimple works entirely offline: no account, telemetry, or uploads. Its only network access is the manual **Check for updates**, which queries the public GitHub Releases API and opens the release page without downloading or installing anything.

- macOS only; ad-hoc signed and not notarized. Tested on Apple Silicon and Intel; other macOS versions remain untested (see Installation).
- External, network, and cloud-synced folders depend on their own permissions, and icons may not sync across devices.
- The folder template stays Big Sur light with native blue, whatever the system version or interface appearance; folder color is separate from light/dark mode.
- Writing uses macOS file coordination and identity checks; programs that bypass coordination can still replace a path afterwards.
- There is no icon backup or undo, PNG/ICNS export, edit history, preset manager, automatic updates, or plugin marketplace.
- Target paths, imported artwork, and edit drafts are not saved across launches; shortcut, appearance, and language preferences are.
- Full VoiceOver coverage is not part of this release.

## Feedback

Report bugs and feature requests at [GitHub Issues](https://github.com/draph/dimple-mac/issues). Include the app version, macOS version, and architecture when reporting a problem.

## Build from source

Use Node.js 24, npm 12, the Rust toolchain pinned in `rust-toolchain.toml`, and Xcode or Command Line Tools with the macOS SDK. Dependencies are pinned by the npm and Cargo lockfiles.

```sh
git clone https://github.com/draph/dimple-mac.git
cd dimple-mac
source "$HOME/.cargo/env"
npm ci
npm run tauri:dev
```

Run checks and build the native executable (folder-writing tests must use isolated temporary directories):

```sh
npm run typecheck
npm test
npm run build
npm run check:rust
npm run tauri build -- --no-bundle
```

Build the Universal app and DMG:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm run build:universal
```

Outputs are `macos/Dimple.app` and `dmg/Dimple_<version>_universal.dmg` under `src-tauri/target/universal-apple-darwin/release/bundle/`. The build is ad-hoc signed, not Developer ID signed or notarized: `codesign -dv` reports `Signature=adhoc` and `lipo -archs` reports `x86_64 arm64`. Launch the app from Finder or `open path/to/Dimple.app` rather than running the binary inside `Contents/MacOS`. The bundle identifier is `com.draph.dimple`.

## Uninstall

Remove folder icons in the app first if you want the system defaults back. Quit Dimple from the menu bar, then move it to the Trash. Removing the app does not affect icons already applied or restore overwritten custom icons. Optionally delete `~/Library/Application Support/com.draph.dimple/shortcut.json` and `language.json`.

## License and upstream

Project code is [MIT-licensed](LICENSE). The default template is vendored under `src/folder-templates/GenericFolderIcon.BigSur.iconset/`, copied byte-for-byte from the pinned upstream `folderify` commit `16de35dd0ee94be4eae046dde709949b5d779e48`; per-file hashes and provenance are recorded there.

The pinned upstream repository's MIT declaration is the license basis for that template, and the project's own limitation for it is recorded in the [third-party notices](THIRD_PARTY_NOTICES.md), also bundled at `Contents/Resources/THIRD_PARTY_NOTICES.md`. Other third-party assets retain their own licenses.
