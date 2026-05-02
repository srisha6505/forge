import { memo } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeHighlight from "rehype-highlight";
import rehypeKatex from "rehype-katex";
import { open as shellOpen } from "@tauri-apps/plugin-shell";

// Route external-protocol clicks through the OS shell so the chat
// panel doesn't navigate the Tauri webview when a user clicks a
// citation or reference URL.
const EXTERNAL_RE = /^(https?:|mailto:|ftp:|tel:)/i;

const CHAT_MD_COMPONENTS = {
  a({ href, children, ...rest }: { href?: string; children?: React.ReactNode }) {
    return (
      <a
        href={href}
        onClick={(e) => {
          if (href && EXTERNAL_RE.test(href)) {
            e.preventDefault();
            void shellOpen(href).catch(() => {});
          }
        }}
        {...rest}
      >
        {children}
      </a>
    );
  },
};

interface Props {
  body: string;
}

// Isolated so MessageBlock can React.lazy() this. The whole unified
// pipeline (remark-gfm + remark-math + rehype-katex + rehype-highlight,
// ~150 KB gzipped between them) only loads when a finished assistant
// turn renders, never during streaming and never if the user hasn't
// opened a chat at all. Once loaded, it's cached for the rest of the
// session — every subsequent finished turn renders instantly.
function ChatMarkdownImpl({ body }: Props) {
  return (
    <ReactMarkdown
      remarkPlugins={[remarkGfm, remarkMath]}
      rehypePlugins={[rehypeKatex, rehypeHighlight]}
      components={CHAT_MD_COMPONENTS}
    >
      {body}
    </ReactMarkdown>
  );
}

export default memo(ChatMarkdownImpl);
