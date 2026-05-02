// Typed wrappers around the Tauri invoke API so the rest of the app
// does not sprinkle `invoke("...")` calls throughout.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface VaultEntry {
  name: string;
  path: string;
  is_dir: boolean;
}

export interface TreeNode {
  name: string;
  path: string;
  is_dir: boolean;
  children: TreeNode[];
}

export interface Settings {
  last_vault_path: string | null;
  theme: string;
  open_tabs: string[];
  active_tab: number | null;
  body_font: string;
  interface_font: string;
  mono_font: string;
  font_size: number;
  sidebar_width: number;
  chat_panel_width: number;
  max_tool_iterations: number;
  ai_provider: string;
  api_key: string | null;
  api_model: string;
  whisper_model_path: string | null;
  whisper_language: string;
  piper_bin_path: string | null;
  piper_voice_path: string | null;
  wake_word: string;
  copilot_model: string;
  stt_provider: string;
  tts_provider: string;
  deepgram_api_key: string | null;
  deepgram_stt_model: string;
  deepgram_tts_voice: string;
  edge_tts_voice: string;
  gtts_lang: string;
}

export interface ChatTurn {
  role: "user" | "assistant";
  content: string;
}

export interface ConnectResult {
  model_name: string;
}

// ── Settings ────────────────────────────────────────────────────────────

// New two-scope settings (Phase 1). Legacy `getSettings`/`setSettings` are
// kept below as adapters so existing call sites keep working while screens
// migrate.

export interface AppSettings {
  last_opened_vault: string | null;
}

export interface ProviderConfig {
  api_key: string | null;
  base_url: string | null;
  default_model: string | null;
}

export interface RoutedModel {
  provider: string;
  model: string;
}

export interface RoutingConfig {
  chat: RoutedModel | null;
  fast: RoutedModel | null;
  summarise: RoutedModel | null;
  embed: RoutedModel | null;
}

export interface AiSettings {
  default_provider: string;
  providers: Record<string, ProviderConfig>;
  routing: RoutingConfig;
}

export interface VoiceSettings {
  stt_provider: "whisper" | "none" | string;
  whisper_model: string;
  tts_voice: string;
}

export interface VaultSettings {
  theme: "light" | "dark" | string;
  sidebar_width: number;
  chat_panel_width: number;
  recent_files: string[];
  ai: AiSettings;
  voice: VoiceSettings;
  system_prompt: string;
  tools_allowed: string[];
}

export const getAppSettings = () =>
  invoke<AppSettings>("get_app_settings");
export const setAppSettings = (settings: AppSettings) =>
  invoke<void>("set_app_settings", { settings });

export const getVaultSettings = (vaultPath: string) =>
  invoke<VaultSettings>("get_vault_settings", { vaultPath });
export const setVaultSettings = (
  vaultPath: string,
  settings: VaultSettings,
) => invoke<void>("set_vault_settings", { vaultPath, settings });
export const migrateVaultSettings = (vaultPath: string) =>
  invoke<boolean>("migrate_vault_settings", { vaultPath });

// ── Chats (markdown-on-disk) ────────────────────────────────────────────

export type ChatRole = "user" | "assistant" | "tool";

export interface ChatMarkdownTurn {
  role: ChatRole;
  timestamp: string;
  body: string;
}

export interface ChatHeader {
  forge_chat: number;
  created: string;
  updated: string;
  model: string | null;
  provider: string | null;
  system_prompt: string | null;
  tools_allowed: string[];
}

export interface ChatFile {
  id: string;
  path: string;
  header: ChatHeader;
  turns: ChatMarkdownTurn[];
}

export interface ChatSummary {
  id: string;
  path: string;
  title: string;
  created: string;
  updated: string;
  model: string | null;
  turn_count: number;
}

export interface SaveChatPayload {
  vault_path: string;
  chat_id: string | null;
  header: ChatHeader;
  turns: ChatMarkdownTurn[];
}

export const saveChat = (payload: SaveChatPayload) =>
  invoke<ChatSummary>("save_chat", { payload });
export const loadChat = (vaultPath: string, chatId: string) =>
  invoke<ChatFile>("load_chat", { vaultPath, chatId });
export const listChats = (vaultPath: string) =>
  invoke<ChatSummary[]>("list_chats", { vaultPath });
export const deleteChat = (vaultPath: string, chatId: string) =>
  invoke<void>("delete_chat", { vaultPath, chatId });
export const exportChatAsNote = (
  vaultPath: string,
  chatId: string,
  destRelpath: string,
) =>
  invoke<string>("export_chat_as_note", {
    vaultPath,
    chatId,
    destRelpath,
  });

/** @deprecated Use getVaultSettings / getAppSettings instead. */
export const getSettings = () => invoke<Settings>("get_settings");
/** @deprecated Use setVaultSettings / setAppSettings instead. */
export const setSettings = (settings: Settings) =>
  invoke<void>("set_settings", { new: settings });

// ── Vault ───────────────────────────────────────────────────────────────

export const currentVault = () => invoke<string | null>("current_vault");
export const openVault = (path: string) =>
  invoke<VaultEntry[]>("open_vault", { path });
export const listVaultFiles = (subPath?: string) =>
  invoke<VaultEntry[]>("list_vault_files", { subPath: subPath ?? null });
export const listVaultTree = () => invoke<TreeNode>("list_vault_tree");

// ── Files ───────────────────────────────────────────────────────────────

export const readFile = (path: string) =>
  invoke<string>("read_file", { path });
export const writeFile = (path: string, content: string) =>
  invoke<void>("write_file", { path, content });
export const renameFile = (from: string, to: string) =>
  invoke<void>("rename_file", { from, to });
export const deleteFile = (path: string) =>
  invoke<void>("delete_file", { path });

// ── Inference / chat ────────────────────────────────────────────────────

export const connectInference = () =>
  invoke<ConnectResult>("connect_inference");
export const sendChatMessage = (history: ChatTurn[]) =>
  invoke<void>("send_chat_message", { history });
export const stopChat = () => invoke<void>("stop_chat");

// ── Chat event subscriptions ────────────────────────────────────────────

export interface ToolStartPayload {
  name: string;
  args: string;
}
export interface ToolResultPayload {
  name: string;
  content: string;
  is_error: boolean;
}

export const onChatToken = (handler: (text: string) => void): Promise<UnlistenFn> =>
  listen<string>("chat://token", (event) => handler(event.payload));
export const onChatThinking = (handler: (text: string) => void): Promise<UnlistenFn> =>
  listen<string>("chat://thinking", (event) => handler(event.payload));
export const onChatToolStart = (
  handler: (payload: ToolStartPayload) => void,
): Promise<UnlistenFn> =>
  listen<ToolStartPayload>("chat://tool-start", (event) => handler(event.payload));
export const onChatToolResult = (
  handler: (payload: ToolResultPayload) => void,
): Promise<UnlistenFn> =>
  listen<ToolResultPayload>("chat://tool-result", (event) => handler(event.payload));
export const onChatDone = (handler: () => void): Promise<UnlistenFn> =>
  listen<void>("chat://done", () => handler());
export const onChatError = (handler: (message: string) => void): Promise<UnlistenFn> =>
  listen<string>("chat://error", (event) => handler(event.payload));

export const onVaultChanged = (handler: () => void): Promise<UnlistenFn> =>
  listen<void>("vault://changed", () => handler());

// ── Search ──────────────────────────────────────────────────────────────

export interface SearchHit {
  path: string;
  title: string;
  heading: string;
  snippet: string;
  score: number;
  /** 1-based line number where the chunk begins. */
  line_start: number;
  /** Lowercased query terms (or the literal needle for quoted queries)
   * that the renderer should highlight. Empty for vector-only hits. */
  matched_terms: string[];
  /** "keyword" → BM25/FTS hit, "vector" → semantic-only neighbour,
   * "literal" → quoted substring match. */
  source: "keyword" | "vector" | "literal";
}

export interface SearchStatus {
  indexed: boolean;
  chunk_count: number;
  vectors_available: boolean;
}

export const searchVault = (query: string, limit?: number) =>
  invoke<SearchHit[]>("search_vault", { query, limit: limit ?? null });
export const reindexVault = () => invoke<SearchStatus>("reindex_vault");
export const searchStatus = () => invoke<SearchStatus>("search_status");

// ── Speech-to-text ──────────────────────────────────────────────────────

export const transcribeAudio = (wavBytes: Uint8Array) =>
  invoke<string>("transcribe_audio", { wavBytes: Array.from(wavBytes) });
export const startRecording = () => invoke<void>("start_recording");
export const stopRecordingAndTranscribe = () =>
  invoke<string>("stop_recording_and_transcribe");

// ── Voice conversation mode ─────────────────────────────────────────────

export const voiceStart = () => invoke<void>("voice_start");
export const voiceStartWake = () => invoke<void>("voice_start_wake");
export const voiceStop = () => invoke<void>("voice_stop");
export const voiceInterrupt = () => invoke<void>("voice_interrupt");
export const voiceSetMuted = (muted: boolean) =>
  invoke<void>("voice_set_muted", { muted });

export const onVoiceState = (handler: (state: string) => void): Promise<UnlistenFn> =>
  listen<string>("voice://state", (e) => handler(e.payload));
export const onVoiceTranscript = (handler: (text: string) => void): Promise<UnlistenFn> =>
  listen<string>("voice://transcript", (e) => handler(e.payload));
export const onVoiceAssistantText = (handler: (text: string) => void): Promise<UnlistenFn> =>
  listen<string>("voice://assistant-text", (e) => handler(e.payload));
export const onVoiceTtsChunk = (handler: (b64: string) => void): Promise<UnlistenFn> =>
  listen<string>("voice://tts-chunk", (e) => handler(e.payload));
export const onVoiceError = (handler: (msg: string) => void): Promise<UnlistenFn> =>
  listen<string>("voice://error", (e) => handler(e.payload));
export const onVoiceStopped = (handler: () => void): Promise<UnlistenFn> =>
  listen<void>("voice://stopped", () => handler());
export const onVoiceBargeIn = (handler: () => void): Promise<UnlistenFn> =>
  listen<void>("voice://barge-in", () => handler());

// ── LaTeX ───────────────────────────────────────────────────────────────

export interface LatexCompileResult {
  pdf_path: string;
  log: string;
  engine: string;
}

export interface LatexStatus {
  tectonic: boolean;
  xelatex: boolean;
  pdflatex: boolean;
}

export const compileLatex = (path: string) =>
  invoke<LatexCompileResult>("compile_latex", { path });
export const latexStatus = () => invoke<LatexStatus>("latex_status");
export const openInTextEditor = (path: string) =>
  invoke<void>("open_in_text_editor", { path });

// ── Copilot auth ────────────────────────────────────────────────────────

export interface CopilotStatus {
  signed_in: boolean;
  login: string | null;
}

export interface DeviceCode {
  user_code: string;
  verification_uri: string;
  device_code: string;
  interval: number;
  expires_in: number;
}

export type CopilotPollResult =
  | { status: "pending" }
  | { status: "slow_down" }
  | { status: "ok"; login: string | null }
  | { status: "denied" }
  | { status: "expired" }
  | { status: "other"; message: string }
  | { status: "no_code" };

export interface CopilotModel {
  id: string;
  name: string;
  vendor: string;
}

export const copilotStatus = () => invoke<CopilotStatus>("copilot_status");
export const copilotLoginStart = () =>
  invoke<DeviceCode>("copilot_login_start");
export const copilotLoginPoll = () =>
  invoke<CopilotPollResult>("copilot_login_poll");
export const copilotLogout = () => invoke<void>("copilot_logout");
export const copilotModels = () => invoke<CopilotModel[]>("copilot_models");

// ── Models catalog + downloads ──────────────────────────────────────────

export type ModelKind = "llm" | "stt" | "tts";

export interface ModelInfo {
  id: string;
  kind: ModelKind;
  name: string;
  description: string;
  size_bytes: number;
  url: string;
  filename: string;
  local_path: string | null;
  downloaded: boolean;
  on_disk_bytes: number | null;
}

export interface DownloadProgress {
  id: string;
  downloaded: number;
  total: number;
  phase: string; // "primary" | "config" | "done" | "cancelled" | "error"
  error: string | null;
}

export interface GpuStatus {
  cuda_available: boolean;
  details: string;
}

// ── Build capabilities ────────────────────────────────────────────────

export interface RuntimeCapabilities {
  /** Permanently false now that Forge has no embedded inference runtime.
   *  Kept on the type so downstream code that probes capabilities still
   *  type-checks; treat as a constant in any new code. Local models
   *  run via Ollama through the openai_compat provider. */
  local_llm: boolean;
}

let _capsPromise: Promise<RuntimeCapabilities> | null = null;
/** Cached lazy fetch of build features. Call freely; runs the IPC at most
 *  once per page load. */
export const runtimeCapabilities = (): Promise<RuntimeCapabilities> => {
  if (!_capsPromise) {
    _capsPromise = invoke<RuntimeCapabilities>("runtime_capabilities").catch(
      // Older backends without the command: assume no local LLM.
      () => ({ local_llm: false }),
    );
  }
  return _capsPromise;
};

export const listModels = () => invoke<ModelInfo[]>("list_models");
export const startModelDownload = (id: string) =>
  invoke<void>("start_model_download", { id });
export const cancelModelDownload = (id: string) =>
  invoke<boolean>("cancel_model_download", { id });
export const deleteModel = (id: string) =>
  invoke<boolean>("delete_model", { id });
export const detectGpu = () => invoke<GpuStatus>("detect_gpu");
export const onModelDownloadProgress = (
  handler: (p: DownloadProgress) => void,
): Promise<UnlistenFn> =>
  listen<DownloadProgress>("model://download-progress", (e) =>
    handler(e.payload),
  );

// ── External binaries (whisper-cli, piper) ──────────────────────────────

export interface BinaryStatus {
  whisper_cli: string | null;
  piper: string | null;
}

export interface BinaryInstallEvent {
  id: string; // "whisper-cli" | "piper"
  phase: string; // cloning | building | downloading | extracting | done | error
  detail: string;
  progress: number | null;
}

export const binaryStatus = () => invoke<BinaryStatus>("binary_status");
export const installWhisperCpp = () => invoke<void>("install_whisper_cpp");
export const installPiper = () => invoke<void>("install_piper");
export const cancelBinaryInstall = (id: string) =>
  invoke<boolean>("cancel_binary_install", { id });
export const onBinaryInstall = (
  handler: (e: BinaryInstallEvent) => void,
): Promise<UnlistenFn> =>
  listen<BinaryInstallEvent>("binary://install", (e) => handler(e.payload));

// ── Links / backlinks / graph ───────────────────────────────────────────

export interface LinkHit {
  path: string;
  name: string;
  snippet: string;
}

export interface GraphNode {
  id: string;
  name: string;
  degree: number;
}

export interface GraphEdge {
  source: string;
  target: string;
}

export interface LinkGraph {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export const listBacklinks = (target: string) =>
  invoke<LinkHit[]>("list_backlinks", { target });
export const linkGraph = () => invoke<LinkGraph>("link_graph");

// ── AI providers (Phase 3) ─────────────────────────────────────────────

export interface ProviderCapabilities {
  context_window: number;
  max_output: number;
  tokenizer_kind: string;
  supports_caching: boolean;
  supports_tools: boolean;
  supports_vision: boolean;
}

export interface ProviderModel {
  id: string;
  display_name: string;
  capabilities: ProviderCapabilities;
}

export interface ProviderTestResult {
  ok: boolean;
  error: string | null;
  models: ProviderModel[];
}

export const testAnthropic = (
  apiKey: string,
  baseUrl?: string,
): Promise<ProviderTestResult> =>
  invoke<ProviderTestResult>("test_anthropic", {
    apiKey,
    baseUrl: baseUrl ?? null,
  });
export const testOpenai = (
  apiKey: string,
  baseUrl?: string,
): Promise<ProviderTestResult> =>
  invoke<ProviderTestResult>("test_openai", {
    apiKey,
    baseUrl: baseUrl ?? null,
  });
export const testGemini = (apiKey: string): Promise<ProviderTestResult> =>
  invoke<ProviderTestResult>("test_gemini", { apiKey });
export const testCopilot = (): Promise<ProviderTestResult> =>
  invoke<ProviderTestResult>("test_copilot");
export const testOpenaiCompat = (
  apiKey: string,
  baseUrl: string,
): Promise<ProviderTestResult> =>
  invoke<ProviderTestResult>("test_openai_compat", { apiKey, baseUrl });
export const listProviderModels = (
  provider: string,
  apiKey?: string,
  baseUrl?: string,
): Promise<ProviderModel[]> =>
  invoke<ProviderModel[]>("list_provider_models", {
    provider,
    apiKey: apiKey ?? null,
    baseUrl: baseUrl ?? null,
  });

// ── Terminal ────────────────────────────────────────────────────────────

export type TerminalOutputEvent = { id: number; bytes_b64: string };

export const spawnTerminal = (vaultPath: string | null) =>
  invoke<number>("spawn_terminal", { vaultPath });
export const writeTerminal = (id: number, data: string) =>
  invoke<void>("write_terminal", { id, data });
export const resizeTerminal = (id: number, cols: number, rows: number) =>
  invoke<void>("resize_terminal", { id, cols, rows });
export const killTerminal = (id: number) =>
  invoke<void>("kill_terminal", { id });
export const listTerminals = () => invoke<number[]>("list_terminals");

export const onTerminalOutput = (
  handler: (e: TerminalOutputEvent) => void,
): Promise<UnlistenFn> =>
  listen<TerminalOutputEvent>("terminal://output", (ev) => handler(ev.payload));
