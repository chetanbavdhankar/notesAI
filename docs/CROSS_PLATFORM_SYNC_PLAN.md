# NotesAI Cross-Platform Shared Library Plan

Status: planned. NotesAI currently supports compact Google Drive backup and manual restore. Continuous multi-device synchronization has not been implemented.

This plan defines how Windows, macOS, Android, and iOS should share one NotesAI library while keeping every device usable offline. It also establishes data ownership as a product requirement: the signed-in user owns the cloud copy, NotesAI has read and write access only after consent, and the user can add, update, export, or remove synchronized data.

## Recommended architecture

Every device keeps its own local SQLite database. The local database serves the UI, ingestion queue, search, chat retrieval, and offline work. A synchronization engine exchanges small, encrypted change batches through a user-owned Google Drive folder.

Do not place the live SQLite database, its WAL file, FTS5 tables, or sqlite-vec tables in a synchronized folder. SQLite expects local filesystem locking and atomicity. Cloud file synchronization can copy a database and WAL at different moments, overwrite concurrent work, or expose an incomplete transaction.

```text
Windows local SQLite  --\
macOS local SQLite   ----> local outbox -> encrypted change files -> Google Drive
Android local SQLite ----> Google Drive -> verified inbox -> merge -> local SQLite
iOS local SQLite     --/                         |
                                                +-> encrypted snapshots and blobs
```

The cloud is a synchronization transport and recovery location. It is not queried for normal note display or retrieval. A disconnected device continues to capture, search, organize, and edit notes, then synchronizes when connectivity returns.

## Product decisions

1. **Local-first operation:** a successful local transaction completes the user's action. Cloud availability must not block capture or editing.
2. **Read and write synchronization:** creation, edits, topic changes, attachment changes, and deletions move in both directions between every authorized device.
3. **User ownership:** users can inspect synchronization status, export their library, delete individual notes through NotesAI, remove cloud attachments, remove a device, erase the cloud synchronization set, disconnect Google Drive, or keep the local library after disconnecting.
4. **Visible Drive location:** use a user-visible `NotesAI` folder as the default so the owner can see, copy, download, or delete it in Google Drive. Keep using the limited `drive.file` scope where feasible so NotesAI can access only files it creates or the user explicitly opens with it.
5. **Managed file format:** users edit note content through NotesAI. Directly editing encrypted operation files in Drive is unsupported because it bypasses validation and can corrupt synchronization history. Direct deletion is allowed, but the app must explain the result and recover safely.
6. **Separate backup and sync:** synchronized operations maintain a current library. Periodic snapshots provide disaster recovery. Restoring a snapshot is an explicit operation and must not silently replace a newer library.
7. **Derived data stays local:** FTS5 rows, vector embeddings, hydrated chunks, caches, downloaded embedding models, API keys, OAuth tokens, and device-specific preferences are not synchronized.
8. **Provider-independent core:** Google Drive is the first transport. The merge protocol must not depend on Drive-specific IDs so OneDrive, Dropbox, WebDAV, S3-compatible storage, or a dedicated service can be added later.

## What is synchronized

| Data | Synchronize | Notes |
| --- | --- | --- |
| Note ID, title, body, source URL, timestamps | Yes | Canonical user data |
| User topics, tags, flags, archive state | Yes | Supports consistent organization and retrieval |
| Deletions | Yes | Represented by tombstones until all active devices acknowledge them |
| Images and file attachments | Yes | Content-addressed, encrypted, deduplicated, and size-limited |
| Hydrated source text | Yes | When it is part of the saved note; preserve provenance |
| Saved chat conversations | Optional | User-controlled library setting; off by default initially |
| Theme and harmless UI preferences | Optional | Account preference; allow device override |
| LLM profile names/base URLs/model choices | Optional | Exclude secrets and allow device-specific endpoints |
| LLM API keys and Google OAuth tokens | No | Store in each operating system's secure credential store |
| FTS5 and sqlite-vec indexes | No | Rebuild from canonical notes on each device |
| Embeddings and embedding models | No initially | Large, derived, and potentially platform/model dependent |
| Temporary ingestion files and logs | No | Device-specific operational data |

## Cloud folder layout

Use immutable, bounded files wherever possible so two devices never update the same operation file.

```text
NotesAI/
  library.json
  devices/
    <device-id>.json
  changes/
    <device-id>/
      0000000000000001-0000000000000100.nsaic
      0000000000000101-0000000000000200.nsaic
  blobs/
    ab/cd/<sha256>.nsaib
  snapshots/
    <snapshot-id>.nsais
  acknowledgements/
    <device-id>.json
```

- `library.json` contains only format version, library ID, encryption parameters, feature flags, and current snapshot pointer. Updates use Drive revision preconditions so stale clients cannot overwrite a newer manifest.
- Each device writes only inside its own `changes/<device-id>` stream. Files contain a sequence range and are never modified after successful upload.
- Attachment names derive from the plaintext content hash, but the stored object is encrypted. Identical attachments upload once.
- Device and acknowledgement files are small mutable coordination records. A failed conditional update must be fetched and retried, never overwritten blindly.
- Snapshots are immutable encrypted compactions of canonical records. Retain a small documented history rather than unlimited revisions.

The existing alternating backup snapshots remain useful during migration, but they are not a synchronization protocol and should keep their current restore semantics until the new engine is proven.

## Identity and ordering

### Library and record identifiers

- Generate one random `library_id` when synchronization is enabled.
- Give each device a random `device_id`, a display name, platform, creation time, and public enrollment key.
- Keep current note IDs if they are globally unique. New records should use UUIDv7 or another collision-resistant ID generated without a server.
- Assign every local operation a strictly increasing per-device `sequence` inside the same SQLite transaction as the note change.

Wall-clock timestamps are useful to users but are insufficient for conflict detection because device clocks drift. Use a hybrid logical clock or a per-record version vector. A practical first implementation can store a compact map of the highest sequence observed from each device on that record.

### Operation envelope

```json
{
  "format": "notesai.change",
  "version": 1,
  "library_id": "...",
  "device_id": "...",
  "sequence": 42,
  "operation_id": "device-id:42",
  "entity": "note",
  "entity_id": "...",
  "kind": "upsert",
  "parents": { "device-a": 11, "device-b": 29 },
  "payload_hash": "sha256:...",
  "created_at": "RFC3339 timestamp"
}
```

The authenticated encrypted payload contains the canonical record or patch. Start with complete note records because they are easier to validate and recover than arbitrary JSON patches. Batch many small operations into one compressed file to avoid excessive Drive file counts.

## Local schema additions

The first implementation should add tables similar to the following through normal database migrations:

| Table | Purpose |
| --- | --- |
| `sync_library` | Library ID, format version, encryption state, remote folder ID |
| `sync_devices` | Known devices, status, last activity, retirement time |
| `sync_outbox` | Durable local operations waiting to be batched and uploaded |
| `sync_applied` | Operation IDs already merged; enforces idempotency |
| `sync_cursors` | Drive page token and per-device highest downloaded sequence |
| `sync_conflicts` | Both versions and resolution state |
| `sync_tombstones` | Deleted entity IDs and causal versions |
| `sync_blob_refs` | Content hash, local path, remote ID, reference count, upload state |

Creating or editing a note and inserting its outbox record must happen in one SQLite transaction. Applying incoming operations, recording their IDs, and scheduling index rebuilds must also happen in one transaction. A crash can then retry without losing or duplicating user changes.

## Bidirectional synchronization cycle

Run synchronization on explicit **Sync now**, app startup, app resume, after meaningful local changes, and permitted background opportunities. Mobile operating systems do not guarantee a continuously running process, so correctness must not depend on real-time polling.

1. Acquire an in-process synchronization lock. Only one cycle runs per local database.
2. Refresh the OAuth access token from the platform credential store.
3. Read local outbox rows and package a bounded batch.
4. Compress, encrypt, and upload it under this device's next sequence range.
5. Confirm the remote file before marking outbox rows uploaded.
6. Request Drive changes since the saved page token, or list the NotesAI folder during bootstrap/recovery.
7. Download unknown manifests, change batches, and required blobs to a staging directory.
8. Verify library ID, format version, size limits, file hashes, authenticated encryption, sequence continuity, and schema before touching the database.
9. Apply operations idempotently in deterministic order and preserve concurrent conflicts.
10. Rebuild or queue FTS/vector indexing only for affected notes.
11. Save cursors only after the database commit succeeds.
12. Upload the device acknowledgement and expose the result in Settings.

Google Drive's changes feed can efficiently report modifications after a stored start page token. A full listing is still required for first enrollment, an expired cursor, repair, and audit. See [Retrieve changes](https://developers.google.com/workspace/drive/api/guides/manage-changes).

## Updates, conflicts, and deletion

### Normal updates

If an incoming record descends from the local version, replace the canonical record and retain normal local revision history. If the local version descends from the incoming record, the incoming operation is already superseded and only its applied marker is needed.

### Concurrent edits

If neither version causally contains the other, preserve both. Do not use timestamp-only last-write-wins for note bodies.

- Merge independent metadata fields automatically when safe, such as one device adding a tag while another changes a color.
- For concurrent body/title edits, keep the local note visible, create a conflict record containing both complete versions, and display **Keep this version**, **Keep other version**, or **Merge manually**.
- Resolving a conflict creates a new operation descended from both versions so every device reaches the same result.
- Never discard a version merely because it arrived later.

A text CRDT can be reconsidered if live collaborative editing becomes a product requirement. It adds storage and migration complexity that ordinary asynchronous personal note editing does not currently justify.

### User-controlled deletion

Deleting a synchronized note creates a tombstone operation and removes it from normal local views. Other devices apply that deletion. Provide a configurable recovery period, initially 30 days, during which the user can restore it.

After every active device acknowledges a tombstone and the recovery period expires, compaction may remove the old note content and unreferenced attachments. A retired or lost device must not prevent cleanup forever; allow the user to remove that device from the library.

Settings should expose these actions:

- Delete a note everywhere
- Restore a recently deleted note
- Remove selected cloud attachments
- Remove a device and invalidate its future writes until it enrolls again
- Delete synchronized cloud data while preserving this device's local library
- Delete synchronized cloud data and the local library, with an explicit destructive confirmation
- Disconnect Google Drive without deleting either copy
- Export a portable decrypted archive chosen by the user

If the owner manually deletes the entire `NotesAI` Drive folder, a device must not silently recreate and repopulate it. Pause synchronization, explain that the remote library was removed, and offer **Keep local and create a new cloud library**, **Reconnect to another folder**, or **Keep local only**.

If the owner deletes an individual protocol file directly, report a damaged/incomplete remote history. Recover from a valid snapshot plus later complete batches where possible. Do not infer that deleting an internal batch means deleting all notes once contained in it.

## Google Drive access model

The current desktop integration requests `https://www.googleapis.com/auth/drive.file`. This scope allows NotesAI to create, read, update, and delete files it created or the user explicitly opened with the app. That is preferable to requesting access to every file in the account. Google's current scope descriptions are documented in [OAuth 2.0 scopes for Google APIs](https://developers.google.com/identity/protocols/oauth2/scopes).

The recommended visible-folder mode is:

1. NotesAI creates a `NotesAI` folder and protocol files after consent, or the user selects an existing folder through a supported picker/explicit link flow.
2. NotesAI stores the folder ID locally and validates ownership/access before every destructive cloud action.
3. NotesAI performs create, download, conditional update, and delete operations only inside that folder.
4. The owner can see and remove the folder in the Drive interface. NotesAI provides safer record-level controls inside the app.

Google also provides an `appDataFolder` with the narrower `drive.appdata` scope. It is hidden from the normal Drive UI, accessible only by the creating app, and can still be manually removed as application data. It is useful for internal cursors or configuration, but it should not be the only library location because this product requires visible user control. See [Store application-specific data](https://developers.google.com/workspace/drive/api/guides/appdata).

All Windows, macOS, Android, and iOS OAuth clients should belong to the same configured Google project and production consent setup. Each platform uses its native OAuth client type and secure token store. A folder link alone never authorizes access.

## Encryption and device enrollment

Drive access control and Google's storage encryption are not end-to-end encryption controlled by NotesAI. Personal notes should be encrypted before upload.

- Generate a random library master key on the first device.
- Use a maintained, audited cryptographic library and an authenticated encryption mode such as XChaCha20-Poly1305 or AES-256-GCM.
- Use a unique nonce per object and authenticate the library ID, object type, format version, and sequence metadata.
- Keep OAuth tokens and unlocked library keys in Windows Credential Manager/DPAPI, macOS/iOS Keychain, or Android Keystore-backed storage.
- Never place plaintext API keys, OAuth tokens, or recovery secrets in synchronized files or logs.
- Enroll another device using an authenticated QR/device-pairing flow or a high-entropy recovery key. Google sign-in alone proves Drive access; it should not silently replace possession of the NotesAI encryption key.
- Support key rotation as a staged migration. Do not rewrite the entire cloud library inside one fragile transaction.

The visible folder will contain opaque encrypted files. Data ownership is provided through full in-app read/write/delete/export controls and the owner's ability to remove the folder. Human-readable direct editing in Drive would conflict with end-to-end encryption. If human-editable Markdown export is wanted later, provide it as a separate one-way export format rather than synchronization storage.

## Attachments and storage limits

Images will dominate storage, not note text. Treat attachment storage separately:

- Hash plaintext with SHA-256 and address blobs by that hash.
- Deduplicate identical images and screenshots.
- Preserve the original only when the user enables it; otherwise allow an optimized image plus OCR/description output.
- Generate thumbnails locally and avoid synchronizing derived thumbnails unless measurement proves it saves overall bandwidth.
- Upload resumably for large blobs and retry idempotently.
- Enforce per-file and per-sync-cycle limits and display pending size before uploading on metered/mobile connections.
- Garbage-collect a blob only when no live note references it, its recovery window expired, and active devices acknowledged the removal.
- Show local library size, cloud size attributable to NotesAI, pending upload/download sizes, and the largest attachments.

Periodic snapshots should contain canonical structured data and attachment references, not duplicate all attachment bytes. Keep a bounded number of snapshots and let users adjust retention.

## Platform behavior

### Windows and macOS

- Synchronize while the tray/menu-bar process is running.
- Trigger promptly after local edits with debouncing, and periodically while online.
- Store secrets in DPAPI/Credential Manager or Keychain.
- Never require the main window to remain visible.

### Android

- Save shared content locally before dismissing the capture UI.
- Use WorkManager for durable deferred synchronization and an explicit foreground action for long transfers.
- Respect battery, network, and metered-data constraints.
- Store credentials and wrapped keys with Android Keystore-backed facilities.

### iOS

- Let the Share Extension write only to a small App Group inbox; the main app imports and synchronizes later.
- Use permitted background tasks opportunistically. Do not promise immediate synchronization after the app is force-quit.
- Store credentials and keys in Keychain with the minimum required access group.

## Settings and user experience

Add a dedicated **Sync** section separate from **Backup**:

- Google account and selected Drive folder
- Sync enabled/paused state
- Last successful upload and download
- Pending operations and bytes
- Current device name and known devices
- Wi-Fi-only attachment option
- Saved-chat and preference synchronization toggles
- Conflict and recently deleted counts
- Sync now, repair, export, disconnect, remove device, and delete cloud data actions

Every status should say whether local saving succeeded independently of cloud synchronization. Errors must identify authentication, quota, network, corrupt remote data, unsupported format, or conflict rather than displaying a generic failure.

The first device creates the shared library. A new device should offer:

1. Sign in to Google.
2. Find or select the NotesAI folder.
3. Verify the expected library ID and account.
4. Obtain the encryption key by pairing or recovery key.
5. Download the latest valid snapshot.
6. Replay later operation batches.
7. Build local search and vector indexes.
8. Show completion and any conflicts before enabling normal background sync.

## Migration from current backup support

The repository currently creates gzip snapshot backups with schema version 2, maintains two local/cloud slots, uses `drive.file`, and restores without importing indexes or secrets. Migration should preserve that tested recovery path.

1. Add the synchronization schema and engine behind a disabled feature flag.
2. Keep the current Backup UI and scheduled snapshots unchanged.
3. When the user enables Sync, create a new library folder/manifest and a fresh encrypted snapshot from local canonical records.
4. Mark the current device as the initial authority only for this one bootstrap operation.
5. Enroll a second desktop device and prove full reconstruction before enabling sync on mobile.
6. Keep legacy backups restorable; never reinterpret a backup file as a change batch.
7. After a stable release, allow scheduled snapshots to target the new library's `snapshots` directory while retaining export compatibility.

## Implementation phases and exit gates

### Phase 0 — Protocol specification and threat model

Estimated effort: two to four days.

- Define canonical schemas, size limits, version-vector/HLC behavior, encryption envelope, folder layout, and error taxonomy.
- Document account compromise, stolen device, malicious/corrupt Drive files, rollback, replay, lost recovery key, and accidental deletion cases.
- Create fixed test vectors that Rust, Kotlin, and Swift implementations must read identically.

Exit gate: reviewed version 1 protocol and portable fixtures exist before database or Drive implementation starts.

### Phase 1 — Local sync journal

Estimated effort: three to five days.

- Add migrations, transactional outbox, applied-operation table, tombstones, conflict storage, and status APIs.
- Build a filesystem transport used only in tests.
- Verify crash recovery, duplicate delivery, reordered batches, concurrent edits, delete/edit conflicts, and index rebuilding.

Exit gate: two isolated local databases converge through a directory transport without data loss.

### Phase 2 — Google Drive read/write transport

Estimated effort: four to seven days, excluding OAuth verification.

- Reuse the authenticated Drive client but add list/download/delete, change cursors, conditional manifest updates, bounded uploads, and repair listing.
- Create the visible NotesAI folder and request only necessary OAuth scopes.
- Add safe cloud-delete and disconnect flows.
- Test quota, revoked access, expired tokens, interrupted uploads, missing files, duplicate files, and folder deletion.

Exit gate: Windows and macOS can add, update, and delete notes in either direction through one Drive library.

### Phase 3 — Encryption and enrollment

Estimated effort: four to seven days.

- Encrypt batches, snapshots, manifests containing private fields, and blobs.
- Implement secure key storage, recovery-key import/export, and device pairing.
- Add device removal and key-rotation groundwork.

Exit gate: Drive never receives plaintext note content and a fresh device reconstructs the library only with both Drive authorization and the NotesAI key.

### Phase 4 — Compaction and attachments

Estimated effort: four to eight days.

- Produce deterministic snapshots, replay from snapshots, collect acknowledgements, prune eligible history, and deduplicate blobs.
- Implement attachment transfer policies and storage reporting.

Exit gate: a large history bootstraps in bounded time and storage stabilizes after compaction without breaking a temporarily offline device.

### Phase 5 — Android and iOS integration

Estimated effort: one to three weeks after each mobile shell is viable.

- Bind the shared Rust sync engine where feasible and implement platform OAuth, secure storage, lifecycle scheduling, and network policy.
- Exercise airplane mode, process death, background restrictions, revoked permission, low storage, and cross-platform conflicts on physical devices.

Exit gate: all four platforms pass the same convergence fixtures and a user can safely add, edit, and delete notes from any device.

### Phase 6 — Operational hardening

Estimated effort: three to seven days plus beta observation.

- Add privacy-preserving diagnostics, manual repair, protocol-version compatibility, roll-forward migrations, and staged rollout controls.
- Run multi-day soak tests with offline devices, clock skew, repeated retries, thousands of notes, large images, and concurrent changes.
- Document recovery and export independently of the app.

Exit gate: beta users can diagnose or recover common failures without editing protocol files.

## Required validation scenarios

The implementation is not complete until automated or repeatable integration tests cover:

- Create on each platform and receive on every other platform
- Edit offline on one device and online on another
- Concurrent title/body edits with no silent loss
- Topic/tag changes combined with note edits
- Delete, restore, retention expiry, and attachment garbage collection
- Duplicate, reordered, partially downloaded, corrupt, oversized, and malicious files
- Crash before upload confirmation and crash during local merge
- OAuth expiry/revocation, Drive quota exhaustion, and folder access removal
- User manually deletes the Drive folder or an internal file
- New-device bootstrap from snapshot plus later changes
- Device retirement after a long offline period
- Old client encountering a newer unsupported protocol version
- Export and restore without OAuth or access to the original device
- Local search and citations after imported changes rebuild their indexes

Convergence means every active device has the same canonical records, tombstones, and conflict decisions after all valid operations are delivered. Matching timestamps alone is not evidence of convergence.

## Practical limitations

- Google Drive is a file API, not a transactional synchronization database. Sync will normally be prompt while apps are active, but mobile background scheduling can delay it.
- Direct Drive file manipulation cannot safely map every deleted internal batch to a user-level note deletion. Record-level changes should be made through NotesAI.
- End-to-end encryption prevents human-readable note editing inside the Drive web interface. A separate Markdown export can satisfy portability without weakening synchronized storage.
- A user who deletes all cloud copies and has no enrolled device or export cannot recover the library.
- A dedicated synchronization service may eventually provide faster notifications, simpler account recovery, and better large-scale operations. The protocol/transport boundary keeps that migration possible without abandoning local SQLite.

## Starting point when work resumes

Begin with Phase 0, then implement the local journal and filesystem convergence tests before changing Google Drive behavior. The first production milestone should be Windows-to-macOS text-note synchronization. Add encrypted attachments and mobile scheduling only after add, update, conflict, delete, recovery, and compaction behavior is proven for two desktop databases.

