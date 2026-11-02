# Native desktop acceptance

## Status — 2026-11-02

**Partial acceptance only. The native media end-to-end flow remains incomplete.**

Tested the gallery implementation on macOS with a new local test home and a unique Tauri identifier. No production library was used. The gallery was subsequently committed and pushed as `aa35a52`; the acceptance runs did not package or publish an application.

| Check | Result |
| --- | --- |
| Offline, locked Tauri debug build with `custom-protocol` | Passed |
| Database setup and actual native page loading | Observed in a temporary diagnostic build |
| Native WebArea and archive controls in an uninstrumented build | Passed |
| Create a synthetic character through native UI | Passed; independently confirmed in test SQLite |
| Open the real system image picker | Passed |
| Cancel selection without creating assets or file-ledger entries | Passed |
| Select and import an image | Not completed; targeted shortcut delivery unverified, automatic input experiments stopped |
| Render a managed preview through native IPC | Not completed |
| Save image metadata and verify after restarting | Not completed |
| Remove an image / delete a character with images | Not completed |

Checkpoint summary before the manual-selection handoff: one synthetic character, zero assets, zero `media_files` rows, zero foreign-key violations. The synthetic source PNG hash was unchanged; this does **not** prove import preservation because no native import completed. Earlier owned application processes were stopped after their exact executable identity was verified; their exit status 143 reflects deliberate SIGTERM, not a crash finding. On 2026-11-02 the remaining isolated acceptance instance was also stopped after identity verification; no manual-selection session remains active. The native media gate stays incomplete while directory-backup export is developed independently.

## What the investigation established

- The temporary diagnostic build reported `tauri::is_dev() == false`, successful database setup, Started/Finished page events, and a mounted React root. Debug builds do not necessarily use `devUrl`; the `custom-protocol` feature controls this distinction.
- Initial AX reads exposed only window controls. Subsequent reads after enabling accessibility presentation and activating the exact test process exposed `AXWebArea` and real archive controls. Missing WebArea alone is not proof of a blank or missing WebView. The original launch failure's complete cause remains unproven.
- Swift's system-directory API returned a `/tmp` alias despite a `/private/tmp` test home. The actual Tauri diagnostic path did **not** use that alias, and database initialization succeeded. This was not established as the startup cause. Directory no-follow protections were unchanged.
- The bounded diagnostic error buffer observed an `img-src` CSP event, but no JavaScript exception category during its sampled startup. Its source was not identified; CSP was not loosened. This is not a claim of a clean console or successful media decoding.
- The foreground-check failure was reproduced and corrected in the external driver by initializing `NSApplication.shared` before querying `NSWorkspace`. A separate probe then agreed on the target PID through workspace, running-application, and AX-frontmost signals. This was a driver change, not a product fix.
- Targeted CGEvent keyboard navigation still did not expose the Go to Folder panel. A bounded, single-process experiment used the exact app PID and complete modifier-key events; observing an event-post call is not proof of delivery. Existing AX and event-post preflights were available; no new permission was requested. The alternative deprecated `AXUIElementPostKeyboardEvent` API failed to compile because it is unavailable to Swift in this SDK, and was not used. No global keyboard events or C/Objective-C workaround was attempted. Automatic input experiments stopped. No native image import completed, and no manual-selection session remains active.

The diagnostic code existed only in a temporary source copy, behind a non-default feature plus debug/macOS compilation guards. It added no dependencies, diagnostic IPC, network port, filesystem capability, or production logging. The product source was rebuilt without the feature; probe markers were checked absent from the final acceptance executable. Product behavior was not changed during this investigation.

## Reproduction constraints

1. Preserve existing source changes. Use a new local private test root, synthetic images, and a unique identifier unrelated to the production identifier. Do not reuse a real library or personal browser profile.
2. Set both `HOME` and `CFFIXED_USER_HOME` for the launched app. Verify the effective application-support path belongs to the intended test root **before** allowing database initialization or writing test records. Resolve known system aliases for the isolation check only; do not relax production no-follow rules.
3. Use the existing dependency installation and project Tauri CLI:

   ```sh
   CARGO_NET_OFFLINE=true npm run tauri -- build \
     --debug --no-bundle --features custom-protocol \
     --config "$TEST_ROOT/tauri-test-config.json" \
     -- --offline --locked
   ```

   The additional config must contain the unique test identifier. Invoke a verified copy of this specific build with the isolated environment, not an older binary or an ambiguously launched installed application. No Vite server is required for this build mode.
4. Use an AX driver restricted to the exact process, existing permissions, window-first traversal, deduplication, and finite depth/node/time limits. Preserve AX error codes instead of converting failures into empty child lists. Require verified foreground identity before keyboard input. Capture only the test window; do not inspect other applications or global logs.
5. If diagnostic code is needed, keep it in a new source copy, not the product. Validate identifier and test-data root before database setup. Observe bounded stage/error/DOM summaries without paths, full DOM text, arbitrary evaluation endpoints, or raw I/O errors. Rebuild and repeat acceptance without diagnostics afterward.
6. Stop on permission refusal, keychain prompts, isolation mismatch, or unreliable targeting. Do not report an unexecuted step as passed, and do not substitute browser mock IPC for the native chain.

## Backup desktop smoke check — 2026-11-02

A separate uninstrumented build used another new test home and unique identifier. Its actual SQLite library contained zero characters, assets, and file-ledger entries. The native WebArea exposed Settings, the privacy warning, and the consent checkbox; export was disabled before consent. The external AX driver's attempt to press the checkbox found no matching element, so no folder was selected and no export completed. This is an incomplete automation step, not an established product defect. The exact test executable was verified before its process was stopped; no waiting test window remains. No new screenshot-based visual acceptance was performed.

## Windows implementation gate — 2026-11-02

Windows media and backup development is in progress; the feature branch is not a usable Windows release. The target is Windows 10/11 x64 on local fixed NTFS, using synthetic data only. The existing pull request automatically runs the shared CI matrix.

| Revision | Actual Windows result |
| --- | --- |
| `c736806` | Stopped at Clippy; platform-gated unreachable/unused code |
| `68ccf15` | Frontend focus-test correction passed frontend checks on both platforms; old Rust Clippy gate remained |
| `fa5524c` | Frontend, formatting, and strict Clippy passed; Rust tests: 17 passed, 19 failed, none ignored. Failures included unsupported media/backup, rename error 87, and concurrent initialization `DatabaseBusy` |
| `acfd0b8` | Frontend and Clippy passed; backend tests still failed, including initialization `AccessDenied`. Root cause not established |
| `61c7519`, `2d1a4fa` | Stopped at Windows compilation: missing feature/imports and incomplete SID refactoring. These changes have not verified ACL creation or media behavior |
| `5e9ed62` | Windows compilation passed for the first time; strict Clippy then failed with a single `items_after_test_module` lint in `windows.rs`. Backend tests and the desktop build were not reached |
| `324fa9b` | Frontend, formatting, and strict Clippy passed on Windows; tests then failed: 18 passed, 26 failed. Nearly all failures were `Access is denied` while the test fixture created its synthetic library root through the new private-directory path, including the hostile-ACE fixture itself. Root cause not established |
| `571b7ab` | Stopped at Windows Clippy: the new test-only diagnostic helper had a borrow error. Not a product-code failure |
| `2f28f35` | Frontend, formatting, and strict Clippy passed; the staged diagnostics attributed every failure to the first creation stage: `token_sid` succeeds and `Directory::open(create)` itself returns `Access is denied`, before any security-descriptor readback. Root cause not yet fixed; creation parameters are under audit |

The corresponding macOS CI jobs passed. That does not validate Windows-only code. No successful Windows media lifecycle, backup, or desktop build is claimed; WebView2 desktop acceptance is also separate. The earlier hypothesis that numeric SID strings are invalid SDDL was withdrawn: SDDL permits numeric SID strings, and the observed `AccessDenied` requires further diagnosis.

Independent static review of `2d1a4fa` also confirmed unsafe Windows security-descriptor construction: an incorrect absolute-descriptor layout, uninitialized state, an out-of-bounds ACL copy, dangling owner/group SID pointers, a constant-SID construction panic, and missing DACL protection against inheritance. The replacement candidate uses the SDK descriptor type, initialized protected DACL, and shared-lifetime SID/ACL ownership; independent static re-review closed those findings. New no-file SID/descriptor tests and a hostile inherit-only parent-ACL creation test also passed independent static review. Their Windows execution is still pending; neither review nor macOS checks prove Windows compilation or runtime behavior.

New macOS regressions use independent subprocesses and the production `Database::initialize` path with the standard library filename, rather than the serialized test helper. The integrated suite passed 68 Rust tests with no failures or ignored tests, strict Clippy, formatting, 180 frontend tests, and an offline locked debug/no-bundle build. Review confirmed the new tests cover cold migration/recovery, ordinary-operation lock contention, and release after a process exits, is killed, or its test handle drops. The startup marker precedes initialization: a subsequent 100 ms observation does not prove the child has entered the initialization retry branch. None of these macOS results validates Windows code or native GUI behavior.

The Windows ancestor-ACL policy also recognizes the fixed TrustedInstaller service SID as a trusted system identity, alongside the existing SYSTEM/Administrators trust boundary. Its ancestor-only allowance is not a guarantee that every delete-capable mask is rejected. Final private objects still require the current user as owner and grant access only to that user, SYSTEM, and Administrators. This policy does not promise protection from administrators or hostile processes running as the same user; ordinary fixed-NTFS ancestor compatibility still requires runner validation.

## Remaining acceptance gate

In a controllable native desktop session, perform this entire flow against synthetic data:

1. Create a character; open and cancel the native picker.
2. Select a static synthetic PNG; verify the visible preview actually decodes.
3. Save author, plain-text source URL, and notes.
4. Close and restart the same isolated library; verify image and metadata persistence.
5. Cancel then confirm image removal; separately delete a character that still owns an image.
6. Check only the test database using read-only SQLite summaries. Verify asset/ledger ownership, hashes and sizes of original/preview files, foreign keys, cleanup state, and actual removal of the authorized files. Verify source images remain unchanged.

Directory-backup export is implemented and integrated separately on macOS. Its real temporary-library backend tests, frontend mock-IPC tests, and offline locked Tauri build pass; the native folder-picker-to-export desktop flow has not been accepted. See [backup format and limits](backup-export.md). Restore remains unimplemented. Keep using non-critical data until the remaining native flow and backup/restore are complete. Windows media and backup acceptance has not passed; actual Windows CI has exposed compilation and backend failures documented above. Linux remains unvalidated. The documented JPEG recovery behavior and decoder-memory limits are unchanged.
