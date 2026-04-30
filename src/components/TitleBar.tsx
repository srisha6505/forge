import { useEffect, useState } from "react";
import { Minus, Square, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";

// Custom title bar — replaces native chrome. The other Tauri-side fork
// flips `decorations: false` in tauri.conf.json, so this component IS
// the chrome. The wide center region carries `data-tauri-drag-region`
// which Tauri uses to detect window-drag intent. Right-side buttons
// proxy to the Tauri window API (no Rust changes needed).
//
// Linux GTK note: snap-to-edge and double-click-to-maximize are lost
// when decorations are off. The buttons compensate; we don't try to
// re-implement GTK behavior.

const win = getCurrentWindow();

export default function TitleBar() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void win.isMaximized().then(setMaximized);
    void win.onResized(() => {
      void win.isMaximized().then(setMaximized);
    }).then((u) => {
      unlisten = u;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  const onMinimize = () => void win.minimize();
  const onMaximize = async () => {
    const m = await win.isMaximized();
    if (m) await win.unmaximize();
    else await win.maximize();
  };
  const onClose = () => void win.close();

  return (
    <div
      style={{
        height: 32,
        flexShrink: 0,
        display: "flex",
        alignItems: "stretch",
        background: "var(--background-secondary)",
        borderBottom: "1px solid var(--background-modifier-border)",
        userSelect: "none",
        WebkitUserSelect: "none",
      }}
    >
      {/* Left — app name (subtle) */}
      <div
        data-tauri-drag-region
        style={{
          display: "flex",
          alignItems: "center",
          paddingLeft: 12,
          paddingRight: 12,
          fontSize: 12,
          fontWeight: 500,
          color: "var(--text-muted)",
          letterSpacing: 0.2,
        }}
      >
        Forge
      </div>

      {/* Center drag region — fills remaining width */}
      <div
        data-tauri-drag-region
        style={{ flex: 1, minWidth: 0 }}
      />

      {/* Right — window controls */}
      <div style={{ display: "flex", alignItems: "stretch" }}>
        <TitleBarButton onClick={onMinimize} aria-label="Minimize">
          <Minus size={14} />
        </TitleBarButton>
        <TitleBarButton onClick={onMaximize} aria-label={maximized ? "Restore" : "Maximize"}>
          <Square size={12} />
        </TitleBarButton>
        <TitleBarButton onClick={onClose} aria-label="Close" variant="close">
          <X size={14} />
        </TitleBarButton>
      </div>
    </div>
  );
}

interface BtnProps {
  onClick: () => void;
  children: React.ReactNode;
  "aria-label": string;
  variant?: "default" | "close";
}

function TitleBarButton({ onClick, children, "aria-label": ariaLabel, variant = "default" }: BtnProps) {
  const [hover, setHover] = useState(false);
  const isClose = variant === "close";
  const bg = !hover
    ? "transparent"
    : isClose
      ? "hsla(0, 70%, 55%, 0.9)"
      : "var(--background-modifier-hover)";
  const fg = isClose && hover ? "white" : "var(--icon-color, var(--text-muted))";
  return (
    <button
      type="button"
      onClick={onClick}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      aria-label={ariaLabel}
      style={{
        // -webkit-app-region: no-drag prevents the drag-region from
        // capturing button clicks on platforms that honor that hint.
        // @ts-expect-error -- non-standard CSS property
        WebkitAppRegion: "no-drag",
        width: 28,
        height: "100%",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: bg,
        color: fg,
        border: "none",
        padding: 0,
        cursor: "pointer",
        transition: "background 80ms ease",
      }}
    >
      {children}
    </button>
  );
}
