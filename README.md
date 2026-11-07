<div align="center">

# Furfolio

**A quiet, private home for furry character records.**<br>
**一个安静、私密、属于你的兽设档案库。**

Local-first · Offline · No account · No cloud · No telemetry

`Furfolio / Character Archive 001`

</div>

---

Furfolio is a desktop character library built for people who want to keep their OCs organized without handing their collection to a cloud service. Names, species, pronouns, tags, signature colors, descriptions, and private notes stay together in a focused local archive.

Its interface treats each character as a record—not as content for a dashboard template. A warm, soft visual language — a cream canvas, rounded white cards, and cherry-blossom and sage accents — keeps the archive calm while the user's characters remain the only protagonists.

> **Current stage:** Furfolio `0.1.0` is under active development. Character CRUD, managed reference images, and directory-backup export (macOS and Windows backend) are implemented. Backend tests run on both platforms in CI with real temporary SQLite libraries and synthetic media; frontend tests use mock IPC. Native desktop end-to-end acceptance is still incomplete, and the Windows desktop flow is unverified. Restore is not implemented.

## What works today

- Create, browse, edit, and delete character records
- Store name, species, pronouns, tags, signature colors, description, and private notes
- Search by character name, species, or tag; press `/` to focus search
- Navigate through a sidebar table of contents with archive plate numbering, page-turn view transitions, and theme cross-fades
- Switch between English and Simplified Chinese
- Follow system appearance or choose light/dark theme
- Persist data in a local SQLite database through typed Tauri commands
- Run without an account, cloud synchronization, analytics, telemetry, or AI
- Use responsive desktop and narrow-window layouts with accessible controls
- Manage reference images in the detail-view frontend through typed IPC: native single-image selection, managed PNG previews, inline artist/source/notes editing, and confirmed removal
- See persistent, retryable image-cleanup warnings without keeping already-deleted records visible
- Export an unencrypted whole-library directory on macOS or Windows through Settings, with a consistent SQLite snapshot, referenced media, and a versioned SHA-256 manifest

### Managed reference gallery

The gallery imports one static PNG, JPEG, or WebP at a time through a Rust-native file picker (20 MiB, 12,000 pixels per side, 24 MP). Canceling is not an error. Rust copies the original bytes into managed storage and generates a separate PNG preview; the source file is not modified. The frontend never receives source or database paths. Previews are capped at 4 MiB, with one preview request in flight and Blob URLs released on selection changes or unmount.

Artist credits (200 characters), source URLs (2048), and image notes (10000) are trimmed before submission. Source URLs accept HTTP(S) without credentials and remain plain text: no automatic links, fetches, or remote previews. Media operations lock the affected character against duplicate mutations and character deletion, while navigation remains available. Committed deletions with pending file cleanup update the visible archive immediately and retain a global retry warning, including across navigation.

An append-only schema migration adds a file-ownership ledger for interrupted imports and deletions. Cleanup handles only ledger-authorized files, not arbitrary directory contents. A partially written or externally changed file can remain pending rather than be deleted unsafely; retry cannot repair mismatched content automatically.

**Current limits:**

- Managed media I/O and whole-library backup export are implemented and CI-tested on macOS and on Windows 10/11 x64 with local NTFS volumes (latest run: both jobs green, 68 backend tests each). The Windows capability layer walks component names handle-relative, so media and backup paths stay verbatim-safe; SQLite itself does not prepend `\\?\`, so a library root past the 260-character MAX_PATH cannot open its database. Windows support is backend-verified only: the native desktop flow (folder picker, preview rendering in WebView2, restart, and end-to-end export) has not been exercised on a real Windows desktop. Linux is not validated and fails closed.
- JPEG compatibility is retained deliberately: the decoder may recover some damaged or truncated JPEGs by synthesizing pixels. Import success is **not** proof of JPEG integrity. Original bytes remain unchanged. The byte/dimension/pixel limits still apply.
- The 128 MiB decoder allocation budget is not a hard process-RSS or all-scratch-memory limit, particularly for WebP.
- Native picker → import → restart → edit → remove acceptance is incomplete. On 2026-11-01, a fresh, uninstrumented desktop build exposed a usable native WebArea, created a synthetic character, and opened/canceled the system image picker in an isolated library. The external driver's foreground check was corrected, but targeted keyboard navigation still did not open the path panel. Automatic input experiments stopped; no native image import completed. The remaining isolated test instance was stopped on 2026-11-02. See [native acceptance](docs/native-acceptance.md). Browser exercises use synthetic IPC and cannot prove native file handling.
- Avatar selection/cropping, original-image viewing, batch import, and restore are not included. Continue using non-critical data.

### Directory backup export

Settings offers an unencrypted library export on macOS and Windows (local NTFS). After acknowledging the privacy warning, choose a parent directory through the native folder picker. Furfolio writes a new directory containing a consistent `library.sqlite3`, its referenced original images/previews, and a versioned `manifest.json` with relative paths, sizes, and SHA-256 digests. Existing output is never overwritten; canceling is not an error.

The copy includes private notes and original images; SQLite pages may retain historical content. Keep the whole directory private. Export does not implement restore, encryption, or automatic backup. A published-but-durability-uncertain result is distinct from failure and must be inspected before retrying. Backend tests cover real temporary libraries and synthetic media, but the native folder-picker-to-export desktop flow remains unvalidated. See [format, limits, and failure semantics](docs/backup-export.md).

## Design principles

| Principle | Meaning |
| --- | --- |
| **Characters first** | User-created OCs lead every library card and detail view. Furfolio adds no mascot or invented character appearance. |
| **Local by default** | Records remain on the device. No account or network service is required. |
| **Calm archive language** | Paper layers, swatches, record numbers, index lines, and restrained depth replace glassmorphism and decorative gradients. |
| **Portable ownership** | Complete export and restore are planned as core product capabilities, not premium extras. |
| **Credits matter** | The reference-gallery frontend keeps artist credits, source text, and image notes alongside each managed reference. |
| **Accessible together** | Keyboard use, meaningful control names, responsive layouts, and English/Chinese copy ship as one interface. |

## Privacy model

Furfolio is intentionally small in trust surface:

- No sign-in
- No cloud database
- No telemetry or behavioral analytics
- No AI processing
- No silent upload path
- Restrictive Tauri content security policy
- SQLite bundled with the app for local persistence

Furfolio is still pre-release software. Use development builds with non-critical data until export and restore workflows are complete.

## Technology

| Layer | Stack |
| --- | --- |
| Interface | React 19, TypeScript, Vite 7 |
| Desktop runtime | Tauri 2 |
| Local storage | Rust, rusqlite, SQLite |
| Localization | i18next, react-i18next |
| Testing | Vitest, Testing Library |
| Quality | ESLint, TypeScript, Cargo fmt/clippy/test |

```text
React interface
      │ typed invoke commands
      ▼
Tauri command boundary
      │ validated inputs / structured errors
      ▼
Rust character service
      │ versioned local schema
      ▼
SQLite on this device
```

## Development

### Requirements

- Node.js 24
- Rust stable
- [Tauri 2 platform prerequisites](https://v2.tauri.app/start/prerequisites/)

### Start the desktop app

```sh
npm ci --ignore-scripts --include=optional
npm run tauri -- dev
```

Keep platform-specific optional packages enabled. The locked toolchain uses their prebuilt binaries without dependency lifecycle scripts; do not copy `node_modules` between platforms.

### Run frontend checks

```sh
npm run check
```

This runs ESLint, Vitest, TypeScript checks, and the Vite production build.

### Run Rust checks

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

## Project layout

```text
src/
├── components/        Character library, forms, covers, details, dialogs
├── data/              Narrow typed Tauri IPC client
├── i18n/              English and Simplified Chinese copy
├── App.tsx            Application state and navigation
└── App.css            Furfolio archive visual system

src-tauri/src/
├── characters.rs      Character persistence operations
├── commands.rs        Tauri command boundary
├── database.rs        SQLite setup and migrations
├── error.rs           Stable command errors
├── models.rs          Rust data contracts
└── lib.rs             Runtime wiring

design/                Original character-diagnostic SVG studies (not app assets)
```

## Roadmap

- [x] Local SQLite database and versioned migrations
- [x] Character create/read/update/delete flow
- [x] Search, bilingual interface, and light/dark themes
- [x] Responsive archive-style character interface
- [x] Reference-gallery frontend and typed native-import IPC client
- [x] Artist credit, source text, image notes, and cleanup-retry UI
- [ ] Native media integration and isolated desktop end-to-end acceptance
- [x] macOS and Windows (backend) directory-backup export and manifest validation
- [ ] Native backup desktop end-to-end acceptance and restore
- [ ] Release packaging and migration hardening

## Status and contribution

Furfolio is an early personal product, not a stable public release. Interfaces and storage details may change before `1.0`. Issues and focused suggestions are welcome once the repository opens for wider collaboration.

---

<div align="center">

**Your characters are the collection. Furfolio is only the archive.**<br>
**角色属于你，档案也应该属于你。**

</div>
