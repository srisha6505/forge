import { useCallback, useEffect, useRef } from "react";

interface Props {
  onResize: (delta: number) => void;
  onDone?: () => void;
  /** Side we are attached to so the cursor feels right. */
  side?: "left" | "right";
}

/**
 * A thin vertical drag handle. On mouseDown it captures pointer events and
 * calls `onResize` with the mouse delta relative to the last event until
 * the user releases. Keeps overall state in the parent so the parent can
 * clamp the resulting width.
 */
export default function ResizeHandle({ onResize, onDone, side = "right" }: Props) {
  const startXRef = useRef<number | null>(null);
  const dragging = useRef(false);

  const onMouseMove = useCallback(
    (e: MouseEvent) => {
      if (!dragging.current || startXRef.current === null) return;
      const delta = e.clientX - startXRef.current;
      startXRef.current = e.clientX;
      onResize(delta);
    },
    [onResize],
  );

  const stopDrag = useCallback(() => {
    if (!dragging.current) return;
    dragging.current = false;
    startXRef.current = null;
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
    onDone?.();
  }, [onDone]);

  useEffect(() => {
    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", stopDrag);
    return () => {
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", stopDrag);
    };
  }, [onMouseMove, stopDrag]);

  const onMouseDown = (e: React.MouseEvent<HTMLDivElement>) => {
    e.preventDefault();
    dragging.current = true;
    startXRef.current = e.clientX;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  };

  return (
    <div
      onMouseDown={onMouseDown}
      className={`workspace-leaf-resize-handle w-[3px] cursor-col-resize flex-shrink-0 hover:bg-[var(--interactive-accent)] transition-colors ${
        side === "right" ? "" : ""
      }`}
      style={{ background: "transparent" }}
    />
  );
}
