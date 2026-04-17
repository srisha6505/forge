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
  model_path: string | null;
  gpu_layers: number;
  ctx_size: number;
  chat_panel_width: number;
  max_tool_iterations: number;
  ai_provider: string;
  api_key: string | null;
  api_model: string;
  whisper_model_path: string | null;
  whisper_language: string;
}

export interface ChatTurn {
  role: "user" | "assistant";
  content: string;
}

export interface ConnectResult {
  model_name: string;
}

// ── Settings ────────────────────────────────────────────────────────────

export const getSettings = () => invoke<Settings>("get_settings");
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
