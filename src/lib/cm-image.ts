// Inline image rendering for `![alt](path)` markdown images. Walks the
// syntax tree, replaces each Image node with a block widget that loads
// the resolved file via Tauri's asset protocol.
//
// Path resolution:
//   - http:// / https:// / data: → use as-is
//   - absolute fs path           → convertFileSrc
//   - relative path              → resolved against current doc's dir,
//                                  then convertFileSrc
//
// The current doc's path is read at build-time from a getter passed in
// at extension creation. Editor.tsx wires a fresh-ref so the same
// extension instance keeps working across file switches.

import {
  syntaxTree,
  syntaxTreeAvailable,
} from "@codemirror/language";
import {
  type EditorState,
  type Extension,
  RangeSetBuilder,
  StateField,
} from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  EditorView,
  WidgetType,
} from "@codemirror/view";
import { assetUrl } from "./file-types";
import { activeLinesField } from "./cm-active-lines";

function isAbsolutePath(p: string): boolean {
  return p.startsWith("/") || /^[a-zA-Z]:[\\/]/.test(p);
}

function isExternalUrl(p: string): boolean {
  return /^(https?:|data:|asset:|file:)/i.test(p);
}

function dirname(p: string): string {
  const i = Math.max(p.lastIndexOf("/"), p.lastIndexOf("\\"));
  return i >= 0 ? p.slice(0, i) : "";
}

// Resolve `..` and `.` segments without leaning on a Node-only API.
// Tauri's asset protocol expects a clean canonical path; passing an
// un-normalised `/a/b/c/../foo` silently 404s in WebKitGTK, which is
// exactly what was producing "Image failed to load" for every
// `../plots/...` reference in long-form notes.
function normalizePath(p: string): string {
  const isAbs = p.startsWith("/");
  const isWinAbs = /^[a-zA-Z]:[\\/]/.test(p);
  const winDrive = isWinAbs ? p.slice(0, 2) : "";
  const body = isWinAbs ? p.slice(2) : p;
  const parts = body.split(/[\\/]+/);
  const out: string[] = [];
  for (const part of parts) {
    if (part === "" || part === ".") continue;
    if (part === "..") {
      // Pop only when the stack has a "real" segment to consume.
      // For absolute paths, leading `..` segments past the root are
      // collapsed to nothing. For relative paths (no leading slash),
      // we preserve them so e.g. `../foo` from a relative base
      // remains relative.
      if (out.length > 0 && out[out.length - 1] !== "..") out.pop();
      else if (!isAbs && !isWinAbs) out.push("..");
      continue;
    }
    out.push(part);
  }
  if (isWinAbs) return winDrive + "/" + out.join("/");
  return (isAbs ? "/" : "") + out.join("/");
}

export function resolveImagePath(src: string, docPath: string | null): string {
  const trimmed = src.trim();
  if (!trimmed) return "";
  if (isExternalUrl(trimmed)) return trimmed;
  // Strip URL fragment / query before joining.
  const clean = trimmed.replace(/[?#].*$/, "");
  if (isAbsolutePath(clean)) return assetUrl(normalizePath(clean));
  if (!docPath) return clean;
  const base = dirname(docPath);
  const joined = base ? `${base}/${clean}` : clean;
  return assetUrl(normalizePath(joined));
}

// Module-level cache of image metadata (intrinsic dimensions + load
// state) keyed by URL. CM6 destroys widget DOM when a line scrolls
// out of viewport and re-creates it via toDOM() when it scrolls back
// in — without this cache, every re-mount re-decodes the image and
// the user sees a layout flash. The cache lets the new <img> reserve
// the correct size IMMEDIATELY (browsers paint cached images on the
// same paint frame), so re-mount is invisible.
//
// Exported so cm-wikilinks can reuse the same machinery for image
// embeds (`![[file.png]]`) — sharing the cache means a file
// referenced both as a markdown image and a wikilink embed only
// loads once.
export type ImageMeta = {
  width: number;
  height: number;
  loaded: boolean;
  failed: boolean;
};
const imageMetaCache = new Map<string, ImageMeta>();
const probeCache = new Map<string, HTMLImageElement>();

export function getOrCreateProbe(url: string): ImageMeta {
  let meta = imageMetaCache.get(url);
  if (meta) return meta;
  meta = { width: 0, height: 0, loaded: false, failed: false };
  imageMetaCache.set(url, meta);

  if (!url) {
    meta.failed = true;
    return meta;
  }

  const probe = new Image();
  probeCache.set(url, probe);
  probe.addEventListener("load", () => {
    meta!.width = probe.naturalWidth;
    meta!.height = probe.naturalHeight;
    meta!.loaded = true;
  });
  probe.addEventListener("error", () => {
    meta!.failed = true;
  });
  probe.src = url;
  return meta;
}

export function imageMetaFor(url: string): ImageMeta | undefined {
  return imageMetaCache.get(url);
}

const IMAGE_EXT_RE = /\.(png|jpe?g|gif|webp|bmp|svg|avif|tiff?|ico)$/i;
export function isImagePath(p: string): boolean {
  return IMAGE_EXT_RE.test(p);
}

class ImageWidget extends WidgetType {
  constructor(
    readonly alt: string,
    readonly url: string,
    readonly raw: string,
  ) {
    super();
  }
  eq(other: ImageWidget) {
    return other.url === this.url && other.alt === this.alt;
  }
  // CM6 hands toDOM the EditorView so we can call requestMeasure when
  // the image's intrinsic size becomes known. Without that hook, CM6
  // caches the line height it measured at first paint (when img is
  // 0x0) and the outer scroller's scrollHeight stays too short — the
  // user can't reach the bottom of the document.
  toDOM(view: EditorView) {
    const wrap = document.createElement("div");
    wrap.className = "cm-image-widget";
    const renderErrorLabel = (message: string) => {
      wrap.classList.add("is-error");
      const label = document.createElement("div");
      label.className = "cm-image-error-label";
      label.textContent = message;
      wrap.appendChild(label);
    };
    if (!this.url) {
      renderErrorLabel(`Image not found: ${this.raw}`);
      return wrap;
    }

    const meta = getOrCreateProbe(this.url);
    if (meta.failed) {
      renderErrorLabel(`Image failed to load: ${this.raw}`);
      return wrap;
    }

    const img = document.createElement("img");
    img.alt = this.alt;
    // async decode keeps scrolling buttery — sync would block the
    // main thread on every viewport-driven re-mount. The browser's
    // image cache and our pre-set width/height attrs cover the
    // "blank frame" risk that async would otherwise introduce.
    img.decoding = "async";
    // Pin width/height attributes BEFORE assigning src so layout
    // reserves the correct box on the very first paint frame. CM6
    // measures via getBoundingClientRect; an undersized initial paint
    // makes the line shorter than the final image and the bottom of
    // the document becomes unreachable.
    if (meta.loaded && meta.width && meta.height) {
      img.width = meta.width;
      img.height = meta.height;
    }
    img.src = this.url;

    // CM6 viewport-virtualizes line DOM, so a single image is mounted
    // and unmounted many times during a long scroll. Only attach load
    // / error listeners on the FIRST mount (when the probe hasn't
    // settled). Subsequent mounts already have correct width/height
    // attrs baked in from the cached meta — they need no measurement
    // and no event handlers, which keeps each scroll-induced mount
    // cheap and avoids the visible "blink" the user reported.
    if (!meta.loaded) {
      img.addEventListener(
        "load",
        () => {
          meta.width = img.naturalWidth;
          meta.height = img.naturalHeight;
          meta.loaded = true;
          if (!img.getAttribute("width")) img.width = meta.width;
          if (!img.getAttribute("height")) img.height = meta.height;
          // First load — CM6 learns the new line height now so total
          // content height grows. On every subsequent mount we skip
          // this entirely.
          view.requestMeasure();
        },
        { once: true },
      );
      img.addEventListener(
        "error",
        () => {
          meta.failed = true;
          wrap.classList.add("is-error");
          // Append a sibling error label rather than nuking children
          // (which would collapse height and snap the scroller back).
          img.style.display = "none";
          const label = document.createElement("div");
          label.className = "cm-image-error-label";
          label.textContent = `Image failed to load: ${this.raw}`;
          wrap.appendChild(label);
        },
        { once: true },
      );
    }
    wrap.appendChild(img);

    if (this.alt) {
      const cap = document.createElement("div");
      cap.className = "cm-image-caption";
      cap.textContent = this.alt;
      wrap.appendChild(cap);
    }
    return wrap;
  }
  // In-place update when only the alt text or other ImageWidget fields
  // change for the same URL; lets CM6 reuse the cached <img> instead
  // of destroying and re-decoding.
  updateDOM(dom: HTMLElement): boolean {
    const img = dom.querySelector("img");
    if (!img) return false;
    if (img.src !== this.url) {
      // URL changed: easier to let CM6 recreate so all the load
      // listeners and probe wiring start fresh against the new src.
      return false;
    }
    if (img.alt !== this.alt) img.alt = this.alt;
    const cap = dom.querySelector(".cm-image-caption");
    if (cap && cap.textContent !== this.alt) cap.textContent = this.alt;
    return true;
  }
  ignoreEvent() {
    return false;
  }
}

interface DecoItem {
  from: number;
  to: number;
  deco: Decoration;
}

function buildAll(
  state: EditorState,
  docPath: string | null,
): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const doc = state.doc;
  const tree = syntaxTree(state);
  const items: DecoItem[] = [];

  const activeLines = state.field(activeLinesField);

  tree.iterate({
    enter: (node) => {
      if (node.name !== "Image") return;
      const startLine = doc.lineAt(node.from);
      if (activeLines.has(startLine.number)) return false;

      const text = doc.sliceString(node.from, node.to);
      // Image syntax: ![alt](src "title"?). Tolerate empty alt and
      // whitespace inside the parens.
      const m = /^!\[([^\]]*)\]\(\s*([^\s)]+)(?:\s+"[^"]*")?\s*\)/.exec(text);
      if (!m) return false;
      const alt = m[1];
      const src = m[2];
      const url = resolveImagePath(src, docPath);

      // Pre-warm the probe NOW, while we walk the tree, instead of
      // waiting for a widget to mount. CM6 mounts widgets only when
      // their line enters the viewport buffer — so a widget mounting
      // mid-scroll would otherwise be the trigger for the network
      // fetch + intrinsic-size discovery + view.requestMeasure, and
      // that cascade is exactly what produced the visible "flash"
      // a few scrolls ahead of the image. By kicking off probes at
      // build time, every probe starts loading the moment the doc is
      // parsed; by the time the user's scroll reaches buffer range,
      // the cached width/height are already known and no measure or
      // layout shift is needed.
      if (url) getOrCreateProbe(url);

      items.push({
        from: node.from,
        to: node.to,
        deco: Decoration.replace({
          widget: new ImageWidget(alt, url, text),
        }),
      });
      return false;
    },
  });

  items.sort((a, b) => a.from - b.from);
  for (const item of items) builder.add(item.from, item.to, item.deco);
  return builder.finish();
}

const imageTheme = EditorView.theme({
  ".cm-image-widget": {
    display: "block",
    margin: "12px auto",
    maxWidth: "100%",
    textAlign: "center",
    // `contain: layout` only — `contain: paint` was suspected of
    // confusing WebKitGTK's getBoundingClientRect path that CM6 uses
    // for line-height measurement, which underreserved scroll height.
    // Layout containment alone still isolates reflow without paint
    // boundary side-effects.
    contain: "layout",
  },
  ".cm-image-widget img": {
    maxWidth: "100%",
    maxHeight: "560px",
    height: "auto",
    borderRadius: "6px",
    display: "block",
    margin: "0 auto",
    background: "var(--background-secondary)",
  },
  ".cm-image-caption": {
    fontSize: "11px",
    color: "var(--text-muted)",
    marginTop: "6px",
    fontStyle: "italic",
  },
  ".cm-image-widget.is-error": {
    background: "var(--background-modifier-error)",
    borderRadius: "4px",
    padding: "8px 12px",
    textAlign: "left",
  },
  ".cm-image-error-label": {
    fontFamily: "var(--font-monospace)",
    fontSize: "11px",
    color: "var(--text-error)",
    whiteSpace: "pre-wrap",
    wordBreak: "break-all",
  },
});

export function createImagePlugin(
  getDocPath: () => string | null,
): Extension {
  // The state field re-reads getDocPath() on every rebuild so file
  // switches resolve correctly without an extension reconfigure.
  const field = StateField.define<DecorationSet>({
    create(state) {
      if (!syntaxTreeAvailable(state)) return Decoration.none;
      try {
        return buildAll(state, getDocPath());
      } catch (e) {
        console.error("[cm-image] build failed (create):", e);
        return Decoration.none;
      }
    },
    update(value, tr) {
      const treeChanged = syntaxTree(tr.startState) !== syntaxTree(tr.state);
      const activeChanged =
        tr.startState.field(activeLinesField, false) !==
        tr.state.field(activeLinesField, false);
      if (!tr.docChanged && !treeChanged && !activeChanged) return value;
      if (!syntaxTreeAvailable(tr.state)) return Decoration.none;
      try {
        return buildAll(tr.state, getDocPath());
      } catch (e) {
        console.error("[cm-image] build failed (update):", e);
        return value;
      }
    },
    provide: (f) => EditorView.decorations.from(f),
  });

  return [field, imageTheme];
}
