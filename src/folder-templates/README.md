# Vendored folder template

`GenericFolderIcon.BigSur.iconset/` contains the ten Big Sur light folder PNGs
used as the application's fixed default template. The files are copied
byte-for-byte from the pinned upstream `lgarron/folderify` commit
`16de35dd0ee94be4eae046dde709949b5d779e48`, path
`src/resources/folders/GenericFolderIcon.BigSur.iconset/`. They are not
original project artwork and must not be edited in place.

The per-file SHA-256 values are recorded in
`GenericFolderIcon.BigSur.iconset/SHA256SUMS`. The accepted license basis and
its limitation (the upstream repository's MIT declaration; no separately
verified Apple authorization) are recorded in `THIRD_PARTY_NOTICES.md`.

The frontend loads these files through the Vite glob in `src/Preview.tsx`.
Keep the file names and byte content stable: changing them invalidates the
visual calibration and the committed goldens. To update from a newer upstream
commit, follow the synchronization procedure in `详细设计.md` section 9.2,
replace all ten files together, record the new commit and hashes, and rerun the
visual baselines before accepting the change. Do not mix a template update with
unrelated changes.
