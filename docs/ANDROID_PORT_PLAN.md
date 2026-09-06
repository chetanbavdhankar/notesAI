# NotesAI Android Port Plan

Status: planned. No Android implementation has started.

This plan is based on NotesAI 0.4.0. Android should reuse the React interface, Rust domain logic, note/backup schema, retrieval rules, model profiles, topic organization, and citation behavior where the mobile toolchain supports them. Android-specific capture, lifecycle, secure storage, scheduled work, and distribution belong in Kotlin/Tauri plugins and the generated Android project.

## Product decision: how capture should work

The first Android release should offer several legitimate, user-initiated capture paths:

1. **Android Sharesheet — primary path.** From a browser, Reddit, YouTube, X, Photos, or another app, tap Share → NotesAI. The capture screen opens as a small confirmation sheet with Save as the focused action. Support `text/plain`, shared URLs, `image/*`, and later PDFs. Allow editing and topic selection, but make them optional.
2. **Selected-text action — fastest text path.** Register for Android's selected-text processing action so NotesAI appears beside Copy when a source app exposes selected text. One tap should write a durable inbox record and show a short confirmation.
3. **Quick Settings tile — clipboard path.** A tile named Capture to NotesAI opens a lightweight foreground capture activity. It should read the clipboard only after this explicit interaction and show the captured preview. Modern Android restricts background clipboard reads, so a tile that silently reads the clipboard without a foreground UI must not be promised until it is proven on all supported versions.
4. **Home-screen widget/app shortcut — quick entry.** Provide New note, Paste and save, Scan text, and Open assistant actions. A widget is useful when nothing is currently selected or shareable.
5. **Notification action — optional.** A user-enabled, low-priority persistent capture notification can expose New note and Paste. It should be optional because permanent notifications add clutter.

Do not make a floating overlay or Accessibility Service part of the base product. An overlay requires special display-over-other-apps permission, is intrusive, and cannot by itself retrieve text from the underlying app. Reading the active app's view hierarchy requires Accessibility access to potentially private screen content and creates substantial disclosure and Google Play review obligations. If a floating edge control is ever prototyped, it should only open a capture sheet or accept already copied text; it must not secretly scrape the screen.

Recommended capture target: Share → NotesAI → Save should take two deliberate taps the first time. After Android learns the user's share habits, NotesAI should remain easy to reach. Selected text → NotesAI should be the closest equivalent to a desktop hotkey.

## Definition of done

The Android port is complete when a Play-distributed build can receive shared text, URLs, and images; immediately save them into a durable local inbox; hydrate and index them under Android lifecycle limits; search and chat with clickable citations; organize notes; synchronize note changes through Google Drive; recover from offline use and process death; and pass cross-device conflict, deletion, security, and backup tests.

## Known codebase blockers

| Area | Current desktop assumption | Android work |
| --- | --- | --- |
| Layout | `App.tsx` uses a three-panel desktop shell with a 920px minimum | Add mobile navigation, full-screen note/chat routes, bottom navigation, sheets, and safe-area handling |
| Capture | Global keyboard shortcuts and tray process | Add Kotlin share receiver, selected-text action, Quick Settings tile, widget/app shortcuts, and a durable capture inbox |
| Background work | An always-running Rust worker polls SQLite | Use Android WorkManager for deferred hydration/index/sync and foreground execution only for user-visible long work |
| Clipboard | Desktop background process reads clipboard | Restrict reads to visible, user-initiated capture flows; never poll the clipboard in the background |
| Native ML | FastEmbed 4.9.1 with ONNX Runtime and sqlite-vec desktop builds | Prove Android ARM64 compilation, packaged inference, memory use, and sqlite-vec registration on a physical phone |
| Video hydration | Bundled Windows `yt-dlp.exe` | Do not execute the desktop binary; use portable metadata extraction and define a compliant transcript strategy |
| Google auth | Desktop OAuth loopback flow and Windows DPAPI | Use an Android OAuth client, native authorization flow/PKCE, and Android Keystore-backed token storage |
| Drive | Compact backup uploads, not continuous sync | Add multi-device change exchange, cursors, tombstones, conflicts, and attachment deduplication |
| Local providers | Ollama/LM Studio default to loopback on a computer | Treat phone loopback as the phone; support remote providers and optional user-configured LAN endpoints with clear privacy/network behavior |
| Tauri plugins | Several desktop plugins are initialized unconditionally | Audit official mobile support; guard desktop-only plugins and add Kotlin/Swift mobile plugin bridges where needed |

## Phase 0 — Android toolchain and compile inventory

Estimated effort: one day.

1. Start on the MacBook or another supported development machine after the repository baseline passes.
2. Install Android Studio, its bundled JDK, current Android SDK platform/tools, NDK, command-line tools, and Rust Android targets following Tauri's current prerequisites.
3. Create a dedicated branch such as `codex/android-port` and initialize Tauri's Android project.
4. Run the shared frontend and Rust tests before edits, then attempt an Android debug build.
5. Record every unsupported crate/plugin and generated-project change. Do not remove core features merely to produce an APK.
6. Select a realistic minimum Android API only after checking Tauri/WebView, security, WorkManager, and native ML requirements. Test the current Play target API separately from the minimum runtime API.

Exit gate: the baseline is recorded, the generated Android project is understood, and every native/plugin blocker has an owner and fallback.

## Phase 1 — Mobile shell and local database

Estimated effort: two to four days.

1. Add a responsive application shell: bottom navigation for Library, Capture, Assistant, and Settings; full-screen note detail; slide-up filters and reference drawer.
2. Preserve desktop layout behind responsive breakpoints rather than forking the React app.
3. Initialize the same logical note schema in app-private storage. Verify FTS5, WAL behavior, migrations, topic tables, transactions, and Unicode on emulator and physical ARM64 hardware.
4. Suspend polling when appropriate and wake the queue explicitly after capture, app resume, sync import, or scheduled work.
5. Handle process death at every queue state. A raw capture must be durable before the share UI reports success.

Exit gate: captures and edits survive force-stop/relaunch, schema migrations work, and desktop browser/Windows tests remain green.

## Phase 2 — Capture surfaces

Estimated effort: three to five days.

Build a small Android-specific ingestion bridge that validates incoming intents and writes normalized inbox records through one shared command. Accept bounded content only, copy shared content URIs while temporary permission is valid, reject unsupported MIME types clearly, and never trust the sender's declared type or filename.

Implement in this order:

1. `ACTION_SEND` for text and URLs.
2. `ACTION_SEND`/`ACTION_SEND_MULTIPLE` for images with content-URI permission handling.
3. Selected-text processing for source apps that expose it.
4. A Quick Settings tile that opens a foreground confirmation sheet.
5. Pinned app shortcuts and a compact home-screen widget.

Measure taps and latency. The confirmation sheet should render from native data immediately and queue hydration afterward. Keep an optional “Save immediately without editing” preference, but always show a notification/toast with Undo so accidental shares are recoverable.

Exit gate: Chrome, Firefox, Reddit, YouTube, Photos, Files, and at least two common social apps can share supported content into one durable note without opening the full library.

## Phase 3 — Native search and embedding feasibility

Estimated effort: three to seven days. This is the largest Android engineering risk.

1. Build and run `rusqlite`/FTS5 and `sqlite-vec` for `arm64-v8a`; test insert, vector query, delete, restore, and topic-scoped hybrid retrieval.
2. Test the pinned FastEmbed/ORT stack on a physical ARM64 phone in release mode. Record binary size, first model download, cold load, peak memory, indexing time, thermal behavior, and battery use.
3. Confirm that native libraries are present in the AAB splits and load without relying on files outside the application package.
4. If the current stack fails, evaluate a mobile-supported ONNX Runtime build or a native mobile embedding adapter. Any dependency/model change must reproduce 384-dimensional output compatibility or trigger a controlled local re-index.
5. Do not synchronize vector indexes as the normal design. Synchronize source records and rebuild derived chunks/FTS/vectors locally so architecture and library versions can differ safely.
6. If acceptable on-device dense embeddings are not ready for the first beta, ship capture/browse/edit/sync plus FTS5 search, and label vector chat as unavailable. Do not silently send notes to a cloud embedding service in a local-first app.

Exit gate: either local hybrid retrieval passes defined performance limits on a mid-range physical phone, or the beta scope explicitly states lexical-only retrieval while preserving the data needed for later indexing.

## Phase 4 — Hydration, images, models, and RAG

Estimated effort: three to six days.

1. Reuse portable HTML/OpenGraph hydration and enforce network timeouts, size limits, redirect limits, and private-network protections.
2. Replace desktop `yt-dlp` execution with a mobile-safe strategy. For the first release, saving the URL plus available page metadata is acceptable; transcript support must be independently proven and compliant before it is claimed.
3. Add image capture storage using content hashes, bounded dimensions, thumbnail generation, OCR, and later vision description. Keep the original only if the user chooses or storage policy permits.
4. Support remote OpenAI-compatible profiles. Allow an Ollama/LM Studio server on the user's LAN only through an explicit URL; explain that `127.0.0.1` points to the phone, not the laptop.
5. Preserve the organizer's review-before-save rule and citation coverage checks. A mobile citation opens the exact passage and then the full note.
6. Keep remote-provider disclosure beside configuration: questions and retrieved passages leave the device when a remote profile is used.

Exit gate: supported captures hydrate safely, organization cannot auto-commit malformed output, and RAG answers either cite valid local passages or state that retrieval is unavailable.

## Phase 5 — Android background execution

Estimated effort: two to four days.

1. Replace the always-running worker assumption with an explicit job queue and WorkManager jobs for hydration, embedding, maintenance, Drive sync, and retry.
2. Make every job idempotent and revision-aware. Android may delay, stop, or retry it.
3. Use network and battery constraints appropriately. Offer “index while charging” for expensive backlogs.
4. Use expedited/foreground work only after a direct user action and show system-required progress. Do not keep a hidden permanent service merely to imitate desktop behavior.
5. Test airplane mode, metered connections, Doze, low battery, reboot, force-stop, app update, and revoked notification permission.

Exit gate: captured data is never lost, queued work eventually resumes under normal Android conditions, and no core feature depends on an immortal process.

## Phase 6 — Google Drive multi-device sync

Estimated effort: five to ten days. This is a new synchronization system, not a small extension of backup.

The existing rotating compressed snapshots are suitable for recovery but insufficient for live multi-device use. Add a versioned sync layer:

- Stable note IDs, `updated_at`, content revision/hash, deleted-note tombstones, device ID, and attachment hashes.
- Immutable compressed change batches per device with monotonically increasing sequence numbers.
- A local cursor for every observed device so only new batches download.
- Deterministic merge: accept non-conflicting changes; preserve both versions for concurrent edits; never silently overwrite divergent content.
- Content-addressed, deduplicated image attachments uploaded separately from note metadata.
- Periodic compact snapshots plus a conservative retention policy. Prune old batches only when recovery remains possible.
- Transactional import into the local database, followed by local re-indexing of changed notes.

Use an Android OAuth client and PKCE/native authorization. Store tokens with Android Keystore-backed secure storage. Share extensions/receivers must enqueue while offline without requiring sign-in UI. Provide Sync now, last-success time, pending count, conflict list, and understandable errors.

Exit gate: two phones plus one desktop can create, edit, delete, and organize notes offline, then converge without lost updates. Storage growth is measured across thousands of text notes and repeated images.

## Phase 7 — Privacy, security, and Play policy

Estimated effort: two to four days plus review time.

1. Create a data inventory for note text, URLs, images, model-provider transfers, OAuth tokens, diagnostics, and Drive files.
2. Publish a privacy policy and complete Play's Data safety declarations accurately.
3. Use Android Photo Picker for manual image selection where practical and request only permissions used by a visible feature.
4. Keep database, attachment files, tokens, logs, and backups out of Android Auto Backup unless their encryption and restore behavior are deliberate.
5. Threat-test malicious share intents, oversized images, unsafe content URIs, HTML prompt injection, deep links, exported components, and local-network endpoints.
6. Avoid Accessibility Service in the Play build. If it is ever proposed, require a separate policy review, prominent disclosure/consent design, security assessment, and proof that the intended use is permitted before code is merged.

Exit gate: least-privilege permissions, security tests, privacy disclosures, and Play declarations agree with actual behavior.

## Phase 8 — Testing and release

Estimated effort: three to five days plus Play review.

1. Test at least one current Pixel-class device, one Samsung device, one mid-range device, and emulator API levels at the chosen minimum/current boundaries.
2. Add Android unit/instrumentation tests for intent parsing, content URI lifetime, process death, migrations, jobs, OAuth redirects, sync merges, and citation navigation.
3. Generate adaptive icon, monochrome themed icon, splash assets, screenshots, and phone/tablet layouts.
4. Configure release signing with secrets outside the repository and build an Android App Bundle (`.aab`).
5. Use internal testing, then closed testing, before production. Exercise an update from the prior beta without clearing local data.
6. Add GitHub CI for shared tests and unsigned build verification; keep Play signing/release promotion gated.

Exit gate: the Play-installed build passes capture, offline, process-death, sync, model-disclosure, update, and deletion scenarios on physical devices.

## First Android work session

1. Complete the macOS/Android toolchain setup and record versions.
2. Branch from the current tested master.
3. Initialize Tauri Android and attempt an unmodified debug build.
4. Classify compile failures as desktop plugin, native library, packaging, or application lifecycle issues.
5. Get SQLite/FTS5 running before redesigning the UI.
6. Implement a minimal text share receiver that durably stores one note.
7. Stop and review the native embedding spike before promising full RAG parity or a release date.

## Acceptance checklist

- [ ] Shared frontend and Rust tests still pass on desktop.
- [ ] Android debug/release builds run on a physical ARM64 device.
- [ ] Text, URL, image, and selected-text captures are durable before confirmation.
- [ ] Quick Settings and widget actions obey clipboard/privacy restrictions.
- [ ] Force-stop, reboot, Doze, offline mode, and app update do not lose notes.
- [ ] SQLite migrations, FTS5, topics, and citations work locally.
- [ ] Dense embedding support is proven or clearly excluded from the beta.
- [ ] Ollama/LAN and remote-provider behavior is accurately explained.
- [ ] Drive sync handles offline creates, edits, deletions, conflicts, and deduplicated attachments.
- [ ] No OAuth token, API key, or note body appears in logs or unintended backups.
- [ ] Accessibility Service and system-wide screen scraping are absent from the baseline build.
- [ ] Privacy policy, Data safety form, permissions, and actual behavior agree.
- [ ] Signed AAB installs through Play internal/closed testing and survives an upgrade.

## Authoritative references

- [Tauri mobile prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Tauri 2 mobile architecture and native plugins](https://v2.tauri.app/blog/tauri-20/)
- [Tauri mobile plugin development](https://v2.tauri.app/develop/plugins/)
- [Android receiving shared content](https://developer.android.com/develop/ui/compose/sharing/receive)
- [Android background clipboard privacy](https://developer.android.com/privacy-and-security/risks/secure-clipboard-handling)
- [Android background-work guidance](https://developer.android.com/develop/background-work/background-tasks)
- [Android accessibility service behavior](https://developer.android.com/guide/topics/ui/accessibility/service)
- [Google Play Accessibility API policy](https://support.google.com/googleplay/android-developer/answer/10964491)
- [Google Play user-data policy](https://support.google.com/googleplay/android-developer/answer/10144311)

