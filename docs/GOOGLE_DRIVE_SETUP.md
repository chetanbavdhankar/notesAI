# Google Drive backup

In NotesAI 0.3.0, open **Settings → Backup → One-time Google setup**.

1. Create/select a project in [Google Cloud Console](https://console.cloud.google.com/).
2. Enable the **Google Drive API** for that project.
3. Configure Google Auth Platform Branding and Audience. For a testing app, add your Google account as a test user.
4. Under Clients, create an OAuth client of type **Desktop app**.
5. Paste its client ID and client secret into NotesAI. Click **Sign in with Google** and complete consent in your browser.
6. Click **Back up now**. NotesAI creates a **NotesAI Backups** folder and shows the last successful upload.
7. Enable automatic backups and **Save settings** to apply the schedule. NotesAI must be running (including in the tray) for the schedule to run.

The app uses a loopback callback, PKCE S256, OAuth state validation and the limited `drive.file` scope. Google credentials are encrypted using Windows DPAPI for the current Windows account. They are never included in exported backups. Setup follows [Google's installed-app OAuth flow](https://developers.google.com/identity/protocols/oauth2/native-app) and [Drive scope guidance](https://developers.google.com/workspace/drive/api/guides/api-specific-auth).

If Google rejects consent, check the Desktop client type, enabled Drive API, and test-user list. Testing-mode tokens may expire and require reconnection. A public release should provide the publisher's configured OAuth application so ordinary users only sign in; this build supports a personal one-time client setup.

## Storage and restore

Local SQLite remains authoritative. Gzip snapshots contain note IDs, text, tags, URLs, and capture dates. They exclude model downloads, embeddings, search indexes, provider configuration and API keys. Uploads occur only when the note content changes. Two alternating snapshots are retained per device; Google may temporarily retain previous binary file revisions. Two local snapshots are also retained after a backup/export.

Cloud snapshot content is not end-to-end encrypted by NotesAI. It has Google Drive's normal storage protections. Disconnect deletes credentials on this computer without deleting backups; revoke the application's grant in Google account settings if desired.

Use **Export backup** for an additional file copy. On another PC, download a `.json.gz` snapshot from Drive and choose **Restore backup file**. Identical IDs/content are skipped; conflicting local notes remain unchanged and the imported version becomes a separate note. Indexes rebuild locally. Repeated imports of a conflicting archive may create additional copies. This release provides backup/restore, not automatic cross-device synchronization. Google Drive is the first direct cloud integration; other providers are not implemented.

Archives are limited to 32 MB compressed and 64 MB expanded. A failed cloud upload retains the local copy and displays an error. Existing folder links work only for folders accessible to this OAuth app; a shared link alone cannot authorize uploads.

## Validation boundary

Local compression/restore, conflict preservation, PKCE, callback state and Windows encrypted credential round-trips have automated tests. Live Google consent and uploads require a user-configured OAuth client and have not been verified against a real account in this build.
