import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { api, desktop } from "./api";
import { Modal, errorText } from "./components";
import type { Note } from "./types";

type Proposal = {
  note_id: string;
  revision: number;
  topics: string[];
  reason: string;
};
export function OrganizeDialog({
  notes,
  onClose,
  onChanged,
}: {
  notes: Note[];
  onClose: () => void;
  onChanged: () => void;
}) {
  const [items] = useState(notes);
  const [proposals, setProposals] = useState<Record<string, Proposal>>({});
  const [choices, setChoices] = useState<Record<string, string>>(() =>
    Object.fromEntries(notes.map((n) => [n.id, (n.topics ?? []).join(", ")])),
  );
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [saved, setSaved] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [saving, setSaving] = useState("");
  const [progress, setProgress] = useState("");
  const stop = useRef(false);
  useEffect(
    () => () => {
      stop.current = true;
    },
    [],
  );
  const suggest = async () => {
    stop.current = false;
    setBusy(true);
    for (const [index, note] of items.entries()) {
      if (stop.current) break;
      if (saved.includes(note.id)) continue;
      setProgress(`Reviewing ${index + 1} of ${items.length}: ${note.title}`);
      try {
        const result = await invoke<Proposal>("suggest_topics", {
          id: note.id,
        });
        if (stop.current) break;
        setProposals((p) => ({ ...p, [note.id]: result }));
        setChoices((p) => ({ ...p, [note.id]: result.topics[0] ?? "" }));
        setErrors((p) => ({ ...p, [note.id]: "" }));
      } catch (e) {
        if (!stop.current)
          setErrors((p) => ({ ...p, [note.id]: errorText(e) }));
      }
    }
    setBusy(false);
    setProgress("");
  };
  const save = async (note: Note) => {
    setSaving(note.id);
    try {
      await api.applyTopics(
        { ...note, revision: proposals[note.id]?.revision ?? note.revision },
        (choices[note.id] ?? "")
          .split(",")
          .map((t) => t.trim())
          .filter(Boolean),
      );
      setSaved((s) => [...s, note.id]);
      onChanged();
    } catch (e) {
      setErrors((p) => ({ ...p, [note.id]: errorText(e) }));
    } finally {
      setSaving("");
    }
  };
  return (
    <Modal title="Organize by topic" onClose={onClose} wide>
      <p className="modal-subtitle">
        Review suggestions before saving. Choose several topics when useful, or
        enter your own separated by commas. Leaving a note unsaved keeps it in
        Unorganized.
      </p>
      <p className="settings-note">
        The organizer uses your Settings → Organization model. Remote models
        receive note excerpts. Existing topic names are reused where possible.
      </p>
      <button
        className="primary"
        disabled={!desktop || busy || items.length === saved.length}
        onClick={() => void suggest()}
      >
        Suggest topics for remaining notes
      </button>
      {busy && (
        <button
          className="secondary"
          onClick={() => {
            stop.current = true;
          }}
        >
          Stop after current note
        </button>
      )}
      <p role="status">
        {progress || `${saved.length} of ${items.length} reviewed`}
      </p>
      <div className="organization-list">
        {items.map((note) => (
          <article className="organization-card" key={note.id}>
            <h3>{note.title}</h3>
            {saved.includes(note.id) ? (
              <p role="status">Topics saved ✓</p>
            ) : (
              <>
                <p className="muted">{note.body.slice(0, 180)}</p>
                {proposals[note.id] && (
                  <>
                    <p>{proposals[note.id].reason}</p>
                    <div className="topic-options">
                      {proposals[note.id].topics.map((topic) => {
                        const selected = (choices[note.id] ?? "")
                          .split(",")
                          .map((t) => t.trim());
                        return (
                          <label key={topic}>
                            <input
                              type="checkbox"
                              checked={selected.includes(topic)}
                              onChange={(e) =>
                                setChoices((c) => ({
                                  ...c,
                                  [note.id]: (e.target.checked
                                    ? [...selected.filter(Boolean), topic]
                                    : selected.filter((t) => t !== topic)
                                  ).join(", "),
                                }))
                              }
                            />
                            {topic}
                          </label>
                        );
                      })}
                    </div>
                  </>
                )}
                <label>
                  Topics for {note.title}
                  <input
                    value={choices[note.id] ?? ""}
                    disabled={busy}
                    onChange={(e) =>
                      setChoices((c) => ({ ...c, [note.id]: e.target.value }))
                    }
                    placeholder="e.g. Machine learning, Research"
                  />
                </label>
                <button
                  className="secondary"
                  disabled={busy || !!saving}
                  onClick={() => void save(note)}
                >
                  {saving === note.id
                    ? "Saving…"
                    : choices[note.id]?.trim()
                      ? "Save topics"
                      : "Mark reviewed without a topic"}
                </button>
                {errors[note.id] && (
                  <p className="error" role="alert">
                    {errors[note.id]}
                  </p>
                )}
              </>
            )}
          </article>
        ))}
      </div>
      {!items.length && (
        <p>
          All notes have been reviewed. New or edited notes will appear here
          once hydrated.
        </p>
      )}
    </Modal>
  );
}

export function BackgroundSettings() {
  const [enabled, setEnabled] = useState(false),
    [busy, setBusy] = useState(true),
    [error, setError] = useState("");
  useEffect(() => {
    void api
      .startup()
      .then(setEnabled)
      .catch((e) => setError(errorText(e)))
      .finally(() => setBusy(false));
  }, []);
  return (
    <section className="background-settings">
      <h3>Background capture</h3>
      <p>
        Closing the window keeps NotesAI in the Windows system tray. Clipboard
        hotkeys and indexing continue. Use the tray menu to reopen NotesAI or
        quit completely.
      </p>
      <label className="startup-toggle">
        <input
          type="checkbox"
          checked={enabled}
          disabled={!desktop || busy}
          onChange={async (e) => {
            const value = e.target.checked;
            setBusy(true);
            setError("");
            try {
              await api.setStartup(value);
              setEnabled(value);
            } catch (e) {
              setError(errorText(e));
            } finally {
              setBusy(false);
            }
          }}
        />
        Start NotesAI with Windows, in the tray
      </label>
      <p className="settings-note">
        This switch applies immediately. Enable it from the installed app so
        Windows uses its permanent location.
      </p>
      <button
        className="secondary"
        disabled={!desktop}
        onClick={() => void api.hide().catch((e) => setError(errorText(e)))}
      >
        Run in background
      </button>
      <p>
        Copy text, then press Win + Alt + S or Ctrl + Shift + C. No open window
        is needed.
      </p>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
    </section>
  );
}
