# NotesAI iOS Port Plan

Status: planned. No iOS implementation has started.

This plan is based on NotesAI 0.4.0. The iOS app should reuse the React interface, Rust domain logic, note/backup schema, topic review, retrieval rules, citations, and provider profiles where mobile dependencies allow. System integration belongs in Swift/Tauri plugins and Xcode targets.

## Product decision: how capture should work

iOS does not allow NotesAI to place a persistent floating button over arbitrary apps or continuously read another app's screen. Design around system-sanctioned, user-initiated surfaces:

1. **Share Extension — primary path.** From Safari, Photos, YouTube, Reddit, X, or another app, tap Share → NotesAI. The extension writes text, URL, image, or supported file metadata to a durable App Group inbox, confirms quickly, and exits. Users can add NotesAI to Share-sheet Favorites to keep it near the front.
2. **App Shortcut — closest hotkey equivalent.** Add “Save to NotesAI” and “New NotesAI capture” through App Intents. Users can invoke them from Shortcuts, Siri, Spotlight, the Home Screen, supported widgets/controls, an iPhone Action button, or an Accessibility Back Tap configured to run a shortcut.
3. **Clipboard capture — explicit only.** A shortcut or visible in-app button may paste copied text after a direct user action. iOS reports cross-app pasteboard access when it lacks recognized user intent, so NotesAI must not poll the clipboard or imply silent background capture.
4. **Widget/control — quick launch.** Provide New note, Paste and save, Scan text, and Open assistant. Keep note content privacy-sensitive on the Lock Screen and Always-On display.
5. **Screenshot/image share — fallback for unavailable text.** When a source app does not expose its text, the user can share a screenshot to NotesAI. OCR and image description belong to the planned image-ingestion pipeline; the app must clearly distinguish extracted text from model-generated description.

Do not attempt Picture-in-Picture tricks, keyboard-extension surveillance, screen recording, or other substitutes for a floating overlay. They would create poor privacy, reliability, and App Review outcomes. Share Extension plus a user-configured Action button or Back Tap is the recommended low-friction combination.

## Definition of done

The iOS port is complete when an App Store/TestFlight build can receive shared text, URLs, and images in a short-lived Share Extension; save them durably while offline; process them under iOS lifecycle limits; search and chat with clickable citations; organize topics; securely synchronize through Google Drive; recover from extension/app termination; and pass cross-device merge, deletion, security, accessibility, and upgrade tests.

## Known codebase blockers

| Area | Current desktop assumption | iOS work |
| --- | --- | --- |
| Layout | Fixed desktop sidebar/feed/detail layout and 920px minimum | Add compact navigation, full-screen routes, sheets, Dynamic Type, safe areas, and touch targets |
| Capture | Global shortcuts and a long-running tray process | Add a Swift Share Extension, App Intents, widgets/controls, and an App Group inbox |
| Background work | Persistent Rust polling worker | Use explicit queue wakes and permitted BackgroundTasks; expect scheduling to be discretionary |
| Clipboard | Background desktop reads | Read pasteboard only through visible user intent; no polling |
| Database sharing | One process owns the app database | Keep extension writes short and coordinated; prefer an append-only App Group inbox that the main app imports transactionally |
| Native ML/search | Desktop FastEmbed/ORT/sqlite-vec | Prove iOS device/simulator builds, App Store-compatible native libraries, memory, thermal, and package size |
| Video hydration | Runs bundled `yt-dlp.exe` | iOS cannot use that desktop executable; define a compliant metadata/transcript alternative |
| Google auth | Desktop loopback OAuth and Windows DPAPI | Use an iOS OAuth client/native redirect and Keychain access group shared only where required |
| Drive | Recovery snapshots rather than live synchronization | Add device change batches, cursors, tombstones, conflicts, attachment hashes, and compaction |
| Local LLM | Ollama/LM Studio assumed on localhost | Remote models are the practical baseline; optional Mac/PC LAN servers require explicit configuration and local-network permission |
| Tauri desktop plugins | Tray, single-instance, global shortcut, autostart | Compile only on desktop; implement mobile integrations through Swift/Tauri plugin code |

## Phase 0 — Apple setup and compile inventory

Estimated effort: one to two days, excluding Apple account approval.

1. Use the MacBook with current Xcode, full iOS SDK, CocoaPods as required by Tauri, Node.js, and Rust iOS device/simulator targets.
2. Join the Apple Developer Program before testing App Groups, associated capabilities, TestFlight, and production signing on devices.
3. Create a branch such as `codex/ios-port`, initialize Tauri's iOS project, and preserve generated-project changes deliberately.
4. Run all shared tests, then attempt simulator and physical-device builds without weakening core security settings.
5. Inventory unsupported Rust crates and Tauri plugins. Tauri mobile supports native Swift/Kotlin plugin bridges, but not every desktop plugin is meaningful or implemented on mobile.
6. Select minimum iOS only after checking Tauri/WebKit, App Intents/widget goals, BackgroundTasks, native ML, and App Store requirements.

Exit gate: a signed development shell runs on a physical iPhone and the complete dependency/plugin blocker list is recorded.

## Phase 1 — Mobile shell and local storage

Estimated effort: two to four days.

1. Add a responsive shell shared with Android: bottom navigation, full-screen Library/Note/Assistant views, modal filters, and a compact citation drawer.
2. Support Dynamic Type, VoiceOver order, safe areas, keyboard avoidance, reduced motion, light/dark appearance, and portrait/landscape behavior.
3. Put the main database in the app container. Create an App Group for the main app, Share Extension, and relevant widget/intent targets.
4. Use the App Group for a small append-only capture inbox and attachment staging area rather than letting the extension perform long hydration, embeddings, or LLM work.
5. Coordinate all shared-container access. Import each inbox record exactly once into the main database, then acknowledge/remove it only after a committed transaction.
6. Apply file protection appropriate for personal notes and define what remains accessible before first unlock after reboot.

Exit gate: an extension-created inbox record survives termination and imports once, without corrupting the main database.

## Phase 2 — Share Extension and quick actions

Estimated effort: three to six days.

1. Add a native Swift Share Extension with activation rules limited to supported text, URL, image, and later PDF types.
2. Read `NSExtensionItem`/item providers defensively, enforce size/time limits, copy attachments into the App Group before provider access expires, and treat all metadata as untrusted.
3. Present a fast confirmation UI: title preview, optional comment/topic, Save, Cancel. Save raw data first and finish the extension promptly.
4. Add App Intents for New note, Save provided text/URL/file, Paste and save, Scan text, and Open assistant.
5. Expose useful intents to Shortcuts, Siri, Spotlight, Home Screen, widgets/controls, and supported Action-button workflows.
6. Document optional Back Tap setup as a user configuration; do not claim the app can reserve Back Tap globally.
7. Add Undo/recent-capture recovery in the main app because a completed extension cannot rely on remaining alive.

Exit gate: Safari, Photos, Files, YouTube, Reddit, and at least two social apps can share supported content in a few seconds, including while the main app is not running.

## Phase 3 — Native search and embedding feasibility

Estimated effort: four to eight days. This is the largest iOS technical risk.

1. Build `rusqlite` with FTS5 and `sqlite-vec` for physical ARM64 and simulator targets. Test migrations, vector registration, query, delete, and restore.
2. Determine whether the pinned FastEmbed/ORT versions are compatible with iOS packaging and App Store rules. Do not infer iOS support from macOS success.
3. Run release-mode BGE inference on a physical iPhone. Measure model download/storage, app binary size, cold load, peak memory, indexing speed, battery, and thermal pressure.
4. If current ORT packaging fails, evaluate a supported ONNX Runtime mobile build, Core ML conversion, or another on-device adapter. Preserve embedding compatibility or trigger a controlled device-local re-index.
5. Keep source notes authoritative and indexes derived. Synchronize source changes and rebuild FTS/vector indexes per device.
6. If dense embeddings cannot meet the first beta gate, ship reliable capture/browse/edit/sync and FTS5 search first. Never silently replace local embeddings with a cloud call.

Exit gate: hybrid retrieval works acceptably on a supported physical iPhone, or the beta clearly declares lexical-only retrieval with a documented parity plan.

## Phase 4 — Hydration, images, providers, and citations

Estimated effort: three to six days.

1. Reuse safe portable HTTP/OpenGraph parsing with mobile limits. Queue network hydration after the raw capture is durable.
2. Treat YouTube transcript extraction as a separate capability. The desktop `yt-dlp` binary cannot be bundled/executed as-is on iOS; save URL/metadata until a compliant approach is proven.
3. Add image storage, thumbnails, Vision OCR, and optional vision-model description. Preserve provenance and label OCR versus generated summaries.
4. Support remote OpenAI-compatible providers as the baseline. Allow explicit LAN URLs for Ollama/LM Studio on a Mac/PC; request local-network access in context and explain that phone `localhost` is not the computer.
5. Preserve review-before-save topic organization, source-number validation, paragraph/list citation coverage, repair, and linked-excerpt fallback.
6. Ensure a Share Extension never runs remote AI automatically; user/provider disclosure belongs in the main app.

Exit gate: supported items hydrate predictably, private data transfers are visible, and every grounded answer opens its exact local source passage.

## Phase 5 — iOS lifecycle and background work

Estimated effort: two to five days.

iOS does not provide an always-running background process. Redesign the worker as an idempotent queue processed when the app is foregrounded and, opportunistically, through BackgroundTasks. The system decides when scheduled refresh/processing tasks run; they cannot guarantee immediate execution.

1. Wake processing on app launch/resume, capture import, sync import, and user-requested Sync/Index actions.
2. Use suitable BackgroundTasks for refresh and maintenance, with expiration handlers and transactional checkpoints.
3. For user-started long indexing that may continue after backgrounding, evaluate the current continued-processing API against the chosen minimum iOS version; provide progress and cancellation where the system requires it.
4. Use background URL sessions for permitted Drive transfers and set the App Group shared container when extension/main-app coordination requires it.
5. Test low-power mode, Background App Refresh disabled, force-quit, reboot, locked device, poor network, expiration, and OS-terminated work.

Exit gate: queued work is never considered complete before commit, safely retries after termination, and the UI never promises a background completion time iOS cannot guarantee.

## Phase 6 — Google Drive multi-device sync

Estimated effort: five to ten days. The current backup mechanism must be extended into a real sync protocol.

Use the same protocol planned for Android:

- Stable note IDs, device IDs, content revisions/hashes, `updated_at`, and deletion tombstones.
- Immutable gzip-compressed per-device change batches with monotonically increasing sequence numbers.
- Per-device local cursors so each client downloads only unseen changes.
- Transactional deterministic merge with explicit preserved conflicts for concurrent edits.
- Content-addressed attachment blobs so identical images upload once.
- Periodic compact snapshots and conservative cleanup that never destroys the last recovery point.
- Device-local rebuilding of chunks, FTS, and vectors after source changes merge.

Register a separate iOS OAuth client and use native authorization with PKCE/redirect handling. Store tokens in Keychain. If the Share Extension needs limited shared credentials, use a narrowly scoped Keychain access group; preferably let it write offline to the App Group and leave Drive access to the main app/background session.

The UI needs Sync now, last success, pending upload/download counts, offline state, device list, conflict review, and remote-delete behavior. “Backup connected” and “Sync active” must be separate states.

Exit gate: iPhone, Android, macOS, and Windows clients converge after offline edits without overwriting divergent content, and cloud storage growth is measured and bounded.

## Phase 7 — Privacy, security, and App Store readiness

Estimated effort: three to five days plus review.

1. Create privacy manifests and App Store privacy disclosures matching the code and every included SDK.
2. Publish a privacy policy covering local notes, Google Drive, chosen AI providers, OCR/images, diagnostics, deletion, and account handling.
3. Keep API keys and OAuth tokens in Keychain, redact logs, apply Data Protection, and ensure backups exclude secrets and derived indexes.
4. Validate incoming item providers, URLs, files, deep links, OAuth callbacks, HTML, image bombs, and prompt injection.
5. Use purpose strings only for capabilities actually used. Request Photos, camera, local-network, notification, or other access at the moment the corresponding feature is invoked.
6. Provide an App Review demo path that works without the reviewer installing Ollama. A remote test profile or local demo library can demonstrate retrieval without exposing real credentials.
7. Confirm third-party content hydration and provider integrations comply with source terms and App Review requirements.

Exit gate: privacy labels, manifests, permissions, app behavior, and review notes agree; a clean account can exercise the core app without private developer infrastructure.

## Phase 8 — TestFlight and App Store release

Estimated effort: three to six days plus Apple review.

1. Add app/extension identifiers, App Group, Keychain group, OAuth redirect configuration, signing, provisioning profiles, and required capabilities in Xcode.
2. Generate iPhone/iPad icons, launch assets, screenshots, descriptions, support/privacy URLs, and age/content declarations.
3. Test current and chosen minimum iOS versions on physical devices, including a smaller phone and current large phone; add iPad only when its responsive layout is intentionally supported.
4. Add Swift unit/UI tests for extension imports, App Intents, inbox recovery, item providers, OAuth, Keychain, sync merges, migrations, and citation navigation.
5. Distribute through internal TestFlight, then external beta. Exercise installation, upgrade, extension enablement, Share-sheet Favorites, revoked permissions, and data migration.
6. Add CI for shared tests, simulator compilation, archive validation, and signed TestFlight artifacts with credentials in encrypted secrets.

Exit gate: the TestFlight/App Store build captures from real host apps, survives termination/upgrades, syncs without data loss, and passes accessibility/privacy review.

## First iOS work session

1. Install the full Xcode/iOS/Tauri toolchain and record versions and test device.
2. Branch from current master and initialize Tauri iOS.
3. Make an unchanged shell run on a physical iPhone.
4. Inventory desktop-only plugins and native dependencies.
5. Create the App Group and a minimal Swift Share Extension.
6. Save one shared text item to an append-only App Group inbox and import it once into SQLite.
7. Perform the physical-device FTS/sqlite-vec/FastEmbed spike before promising RAG parity or dates.

## Acceptance checklist

- [ ] Shared desktop tests remain green.
- [ ] Development and release builds run on a physical supported iPhone.
- [ ] Share Extension accepts bounded text, URL, and image input while the main app is closed.
- [ ] Extension inbox imports exactly once after crashes or termination.
- [ ] App Intents work through Shortcuts and at least one quick hardware/system trigger.
- [ ] Clipboard reads follow explicit user intent and no polling occurs.
- [ ] Background expiration, low-power mode, disabled refresh, offline use, and force-quit do not lose notes.
- [ ] SQLite/FTS5/topics/citations work; dense embeddings are proven or clearly excluded.
- [ ] Remote and LAN model transfers are clearly disclosed.
- [ ] Drive sync handles offline creates, edits, deletes, conflicts, and deduplicated attachments.
- [ ] Keychain/App Group access is minimal and backups contain no secrets.
- [ ] Dynamic Type, VoiceOver, safe areas, reduced motion, and Lock Screen privacy are tested.
- [ ] TestFlight upgrade preserves notes and migrations.
- [ ] App Store privacy declarations and review notes match behavior.

## Authoritative references

- [Tauri mobile prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Tauri 2 mobile architecture](https://v2.tauri.app/blog/tauri-20/)
- [Tauri mobile plugin development](https://v2.tauri.app/develop/plugins/)
- [Apple Share Extension guide](https://developer.apple.com/library/archive/documentation/General/Conceptual/ExtensibilityPG/Share.html)
- [Apple App Groups](https://developer.apple.com/documentation/xcode/configuring-app-groups)
- [Apple App Intents](https://developer.apple.com/documentation/appintents)
- [Apple pasteboard privacy](https://developer.apple.com/documentation/uikit/uipasteboard)
- [Apple background tasks](https://developer.apple.com/documentation/backgroundtasks)
- [Apple widget privacy](https://developer.apple.com/documentation/widgetkit/creating-a-widget-extension)
- [Apple App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/)

