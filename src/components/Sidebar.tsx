import { memo, useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  FileText,
  FolderClosed,
  FolderOpen,
} from "lucide-react";
import type { TreeNode } from "../lib/tauri";

interface Props {
  vaultName: string | null;
  tree: TreeNode | null;
  activePath: string | null;
  onPickVault: () => void;
  onOpenFile: (path: string, options?: { newTab?: boolean }) => void;
}

function Sidebar({
  vaultName,
  tree,
  activePath,
  onPickVault,
  onOpenFile,
}: Props) {
  return (
    <aside className="nav-files-container h-full flex flex-col bg-[var(--background-secondary)]">
      <header
        className="flex-shrink-0 flex items-center justify-between gap-2 px-3 border-b border-[var(--background-modifier-border)]"
        style={{ height: "var(--topbar-height)" }}
      >
        <div className="min-w-0 flex-1">
          <div className="text-[10px] font-semibold uppercase tracking-wider text-[var(--text-faint)] leading-none">
            Vault
          </div>
          <div className="text-[13px] font-semibold truncate text-[var(--text-normal)] mt-0.5 leading-tight">
            {vaultName ?? "No vault"}
          </div>
        </div>
        <button
          onClick={onPickVault}
          className="text-[10px] font-medium uppercase tracking-wider px-2 py-1 rounded-md border border-[var(--background-modifier-border)] hover:border-[var(--interactive-accent)] hover:text-[var(--text-accent)] text-[var(--text-muted)] transition-colors"
        >
          Open
        </button>
      </header>

      <div className="nav-folder mod-root flex-1 overflow-y-auto overflow-x-hidden px-1 py-2">
        {!tree && (
          <div className="px-3 py-6 text-center text-[11px] text-[var(--text-faint)]">
            No vault open
          </div>
        )}
        {tree &&
          tree.children.map((node) => (
            <MemoizedTreeNodeView
              key={node.path}
              node={node}
              depth={0}
              activePath={activePath}
              onOpen={onOpenFile}
            />
          ))}
      </div>
    </aside>
  );
}

export default memo(Sidebar);

interface NodeProps {
  node: TreeNode;
  depth: number;
  activePath: string | null;
  onOpen: (path: string, options?: { newTab?: boolean }) => void;
}

function TreeNodeView({ node, depth, activePath, onOpen }: NodeProps) {
  const [open, setOpen] = useState(depth === 0);

  if (node.is_dir) {
    const Icon = open ? FolderOpen : FolderClosed;
    const Chevron = open ? ChevronDown : ChevronRight;
    return (
      <div className="nav-folder">
        <button
          onClick={() => setOpen((v) => !v)}
          className="nav-folder-title w-full text-left flex items-center gap-1 h-[26px] pr-2 text-[13px] text-[var(--text-muted)] hover:bg-[var(--background-modifier-hover)] hover:text-[var(--text-normal)]"
          style={{ paddingLeft: 4 + depth * 12 }}
        >
          <Chevron
            size={12}
            strokeWidth={2.2}
            className="nav-folder-collapse-indicator flex-shrink-0 text-[var(--text-faint)]"
          />
          <Icon
            size={14}
            strokeWidth={1.7}
            className="flex-shrink-0 text-[var(--text-faint)]"
          />
          <span className="nav-folder-title-content truncate font-medium">
            {node.name}
          </span>
        </button>
        {open && (
          <div className="nav-folder-children">
            {node.children.map((child) => (
              <MemoizedTreeNodeView
                key={child.path}
                node={child}
                depth={depth + 1}
                activePath={activePath}
                onOpen={onOpen}
              />
            ))}
          </div>
        )}
      </div>
    );
  }

  const isActive = activePath === node.path;
  const display = node.name.replace(/\.mdx?$/i, "");
  return (
    <button
      onClick={(e) =>
        onOpen(node.path, { newTab: e.ctrlKey || e.metaKey })
      }
      onAuxClick={(e) => {
        // Middle click opens in new tab too
        if (e.button === 1) {
          e.preventDefault();
          onOpen(node.path, { newTab: true });
        }
      }}
      className={`nav-file-title w-full text-left flex items-center gap-1.5 h-[26px] pr-2 text-[13px] truncate ${
        isActive
          ? "is-active bg-[var(--background-modifier-active)] text-[var(--text-accent)] font-medium"
          : "text-[var(--text-normal)] hover:bg-[var(--background-modifier-hover)]"
      }`}
      style={{ paddingLeft: 4 + depth * 12 + 14 }}
      title={node.path}
    >
      <FileText
        size={13}
        strokeWidth={1.7}
        className={`flex-shrink-0 ${
          isActive ? "text-[var(--text-accent)]" : "text-[var(--text-faint)]"
        }`}
      />
      <span className="nav-file-title-content truncate">{display}</span>
    </button>
  );
}

const MemoizedTreeNodeView = memo(TreeNodeView);
