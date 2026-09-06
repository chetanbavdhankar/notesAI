export interface Note {
  id: string;

  title: string;

  body: string;

  source_url: string | null;

  kind: string;

  tags: string[];

  created_at: string;

  status: string;

  error: string | null;
  topics?: string[];
  organized?: boolean;
  revision?: number;
}

export interface Profile {
  provider?: string | null;

  id: string;

  name: string;

  base_url: string;

  api_key: string;

  model_name: string;

  context_window_limit: number;

  temperature: number;
}

export interface Settings {
  profiles: Profile[];

  active_profile_id: string;

  top_k: number;

  appearance: Appearance;

  backup: BackupConfig;
  organizer_profile_id?: string;
}

export interface BackupConfig {
  automatic: boolean;
  interval_minutes: number;
  client_id: string;
  folder_link: string;
}

export interface BackupStatus {
  connected: boolean;
  account: string | null;
  local_path: string;
  folder_id: string;
  last_local_at: string | null;
  last_upload_at: string | null;
  compressed_bytes: number;
  note_count: number;
  error: string | null;
}

export const defaultBackup: BackupConfig = {
  automatic: false,
  interval_minutes: 15,
  client_id: "",
  folder_link: "",
};

export interface Appearance {
  mode: "light" | "dark" | "system";

  palette: string;

  accent: string;
}

export interface Discovery {
  base_url: string;

  models: string[];

  message: string;
}

export const defaultAppearance: Appearance = {
  mode: "light",

  palette: "sage",

  accent: "#527a43",
};

export const providerFor = (profile: Profile): string =>
  profile.provider ??
  (profile.name.toLowerCase() === "ollama" ||
  /:11434(?:\/|$)/.test(profile.base_url)
    ? "ollama"
    : "custom");

export function normalizeSettings(settings: Settings): Settings {
  return {
    ...settings,

    appearance: { ...defaultAppearance, ...settings.appearance },

    backup: { ...defaultBackup, ...settings.backup },

    profiles: settings.profiles.map((p) => ({
      ...p,

      provider: providerFor(p),
    })),
  };
}

export interface Hit {
  chunk_id: number;

  note_id: string;

  title: string;

  text: string;

  source_url: string | null;

  created_at: string;

  score: number;
}

export interface ChatEvent {
  request_id: string;

  kind: "sources" | "token" | "replace" | "done" | "error" | "status";

  text?: string;

  sources?: Hit[];
}

export interface Message {
  role: "user" | "assistant";

  content: string;

  sources?: Hit[];

  status?: string;
  error?: string;
}

export const presets = [
  {
    name: "Ollama",

    provider: "ollama",

    base_url: "http://127.0.0.1:11434/v1",

    model_name: "llama3.2",
  },

  {
    name: "LM Studio",

    provider: "lmstudio",

    base_url: "http://127.0.0.1:1234/v1",

    model_name: "",
  },

  {
    name: "OpenRouter",

    provider: "openrouter",

    base_url: "https://openrouter.ai/api/v1",

    model_name: "openai/gpt-4o-mini",
  },

  {
    name: "Gemini",

    provider: "gemini",

    base_url: "https://generativelanguage.googleapis.com/v1beta/openai",

    model_name: "gemini-2.5-flash",
  },

  {
    name: "Custom / Anthropic adapter",

    provider: "custom",

    base_url: "http://127.0.0.1:4000/v1",

    model_name: "",
  },
];

export const defaultSettings: Settings = {
  profiles: [
    {
      id: "ollama",

      ...presets[0],

      api_key: "",

      context_window_limit: 8192,

      temperature: 0.3,
    },
  ],

  active_profile_id: "ollama",

  top_k: 6,

  appearance: defaultAppearance,

  backup: defaultBackup,
};
