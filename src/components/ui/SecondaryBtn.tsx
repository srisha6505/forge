import type { CSSProperties, MouseEvent, ReactNode } from "react";

export interface SecondaryBtnProps {
  children: ReactNode;
  onClick?: () => void;
  style?: CSSProperties;
}

// 32px bordered secondary button. Neutral interactive surface; pairs
// with PrimaryBtn when an action has a dismiss/cancel counterpart.
export default function SecondaryBtn({
  children,
  onClick,
  style,
}: SecondaryBtnProps) {
  return (
    <button
      onClick={onClick}
      style={{
        height: 32,
        padding: "0 12px",
        borderRadius: "var(--radius-m)",
        background: "var(--interactive-normal)",
        border: "1px solid var(--background-modifier-border)",
        color: "var(--text-normal)",
        fontSize: "var(--font-ui-medium)",
        fontWeight: 500,
        cursor: "pointer",
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
        transition:
          "background-color var(--motion-duration-fast) var(--motion-ease)",
        ...style,
      }}
      onMouseEnter={(e: MouseEvent<HTMLButtonElement>) => {
        e.currentTarget.style.background = "var(--interactive-hover)";
      }}
      onMouseLeave={(e: MouseEvent<HTMLButtonElement>) => {
        e.currentTarget.style.background = "var(--interactive-normal)";
      }}
    >
      {children}
    </button>
  );
}
