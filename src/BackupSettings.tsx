import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { api, desktop } from "./api";
import type { BackupConfig, BackupStatus } from "./types";

export function BackupSettings({
  config,
  onChange,
}: {
  config: BackupConfig;
  onChange: (config: BackupConfig) => void;
}) {
  const [status, setStatus] = useState<BackupStatus>();
  const [secret, setSecret] = useState("");
  const [busy, setBusy] = useState("");
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  useEffect(() => {
    if (desktop)
      void invoke<BackupStatus>("backup_status")
        .then(setStatus)
        .catch((e) => setError(String(e)));
  }, []);
  const run = async (
    name: string,
    command: string,
    args?: Record<string, unknown>,
  ) => {
    setBusy(name);
    setError("");
    setMessage("");
    try {
      const result = await invoke<
        | BackupStatus
        | { imported: number; skipped: number; conflicts: number }
        | null
      >(command, args);
      if (result && "connected" in result) {
        setStatus(result);
        setMessage(name === "connect" ? "Google Drive connected." : "Done.");
      }
      if (result && "imported" in result)
        setMessage(
          `Restored ${result.imported} notes; ${result.skipped} already present. Existing notes were kept. Search indexes will rebuild locally.`,
        );
      if (name === "connect") setSecret("");
    } catch (e) {
      setError(String(e));
      if (desktop)
        void invoke<BackupStatus>("backup_status")
          .then(setStatus)
          .catch(() => {});
    } finally {
      setBusy("");
    }
  };
  return (
    <section className="backup-settings">
      <h3>Backup & restore</h3>
      <p>
        Your library stays local. Keep a compact copy in Google Drive, or export
        a backup file. This is backup and restore; live device sync is not yet
        included.
      </p>
      <details open={!status?.connected}>
        <summary>One-time Google setup</summary>
        <ol>
          <li>
            Open Google Cloud Console, create or select a project, and enable
            the Google Drive API.
          </li>
          <li>
            In Google Auth Platform, configure Branding and Audience. If the app
            is in testing, add your Google account as a test user.
          </li>
          <li>
            Under Clients, create an OAuth client with application type{" "}
            <strong>Desktop app</strong>. Copy its client ID and client secret
            below.
          </li>
        </ol>
        <button
          className="secondary"
          onClick={() =>
            void api.open(
              "https://console.cloud.google.com/apis/library/drive.googleapis.com",
            )
          }
        >
          Open Google Cloud Console
        </button>
        <label>
          Google client ID
          <input
            value={config.client_id}
            placeholder="…apps.googleusercontent.com"
            onChange={(e) =>
              onChange({ ...config, client_id: e.target.value.trim() })
            }
          />
        </label>
        <label>
          Desktop client secret
          <input
            type="password"
            autoComplete="off"
            value={secret}
            onChange={(e) => setSecret(e.target.value)}
            placeholder="Enter for sign-in only"
          />
        </label>
        <p className="settings-note">
          Credentials are encrypted for your Windows account. Google grants
          access only to files created or opened with NotesAI. Testing-mode
          authorization may need periodic reconnection.
        </p>
      </details>
      <div className="backup-actions">
        <button
          className="primary"
          disabled={!desktop || !!busy || !config.client_id || !secret}
          onClick={() =>
            void run("connect", "google_connect", {
              clientId: config.client_id,
              clientSecret: secret,
            })
          }
        >
          {status?.connected ? "Reconnect Google Drive" : "Sign in with Google"}
        </button>
        {busy === "connect" && (
          <button
            className="secondary"
            onClick={() => void invoke("google_cancel_connect")}
          >
            Cancel sign-in
          </button>
        )}
        {status?.connected && (
          <button
            className="secondary"
            disabled={!!busy}
            onClick={() => void run("disconnect", "google_disconnect")}
          >
            Disconnect
          </button>
        )}
      </div>
      <p className="connection-status">
        {status?.connected
          ? `Connected${status.account ? ` as ${status.account}` : " to Google Drive"}`
          : "Google Drive is not connected."}
      </p>
      <label className="backup-toggle">
        <input
          type="checkbox"
          checked={config.automatic}
          onChange={(e) => onChange({ ...config, automatic: e.target.checked })}
        />{" "}
        Automatically back up changed notes
      </label>
      <label>
        Check every (minutes)
        <input
          type="number"
          min={5}
          max={1440}
          value={config.interval_minutes}
          onChange={(e) =>
            onChange({ ...config, interval_minutes: Number(e.target.value) })
          }
        />
      </label>
      <p className="settings-note">
        Save settings to apply the schedule. Two compressed snapshots per device
        contain note text, tags, URLs and dates. Models, embeddings, and API
        keys are excluded. Google may temporarily retain older file revisions.
        Cloud copies use Google Drive storage protection; they are not
        end-to-end encrypted by NotesAI.
      </p>
      <details>
        <summary>Backup folder</summary>
        <p>
          A NotesAI Backups folder is created automatically. To reuse a folder
          created by this app, paste its Drive link here. A shared link alone
          does not grant upload access.
        </p>
        <label>
          Existing NotesAI folder link
          <input
            value={config.folder_link}
            onChange={(e) =>
              onChange({ ...config, folder_link: e.target.value })
            }
          />
        </label>
      </details>
      <div className="backup-actions">
        <button
          className="primary"
          disabled={!desktop || !!busy || !status?.connected}
          onClick={() => void run("backup", "backup_now", { config })}
        >
          Back up now
        </button>
        <button
          className="secondary"
          disabled={!desktop || !!busy}
          onClick={() => void run("export", "backup_export")}
        >
          Export backup
        </button>
        <button
          className="secondary"
          disabled={!desktop || !!busy}
          onClick={() => void run("restore", "backup_restore")}
        >
          Restore backup file
        </button>
        {status?.folder_id && (
          <button
            className="secondary"
            onClick={() =>
              void api.open(
                `https://drive.google.com/drive/folders/${status.folder_id}`,
              )
            }
          >
            Open Drive folder
          </button>
        )}
      </div>
      <p className="settings-note">
        Restore keeps existing notes and imports conflicting versions as
        separate copies. Download a .json.gz snapshot from Drive to restore on
        another PC. Disconnect removes credentials from this PC; revoke access
        in your Google account to remove the grant.
      </p>
      {status?.last_local_at && (
        <p className="backup-status">
          Local backup: {new Date(status.last_local_at).toLocaleString()} ·{" "}
          {status.note_count} notes ·{" "}
          {(status.compressed_bytes / 1024).toFixed(1)} KB
          <br />
          Drive backup:{" "}
          {status.last_upload_at
            ? new Date(status.last_upload_at).toLocaleString()
            : "Not uploaded yet"}
        </p>
      )}
      {busy && (
        <p role="status">
          {busy === "connect" ? "Finish sign-in in your browser…" : "Working…"}
        </p>
      )}
      {message && <p role="status">{message}</p>}
      {(error || status?.error) && (
        <p className="error" role="alert">
          {error || status?.error}
        </p>
      )}
    </section>
  );
}
