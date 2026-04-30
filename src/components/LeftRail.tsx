import { memo } from "react";
import type { CSSProperties, MouseEvent, ReactNode } from "react";
import {
  Files,
  Search,
  MessageSquare,
  Network,
  Mic,
  Moon,
  Sun,
  Settings,
} from "./ui/Icons";

export type TabKind = "files" | "search" | "chats" | "graph";

interface Props {
  active: TabKind;
  onChange: (tab: TabKind) => void;
  onOpenSettings: () => void;
  theme: "light" | "dark";
  onToggleTheme: () => void;
  onDictate?: () => void;
  // Highlights the rail's mic icon while dictation is live so the user
  // can see the toggle state from anywhere in the app.
  dictationActive?: boolean;
}

interface RailBtnProps {
  icon: ReactNode;
  label: string;
  active?: boolean;
  onClick?: () => void;
}

// Rail button matching forge_ui/Shell.jsx: 36px tall, 48px wide (fills
// rail), 18px icon. Active state uses a 2px accent left-border painted
// via absolute span (not the shadow-inset approach).
function RailBtn({ icon, label, active, onClick }: RailBtnProps) {
  const baseStyle: CSSProperties = {
    width: "100%",
    height: 36,
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    background: active
      ? "var(--background-modifier-active)"
      : "transparent",
    border: 0,
    color: active ? "var(--icon-color-active)" : "var(--icon-color)",
    cursor: "pointer",
    position: "relative",
    transition:
      "background var(--motion-duration-fast) var(--motion-ease), color var(--motion-duration-fast) var(--motion-ease)",
  };
  return (
    <button
      onClick={onClick}
      title={label}
      aria-label={label}
      style={baseStyle}
      onMouseEnter={(e: MouseEvent<HTMLButtonElement>) => {
        if (!active)
          e.currentTarget.style.background =
            "var(--background-modifier-hover)";
      }}
      onMouseLeave={(e: MouseEvent<HTMLButtonElement>) => {
        if (!active) e.currentTarget.style.background = "transparent";
      }}
    >
      {active && (
        <span
          style={{
            position: "absolute",
            left: 0,
            top: 6,
            bottom: 6,
            width: 2,
            background: "var(--interactive-accent)",
            borderRadius: "0 2px 2px 0",
          }}
        />
      )}
      {icon}
    </button>
  );
}

function LeftRail({
  active,
  onChange,
  onOpenSettings,
  theme,
  onToggleTheme,
  onDictate,
  dictationActive,
}: Props) {
  const top: { id: TabKind; icon: ReactNode; label: string }[] = [
    { id: "files", icon: <Files size={18} />, label: "Files" },
    { id: "search", icon: <Search size={18} />, label: "Search" },
    { id: "chats", icon: <MessageSquare size={18} />, label: "Chats" },
    { id: "graph", icon: <Network size={18} />, label: "Graph" },
  ];

  return (
    <nav
      className="workspace-ribbon side-dock-ribbon"
      style={{
        background: "var(--background-secondary-alt)",
        borderRight: "1px solid var(--background-modifier-border)",
        display: "flex",
        flexDirection: "column",
        alignItems: "stretch",
        padding: "8px 0",
        width: "var(--ribbon-width)",
        flexShrink: 0,
        height: "100%",
      }}
    >
      {top.map((t) => (
        <RailBtn
          key={t.id}
          icon={t.icon}
          label={t.label}
          active={active === t.id}
          onClick={() => onChange(t.id)}
        />
      ))}
      <div style={{ flex: 1 }} />
      <RailBtn
        icon={<Mic size={18} />}
        label={dictationActive ? "Dictation — listening (click to stop)" : "Dictation — universal voice input"}
        active={dictationActive}
        onClick={onDictate}
      />
      <RailBtn
        icon={
          theme === "dark" ? <Sun size={18} /> : <Moon size={18} />
        }
        label="Toggle theme"
        onClick={onToggleTheme}
      />
      <RailBtn
        icon={<Settings size={18} />}
        label="Settings"
        onClick={onOpenSettings}
      />
    </nav>
  );
}

export default memo(LeftRail);
