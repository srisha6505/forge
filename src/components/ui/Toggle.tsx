export interface ToggleProps {
  on: boolean;
  onChange: (v: boolean) => void;
}

// 32x18 switch. Binary on/off; prefer over a checkbox when the change
// takes effect immediately (no explicit Apply step).
export default function Toggle({ on, onChange }: ToggleProps) {
  return (
    <button
      onClick={() => onChange(!on)}
      role="switch"
      aria-checked={on}
      style={{
        width: 32,
        height: 18,
        borderRadius: 999,
        border: 0,
        padding: 2,
        cursor: "pointer",
        background: on
          ? "var(--interactive-accent)"
          : "var(--background-modifier-border)",
        transition:
          "background-color var(--motion-duration-fast) var(--motion-ease)",
        display: "flex",
        alignItems: "center",
      }}
    >
      <div
        style={{
          width: 14,
          height: 14,
          borderRadius: 999,
          background: "var(--background-primary)",
          transform: on ? "translateX(14px)" : "translateX(0)",
          transition:
            "transform var(--motion-duration-fast) var(--motion-ease)",
        }}
      />
    </button>
  );
}
