import { BackupSettings } from "./BackupSettings";

import { useEffect, useRef, useState } from "react";

import ReactMarkdown from "react-markdown";

import remarkGfm from "remark-gfm";

import { Check, Download, Plus, X } from "lucide-react";

import { api, desktop } from "./api";

import {
  presets,
  providerFor,
  type Profile,
  type Appearance,
  type Hit,
  type Settings,
} from "./types";

import { palettes } from "./appearance";

export const errorText = (e: unknown) =>
  e instanceof Error ? e.message : String(e);

export const tagsFrom = (text: string) => [
  ...new Set(
    text

      .split(",")

      .map((s) => s.trim())

      .filter(Boolean),
  ),
];

export function Modal({
  title,

  onClose,

  children,

  wide = false,
}: {
  title: string;

  onClose: () => void;

  children: React.ReactNode;

  wide?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);

  const closeRef = useRef(onClose);

  closeRef.current = onClose;

  useEffect(() => {
    const previous = document.activeElement as HTMLElement;

    ref.current?.querySelector<HTMLElement>("input,textarea,button")?.focus();

    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeRef.current();

      if (e.key === "Tab") {
        const nodes = ref.current?.querySelectorAll<HTMLElement>(
          "button:not(:disabled),input,textarea,select,a[href]",
        );

        if (!nodes?.length) return;

        const first = nodes[0],
          last = nodes[nodes.length - 1];

        if (e.shiftKey && document.activeElement === first) {
          e.preventDefault();

          last.focus();
        } else if (!e.shiftKey && document.activeElement === last) {
          e.preventDefault();

          first.focus();
        }
      }
    };

    document.addEventListener("keydown", handler);

    return () => {
      document.removeEventListener("keydown", handler);

      previous?.focus();
    };
  }, []);

  return (
    <div
      className="modal-backdrop"

      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className={`modal ${wide ? "wide" : ""}`}

        role="dialog"

        aria-modal="true"

        aria-label={title}

        ref={ref}
      >
        <div className="modal-head">
          <h2>{title}</h2>

          <button
            className="icon-button"

            aria-label="Close dialog"

            onClick={onClose}
          >
            <X size={20} />
          </button>
        </div>

        {children}
      </div>
    </div>
  );
}

export function Markdown({
  text,

  sources = [],

  onSource,
}: {
  text: string;

  sources?: Hit[];

  onSource?: (hit: Hit) => void;
}) {
  const prepared = text.replace(
    /\[\^(\d+)\](?!:)/g,

    (_, n) => `[${n}](#citation-${n})`,
  );

  return (
    <div className="markdown">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}

        components={{
          a: ({ href, children }) => {
            if (href?.startsWith("#citation-")) {
              const index = Number(href.slice(10)) - 1;

              const hit = sources[index];

              return (
                <button
                  className="citation"

                  disabled={!hit}

                  title={hit?.title ?? "Unverified citation"}

                  onClick={() => hit && onSource?.(hit)}
                >
                  {children}
                </button>
              );
            }

            return (
              <a
                href={href}

                onClick={(e) => {
                  e.preventDefault();

                  if (href) void api.open(href);
                }}
              >
                {children}
              </a>
            );
          },

          img: ({ alt }) => <span className="muted">[Image: {alt}]</span>,
        }}
      >
        {prepared}
      </ReactMarkdown>
    </div>
  );
}

export function SettingsDialog({
  initial,

  onClose,

  onSaved,

  onAppearancePreview,
}: {
  initial: Settings;

  onClose: () => void;

  onSaved: (s: Settings) => void;

  onAppearancePreview: (appearance: Appearance) => void;
}) {
  const [settings, setSettings] = useState<Settings>(structuredClone(initial));

  const [section, setSection] = useState("Models");

  const [selected, setSelected] = useState(initial.active_profile_id);

  const [models, setModels] = useState<string[]>([]);

  const [busy, setBusy] = useState(false);

  const [error, setError] = useState("");

  const [discoveryMessage, setDiscoveryMessage] = useState("");

  const discoveryGeneration = useRef(0);

  useEffect(
    () => () => {
      discoveryGeneration.current++;
    },

    [],
  );

  const profile = settings.profiles.find((p) => p.id === selected)!;

  const update = (patch: Partial<typeof profile>) =>
    setSettings((s) => ({
      ...s,

      profiles: s.profiles.map((p) =>
        p.id === selected ? { ...p, ...patch } : p,
      ),
    }));

  const clearDiscovery = () => {
    discoveryGeneration.current++;

    setModels([]);

    setDiscoveryMessage("");

    setError("");

    setBusy(false);
  };

  const discover = async (target: Profile = profile) => {
    const generation = ++discoveryGeneration.current;

    setBusy(true);

    setError("");

    setModels([]);

    setDiscoveryMessage("");

    try {
      const result = await api.discover(target);

      if (generation !== discoveryGeneration.current) return;

      setModels(result.models);

      setDiscoveryMessage(result.message);

      let sameServer = false;

      try {
        sameServer =
          new URL(target.base_url).origin === new URL(result.base_url).origin;
      } catch {
        /* A bind address may not yet be a URL. */
      }

      setSettings((s) => ({
        ...s,

        profiles: s.profiles.map((p) =>
          p.id === target.id
            ? {
                ...p,

                base_url: result.base_url,

                api_key: sameServer ? p.api_key : "",

                model_name: result.models.includes(p.model_name)
                  ? p.model_name
                  : (result.models[0] ?? ""),
              }
            : p,
        ),
      }));
    } catch (e) {
      if (generation === discoveryGeneration.current) setError(errorText(e));
    } finally {
      if (generation === discoveryGeneration.current) setBusy(false);
    }
  };

  const save = async () => {
    setError("");

    if (
      settings.profiles.some(
        (p) =>
          !p.name.trim() ||
          !/^https?:\/\//.test(p.base_url) ||
          p.context_window_limit < 2048 ||
          p.context_window_limit > 2000000 ||
          !Number.isFinite(p.temperature) ||
          p.temperature < 0 ||
          p.temperature > 2,
      )
    ) {
      setError("Check profile names, URLs, context windows, and temperatures.");

      return;
    }

    setBusy(true);

    try {
      await api.saveSettings(settings);

      onSaved(settings);

      onClose();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title="Make yourself at home" onClose={onClose} wide>
      <p className="modal-subtitle">Your models. Your context. Your choice.</p>

      <nav className="settings-tabs" aria-label="Settings sections">
        {["Models", "Appearance", "Retrieval", "Backup"].map((name) => (
          <button
            key={name}
            aria-pressed={section === name}
            onClick={() => setSection(name)}
          >
            {name}
          </button>
        ))}
      </nav>

      {section === "Appearance" && (
        <section className="appearance-settings" aria-label="Appearance">
          <div className="appearance-heading">
            <h3>Appearance</h3>

            <span>Preview your changes before saving.</span>
          </div>

          <fieldset>
            <legend>Theme</legend>

            <div className="theme-options">
              {(["light", "dark", "system"] as const).map((mode) => (
                <button
                  key={mode}

                  type="button"

                  aria-pressed={settings.appearance.mode === mode}

                  onClick={() => {
                    const appearance = { ...settings.appearance, mode };

                    setSettings((s) => ({ ...s, appearance }));

                    onAppearancePreview(appearance);
                  }}
                >
                  {mode === "system"
                    ? "Follow system"
                    : mode === "light"
                      ? "Light"
                      : "Dark"}
                </button>
              ))}
            </div>
          </fieldset>

          <fieldset>
            <legend>Color palette</legend>

            <div className="palette-options">
              {palettes.map((p) => (
                <button
                  key={p.id}

                  type="button"

                  aria-label={`${p.name} palette`}

                  aria-pressed={settings.appearance.palette === p.id}

                  onClick={() => {
                    const appearance = {
                      ...settings.appearance,
                      palette: p.id,
                    };

                    setSettings((s) => ({ ...s, appearance }));

                    onAppearancePreview(appearance);
                  }}
                >
                  <span
                    className="color-swatch"

                    style={{ background: p.color }}
                  />

                  {p.name}

                  {settings.appearance.palette === p.id && <Check size={13} />}
                </button>
              ))}
            </div>
          </fieldset>

          <div className="custom-accent">
            <button
              type="button"

              aria-pressed={settings.appearance.palette === "custom"}

              onClick={() => {
                const appearance = {
                  ...settings.appearance,
                  palette: "custom",
                };

                setSettings((s) => ({ ...s, appearance }));

                onAppearancePreview(appearance);
              }}
            >
              Custom accent
            </button>

            <input
              aria-label="Custom accent color"

              type="color"

              value={settings.appearance.accent}

              onChange={(e) => {
                const appearance = {
                  ...settings.appearance,

                  palette: "custom",

                  accent: e.target.value,
                };

                setSettings((s) => ({ ...s, appearance }));

                onAppearancePreview(appearance);
              }}
            />

            <span>{settings.appearance.accent.toUpperCase()}</span>

            <small>Shades adapt for readable text.</small>
          </div>
        </section>
      )}

      {section === "Models" && (
        <div className="settings-grid">
          <aside className="profile-list">
            <span className="eyebrow">MODEL PROFILES</span>

            {settings.profiles.map((p) => (
              <button
                key={p.id}

                className={selected === p.id ? "selected" : ""}

                onClick={() => {
                  setSelected(p.id);

                  clearDiscovery();
                }}
              >
                {p.name}

                {settings.active_profile_id === p.id && <Check size={14} />}
              </button>
            ))}

            <button
              onClick={() => {
                const id = crypto.randomUUID();

                setSettings((s) => ({
                  ...s,

                  profiles: [
                    ...s.profiles,

                    {
                      id,

                      provider: "custom",

                      name: "New profile",

                      base_url: "http://127.0.0.1:1234/v1",

                      api_key: "",

                      model_name: "",

                      context_window_limit: 8192,

                      temperature: 0.3,
                    },
                  ],
                }));

                setSelected(id);

                clearDiscovery();
              }}
            >
              <Plus size={15} /> Add profile
            </button>
          </aside>

          <div className="profile-fields">
            <label>
              Provider
              <select
                aria-label="Provider"

                value={providerFor(profile)}

                onChange={(e) => {
                  const preset = presets.find(
                    (p) => p.provider === e.target.value,
                  );

                  if (preset) {
                    clearDiscovery();

                    update({ ...preset, api_key: "" });

                    if (preset.provider === "ollama" && desktop)
                      void discover({ ...profile, ...preset, api_key: "" });
                  }
                }}
              >
                {presets.map((p) => (
                  <option key={p.name} value={p.provider}>
                    {p.name}
                  </option>
                ))}
              </select>
            </label>

            <label>
              Profile name
              <input
                value={profile.name}

                onChange={(e) => update({ name: e.target.value })}
              />
            </label>

            <label>
              API base URL
              <input
                value={profile.base_url}

                onChange={(e) => {
                  clearDiscovery();

                  update({ base_url: e.target.value });
                }}

                placeholder="https://provider.example/v1"
              />
            </label>

            <label>
              API key
              <input
                type="password"

                autoComplete="off"

                value={profile.api_key}

                onChange={(e) => {
                  clearDiscovery();

                  update({ api_key: e.target.value });
                }}

                placeholder="Not needed for most local runners"
              />
            </label>

            <div className="model-settings">
              <label htmlFor="model-id">Model ID</label>

              <div className="input-action">
                <input
                  id="model-id"

                  value={profile.model_name}

                  onChange={(e) => update({ model_name: e.target.value })}

                  placeholder="Model ID"
                />

                <button
                  className="secondary"

                  disabled={busy}

                  onClick={() => void discover()}
                >
                  <Download size={15} /> {busy ? "Discovering…" : "Discover"}
                </button>
              </div>

              {models.length > 0 && (
                <label className="installed-models">
                  {providerFor(profile) === "ollama"
                    ? "Installed Ollama models"
                    : "Available models"}

                  <select
                    id="available-models"

                    aria-label={
                      providerFor(profile) === "ollama"
                        ? "Installed Ollama models"
                        : "Available models"
                    }

                    value={
                      models.includes(profile.model_name)
                        ? profile.model_name
                        : ""
                    }

                    onChange={(e) => update({ model_name: e.target.value })}
                  >
                    <option value="" disabled>
                      Select a model
                    </option>

                    {models.map((m) => (
                      <option key={m} value={m}>
                        {m}
                      </option>
                    ))}
                  </select>
                </label>
              )}

              {discoveryMessage && (
                <p className="discovery-message" role="status">
                  {discoveryMessage}
                </p>
              )}

              {providerFor(profile) === "ollama" && !discoveryMessage && (
                <p className="discovery-hint">
                  Discover checks this address, OLLAMA_HOST, and local defaults.
                  Ollama must be running to list its installed models.
                </p>
              )}
            </div>

            <div className="two-fields">
              <label>
                Context window
                <input
                  type="number"

                  min={2048}

                  max={2000000}

                  value={profile.context_window_limit}

                  onChange={(e) =>
                    update({ context_window_limit: Number(e.target.value) })
                  }
                />
              </label>

              <label>
                Temperature
                <input
                  type="number"

                  min={0}

                  max={2}

                  step={0.1}

                  value={profile.temperature}

                  onChange={(e) =>
                    update({ temperature: Number(e.target.value) })
                  }
                />
              </label>
            </div>

            <div className="profile-actions">
              <button
                className="secondary"

                onClick={() =>
                  setSettings((s) => ({ ...s, active_profile_id: selected }))
                }
              >
                {settings.active_profile_id === selected ? (
                  <>
                    <Check size={14} /> Active profile
                  </>
                ) : (
                  "Use this profile"
                )}
              </button>

              {settings.profiles.length > 1 && (
                <button
                  className="text-button danger"

                  onClick={() => {
                    const rest = settings.profiles.filter(
                      (p) => p.id !== selected,
                    );

                    setSettings((s) => ({
                      ...s,

                      profiles: rest,

                      active_profile_id:
                        s.active_profile_id === selected
                          ? rest[0].id
                          : s.active_profile_id,
                    }));

                    setSelected(rest[0].id);

                    clearDiscovery();
                  }}
                >
                  Remove profile
                </button>
              )}
            </div>
          </div>
        </div>
      )}

      {section === "Retrieval" && (
        <>
          <div className="retrieval-setting">
            <div>
              <strong>Retrieval depth</strong>

              <p>Maximum source passages per answer</p>
            </div>

            <input
              aria-label="Top K passages"

              type="range"

              min={3}

              max={15}

              value={settings.top_k}

              onChange={(e) =>
                setSettings((s) => ({ ...s, top_k: Number(e.target.value) }))
              }
            />

            <span>{settings.top_k}</span>
          </div>

          <p className="settings-note">
            Embeddings run locally with BGE Small. The first index downloads the
            model. Remote profiles send your question and retrieved passages to
            that provider. API keys are stored in your local settings JSON; use
            a local runner to keep generation on-device.
          </p>
        </>
      )}

      {section === "Backup" && (
        <BackupSettings
          config={settings.backup}
          onChange={(backup) => setSettings((s) => ({ ...s, backup }))}
        />
      )}

      {!desktop && (
        <p className="notice">
          Browser preview: model connections and inference require the desktop
          app.
        </p>
      )}

      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}

      <div className="modal-footer">
        <button className="secondary" onClick={onClose}>
          Cancel
        </button>

        <button className="primary" disabled={busy} onClick={() => void save()}>
          Save settings
        </button>
      </div>
    </Modal>
  );
}
