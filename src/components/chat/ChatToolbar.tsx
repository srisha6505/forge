import { ChevronDown, Plug, Settings as SettingsIcon, Trash2 } from "lucide-react";

interface Props {
  modelLabel: string;
  connected: boolean;
  busy: boolean;
  tokenEstimate: number;
  contextLimit: number;
  messageCount: number;
  onConnect: () => void;
  onClear: () => void;
  onOpenAiSettings?: () => void;
}

export function ChatToolbar({
  modelLabel,
  connected,
  busy,
  tokenEstimate,
  contextLimit,
  messageCount,
  onConnect,
  onClear,
  onOpenAiSettings,
}: Props) {
  const tokenPct = Math.min(100, Math.round((tokenEstimate / contextLimit) * 100));
  const tokensLabel = formatTokens(tokenEstimate);
  const limitLabel = formatTokens(contextLimit);

  return (
    <div className="forge-chat__toolbar">
      <div className="forge-chat__toolbar-left">
        <button
          type="button"
          onClick={onConnect}
          disabled={busy}
          className="forge-chat__model"
          title={connected ? `Connected: ${modelLabel}` : "Connect inference"}
        >
          {connected ? (
            <>
              <span className="forge-chat__model-dot" aria-hidden />
              <span className="forge-chat__model-name">{modelLabel}</span>
              <ChevronDown size={11} className="forge-chat__model-chevron" />
            </>
          ) : (
            <>
              <Plug size={12} />
              <span>Connect</span>
            </>
          )}
        </button>
      </div>

      <div className="forge-chat__toolbar-right">
        {connected && (
          <div
            className="forge-chat__context"
            title={`${tokenEstimate.toLocaleString()} of ${contextLimit.toLocaleString()} tokens, ${messageCount} messages`}
          >
            <span className="forge-chat__context-bar" aria-hidden>
              <span
                className="forge-chat__context-bar-fill"
                style={{ width: `${tokenPct}%` }}
              />
            </span>
            <span className="forge-chat__context-text">
              {tokensLabel} / {limitLabel}
            </span>
          </div>
        )}
        {onOpenAiSettings && (
          <button
            type="button"
            onClick={onOpenAiSettings}
            className="forge-chat__icon-btn"
            title="AI settings (model, provider, STT/TTS)"
            aria-label="AI settings"
          >
            <SettingsIcon size={13} />
          </button>
        )}
        <button
          type="button"
          onClick={onClear}
          disabled={messageCount === 0 || busy}
          className="forge-chat__icon-btn"
          title="Clear conversation"
          aria-label="Clear conversation"
        >
          <Trash2 size={13} />
        </button>
      </div>
    </div>
  );
}

function formatTokens(n: number): string {
  if (n < 1000) return String(n);
  if (n < 10_000) return (n / 1000).toFixed(1) + "k";
  return Math.round(n / 1000) + "k";
}
