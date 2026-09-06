import { Channel, invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  defaultSettings,
  normalizeSettings,
  type Discovery,
  type ChatEvent,
  type Hit,
  type Message,
  type Note,
  type Profile,
  type Settings,
} from "./types";
export const desktop = isTauri();
const seed: Note[] = [
  {
    id: "welcome",
    title: "A little less forgotten.",
    body: "# A little less forgotten.\n\nA useful thought, a passage worth keeping, a link you want to come back to. Give it a home here.\n\n## A space for your context\n\n**Capture first. Organize later.** Paste text, code, or a URL into a new capture. In the desktop app, copy something and press **Win + Alt + S** or **Ctrl + Shift + C** from any app.\n\nYour library stays on your computer. Local embeddings connect ideas across your notes, and the assistant brings back the passages behind its answers.\n\n## Make it yours\n\n1. Save your first piece of context.\n2. Choose your model in Settings.\n3. Ask a question in the assistant.\n\n> This is a sample note in the browser preview. Your desktop library starts empty.",
    kind: "note",
    tags: ["getting started"],
    source_url: null,
    created_at: new Date().toISOString(),
    status: "preview",
    error: null,
  },
  {
    id: "sample-2",
    title: "Good notes preserve the why",
    body: "# Good notes preserve the why\n\nA link tells you where to go. A note tells you why you wanted to go there.\n\nWhen saving a source, add a sentence about what caught your attention. Future you will appreciate the context.\n\nThis is sample content for the browser preview.",
    kind: "note",
    tags: ["ideas", "writing"],
    source_url: null,
    created_at: new Date(Date.now() - 86400000).toISOString(),
    status: "preview",
    error: null,
  },
];
function previewNotes(): Note[] {
  try {
    return (
      JSON.parse(localStorage.getItem("notesai-preview") || "null") ?? seed
    );
  } catch {
    return seed;
  }
}
function persist(notes: Note[]) {
  localStorage.setItem("notesai-preview", JSON.stringify(notes));
  window.dispatchEvent(new Event("notes-changed"));
}
export const api = {
  list: (): Promise<Note[]> =>
    desktop ? invoke("list_notes") : Promise.resolve(previewNotes()),
  capture: async (text: string, tags: string[]) => {
    if (desktop) return invoke<string>("capture", { text, tags });
    const id = crypto.randomUUID();
    persist([
      {
        id,
        title: text.split("\n")[0].slice(0, 100),
        body: text,
        source_url: /^https?:\/\/\S+$/.test(text) ? text : null,
        kind: /^https?:/.test(text) ? "link" : "note",
        tags,
        created_at: new Date().toISOString(),
        status: "preview",
        error: null,
      },
      ...previewNotes(),
    ]);
    return id;
  },
  edit: async (id: string, title: string, body: string, tags: string[]) => {
    if (desktop) return invoke("edit_note", { id, title, body, tags });
    persist(
      previewNotes().map((n) =>
        n.id === id
          ? {
              ...n,
              title,
              body,
              tags,
              organized: false,
              revision: (n.revision ?? 0) + 1,
            }
          : n,
      ),
    );
  },
  remove: async (id: string) => {
    if (desktop) return invoke("delete_note", { id });
    persist(previewNotes().filter((n) => n.id !== id));
  },
  retry: (id: string) => invoke("retry_note", { id }),
  settings: async (): Promise<Settings> => {
    const saved = desktop
      ? await invoke<Settings>("get_settings")
      : (JSON.parse(
          localStorage.getItem("notesai-preview-settings") || "null",
        ) ?? defaultSettings);
    return normalizeSettings(saved);
  },
  saveSettings: async (settings: Settings) => {
    if (desktop) return invoke("save_settings", { settings });
    localStorage.setItem("notesai-preview-settings", JSON.stringify(settings));
  },
  discover: (profile: Profile): Promise<Discovery> => {
    if (!desktop)
      return Promise.reject(
        new Error("Model discovery is available in the desktop app."),
      );
    return invoke("discover_models", { profile });
  },
  search: (query: string, top_k: number, topic = ""): Promise<Hit[]> => {
    if (!desktop)
      return Promise.reject(
        new Error(
          "Hybrid search requires the desktop app and local embeddings.",
        ),
      );
    return invoke("search", { query, topK: top_k, topic: topic || null });
  },
  chat: async (
    question: string,
    history: Message[],
    request_id: string,
    onEvent: (event: ChatEvent) => void,
    topic = "",
  ) => {
    if (!desktop)
      throw new Error(
        "Run the desktop app to use retrieval and chat. Browser preview does not simulate AI responses.",
      );
    const channel = new Channel<ChatEvent>();
    channel.onmessage = onEvent;
    await invoke("chat", {
      question,
      history: history.map(({ role, content }) => ({ role, content })),
      requestId: request_id,
      onEvent: channel,
      topic: topic || null,
    });
  },
  cancel: (request_id: string) =>
    invoke("cancel_chat", { requestId: request_id }),
  applyTopics: async (note: Note, topics: string[]) => {
    if (desktop)
      return invoke("apply_topics", {
        id: note.id,
        revision: note.revision ?? 0,
        topics,
      });
    persist(
      previewNotes().map((n) =>
        n.id === note.id ? { ...n, topics, organized: true } : n,
      ),
    );
  },
  startup: () =>
    desktop ? invoke<boolean>("startup_enabled") : Promise.resolve(false),
  setStartup: (enabled: boolean) => invoke("set_startup", { enabled }),
  hide: () => invoke("hide_window"),
  open: async (url: string) => {
    if (!/^https?:\/\//i.test(url)) return;
    if (desktop) await invoke("open_source", { url });
    else window.open(url, "_blank", "noopener,noreferrer");
  },
  subscribe: async (callback: () => void) => {
    if (desktop) return listen("notes-changed", callback);
    window.addEventListener("notes-changed", callback);
    return () => window.removeEventListener("notes-changed", callback);
  },
};
