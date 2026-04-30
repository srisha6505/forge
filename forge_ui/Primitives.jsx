/* global React, Icons */

/* ── Shared small components ── */

function GhostBtn({ icon, label, onClick, active, size = 28, style }) {
  return React.createElement("button", {
    onClick, "aria-label": label, title: label,
    style: {
      width: size, height: size, display: "inline-flex", alignItems: "center", justifyContent: "center",
      background: active ? "var(--background-modifier-active)" : "transparent",
      border: 0, borderRadius: "var(--radius-s)", cursor: "pointer",
      color: active ? "var(--icon-color-active)" : "var(--icon-color)",
      transition: "background-color var(--motion-duration-fast) var(--motion-ease), color var(--motion-duration-fast) var(--motion-ease)",
      ...style
    },
    onMouseEnter: (e) => { if(!active) { e.currentTarget.style.background = "var(--background-modifier-hover)"; e.currentTarget.style.color = "var(--icon-color-hover)"; }},
    onMouseLeave: (e) => { if(!active) { e.currentTarget.style.background = "transparent"; e.currentTarget.style.color = "var(--icon-color)"; }},
  }, icon);
}

function SecondaryBtn({ children, onClick, style }) {
  return React.createElement("button", {
    onClick,
    style: {
      height: 32, padding: "0 12px", borderRadius: "var(--radius-m)",
      background: "var(--interactive-normal)", border: "1px solid var(--background-modifier-border)",
      color: "var(--text-normal)", fontSize: "var(--font-ui-medium)", fontWeight: 500,
      cursor: "pointer", display: "inline-flex", alignItems: "center", gap: 6,
      transition: "background-color var(--motion-duration-fast) var(--motion-ease)", ...style
    },
    onMouseEnter: (e) => e.currentTarget.style.background = "var(--interactive-hover)",
    onMouseLeave: (e) => e.currentTarget.style.background = "var(--interactive-normal)",
  }, children);
}

function PrimaryBtn({ children, onClick, style }) {
  return React.createElement("button", {
    onClick,
    style: {
      height: 32, padding: "0 14px", borderRadius: "var(--radius-m)",
      background: "var(--interactive-accent)", border: 0,
      color: "var(--text-on-accent)", fontSize: "var(--font-ui-medium)", fontWeight: 500,
      cursor: "pointer", display: "inline-flex", alignItems: "center", gap: 6,
      transition: "background-color var(--motion-duration-fast) var(--motion-ease)", ...style
    },
    onMouseEnter: (e) => e.currentTarget.style.background = "var(--interactive-accent-hover)",
    onMouseLeave: (e) => e.currentTarget.style.background = "var(--interactive-accent)",
  }, children);
}

function SegCtrl({ options, value, onChange }) {
  return React.createElement("div", {
    style: {
      height: 28, display: "inline-flex", background: "var(--background-modifier-hover)",
      borderRadius: "var(--radius-m)", padding: 2, gap: 2,
    }
  }, options.map(o => React.createElement("button", {
    key: o.value, onClick: () => onChange(o.value),
    style: {
      height: 24, padding: "0 12px", border: 0, borderRadius: "var(--radius-s)",
      background: value === o.value ? "var(--background-primary)" : "transparent",
      color: value === o.value ? "var(--text-normal)" : "var(--text-muted)",
      fontSize: "var(--font-ui-small)", fontWeight: 500, cursor: "pointer",
      boxShadow: value === o.value ? "var(--shadow-s)" : "none",
      transition: "all var(--motion-duration-fast) var(--motion-ease)",
    }
  }, o.label)));
}

function Toggle({ on, onChange }) {
  return React.createElement("button", {
    onClick: () => onChange(!on), role: "switch", "aria-checked": on,
    style: {
      width: 32, height: 18, borderRadius: 999, border: 0, padding: 2, cursor: "pointer",
      background: on ? "var(--interactive-accent)" : "var(--background-modifier-border)",
      transition: "background-color var(--motion-duration-fast) var(--motion-ease)",
      display: "flex", alignItems: "center",
    }
  }, React.createElement("div", {
    style: {
      width: 14, height: 14, borderRadius: 999, background: "var(--background-primary)",
      transform: on ? "translateX(14px)" : "translateX(0)",
      transition: "transform var(--motion-duration-fast) var(--motion-ease)",
    }
  }));
}

function InputField({ value, placeholder, readOnly, type = "text", style }) {
  return React.createElement("input", {
    type, value, placeholder, readOnly,
    style: {
      height: 32, padding: "0 10px", borderRadius: "var(--radius-s)",
      background: "var(--background-modifier-form-field)",
      border: "1px solid var(--background-modifier-border)",
      color: "var(--text-normal)", fontSize: "var(--font-ui-medium)",
      outline: "none", width: "100%", ...style
    }
  });
}

function Chip({ children, active, onClick }) {
  return React.createElement("button", {
    onClick,
    style: {
      padding: "2px 8px", borderRadius: 999, border: 0,
      background: active ? "var(--background-modifier-active)" : "var(--background-modifier-message)",
      color: active ? "var(--text-accent)" : "var(--text-muted)",
      fontSize: "var(--font-ui-smaller)", fontWeight: 500, cursor: "pointer",
    }
  }, children);
}

function StatusDot({ variant = "connected" }) {
  const colors = { connected: "hsl(92, 42%, 45%)", error: "var(--text-error)", idle: "var(--text-faint)" };
  return React.createElement("span", {
    style: { width: 6, height: 6, borderRadius: 999, background: colors[variant] || colors.idle, display: "inline-block", flexShrink: 0 }
  });
}

function Divider({ style }) {
  return React.createElement("div", { style: { height: 1, background: "var(--hr-color)", ...style } });
}

function Kbd({ children }) {
  return React.createElement("kbd", {
    style: {
      background: "var(--background-modifier-border)", color: "var(--text-muted)",
      fontSize: "var(--font-ui-smaller)", fontWeight: 500, fontFamily: "var(--font-interface)",
      padding: "1px 5px", borderRadius: "var(--radius-s)",
      border: "1px solid var(--background-modifier-border-hover)",
      borderBottomWidth: 2, minWidth: 16, display: "inline-block", textAlign: "center",
    }
  }, children);
}

Object.assign(window, { GhostBtn, SecondaryBtn, PrimaryBtn, SegCtrl, Toggle, InputField, Chip, StatusDot, Divider, Kbd });
