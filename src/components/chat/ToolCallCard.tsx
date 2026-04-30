import { useState } from "react";
import {
  ChevronRight,
  CircleCheck,
  CircleX,
  Search,
  FileText,
  FolderOpen,
  Pencil,
  FilePlus,
  Trash2,
  Globe,
  Wrench,
} from "lucide-react";

export interface ToolCallProps {
  name: string;
  args: string;
  result?: string;
  isError?: boolean;
}

const TOOL_ICON: Record<string, typeof Wrench> = {
  search_vault: Search,
  grep_vault: Search,
  read_file: FileText,
  read_section: FileText,
  list_files: FolderOpen,
  write_file: FilePlus,
  edit_file: Pencil,
  rename_file: Pencil,
  delete_file: Trash2,
  web_search: Globe,
};

export function ToolCallCard({ name, args, result, isError }: ToolCallProps) {
  const [open, setOpen] = useState(false);
  const Icon = TOOL_ICON[name] ?? Wrench;
  const done = result !== undefined;
  const summary = summariseArgs(name, args);

  const status = !done ? "running" : isError ? "error" : "done";

  return (
    <div className="forge-tool-card">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="forge-tool-card__head"
        aria-expanded={open}
      >
        <ChevronRight
          size={12}
          className={`forge-tool-card__chevron ${open ? "is-open" : ""}`}
        />
        <Icon size={12} className="forge-tool-card__icon" />
        <span className="forge-tool-card__name">{name}</span>
        {summary && (
          <span className="forge-tool-card__summary">{summary}</span>
        )}
        <ToolStatus status={status} />
      </button>
      {open && (
        <div className="forge-tool-card__body">
          <ToolArgs args={args} />
          {done && (
            <ToolResult content={result ?? ""} isError={isError === true} />
          )}
        </div>
      )}
    </div>
  );
}

function ToolStatus({ status }: { status: "running" | "done" | "error" }) {
  if (status === "running") {
    return (
      <span className="forge-tool-card__status">
        <span className="forge-tool-dot" aria-hidden />
      </span>
    );
  }
  if (status === "error") {
    return (
      <span className="forge-tool-card__status forge-tool-card__status--error">
        <CircleX size={12} />
      </span>
    );
  }
  return (
    <span className="forge-tool-card__status forge-tool-card__status--done">
      <CircleCheck size={12} />
    </span>
  );
}

function ToolArgs({ args }: { args: string }) {
  let pretty = args;
  try {
    pretty = JSON.stringify(JSON.parse(args), null, 2);
  } catch {
    // leave as-is
  }
  if (!pretty.trim()) return null;
  return (
    <div className="forge-tool-card__section">
      <div className="forge-tool-card__label">arguments</div>
      <pre className="forge-tool-card__pre">{pretty}</pre>
    </div>
  );
}

function ToolResult({ content, isError }: { content: string; isError: boolean }) {
  const trimmed = content.length > 4000
    ? content.slice(0, 4000) + "\n... (truncated)"
    : content;
  return (
    <div className="forge-tool-card__section">
      <div className="forge-tool-card__label">
        {isError ? "error" : "result"}
      </div>
      <pre
        className={`forge-tool-card__pre ${
          isError ? "forge-tool-card__pre--error" : ""
        }`}
      >
        {trimmed}
      </pre>
    </div>
  );
}

function summariseArgs(name: string, args: string): string {
  try {
    const parsed = JSON.parse(args);
    switch (name) {
      case "search_vault":
      case "web_search":
      case "grep_vault":
        return parsed.query ? `"${parsed.query}"` : "";
      case "read_file":
      case "write_file":
      case "edit_file":
      case "delete_file":
      case "read_section":
        return parsed.path ?? "";
      case "list_files":
        return parsed.directory ?? "";
      case "rename_file":
        return parsed.from && parsed.to ? `${parsed.from} -> ${parsed.to}` : "";
      default:
        return "";
    }
  } catch {
    return "";
  }
}
