import {
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import {
  GhostBtn,
  SecondaryBtn,
  PrimaryBtn,
  Toggle,
  InputField,
  StatusDot,
  Divider,
  type StatusDotVariant,
} from "./ui";
import { X } from "./ui/Icons";
import {
  getSettings,
  setSettings as saveSettings,
  getVaultSettings,
  setVaultSettings,
  copilotStatus,
  copilotLoginStart,
  copilotLoginPoll,
  copilotLogout,
  copilotModels,
  listModels,
  startModelDownload,
  cancelModelDownload,
  deleteModel,
  detectGpu,
  onModelDownloadProgress,
  binaryStatus,
  installWhisperCpp,
  installPiper,
  cancelBinaryInstall,
  onBinaryInstall,
  testAnthropic,
  testOpenai,
  testGemini,
  testCopilot,
  testOpenaiCompat,
  listProviderModels,
  runtimeCapabilities,
  type Settings,
  type VaultSettings,
  type ProviderConfig,
  type RoutedModel,
  type CopilotStatus,
  type CopilotModel,
  type DeviceCode,
  type ModelInfo,
  type GpuStatus,
  type DownloadProgress,
  type BinaryStatus,
  type BinaryInstallEvent,
  type ProviderModel,
  type ProviderTestResult,
} from "../lib/tauri";

interface Props {
  open: boolean;
  onClose: () => void;
  // Per-vault scope. The provider/voice surfaces that already live on
  // the legacy global Settings stay there for now; new fields (ai
  // routing, system_prompt, tools_allowed) are sourced and saved on
  // VaultSettings.
  vaultPath: string | null;
}

type TabId =
  | "providers"
  | "routing"
  | "context"
  | "tools"
  | "prompts"
  | "voice"
  | "terminal"
  | "chatfiles";

const TABS: { id: TabId; label: string }[] = [
  { id: "providers", label: "Providers" },
  { id: "routing", label: "Routing" },
  { id: "context", label: "Context" },
  { id: "tools", label: "Tools" },
  { id: "prompts", label: "Prompts" },
  { id: "voice", label: "Voice" },
  { id: "terminal", label: "Terminal" },
  { id: "chatfiles", label: "Chat files" },
];

// Module-level caches so reopening the modal doesn't re-hit the slow
// Rust probes (Vulkan/CUDA detection, whisper/piper binary lookup,
// Copilot OAuth status). These rarely change during a session — the
// only times they DO are when the user installs a binary or signs in,
// both of which happen INSIDE this modal and update the cache via
// setBinStatus / setCopilot. TTL gives a safety net in case external
// state changes (user runs `apt install` outside, etc.).
const CACHE_TTL_MS = 60_000;
let gpuCache: { ts: number; value: GpuStatus } | null = null;
let binCache: { ts: number; value: BinaryStatus } | null = null;
let copilotCache: { ts: number; value: CopilotStatus } | null = null;

function cachedFetch<T>(
  cache: { ts: number; value: T } | null,
  fetcher: () => Promise<T>,
  setCache: (v: { ts: number; value: T }) => void,
): Promise<T> {
  if (cache && Date.now() - cache.ts < CACHE_TTL_MS) {
    return Promise.resolve(cache.value);
  }
  return fetcher().then((v) => {
    setCache({ ts: Date.now(), value: v });
    return v;
  });
}

// AI configuration modal. Visual layout mirrors
// forge_ui/SettingsModal.jsx::AISettingsModal (provider cards, routing
// grid, tools list) but the backend integrations from the prior
// SettingsModal.tsx — Copilot device-code login, model-catalog
// downloads, whisper-cli/piper binary installers — are preserved
// verbatim. Provider-card rewrite wraps those flows inside the new
// card shell.
export default function AISettingsModal({ open, onClose, vaultPath }: Props) {
  const [tab, setTab] = useState<TabId>("providers");

  const [settings, setLocal] = useState<Settings | null>(null);
  const [vaultSettings, setVaultLocal] = useState<VaultSettings | null>(null);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [progress, setProgress] = useState<Record<string, DownloadProgress>>(
    {},
  );
  const [gpu, setGpu] = useState<GpuStatus | null>(null);
  // Build capabilities — drives the local-LLM UI visibility. Lite
  // builds (no `local-llm` Cargo feature) set local_llm=false, hiding
  // the "Local GGUF (in-process)" provider card and stripping it from
  // the routing dropdown. Defaults to `true` while the IPC resolves so
  // full builds don't flicker the card in/out on boot.
  const [caps, setCaps] = useState<{ local_llm: boolean }>({ local_llm: true });
  const [copilot, setCopilot] = useState<CopilotStatus>({
    signed_in: false,
    login: null,
  });
  const [copilotModelList, setCopilotModelList] = useState<CopilotModel[]>([]);
  const [deviceCode, setDeviceCode] = useState<DeviceCode | null>(null);
  const [pollMsg, setPollMsg] = useState("");
  const [binStatus, setBinStatus] = useState<BinaryStatus>({
    whisper_cli: null,
    piper: null,
  });
  const [binInstall, setBinInstall] = useState<
    Record<string, BinaryInstallEvent>
  >({});
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  // Live test results per provider id ("anthropic", "openai", "gemini",
  // "copilot", "openai_compat"). null = never tested, otherwise carries
  // the result for the green-tick / red-cross summary line.
  const [providerTests, setProviderTests] = useState<
    Record<string, ProviderTestResult | null>
  >({});
  const [providerModels, setProviderModels] = useState<
    Record<string, ProviderModel[]>
  >({});
  const [providerTesting, setProviderTesting] = useState<Record<string, boolean>>(
    {},
  );
  const unlistenRef = useRef<(() => void) | null>(null);
  const unlistenBinRef = useRef<(() => void) | null>(null);
  // Debounced vault-settings persist. Each call replaces the pending
  // timer so rapid edits coalesce into a single write.
  const vaultSaveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (!open) return;
    getSettings().then(setLocal).catch(console.error);
    runtimeCapabilities().then(setCaps).catch(console.error);
    if (vaultPath) {
      getVaultSettings(vaultPath)
        .then((vs) => {
          // One-shot migration: legacy "deepgram" stt_provider rewrites
          // to "whisper" the moment the modal opens with that vault.
          // Phase 3 spec §7.1 — Deepgram is gone.
          if (vs.voice.stt_provider === "deepgram") {
            const fixed = {
              ...vs,
              voice: { ...vs.voice, stt_provider: "whisper" },
            };
            setVaultLocal(fixed);
            void setVaultSettings(vaultPath, fixed).catch((err) =>
              console.warn("deepgram migration save failed", err),
            );
          } else {
            setVaultLocal(vs);
          }
          // Cache-first model hydration so dropdowns are populated even
          // before the user clicks Test.
          for (const provider of [
            "anthropic",
            "openai",
            "gemini",
            "copilot",
            "openai_compat",
          ]) {
            const cfg = vs.ai.providers[provider];
            const apiKey = cfg?.api_key ?? undefined;
            const baseUrl = cfg?.base_url ?? undefined;
            if (
              provider === "copilot" ||
              (apiKey && (provider !== "openai_compat" || baseUrl))
            ) {
              listProviderModels(provider, apiKey, baseUrl)
                .then((m) =>
                  setProviderModels((prev) => ({ ...prev, [provider]: m })),
                )
                .catch(() => {});
            }
          }
        })
        .catch((e) => {
          console.warn("vault settings load failed", e);
          setVaultLocal(null);
        });
    } else {
      setVaultLocal(null);
    }
    refreshModels();
    cachedFetch(gpuCache, detectGpu, (c) => {
      gpuCache = c;
    })
      .then(setGpu)
      .catch(console.error);
    cachedFetch(copilotCache, copilotStatus, (c) => {
      copilotCache = c;
    })
      .then((s) => {
        setCopilot(s);
        if (s.signed_in) {
          copilotModels()
            .then(setCopilotModelList)
            .catch((e) => {
              console.warn("copilot models fetch failed", e);
              setCopilotModelList([]);
            });
        } else {
          setCopilotModelList([]);
        }
      })
      .catch(console.error);
    setDeviceCode(null);
    setPollMsg("");
    setDirty(false);

    cachedFetch(binCache, binaryStatus, (c) => {
      binCache = c;
    })
      .then(setBinStatus)
      .catch(console.error);

    let active = true;
    onModelDownloadProgress((p) => {
      if (!active) return;
      setProgress((prev) => ({ ...prev, [p.id]: p }));
      if (
        p.phase === "done" ||
        p.phase === "cancelled" ||
        p.phase === "error"
      ) {
        refreshModels();
      }
    }).then((un) => {
      unlistenRef.current = un;
    });

    onBinaryInstall((e) => {
      if (!active) return;
      setBinInstall((prev) => ({ ...prev, [e.id]: e }));
      if (e.phase === "done" || e.phase === "error") {
        binCache = null; // freshly installed/uninstalled — invalidate
        binaryStatus().then(setBinStatus).catch(console.error);
      }
    }).then((un) => {
      unlistenBinRef.current = un;
    });

    return () => {
      active = false;
      if (unlistenRef.current) {
        unlistenRef.current();
        unlistenRef.current = null;
      }
      if (unlistenBinRef.current) {
        unlistenBinRef.current();
        unlistenBinRef.current = null;
      }
    };
  }, [open, vaultPath]);

  const refreshModels = () => {
    listModels().then(setModels).catch(console.error);
  };

  if (!open) return null;

  const update = <K extends keyof Settings>(k: K, v: Settings[K]) => {
    if (!settings) return;
    setLocal({ ...settings, [k]: v });
    setDirty(true);
  };

  // Mutate vault settings + flush to disk via debounced setVaultSettings.
  // 800 ms matches the spec's debounce target. State is updated
  // synchronously so the UI is never out of sync with the pending write.
  const updateVault = (mutate: (vs: VaultSettings) => VaultSettings) => {
    if (!vaultPath || !vaultSettings) return;
    const next = mutate(vaultSettings);
    setVaultLocal(next);
    setDirty(true);
    if (vaultSaveTimer.current) clearTimeout(vaultSaveTimer.current);
    vaultSaveTimer.current = setTimeout(() => {
      setVaultSettings(vaultPath, next).catch((e) =>
        console.warn("vault settings save failed", e),
      );
    }, 800);
  };

  const updateProviderConfig = (
    providerId: string,
    patch: Partial<ProviderConfig>,
  ) => {
    updateVault((vs) => ({
      ...vs,
      ai: {
        ...vs.ai,
        providers: {
          ...vs.ai.providers,
          [providerId]: {
            api_key: vs.ai.providers[providerId]?.api_key ?? null,
            base_url: vs.ai.providers[providerId]?.base_url ?? null,
            default_model: vs.ai.providers[providerId]?.default_model ?? null,
            ...patch,
          },
        },
      },
    }));
  };

  const updateRoutingSlot = (
    slot: keyof VaultSettings["ai"]["routing"],
    next: RoutedModel | null,
  ) => {
    updateVault((vs) => ({
      ...vs,
      ai: {
        ...vs.ai,
        routing: { ...vs.ai.routing, [slot]: next },
      },
    }));
  };

  const runProviderTest = async (providerId: string) => {
    if (!vaultSettings) return;
    setProviderTesting((prev) => ({ ...prev, [providerId]: true }));
    try {
      const cfg = vaultSettings.ai.providers[providerId] ?? {
        api_key: null,
        base_url: null,
        default_model: null,
      };
      let result: ProviderTestResult;
      switch (providerId) {
        case "anthropic":
          result = await testAnthropic(
            cfg.api_key ?? "",
            cfg.base_url ?? undefined,
          );
          break;
        case "openai":
          result = await testOpenai(
            cfg.api_key ?? "",
            cfg.base_url ?? undefined,
          );
          break;
        case "gemini":
          result = await testGemini(cfg.api_key ?? "");
          break;
        case "copilot":
          result = await testCopilot();
          break;
        case "openai_compat":
          result = await testOpenaiCompat(
            cfg.api_key ?? "",
            cfg.base_url ?? "",
          );
          break;
        default:
          return;
      }
      setProviderTests((prev) => ({ ...prev, [providerId]: result }));
      if (result.ok) {
        setProviderModels((prev) => ({
          ...prev,
          [providerId]: result.models,
        }));
        // Auto-pick the first model as default if none chosen yet.
        if (!cfg.default_model && result.models.length > 0) {
          updateProviderConfig(providerId, {
            default_model: result.models[0].id,
          });
        }
      }
    } catch (e) {
      setProviderTests((prev) => ({
        ...prev,
        [providerId]: { ok: false, error: String(e), models: [] },
      }));
    } finally {
      setProviderTesting((prev) => ({ ...prev, [providerId]: false }));
    }
  };

  const save = async () => {
    if (!settings) return;
    setSaving(true);
    try {
      await saveSettings(settings);
      // Mirror provider/model/voice settings into VaultSettings so the
      // new scope stays in sync with what the legacy fields already
      // capture. We touch only the AI/voice slots — other fields pass
      // through untouched (theme, widths, recent_files, etc.).
      if (vaultPath && vaultSettings) {
        const next: VaultSettings = {
          ...vaultSettings,
          ai: {
            ...vaultSettings.ai,
            providers: {
              ...vaultSettings.ai.providers,
              anthropic: {
                ...(vaultSettings.ai.providers.anthropic ?? {
                  api_key: null,
                  base_url: null,
                  default_model: null,
                }),
                api_key: settings.api_key,
                default_model: settings.api_model,
              },
            },
          },
          voice: {
            ...vaultSettings.voice,
            stt_provider:
              settings.stt_provider === "local"
                ? "whisper"
                : settings.stt_provider,
            tts_voice: settings.edge_tts_voice,
          },
        };
        setVaultLocal(next);
        await setVaultSettings(vaultPath, next).catch((e) =>
          console.warn("vault settings save failed", e),
        );
      }
      setDirty(false);
    } catch (e) {
      console.error("save failed", e);
      alert(`Save failed: ${e}`);
    } finally {
      setSaving(false);
    }
  };

  const startCopilotLogin = async () => {
    try {
      const dc = await copilotLoginStart();
      setDeviceCode(dc);
      setPollMsg("Waiting for authorization…");
      const tick = async () => {
        try {
          const res = await copilotLoginPoll();
          switch (res.status) {
            case "ok":
              setPollMsg(`Signed in as ${res.login ?? "(unknown)"}`);
              setDeviceCode(null);
              copilotCache = null;
              setCopilot(await copilotStatus());
              copilotModels()
                .then(setCopilotModelList)
                .catch(() => setCopilotModelList([]));
              return;
            case "denied":
              setPollMsg("Access denied");
              setDeviceCode(null);
              return;
            case "expired":
              setPollMsg("Code expired. Try again.");
              setDeviceCode(null);
              return;
            case "other":
              setPollMsg(`Error: ${res.message}`);
              return;
            case "slow_down":
              setPollMsg("Slowing down…");
              setTimeout(tick, (dc.interval + 5) * 1000);
              return;
            case "pending":
              setPollMsg("Waiting for authorization…");
              setTimeout(tick, dc.interval * 1000);
              return;
            case "no_code":
              setPollMsg("No pending code");
              return;
          }
        } catch (e) {
          setPollMsg(`Poll error: ${e}`);
        }
      };
      setTimeout(tick, dc.interval * 1000);
    } catch (e) {
      setPollMsg(`Start failed: ${e}`);
    }
  };

  const logoutCopilot = async () => {
    await copilotLogout();
    copilotCache = null;
    setCopilot({ signed_in: false, login: null });
    setCopilotModelList([]);
  };

  const copyCode = () => {
    if (!deviceCode) return;
    navigator.clipboard?.writeText(deviceCode.user_code).catch(() => {});
  };

  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed",
        inset: 0,
        background: "var(--modal-backdrop)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: "var(--z-modal-backdrop)" as unknown as number,
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          background: "var(--background-primary)",
          border: "1px solid var(--background-modifier-border)",
          borderRadius: "var(--radius-l)",
          boxShadow: "var(--shadow-l)",
          width: 800,
          maxWidth: "96vw",
          height: "85vh",
          display: "flex",
          flexDirection: "column",
          zIndex: "var(--z-modal)" as unknown as number,
        }}
      >
        <div
          style={{
            padding: "20px 24px 0",
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
          }}
        >
          <span
            style={{
              fontSize: "var(--font-ui-larger)",
              fontWeight: 600,
              color: "var(--text-normal)",
            }}
          >
            AI Settings
          </span>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            {gpu && (
              <span
                style={{
                  fontSize: "var(--font-ui-smaller)",
                  color: "var(--text-muted)",
                  padding: "2px 8px",
                  borderRadius: "var(--radius-s)",
                  border: "1px solid var(--background-modifier-border)",
                }}
              >
                {gpu.cuda_available ? "GPU" : "CPU only"} · {gpu.details}
              </span>
            )}
            <GhostBtn icon={<X size={16} />} label="Close" onClick={onClose} />
          </div>
        </div>

        <div
          style={{
            display: "flex",
            gap: 0,
            padding: "0 24px",
            borderBottom: "1px solid var(--background-modifier-border)",
            marginTop: 12,
            overflowX: "auto",
          }}
        >
          {TABS.map((t) => (
            <TabButton
              key={t.id}
              active={tab === t.id}
              onClick={() => setTab(t.id)}
              label={t.label}
            />
          ))}
        </div>

        <div
          style={{ flex: 1, overflowY: "auto", padding: "16px 24px 20px" }}
        >
          {!settings ? (
            <ProvidersSkeleton />
          ) : (
            <>
              {tab === "providers" && (
                <ProvidersTab
                  settings={settings}
                  update={update}
                  models={models}
                  progress={progress}
                  copilot={copilot}
                  copilotModelList={copilotModelList}
                  deviceCode={deviceCode}
                  pollMsg={pollMsg}
                  startCopilotLogin={startCopilotLogin}
                  logoutCopilot={logoutCopilot}
                  copyCode={copyCode}
                  refreshModels={refreshModels}
                  vaultSettings={vaultSettings}
                  updateProviderConfig={updateProviderConfig}
                  providerTests={providerTests}
                  providerModels={providerModels}
                  providerTesting={providerTesting}
                  runProviderTest={runProviderTest}
                  caps={caps}
                />
              )}
              {tab === "routing" && (
                <RoutingTab
                  vaultSettings={vaultSettings}
                  providerModels={providerModels}
                  updateRoutingSlot={updateRoutingSlot}
                  caps={caps}
                />
              )}
              {tab === "context" && <ContextTab />}
              {tab === "tools" && <ToolsTab />}
              {tab === "prompts" && (
                <PromptsTab
                  vaultSettings={vaultSettings}
                  updateVault={updateVault}
                />
              )}
              {tab === "voice" && (
                <VoiceTab
                  settings={settings}
                  update={update}
                  models={models}
                  progress={progress}
                  binStatus={binStatus}
                  binInstall={binInstall}
                  refreshModels={refreshModels}
                />
              )}
              {tab === "terminal" && <Placeholder label="Terminal" />}
              {tab === "chatfiles" && <Placeholder label="Chat files" />}
            </>
          )}
        </div>

        <div
          style={{
            padding: "12px 24px",
            borderTop: "1px solid var(--background-modifier-border)",
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            gap: 8,
          }}
        >
          <span
            style={{
              fontSize: "var(--font-ui-smaller)",
              color: "var(--text-muted)",
            }}
          >
            {dirty ? "Unsaved changes" : "All saved"}
          </span>
          <div style={{ display: "flex", gap: 8 }}>
            <SecondaryBtn onClick={onClose}>Close</SecondaryBtn>
            <PrimaryBtn onClick={save}>
              {saving ? "Saving…" : "Save"}
            </PrimaryBtn>
          </div>
        </div>
      </div>
    </div>
  );
}

function TabButton({
  active,
  onClick,
  label,
}: {
  active: boolean;
  onClick: () => void;
  label: string;
}) {
  return (
    <button
      onClick={onClick}
      style={{
        height: 36,
        padding: "0 12px",
        background: "transparent",
        border: 0,
        borderBottom: active
          ? "2px solid var(--text-accent)"
          : "2px solid transparent",
        color: active ? "var(--text-normal)" : "var(--text-muted)",
        fontSize: "var(--font-ui-medium)",
        fontWeight: 500,
        cursor: "pointer",
        whiteSpace: "nowrap",
      }}
      onMouseEnter={(e) => {
        if (!active)
          e.currentTarget.style.background =
            "var(--background-modifier-hover)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.background = "transparent";
      }}
    >
      {label}
    </button>
  );
}

function ProviderCard({
  name,
  status,
  statusLabel,
  active,
  onActivate,
  activatable = true,
  children,
}: {
  name: string;
  status: StatusDotVariant;
  statusLabel: string;
  // When `active` is true, shows an "Active" pill and highlights the
  // border. When false and `onActivate` is provided, shows "Make active"
  // — clicking saves settings.ai_provider so chat routes to this provider.
  active?: boolean;
  onActivate?: () => void;
  // Some cards (notably Local GGUF, which uses a separate code path)
  // shouldn't show the activate affordance. Default true for normal
  // remote providers.
  activatable?: boolean;
  children: ReactNode;
}) {
  return (
    <div
      style={{
        background: "var(--background-primary-alt)",
        border: active
          ? "1px solid var(--interactive-accent)"
          : "1px solid var(--background-modifier-border)",
        borderRadius: "var(--radius-m)",
        padding: 16,
        marginBottom: 12,
        boxShadow: active
          ? "0 0 0 2px hsla(38, 65%, 55%, 0.10)"
          : "none",
        transition: "border-color 120ms ease, box-shadow 120ms ease",
      }}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: 12,
          gap: 8,
        }}
      >
        <span
          style={{
            fontSize: "var(--font-ui-medium)",
            fontWeight: 600,
            color: "var(--text-normal)",
          }}
        >
          {name}
        </span>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            fontSize: "var(--font-ui-small)",
            color: "var(--text-muted)",
          }}
        >
          {activatable &&
            (active ? (
              <span
                style={{
                  fontSize: 10.5,
                  fontWeight: 600,
                  letterSpacing: "0.04em",
                  textTransform: "uppercase",
                  color: "var(--text-on-accent)",
                  background: "var(--interactive-accent)",
                  padding: "2px 7px",
                  borderRadius: 999,
                }}
              >
                Active
              </span>
            ) : onActivate ? (
              <button
                type="button"
                onClick={onActivate}
                style={{
                  fontSize: 10.5,
                  fontWeight: 500,
                  color: "var(--text-muted)",
                  background: "transparent",
                  border: "1px solid var(--background-modifier-border)",
                  borderRadius: 999,
                  padding: "2px 8px",
                  cursor: "pointer",
                }}
                title={`Make ${name} the active provider for chat`}
              >
                Make active
              </button>
            ) : null)}
          <StatusDot variant={status} />
          {statusLabel}
        </div>
      </div>
      {children}
    </div>
  );
}

function FieldRow({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        marginBottom: 8,
      }}
    >
      <span
        style={{
          width: 100,
          fontSize: "var(--font-ui-small)",
          fontWeight: 500,
          color: "var(--text-muted)",
          flexShrink: 0,
        }}
      >
        {label}
      </span>
      {children}
    </div>
  );
}

const selectStyle: CSSProperties = {
  height: 32,
  padding: "0 10px",
  borderRadius: "var(--radius-s)",
  background: "var(--background-modifier-form-field)",
  border: "1px solid var(--background-modifier-border)",
  color: "var(--text-normal)",
  fontSize: "var(--font-ui-medium)",
  flex: 1,
};

// ── Providers tab ──
interface ProvidersProps {
  settings: Settings;
  update: <K extends keyof Settings>(k: K, v: Settings[K]) => void;
  models: ModelInfo[];
  progress: Record<string, DownloadProgress>;
  copilot: CopilotStatus;
  copilotModelList: CopilotModel[];
  deviceCode: DeviceCode | null;
  pollMsg: string;
  startCopilotLogin: () => Promise<void>;
  logoutCopilot: () => Promise<void>;
  copyCode: () => void;
  refreshModels: () => void;
  vaultSettings: VaultSettings | null;
  updateProviderConfig: (
    providerId: string,
    patch: Partial<ProviderConfig>,
  ) => void;
  providerTests: Record<string, ProviderTestResult | null>;
  providerModels: Record<string, ProviderModel[]>;
  providerTesting: Record<string, boolean>;
  runProviderTest: (providerId: string) => Promise<void>;
  caps: { local_llm: boolean };
}

// Single-line summary for the Test button result. Friendly green tick on
// success, red cross + truncated error on failure.
function TestStatusLine({
  testing,
  result,
}: {
  testing: boolean;
  result: ProviderTestResult | null | undefined;
}) {
  if (testing) {
    return (
      <span
        style={{
          fontSize: "var(--font-ui-smaller)",
          color: "var(--text-muted)",
        }}
      >
        Testing…
      </span>
    );
  }
  if (!result) return null;
  if (result.ok) {
    const first = result.models[0];
    const ctx = first
      ? `, context ${Math.round(first.capabilities.context_window / 1000)}k`
      : "";
    return (
      <span
        style={{
          fontSize: "var(--font-ui-smaller)",
          color: "var(--color-green, #4caf50)",
        }}
      >
        ✓ Connected — {result.models.length} models{ctx}
      </span>
    );
  }
  return (
    <span
      style={{
        fontSize: "var(--font-ui-smaller)",
        color: "var(--text-error, #f44)",
        overflow: "hidden",
        textOverflow: "ellipsis",
        whiteSpace: "nowrap",
        maxWidth: 380,
      }}
      title={result.error ?? ""}
    >
      ✗ {result.error ?? "Failed"}
    </span>
  );
}

// Re-usable model dropdown that swaps to a free-text fallback when there
// are no enumerated models yet (eg user has not clicked Test).
function ModelSelect({
  value,
  models,
  onChange,
  placeholder,
}: {
  value: string;
  models: ProviderModel[] | undefined;
  onChange: (id: string) => void;
  placeholder?: string;
}) {
  if (!models || models.length === 0) {
    return (
      <InputField
        value={value}
        placeholder={placeholder ?? "model id"}
        onChange={onChange}
        style={{ flex: 1 }}
      />
    );
  }
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      style={selectStyle}
    >
      {!models.some((m) => m.id === value) && value && (
        <option value={value}>{value}</option>
      )}
      <option value="" disabled>
        Select…
      </option>
      {models.map((m) => (
        <option key={m.id} value={m.id}>
          {m.display_name}
        </option>
      ))}
    </select>
  );
}

function ProvidersTab(props: ProvidersProps) {
  const {
    settings,
    update,
    models,
    progress,
    copilot,
    copilotModelList,
    deviceCode,
    pollMsg,
    startCopilotLogin,
    logoutCopilot,
    copyCode,
    refreshModels,
    vaultSettings,
    updateProviderConfig,
    providerTests,
    providerModels,
    providerTesting,
    runProviderTest,
    caps,
  } = props;

  const llmModels = models.filter((m) => m.kind === "llm");
  const downloadedLlm = llmModels.filter((m) => m.downloaded);

  // Pull provider config out of vault settings with safe defaults so the
  // inputs are always controlled even before the file exists.
  const providerCfg = (id: string): ProviderConfig => {
    return (
      vaultSettings?.ai.providers[id] ?? {
        api_key: null,
        base_url: null,
        default_model: null,
      }
    );
  };

  const cardStatus = (id: string) => {
    const t = providerTests[id];
    if (t?.ok) return { status: "connected" as const, label: "connected" };
    if (t && !t.ok) return { status: "error" as const, label: "test failed" };
    const cfg = providerCfg(id);
    if (cfg.api_key) return { status: "idle" as const, label: "key set" };
    return { status: "idle" as const, label: "not configured" };
  };

  return (
    <>
      <ProviderCard
        name="Anthropic"
        status={cardStatus("anthropic").status}
        statusLabel={cardStatus("anthropic").label}
        active={settings.ai_provider === "anthropic"}
        onActivate={() => update("ai_provider", "anthropic")}
      >
        <FieldRow label="API key">
          <InputField
            type="password"
            value={providerCfg("anthropic").api_key ?? ""}
            placeholder="sk-ant-..."
            onChange={(v) =>
              updateProviderConfig("anthropic", { api_key: v || null })
            }
            style={{ flex: 1 }}
          />
          <SecondaryBtn
            onClick={() => void runProviderTest("anthropic")}
          >
            {providerTesting["anthropic"] ? "Testing…" : "Test"}
          </SecondaryBtn>
        </FieldRow>
        <FieldRow label="Default model">
          <ModelSelect
            value={providerCfg("anthropic").default_model ?? ""}
            models={providerModels["anthropic"]}
            onChange={(id) =>
              updateProviderConfig("anthropic", { default_model: id })
            }
            placeholder="claude-sonnet-4-6"
          />
        </FieldRow>
        <TestStatusLine
          testing={!!providerTesting["anthropic"]}
          result={providerTests["anthropic"]}
        />
      </ProviderCard>

      <ProviderCard
        name="OpenAI"
        status={cardStatus("openai").status}
        statusLabel={cardStatus("openai").label}
        active={settings.ai_provider === "openai"}
        onActivate={() => update("ai_provider", "openai")}
      >
        <FieldRow label="API key">
          <InputField
            type="password"
            value={providerCfg("openai").api_key ?? ""}
            placeholder="sk-..."
            onChange={(v) =>
              updateProviderConfig("openai", { api_key: v || null })
            }
            style={{ flex: 1 }}
          />
          <SecondaryBtn onClick={() => void runProviderTest("openai")}>
            {providerTesting["openai"] ? "Testing…" : "Test"}
          </SecondaryBtn>
        </FieldRow>
        <FieldRow label="Base URL">
          <InputField
            value={providerCfg("openai").base_url ?? ""}
            placeholder="https://api.openai.com"
            onChange={(v) =>
              updateProviderConfig("openai", { base_url: v || null })
            }
            style={{ flex: 1 }}
          />
        </FieldRow>
        <FieldRow label="Default model">
          <ModelSelect
            value={providerCfg("openai").default_model ?? ""}
            models={providerModels["openai"]}
            onChange={(id) =>
              updateProviderConfig("openai", { default_model: id })
            }
            placeholder="gpt-4o"
          />
        </FieldRow>
        <TestStatusLine
          testing={!!providerTesting["openai"]}
          result={providerTests["openai"]}
        />
      </ProviderCard>

      <ProviderCard
        name="Gemini"
        status={cardStatus("gemini").status}
        statusLabel={cardStatus("gemini").label}
        active={settings.ai_provider === "gemini"}
        onActivate={() => update("ai_provider", "gemini")}
      >
        <FieldRow label="API key">
          <InputField
            type="password"
            value={providerCfg("gemini").api_key ?? ""}
            placeholder="AI..."
            onChange={(v) =>
              updateProviderConfig("gemini", { api_key: v || null })
            }
            style={{ flex: 1 }}
          />
          <SecondaryBtn onClick={() => void runProviderTest("gemini")}>
            {providerTesting["gemini"] ? "Testing…" : "Test"}
          </SecondaryBtn>
        </FieldRow>
        <FieldRow label="Default model">
          <ModelSelect
            value={providerCfg("gemini").default_model ?? ""}
            models={providerModels["gemini"]}
            onChange={(id) =>
              updateProviderConfig("gemini", { default_model: id })
            }
            placeholder="gemini-2.0-flash"
          />
        </FieldRow>
        <TestStatusLine
          testing={!!providerTesting["gemini"]}
          result={providerTests["gemini"]}
        />
      </ProviderCard>

      <div
        style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}
      >
        <ProviderCard
          name="OpenAI-compatible"
          status={cardStatus("openai_compat").status}
          statusLabel={cardStatus("openai_compat").label}
          active={settings.ai_provider === "openai_compat"}
          onActivate={() => update("ai_provider", "openai_compat")}
        >
          <FieldRow label="Base URL">
            <InputField
              value={providerCfg("openai_compat").base_url ?? ""}
              placeholder="http://localhost:11434/v1"
              onChange={(v) =>
                updateProviderConfig("openai_compat", { base_url: v || null })
              }
              style={{ flex: 1 }}
            />
          </FieldRow>
          <FieldRow label="API key">
            <InputField
              type="password"
              value={providerCfg("openai_compat").api_key ?? ""}
              placeholder="(optional for local)"
              onChange={(v) =>
                updateProviderConfig("openai_compat", { api_key: v || null })
              }
              style={{ flex: 1 }}
            />
            <SecondaryBtn
              onClick={() => void runProviderTest("openai_compat")}
            >
              {providerTesting["openai_compat"] ? "Testing…" : "Test"}
            </SecondaryBtn>
          </FieldRow>
          <FieldRow label="Default model">
            <ModelSelect
              value={providerCfg("openai_compat").default_model ?? ""}
              models={providerModels["openai_compat"]}
              onChange={(id) =>
                updateProviderConfig("openai_compat", { default_model: id })
              }
            />
          </FieldRow>
          <TestStatusLine
            testing={!!providerTesting["openai_compat"]}
            result={providerTests["openai_compat"]}
          />
        </ProviderCard>

        <ProviderCard
          name="Copilot"
          status={copilot.signed_in ? "connected" : "idle"}
          statusLabel={copilot.signed_in ? "logged in" : "not signed in"}
          active={settings.ai_provider === "copilot"}
          onActivate={() => update("ai_provider", "copilot")}
        >
          {copilot.signed_in ? (
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                gap: 8,
              }}
            >
              <div
                style={{
                  fontSize: "var(--font-ui-small)",
                  color: "var(--text-muted)",
                }}
              >
                Signed in as{" "}
                <span
                  style={{
                    color: "var(--text-normal)",
                    fontWeight: 500,
                  }}
                >
                  {copilot.login ?? "(unknown)"}
                </span>
              </div>
              <SecondaryBtn onClick={logoutCopilot}>Sign out</SecondaryBtn>
            </div>
          ) : deviceCode ? (
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <div
                style={{
                  fontSize: "var(--font-ui-small)",
                  color: "var(--text-muted)",
                }}
              >
                Open this URL and enter the code:
              </div>
              <a
                href={deviceCode.verification_uri}
                target="_blank"
                rel="noopener noreferrer"
                style={{
                  fontSize: "var(--font-ui-small)",
                  color: "var(--text-accent)",
                  textDecoration: "underline",
                }}
              >
                {deviceCode.verification_uri}
              </a>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <code
                  style={{
                    padding: "4px 10px",
                    fontSize: 14,
                    letterSpacing: "0.2em",
                    background: "var(--background-primary)",
                    border:
                      "1px solid var(--background-modifier-border)",
                    borderRadius: "var(--radius-s)",
                  }}
                >
                  {deviceCode.user_code}
                </code>
                <SecondaryBtn onClick={copyCode}>Copy</SecondaryBtn>
              </div>
              <div
                style={{
                  fontSize: "var(--font-ui-smaller)",
                  color: "var(--text-muted)",
                }}
              >
                {pollMsg}
              </div>
            </div>
          ) : (
            <PrimaryBtn onClick={startCopilotLogin}>
              Sign in with GitHub
            </PrimaryBtn>
          )}
          {copilot.signed_in && (
            <>
              <div style={{ height: 8 }} />
              <FieldRow label="Model">
                {copilotModelList.length > 0 ? (
                  <select
                    value={settings.copilot_model}
                    onChange={(e) => update("copilot_model", e.target.value)}
                    style={selectStyle}
                  >
                    {!copilotModelList.some(
                      (m) => m.id === settings.copilot_model,
                    ) && (
                      <option value={settings.copilot_model}>
                        {settings.copilot_model || "(pick one)"}
                      </option>
                    )}
                    {copilotModelList.map((m) => (
                      <option key={m.id} value={m.id}>
                        {m.name}
                        {m.vendor ? ` (${m.vendor})` : ""}
                      </option>
                    ))}
                  </select>
                ) : (
                  <InputField
                    value={settings.copilot_model}
                    onChange={(v) => update("copilot_model", v)}
                    placeholder="loading..."
                    style={{ flex: 1 }}
                  />
                )}
                <SecondaryBtn onClick={() => void runProviderTest("copilot")}>
                  {providerTesting["copilot"] ? "Testing…" : "Test"}
                </SecondaryBtn>
              </FieldRow>
              <TestStatusLine
                testing={!!providerTesting["copilot"]}
                result={providerTests["copilot"]}
              />
            </>
          )}
        </ProviderCard>
      </div>

      {caps.local_llm && (
      <ProviderCard
        name="Local GGUF (in-process)"
        status={settings.model_path ? "connected" : "idle"}
        active={settings.ai_provider === "local"}
        onActivate={() => update("ai_provider", "local")}
        statusLabel={
          settings.model_path ? "model loaded" : "no model loaded"
        }
      >
        <FieldRow label="Model">
          {downloadedLlm.length === 0 ? (
            <span
              style={{
                flex: 1,
                fontSize: "var(--font-ui-small)",
                color: "var(--text-faint)",
                fontStyle: "italic",
              }}
            >
              No local models downloaded yet — use the catalogue below.
            </span>
          ) : (
            <select
              value={settings.model_path ?? ""}
              onChange={(e) => update("model_path", e.target.value || null)}
              style={selectStyle}
            >
              <option value="" disabled>
                Select a model…
              </option>
              {downloadedLlm.map((m) => (
                <option key={m.id} value={m.local_path ?? ""}>
                  {m.name}
                </option>
              ))}
            </select>
          )}
        </FieldRow>
        <FieldRow label="GPU layers">
          <InputField
            type="number"
            value={String(settings.gpu_layers)}
            onChange={(v) => update("gpu_layers", parseInt(v || "0", 10))}
            style={{ width: 100 }}
          />
        </FieldRow>
        <FieldRow label="Context">
          <InputField
            type="number"
            value={String(settings.ctx_size)}
            onChange={(v) => update("ctx_size", parseInt(v || "0", 10))}
            style={{ width: 100 }}
          />
        </FieldRow>
        <Divider style={{ margin: "12px 0" }} />
        <div
          style={{
            fontSize: "var(--font-ui-smaller)",
            fontWeight: 600,
            color: "var(--text-muted)",
            textTransform: "uppercase",
            letterSpacing: "0.04em",
            marginBottom: 8,
          }}
        >
          Catalogue
        </div>
        <DownloadList
          entries={llmModels}
          progress={progress}
          onDownload={(id) => startModelDownload(id)}
          onCancel={(id) => cancelModelDownload(id)}
          onDelete={async (id) => {
            await deleteModel(id);
            refreshModels();
            const m = llmModels.find((x) => x.id === id);
            if (m && settings.model_path === m.local_path) {
              update("model_path", null);
            }
          }}
        />
      </ProviderCard>
      )}
    </>
  );
}

// ── Routing tab ──
const ROUTING_PROVIDERS: { id: string; label: string }[] = [
  { id: "anthropic", label: "Anthropic" },
  { id: "openai", label: "OpenAI" },
  { id: "gemini", label: "Gemini" },
  { id: "copilot", label: "Copilot" },
  { id: "openai_compat", label: "OpenAI-compatible" },
  { id: "local", label: "Local GGUF" },
];

interface RoutingProps {
  vaultSettings: VaultSettings | null;
  providerModels: Record<string, ProviderModel[]>;
  updateRoutingSlot: (
    slot: keyof VaultSettings["ai"]["routing"],
    next: RoutedModel | null,
  ) => void;
  caps: { local_llm: boolean };
}

function RoutingCard({
  slot,
  slotKey,
  current,
  providerModels,
  updateSlot,
  caps,
}: {
  slot: string;
  slotKey: keyof VaultSettings["ai"]["routing"];
  current: RoutedModel | null;
  providerModels: Record<string, ProviderModel[]>;
  updateSlot: (
    slot: keyof VaultSettings["ai"]["routing"],
    next: RoutedModel | null,
  ) => void;
  caps: { local_llm: boolean };
}) {
  const provider = current?.provider ?? "";
  const model = current?.model ?? "";
  const optionsForProvider = providerModels[provider] ?? [];

  return (
    <div
      style={{
        background: "var(--background-primary-alt)",
        border: "1px solid var(--background-modifier-border)",
        borderRadius: "var(--radius-m)",
        padding: 16,
      }}
    >
      <div
        style={{
          fontSize: "var(--font-ui-medium)",
          fontWeight: 600,
          color: "var(--text-normal)",
          marginBottom: 10,
        }}
      >
        {slot}
      </div>
      <div style={{ display: "flex", gap: 8, marginBottom: 6 }}>
        <select
          value={provider}
          onChange={(e) => {
            const p = e.target.value;
            if (!p) updateSlot(slotKey, null);
            else updateSlot(slotKey, { provider: p, model });
          }}
          style={selectStyle}
        >
          <option value="">(none)</option>
          {ROUTING_PROVIDERS.filter(
            (p) => caps.local_llm || p.id !== "local",
          ).map((p) => (
            <option key={p.id} value={p.id}>
              {p.label}
            </option>
          ))}
        </select>
        {optionsForProvider.length > 0 ? (
          <select
            value={model}
            onChange={(e) =>
              updateSlot(slotKey, { provider, model: e.target.value })
            }
            style={selectStyle}
          >
            <option value="">(pick model)</option>
            {!optionsForProvider.some((m) => m.id === model) && model && (
              <option value={model}>{model}</option>
            )}
            {optionsForProvider.map((m) => (
              <option key={m.id} value={m.id}>
                {m.display_name}
              </option>
            ))}
          </select>
        ) : (
          <InputField
            value={model}
            onChange={(v) => updateSlot(slotKey, { provider, model: v })}
            placeholder="model id"
            style={{ flex: 1 }}
          />
        )}
      </div>
    </div>
  );
}

function RoutingTab({
  vaultSettings,
  providerModels,
  updateRoutingSlot,
  caps,
}: RoutingProps) {
  if (!vaultSettings) {
    return (
      <div
        style={{
          padding: 20,
          textAlign: "center",
          color: "var(--text-faint)",
        }}
      >
        Open a vault to configure routing.
      </div>
    );
  }
  const r = vaultSettings.ai.routing;
  return (
    <div
      style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}
    >
      <RoutingCard
        slot="Chat"
        slotKey="chat"
        current={r.chat}
        providerModels={providerModels}
        updateSlot={updateRoutingSlot}
        caps={caps}
      />
      <RoutingCard
        slot="Fast"
        slotKey="fast"
        current={r.fast}
        providerModels={providerModels}
        updateSlot={updateRoutingSlot}
        caps={caps}
      />
      <RoutingCard
        slot="Summarise"
        slotKey="summarise"
        current={r.summarise}
        providerModels={providerModels}
        updateSlot={updateRoutingSlot}
        caps={caps}
      />
      <RoutingCard
        slot="Embed"
        slotKey="embed"
        current={r.embed}
        providerModels={providerModels}
        updateSlot={updateRoutingSlot}
        caps={caps}
      />
    </div>
  );
}

// ── Context tab ──
// Wireframe shown while settings + vault settings are loading. Matches
// the Providers panel layout (the default tab) so the modal doesn't
// jump on first paint. Bars pulse via a CSS animation defined inline
// so we don't need to touch index.css. Three card-shaped blocks +
// matching field rows mirror the real Anthropic / OpenAI / Gemini
// cards underneath.
function ProvidersSkeleton() {
  const Bar = ({ w, h = 14, mb = 0 }: { w: string; h?: number; mb?: number }) => (
    <div
      style={{
        width: w,
        height: h,
        marginBottom: mb,
        borderRadius: 4,
        background: "var(--background-modifier-border)",
        opacity: 0.55,
        animation: "forge-skel-pulse 1.2s ease-in-out infinite",
      }}
    />
  );
  const Card = () => (
    <div
      style={{
        background: "var(--background-primary-alt)",
        border: "1px solid var(--background-modifier-border)",
        borderRadius: "var(--radius-m)",
        padding: 16,
        marginBottom: 12,
      }}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: 14,
        }}
      >
        <Bar w="92px" h={16} />
        <Bar w="78px" h={11} />
      </div>
      <div
        style={{
          display: "flex",
          gap: 8,
          alignItems: "center",
          marginBottom: 10,
        }}
      >
        <Bar w="100px" h={11} />
        <div style={{ flex: 1 }}>
          <Bar w="100%" h={28} />
        </div>
        <Bar w="62px" h={28} />
      </div>
      <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
        <Bar w="100px" h={11} />
        <div style={{ flex: 1 }}>
          <Bar w="100%" h={28} />
        </div>
      </div>
    </div>
  );
  return (
    <>
      <style>{`
        @keyframes forge-skel-pulse {
          0%, 100% { opacity: 0.35; }
          50% { opacity: 0.7; }
        }
      `}</style>
      <Card />
      <Card />
      <Card />
    </>
  );
}

function PromptsTab({
  vaultSettings,
  updateVault,
}: {
  vaultSettings: VaultSettings | null;
  updateVault: (mutate: (vs: VaultSettings) => VaultSettings) => void;
}) {
  // Local mirror so the textarea is responsive — debounced commit to
  // disk runs through updateVault. Keep this in sync if vaultSettings
  // arrives or changes (e.g. user opens a different vault).
  const [draft, setDraft] = useState<string>(
    vaultSettings?.system_prompt ?? "",
  );
  useEffect(() => {
    setDraft(vaultSettings?.system_prompt ?? "");
  }, [vaultSettings?.system_prompt]);

  if (!vaultSettings) {
    return (
      <div style={{ padding: 20, color: "var(--text-faint)" }}>
        Open a vault to edit its system prompt.
      </div>
    );
  }

  const commit = (v: string) => {
    setDraft(v);
    updateVault((vs) => ({ ...vs, system_prompt: v }));
  };

  return (
    <div>
      <div style={{ marginBottom: 12 }}>
        <div
          style={{
            fontSize: "var(--font-ui-medium)",
            fontWeight: 600,
            color: "var(--text-normal)",
            marginBottom: 4,
          }}
        >
          System prompt
        </div>
        <div
          style={{
            fontSize: "var(--font-ui-small)",
            color: "var(--text-muted)",
            lineHeight: 1.5,
          }}
        >
          Prepended to every chat in this vault. Replaces Forge's default
          tool-using preamble — the agent still has access to its tools
          (write_file, search_vault, etc.) regardless of what you write
          here, so you can use this for domain rules, voice, output
          format, or pasting in the widget contract.
          {" "}
          <span style={{ color: "var(--text-faint)" }}>
            Leave empty to use the default.
          </span>
        </div>
      </div>

      <textarea
        value={draft}
        onChange={(e) => commit(e.target.value)}
        placeholder={`e.g.

You output interactive widgets as fenced markdown:

\`\`\`js-widget height=320
<div id="root"></div>
<script>
  const App = () => {
    const [a, setA] = useState(3);
    return html\`<\${Panel} title="Demo">...<//>\`;
  };
  render(html\`<\${App}/>\`, document.getElementById('root'));
</script>
\`\`\`

Globals: html, useState, useEffect, useRef, render, Slider, Readout, Panel, Grid, Plot.
Close composite tags with <//>, never </\${Panel}>.
For Chart.js add needs=chart.
When saving, call write_file with BOTH path and content.`}
        spellCheck={false}
        style={{
          width: "100%",
          minHeight: 320,
          padding: "10px 12px",
          background: "var(--background-modifier-form-field)",
          border: "1px solid var(--background-modifier-border)",
          borderRadius: "var(--radius-s)",
          color: "var(--text-normal)",
          fontFamily: "var(--font-monospace)",
          fontSize: 12.5,
          lineHeight: 1.5,
          resize: "vertical",
          outline: "none",
        }}
        onFocus={(e) => {
          e.currentTarget.style.borderColor =
            "var(--background-modifier-border-focus)";
        }}
        onBlur={(e) => {
          e.currentTarget.style.borderColor =
            "var(--background-modifier-border)";
        }}
      />

      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginTop: 8,
          fontSize: "var(--font-ui-smaller)",
          color: "var(--text-faint)",
        }}
      >
        <span>
          {draft.length === 0
            ? "Using built-in default."
            : `${draft.length} chars · saved per-vault`}
        </span>
        {draft.length > 0 && (
          <button
            type="button"
            onClick={() => commit("")}
            style={{
              background: "transparent",
              border: "none",
              color: "var(--text-muted)",
              cursor: "pointer",
              fontSize: "var(--font-ui-smaller)",
              padding: "2px 6px",
              borderRadius: "var(--radius-s)",
            }}
            title="Clear and use the built-in default"
          >
            Reset to default
          </button>
        )}
      </div>
    </div>
  );
}

function ContextTab() {
  const [threshold, setThreshold] = useState(80); // TODO: persist
  const [blockSize, setBlockSize] = useState("8"); // TODO: persist
  return (
    <>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          minHeight: 40,
          padding: "8px 0",
        }}
      >
        <div>
          <div
            style={{ fontSize: "var(--font-ui-medium)", fontWeight: 500 }}
          >
            Compaction threshold
          </div>
          <div
            style={{
              fontSize: "var(--font-ui-small)",
              color: "var(--text-muted)",
              marginTop: 2,
            }}
          >
            Percentage of context window before compacting
          </div>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <input
            type="range"
            min={50}
            max={95}
            value={threshold}
            onChange={(e) => setThreshold(Number(e.target.value))}
            style={{ width: 120, accentColor: "var(--interactive-accent)" }}
          />
          <span
            style={{
              fontSize: "var(--font-ui-small)",
              color: "var(--text-muted)",
            }}
          >
            {threshold}%
          </span>
        </div>
      </div>
      <Divider />
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          minHeight: 40,
          padding: "8px 0",
        }}
      >
        <div>
          <div
            style={{ fontSize: "var(--font-ui-medium)", fontWeight: 500 }}
          >
            Summary block size
          </div>
          <div
            style={{
              fontSize: "var(--font-ui-small)",
              color: "var(--text-muted)",
              marginTop: 2,
            }}
          >
            Number of turns per summary block
          </div>
        </div>
        <InputField
          type="number"
          value={blockSize}
          onChange={setBlockSize}
          style={{ width: 80 }}
        />
      </div>
    </>
  );
}

// ── Tools tab ──
interface ToolDef {
  name: string;
  desc: string;
  safe: boolean;
  defaultOn: boolean;
}

const TOOL_CATALOG: ToolDef[] = [
  { name: "hybrid_search", desc: "Search vault by content + embeddings", safe: true, defaultOn: true },
  { name: "read_note", desc: "Read a note's full content", safe: true, defaultOn: true },
  { name: "edit_note", desc: "Edit or append to a note", safe: true, defaultOn: true },
  { name: "create_note", desc: "Create a new note", safe: true, defaultOn: true },
  { name: "list_files", desc: "List files in a directory", safe: true, defaultOn: true },
  { name: "shell_exec", desc: "Execute a shell command", safe: false, defaultOn: false },
  { name: "web_search", desc: "Search the web", safe: false, defaultOn: false },
  { name: "web_fetch", desc: "Fetch a URL", safe: false, defaultOn: false },
];

function ToolsTab() {
  const [enabled, setEnabled] = useState<Record<string, boolean>>(() => {
    const o: Record<string, boolean> = {};
    for (const t of TOOL_CATALOG) o[t.name] = t.defaultOn;
    return o;
  });
  // TODO: persist — extend Settings with enabled_tools set
  const setAll = (fn: (t: ToolDef) => boolean) => {
    const o: Record<string, boolean> = {};
    for (const t of TOOL_CATALOG) o[t.name] = fn(t);
    setEnabled(o);
  };
  return (
    <>
      <div style={{ display: "flex", gap: 8, marginBottom: 12 }}>
        <SecondaryBtn onClick={() => setAll((t) => t.safe)}>
          Enable safe-only
        </SecondaryBtn>
        <SecondaryBtn onClick={() => setAll(() => true)}>
          Enable all
        </SecondaryBtn>
        <SecondaryBtn onClick={() => setAll(() => false)}>
          Disable all
        </SecondaryBtn>
      </div>
      <div
        style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 1 }}
      >
        {TOOL_CATALOG.map((t) => (
          <div
            key={t.name}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 10,
              padding: "8px 10px",
              borderBottom: "1px solid var(--hr-color)",
            }}
          >
            <Toggle
              on={!!enabled[t.name]}
              onChange={(v) =>
                setEnabled((prev) => ({ ...prev, [t.name]: v }))
              }
            />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div
                style={{
                  fontSize: "var(--font-ui-medium)",
                  fontWeight: 500,
                  fontFamily: "var(--font-monospace)",
                  color: "var(--text-normal)",
                }}
              >
                {t.name}
              </div>
              <div
                style={{
                  fontSize: "var(--font-ui-small)",
                  color: "var(--text-muted)",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {t.desc}
              </div>
            </div>
          </div>
        ))}
      </div>
    </>
  );
}

// ── Voice tab (SST + TTS) ──
interface VoiceProps {
  settings: Settings;
  update: <K extends keyof Settings>(k: K, v: Settings[K]) => void;
  models: ModelInfo[];
  progress: Record<string, DownloadProgress>;
  binStatus: BinaryStatus;
  binInstall: Record<string, BinaryInstallEvent>;
  refreshModels: () => void;
}

function VoiceTab(props: VoiceProps) {
  const {
    settings,
    update,
    models,
    progress,
    binStatus,
    binInstall,
    refreshModels,
  } = props;

  const sttModels = models.filter((m) => m.kind === "stt");
  const ttsModels = models.filter((m) => m.kind === "tts");
  const downloadedStt = sttModels.filter((m) => m.downloaded);
  const downloadedTts = ttsModels.filter((m) => m.downloaded);

  return (
    <>
      <ProviderCard
        name="Speech to text"
        status={binStatus.whisper_cli ? "connected" : "idle"}
        statusLabel={
          binStatus.whisper_cli
            ? "whisper-cli installed"
            : "whisper-cli missing"
        }
      >
        <FieldRow label="Provider">
          <select
            value={
              settings.stt_provider === "deepgram"
                ? "local"
                : settings.stt_provider
            }
            onChange={(e) => update("stt_provider", e.target.value)}
            style={selectStyle}
          >
            <option value="local">Local (Whisper)</option>
          </select>
        </FieldRow>
        {(settings.stt_provider === "local" ||
          settings.stt_provider === "deepgram") && (
          <>
            <FieldRow label="Model">
              {downloadedStt.length === 0 ? (
                <span
                  style={{
                    flex: 1,
                    fontSize: "var(--font-ui-small)",
                    color: "var(--text-faint)",
                    fontStyle: "italic",
                  }}
                >
                  No whisper models downloaded — use the catalogue.
                </span>
              ) : (
                <select
                  value={settings.whisper_model_path ?? ""}
                  onChange={(e) =>
                    update("whisper_model_path", e.target.value || null)
                  }
                  style={selectStyle}
                >
                  <option value="" disabled>
                    Select…
                  </option>
                  {downloadedStt.map((m) => (
                    <option key={m.id} value={m.local_path ?? ""}>
                      {m.name}
                    </option>
                  ))}
                </select>
              )}
            </FieldRow>
            <BinaryRow
              id="whisper-cli"
              label="whisper-cli"
              description="Required for local STT. Built from source."
              path={binStatus.whisper_cli}
              install={binInstall["whisper-cli"]}
              onInstall={() => installWhisperCpp()}
              onCancel={() => cancelBinaryInstall("whisper-cli")}
            />
            <Divider style={{ margin: "12px 0" }} />
            <DownloadList
              entries={sttModels}
              progress={progress}
              onDownload={(id) => startModelDownload(id)}
              onCancel={(id) => cancelModelDownload(id)}
              onDelete={async (id) => {
                await deleteModel(id);
                refreshModels();
                const m = sttModels.find((x) => x.id === id);
                if (m && settings.whisper_model_path === m.local_path) {
                  update("whisper_model_path", null);
                }
              }}
            />
          </>
        )}
      </ProviderCard>

      <ProviderCard
        name="Text to speech"
        status={settings.tts_provider ? "connected" : "idle"}
        statusLabel={settings.tts_provider}
      >
        <FieldRow label="Provider">
          <select
            value={
              settings.tts_provider === "deepgram"
                ? "edge"
                : settings.tts_provider
            }
            onChange={(e) => update("tts_provider", e.target.value)}
            style={selectStyle}
          >
            <option value="edge">Edge TTS</option>
            <option value="gtts">Google Translate TTS</option>
            <option value="local">Local (Piper)</option>
          </select>
        </FieldRow>
        {settings.tts_provider === "edge" && (
          <FieldRow label="Voice">
            <InputField
              value={settings.edge_tts_voice}
              onChange={(v) => update("edge_tts_voice", v)}
              placeholder="en-US-JennyNeural"
              style={{ flex: 1 }}
            />
          </FieldRow>
        )}
        {settings.tts_provider === "gtts" && (
          <FieldRow label="Language">
            <InputField
              value={settings.gtts_lang}
              onChange={(v) => update("gtts_lang", v)}
              placeholder="en"
              style={{ flex: 1 }}
            />
          </FieldRow>
        )}
        {settings.tts_provider === "local" && (
          <>
            <FieldRow label="Voice">
              {downloadedTts.length === 0 ? (
                <span
                  style={{
                    flex: 1,
                    fontSize: "var(--font-ui-small)",
                    color: "var(--text-faint)",
                    fontStyle: "italic",
                  }}
                >
                  No piper voices downloaded — use the catalogue.
                </span>
              ) : (
                <select
                  value={settings.piper_voice_path ?? ""}
                  onChange={(e) =>
                    update("piper_voice_path", e.target.value || null)
                  }
                  style={selectStyle}
                >
                  <option value="" disabled>
                    Select…
                  </option>
                  {downloadedTts.map((m) => (
                    <option key={m.id} value={m.local_path ?? ""}>
                      {m.name}
                    </option>
                  ))}
                </select>
              )}
            </FieldRow>
            <BinaryRow
              id="piper"
              label="piper"
              description="Required for local TTS. Prebuilt binary."
              path={binStatus.piper}
              install={binInstall["piper"]}
              onInstall={() => installPiper()}
              onCancel={() => cancelBinaryInstall("piper")}
            />
            <Divider style={{ margin: "12px 0" }} />
            <DownloadList
              entries={ttsModels}
              progress={progress}
              onDownload={(id) => startModelDownload(id)}
              onCancel={(id) => cancelModelDownload(id)}
              onDelete={async (id) => {
                await deleteModel(id);
                refreshModels();
                const m = ttsModels.find((x) => x.id === id);
                if (m && settings.piper_voice_path === m.local_path) {
                  update("piper_voice_path", null);
                }
              }}
            />
          </>
        )}
      </ProviderCard>
    </>
  );
}

function Placeholder({ label }: { label: string }) {
  return (
    <div
      style={{
        padding: 20,
        textAlign: "center",
        color: "var(--text-faint)",
      }}
    >
      {label} settings
    </div>
  );
}

// ── Shared: model download row + binary install row ──
function DownloadList({
  entries,
  progress,
  onDownload,
  onCancel,
  onDelete,
}: {
  entries: ModelInfo[];
  progress: Record<string, DownloadProgress>;
  onDownload: (id: string) => Promise<void> | void;
  onCancel: (id: string) => Promise<boolean> | boolean;
  onDelete: (id: string) => Promise<void> | void;
}) {
  if (entries.length === 0) {
    return (
      <div
        style={{
          fontSize: "var(--font-ui-smaller)",
          color: "var(--text-faint)",
          fontStyle: "italic",
        }}
      >
        No downloadable options.
      </div>
    );
  }
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      {entries.map((m) => {
        const prog = progress[m.id];
        const active =
          prog && !["done", "cancelled", "error"].includes(prog.phase);
        return (
          <div
            key={m.id}
            style={{
              padding: 10,
              borderRadius: "var(--radius-s)",
              border: "1px solid var(--background-modifier-border)",
              background: "var(--background-primary)",
            }}
          >
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "flex-start",
                gap: 12,
              }}
            >
              <div style={{ minWidth: 0, flex: 1 }}>
                <div
                  style={{
                    fontSize: "var(--font-ui-medium)",
                    fontWeight: 500,
                    color: "var(--text-normal)",
                  }}
                >
                  {m.name}
                </div>
                <div
                  style={{
                    fontSize: "var(--font-ui-small)",
                    color: "var(--text-muted)",
                    marginTop: 2,
                  }}
                >
                  {m.description}
                </div>
                <div
                  style={{
                    fontSize: "var(--font-ui-smaller)",
                    color: "var(--text-faint)",
                    marginTop: 2,
                  }}
                >
                  {formatBytes(m.size_bytes)} · id:{" "}
                  <code>{m.id}</code>
                </div>
              </div>
              <div style={{ display: "flex", gap: 8, flexShrink: 0 }}>
                {m.downloaded && !active && (
                  <SecondaryBtn onClick={() => void onDelete(m.id)}>
                    Delete
                  </SecondaryBtn>
                )}
                {active ? (
                  <SecondaryBtn onClick={() => void onCancel(m.id)}>
                    Cancel
                  </SecondaryBtn>
                ) : !m.downloaded ? (
                  <PrimaryBtn onClick={() => void onDownload(m.id)}>
                    Download
                  </PrimaryBtn>
                ) : (
                  <span
                    style={{
                      fontSize: "var(--font-ui-smaller)",
                      color: "var(--interactive-accent)",
                      alignSelf: "center",
                    }}
                  >
                    Installed
                  </span>
                )}
              </div>
            </div>
            {active && prog && (
              <div style={{ marginTop: 8 }}>
                <ProgressBar
                  pct={
                    prog.total > 0
                      ? Math.min(100, (prog.downloaded / prog.total) * 100)
                      : 30
                  }
                />
                <div
                  style={{
                    display: "flex",
                    justifyContent: "space-between",
                    fontSize: "var(--font-ui-smaller)",
                    color: "var(--text-muted)",
                    marginTop: 4,
                  }}
                >
                  <span>{prog.phase}</span>
                  <span>
                    {formatBytes(prog.downloaded)} /{" "}
                    {formatBytes(prog.total || m.size_bytes)}
                  </span>
                </div>
              </div>
            )}
            {prog?.phase === "error" && prog.error && (
              <div
                style={{
                  fontSize: "var(--font-ui-smaller)",
                  color: "var(--text-error)",
                  marginTop: 4,
                }}
              >
                Error: {prog.error}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

function BinaryRow({
  id: _id,
  label,
  description,
  path,
  install,
  onInstall,
  onCancel,
}: {
  id: string;
  label: string;
  description: string;
  path: string | null;
  install: BinaryInstallEvent | undefined;
  onInstall: () => Promise<void> | void;
  onCancel: () => Promise<boolean> | boolean;
}) {
  const installed = !!path;
  const active = install && !["done", "error"].includes(install.phase);
  return (
    <div
      style={{
        padding: 10,
        marginTop: 8,
        borderRadius: "var(--radius-s)",
        border: "1px solid var(--background-modifier-border)",
        background: "var(--background-primary)",
      }}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "flex-start",
          gap: 12,
        }}
      >
        <div style={{ minWidth: 0, flex: 1 }}>
          <div
            style={{
              fontSize: "var(--font-ui-medium)",
              fontWeight: 500,
              color: "var(--text-normal)",
            }}
          >
            {label}
          </div>
          <div
            style={{
              fontSize: "var(--font-ui-small)",
              color: "var(--text-muted)",
              marginTop: 2,
            }}
          >
            {description}
          </div>
          <div
            style={{
              fontSize: "var(--font-ui-smaller)",
              color: "var(--text-faint)",
              marginTop: 2,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
            title={path ?? ""}
          >
            {installed ? `Found at ${path}` : "Not installed"}
          </div>
        </div>
        <div style={{ display: "flex", gap: 8, flexShrink: 0 }}>
          {active ? (
            <SecondaryBtn onClick={() => void onCancel()}>Cancel</SecondaryBtn>
          ) : installed ? (
            <SecondaryBtn onClick={() => void onInstall()}>
              Reinstall
            </SecondaryBtn>
          ) : (
            <PrimaryBtn onClick={() => void onInstall()}>Install</PrimaryBtn>
          )}
        </div>
      </div>
      {install && (
        <div style={{ marginTop: 8 }}>
          <ProgressBar
            pct={
              install.progress != null
                ? Math.min(100, install.progress * 100)
                : active
                  ? 40
                  : 100
            }
          />
          <div
            style={{
              marginTop: 4,
              fontSize: "var(--font-ui-smaller)",
              color:
                install.phase === "error"
                  ? "var(--text-error)"
                  : "var(--text-muted)",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {install.phase}: {install.detail}
          </div>
        </div>
      )}
    </div>
  );
}

function ProgressBar({ pct }: { pct: number }) {
  return (
    <div
      style={{
        height: 6,
        borderRadius: 3,
        background: "var(--background-modifier-border)",
        overflow: "hidden",
      }}
    >
      <div
        style={{
          height: "100%",
          width: `${pct}%`,
          background: "var(--interactive-accent)",
          transition: "width 0.2s ease",
        }}
      />
    </div>
  );
}

function formatBytes(n: number): string {
  if (n <= 0) return "-";
  const units = ["B", "KB", "MB", "GB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}
