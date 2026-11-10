# Folderify macOS patch

Source: crates.io `global-hotkey` 0.8.0 (the version already pinned by the app).
`UPSTREAM-SHA256SUMS` records the unmodified source and accompanying files.
The only upstream source changes are in `src/platform_impl/macos/mod.rs`:

- Pass `kEventHotKeyExclusive` to Carbon so another exclusive registration
  produces a registration error instead of silently sharing the combination.
- Keep a hotkey handle until OS unregistration succeeds, so a failed release
  cannot turn a retry into a false success.

The existing Tauri global-shortcut plugin still owns registration and event
routing. No dependency or runtime network service was added. Carbon allows
legacy nonexclusive registrations to coexist even with exclusive registrations;
this patch cannot detect all system-reserved keys or other shortcut mechanisms.

Reassess both changes against upstream when upgrading; remove this patch when
upstream provides equivalent semantics. Verify exclusive conflict reporting,
change/disable/restart, capture forwarding, and rollback before upgrading.
Selected license: MIT (`LICENSE-MIT`); upstream also offers Apache-2.0.
