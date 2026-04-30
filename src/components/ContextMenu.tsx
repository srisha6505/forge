import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { ReactNode, MouseEvent as ReactMouseEvent } from "react";
import { createPortal } from "react-dom";

// Themed right-click menu, rendered into a body-level portal so it
// doesn't get clipped by overflow:hidden parents (sidebars, panels).
//
// We picked React rendering over Tauri 2's native menu API because GTK
// menus on Linux ignore the app's theme entirely (plain gray boxes,
// no warm-amber palette, no border-radius). On Mac/Windows native
// would look correct, but we'd need a per-platform branch for one
// component — not worth it. The pure-React version themes correctly on
// every platform and the open latency cost (~5ms) is invisible.

export type MenuItem =
  | { kind: "sep" }
  | {
      kind?: "item";
      label: string;
      icon?: ReactNode;
      onClick: () => void;
      disabled?: boolean;
      destructive?: boolean;
      hint?: string;
    };

interface Props {
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
}

export function ContextMenu({ x, y, items, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  // Start at the click coords; clamp to viewport after the menu measures.
  const [pos, setPos] = useState<{ left: number; top: number }>({
    left: x,
    top: y,
  });

  // Outside-click + Escape dismiss. mousedown (not click) so the menu
  // closes BEFORE the document handler fires for the next interaction.
  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    // capture: true so we beat any inner stopPropagation handlers.
    document.addEventListener("mousedown", onDown, true);
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("mousedown", onDown, true);
      document.removeEventListener("keydown", onKey, true);
    };
  }, [onClose]);

  // After first paint, measure the menu and clamp into the viewport.
  // Layout effect so the user never sees the off-screen flash.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const margin = 6;
    let left = x;
    let top = y;
    if (left + r.width > vw - margin) left = vw - r.width - margin;
    if (top + r.height > vh - margin) top = vh - r.height - margin;
    if (left < margin) left = margin;
    if (top < margin) top = margin;
    setPos({ left, top });
  }, [x, y, items]);

  const stop = (e: ReactMouseEvent) => e.stopPropagation();

  return createPortal(
    <div
      ref={ref}
      className="forge-context-menu"
      style={{ left: pos.left, top: pos.top }}
      onMouseDown={stop}
      onContextMenu={(e) => e.preventDefault()}
      role="menu"
    >
      {items.map((it, i) => {
        if (it.kind === "sep") {
          return <div key={i} className="forge-context-menu__sep" role="separator" />;
        }
        return (
          <button
            key={i}
            type="button"
            disabled={!!it.disabled}
            onClick={() => {
              if (it.disabled) return;
              try {
                it.onClick();
              } finally {
                onClose();
              }
            }}
            className={
              "forge-context-menu__item" +
              (it.destructive ? " is-destructive" : "")
            }
            role="menuitem"
          >
            {it.icon ? (
              <span className="forge-context-menu__icon">{it.icon}</span>
            ) : (
              <span className="forge-context-menu__icon" aria-hidden />
            )}
            <span className="forge-context-menu__label">{it.label}</span>
            {it.hint && (
              <span className="forge-context-menu__hint">{it.hint}</span>
            )}
          </button>
        );
      })}
    </div>,
    document.body,
  );
}
