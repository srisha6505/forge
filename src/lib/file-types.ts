// Extension-based file classification for the workspace. The App shell
// reads this to decide which viewer to render (Editor / MarkdownPreview
// / PdfViewer / ImageViewer / LatexViewer). Keep in sync with the
// `is_viewable_ext` allowlist in `src-tauri/src/commands.rs`.

import { convertFileSrc } from "@tauri-apps/api/core";

export type FileKind = "markdown" | "pdf" | "image" | "latex" | "docx" | "other";

export function extOf(path: string): string {
  const dot = path.lastIndexOf(".");
  if (dot < 0 || dot === path.length - 1) return "";
  return path.slice(dot + 1).toLowerCase();
}

export function fileKind(path: string): FileKind {
  const e = extOf(path);
  if (e === "md" || e === "markdown" || e === "mdx") return "markdown";
  if (e === "pdf") return "pdf";
  if (e === "tex") return "latex";
  if (e === "docx" || e === "doc" || e === "odt") return "docx";
  if (
    e === "png" || e === "jpg" || e === "jpeg" || e === "gif" ||
    e === "webp" || e === "bmp" || e === "svg" || e === "ico" ||
    e === "avif" || e === "tif" || e === "tiff"
  ) return "image";
  return "other";
}

/** Binary formats the frontend should NOT read via the `readFile`
 *  (string) Tauri command -- they go through `convertFileSrc` instead. */
export function isBinaryKind(kind: FileKind): boolean {
  return kind === "pdf" || kind === "image" || kind === "docx";
}

/** Strip extension from a filename for tab titles. Only strips
 *  recognised viewable extensions so titles for unknown files keep
 *  their suffix. */
export function stripViewableExt(name: string): string {
  return name.replace(
    /\.(md|markdown|mdx|pdf|tex|png|jpe?g|gif|webp|bmp|svg|ico|avif|tiff?|docx?|odt)$/i,
    "",
  );
}

/** Convert an absolute filesystem path to an `asset://` URL that the
 *  webview can load directly (img src, pdfjs, etc). Requires
 *  `app.security.assetProtocol.enable = true` in tauri.conf.json. */
export function assetUrl(path: string): string {
  return convertFileSrc(path);
}
