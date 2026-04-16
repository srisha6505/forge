import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";

interface Props {
  title: string | null;
  content: string;
  readableWidth: boolean;
}

export default function MarkdownPreview({
  title,
  content,
  readableWidth,
}: Props) {
  return (
    <div
      className={`markdown-preview-view ${readableWidth ? "is-readable" : ""}`}
    >
      <div className="markdown-preview-sizer">
        {title && (
          <h1 className="mb-4" style={{ marginTop: 0 }}>
            {title}
          </h1>
        )}
        <ReactMarkdown
          remarkPlugins={[remarkGfm]}
          rehypePlugins={[rehypeHighlight]}
        >
          {content}
        </ReactMarkdown>
      </div>
    </div>
  );
}
