# NotesAI macOS Port Plan

Status: planned, no macOS implementation has started.

This plan is based on the NotesAI 0.4.0 codebase. The first supported target is an Apple Silicon Mac (`aarch64-apple-darwin`) distributed as a directly downloadable app/DMG. Intel and universal builds are a later decision after the native embedding stack is proven on Intel. The Mac App Store is outside the first release because its sandbox adds restrictions around global shortcuts, background behavior, local model connections, and bundled tools.

## Definition of done

The macOS port is complete when a clean Mac can install NotesAI, capture copied text while its window is hidden, index and retrieve notes locally, use Ollama or another configured model, organize notes by topic, open citations, back up to Google Drive with secrets stored in Keychain, start at login when enabled, and pass the cross-platform backup tests. A public build must also be Developer ID signed, hardened-runtime enabled, notarized, stapled, and distributed as a DMG.

## Scope and decisions

- Keep one repository, database schema, React frontend, and Rust backend for Windows and macOS.
- Keep the local SQLite database as the working copy. Google Drive receives compact snapshots; it must never host the live SQLite/WAL files.
- Preserve the existing bundle identifier, `com.notesai.desktop`, unless an Apple Developer identifier conflict is discovered before signing.
- Begin with Apple Silicon. Set the minimum supported macOS version only after the ONNX/FastEmbed spike identifies the real dependency floor; do not claim a version earlier.
- Use a regular app with a menu-bar item and a hidden window at login. Do not build a privileged daemon or system service. The logged-in user session is required for clipboard and shortcuts.
- Capture the clipboard after the user copies content. Reading arbitrary selection from other apps is a separate Accessibility-permission feature and is not required for the first Mac release.
- Use `Command + Option + S` as the default Mac shortcut. Keep shortcut registration errors visible and add configurable shortcuts later if conflicts are common.
- Ship by direct download first. Reconsider the Mac App Store only after the direct-download release is stable.

## Known blockers in the current code

| Area | Current implementation | Required Mac work |
| --- | --- | --- |
| Login startup | `src-tauri/src/background.rs` writes the Windows `Run` registry key | Replace the platform implementation with Tauri's autostart plugin and use its macOS LaunchAgent launcher with `--background` |
| URL opening | `src-tauri/src/lib.rs` launches `explorer.exe` under `cfg(windows)` | Use Tauri's opener plugin or a safe macOS `open` implementation, then test HTTP/HTTPS validation |
| Google credentials | `src-tauri/src/google_auth.rs` encrypts with Windows DPAPI and rejects non-Windows platforms | Store refresh/access tokens in macOS Keychain; keep non-secret OAuth configuration separate |
| Ollama discovery | `src-tauri/src/discovery.rs` can read `OLLAMA_HOST` from the Windows registry | Retain environment/default discovery on Mac and test the Ollama app and CLI launch modes |
| Video reader | `scripts/fetch-reader.mjs`, bundle resources, and `ingest.rs` assume `yt-dlp.exe` | Fetch and verify the macOS release asset, choose the platform binary at runtime, and sign/notarize the bundled executable |
| Packaging | `tauri.conf.json` targets NSIS and only lists `.ico` files | Add platform-specific Tauri configuration, `.icns` assets, app/DMG targets, and Mac bundle metadata |
| Shortcuts | `lib.rs` registers Windows-oriented shortcut strings for every desktop build | Register a platform-specific default and verify conflicts, sleep/wake, login startup, and keyboard layouts |
| Native ML/search | FastEmbed 4.9.1, `ort`/`ort-sys` 2.0.0-rc.9, `sqlite-vec` 0.1.6 | Prove compilation, inference, SQLite registration, search accuracy, packaging, and runtime library loading on Apple Silicon before other work |
| Native smoke tests | `scripts/native-smoke.mjs` assumes Windows executable/process behavior | Add Mac-safe executable paths and assertions without touching the user's real library |

## Phase 0 — Prepare the Mac and establish a baseline

Estimated effort: half a day, excluding large downloads.

1. Clone this repository on the Mac and create a branch such as `codex/macos-port`.
2. Install current Xcode and its command-line tools, Node.js, and Rust through rustup. Confirm the active Rust host is `aarch64-apple-darwin` on Apple Silicon.
3. Run `npm ci`, `npm test`, and `npm run test:native` before changing code.
4. Run a compile-only Tauri build and record every platform error in the pull request or work log.
5. Record the Mac model, CPU architecture, macOS version, Xcode version, Node version, and Rust version used for validation.

Exit gate: frontend tests and platform-neutral Rust tests pass, and the complete initial compiler error list is captured. Do not start packaging or signing yet.

## Phase 1 — Prove the native data and embedding stack

Estimated effort: one to three days. This is the highest technical risk.

1. Make the crate compile for `aarch64-apple-darwin` without disabling FTS5, vector search, embeddings, Google backup, or ingestion merely to obtain a green build.
2. Run the ignored real embedding test so BGE Small downloads, loads through ONNX Runtime, embeds text, and survives a second cached run.
3. Test `sqlite-vec` registration through the existing Rust integration, then run insert, cosine retrieval, delete, and topic-scoped retrieval tests.
4. Build a release binary and launch it outside Cargo. This catches native libraries that compile but are missing from the `.app` bundle.
5. Inspect the `.app` with `otool -L` and confirm that packaged code does not depend on Homebrew-only paths.
6. Measure first-run model download, idle memory, indexing memory, and indexing time for a representative long note. Record results; do not establish performance claims from debug builds.

Decision gate:

- If Apple Silicon works, continue with it as the first release target.
- If ONNX Runtime cannot be packaged reliably, evaluate an updated compatible FastEmbed/ORT pair in a dedicated change. Re-run Windows tests before accepting any dependency upgrade.
- If an Intel build is wanted, test `x86_64-apple-darwin` separately. Current upstream ONNX Runtime packaging has dropped Intel macOS in newer releases, so do not label a build “universal” until both architecture slices pass inference and packaging tests. A pure-Rust embedding backend or two separate builds may be more realistic than a universal binary.

Exit gate: a release-mode app performs local embeddings and hybrid retrieval on the target Mac with no Homebrew runtime dependency.

## Phase 2 — Introduce small platform services

Estimated effort: one to two days.

Create narrow Rust interfaces for startup-at-login, opening external URLs, secure credential storage, shortcut definitions, and bundled-reader discovery. Keep the existing Windows implementations behind `cfg(windows)` and add `cfg(target_os = "macos")` implementations. Shared code should call these interfaces rather than contain scattered operating-system checks.

For startup, use the official Tauri autostart plugin with `MacosLauncher::LaunchAgent` and pass `--background`. Preserve the current behavior: a login launch hides the main window; a normal second launch brings the existing window forward. Add only the plugin permissions needed to check, enable, and disable autostart.

For Google OAuth, use macOS Keychain for tokens. Migration from Windows DPAPI ciphertext is neither possible nor necessary: users sign in once on each Mac. Backups must continue excluding API keys and OAuth tokens.

Exit gate: Windows tests still pass, while Mac startup, URL opening, and Keychain round trips have focused tests or a documented manual test where automation is impractical.

## Phase 3 — Menu bar, lifecycle, clipboard, and shortcuts

Estimated effort: one to two days.

1. Reuse the existing Tauri tray menu as a macOS menu-bar item. Provide Open NotesAI, Capture Clipboard, Pause/Resume Capture, Settings, and Quit.
2. Confirm that closing the main window hides it and leaves the process alive. Quit must unregister shortcuts and terminate normally.
3. Register `Command + Option + S` on Mac and keep the current Windows shortcuts on Windows. A notification should report registration conflicts without crashing startup.
4. Test clipboard text and URL capture from Safari, Notes, VS Code, and another common app.
5. Test cold login startup, a hidden window, a second launch, sleep/wake, screen lock/unlock, and shortcut use after several hours.
6. Request notification permission in context and handle denial without blocking capture.

Do not request Accessibility permission for the first release because copied clipboard content does not require it. Revisit that permission only if direct “capture current selection without copying” becomes a requirement.

Exit gate: with no visible NotesAI window, copying text and pressing the shortcut creates one durable note and produces a notification. Relaunching focuses the existing process; tray Quit stops capture.

## Phase 4 — Make ingestion and model discovery portable

Estimated effort: one to two days.

1. Change the reader fetch script to select a pinned, checksum-verified Windows or macOS `yt-dlp` asset. Keep its upstream and third-party notices in the app bundle.
2. Resolve the platform-specific bundled reader filename in `ingest.rs`; retain PATH fallback for development.
3. Test a normal page, a YouTube video with captions, a video without captions, Reddit OpenGraph content, and expected failure messages.
4. Verify Ollama discovery at `127.0.0.1:11434`, including all installed models, a stopped server, a custom host, and proxy bypass for loopback.
5. Test actual chat, citation repair, and topic organization with at least one small Ollama model on the Mac.
6. Verify LM Studio if it is part of the Mac beta acceptance scope; otherwise document it as unverified rather than claiming support.

Exit gate: capture-to-index and question-to-clickable-citation both pass in the packaged app, and malformed model output cannot silently save topic assignments.

## Phase 5 — Cross-platform data and backup compatibility

Estimated effort: one day.

1. Confirm that Tauri resolves the Mac data directory under the user's Application Support area and display the exact resolved path in diagnostics/help.
2. Add fixtures for a schema-v1 backup and current schema-v2 backup. Test Windows-created backup to Mac and Mac-created backup to Windows.
3. Verify notes, tags, topics, review revisions, URLs, Unicode, timestamps, conflict copies, and re-indexing after restore.
4. Test Google Drive OAuth through the loopback callback and Keychain persistence after restart.
5. Confirm compressed snapshots remain small and contain no API key, client secret, refresh token, access token, embedding, or search index.
6. Keep the warning that syncing the active SQLite database through Drive/iCloud/Dropbox is unsupported.

Exit gate: a Windows backup restores on Mac and a Mac backup restores on Windows without data loss or secret leakage.

## Phase 6 — macOS presentation and accessibility

Estimated effort: one day.

1. Generate a proper `.icns` icon set from the approved master artwork. Inspect 16, 32, 128, 256, 512, and 1024 pixel renderings on light and dark desktops.
2. Check title-bar spacing, traffic-light controls, Retina scaling, compact laptop layouts, keyboard focus, VoiceOver labels, reduced motion, and system light/dark changes.
3. Render shortcut labels as macOS symbols (`⌘⌥S`) without changing their underlying commands.
4. Decide deliberately whether NotesAI should keep a Dock icon during normal use. Avoid making it an agent-only application until window activation and Settings access have been tested from cold launch.

Exit gate: the app remains usable with keyboard-only navigation and VoiceOver, and the icon is recognizable in Finder, Dock, Spotlight, and the menu bar.

## Phase 7 — Package, sign, notarize, and beta test

Estimated effort: one to two days after Apple credentials exist. Apple account approval time is external.

1. Add a macOS Tauri configuration rather than placing Mac-only resources in the Windows NSIS configuration.
2. Create `.app` and `.dmg` artifacts. Test the unsigned artifact locally first.
3. Enrol in the Apple Developer Program and create a Developer ID Application certificate for direct distribution.
4. Enable hardened runtime and add only entitlements demonstrated to be necessary. Sign the app and every nested executable, including the bundled reader.
5. Submit with Apple's current `notarytool`, inspect the notary log, staple the ticket, and validate with `codesign`, `spctl`, and `stapler`.
6. Download the DMG through a browser on a clean Mac user account. Install it from Downloads so Gatekeeper quarantine is genuinely exercised.
7. Run a small beta on at least two Apple Silicon Macs and collect macOS version, startup, permission, model, and crash information without collecting note content.

Exit gate: a clean Mac installs the downloaded DMG without bypassing Gatekeeper, and all acceptance checks pass from the installed app.

## Phase 8 — CI and release automation

Estimated effort: one to two days.

1. Add a macOS GitHub Actions job for frontend tests, Rust tests, release build, signing, notarization, and DMG upload.
2. Cache npm, Cargo, ONNX build artifacts, and the embedding model only where licenses and repository policy permit.
3. Store the signing certificate, certificate password, Apple team ID, and notarization credentials only in encrypted repository secrets.
4. Keep the Windows release job independent so a Mac packaging failure cannot overwrite Windows artifacts.
5. Publish architecture in asset names, for example `NotesAI_0.x.y_aarch64.dmg`. Do not call it universal unless both slices were built and exercised.
6. Add release verification: artifact checksums, signatures, notarization status, version consistency, and an attached software/license inventory.

Exit gate: a version tag reproducibly creates independently downloadable Windows and notarized Apple Silicon macOS artifacts.

## First Mac work session

Use this order when beginning on the MacBook:

1. Record the machine/toolchain details and run the baseline tests.
2. Create the macOS branch.
3. Attempt a native debug compile without changing dependencies.
4. Fix compile blockers with platform guards and small service abstractions.
5. Run the real embedding test and `sqlite-vec` retrieval test.
6. Produce and launch a release-mode `.app` outside Cargo.
7. Stop and document the result of the native dependency gate before implementing menu-bar polish.

The first useful pull request should contain the platform abstractions and a working unsigned Apple Silicon `.app`. Signing, notarization, CI, Intel support, and UI refinements should be separate follow-up changes so failures are easier to isolate.

## Acceptance checklist

- [ ] `npm ci`, frontend tests, and native Rust tests pass on Apple Silicon.
- [ ] BGE embedding downloads once, loads from cache, and indexes locally.
- [ ] FTS5, sqlite-vec, RRF, topic filtering, and exact citation sources pass.
- [ ] Packaged `.app` has no unbundled Homebrew dependency.
- [ ] Menu-bar commands and close-to-background behavior work.
- [ ] Login startup launches with `--background` and no visible main window.
- [ ] `⌘⌥S` captures copied text while the window is hidden.
- [ ] Shortcut and notification failures are understandable and recoverable.
- [ ] Ollama discovery, chat, topic organization, and citation repair pass with a real model.
- [ ] HTTP pages and the bundled macOS video reader pass ingestion checks.
- [ ] Google OAuth tokens survive restart in Keychain and never enter backups.
- [ ] Windows-to-Mac and Mac-to-Windows snapshot restore passes.
- [ ] macOS icon, dark mode, Retina layout, keyboard navigation, and VoiceOver are checked.
- [ ] Every nested executable is signed with hardened runtime.
- [ ] The DMG is notarized, stapled, and accepted by Gatekeeper after browser download.
- [ ] Windows regression tests still pass before merging.

## Risks to track explicitly

| Risk | Mitigation |
| --- | --- |
| ONNX Runtime or FastEmbed compiles but fails after bundling | Make release-mode embedding in the packaged `.app` the first gate; inspect linked libraries |
| Intel Mac support conflicts with current/future ONNX binaries | Ship Apple Silicon first; test Intel separately; consider separate artifacts or another embedding backend |
| Login startup opens a visible or duplicate window | Pass `--background`, retain single-instance handling, and test cold login plus second launch |
| macOS permissions make shortcuts/notifications appear broken | Show permission and shortcut-registration status in Settings; test denial and recovery |
| Keychain migration is confused with backup migration | Require new Google sign-in on each Mac; move note data through snapshots only |
| Bundled `yt-dlp` breaks notarization | Pin and checksum the Mac asset, include licenses, sign nested code before notarization |
| A live database is placed in a cloud-synced folder | Keep the working DB in Application Support and upload atomic compressed snapshots only |
| Mac App Store sandbox blocks core desktop behavior | Use Developer ID + notarized DMG first; evaluate the store as a separate product track |

## Authoritative references

- [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Tauri v2 autostart plugin](https://v2.tauri.app/plugin/autostart/)
- [Tauri v2 global shortcut plugin](https://v2.tauri.app/plugin/global-shortcut/)
- [Tauri v2 system tray guidance](https://v2.tauri.app/learn/system-tray/)
- [Tauri macOS application bundles and DMGs](https://v2.tauri.app/distribute/)
- [Tauri macOS signing](https://v2.tauri.app/distribute/sign/macos/)
- [Apple notarization requirements](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)
- [Apple Developer ID certificates](https://developer.apple.com/help/account/certificates/create-developer-id-certificates)

