# Built-in icon library

All 20 SVG badges in this directory are original geometric artwork created for
Dimple with AI assistance, covered by the project's
[MIT license](../../LICENSE) (Copyright (c) 2026 efun). They contain no fonts,
brand logos, SF Symbols, or external resources, and are not traced from a
third-party icon collection.

`icons.json` records each stable ID, localized name key, category, bilingual
keywords, source path, and license. There are five icons each for development,
tools, arrows, and media. Display names and categories live in `src/locales/`.
SVG thumbnails are bundled by Vite; selected badges pass through the same Rust
SVG validation and Alpha normalization as imports. Embedded Rust assets are
checked against the manifest by a test. No runtime network access is needed.
