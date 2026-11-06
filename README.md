# Folderify Desktop

English | [简体中文](README.zh-CN.md)

A macOS menu bar app for customizing folder icons, built with Tauri v2, React, TypeScript, Vite, and Tailwind.
An independently maintained Folderify GUI project (folderify-gui), inspired by [lgarron/folderify](https://github.com/lgarron/folderify); this is not its official desktop app.

This is currently the T01 engineering prototype: a startup panel and a read-only native connection check. It cannot customize folder icons yet.

## Installation and usage status

No production version is available for installation. Start the prototype from source using the instructions below. It only displays engineering status; target selection, previews, and icon application are not implemented.

Finder selection is not implemented, so the prototype does not request Finder automation permission. The planned behavior is to request permission only when reading Finder selections, with manual selection available if permission is denied. Chinese and English UI support is planned; the current prototype UI is Chinese.

## Development

Verified local environment: Apple Silicon (arm64), macOS 26.6.2, Xcode 27.2 (SDK 27.2), Node 24.18.0, npm 12.2.2, and Rust 1.99.1.
The project directory is `folderify-desktop`; the application name is Folderify Desktop.
Rust is pinned in `rust-toolchain.toml`; npm and Cargo lockfiles pin dependencies.

```sh
# Install Rust from https://rustup.rs/ if needed, then load its environment.
source "$HOME/.cargo/env"
npm ci
git submodule update --init --recursive
npm run tauri dev
```

```sh
npm run typecheck
npm run build
npm run check:rust
npm run tauri build -- --no-bundle
# Future delivery builds require both Rust targets.
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm run tauri build -- --target universal-apple-darwin
```

`npm run check:rust` runs fmt, Clippy (warnings are errors), and unit tests. Frontend type checks cover both the UI and Vite configuration.
There are no frontend business-logic tests yet; the native session interface has a serialization test.
All future folder-writing tests must use isolated temporary directories.

## Upstream and assets

Read-only reference submodule: `upstream/folderify`, from https://github.com/lgarron/folderify.
Pinned commit: `16de35dd0ee94be4eae046dde709949b5d779e48`.
Templates are under `src/resources/folders/`, including Big Sur and Tahoe multi-size iconsets.
The repository's MIT notice alone does not establish redistribution rights for Apple images. The default template remains undecided; the application currently does not reference or bundle these assets.

## Engineering prototype verification (2026-11-06)

These are historical T01 results. Checks after renaming are recorded separately in the task document. Build artifact names below reflect the new project name.

- `npm run typecheck`, `npm run build`, and `cargo check --manifest-path src-tauri/Cargo.toml` passed.
- `npm run check:rust` passed: fmt, Clippy with no warnings, and one unit test.
- `npm run tauri dev` compiled and started. A development-only CSP permits local Vite hot reload connections.
- `npm run tauri build -- --debug --bundles app` passed. The debug app path is `src-tauri/target/debug/bundle/macos/Folderify Desktop.app`.
- Native UI inspection confirmed that the local `tauri://localhost` page displayed a successful native connection for v0.1.0.
- The T01 debug app was 28.36 MiB; this is not a release size or final delivery measurement.

## Current limitations

- Target selection, icon generation, menu bar integration, Finder integration, and applying or restoring folder icons are not implemented.
- macOS 10.15 in the configuration is a candidate framework build minimum, not a verified product support claim. Only the local environment is available for verification.
- Intel and minimum-version runtime checks, Universal builds, performance, signing, notarization, and installer acceptance remain unverified.
- There is no public release, and Finder permission is not requested automatically. The application icon comes from the official project template; the product icon is not finalized.

## Removing the development prototype

Stop `npm run tauri dev` and delete any debug app bundle you copied elsewhere. The prototype does not persist targets, assets, or settings, and cannot write folder icons. Source files and development toolchains are managed separately; deleting the app does not remove them.

## License

Project code is licensed under MIT; see [LICENSE](LICENSE). Upstream and third-party assets require separate review. Third-party notices for the application bundle will be completed during packaging.
