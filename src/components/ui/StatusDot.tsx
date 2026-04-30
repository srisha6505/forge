export type StatusDotVariant = "connected" | "error" | "idle";

export interface StatusDotProps {
  variant?: StatusDotVariant;
}

// 6px dot for connection / health / availability indicators. Colours:
//   connected — saturated green (explicit literal so it reads the same
//               on both themes; palette greens are too subtle).
//   error     — --text-error.
//   idle      — --text-faint.
export default function StatusDot({ variant = "connected" }: StatusDotProps) {
  const colors: Record<StatusDotVariant, string> = {
    connected: "hsl(92, 42%, 45%)",
    error: "var(--text-error)",
    idle: "var(--text-faint)",
  };
  return (
    <span
      style={{
        width: 6,
        height: 6,
        borderRadius: 999,
        background: colors[variant] ?? colors.idle,
        display: "inline-block",
        flexShrink: 0,
      }}
    />
  );
}
