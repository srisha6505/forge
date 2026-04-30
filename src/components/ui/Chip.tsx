import type { ReactNode } from "react";

export interface ChipProps {
  children: ReactNode;
  active?: boolean;
  onClick?: () => void;
}

// Pill-shaped chip. Used for tags, filter pills, and metadata badges.
// Active state swaps to the accent tint.
export default function Chip({ children, active, onClick }: ChipProps) {
  return (
    <button
      onClick={onClick}
      style={{
        padding: "2px 8px",
        borderRadius: 999,
        border: 0,
        background: active
          ? "var(--background-modifier-active)"
          : "var(--background-modifier-message)",
        color: active ? "var(--text-accent)" : "var(--text-muted)",
        fontSize: "var(--font-ui-smaller)",
        fontWeight: 500,
        cursor: "pointer",
      }}
    >
      {children}
    </button>
  );
}
