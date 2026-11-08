# Folderify Desktop

English | [简体中文](README.zh-CN.md)

> Under development. This is a draft of the first-release user guide; it describes the intended product, not the current implementation status. No public installer is available yet. Download details, supported systems, and signing status will be finalized against the release build before publication.

The current development build supports folder selection, target details and removal, and a built-in code-bracket preview. Targets are validated and deduplicated by directory identity, with per-item errors and symbolic-link details. The built-in badge can be applied, and default icons can be restored after confirmation. Existing custom icons and linked targets require confirmation; overwriting does not keep a backup. Completed results allow a new edit without a dismissal step; invalid input or a canceled picker preserves the old context. After all targets succeed, they are released after about one second and the panel hides unless pinned. Reloading the WebView restores the current operation without replaying writes. PNG/static SVG import is available with safe Alpha-only processing and transparent-margin cropping. Batch retry/resume and the remaining editing/window behavior are still pending. Native writes use system file coordination and identity checks, which cannot prevent replacement by programs that bypass coordination.

A macOS menu bar app for giving folders custom icons with a native recessed look. Select folders, choose a badge, and apply—all in one panel.

Folderify Desktop is an independently maintained GUI project inspired by [lgarron/folderify](https://github.com/lgarron/folderify), not its official desktop app. It uses Tauri v2, Rust, React, TypeScript, Vite, and Tailwind.

## Features

- Apply icons to one folder or a batch of up to 100 folders.
- Choose a built-in icon, import a transparent PNG or static SVG, or enter a one-line badge of 1–4 visible characters.
- Preview changes and optionally adjust scale, position, and folder color.
- Open the panel from the menu bar, a global shortcut, or a Finder context-menu service; drag folders or badge files onto the panel or menu bar.
- Restore the system's default folder icon after confirmation.
- Work offline, with no account, telemetry, uploads, or runtime network dependency.
- Use Simplified Chinese or English, with keyboard and VoiceOver support.

## Installation

The release distribution is intended to provide a Universal app and DMG for Apple Silicon and Intel Macs. Minimum macOS version, tested hardware and storage combinations, download location, SHA-256, and signing/notarization status remain to be confirmed before publication. The version in the build configuration is not a verified support claim.

To install the release, open its DMG, drag **Folderify Desktop** to **Applications**, and launch it. The installed app includes its runtime resources; Python, ImageMagick, Homebrew, and development tools are not required.

Follow the signing and installation notes supplied with that release. An unsigned or unnotarized build must be labeled as a prerelease; disabling macOS security protections is not part of the installation procedure.

## Permissions

Finder automation permission is requested when you first use Finder selection reading. If you decline, you can still select folders manually or drag them into the panel. To change the permission later, open **System Settings → Privacy & Security → Automation** and look for Folderify Desktop's Finder access; system wording varies by macOS version.

The app does not elevate privileges. Each target must be a writable folder; system-protected directories and application bundles are rejected.

## Basic use

1. Select folders in Finder, then click the menu bar icon or press the default shortcut, **Option + F**. You can also drag folders into the panel, use the folder picker, or invoke the Finder service.
2. Choose a built-in badge, import a PNG/SVG, or enter text. The preview updates in the same panel; adjustments are optional.
3. Click **Apply**. If existing custom icons would be overwritten, review the affected targets and confirm.

When Finder is not the active app, use manual selection. Manually selected targets remain when you reopen the panel; **Use Finder Selection** explicitly replaces them. The target summary and expandable list show what will be changed, and individual folders can be removed before applying.

Once all targets succeed, they are released after the success message. The panel hides unless pinned; **Esc** hides an idle panel, and hiding does not quit the app. Use the menu bar to quit or open settings. The global shortcut can be changed or disabled in settings.

### Badges and adjustments

Use the built-in library's name, category, or Chinese/English keywords to search. For imports, use a single PNG or static SVG, no larger than 10 MiB. Rasterized input is limited to 4096 pixels per side and 16 million pixels in total. SVG imports support paths, basic shapes, groups, transforms, gradients, internal references, clipping and static masks. Scripts, external resources, DTD/entities, HTML, animation, text, embedded images, filters, dashed strokes, stylesheets and unknown features are rejected as a whole. SVG must be UTF-8 with the standard SVG namespace and valid dimensions or a viewBox. Inline styles use a restricted set of simple declarations; function-valued paint belongs in presentation attributes. Complexity limits include 20,000 elements/expanded nodes, depth 64, and 200,000 path commands; loading is stopped after 10 seconds. Unsupported content is reported rather than silently omitted.

If an import fails, the previous valid badge and target context remain, but Apply stays disabled until you restore the previous badge or select a valid one. Reloading the WebView preserves this state.

The badge uses the image's transparency, not its color. Transparent margins are trimmed for placement; an opaque PNG produces a solid rectangular badge. Text accepts a single line of 1–4 visible characters in Chinese, English, or digits; whitespace-only input and unavailable glyphs are rejected.

Expand **Adjustments** to change scale, horizontal/vertical position, or folder color. Presets include native blue, red, orange, yellow, green, purple, and gray, with a custom color option. **Reset Parameters** keeps the selected folders and badge and resets the adjustments; it does not change icons on disk.

### Restore defaults and handle partial results

Choose **Restore Default Icon** from the secondary actions menu and confirm the targets. This removes custom icons so Finder uses the system default. **It does not recover previous custom icons.** Overwriting and restoring have no backup or undo feature.

Batch results distinguish successful, failed, and unprocessed targets. **Retry Failed** retries eligible failures; **Continue Remaining** processes targets that have not started, using the original operation and adjustments. Successful targets are skipped. **Stop Remaining** lets the current write finish and stops subsequent targets.

If a system write takes longer than expected, wait for its result; a timeout does not mean the write was canceled. An uncertain result cannot be retried until resolved. Finder may take time to refresh its display after a successful write. You can start a new edit after results are determined; accepting a new edit clears the old retry/continue context.

## Limitations and privacy

- Only macOS is targeted. Tested versions, architectures, and storage combinations will be listed with the release; untested combinations are not promised to work.
- Each operation handles at most 100 folders and does not recurse into their contents. Symbolic links resolve to their actual folder targets and require confirmation.
- External, network, and cloud-synced folders depend on their permissions and filesystem behavior; icons are not guaranteed to sync across devices.
- The folder style is fixed to Big Sur light/native blue as its base, regardless of the running system or interface appearance. Changes to folder color are separate from the app's light/dark interface.
- The writing design uses macOS file coordination and identity checks. Programs that bypass coordination can still replace a path after the final check; this is not a mandatory lock against every process.
- Previewing and adjusting do not write to targets. Applying or restoring changes folder icon metadata, not folder contents.
- Target paths, imported artwork, and editing drafts are not saved across app launches. Shortcut settings are saved. Logs must not include full private paths, artwork contents, or credentials.
- There is no standalone PNG/ICNS export, edit history, icon backup/undo, preset manager, automatic updater, or plugin marketplace.

## Language

The app follows the preferred supported language macOS resolves for it: Simplified Chinese or English, with English as fallback. Change the app language in macOS settings and restart to apply it. There is no in-app language selector. Language affects presentation, not folder names, paths, or user-entered badge text.

## Build from source

Use Node.js 24, npm 12, the Rust toolchain pinned in `rust-toolchain.toml`, and Xcode or Command Line Tools with the macOS SDK. Install Rust from [rustup.rs](https://rustup.rs/) if needed. Dependencies are pinned by the npm and Cargo lockfiles.

```sh
git clone --recurse-submodules https://github.com/draph/folderify-desktop.git
cd folderify-desktop
source "$HOME/.cargo/env"
npm ci
git submodule update --init --recursive
npm run tauri dev
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
npm run tauri build -- --target universal-apple-darwin
```

Build outputs are under `src-tauri/target/`; packaging does not itself establish signing, notarization, or runtime compatibility. `check:rust` runs fmt, Clippy with warnings treated as errors, and unit tests. Folder-writing tests must use isolated temporary directories.

## Uninstall

If you want system default icons, restore them in the app before uninstalling. Quit Folderify Desktop from the menu bar, then move it from **Applications** to the Trash. Removing the app does not remove icons already applied to folders or restore overwritten custom icons. Source checkouts and development toolchains are managed separately.

## License and upstream

Project code is [MIT-licensed](LICENSE). The read-only `upstream/folderify` submodule is pinned to `16de35dd0ee94be4eae046dde709949b5d779e48`; the selected default is `src/resources/folders/GenericFolderIcon.BigSur.iconset/` within that submodule.

The project uses the pinned upstream repository's MIT declaration as the selected template's license basis. Resource sources, the upstream license text, and its documented limitations are in [third-party notices](THIRD_PARTY_NOTICES.md). Other third-party assets retain their own licenses.
