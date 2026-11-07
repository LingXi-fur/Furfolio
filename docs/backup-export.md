# Directory backup export

**Status:** export is implemented for macOS. Integrated checks pass: 206 frontend tests, 76 Rust tests, lint, TypeScript, formatting, Clippy, and an offline locked Tauri debug build. Backend tests exercise real temporary SQLite libraries and synthetic media. Native folder selection through completed export has not been accepted end to end. Empty-library restore is implemented and backend-tested on macOS and Windows; native desktop restore acceptance remains unverified. Continue using non-critical data.

This document describes the first export format and its safety boundaries. It is not a claim of completed native desktop acceptance.

## Privacy

Export creates an **unencrypted local copy** of the library, including private notes and original reference images. SQLite pages may also retain historical content. The application does not upload the export; the chosen destination may have its own synchronization or sharing behavior.

Treat the whole directory as private. Export does not implement encryption, automatic backup, retention, or secure deletion. SQLite normalization may discard unused pages; this is not a secure-erasure guarantee.

## Format

The application chooses a new name within a parent directory selected through the native folder picker:

```text
furfolio-backup-YYYYMMDDTHHMMSSZ-<lowercase UUID v4>/
├── library.sqlite3
├── manifest.json
└── media/
    └── <managed media filenames>
```

The database is a consistent standalone SQLite snapshot, not a copy of an active database file and its WAL sidecars. Only media referenced by the snapshot's ownership ledger is included. Unknown source files are not exported.

The versioned manifest records application/schema versions, UTC creation time, counts, payload size, and the size and SHA-256 digest of the database and each media file. Entries use relative paths. The manifest does not duplicate private notes, source URLs, original filenames, or the source library's absolute path.

`totalBytes` means database plus media payload; it excludes the manifest. Hashes detect changes relative to the manifest. They are not a signature or proof of authenticity if someone can replace both the payload and manifest.

## Limits

| Resource | Maximum |
| --- | --- |
| Database snapshot | 64 MiB |
| Characters | 10,000 |
| Reference images | 1,000 |
| Media files | 2,000 |
| Database + media payload | 2 GiB |
| Manifest | 2 MiB |
| Snapshot work | 30-second soft deadline |
| Complete export | 10-minute soft deadline |

These are format and operation limits, not hard process-memory or real-time guarantees. Operating-system I/O can take longer than a software deadline. The initial implementation targets macOS; unsupported export must not disable basic character CRUD on other platforms.

## Consistency and publication

Export holds the media-operation lock while fixing the database snapshot, copying its referenced media, checking the output, and publishing. Ordinary character text edits can continue; they do not change an already fixed snapshot.

Pending imports/deletions, missing ownership records, invalid schema or foreign keys, and changed or missing media cause export to fail. Export does not repair the source library or silently omit broken references.

Output is first written into a new private staging directory within the chosen parent. Before publication, the expected files, identities, sizes, and hashes are checked. Publication must not replace an existing file, directory, or symbolic link. Existing destination permissions are not changed.

Available-space checks are estimates. A later write can still fail. File and directory synchronization does not guarantee power-loss durability on every device or filesystem.

## Results and incomplete output

- **Canceled folder selection:** no success or error is displayed.
- **Success:** show only the generated directory name, character/image counts, and payload bytes. No source or destination absolute paths cross the frontend command boundary.
- **Published, durability uncertain:** the final directory was created, but the parent-directory synchronization failed. Inspect that directory before deciding what to do next. Do not treat it as absent or retry blindly.
- **Incomplete staging directory remains:** a `.furfolio-backup-<UUID>.partial` directory may remain when safe cleanup cannot be established. It is not a successful backup. Unknown contents are not recursively removed, and old partial directories are not automatically cleaned up.

Export is not restore. Restore imports an exported directory into an empty library only and verifies every file against the manifest before materializing it; it never merges with or overwrites an existing library. Native restore acceptance is still unverified, so a successful export is not yet an end-to-end proven recovery path.

## Acceptance requirements

Backend tests must use synthetic images and temporary databases, including committed WAL data, independent read-only reopening without sidecars, manifest/hash verification, source preservation, destination collision sentinels, interrupted writes, and uncertain publication.

Frontend tests cover consent, cancellation, duplicate requests, stable error messages, focus behavior, draft preservation, and result display. Mock IPC tests do not establish native folder selection or filesystem behavior. Native acceptance gaps must remain explicit.
