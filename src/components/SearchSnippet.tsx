import { memo, useMemo } from "react";

// Inline-only markdown renderer for search-result rows. Hand-rolled char
// scan instead of regex — predictable performance, no backtracking, no
// lookbehinds, no regex-engine quirks across webview versions. Works
// out to a single forward pass over the snippet, O(n).
//
// Supported inline marks:
//   `code`            → <code>
//   **bold**          → <strong>
//   *italic*          → <em>
//   [[wiki|alias]]    → text (alias if present)
//   [text](url)       → text (link content only)
//   ## heading        → markers stripped at start of line
// Everything else is rendered as plain text.
//
// Matched query terms are highlighted with <mark> AFTER inline markdown
// is resolved, so a term inside `code` still highlights and inherits
// the styled segment's colour.

type SegKind = "text" | "code" | "bold" | "italic";
interface Seg {
  kind: SegKind;
  text: string;
}

const HEADING_RE = /^\s*#{1,6}\s+/gm;
const NEWLINE_COLLAPSE_RE = /[ \t]*\n+[ \t]*/g;

function preclean(src: string): string {
  return src
    .replace(HEADING_RE, "")
    .replace(NEWLINE_COLLAPSE_RE, " ")
    .trim();
}

function isWS(ch: string): boolean {
  return ch === " " || ch === "\t" || ch === "\n";
}

// Char-by-char inline parser. Each branch advances `i` past a recognised
// span and pushes a typed segment; otherwise we extend the current text
// run by one char. No regex engine, no backtracking.
function parseInline(src: string): Seg[] {
  const out: Seg[] = [];
  if (!src) return out;

  let i = 0;
  let runStart = 0;
  const n = src.length;

  const flushText = (end: number) => {
    if (end > runStart) {
      out.push({ kind: "text", text: src.slice(runStart, end) });
    }
  };

  while (i < n) {
    const c = src.charCodeAt(i);

    // `code`  (96 = `)
    if (c === 96) {
      const close = src.indexOf("`", i + 1);
      if (close > i + 1 && !src.slice(i + 1, close).includes("\n")) {
        flushText(i);
        out.push({ kind: "code", text: src.slice(i + 1, close) });
        i = close + 1;
        runStart = i;
        continue;
      }
    }

    // **bold**  (42 = *)
    else if (c === 42 && src.charCodeAt(i + 1) === 42) {
      // closing `**` must be at i+3 or later (need ≥1 char inside)
      let j = i + 2;
      let close = -1;
      while (j < n - 1) {
        if (src.charCodeAt(j) === 10) break; // newline aborts
        if (src.charCodeAt(j) === 42 && src.charCodeAt(j + 1) === 42) {
          close = j;
          break;
        }
        j++;
      }
      if (close > i + 2) {
        flushText(i);
        out.push({ kind: "bold", text: src.slice(i + 2, close) });
        i = close + 2;
        runStart = i;
        continue;
      }
    }

    // *italic* — single `*`. Disambiguate from list bullets: the char
    // immediately after `*` must NOT be whitespace and must NOT be `*`.
    else if (c === 42) {
      const next = src[i + 1];
      if (next && next !== "*" && !isWS(next)) {
        let j = i + 1;
        let close = -1;
        while (j < n) {
          const cj = src.charCodeAt(j);
          if (cj === 10) break;
          if (cj === 42 && src.charCodeAt(j + 1) !== 42) {
            close = j;
            break;
          }
          j++;
        }
        if (close > i + 1) {
          flushText(i);
          out.push({ kind: "italic", text: src.slice(i + 1, close) });
          i = close + 1;
          runStart = i;
          continue;
        }
      }
    }

    // [[wikilink]] or [[wikilink|alias]]  (91 = [, 93 = ])
    else if (c === 91 && src.charCodeAt(i + 1) === 91) {
      const close = src.indexOf("]]", i + 2);
      if (close > i + 2) {
        flushText(i);
        const inner = src.slice(i + 2, close);
        const pipe = inner.indexOf("|");
        const text = pipe >= 0 ? inner.slice(pipe + 1) : inner;
        out.push({ kind: "text", text });
        i = close + 2;
        runStart = i;
        continue;
      }
    }

    // [text](url)
    else if (c === 91) {
      const closeBracket = src.indexOf("]", i + 1);
      if (closeBracket > i + 1 && src.charCodeAt(closeBracket + 1) === 40) {
        const closeParen = src.indexOf(")", closeBracket + 2);
        if (closeParen > closeBracket + 1) {
          flushText(i);
          out.push({
            kind: "text",
            text: src.slice(i + 1, closeBracket),
          });
          i = closeParen + 1;
          runStart = i;
          continue;
        }
      }
    }

    i++;
  }
  flushText(n);
  return out;
}

function escapeRe(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function buildTermsRe(terms: string[]): RegExp | null {
  const valid = terms.filter((t) => t && t.trim().length > 0);
  if (valid.length === 0) return null;
  const sorted = [...valid].sort((a, b) => b.length - a.length);
  return new RegExp(`(${sorted.map(escapeRe).join("|")})`, "gi");
}

function highlightText(
  text: string,
  termsRe: RegExp | null,
  keyPrefix: string,
) {
  if (!termsRe) return [text];
  const parts = text.split(termsRe);
  if (parts.length === 1) return [text];
  return parts.map((part, i) => {
    if (i % 2 === 1) {
      return (
        <mark
          key={`${keyPrefix}-${i}`}
          className="bg-[var(--text-accent)]/25 text-[var(--text-accent)] rounded px-[2px]"
          style={{ fontWeight: "inherit" }}
        >
          {part}
        </mark>
      );
    }
    return part;
  });
}

interface Props {
  source: string;
  highlightTerms?: string[];
  /** When true, renders a single-line plain version (no inline marks). */
  plain?: boolean;
  className?: string;
}

function SearchSnippetImpl({
  source,
  highlightTerms,
  plain = false,
  className = "",
}: Props) {
  const cleaned = useMemo(() => preclean(source), [source]);
  const segments = useMemo(
    () => (plain ? null : parseInline(cleaned)),
    [plain, cleaned],
  );
  const termsKey = (highlightTerms ?? []).join("|");
  const termsRe = useMemo(
    () => buildTermsRe(highlightTerms ?? []),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [termsKey],
  );

  if (plain) {
    return (
      <span className={className}>
        {highlightText(cleaned, termsRe, "p")}
      </span>
    );
  }

  return (
    <span className={className}>
      {segments!.map((seg, i) => {
        const inner = highlightText(seg.text, termsRe, `s${i}`);
        if (seg.kind === "bold") {
          return (
            <strong key={i} className="font-bold">
              {inner}
            </strong>
          );
        }
        if (seg.kind === "italic") {
          return (
            <em key={i} className="italic">
              {inner}
            </em>
          );
        }
        if (seg.kind === "code") {
          return (
            <code
              key={i}
              className="px-1 rounded bg-[var(--background-modifier-form-field)] text-[var(--text-normal)] font-mono text-[0.92em]"
            >
              {inner}
            </code>
          );
        }
        return <span key={i}>{inner}</span>;
      })}
    </span>
  );
}

export const SearchSnippet = memo(SearchSnippetImpl);
