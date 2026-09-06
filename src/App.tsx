import { useCallback, useEffect, useRef, useState } from "react";
import {
  ArrowDownUp,
  ArrowUp,
  BookOpen,
  Check,
  ChevronRight,
  CircleHelp,
  Clipboard,
  FileText,
  Globe,
  Library,
  MessageSquare,
  MoreHorizontal,
  PanelRightClose,
  Plus,
  Search,
  Settings2,
  ShieldCheck,
  Sparkles,
  Square,
  Tag,
  Trash2,
  X,
  Youtube,
  Pencil,
  ExternalLink,
  RotateCw,
} from "lucide-react";
import { api, desktop } from "./api";
import { useAppearance } from "./appearance";
import {
  Markdown,
  Modal,
  SettingsDialog,
  errorText,
  tagsFrom,
} from "./components";
import {
  defaultSettings,
  type Hit,
  type Message,
  type Note,
  type Settings,
  type Appearance,
} from "./types";
type Filter = "all" | "note" | "link" | "video" | "social";
const date = (s: string) =>
  new Date(s).toLocaleDateString(undefined, { month: "short", day: "numeric" });
const iconFor = (kind: string, size = 17) =>
  kind === "video" ? (
    <Youtube size={size} />
  ) : kind === "note" ? (
    <FileText size={size} />
  ) : (
    <Globe size={size} />
  );
export default function App() {
  const [appearancePreview, setAppearancePreview] = useState<Appearance | null>(
    null,
  );
  const [notes, setNotes] = useState<Note[]>([]),
    [activeId, setActiveId] = useState(""),
    [filter, setFilter] = useState<Filter>("all"),
    [query, setQuery] = useState(""),
    [tag, setTag] = useState(""),
    [dateFilter, setDateFilter] = useState(""),
    [reverse, setReverse] = useState(false);
  const [settings, setSettings] = useState<Settings>(defaultSettings),
    [settingsOpen, setSettingsOpen] = useState(false),
    [captureOpen, setCaptureOpen] = useState(false),
    [help, setHelp] = useState(false),
    [mode, setMode] = useState<"note" | "chat">("note"),
    [reference, setReference] = useState<Hit | null>(null);
  const [error, setError] = useState(""),
    [toast, setToast] = useState(""),
    [loading, setLoading] = useState(true),
    [hybridHits, setHybridHits] = useState<Hit[] | null>(null),
    [searching, setSearching] = useState(false);
  const [messages, setMessages] = useState<Message[]>([]),
    [question, setQuestion] = useState(""),
    [streaming, setStreaming] = useState(false);
  const requestId = useRef("");
  useAppearance(appearancePreview ?? settings.appearance);
  const chatEnd = useRef<HTMLDivElement>(null);
  const autoScroll = useRef(true);
  const active = notes.find((n) => n.id === activeId);
  const profile = settings.profiles.find(
    (p) => p.id === settings.active_profile_id,
  );
  const refresh = useCallback(async () => {
    try {
      const result = await api.list();
      setNotes(result);
      setActiveId((id) =>
        result.some((n) => n.id === id) ? id : (result[0]?.id ?? ""),
      );
    } catch (e) {
      setError(errorText(e));
    } finally {
      setLoading(false);
    }
  }, []);
  useEffect(() => {
    void refresh();
    void api
      .settings()
      .then(setSettings)
      .catch((e) => setError(errorText(e)));
    let dispose: (() => void) | undefined;
    let alive = true;
    void api
      .subscribe(() => void refresh())
      .then((fn) => {
        if (alive) dispose = fn;
        else fn();
      });
    return () => {
      alive = false;
      dispose?.();
    };
  }, [refresh]);
  useEffect(() => {
    if (!toast) return;
    const timer = setTimeout(() => setToast(""), 3500);
    return () => clearTimeout(timer);
  }, [toast]);
  useEffect(() => {
    const container = chatEnd.current?.parentElement;
    if (container && autoScroll.current)
      container.scrollTop = container.scrollHeight;
  }, [messages]);
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "k") {
        e.preventDefault();
        document.getElementById("library-search")?.focus();
      }
      if ((e.ctrlKey || e.metaKey) && e.key === "n") {
        e.preventDefault();
        setCaptureOpen(true);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);
  let filtered = notes.filter(
    (n) =>
      (filter === "all" || n.kind === filter) &&
      (!tag || n.tags.includes(tag)) &&
      (!dateFilter || n.created_at.slice(0, 10) >= dateFilter) &&
      `${n.title} ${n.body} ${n.tags.join(" ")}`
        .toLowerCase()
        .includes(query.toLowerCase()),
  );
  if (reverse) filtered = [...filtered].reverse();
  const tags = [...new Set(notes.flatMap((n) => n.tags))].sort();
  const ask = async (text = question) => {
    if (!text.trim() || streaming) return;
    autoScroll.current = true;
    const history = messages.filter((m) => m.content && !m.error);
    setQuestion("");

    setMode("chat");
    setStreaming(true);
    requestId.current = crypto.randomUUID();
    const id = requestId.current;
    setMessages((m) => [
      ...m,
      { role: "user", content: text },
      { role: "assistant", content: "", sources: [] },
    ]);
    try {
      await api.chat(text, history, id, (event) => {
        if (event.request_id !== requestId.current) return;
        setMessages((m) => {
          const next = [...m];
          const last = { ...next[next.length - 1] };
          if (event.kind === "error")
            last.error = event.text ?? "Generation failed";
          if (event.kind === "status") last.status = event.text;
          if (event.kind === "sources") last.sources = event.sources;
          if (event.kind === "token") last.content += event.text ?? "";
          next[next.length - 1] = last;
          return next;
        });
      });
    } catch (e) {
      setMessages((m) =>
        m.map((item, i) =>
          i === m.length - 1 ? { ...item, error: errorText(e) } : item,
        ),
      );
    } finally {
      setStreaming(false);
    }
  };
  const hybrid = async () => {
    if (!query.trim()) return;
    setSearching(true);
    setError("");
    try {
      setHybridHits(await api.search(query, settings.top_k));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setSearching(false);
    }
  };
  const select = (id: string) => {
    setActiveId(id);
    setMode("note");
    setReference(null);
  };
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">
            <svg viewBox="0 0 64 64" aria-hidden="true">
              <rect className="brand-tile" x="0" y="0" width="64" height="64" />
              <rect
                className="brand-page"
                x="14"
                y="9"
                width="36"
                height="47"
                rx="6"
              />
              <path className="brand-lines" d="M23 25h18M23 32h18" />
              <path
                className="brand-spark"
                d="M38 37c1.4 4.7 3.9 7.2 8.6 8.6-4.7 1.4-7.2 3.9-8.6 8.6-1.4-4.7-3.9-7.2-8.6-8.6 4.7-1.4 7.2-3.9 8.6-8.6Z"
              />
            </svg>
          </div>
          <span>
            notes<span className="brand-ai">ai</span>
          </span>
          <span className="local-label">LOCAL</span>
        </div>
        <button className="capture-button" onClick={() => setCaptureOpen(true)}>
          <Plus size={18} /> New capture <kbd>Ctrl N</kbd>
        </button>
        <div className="nav-section">
          <span className="eyebrow">YOUR SPACE</span>
          {(
            [
              ["all", "All captures", Library],
              ["note", "Notes", FileText],
              ["link", "Links", Globe],
              ["video", "Videos", Youtube],
              ["social", "Social", MessageSquare],
            ] as const
          ).map(([key, label, Icon]) => (
            <button
              className={`nav-item ${filter === key ? "active" : ""}`}
              key={key}
              onClick={() => {
                setFilter(key);
                setHybridHits(null);
              }}
            >
              <Icon size={18} />
              <span>{label}</span>
              <small>
                {key === "all"
                  ? notes.length
                  : notes.filter((n) => n.kind === key).length}
              </small>
            </button>
          ))}
        </div>
        <div className="nav-section tags-nav">
          <span className="eyebrow">COLLECTION TAGS</span>
          {tags.length ? (
            tags.map((t) => (
              <button
                key={t}
                className={`tag-nav ${tag === t ? "selected" : ""}`}
                onClick={() => setTag(tag === t ? "" : t)}
              >
                <span className="tag-dot" />
                {t}
              </button>
            ))
          ) : (
            <p className="sidebar-hint">
              Add tags to a capture to start a collection.
            </p>
          )}
        </div>
        <div className="sidebar-bottom">
          <div className="shortcut-card">
            <Clipboard size={18} />
            <strong>Catch a passing thought.</strong>
            <p>Copy anything, then press</p>
            <div>
              <kbd>Win</kbd>
              <span>+</span>
              <kbd>Alt</kbd>
              <span>+</span>
              <kbd>S</kbd>
            </div>
          </div>
          <button className="nav-item" onClick={() => setSettingsOpen(true)}>
            <Settings2 size={18} />
            <span>Settings</span>
          </button>
          <button className="nav-item" onClick={() => setHelp(true)}>
            <CircleHelp size={18} />
            <span>A quick guide</span>
          </button>
          <div className="local-status">
            <span /> {desktop ? "Stored on this device" : "Browser preview"}
            <ShieldCheck size={14} />
          </div>
        </div>
      </aside>
      <main className="main">
        <header className="topbar">
          <div className="breadcrumb">
            Your space <ChevronRight size={14} />
            <strong>
              {filter === "all"
                ? "All captures"
                : filter.charAt(0).toUpperCase() + filter.slice(1) + "s"}
            </strong>
          </div>
          <div className="topbar-right">
            <span className="quiet-label">A little less forgotten.</span>
            <button
              className="avatar"
              aria-label="Open settings"
              onClick={() => setSettingsOpen(true)}
            >
              You
            </button>
          </div>
        </header>
        <div className="workspace-heading">
          <div>
            <div className="eyebrow">COLLECT. CONNECT. RECALL.</div>
            <h1>
              Your knowledge, <em>within reach.</em>
            </h1>
            <p>A home for the things you want to remember.</p>
          </div>
          <button
            className="assistant-button"
            onClick={() => setMode(mode === "chat" ? "note" : "chat")}
          >
            <Sparkles size={17} />
            {mode === "chat" ? "Back to note" : "Ask your library"}
            <ChevronRight size={16} />
          </button>
        </div>
        {!desktop && (
          <div className="preview-banner">
            <span className="tag-dot" /> Interface preview · Sample notes and
            browser captures stay in this browser. Run the desktop app for local
            indexing and AI.
          </div>
        )}
        {error && (
          <div className="error app-error" role="alert">
            {error}
            <button
              className="icon-button"
              aria-label="Dismiss error"
              onClick={() => setError("")}
            >
              <X size={16} />
            </button>
          </div>
        )}
        <div className="workspace">
          <section className="feed">
            <div className="feed-search">
              <Search size={18} />
              <input
                id="library-search"
                aria-label="Search captures"
                placeholder="Find something you saved…"
                value={query}
                onChange={(e) => {
                  setQuery(e.target.value);
                  setHybridHits(null);
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void hybrid();
                }}
              />
              <kbd>Ctrl K</kbd>
            </div>
            <div className="feed-toolbar">
              <span>
                {hybridHits
                  ? `${hybridHits.length} passages`
                  : `${filtered.length} captures`}
                {tag && (
                  <button className="filter-chip" onClick={() => setTag("")}>
                    {tag} ×
                  </button>
                )}
              </span>
              <div>
                <input
                  aria-label="Captured since"
                  title="Captured since"
                  className="date-filter"
                  type="date"
                  value={dateFilter}
                  onChange={(e) => setDateFilter(e.target.value)}
                />
                <button
                  title="Reverse date order"
                  aria-label="Reverse date order"
                  className="icon-button"
                  onClick={() => setReverse(!reverse)}
                >
                  <ArrowDownUp size={15} />
                </button>
              </div>
            </div>
            {query && (
              <button
                className="hybrid-button"
                disabled={searching}
                onClick={() => void hybrid()}
              >
                <Sparkles size={14} />
                {searching
                  ? "Searching your context…"
                  : "Search meaning + keywords"}
                <span>↵</span>
              </button>
            )}
            <div className="note-list">
              {loading ? (
                <div className="empty-small">Opening your library…</div>
              ) : hybridHits ? (
                hybridHits.map((hit) => (
                  <button
                    className="note-card"
                    key={hit.chunk_id}
                    onClick={() => {
                      setReference(hit);
                      setActiveId(hit.note_id);
                    }}
                  >
                    <span className="card-meta">
                      <Sparkles size={14} /> MATCHED PASSAGE
                    </span>
                    <h3>{hit.title}</h3>
                    <p>{hit.text}</p>
                    <span className="card-footer">
                      {date(hit.created_at)}
                      <span>View source ↗</span>
                    </span>
                  </button>
                ))
              ) : (
                filtered.map((note) => (
                  <button
                    className={`note-card ${activeId === note.id ? "selected" : ""}`}
                    key={note.id}
                    onClick={() => select(note.id)}
                  >
                    <span className="card-meta">
                      {iconFor(note.kind, 14)}
                      {note.kind === "note" ? "NOTE" : note.kind.toUpperCase()}
                      <span
                        className={`status-dot ${note.status}`}
                        title={note.status}
                      />
                    </span>
                    <h3>{note.title.replace(/^#+\s*/, "")}</h3>
                    <p>
                      {note.body
                        .replace(/[#*>`]/g, "")
                        .replace(/^\s*[^\n]+\n/, "")
                        .trim() || note.source_url}
                    </p>
                    <div className="card-tags">
                      {note.tags.slice(0, 3).map((t) => (
                        <span key={t}>{t}</span>
                      ))}
                    </div>
                    <span className="card-footer">
                      {date(note.created_at)}
                      <span>
                        {note.status === "ready"
                          ? "Indexed"
                          : note.status === "preview"
                            ? "Preview"
                            : note.status === "failed"
                              ? "Needs attention"
                              : note.status + "…"}
                      </span>
                    </span>
                  </button>
                ))
              )}
              {!loading &&
                (hybridHits?.length === 0 ||
                  (!hybridHits && !filtered.length)) && (
                  <div className="empty-small">
                    <BookOpen size={28} />
                    <strong>
                      {notes.length
                        ? "Nothing matches just yet."
                        : "Start with something worth keeping."}
                    </strong>
                    <p>
                      {notes.length
                        ? "Try a different search or clear your filters."
                        : "A thought, a link, a small discovery."}
                    </p>
                    <button
                      className="secondary"
                      onClick={() => {
                        if (notes.length) {
                          setQuery("");
                          setTag("");
                          setDateFilter("");
                          setFilter("all");
                          setHybridHits(null);
                        } else setCaptureOpen(true);
                      }}
                    >
                      {notes.length ? "Clear filters" : "Create a capture"}
                    </button>
                  </div>
                )}
            </div>
            <div className="feed-bottom">
              <ShieldCheck size={13} /> A private library, built one thought at
              a time.
            </div>
          </section>
          <section className="detail-panel">
            <div className="panel-toolbar">
              <div className="segmented">
                <button
                  className={mode === "note" ? "active" : ""}
                  onClick={() => setMode("note")}
                >
                  <FileText size={15} /> Note
                </button>
                <button
                  className={mode === "chat" ? "active" : ""}
                  onClick={() => setMode("chat")}
                >
                  <Sparkles size={15} /> Assistant
                </button>
              </div>
              <span className="panel-label">
                {mode === "chat"
                  ? `${settings.top_k} passages · ${profile?.name ?? "No model"}`
                  : "YOUR SAVED CONTEXT"}
              </span>
            </div>
            {mode === "note" ? (
              active ? (
                <NoteDetail
                  key={active.id}
                  note={active}
                  onChanged={() => void refresh()}
                  onError={setError}
                  onAsk={() => {
                    setMode("chat");
                    setQuestion(`What are the key ideas in "${active.title}"?`);
                  }}
                />
              ) : (
                <div className="detail-empty">
                  <div className="empty-art">
                    <BookOpen size={38} />
                  </div>
                  <h2>Make room for a good idea.</h2>
                  <p>
                    Save something that sparks your curiosity.
                    <br />
                    It will be right here when you need it.
                  </p>
                  <button
                    className="primary"
                    onClick={() => setCaptureOpen(true)}
                  >
                    <Plus size={16} /> Your first capture
                  </button>
                </div>
              )
            ) : (
              <>
                <div
                  className="chat-content"
                  onScroll={(e) => {
                    const el = e.currentTarget;
                    autoScroll.current =
                      el.scrollHeight - el.scrollTop - el.clientHeight < 80;
                  }}
                >
                  {!messages.length ? (
                    <div className="chat-welcome">
                      <div className="sparkle-orbit">
                        <Sparkles size={30} />
                      </div>
                      <span className="eyebrow">THINK WITH YOUR LIBRARY</span>
                      <h2>What’s on your mind?</h2>
                      <p>
                        Find the thread between your saved ideas.
                        <br />
                        Every answer brings its sources along.
                      </p>
                      <div className="suggestions">
                        {[
                          "What have I been thinking about lately?",
                          "Find connections across my notes",
                          "Summarize my saved research",
                        ].map((s) => (
                          <button key={s} onClick={() => void ask(s)}>
                            {s}
                            <ArrowUp size={15} />
                          </button>
                        ))}
                      </div>
                    </div>
                  ) : (
                    messages.map((m, i) => (
                      <div className={`chat-message ${m.role}`} key={i}>
                        <div className="message-label">
                          {m.role === "user" ? "You" : "NotesAI"}
                        </div>
                        <Markdown
                          text={
                            m.content ||
                            (streaming && i === messages.length - 1 && !m.error
                              ? m.status || "Finding the right passages…"
                              : "No answer generated.")
                          }
                          sources={m.sources}
                          onSource={setReference}
                        />
                        {m.error && (
                          <p className="error" role="alert">
                            {m.error}
                          </p>
                        )}
                        {!!m.sources?.length && (
                          <div className="source-chips">
                            {m.sources.map(
                              (h, n) =>
                                m.content.includes(`[^${n + 1}]`) && (
                                  <button
                                    key={h.chunk_id}
                                    onClick={() => setReference(h)}
                                  >
                                    <span>{n + 1}</span>
                                    {h.title.slice(0, 32)}
                                  </button>
                                ),
                            )}
                          </div>
                        )}
                      </div>
                    ))
                  )}
                  <div ref={chatEnd} />
                </div>
                <form
                  className="composer-area"
                  onSubmit={(e) => {
                    e.preventDefault();
                    void ask();
                  }}
                >
                  <div className="composer">
                    <textarea
                      aria-label="Ask your library"
                      placeholder="Ask a question about your saved context…"
                      value={question}
                      onChange={(e) => setQuestion(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" && !e.shiftKey) {
                          e.preventDefault();
                          void ask();
                        }
                      }}
                      rows={1}
                    />
                    <div className="composer-bottom">
                      <button
                        type="button"
                        className="model-button"
                        onClick={() => setSettingsOpen(true)}
                      >
                        <span className="tag-dot" />
                        {profile?.model_name || "Choose a model"}
                      </button>
                      {streaming ? (
                        <button
                          type="button"
                          className="send"
                          aria-label="Stop generating"
                          onClick={() =>
                            void api
                              .cancel(requestId.current)
                              .catch((e) => setError(errorText(e)))
                          }
                        >
                          <Square size={15} />
                        </button>
                      ) : (
                        <button
                          className="send"
                          disabled={!question.trim()}
                          aria-label="Send question"
                        >
                          <ArrowUp size={19} />
                        </button>
                      )}
                    </div>
                  </div>
                  <p>
                    Answers use retrieved context. Open a citation to check the
                    source.{" "}
                    <button
                      type="button"
                      disabled={streaming}
                      onClick={() => {
                        setMessages([]);
                      }}
                    >
                      Clear chat
                    </button>
                  </p>
                </form>
              </>
            )}
          </section>
          {reference && (
            <aside className="reference-drawer">
              <header>
                <span>
                  <BookOpen size={16} /> SOURCE PASSAGE
                </span>
                <button
                  className="icon-button"
                  aria-label="Close reference"
                  onClick={() => setReference(null)}
                >
                  <PanelRightClose size={19} />
                </button>
              </header>
              <h3>{reference.title}</h3>
              <p className="reference-date">
                Captured {date(reference.created_at)} · Passage{" "}
                {reference.chunk_id}
              </p>
              <div className="highlight-passage">
                <Markdown text={reference.text} />
              </div>
              <button
                className="secondary"
                onClick={() => select(reference.note_id)}
              >
                <FileText size={15} /> Open captured note
              </button>
              {reference.source_url && (
                <button
                  className="text-button"
                  onClick={() => void api.open(reference.source_url!)}
                >
                  <ExternalLink size={14} /> Original source
                </button>
              )}
            </aside>
          )}
        </div>
      </main>
      {captureOpen && (
        <CaptureDialog
          onClose={() => setCaptureOpen(false)}
          onCaptured={(id) => {
            void refresh();
            setActiveId(id);
            setMode("note");
            setToast("Captured. A good idea, kept.");
          }}
        />
      )}
      {settingsOpen && (
        <SettingsDialog
          initial={settings}
          onClose={() => {
            setSettingsOpen(false);
            setAppearancePreview(null);
          }}
          onAppearancePreview={setAppearancePreview}
          onSaved={(s) => {
            setSettings(s);
            setToast("Settings saved");
          }}
        />
      )}
      {help && (
        <Modal
          title="Capture now. Connect later."
          onClose={() => setHelp(false)}
        >
          <div className="help-content">
            <p>
              Copy text or a URL in any app, then press <kbd>Win + Alt + S</kbd>{" "}
              or <kbd>Ctrl + Shift + C</kbd>. Both shortcuts capture the current
              clipboard.
            </p>
            <p>
              Use <strong>New capture</strong> for text, code, links, and tags.
              Captures are saved immediately, then hydrated and indexed in the
              background. Failed items can be retried.
            </p>
            <p>
              Choose a model in <strong>Settings</strong>. Ollama and LM Studio
              keep generation local. Remote models receive the passages needed
              to answer your question.
            </p>
            <p>
              The first capture downloads BGE Small. YouTube transcripts use the
              bundled <strong>yt-dlp</strong> reader. Pages requiring sign-in
              may not be readable.
            </p>
            <p>
              Closing the desktop window keeps capture running in the system
              tray. Use the tray menu to quit.
            </p>
          </div>
        </Modal>
      )}
      {toast && (
        <div className="toast" role="status">
          <Check size={17} />
          {toast}
        </div>
      )}
    </div>
  );
}
function CaptureDialog({
  onClose,
  onCaptured,
}: {
  onClose: () => void;
  onCaptured: (id: string) => void;
}) {
  const [text, setText] = useState(""),
    [tags, setTags] = useState(""),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const save = async () => {
    if (!text.trim()) return;
    setBusy(true);
    try {
      const id = await api.capture(text.trim(), tagsFrom(tags));
      onCaptured(id);
      onClose();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal title="Keep a little context" onClose={onClose}>
      <p className="modal-subtitle">
        A note, a link, a piece of code. It all belongs here.
      </p>
      <label>
        Your capture
        <textarea
          autoFocus
          className="capture-text"
          placeholder="Paste a URL or write something worth remembering…"
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
      </label>
      <label>
        Tags <span className="muted">· optional, separated by commas</span>
        <input
          placeholder="ideas, research, to read"
          value={tags}
          onChange={(e) => setTags(e.target.value)}
        />
      </label>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      <div className="modal-footer">
        <span className="muted">
          <ShieldCheck size={14} /> Saved on this device
        </span>
        <button
          className="primary"
          disabled={busy || !text.trim()}
          onClick={() => void save()}
        >
          {busy ? "Saving…" : "Save capture"}
          <ArrowUp size={16} />
        </button>
      </div>
    </Modal>
  );
}
function NoteDetail({
  note,
  onChanged,
  onError,
  onAsk,
}: {
  note: Note;
  onChanged: () => void;
  onError: (s: string) => void;
  onAsk: () => void;
}) {
  const [editing, setEditing] = useState(false),
    [title, setTitle] = useState(note.title),
    [body, setBody] = useState(note.body),
    [tags, setTags] = useState(note.tags.join(", ")),
    [busy, setBusy] = useState(false),
    [confirm, setConfirm] = useState(false);
  useEffect(() => {
    if (!editing) {
      setTitle(note.title);
      setBody(note.body);
      setTags(note.tags.join(", "));
    }
  }, [note, editing]);
  const save = async () => {
    setBusy(true);
    try {
      await api.edit(note.id, title, body, tagsFrom(tags));
      setEditing(false);
      onChanged();
    } catch (e) {
      onError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <div className="note-detail">
        <div className="detail-meta">
          <span className="type-pill">
            {iconFor(note.kind, 13)}
            {note.kind}
          </span>
          <span>{date(note.created_at)}</span>
          <div className="detail-actions">
            <button
              className="icon-button"
              title="Edit note"
              aria-label="Edit note"
              onClick={() => setEditing(!editing)}
            >
              <Pencil size={16} />
            </button>
            <button
              className="icon-button"
              title="Delete note"
              aria-label="Delete note"
              onClick={() => setConfirm(true)}
            >
              <Trash2 size={16} />
            </button>
          </div>
        </div>
        {editing ? (
          <div className="editor">
            <label>
              Title
              <input value={title} onChange={(e) => setTitle(e.target.value)} />
            </label>
            <label>
              Markdown
              <textarea
                value={body}
                onChange={(e) => setBody(e.target.value)}
              />
            </label>
            <label>
              Tags
              <input value={tags} onChange={(e) => setTags(e.target.value)} />
            </label>
            <div className="profile-actions">
              <button
                className="primary"
                disabled={busy || !body.trim()}
                onClick={() => void save()}
              >
                {busy ? "Saving…" : "Save changes"}
              </button>
              <button className="secondary" onClick={() => setEditing(false)}>
                Cancel
              </button>
            </div>
          </div>
        ) : (
          <>
            <h2 className="note-title">{note.title.replace(/^#+\s*/, "")}</h2>
            <div className="detail-tags">
              {note.tags.map((t) => (
                <span key={t}>
                  <Tag size={11} />
                  {t}
                </span>
              ))}
            </div>
            {note.source_url && (
              <button
                className="source-link"
                onClick={() => void api.open(note.source_url!)}
              >
                <Globe size={14} />
                <span>{note.source_url}</span>
                <ExternalLink size={13} />
              </button>
            )}
            <div className="note-rule" />
            <Markdown text={note.body.replace(/^#\s+[^\n]+\n/, "")} />
          </>
        )}
        {note.error && <p className="notice">{note.error}</p>}
        {note.status === "failed" && (
          <button
            className="secondary"
            onClick={() =>
              void api
                .retry(note.id)
                .then(onChanged)
                .catch((e) => onError(errorText(e)))
            }
          >
            <RotateCw size={14} /> Retry indexing
          </button>
        )}
        {["queued", "hydrating", "indexing"].includes(note.status) && (
          <p className="notice">
            <span className="pulse" />{" "}
            {note.status === "indexing"
              ? "Building local embeddings. The first run downloads the model."
              : "Preparing your capture…"}
          </p>
        )}
      </div>
      <div className="detail-footer">
        <span>
          <ShieldCheck size={14} />
          {note.status === "ready"
            ? "Indexed and ready to recall"
            : note.status === "preview"
              ? "Browser preview capture"
              : "Saved locally"}
        </span>
        <button className="text-button" onClick={onAsk}>
          <Sparkles size={14} /> Ask about this note
        </button>
      </div>
      {confirm && (
        <Modal title="Delete this capture?" onClose={() => setConfirm(false)}>
          <p>
            “{note.title}” and its search index will be removed from this
            device.
          </p>
          <div className="modal-footer">
            <button className="secondary" onClick={() => setConfirm(false)}>
              Keep capture
            </button>
            <button
              className="primary destructive"
              disabled={busy}
              onClick={async () => {
                setBusy(true);
                try {
                  await api.remove(note.id);
                  setConfirm(false);
                  onChanged();
                } catch (e) {
                  onError(errorText(e));
                } finally {
                  setBusy(false);
                }
              }}
            >
              Delete capture
            </button>
          </div>
        </Modal>
      )}
    </>
  );
}
