import { useEffect, useRef } from "react";
import { ArrowUp, Square } from "lucide-react";
import { VoiceInput } from "../VoiceInput";

interface Props {
  value: string;
  busy: boolean;
  connected: boolean;
  onChange: (v: string) => void;
  onSend: () => void;
  onStop: () => void;
}

const MIN_ROWS_PX = 36;
// Long system prompts (the WIDGET_PROMPT.md contract is ~150 lines) need
// a tall composer or the user can't see what they're pasting. Cap at
// 50% of viewport so the chat history is never fully eclipsed.
const MAX_ROWS_PX = Math.max(280, Math.floor((typeof window !== "undefined" ? window.innerHeight : 600) * 0.5));

export function ChatComposer({
  value,
  busy,
  connected,
  onChange,
  onSend,
  onStop,
}: Props) {
  const ref = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    const next = Math.min(MAX_ROWS_PX, Math.max(MIN_ROWS_PX, el.scrollHeight));
    el.style.height = next + "px";
  }, [value]);

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      if (!busy) onSend();
    }
  };

  const canSend = !busy && value.trim().length > 0 && connected;

  return (
    <div className="forge-chat__composer">
      <div className="forge-chat__composer-frame">
        <textarea
          ref={ref}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={onKeyDown}
          placeholder={connected ? "Ask anything about your vault" : "Connect to begin"}
          rows={1}
          className="forge-chat__textarea"
          disabled={!connected}
        />
        <div className="forge-chat__composer-actions">
          <div className="forge-chat__composer-actions-left">
            <VoiceInput
              disabled={busy || !connected}
              onTranscript={(text) => {
                if (!text) return;
                onChange(value ? value.trimEnd() + " " + text : text);
              }}
            />
            {/* ConversationToggle (hands-free) needs whisper-cli + piper.
                Hidden until those binaries are installed via Settings. */}
          </div>
          <div className="forge-chat__composer-actions-right">
            {busy ? (
              <button
                type="button"
                onClick={onStop}
                className="forge-chat__send forge-chat__send--stop"
                title="Stop generation"
                aria-label="Stop generation"
              >
                <Square size={11} fill="currentColor" />
              </button>
            ) : (
              <button
                type="button"
                onClick={onSend}
                disabled={!canSend}
                className="forge-chat__send"
                title="Send message"
                aria-label="Send message"
              >
                <ArrowUp size={14} />
              </button>
            )}
          </div>
        </div>
      </div>
      <div className="forge-chat__hint">
        <kbd>Enter</kbd> to send, <kbd>Shift</kbd>+<kbd>Enter</kbd> for newline
      </div>
    </div>
  );
}
