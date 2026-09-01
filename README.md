# Furfolio

Furfolio is a private, local-first desktop library for organizing character references, credits, colors, tags, and notes.

The project is in early development. Version 0.1 currently provides the tested desktop foundation, bilingual interface, theme support, and cross-platform build checks. Character storage and managed image import will be implemented in the next milestones.

## Principles

- Local by default: no account, cloud upload, or analytics
- Portable data: the library will support complete export and restore
- Artist credits first: references keep their creator and source metadata
- Cross-platform: macOS and Windows are first-class targets
- Accessible and bilingual: English and Simplified Chinese ship together

## Development

Requirements:

- Node.js 24
- Rust stable
- Platform prerequisites for Tauri 2

```sh
npm install
npm run check
npm run tauri -- dev
```

Run the Rust checks separately:

```sh
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

## Status

Furfolio is not ready for storing real collections yet. The current milestone establishes the application shell, localization, testing, security baseline, and macOS/Windows CI.
