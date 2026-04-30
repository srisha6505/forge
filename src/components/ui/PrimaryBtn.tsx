import type { CSSProperties, MouseEvent, ReactNode } from "react";

export interface PrimaryBtnProps {
  children: ReactNode;
  onClick?: () => void;
  style?: CSSProperties;
}

// 32px accent-filled primary button. Reserved for the single "submit" or
// "confirm" action per view — avoid using more than one on screen.
export default function PrimaryBtn({
  children,
  onClick,
  style,
}: PrimaryBtnProps) {
  return (
    <button
      onClick={onClick}
      style={{
        height: 32,
        padding: "0 14px",
        borderRadius: "var(--radius-m)",
        background: "var(--interactive-accent)",
        border: 0,
        color: "var(--text-on-accent)",
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
        e.currentTarget.style.background =
          "var(--interactive-accent-hover)";
      }}
      onMouseLeave={(e: MouseEvent<HTMLButtonElement>) => {
        e.currentTarget.style.background = "var(--interactive-accent)";
      }}
    >
      {children}
    </button>
  );
}
