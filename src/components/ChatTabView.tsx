import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import type { CSSProperties } from "react";
import { Chip, GhostBtn } from "./ui";
import { Send, Sparkles } from "./ui/Icons";
import { MessageBlock, type UiMessage } from "./chat/MessageBlock";
import { RunningIndicator } from "./chat/RunningIndicator";
import { ChatModelPicker, type PickerModel } from "./chat/ChatModelPicker";
import { VoiceInput } from "./VoiceInput";
import TOCPanel, { type Heading } from "./TOCPanel";
import {
  connectInference,
  copilotModels,
  getSettings,
  getVaultSettings,
  listProviderModels,
  loadChat,
  onChatDone,
  onChatError,
  onChatToken,
  onChatToolResult,
  onChatToolStart,
  saveChat,
  sendChatMessage,
  setSettings,
  setVaultSettings,
  stopChat,
  writeFile,
  type ChatHeader,
  type ChatMarkdownTurn,
  type ChatTurn,
} from "../lib/tauri";

// Map provider id → vendor label shown in the picker header.
const VENDOR_LABEL: Record<string, string> = {
  openai: "OpenAI",
  anthropic: "Anthropic",
  gemini: "Google",
  openai_compat: "Local",
  copilot: "Copilot",
  local: "Local",
};

const SAVE_DEBOUNCE_MS = 800;

interface Props {
  vaultPath: string | null;
  chatId: string | null;
  onOpenAiSettings?: () => void;
  onOpenFile?: (path: string) => void;
  onChatPersisted?: (chatId: string) => void;
  // Mirror the .md tab affordances so chat-as-tab doesn't feel
  // second-class: TOC panel listing user prompts, optional readable
  // width clamp, and font zoom driven by the same Ctrl+= / Ctrl+-
  // chord App.tsx exposes for markdown.
  tocOpen?: boolean;
  readableWidth?: boolean;
  fontScale?: number;
}

export interface ChatTabHandle {
  appendToInput: (fragment: string) => void;
  flushSave: () => Promise<void>;
}

const ChatTabView = forwardRef<ChatTabHandle, Props>(function ChatTabView(
  {
    vaultPath,
    chatId,
    onOpenAiSettings,
    onOpenFile,
    onChatPersisted,
    tocOpen = false,
    readableWidth = true,
    fontScale = 1,
  },
  ref,
) {
  const [messages, setMessages] = useState<UiMessage[]>([]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [modelLabel, setModelLabel] = useState<string>("not connected");
  const [connected, setConnected] = useState(false);
  // Active provider + its model list, surfaced as a unified picker at
  // the top of the chat. Source of the list varies by provider:
  //   copilot                       → copilotModels()
  //   openai/anthropic/gemini       → listProviderModels(provider, key)
  //   openai_compat                 → listProviderModels with base_url
  //   local                         → listModels() filtered to llm+downloaded
  // Storage on selection also varies: see selectModel() below.
  const [provider, setProvider] = useState<string>("");
  const [modelList, setModelList] = useState<PickerModel[]>([]);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [currentModelId, setCurrentModelId] = useState<string>("");
  const [switching, setSwitching] = useState(false);
  const [persistedId, setPersistedId] = useState<string | null>(chatId);
  const [createdIso, setCreatedIso] = useState<string | null>(null);
  const [headerTs, setHeaderTs] = useState<string>("");
  const endRef = useRef<HTMLDivElement>(null);

  const messagesRef = useRef<UiMessage[]>([]);
  const persistedIdRef = useRef<string | null>(persistedId);
  const createdIsoRef = useRef<string | null>(createdIso);
  const vaultRef = useRef<string | null>(vaultPath);
  const modelLabelRef = useRef<string>(modelLabel);
  const onChatPersistedRef = useRef<typeof onChatPersisted>(onChatPersisted);
  const saveTimer = useRef<number | null>(null);
  // Per-message DOM refs so the TOC panel can scroll a clicked prompt
  // into view. Keyed by message index — sparse on purpose so an
  // out-of-bounds index is a no-op rather than a crash.
  const msgElsRef = useRef<Array<HTMLDivElement | null>>([]);
  const messagesScrollerRef = useRef<HTMLDivElement | null>(null);

  messagesRef.current = messages;
  persistedIdRef.current = persistedId;
  createdIsoRef.current = createdIso;
  vaultRef.current = vaultPath;
  modelLabelRef.current = modelLabel;
  onChatPersistedRef.current = onChatPersisted;

  const turnsFromMessages = useCallback(
    (msgs: UiMessage[]): ChatMarkdownTurn[] => {
      const ts = new Date().toISOString();
      const out: ChatMarkdownTurn[] = [];
      for (const m of msgs) {
        if (m.kind === "user") {
          out.push({ role: "user", timestamp: ts, body: m.content });
        } else if (m.kind === "assistant" && !m.streaming) {
          out.push({ role: "assistant", timestamp: ts, body: m.content });
        } else if (m.kind === "tool" && m.result !== undefined) {
          const json = JSON.stringify(
            { name: m.name, args: m.args, result: m.result, is_error: !!m.isError },
            null,
            2,
          );
          out.push({
            role: "tool",
            timestamp: ts,
            body: "```json\n" + json + "\n```",
          });
        }
      }
      return out;
    },
    [],
  );

  const persistNow = useCallback(async () => {
    const vault = vaultRef.current;
    if (!vault) return;
    const turns = turnsFromMessages(messagesRef.current);
    if (turns.length === 0) return;
    const now = new Date().toISOString();
    const created = createdIsoRef.current ?? now;
    const header: ChatHeader = {
      forge_chat: 1,
      created,
      updated: now,
      model: modelLabelRef.current === "not connected" ? null : modelLabelRef.current,
      provider: null,
      system_prompt: null,
      tools_allowed: [],
    };
    try {
      const summary = await saveChat({
        vault_path: vault,
        chat_id: persistedIdRef.current,
        header,
        turns,
      });
      if (!createdIsoRef.current) {
        createdIsoRef.current = created;
        setCreatedIso(created);
      }
      if (persistedIdRef.current !== summary.id) {
        persistedIdRef.current = summary.id;
        setPersistedId(summary.id);
        onChatPersistedRef.current?.(summary.id);
      }
      setHeaderTs(now);
    } catch (e) {
      console.warn("save_chat failed:", e);
    }
  }, [turnsFromMessages]);

  const scheduleSave = useCallback(() => {
    if (saveTimer.current !== null) {
      window.clearTimeout(saveTimer.current);
    }
    saveTimer.current = window.setTimeout(() => {
      saveTimer.current = null;
      void persistNow();
    }, SAVE_DEBOUNCE_MS);
  }, [persistNow]);

  const flushSave = useCallback(async () => {
    if (saveTimer.current !== null) {
      window.clearTimeout(saveTimer.current);
      saveTimer.current = null;
    }
    await persistNow();
  }, [persistNow]);

  useImperativeHandle(
    ref,
    () => ({
      appendToInput: (fragment: string) => {
        setInput((cur) =>
          cur && !cur.endsWith(" ") ? `${cur} ${fragment}` : `${cur}${fragment}`,
        );
      },
      flushSave,
    }),
    [flushSave],
  );

  // Hydrate from disk when an existing chatId is supplied.
  useEffect(() => {
    if (!chatId || !vaultPath) return;
    let cancelled = false;
    (async () => {
      try {
        const file = await loadChat(vaultPath, chatId);
        if (cancelled) return;
        const ui = file.turns.map(diskTurnToUi).filter(Boolean) as UiMessage[];
        setMessages(ui);
        setPersistedId(file.id);
        setCreatedIso(file.header.created);
        if (file.header.model) setModelLabel(file.header.model);
        setHeaderTs(file.header.updated);
      } catch (e) {
        console.warn("load_chat failed:", e);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [chatId, vaultPath]);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [messages, busy]);

  // Streaming token batcher — see Chat.tsx for the full rationale.
  // Coalesces tokens into one setState per animation frame so growing
  // assistant messages don't trigger an O(N) ReactMarkdown reparse on
  // every chunk.
  const pendingTokensRef = useRef("");
  const rafScheduledRef = useRef(false);

  const flushPendingTokens = useCallback(() => {
    rafScheduledRef.current = false;
    const text = pendingTokensRef.current;
    if (!text) return;
    pendingTokensRef.current = "";
    setMessages((prev) => {
      const copy = [...prev];
      const last = copy[copy.length - 1];
      if (last && last.kind === "assistant" && last.streaming) {
        copy[copy.length - 1] = { ...last, content: last.content + text };
      } else {
        copy.push({ kind: "assistant", content: text, streaming: true });
      }
      return copy;
    });
  }, []);

  useEffect(() => {
    const unlisteners: Array<Promise<() => void>> = [
      onChatToken((t) => {
        pendingTokensRef.current += t;
        if (rafScheduledRef.current) return;
        rafScheduledRef.current = true;
        requestAnimationFrame(flushPendingTokens);
      }),
      onChatToolStart((p) => {
        flushPendingTokens();
        setMessages((prev) => [
          ...prev,
          { kind: "tool", name: p.name, args: p.args },
        ]);
      }),
      onChatToolResult((p) =>
        setMessages((prev) => {
          const copy = [...prev];
          for (let i = copy.length - 1; i >= 0; i--) {
            const m = copy[i];
            if (m.kind === "tool" && m.name === p.name && m.result === undefined) {
              copy[i] = { ...m, result: p.content, isError: p.is_error };
              break;
            }
          }
          return copy;
        }),
      ),
      onChatDone(() => {
        flushPendingTokens();
        setMessages((prev) => {
          const copy = [...prev];
          const last = copy[copy.length - 1];
          if (last && last.kind === "assistant") {
            copy[copy.length - 1] = { ...last, streaming: false };
          }
          return copy;
        });
        setBusy(false);
        scheduleSave();
      }),
      onChatError((msg) => {
        flushPendingTokens();
        setMessages((prev) => [...prev, { kind: "error", message: msg }]);
        setBusy(false);
      }),
    ];
    return () => {
      unlisteners.forEach((p) => p.then((fn) => fn()));
    };
  }, [scheduleSave, flushPendingTokens]);

  useEffect(() => {
    const onBlur = () => {
      void flushSave();
    };
    window.addEventListener("blur", onBlur);
    return () => window.removeEventListener("blur", onBlur);
  }, [flushSave]);

  // Load models for whichever provider is active. Runs on mount and
  // whenever the provider changes. Each branch shapes its provider's
  // native model type into `PickerModel { id, name, vendor }` for the
  // picker.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const s = await getSettings();
        if (cancelled) return;
        const active = s.ai_provider || "";
        setProvider(active);

        // Resolve the currently-selected model id per provider's
        // storage convention (legacy Settings vs vault.ai.providers).
        let curId = "";
        if (active === "copilot") curId = s.copilot_model || "";
        else if (vaultPath) {
          try {
            const vs = await getVaultSettings(vaultPath);
            curId = vs.ai.providers[active]?.default_model || "";
          } catch {}
        }
        if (!cancelled) setCurrentModelId(curId);

        // Fetch the menu.
        setModelsLoading(true);
        let list: PickerModel[] = [];
        try {
          if (active === "copilot") {
            const cp = await copilotModels();
            list = cp.map((m) => ({ id: m.id, name: m.name, vendor: m.vendor }));
          } else if (active === "openai" || active === "anthropic" || active === "gemini" || active === "openai_compat") {
            // Pull api_key + base_url from vault settings so listProviderModels
            // can call the provider's enumeration endpoint.
            let apiKey: string | undefined;
            let baseUrl: string | undefined;
            if (vaultPath) {
              try {
                const vs = await getVaultSettings(vaultPath);
                const cfg = vs.ai.providers[active];
                apiKey = cfg?.api_key ?? undefined;
                baseUrl = cfg?.base_url ?? undefined;
              } catch {}
            }
            const pm = await listProviderModels(active, apiKey, baseUrl);
            const vendor = VENDOR_LABEL[active] ?? active;
            list = pm.map((m) => ({ id: m.id, name: m.display_name, vendor }));
          }
        } catch (e) {
          console.warn(`ChatTabView: list models for ${active} failed:`, e);
        }
        if (!cancelled) {
          setModelList(list);
          setModelsLoading(false);
        }
      } catch (e) {
        console.warn("ChatTabView: load settings failed:", e);
        if (!cancelled) setModelsLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [vaultPath]);

  // Switch the active provider's model: persist to wherever that
  // provider stores its default, then reconnect the inference thread.
  const selectModel = useCallback(
    async (modelId: string) => {
      if (busy || switching) return;
      if (!modelId || modelId === currentModelId) return;
      setSwitching(true);
      try {
        const s = await getSettings();
        if (provider === "copilot") {
          await setSettings({ ...s, copilot_model: modelId });
        } else if (provider && vaultPath) {
          // Per-vault provider config — set default_model.
          const vs = await getVaultSettings(vaultPath);
          const cfg = vs.ai.providers[provider] ?? {
            api_key: null,
            base_url: null,
            default_model: null,
          };
          await setVaultSettings(vaultPath, {
            ...vs,
            ai: {
              ...vs.ai,
              providers: {
                ...vs.ai.providers,
                [provider]: { ...cfg, default_model: modelId },
              },
            },
          });
        }
        setCurrentModelId(modelId);
        const res = await connectInference();
        setModelLabel(res.model_name);
        setConnected(true);
      } catch (e) {
        setMessages((prev) => [
          ...prev,
          { kind: "error", message: `Switch model failed: ${String(e)}` },
        ]);
      } finally {
        setSwitching(false);
      }
    },
    [busy, switching, currentModelId, provider, vaultPath],
  );

  // Auto-connect on mount: try the saved provider/model once silently.
  // Mirrors the same effect in Chat.tsx — if anything is configured we
  // come up "connected" with the right label; if nothing is, we stay
  // at "not connected" without an error toast and retry on next send.
  useEffect(() => {
    if (connected) return;
    let cancelled = false;
    connectInference()
      .then((res) => {
        if (cancelled) return;
        setModelLabel(res.model_name);
        setConnected(true);
      })
      .catch(() => {
        // Expected when no provider is configured yet.
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const send = async () => {
    const text = input.trim();
    if (!text || busy) return;
    if (!connected) {
      try {
        const res = await connectInference();
        setModelLabel(res.model_name);
        setConnected(true);
      } catch (e) {
        setMessages((prev) => [
          ...prev,
          { kind: "error", message: `Connect failed: ${String(e)}` },
        ]);
        return;
      }
    }
    const next: UiMessage[] = [...messages, { kind: "user", content: text }];
    setMessages(next);
    setInput("");
    setBusy(true);
    scheduleSave();

    const history: ChatTurn[] = next
      .filter(
        (m): m is Extract<UiMessage, { kind: "user" } | { kind: "assistant" }> =>
          m.kind === "user" || m.kind === "assistant",
      )
      .map((m) => ({
        role: m.kind === "user" ? "user" : "assistant",
        content: m.content,
      }));

    try {
      await sendChatMessage(history);
    } catch (e) {
      setMessages((prev) => [
        ...prev,
        { kind: "error", message: `Send failed: ${String(e)}` },
      ]);
      setBusy(false);
    }
  };

  const stop = async () => {
    try {
      await stopChat();
    } catch {
      // best-effort
    }
    setBusy(false);
  };

  /**
   * Save a single assistant response (and its preceding user prompt) as a
   * standalone note in the vault. Per-response save is the only "save" path;
   * full-chat export was removed.
   */
  const onSaveResponseAsNote = useCallback(async (assistantIndex: number) => {
    const vault = vaultRef.current;
    if (!vault) return;
    const list = messagesRef.current;
    const a = list[assistantIndex];
    if (!a || a.kind !== "assistant") return;
    let userPrompt = "";
    for (let i = assistantIndex - 1; i >= 0; i--) {
      const m = list[i];
      if (m.kind === "user") { userPrompt = m.content; break; }
    }
    const slug =
      (userPrompt || "response")
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, "-")
        .replace(/^-+|-+$/g, "")
        .slice(0, 60) || "response";
    const ts = new Date().toISOString().slice(0, 10);
    const path = `notes/${slug}-${ts}.md`;
    let body = "";
    if (userPrompt) body += `> ${userPrompt.replace(/\n/g, "\n> ")}\n\n`;
    body += a.content;
    try {
      await writeFile(path, body);
      onOpenFile?.(path);
    } catch (e) {
      console.warn("save response as note failed:", e);
    }
  }, [onOpenFile]);

  const headerTitle = useMemo(() => {
    const t = deriveTitleFromMessages(messages);
    return t || "New conversation";
  }, [messages]);

  const lastIsRunningTool =
    busy &&
    messages.length > 0 &&
    (() => {
      const last = messages[messages.length - 1];
      return last.kind === "tool" && last.result === undefined;
    })();
  const lastIsStreaming =
    busy &&
    messages.length > 0 &&
    (() => {
      const last = messages[messages.length - 1];
      return last.kind === "assistant" && last.streaming;
    })();
  const showThinkingRow = busy && !lastIsRunningTool && !lastIsStreaming;

  // Build the TOC: each user prompt becomes a level-1 entry. We stash
  // the message index in `lineNumber` so onHeadingClick can address the
  // exact `<div>` via msgElsRef without an extra parallel array. The
  // text is the first line of the prompt, ellipsised at ~80 chars.
  const tocHeadings = useMemo<Heading[]>(() => {
    if (!tocOpen) return [];
    const out: Heading[] = [];
    for (let i = 0; i < messages.length; i++) {
      const m = messages[i];
      if (m.kind !== "user") continue;
      const firstLine = (m.content ?? "").split("\n", 1)[0].trim();
      const text = firstLine
        ? firstLine.length > 80
          ? firstLine.slice(0, 77) + "…"
          : firstLine
        : "(empty prompt)";
      out.push({ level: 1, text, lineNumber: i });
    }
    return out;
  }, [tocOpen, messages]);

  const handleTocClick = useCallback((h: Heading) => {
    const idx = h.lineNumber ?? -1;
    const el = msgElsRef.current[idx];
    if (!el) return;
    el.scrollIntoView({ block: "start", behavior: "auto" });
  }, []);

  // --md-zoom flows through the same CSS hook as the markdown editor:
  // rendered text inside MessageBlock reads font sizes via tokens
  // multiplied by --md-zoom. Setting it on the messages scroller scopes
  // the zoom to chat content without affecting chrome.
  const zoomStyle: CSSProperties = {
    "--md-zoom": fontScale,
  } as CSSProperties;
  const messagesInner: CSSProperties = readableWidth
    ? { maxWidth: 720, margin: "0 auto" }
    : { maxWidth: "none", margin: "0 auto" };

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        ...zoomStyle,
      }}
    >
      {/* Header */}
      <div
        style={{
          height: 32,
          minHeight: 32,
          padding: "0 16px",
          display: "flex",
          alignItems: "center",
          gap: 8,
          borderBottom: "1px solid var(--background-modifier-border)",
        }}
      >
        <span
          style={{
            fontSize: "var(--font-ui-medium)",
            fontWeight: 600,
            color: "var(--text-normal)",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
            maxWidth: "60%",
          }}
        >
          {headerTitle}
        </span>
        {provider ? (
          <ChatModelPicker
            models={modelList}
            currentId={currentModelId}
            loading={modelsLoading}
            disabled={busy || switching}
            onSelect={(id) => void selectModel(id)}
          />
        ) : (
          <Chip active onClick={onOpenAiSettings}>
            {modelLabel}
          </Chip>
        )}
        <GhostBtn
          icon={<Sparkles size={14} />}
          label="AI settings — providers, models, voice"
          size={24}
          onClick={onOpenAiSettings}
        />
        <div style={{ flex: 1 }} />
        {headerTs && (
          <span
            style={{
              fontSize: "var(--font-ui-smaller)",
              color: "var(--text-faint)",
            }}
          >
            {formatTs(headerTs)}
          </span>
        )}
      </div>

      {/* Body row: optional TOC on the left, messages + composer on
          the right. Layout mirrors the .md tab: TOC is fixed-width, the
          rest fills the remaining space with its own scroller. */}
      <div
        style={{
          flex: 1,
          display: "flex",
          minHeight: 0,
          minWidth: 0,
          overflow: "hidden",
        }}
      >
        {tocOpen && (
          <TOCPanel
            title={headerTitle}
            headings={tocHeadings}
            onHeadingClick={handleTocClick}
          />
        )}
        <div
          style={{
            flex: 1,
            display: "flex",
            flexDirection: "column",
            minHeight: 0,
            minWidth: 0,
          }}
        >
          {/* Messages scroller */}
          <div
            ref={messagesScrollerRef}
            style={{
              flex: 1,
              overflowY: "auto",
              padding: "20px 64px",
              overflowAnchor: "none",
            }}
          >
            <div style={messagesInner}>
              {messages.length === 0 ? (
                <div
                  style={{
                    padding: "40px 24px",
                    textAlign: "center",
                    color: "var(--text-faint)",
                    fontSize: "var(--font-ui-medium)",
                  }}
                >
                  Ask anything…
                </div>
              ) : (
                messages.map((m, i) => (
                  <div
                    key={i}
                    ref={(el) => {
                      msgElsRef.current[i] = el;
                    }}
                  >
                    <MessageBlock
                      msg={m}
                      isLast={i === messages.length - 1}
                      assistantLabel={modelLabel === "not connected" ? undefined : modelLabel}
                      onSaveAsNote={
                        m.kind === "assistant" && !m.streaming
                          ? () => onSaveResponseAsNote(i)
                          : undefined
                      }
                    />
                  </div>
                ))
              )}
              {showThinkingRow && <RunningIndicator label="Thinking" />}
              <div ref={endRef} />
            </div>
          </div>

          {/* Composer — outer wrapper matches the messages region so the
              input lines up with the prompts above. */}
          <div
            style={{
              width: "100%",
              padding: "0 64px",
              boxSizing: "border-box",
            }}
          >
            <div
              style={{
                ...messagesInner,
                marginTop: 16,
                marginBottom: 20,
                background: "var(--background-primary-alt)",
                border: "1px solid var(--background-modifier-border)",
                borderRadius: "var(--radius-m)",
                display: "flex",
                alignItems: "flex-end",
                padding: "6px 10px",
                gap: 8,
              }}
            >
          <textarea
            value={input}
            rows={1}
            onChange={(e) => {
              setInput(e.target.value);
              const ta = e.currentTarget;
              ta.style.height = "auto";
              ta.style.height = Math.min(ta.scrollHeight, 144) + "px";
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                if (busy) stop();
                else send();
              }
            }}
            placeholder={busy ? "Streaming..." : "Ask anything… (Shift+Enter for newline)"}
            disabled={busy}
            style={{
              flex: 1,
              border: 0,
              background: "transparent",
              outline: "none",
              fontSize: "var(--font-ui-medium)",
              color: "var(--text-normal)",
              minWidth: 0,
              resize: "none",
              fontFamily: "inherit",
              lineHeight: 1.45,
              padding: "4px 0",
              maxHeight: 144,
              overflowY: "auto",
            }}
          />
          <VoiceInput
            onTranscript={(t) =>
              setInput((prev) => (prev ? prev + (prev.endsWith(" ") ? "" : " ") + t : t))
            }
            disabled={busy}
          />
          <button
            onClick={busy ? stop : send}
            disabled={!busy && !input.trim()}
            style={{
              background: input.trim() || busy
                ? "var(--interactive-accent)"
                : "var(--background-modifier-border)",
              color: input.trim() || busy
                ? "var(--text-on-accent)"
                : "var(--text-faint)",
              border: 0,
              borderRadius: "var(--radius-s)",
              width: 28,
              height: 28,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              cursor: input.trim() || busy ? "pointer" : "default",
              flexShrink: 0,
              alignSelf: "flex-end",
            }}
          >
            <Send size={14} />
          </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
});

export default ChatTabView;

function deriveTitleFromMessages(messages: UiMessage[]): string {
  const firstUser = messages.find((m) => m.kind === "user") as
    | Extract<UiMessage, { kind: "user" }>
    | undefined;
  if (!firstUser) return "";
  const collapsed = firstUser.content.split(/\s+/).filter(Boolean).join(" ");
  return collapsed.length > 80 ? collapsed.slice(0, 80) : collapsed;
}

function diskTurnToUi(turn: ChatMarkdownTurn): UiMessage | null {
  if (turn.role === "user") {
    return { kind: "user", content: turn.body };
  }
  if (turn.role === "assistant") {
    return { kind: "assistant", content: turn.body, streaming: false };
  }
  if (turn.role === "tool") {
    const m = turn.body.match(/```json\s*([\s\S]*?)```/);
    if (m) {
      try {
        const parsed = JSON.parse(m[1]) as {
          name?: string;
          args?: unknown;
          result?: unknown;
          is_error?: boolean;
        };
        return {
          kind: "tool",
          name: parsed.name ?? "tool",
          args:
            typeof parsed.args === "string"
              ? parsed.args
              : JSON.stringify(parsed.args ?? {}),
          result:
            typeof parsed.result === "string"
              ? parsed.result
              : JSON.stringify(parsed.result ?? ""),
          isError: parsed.is_error,
        };
      } catch {
        // fall through
      }
    }
    return { kind: "tool", name: "tool", args: turn.body, result: "" };
  }
  return null;
}

function formatTs(iso: string): string {
  try {
    const d = new Date(iso);
    if (isNaN(d.getTime())) return iso;
    return d.toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      year: "numeric",
    });
  } catch {
    return iso;
  }
}
