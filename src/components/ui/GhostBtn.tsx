import type { CSSProperties, MouseEvent, ReactNode } from "react";

export interface GhostBtnProps {
  icon: ReactNode;
  label: string;
  onClick?: () => void;
  active?: boolean;
  size?: number;
  style?: CSSProperties;
}

// Icon-only / text-only ghost button. Transparent when idle, tinted on
// hover, accented when `active`. Used in the left rail, tab bar, and
// toolbar chrome.
export default function GhostBtn({
  icon,
  label,
  onClick,
  active,
  size = 28,
  style,
}: GhostBtnProps) {
  return (
    <button
      onClick={onClick}
      aria-label={label}
      title={label}
      style={{
        width: size,
        height: size,
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        background: active
          ? "var(--background-modifier-active)"
          : "transparent",
        border: 0,
        borderRadius: "var(--radius-s)",
        cursor: "pointer",
        color: active
          ? "var(--icon-color-active)"
          : "var(--icon-color)",
        transition:
          "background-color var(--motion-duration-fast) var(--motion-ease), color var(--motion-duration-fast) var(--motion-ease)",
        ...style,
      }}
      onMouseEnter={(e: MouseEvent<HTMLButtonElement>) => {
        if (!active) {
          e.currentTarget.style.background =
            "var(--background-modifier-hover)";
          e.currentTarget.style.color = "var(--icon-color-hover)";
        }
      }}
      onMouseLeave={(e: MouseEvent<HTMLButtonElement>) => {
        if (!active) {
          e.currentTarget.style.background = "transparent";
          e.currentTarget.style.color = "var(--icon-color)";
        }
      }}
    >
      {icon}
    </button>
  );
}
