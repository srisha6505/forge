import type { ReactNode } from "react";

export interface KbdProps {
  children: ReactNode;
}

// Keyboard badge. Inline <kbd> element styled as a small bordered pill
// for shortcuts in tooltips, hints, and the command palette.
export default function Kbd({ children }: KbdProps) {
  return (
    <kbd
      style={{
        background: "var(--background-modifier-border)",
        color: "var(--text-muted)",
        fontSize: "var(--font-ui-smaller)",
        fontWeight: 500,
        fontFamily: "var(--font-interface)",
        padding: "1px 5px",
        borderRadius: "var(--radius-s)",
        border: "1px solid var(--background-modifier-border-hover)",
        borderBottomWidth: 2,
        minWidth: 16,
        display: "inline-block",
        textAlign: "center",
      }}
    >
      {children}
    </kbd>
  );
}
