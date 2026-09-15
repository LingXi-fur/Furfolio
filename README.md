<div align="center">

# Furfolio

**A quiet, private home for furry character records.**<br>
**一个安静、私密、属于你的兽设档案库。**

Local-first · Offline · No account · No cloud · No telemetry

`Furfolio / Character Archive 001`

</div>

---

Furfolio is a desktop character library built for people who want to keep their OCs organized without handing their collection to a cloud service. Names, species, pronouns, tags, signature colors, descriptions, and private notes stay together in a focused local archive.

Its interface treats each character as a record—not as content for a dashboard template. Layered paper, index marks, color swatches, and archive numbering form the visual language while the user's characters remain the only protagonists.

> **Current stage:** Furfolio `0.1.0` is under active development. Character CRUD, search, localization, themes, and the local SQLite foundation are working and tested. Managed reference-image import, credits, and backup/restore UI remain future work.

## What works today

- Create, browse, edit, and delete character records
- Store name, species, pronouns, tags, signature colors, description, and private notes
- Search by character name, species, or tag; press `/` to focus search
- Switch between English and Simplified Chinese
- Follow system appearance or choose light/dark theme
- Persist data in a local SQLite database through typed Tauri commands
- Run without an account, cloud synchronization, analytics, telemetry, or AI
- Use responsive desktop and narrow-window layouts with accessible controls

## Design principles

| Principle | Meaning |
| --- | --- |
| **Characters first** | User-created OCs lead every library card and detail view. Furfolio adds no mascot or invented character appearance. |
| **Local by default** | Records remain on the device. No account or network service is required. |
| **Calm archive language** | Paper layers, swatches, record numbers, index lines, and restrained depth replace glassmorphism and decorative gradients. |
| **Portable ownership** | Complete export and restore are planned as core product capabilities, not premium extras. |
| **Credits matter** | Managed references will preserve artist and source metadata when image support lands. |
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
npm install
npm run tauri -- dev
```

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
```

## Roadmap

- [x] Local SQLite database and versioned migrations
- [x] Character create/read/update/delete flow
- [x] Search, bilingual interface, and light/dark themes
- [x] Responsive archive-style character interface
- [ ] Managed reference-image import
- [ ] Artist credit and source metadata workflow
- [ ] Full-library export and restore
- [ ] Release packaging and migration hardening

## Status and contribution

Furfolio is an early personal product, not a stable public release. Interfaces and storage details may change before `1.0`. Issues and focused suggestions are welcome once the repository opens for wider collaboration.

---

<div align="center">

**Your characters are the collection. Furfolio is only the archive.**<br>
**角色属于你，档案也应该属于你。**

</div>
