import { memo } from "react";
import {
  Files,
  MessageSquareText,
  Moon,
  Search,
  Settings,
  Sun,
} from "lucide-react";

type TabKind = "files" | "search" | "chat" | "settings";

interface Props {
  active: TabKind;
  onChange: (tab: TabKind) => void;
  onToggleChat: () => void;
  theme: "light" | "dark";
  onToggleTheme: () => void;
}

interface RailButtonProps {
  label: string;
  icon: React.ReactNode;
  active?: boolean;
  onClick: () => void;
}

function RailButton({ label, icon, active, onClick }: RailButtonProps) {
  return (
    <button
      title={label}
      onClick={onClick}
      className={`side-dock-ribbon-action w-9 h-9 flex items-center justify-center rounded-md transition-all duration-150 ${
        active
          ? "bg-[var(--background-modifier-active)] text-[var(--text-accent)] shadow-[inset_0_0_0_1px_var(--background-modifier-border)]"
          : "text-[var(--icon-color)] hover:text-[var(--icon-color-hover)] hover:bg-[var(--background-modifier-hover)]"
      }`}
    >
      {icon}
    </button>
  );
}

function LeftRail({
  active,
  onChange,
  onToggleChat,
  theme,
  onToggleTheme,
}: Props) {
  return (
    <nav
      className="workspace-ribbon side-dock-ribbon flex-shrink-0 h-full flex flex-col items-center py-2 gap-1 bg-[var(--background-secondary)] border-r border-[var(--background-modifier-border)]"
      style={{ width: "var(--ribbon-width)" }}
    >
      <RailButton
        label="Files (Ctrl+Shift+P)"
        icon={<Files size={18} strokeWidth={1.7} />}
        active={active === "files"}
        onClick={() => onChange("files")}
      />
      <RailButton
        label="Search"
        icon={<Search size={18} strokeWidth={1.7} />}
        active={active === "search"}
        onClick={() => onChange("search")}
      />
      <RailButton
        label="Chat (Ctrl+Shift+L)"
        icon={<MessageSquareText size={18} strokeWidth={1.7} />}
        onClick={onToggleChat}
      />

      <div className="flex-1" />

      <RailButton
        label={theme === "light" ? "Switch to dark" : "Switch to light"}
        icon={
          theme === "light" ? (
            <Moon size={18} strokeWidth={1.7} />
          ) : (
            <Sun size={18} strokeWidth={1.7} />
          )
        }
        onClick={onToggleTheme}
      />
      <RailButton
        label="Settings (Ctrl+,)"
        icon={<Settings size={18} strokeWidth={1.7} />}
        active={active === "settings"}
        onClick={() => onChange("settings")}
      />
    </nav>
  );
}

export default memo(LeftRail);
