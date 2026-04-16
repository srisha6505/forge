import { useEffect, useRef, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import {
  connectInference,
  onChatDone,
  onChatError,
  onChatToken,
  onChatToolResult,
  onChatToolStart,
  sendChatMessage,
  type ChatTurn,
} from "../lib/tauri";

type UiMessage =
  | { kind: "user"; content: string }
  | { kind: "assistant"; content: string; streaming: boolean }
  | { kind: "tool"; name: string; args: string; result?: string; isError?: boolean }
  | { kind: "error"; message: string };

export default function Chat() {
  const [messages, setMessages] = useState<UiMessage[]>([]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [modelLabel, setModelLabel] = useState<string>("not connected");
  const [connected, setConnected] = useState(false);
  const endRef = useRef<HTMLDivElement>(null);

  // Auto-scroll on new content.
  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  // Subscribe to Tauri chat events once on mount. Unlisten on unmount.
  useEffect(() => {
    const unlisteners: Array<Promise<() => void>> = [
      onChatToken((t) =>
        setMessages((prev) => {
          const copy = [...prev];
          const last = copy[copy.length - 1];
          if (last && last.kind === "assistant" && last.streaming) {
            copy[copy.length - 1] = { ...last, content: last.content + t };
          } else {
            copy.push({ kind: "assistant", content: t, streaming: true });
          }
          return copy;
        }),
      ),
      onChatToolStart((p) =>
        setMessages((prev) => [
          ...prev,
          { kind: "tool", name: p.name, args: p.args },
        ]),
      ),
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
        setMessages((prev) => {
          const copy = [...prev];
          const last = copy[copy.length - 1];
          if (last && last.kind === "assistant") {
            copy[copy.length - 1] = { ...last, streaming: false };
          }
          return copy;
        });
        setBusy(false);
      }),
      onChatError((msg) => {
        setMessages((prev) => [...prev, { kind: "error", message: msg }]);
        setBusy(false);
      }),
    ];

    return () => {
      unlisteners.forEach((p) => p.then((fn) => fn()));
    };
  }, []);

  const connect = async () => {
    try {
      const res = await connectInference();
      setModelLabel(res.model_name);
      setConnected(true);
    } catch (e) {
      setMessages((prev) => [
        ...prev,
        { kind: "error", message: `Connect failed: ${String(e)}` },
      ]);
    }
  };

  const send = async () => {
    const text = input.trim();
    if (!text || busy) return;
    const next: UiMessage[] = [...messages, { kind: "user", content: text }];
    setMessages(next);
    setInput("");
    setBusy(true);

    const history: ChatTurn[] = next
      .filter((m): m is Extract<UiMessage, { kind: "user" } | { kind: "assistant" }> =>
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

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      send();
    }
  };

  return (
    <div className="forge-chat h-full flex flex-col bg-[var(--background-primary)]">
      <div className="px-4 py-2.5 border-b border-[var(--background-modifier-border)] flex items-center justify-between">
        <div className="text-[11px] uppercase tracking-wider text-[var(--text-faint)]">
          Chat
        </div>
        {!connected ? (
          <button
            onClick={connect}
            className="mod-cta text-[10px] px-2.5 py-1 rounded bg-[var(--interactive-accent)] hover:bg-[var(--interactive-accent-hover)] text-[var(--text-on-accent)] font-medium"
          >
            Connect
          </button>
        ) : (
          <div className="text-[10px] text-[var(--text-muted)] truncate max-w-[220px]">
            {modelLabel}
          </div>
        )}
      </div>

      <div className="flex-1 overflow-y-auto px-4 py-4 space-y-4 min-w-0">
        {messages.length === 0 && (
          <div className="text-center text-[12px] text-[var(--text-faint)] pt-10">
            Ask anything about your vault
          </div>
        )}
        {messages.map((m, i) => (
          <MessageBlock key={i} msg={m} />
        ))}
        <div ref={endRef} />
      </div>

      <div className="px-3 py-2.5 border-t border-[var(--background-modifier-border)] flex gap-2 items-end">
        <textarea
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={onKeyDown}
          placeholder="Message…"
          rows={2}
          className="flex-1 text-[13px] px-3 py-2 rounded bg-[var(--background-modifier-form-field)] outline-none resize-none border border-[var(--background-modifier-border)] focus:border-[var(--interactive-accent)] min-w-0 text-[var(--text-normal)] placeholder:text-[var(--text-faint)]"
        />
        <button
          onClick={send}
          disabled={busy || !input.trim()}
          className="mod-cta px-3 py-2 text-[11px] rounded bg-[var(--interactive-accent)] hover:bg-[var(--interactive-accent-hover)] disabled:bg-[var(--background-modifier-border)] disabled:text-[var(--text-faint)] text-[var(--text-on-accent)] font-medium transition-colors"
        >
          {busy ? "…" : "Send"}
        </button>
      </div>
    </div>
  );
}

function MessageBlock({ msg }: { msg: UiMessage }) {
  if (msg.kind === "user") {
    return (
      <div className="flex justify-end">
        <div className="max-w-[85%] px-3 py-2 rounded-xl bg-[var(--interactive-accent)] text-[var(--text-on-accent)] text-[13px] whitespace-pre-wrap break-words font-medium">
          {msg.content}
        </div>
      </div>
    );
  }
  if (msg.kind === "assistant") {
    const cleaned = stripToolCallProtocol(msg.content);
    return (
      <div className="flex flex-col">
        <div className="prose-chat break-words min-w-0">
          <ReactMarkdown
            remarkPlugins={[remarkGfm]}
            rehypePlugins={[rehypeHighlight]}
          >
            {cleaned}
          </ReactMarkdown>
          {msg.streaming && (
            <span className="inline-block w-2 h-4 bg-[var(--interactive-accent)] animate-pulse align-middle ml-1" />
          )}
        </div>
      </div>
    );
  }
  if (msg.kind === "tool") {
    const done = msg.result !== undefined;
    return (
      <div className="border-l-2 border-[var(--interactive-accent)] pl-3 py-1">
        <div className="flex items-center gap-2 text-[12px]">
          <span className="font-semibold text-[var(--interactive-accent)]">tool</span>
          <span className="font-mono text-[var(--text-accent)]">
            {msg.name}
          </span>
          <span className="text-[var(--text-muted)] flex-1 truncate">
            {summariseArgs(msg.name, msg.args)}
          </span>
          <span
            className={`text-[10px] ${
              !done
                ? "text-[var(--interactive-accent)]"
                : msg.isError
                  ? "text-[var(--text-error)]"
                  : "text-[var(--text-muted)]"
            }`}
          >
            {!done ? "running…" : msg.isError ? "error" : "done"}
          </span>
        </div>
      </div>
    );
  }
  return (
    <div className="text-[12px] text-[var(--text-error)] px-2">{msg.message}</div>
  );
}

// The local LLM occasionally leaks the raw tool-call protocol into the
// streamed token output even after the agent has parsed it: e.g.
// `call:write_file{content:<|"|>...<|"|>,path:<|"|>note.md<|"|>}thought`
// followed by the structured tool block. Strip those raw artifacts so
// the user only sees clean prose + the rendered tool block.
function stripToolCallProtocol(text: string): string {
  let s = text;
  // Strip `call:name{...}` blocks, even across newlines (greedy until a
  // closing brace at the end of a line or end of stream).
  s = s.replace(/call:[a-zA-Z_]+\{[\s\S]*?\}\s*(?:thought\s*)?/g, "");
  // Strip Gemma protocol tags like `<|"|>`, `<|tool_call|>`, etc.
  s = s.replace(/<\|[^|>\n]*\|>/g, "");
  // Strip plain "thought" markers left behind.
  s = s.replace(/^\s*thought\s*$/gm, "");
  // Collapse the resulting runs of blank lines.
  s = s.replace(/\n{3,}/g, "\n\n");
  return s.trim();
}

function summariseArgs(name: string, args: string): string {
  try {
    const parsed = JSON.parse(args);
    switch (name) {
      case "search_vault":
        return `"${parsed.query ?? ""}"`;
      case "read_file":
        return parsed.path ?? "";
      case "list_files":
        return parsed.directory ?? "";
      case "write_file":
        return parsed.path ?? "";
      case "edit_file":
        return parsed.path ?? "";
      case "web_search":
        return `"${parsed.query ?? ""}"`;
      default:
        return "";
    }
  } catch {
    return "";
  }
}
