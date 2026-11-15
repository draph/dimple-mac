# Dimple — Folder icons for Mac

English | [简体中文](README.zh-CN.md)

A macOS menu bar app for giving folders custom icons with a native recessed look. Select folders, choose a badge, and apply—all in one panel.

Dimple is built with Tauri v2, Rust, React, TypeScript, Vite, and Tailwind.

## Features

- Apply icons to one folder or a batch of up to 100 folders.
- Choose a built-in icon, import a PNG, static SVG or JPG/JPEG, or enter a one-line badge of 1–4 visible characters.
- Preview changes and optionally adjust scale, position, and folder color.
- Open the panel from the menu bar, a global shortcut, or a Finder context-menu service; drag folders or badge files onto the panel or menu bar.
- Restore the system's default folder icon after confirmation.
- Work offline, with no account, telemetry, or uploads; only a user-initiated update check contacts GitHub Releases.
- Use Simplified Chinese or English, with keyboard navigation and accessibility labels. Full VoiceOver testing is not part of this release.

## Installation

Dimple is distributed as a Universal DMG for Apple Silicon and Intel Macs. The app is **ad-hoc signed and not notarized**; it does not use a Developer ID certificate. The build floor is macOS 13.3, which covers the Safari 16.4 WebKit features required by Tailwind CSS v4 and the native HTML confirmation dialog. Tested: macOS 26.6.2 on Apple Silicon. The DMG contains an x86_64 slice, but Intel Macs and macOS 13.3–25 have not been verified by the project; untested combinations are not promised to work.

Download the DMG from [GitHub Releases](https://github.com/draph/dimple-mac/releases). Open it, drag **Dimple** to **Applications**, and launch it. The app includes its runtime resources; Python, ImageMagick, Homebrew, and development tools are not required.

On first launch, macOS may block the ad-hoc signed, unnotarized app. Open **System Settings → Privacy & Security**, find the Dimple notice, and choose **Open Anyway**; on older macOS versions, Control-click **Dimple** in Finder and choose **Open**. Do not disable Gatekeeper or other system security protections.

Verify the downloaded file against the SHA-256 published on the corresponding [GitHub Release](https://github.com/draph/dimple-mac/releases) page. The release page also contains the version summary and known limitations.

## Permissions

Finder automation permission is requested when you first use Finder selection reading. If you decline, you can still select folders manually or drag them into the panel. To change the permission later, open **System Settings → Privacy & Security → Automation** and look for Dimple's Finder access; system wording varies by macOS version.

The app does not elevate privileges. Each target must be a writable folder; system-protected directories and application bundles are rejected.

## Basic use

1. Select folders in Finder, then click the menu bar icon or press the default shortcut, **Option + F**. You can also drag folders into the panel, use the folder picker, or invoke the Finder service.
2. Choose a built-in badge, import a PNG/SVG/JPEG, or enter text. The preview updates in the target card at the top; batch targets share the new look, and adjustments are optional.
3. Click **Apply**. If existing custom icons would be overwritten, review the affected targets and confirm.

When Finder is not the active app, use manual selection. Manually selected targets remain when you reopen the panel; open the arrow menu next to **Choose folders** and select **Use folders selected in Finder** to explicitly replace them. The menu supports keyboard navigation; click outside it or press **Esc** to close it. The target summary and expandable list show what will be changed, and individual folders can be removed before applying. With multiple targets, expand the list and choose **Remove all** to clear the targets while keeping the current badge and adjustments.

Finder selection is read automatically only when a hidden panel opens without manual targets or an unfinished operation. Failed automatic reads clear old automatic targets, so an old selection cannot be applied accidentally. Invalid dropped or selected items are reported individually. Entirely invalid input, more than 100 valid folders, mixed folders and badges, or multiple badges preserve the previous targets and badge.

The main window opens on launch and stays open when it loses focus or an operation succeeds. Once all targets succeed, they are released after the success message (about one second). Close the window with **X**, or use the native Hide menu / **Cmd+H**, to hide it without quitting; the current edit stays in memory. Pickers, confirmations, input processing, and unresolved writes prevent hiding. **Esc** does not hide the window; it cancels the active confirmation or shortcut recording, while IME composition and system pickers remain under macOS control.

Open **Settings** from the menu bar or the settings icon in the upper-right corner of the panel. Settings stay unavailable while preparing, confirming, executing, or waiting for an unresolved result. Open them again after the operation finishes. Click **Back** to return to your preserved edit. Icon and text badges have separate tabs; switching tabs keeps the current badge until you select an icon or enter text. The bottom of Settings includes a **Quit** button; quitting from the menu bar remains available. Enable or disable the global shortcut, click the recording field (or press Enter), press a combination including **⌥ Option**, **⌃ Control**, or **⌘ Command**, press **Esc** to end recording, then **Save**. The default is **⌥F**. Recording captures keys without invoking the app's menu actions. Conflicts with another exclusive Carbon registration and save failures show an error and the currently active combinations; a failed rollback can require quitting and reopening the app before another change. The menu bar remains available. macOS can still share combinations registered through legacy nonexclusive APIs or other shortcut mechanisms; not every system-reserved shortcut can be detected.

The menu bar icon menu shows **Show panel** and **Settings**, a separator, **Check for updates**, another separator, and **Quit**.

The compact About section in Settings shows Dimple and the installed package version. **Check for updates** asks the public GitHub Releases API for the highest published version and, when newer, shows it with a **Download** button that opens the release page in your browser. It only runs when you click it; it never downloads or installs updates automatically. The menu bar icon menu has the same entry and opens this section before running the check.

In **Settings → Appearance**, choose **System**, **Dark**, or **Light**. System is the default and follows macOS changes while the app is open. Changes apply immediately and are saved automatically in the app’s local WebView storage; no shortcut save is needed. Dark mode uses neutral black and gray surfaces with blue accents; appearance does not change folder previews or applied icons. To reset this preference, select **System**.

In **Settings → Language**, choose **Follow System**, **简体中文**, or **English**. Follow System is the default and resolves the preferred supported language macOS gives the app when it starts or when you select it again. A manual choice applies immediately to the panel, settings, and native menus, and is saved until you select Follow System. There is no live listener for macOS language changes; restart or select Follow System again to re-resolve it. Language affects presentation, not folder names, paths, or user-entered badge text.

The keyboard shortcut is saved in `~/Library/Application Support/dev.efun.folderify-desktop/shortcut.json`, and the language preference is saved separately in `language.json`. A missing file uses defaults; an unreadable or damaged file uses defaults with a warning and is preserved until you explicitly save. A startup registration conflict preserves the saved shortcut preference. The panel opens near the menu bar icon or on the pointer's screen, fits the available area, and scrolls internally. The opaque app background uses the selected appearance; native system dialogs follow macOS. Multi-display behavior has been manually verified.

### Badges and adjustments

Use the built-in library's name, category, or Chinese/English keywords to search. For imports, use a single PNG, static SVG or JPG/JPEG (case-insensitive extensions), no larger than 10 MiB. Rasterized input is limited to 4096 pixels per side and 16 million pixels in total. SVG imports support paths, basic shapes, groups, transforms, gradients, internal references, clipping and static masks. Scripts, external resources, DTD/entities, HTML, animation, text, embedded images, filters, dashed strokes, stylesheets and unknown features are rejected as a whole. SVG must be UTF-8 with the standard SVG namespace and valid dimensions or a viewBox. Inline styles use a restricted set of simple declarations; function-valued paint belongs in presentation attributes. Complexity limits include 20,000 elements/expanded nodes, depth 64, and 200,000 path commands; loading is stopped after 10 seconds. Unsupported content is reported rather than silently omitted.

If an import fails, the previous valid badge and target context remain, but Apply stays disabled until you restore the previous badge or select a valid one. Reloading the WebView preserves this state.

Original Colors preserves the image background, colors and transparency without shadows or engraving. Opaque images cover the folder front at scale 1 inside a thin folder rim, with overflow cropped around the center and clipped by the rounded face; transparent images keep the badge placement shared with engraving. Monochrome Engraving uses only Alpha and requires transparency; opaque PNG/SVG and JPEG automatically select Original Colors. Transparent PNG/SVG starts with engraving, and replacement images retain the current effect when compatible. Built-in icons and text always use engraving. Transparent margins are trimmed, then colors are downsampled with premultiplied Alpha to a maximum side of 1024 pixels. JPEG uses macOS ImageIO, applies EXIF rotation/mirroring and sends only re-encoded PNG to the UI; RGB and grayscale are supported, while CMYK and other color models are rejected. Structurally incomplete files and decoding/drawing failures are rejected; ImageIO may recover some damaged JPEGs, so this is not a guarantee of detecting every form of corruption. Color processing targets ordinary sRGB, with no professional wide-gamut matching promise. Background removal and a crop editor are not included. Text accepts a single line of 1–4 visible characters in Chinese, English, or digits; whitespace-only input and unavailable glyphs are rejected.

Expand **Adjustments** to change scale, horizontal/vertical position, or folder color. Click **Adjustments** again, click outside the popover, or press **Esc** to close it. Presets include native blue, red, orange, yellow, green, purple, and gray, with a custom color option. **Reset adjustments** keeps the selected folders, badge and effect and resets position, scale and folder color; it does not change icons on disk. Scale ranges from 0.25 to 2 (step 0.01); each offset ranges from −100 to 100 (step 1, two pixels per unit on a 1024-pixel canvas). For opaque original images, scale 1 covers the folder front inside a thin rim; values below 1 shrink the image and can reveal the folder face. Offsets are limited to the image's overflow range, so an axis that exactly fits at scale 1 cannot pan. Colors set hue and saturation while preserving template lightness and alpha, so black, white, and gray inputs produce similar gray folders. Adjustments survive WebView reloads in runtime memory and are cleared when the app exits. Theme colors and engraving are calibrated for the built-in template.

### Restore defaults and handle partial results

Choose **Restore Default Icon** from the secondary actions menu and confirm the targets. This removes custom icons so Finder uses the system default. **It does not recover previous custom icons.** Overwriting and restoring have no backup or undo feature.

Batch results distinguish successful, failed, and unprocessed targets. **Retry Failed** retries eligible failures; **Continue Remaining** processes targets that have not started, using the original operation and adjustments. Successful targets are skipped. **Stop Remaining** lets the current write finish and stops subsequent targets.

If a system write takes longer than expected, wait for its result; a timeout does not mean the write was canceled. An uncertain result cannot be retried until resolved. Finder may take time to refresh its display after a successful write. You can start a new edit after results are determined; accepting a new edit clears the old retry/continue context.

## Limitations and privacy

- Only macOS is targeted. Tested on macOS 26.6.2 on Apple Silicon. The Universal DMG includes an x86_64 slice, but Intel and macOS 13.3–25 are not verified by the project; untested combinations are not promised to work.
- The release is ad-hoc signed and not notarized. macOS may require **Open Anyway** on first launch. There is no Developer ID signature or notarization.
- Full VoiceOver testing and some assistive-technology combinations are not covered in this release.
- Each operation handles at most 100 folders and does not recurse into their contents. Symbolic links resolve to their actual folder targets and require confirmation.
- External, network, and cloud-synced folders depend on their permissions and filesystem behavior; icons are not guaranteed to sync across devices.
- The folder style is fixed to Big Sur light/native blue as its base, regardless of the running system or interface appearance. Changes to folder color are separate from the app's light/dark interface.
- The writing design uses macOS file coordination and identity checks. Programs that bypass coordination can still replace a path after the final check; this is not a mandatory lock against every process.
- Previewing and adjusting do not write to targets. Applying or restoring changes folder icon metadata, not folder contents.
- Target paths, imported artwork, and editing drafts are not saved across app launches. Shortcut, appearance, and language preferences are saved. Logs must not include full private paths, artwork contents, or credentials.
- **Check for updates** is manual and requests the public GitHub Releases API each time; it opens the release page but never downloads or installs anything. The repository and releases must be public. After manually installing a new ad-hoc-signed build, macOS may ask for Finder automation permission again.
- There is no standalone PNG/ICNS export, edit history, icon backup/undo, preset manager, automatic in-app download/install, or plugin marketplace.

## Feedback

Report bugs and feature requests at [GitHub Issues](https://github.com/draph/dimple-mac/issues). Include the app version, macOS version, and architecture when reporting a problem.

## Language

The app follows the preferred supported language macOS resolves for it — Simplified Chinese or English, with English as fallback — unless you choose a language in **Settings → Language**. Choose **Follow System**, **简体中文**, or **English**. A manual choice applies immediately to the panel, settings, and native menus and is saved until you select Follow System again. Follow System is resolved at startup and when selected; macOS language changes are not monitored live. Language affects presentation, not folder names, paths, or user-entered badge text.

## Build from source

Use Node.js 24, npm 12, the Rust toolchain pinned in `rust-toolchain.toml`, and Xcode or Command Line Tools with the macOS SDK. Install Rust from [rustup.rs](https://rustup.rs/) if needed. Dependencies are pinned by the npm and Cargo lockfiles.

```sh
git clone https://github.com/draph/dimple-mac.git
cd dimple-mac
source "$HOME/.cargo/env"
npm ci
npm run tauri:dev
```

Run checks and build the native executable:

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

`build:universal` remaps the builder's Cargo registry and checkout paths out of the release binary. The development command uses `tauri:dev`, which loads the dev-only CSP from `src-tauri/tauri.dev.conf.json`; production builds do not include that CSP. The Universal build outputs are under `src-tauri/target/universal-apple-darwin/release/bundle/`: the app is `macos/Dimple.app` and the DMG is `dmg/Dimple_1.0.0_universal.dmg`. The executable and process are named `Dimple`, matching the app name. The app is ad-hoc signed with hardened runtime disabled; it is not Developer ID signed or notarized. Verify it with `codesign --verify --deep --strict --verbose=2 path/to/Dimple.app`; `codesign -dv` reports `Signature=adhoc`, and `spctl -a -vv` is expected to reject the unnotarized build. `lipo -archs` reports `x86_64 arm64`. `check:rust` runs fmt, Clippy with warnings treated as errors, and unit tests. Folder-writing tests must use isolated temporary directories.

Launch a packaged app from Finder, Spotlight, or `open path/to/Dimple.app`. Running the executable inside `Contents/MacOS` directly can inherit its terminal host's macOS process attribution. On macOS 26, a stale menu-bar association with a disabled host can hide the icon even when Dimple is allowed. Do not reset Control Center or edit its private preferences as a routine installation step. The application identifier remains `dev.efun.folderify-desktop` to preserve settings and system permissions across the rename.

## Uninstall

If you want system default icons, restore them in the app before uninstalling. Quit Dimple from the menu bar, then move it from **Applications** to the Trash. Removing the app does not remove icons already applied to folders or restore overwritten custom icons. Optionally remove `~/Library/Application Support/dev.efun.folderify-desktop/shortcut.json` and `language.json` to reset the saved shortcut and language. Source checkouts and development toolchains are managed separately.

## License and upstream

Project code is [MIT-licensed](LICENSE). The default template is vendored under `src/folder-templates/GenericFolderIcon.BigSur.iconset/`, copied byte-for-byte from the pinned upstream `folderify` commit `16de35dd0ee94be4eae046dde709949b5d779e48`. The build does not require a Git submodule; per-file hashes and provenance are recorded in that directory and in the [third-party notices](THIRD_PARTY_NOTICES.md).

The project uses the pinned upstream repository's MIT declaration as the selected template's license basis. Resource sources, the upstream license text, the complete dependency inventory, and its documented limitations are in [third-party notices](THIRD_PARTY_NOTICES.md); the same file is bundled with the app at `Contents/Resources/THIRD_PARTY_NOTICES.md`. Other third-party assets retain their own licenses.
