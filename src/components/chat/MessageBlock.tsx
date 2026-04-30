import { memo } from "react";
import type { CSSProperties } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeHighlight from "rehype-highlight";
import rehypeKatex from "rehype-katex";
import { open as shellOpen } from "@tauri-apps/plugin-shell";

// Route external-protocol clicks through the OS shell so the chat panel
// doesn't navigate the Tauri webview when a user clicks a citation or
// reference URL. Mirrors the same handler in MarkdownPreview.tsx.
const EXTERNAL_RE = /^(https?:|mailto:|ftp:|tel:)/i;
const CHAT_MD_COMPONENTS = {
  a({ href, children, ...rest }: { href?: string; children?: React.ReactNode }) {
    return (
      <a
        href={href}
        onClick={(e) => {
          if (href && EXTERNAL_RE.test(href)) {
            e.preventDefault();
            void shellOpen(href).catch(() => {});
          }
        }}
        {...rest}
      >
        {children}
      </a>
    );
  },
};
import {
  Copy,
  ExternalLink,
  RotateCcw,
  ArrowUpRight,
  Search as SearchIcon,
  ChevronRight,
} from "../ui/Icons";
import { GhostBtn } from "../ui";
import { ToolCallCard } from "./ToolCallCard";

export type UiMessage =
  | { kind: "user"; content: string }
  | { kind: "assistant"; content: string; streaming: boolean }
  | { kind: "tool"; name: string; args: string; result?: string; isError?: boolean }
  | { kind: "error"; message: string };

export interface MessageBlockProps {
  msg: UiMessage;
  isLast: boolean;
  onOpenAsTab?: () => void;
  /** Save THIS single assistant response (and its preceding user prompt) as a note. */
  onSaveAsNote?: () => void;
  onRegenerate?: () => void;
}

// Memoized so a token append to the LAST assistant message doesn't
// re-render every prior message.
export const MessageBlock = memo(MessageBlockImpl, (prev, next) => {
  if (prev.isLast !== next.isLast) return false;
  if (prev.onOpenAsTab !== next.onOpenAsTab) return false;
  if (prev.onSaveAsNote !== next.onSaveAsNote) return false;
  if (prev.onRegenerate !== next.onRegenerate) return false;
  const a = prev.msg;
  const b = next.msg;
  if (a.kind !== b.kind) return false;
  if (a.kind === "user" && b.kind === "user")
    return a.content === b.content;
  if (a.kind === "assistant" && b.kind === "assistant")
    return a.content === b.content && a.streaming === b.streaming;
  if (a.kind === "tool" && b.kind === "tool")
    return (
      a.name === b.name &&
      a.args === b.args &&
      a.result === b.result &&
      a.isError === b.isError
    );
  if (a.kind === "error" && b.kind === "error")
    return a.message === b.message;
  return false;
});

function MessageBlockImpl({
  msg,
  isLast,
  onOpenAsTab,
  onSaveAsNote,
  onRegenerate,
}: MessageBlockProps) {
  if (msg.kind === "tool") {
    if (msg.result !== undefined) {
      return (
        <div style={{ marginBottom: 12 }}>
          <ToolCallCard
            name={msg.name}
            args={msg.args}
            result={msg.result}
            isError={msg.isError}
          />
        </div>
      );
    }
    return (
      <div style={{ marginBottom: 12 }}>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 6,
            padding: "6px 10px",
            background: "var(--background-modifier-message)",
            borderRadius: "var(--radius-m)",
            fontSize: "var(--font-ui-small)",
            color: "var(--text-muted)",
            cursor: "pointer",
          }}
        >
          <SearchIcon size={14} />
          <span style={{ fontFamily: "var(--font-monospace)", fontWeight: 500 }}>
            {msg.name}
          </span>
          <span
            style={{
              color: "var(--text-faint)",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              flex: 1,
              minWidth: 0,
            }}
          >
            {summariseArgs(msg.args)}
          </span>
          <span style={{ marginLeft: "auto", display: "flex" }}>
            <ChevronRight size={12} />
          </span>
        </div>
      </div>
    );
  }

  if (msg.kind === "error") {
    return (
      <div
        style={{
          marginBottom: 16,
          paddingBottom: 16,
          borderBottom: !isLast ? "1px solid var(--hr-color)" : "none",
        }}
      >
        <div
          style={{
            fontSize: "var(--font-ui-smaller)",
            fontWeight: 500,
            textTransform: "uppercase",
            letterSpacing: "0.06em",
            color: "var(--text-error, var(--text-faint))",
            marginBottom: 6,
          }}
        >
          error
        </div>
        <div
          style={{
            fontSize: "calc(13.5px * var(--md-zoom, 1))",
            lineHeight: 1.6,
            color: "var(--text-error, var(--text-normal))",
            fontFamily: "var(--font-text)",
            whiteSpace: "pre-wrap",
          }}
        >
          {msg.message}
        </div>
      </div>
    );
  }

  const roleLabel = msg.kind === "user" ? "you" : "claude";
  const body =
    msg.kind === "assistant" ? stripToolCallProtocol(msg.content) : msg.content;

  const turnStyle: CSSProperties = {
    marginBottom: 16,
    paddingBottom: 16,
    borderBottom: !isLast ? "1px solid var(--hr-color)" : "none",
  };

  return (
    <div style={turnStyle}>
      <div
        style={{
          fontSize: "var(--font-ui-smaller)",
          fontWeight: 500,
          textTransform: "uppercase",
          letterSpacing: "0.06em",
          color: "var(--text-faint)",
          marginBottom: 6,
        }}
      >
        {roleLabel}
      </div>
      {msg.kind === "assistant" ? (
        <div
          className="prose-chat"
          style={{
            fontSize: "calc(13.5px * var(--md-zoom, 1))",
            lineHeight: 1.6,
            color: "var(--text-normal)",
            fontFamily: "var(--font-text)",
          }}
        >
          <ReactMarkdown
            remarkPlugins={[remarkGfm, remarkMath]}
            rehypePlugins={[rehypeKatex, rehypeHighlight]}
            components={CHAT_MD_COMPONENTS}
          >
            {body}
          </ReactMarkdown>
          {msg.streaming && <span className="forge-msg__caret" aria-hidden />}
        </div>
      ) : (
        <div
          style={{
            fontSize: "calc(13.5px * var(--md-zoom, 1))",
            lineHeight: 1.6,
            color: "var(--text-normal)",
            fontFamily: "var(--font-text)",
            whiteSpace: "pre-wrap",
          }}
        >
          {body}
        </div>
      )}
      {msg.kind === "assistant" && (
        <div style={{ display: "flex", gap: 4, marginTop: 8, opacity: 0.7 }}>
          <GhostBtn
            icon={<Copy size={14} />}
            label="Copy"
            size={24}
            onClick={() => {
              if (navigator.clipboard) navigator.clipboard.writeText(body);
            }}
          />
          {onSaveAsNote && (
            <GhostBtn
              icon={<ExternalLink size={14} />}
              label="Save this response as a note"
              size={24}
              onClick={onSaveAsNote}
            />
          )}
          {onRegenerate && (
            <GhostBtn
              icon={<RotateCcw size={14} />}
              label="Regenerate"
              size={24}
              onClick={onRegenerate}
            />
          )}
          {onOpenAsTab && (
            <GhostBtn
              icon={<ArrowUpRight size={14} />}
              label="Open as tab"
              size={24}
              onClick={onOpenAsTab}
            />
          )}
        </div>
      )}
    </div>
  );
}

// The local LLM occasionally leaks the raw tool-call protocol into the
// streamed token output even after the agent has parsed it.
function stripToolCallProtocol(text: string): string {
  let s = text;
  s = s.replace(/call:[a-zA-Z_]+\{[\s\S]*?\}\s*(?:thought\s*)?/g, "");
  s = s.replace(/<\|[^|>\n]*\|>/g, "");
  s = s.replace(/^\s*thought\s*$/gm, "");
  s = s.replace(/\n{3,}/g, "\n\n");
  return s.trim();
}

function summariseArgs(args: string): string {
  const trimmed = args.trim();
  if (trimmed.length <= 80) return trimmed;
  return trimmed.slice(0, 77) + "…";
}
