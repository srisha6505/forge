// In-app image viewer. Fit-to-viewport by default; click toggles 1:1.
// Wheel+modifier zooms around the cursor; drag pans when scaled past
// the container; keyboard pans/zooms when the body has focus.

import {
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { ZoomIn, ZoomOut, Maximize2, Square } from "lucide-react";
import { assetUrl, extOf } from "../lib/file-types";

interface Props {
  path: string;
  title: string | null;
}

const MIN_SCALE = 0.1;
const MAX_SCALE = 8;
const ZOOM_STEP = 1.25;

function clampScale(s: number): number {
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, s));
}

function ImageViewer({ path, title }: Props) {
  const url = assetUrl(path);
  const ext = extOf(path);

  const scrollRef = useRef<HTMLDivElement>(null);
  const imgRef = useRef<HTMLImageElement>(null);

  // Intrinsic image size (0 until onLoad fires).
  const [nat, setNat] = useState<{ w: number; h: number }>({ w: 0, h: 0 });
  // Container size; tracked so the fit-mode percentage readout is accurate.
  const [box, setBox] = useState<{ w: number; h: number }>({ w: 0, h: 0 });
  const [fit, setFit] = useState(true);
  // Explicit scale used when not in fit mode. 1 = pixel-for-pixel.
  const [scale, setScale] = useState(1);

  const fitScale =
    nat.w > 0 && nat.h > 0 && box.w > 0 && box.h > 0
      ? Math.min(box.w / nat.w, box.h / nat.h, 1)
      : 1;
  const effectiveScale = fit ? fitScale : scale;

  // Reset state when the path changes.
  useEffect(() => {
    setNat({ w: 0, h: 0 });
    setFit(true);
    setScale(1);
  }, [path]);

  // Track container size for the fit-scale readout.
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const measure = () => {
      const r = el.getBoundingClientRect();
      setBox({ w: r.width, h: r.height });
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const handleLoad = useCallback(() => {
    const el = imgRef.current;
    if (!el) return;
    setNat({ w: el.naturalWidth, h: el.naturalHeight });
  }, []);

  // Toggle fit <-> 1:1 on plain click. Suppress when the click is part
  // of a drag (mouseup after movement).
  const dragMovedRef = useRef(false);
  const handleClick = useCallback(() => {
    if (dragMovedRef.current) {
      dragMovedRef.current = false;
      return;
    }
    if (fit) {
      setScale(1);
      setFit(false);
    } else {
      setFit(true);
    }
  }, [fit]);

  // Zoom centered on (clientX, clientY) relative to the scroll container.
  // Math: pixel-under-cursor in image space stays put when scrollLeft is
  // adjusted by deltaScale * imageOffset.
  const zoomAt = useCallback(
    (clientX: number, clientY: number, factor: number) => {
      const el = scrollRef.current;
      if (!el || nat.w === 0) return;
      const prev = fit ? fitScale : scale;
      const next = clampScale(prev * factor);
      if (next === prev) return;

      const rect = el.getBoundingClientRect();
      // Cursor position within the scroll content.
      const cx = clientX - rect.left + el.scrollLeft;
      const cy = clientY - rect.top + el.scrollTop;
      const ratio = next / prev;

      setScale(next);
      setFit(false);

      // Apply the scroll adjustment after layout so the new image size
      // is committed.
      requestAnimationFrame(() => {
        if (!scrollRef.current) return;
        scrollRef.current.scrollLeft = cx * ratio - (clientX - rect.left);
        scrollRef.current.scrollTop = cy * ratio - (clientY - rect.top);
      });
    },
    [fit, fitScale, scale, nat.w],
  );

  // Wheel: Ctrl/Meta zooms; otherwise let the container scroll natively.
  const handleWheel = useCallback(
    (e: React.WheelEvent<HTMLDivElement>) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      e.preventDefault();
      const factor = e.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP;
      zoomAt(e.clientX, e.clientY, factor);
    },
    [zoomAt],
  );

  // Drag-to-pan. Only engages when content overflows the container.
  const dragRef = useRef<{
    x: number;
    y: number;
    sl: number;
    st: number;
  } | null>(null);
  const onMouseDown = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    const el = scrollRef.current;
    if (!el) return;
    const overflows =
      el.scrollWidth > el.clientWidth || el.scrollHeight > el.clientHeight;
    if (!overflows) return;
    dragRef.current = {
      x: e.clientX,
      y: e.clientY,
      sl: el.scrollLeft,
      st: el.scrollTop,
    };
    dragMovedRef.current = false;
    el.style.cursor = "grabbing";
  }, []);

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      const d = dragRef.current;
      const el = scrollRef.current;
      if (!d || !el) return;
      const dx = e.clientX - d.x;
      const dy = e.clientY - d.y;
      if (Math.abs(dx) + Math.abs(dy) > 3) dragMovedRef.current = true;
      el.scrollLeft = d.sl - dx;
      el.scrollTop = d.st - dy;
    };
    const onUp = () => {
      dragRef.current = null;
      const el = scrollRef.current;
      if (el) el.style.cursor = "";
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, []);

  // Keyboard shortcuts on the focused container.
  const onKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      const el = scrollRef.current;
      if (!el) return;
      const center = () => {
        const r = el.getBoundingClientRect();
        return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
      };
      switch (e.key) {
        case "+":
        case "=": {
          const c = center();
          zoomAt(c.x, c.y, ZOOM_STEP);
          e.preventDefault();
          break;
        }
        case "-":
        case "_": {
          const c = center();
          zoomAt(c.x, c.y, 1 / ZOOM_STEP);
          e.preventDefault();
          break;
        }
        case "0":
          setScale(1);
          setFit(false);
          e.preventDefault();
          break;
        case "f":
        case "F":
          setFit((v) => !v);
          e.preventDefault();
          break;
        case "ArrowLeft":
          el.scrollLeft -= 60;
          e.preventDefault();
          break;
        case "ArrowRight":
          el.scrollLeft += 60;
          e.preventDefault();
          break;
        case "ArrowUp":
          el.scrollTop -= 60;
          e.preventDefault();
          break;
        case "ArrowDown":
          el.scrollTop += 60;
          e.preventDefault();
          break;
        case "Home":
          el.scrollTo({ left: 0, top: 0 });
          e.preventDefault();
          break;
        case "End":
          el.scrollTo({ left: el.scrollWidth, top: el.scrollHeight });
          e.preventDefault();
          break;
      }
    },
    [zoomAt],
  );

  // Toolbar button helpers.
  const zoomCenter = useCallback(
    (factor: number) => {
      const el = scrollRef.current;
      if (!el) return;
      const r = el.getBoundingClientRect();
      zoomAt(r.left + r.width / 2, r.top + r.height / 2, factor);
    },
    [zoomAt],
  );

  const pct = Math.round(effectiveScale * 100);
  const dims = nat.w > 0 ? `${nat.w} × ${nat.h}` : "";

  // Sized image styles. In fit mode we use object-fit so the browser
  // handles aspect; in explicit mode we set width/height to natural * scale
  // so the scroll container can overflow.
  const imgStyle: React.CSSProperties = fit
    ? { maxWidth: "100%", maxHeight: "100%", objectFit: "contain" }
    : {
        width: nat.w > 0 ? nat.w * scale : undefined,
        height: nat.h > 0 ? nat.h * scale : undefined,
        maxWidth: "none",
        maxHeight: "none",
      };

  // Subtle theme-agnostic checkerboard. Two near-equal greys with low
  // opacity so it sits behind transparent PNGs without fighting the theme.
  const checkerStyle: React.CSSProperties = {
    backgroundColor: "var(--background-secondary)",
    backgroundImage:
      "linear-gradient(45deg, rgba(127,127,127,0.08) 25%, transparent 25%), " +
      "linear-gradient(-45deg, rgba(127,127,127,0.08) 25%, transparent 25%), " +
      "linear-gradient(45deg, transparent 75%, rgba(127,127,127,0.08) 75%), " +
      "linear-gradient(-45deg, transparent 75%, rgba(127,127,127,0.08) 75%)",
    backgroundSize: "16px 16px",
    backgroundPosition: "0 0, 0 8px, 8px -8px, -8px 0px",
  };

  const btn =
    "h-7 w-7 inline-flex items-center justify-center rounded " +
    "text-[var(--text-muted)] hover:text-[var(--text-normal)] " +
    "hover:bg-[var(--background-modifier-hover)] transition-colors";

  return (
    <div className="workspace-leaf-content flex-1 min-h-0 min-w-0 flex flex-col bg-[var(--background-primary)]">
      <div className="flex-shrink-0 pt-10 pb-2 px-16 w-full">
        <h1 className="text-[34px] font-bold text-[var(--text-title-h1)] leading-[1.15] tracking-tight truncate">
          {title}
        </h1>
      </div>

      <div className="flex-shrink-0 px-16 pb-2 flex items-center justify-end gap-2 text-[12px] text-[var(--text-muted)]">
        {dims && (
          <span className="font-mono tabular-nums">
            {dims}
            {ext && <span className="ml-2 uppercase opacity-70">{ext}</span>}
          </span>
        )}
        <span className="font-mono tabular-nums w-12 text-right">{pct}%</span>
        <div className="flex items-center gap-0.5 ml-2">
          <button
            className={btn}
            title="Zoom out (-)"
            onClick={() => zoomCenter(1 / ZOOM_STEP)}
          >
            <ZoomOut size={14} />
          </button>
          <button
            className={`${btn} ${fit ? "text-[var(--text-accent)]" : ""}`}
            title="Fit to viewport (f)"
            onClick={() => setFit((v) => !v)}
          >
            <Maximize2 size={14} />
          </button>
          <button
            className={btn}
            title="Actual size, 1:1 (0)"
            onClick={() => {
              setScale(1);
              setFit(false);
            }}
          >
            <Square size={14} />
          </button>
          <button
            className={btn}
            title="Zoom in (+)"
            onClick={() => zoomCenter(ZOOM_STEP)}
          >
            <ZoomIn size={14} />
          </button>
        </div>
      </div>

      <div
        ref={scrollRef}
        tabIndex={0}
        onWheel={handleWheel}
        onMouseDown={onMouseDown}
        onClick={handleClick}
        onKeyDown={onKeyDown}
        className="flex-1 min-h-0 min-w-0 overflow-auto outline-none flex items-center justify-center"
        style={checkerStyle}
      >
        <img
          ref={imgRef}
          src={url}
          alt={title ?? ""}
          onLoad={handleLoad}
          draggable={false}
          style={imgStyle}
          className="select-none block"
        />
      </div>
    </div>
  );
}

export default memo(ImageViewer);
