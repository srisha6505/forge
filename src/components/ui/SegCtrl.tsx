export interface SegCtrlOption {
  value: string;
  label: string;
}

export interface SegCtrlProps {
  options: SegCtrlOption[];
  value: string;
  onChange: (v: string) => void;
}

// Segmented control. Single selection, 28px tall, used for mode toggles
// (edit/read, theme variants, etc.) where the choices are mutually
// exclusive and fit on one line.
export default function SegCtrl({ options, value, onChange }: SegCtrlProps) {
  return (
    <div
      style={{
        height: 28,
        display: "inline-flex",
        background: "var(--background-modifier-hover)",
        borderRadius: "var(--radius-m)",
        padding: 2,
        gap: 2,
      }}
    >
      {options.map((o) => (
        <button
          key={o.value}
          onClick={() => onChange(o.value)}
          style={{
            height: 24,
            padding: "0 12px",
            border: 0,
            borderRadius: "var(--radius-s)",
            background:
              value === o.value
                ? "var(--background-primary)"
                : "transparent",
            color:
              value === o.value
                ? "var(--text-normal)"
                : "var(--text-muted)",
            fontSize: "var(--font-ui-small)",
            fontWeight: 500,
            cursor: "pointer",
            boxShadow: value === o.value ? "var(--shadow-s)" : "none",
            transition: "all var(--motion-duration-fast) var(--motion-ease)",
          }}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
