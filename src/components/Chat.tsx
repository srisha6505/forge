import {
  forwardRef,
  memo,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  MessageSquarePlus,
  MoreHorizontal,
  Send,
  Sparkles,
} from "./ui/Icons";
import { GhostBtn, Chip } from "./ui";
import { RunningIndicator } from "./chat/RunningIndicator";
import { MessageBlock, type UiMessage } from "./chat/MessageBlock";
import {
  connectInference,
  listChats,
  loadChat,
  onChatDone,
  onChatError,
  onChatToken,
  onChatToolResult,
  onChatToolStart,
  saveChat,
  sendChatMessage,
  stopChat,
  writeFile,
  type ChatHeader,
  type ChatMarkdownTurn,
  type ChatSummary,
  type ChatTurn,
} from "../lib/tauri";
import { createPortal } from "react-dom";

const DEFAULT_CONTEXT_LIMIT = 8192;
const SAVE_DEBOUNCE_MS = 800;

interface Props {
  vaultPath: string | null;
  chatId: string | null;
  onOpenAiSettings?: () => void;
  onOpenChatAsTab: (chatId: string, title: string) => void;
  onOpenFile?: (path: string) => void;
  onChatPersisted?: (chatId: string) => void;
}

export interface ChatHandle {
  appendToInput: (fragment: string) => void;
  flushSave: () => Promise<void>;
}

const Chat = forwardRef<ChatHandle, Props>(function Chat(
  {
    vaultPath,
    chatId,
    onOpenAiSettings,
    onOpenChatAsTab,
    onOpenFile,
    onChatPersisted,
  },
  ref,
) {
  const [messages, setMessages] = useState<UiMessage[]>([]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [modelLabel, setModelLabel] = useState<string>("not connected");
  const [connected, setConnected] = useState(false);
  const [persistedId, setPersistedId] = useState<string | null>(chatId);
  const [createdIso, setCreatedIso] = useState<string | null>(null);
  const [, setContextLimit] = useState(DEFAULT_CONTEXT_LIMIT);
  const [showChatList, setShowChatList] = useState(false);
  const [pastChats, setPastChats] = useState<ChatSummary[]>([]);
  const endRef = useRef<HTMLDivElement>(null);
  const moreBtnRef = useRef<HTMLDivElement>(null);

  // Refs for the debounced save path. Latest state must be readable
  // from the timer + close/blur callbacks without re-arming on every keystroke.
  const messagesRef = useRef<UiMessage[]>([]);
  const persistedIdRef = useRef<string | null>(persistedId);
  const createdIsoRef = useRef<string | null>(createdIso);
  const vaultRef = useRef<string | null>(vaultPath);
  const modelLabelRef = useRef<string>(modelLabel);
  const onChatPersistedRef = useRef<typeof onChatPersisted>(onChatPersisted);
  const saveTimer = useRef<number | null>(null);

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

  // Hydrate from disk when an existing chatId is supplied. We only hydrate
  // on chatId/vault change so live edits aren't clobbered.
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

  // Streaming token batcher. Without this every Gemma token causes a
  // full ReactMarkdown reparse of the growing assistant message; cost
  // per token is O(N) and perceived smoothness collapses on long
  // replies. With RAF batching we coalesce all tokens that arrive in
  // a single frame into one setState. Final result is identical.
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
        // Tool boundary — flush any in-flight assistant tokens first
        // so they're committed to the assistant bubble BEFORE the new
        // tool message appears below it.
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
        // Critical: drain the token buffer immediately. The user is
        // about to see "streaming complete"; any RAF still in flight
        // would render after we mark the message non-streaming.
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
        // Stream-end is the canonical save trigger.
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

  // Save on app blur — best-effort guard against losing the last turn.
  useEffect(() => {
    const onBlur = () => {
      void flushSave();
    };
    window.addEventListener("blur", onBlur);
    return () => window.removeEventListener("blur", onBlur);
  }, [flushSave]);

  // Ctrl+S also forces a flush. Stops at this surface so the editor save
  // handler still fires when focus is on a file tab.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        const active = document.activeElement;
        if (active && active.tagName === "INPUT") {
          e.preventDefault();
          void flushSave();
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [flushSave]);

  const tokenEstimate = useMemo(() => estimateTokens(messages), [messages]);
  void tokenEstimate;

  const send = async () => {
    const text = input.trim();
    if (!text || busy) return;
    if (!connected) {
      try {
        const res = await connectInference();
        setModelLabel(res.model_name);
        setConnected(true);
        setContextLimit(inferContextLimit(res.model_name));
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
      // backend stop currently best-effort; swallow.
    }
    setBusy(false);
  };

  const clear = () => {
    void flushSave();
    setMessages([]);
    setPersistedId(null);
    setCreatedIso(null);
  };

  /**
   * Save a single assistant response (and its preceding user prompt) as a
   * standalone note in the vault. Path: notes/<slug-of-user-prompt>.md.
   * Per-response save is the only "save to note" surface — full-chat export
   * was removed.
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
    // Avoid clobbering an existing note with the same slug — append a short id.
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

  const headerTitle = useMemo(() => {
    const t = deriveTitleFromMessages(messages);
    return t || "New conversation";
  }, [messages]);

  // Load past chats whenever the popup opens.
  useEffect(() => {
    if (!showChatList) return;
    const vault = vaultRef.current;
    if (!vault) { setPastChats([]); return; }
    let cancelled = false;
    listChats(vault).then((list) => {
      if (cancelled) return;
      list.sort((a, b) => b.updated.localeCompare(a.updated));
      setPastChats(list);
    }).catch(() => { if (!cancelled) setPastChats([]); });
    return () => { cancelled = true; };
  }, [showChatList]);

  const loadChatIntoPanel = useCallback(async (id: string) => {
    const vault = vaultRef.current;
    if (!vault) return;
    try {
      const file = await loadChat(vault, id);
      const ui = file.turns.map(diskTurnToUi).filter(Boolean) as UiMessage[];
      messagesRef.current = ui;
      setMessages(ui);
      setPersistedId(file.id);
      persistedIdRef.current = file.id;
      setCreatedIso(file.header.created);
      createdIsoRef.current = file.header.created;
      if (file.header.model) setModelLabel(file.header.model);
      setShowChatList(false);
    } catch (e) {
      console.warn("loadChat failed:", e);
    }
  }, []);

  return (
    <div
      style={{
        background: "var(--background-primary)",
        borderLeft: "1px solid var(--background-modifier-border)",
        display: "flex",
        flexDirection: "column",
        minWidth: 0,
        height: "100%",
      }}
    >
      {/* Header */}
      <div
        style={{
          height: 36,
          minHeight: 36,
          padding: "0 6px 0 14px",
          display: "flex",
          alignItems: "center",
          gap: 6,
          borderBottom: "1px solid var(--background-modifier-border)",
        }}
      >
        <span
          style={{
            flex: 1,
            fontSize: "var(--font-ui-small)",
            fontWeight: 500,
            color: "var(--text-normal)",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {headerTitle}
        </span>
        {/* Model name is shown in the composer (bottom of panel), so don't
            duplicate it in the header. */}
        <GhostBtn
          icon={<Sparkles size={14} />}
          label="AI settings (providers, models, voice)"
          size={24}
          onClick={onOpenAiSettings}
        />
        <GhostBtn
          icon={<MessageSquarePlus size={14} />}
          label="New conversation"
          size={24}
          onClick={clear}
        />
        <div ref={moreBtnRef} style={{ display: "inline-flex" }}>
          <GhostBtn
            icon={<MoreHorizontal size={14} />}
            label="Past chats"
            size={24}
            onClick={() => setShowChatList((v) => !v)}
          />
        </div>
      </div>

      {/* Messages */}
      <div style={{ flex: 1, overflowY: "auto", padding: "14px 16px" }}>
        {messages.length === 0 ? (
          <EmptyHint />
        ) : (
          messages.map((m, i) => (
            <MessageBlock
              key={i}
              msg={m}
              isLast={i === messages.length - 1}
              onOpenAsTab={
                persistedId
                  ? () => onOpenChatAsTab(persistedId, headerTitle)
                  : undefined
              }
              onSaveAsNote={
                m.kind === "assistant" && !m.streaming
                  ? () => onSaveResponseAsNote(i)
                  : undefined
              }
            />
          ))
        )}
        {showThinkingRow && <RunningIndicator label="Thinking" />}
        <div ref={endRef} />
      </div>

      {/* Composer */}
      <div
        style={{
          margin: "8px 12px 10px",
          background: "var(--background-primary-alt)",
          border: "1px solid var(--background-modifier-border)",
          borderRadius: "var(--radius-m)",
          display: "flex",
          alignItems: "center",
          padding: "6px 10px",
          gap: 8,
        }}
      >
        <Chip onClick={onOpenAiSettings}>{modelLabel}</Chip>
        <input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              if (busy) stop();
              else send();
            }
          }}
          placeholder={busy ? "Streaming..." : "Ask anything..."}
          disabled={busy}
          style={{
            flex: 1,
            border: 0,
            background: "transparent",
            outline: "none",
            fontSize: "var(--font-ui-medium)",
            color: "var(--text-normal)",
            minWidth: 0,
          }}
        />
        <button
          onClick={busy ? stop : send}
          disabled={!busy && !input.trim()}
          title={busy ? "Stop" : "Send"}
          aria-label={busy ? "Stop" : "Send"}
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
          }}
        >
          <Send size={14} />
        </button>
      </div>
      {showChatList && (
        <ChatListPopup
          anchor={moreBtnRef.current}
          chats={pastChats}
          onClose={() => setShowChatList(false)}
          onPickSidebar={(id) => loadChatIntoPanel(id)}
          onPickTab={(id, title) => {
            onOpenChatAsTab(id, title);
            setShowChatList(false);
          }}
        />
      )}
    </div>
  );
});

function ChatListPopup({
  anchor, chats, onClose, onPickSidebar, onPickTab,
}: {
  anchor: HTMLElement | null;
  chats: ChatSummary[];
  onClose: () => void;
  onPickSidebar: (id: string) => void;
  onPickTab: (id: string, title: string) => void;
}) {
  const [pos, setPos] = useState({ top: 0, right: 0 });
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!anchor) return;
    const r = anchor.getBoundingClientRect();
    setPos({ top: r.bottom + 4, right: window.innerWidth - r.right });
  }, [anchor]);

  useEffect(() => {
    const onDoc = (e: MouseEvent) => {
      if (!ref.current) return;
      if (ref.current.contains(e.target as Node)) return;
      if (anchor && anchor.contains(e.target as Node)) return;
      onClose();
    };
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    document.addEventListener("mousedown", onDoc);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [anchor, onClose]);

  return createPortal(
    <div
      ref={ref}
      style={{
        position: "fixed",
        top: pos.top,
        right: pos.right,
        zIndex: 1000,
        width: 320,
        maxHeight: 420,
        background: "var(--background-primary)",
        border: "1px solid var(--background-modifier-border)",
        borderRadius: 8,
        boxShadow: "0 8px 28px hsla(0,0%,0%,0.22)",
        overflow: "hidden",
        display: "flex",
        flexDirection: "column",
      }}
    >
      <div style={{
        padding: "6px 12px", fontSize: "var(--font-ui-smaller)",
        color: "var(--text-muted)",
        borderBottom: "1px solid var(--background-modifier-border)",
      }}>
        Past chats {chats.length > 0 ? `(${chats.length})` : ""}
      </div>
      <div style={{ flex: 1, overflowY: "auto" }}>
        {chats.length === 0 ? (
          <div style={{ padding: 14, fontSize: "var(--font-ui-small)", color: "var(--text-faint)" }}>
            No saved chats.
          </div>
        ) : chats.map((c) => (
          <div
            key={c.id}
            style={{
              display: "grid",
              gridTemplateColumns: "1fr auto",
              gap: 4,
              padding: "6px 8px 6px 12px",
              alignItems: "center",
            }}
          >
            <button
              type="button"
              onClick={() => onPickSidebar(c.id)}
              title="Open in this sidebar"
              style={{
                background: "transparent",
                border: "none",
                color: "var(--text-normal)",
                fontSize: "var(--font-ui-small)",
                textAlign: "left",
                cursor: "pointer",
                padding: "4px 6px",
                borderRadius: 4,
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
              }}
              onMouseEnter={(e) => (e.currentTarget.style.background = "var(--background-modifier-hover)")}
              onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}
            >
              {c.title}
            </button>
            <button
              type="button"
              onClick={() => onPickTab(c.id, c.title)}
              title="Open as tab"
              style={{
                background: "transparent",
                border: "1px solid var(--background-modifier-border)",
                color: "var(--text-muted)",
                fontSize: 11,
                cursor: "pointer",
                padding: "3px 8px",
                borderRadius: 4,
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.background = "var(--background-modifier-hover)";
                e.currentTarget.style.color = "var(--text-normal)";
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.background = "transparent";
                e.currentTarget.style.color = "var(--text-muted)";
              }}
            >
              tab
            </button>
          </div>
        ))}
      </div>
    </div>,
    document.body,
  );
}

export default memo(Chat);

function EmptyHint() {
  return (
    <div
      style={{
        height: "100%",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: "40px 24px",
        textAlign: "center",
        color: "var(--text-faint)",
        fontSize: "var(--font-ui-medium)",
      }}
    >
      Ask anything…
    </div>
  );
}

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
    // Body shape: ```json\n{...}\n```
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
        // fall through to raw text
      }
    }
    return { kind: "tool", name: "tool", args: turn.body, result: "" };
  }
  return null;
}

function estimateTokens(messages: UiMessage[]): number {
  let chars = 0;
  for (const m of messages) {
    if (m.kind === "user" || m.kind === "assistant") chars += m.content.length;
    else if (m.kind === "tool")
      chars += m.name.length + m.args.length + (m.result?.length ?? 0);
    else chars += m.message.length;
  }
  return Math.ceil(chars / 4);
}

function inferContextLimit(modelName: string): number {
  const lower = modelName.toLowerCase();
  if (lower.includes("claude")) return 200_000;
  if (lower.includes("gpt-4")) return 128_000;
  if (lower.includes("gemini")) return 1_000_000;
  if (lower.includes("gemma")) return 8192;
  if (lower.includes("llama")) return 8192;
  return DEFAULT_CONTEXT_LIMIT;
}
