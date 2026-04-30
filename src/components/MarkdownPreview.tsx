import { memo } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeHighlight from "rehype-highlight";
import rehypeKatex from "rehype-katex";
import { open as shellOpen } from "@tauri-apps/plugin-shell";
import "katex/dist/katex.min.css";

interface Props {
  title: string | null;
  content: string;
  readableWidth: boolean;
  fontScale?: number;
}

// External-protocol links must escape the Tauri webview — without this,
// clicking an http(s) link navigates Forge's main window away from the
// editor. Vault-relative links (no scheme) are left to the default
// handler so callers above us can intercept them as tab opens later.
const EXTERNAL_RE = /^(https?:|mailto:|ftp:|tel:)/i;

const MARKDOWN_COMPONENTS = {
  a({ href, children, ...rest }: { href?: string; children?: React.ReactNode }) {
    return (
      <a
        href={href}
        onClick={(e) => {
          if (href && EXTERNAL_RE.test(href)) {
            e.preventDefault();
            void shellOpen(href).catch((err) =>
              console.warn("shell.open failed:", href, err),
            );
          }
        }}
        {...rest}
      >
        {children}
      </a>
    );
  },
};

function MarkdownPreview({
  title,
  content,
  readableWidth,
  fontScale = 1,
}: Props) {
  return (
    <div
      className={`markdown-preview-view flex-1 min-h-0 min-w-0 ${
        readableWidth ? "is-readable" : ""
      }`}
      style={{ fontSize: `calc(var(--font-text-size) * ${fontScale})` }}
    >
      <div className="markdown-preview-sizer">
        {title && (
          <h1 className="mb-4" style={{ marginTop: 0 }}>
            {title}
          </h1>
        )}
        <ReactMarkdown
          remarkPlugins={[remarkGfm, remarkMath]}
          rehypePlugins={[rehypeKatex, rehypeHighlight]}
          components={MARKDOWN_COMPONENTS}
        >
          {content}
        </ReactMarkdown>
      </div>
    </div>
  );
}

// Re-parsing markdown is expensive (remarkGfm + rehypeHighlight). Skip
// re-renders unless title/content/readableWidth actually change.
export default memo(MarkdownPreview);
