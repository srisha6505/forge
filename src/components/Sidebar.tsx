import { memo, useState } from "react";
import type { CSSProperties, MouseEvent } from "react";
import {
  ChevronDown,
  ChevronRight,
  FileText,
  Folder,
  FolderOpen,
  Plus,
  MoreHorizontal,
} from "./ui/Icons";
import { GhostBtn } from "./ui";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import type { TreeNode } from "../lib/tauri";

// File-tree clipboard. Module-scoped — single window, single user, so
// no race. Set when the user picks Cut/Copy from the context menu;
// consumed when they pick Paste in a folder/empty area.
type Clip = { mode: "copy" | "cut"; path: string } | null;
let clip: Clip = null;
const clipListeners = new Set<() => void>();
function setClip(v: Clip) {
  clip = v;
  clipListeners.forEach((fn) => fn());
}
// Cross-module clipboard accessors. App.tsx imports the module and
// reads/clears the clip when handling Paste from a folder/background
// context menu. Underscore-prefixed to mark as a soft API.
export const __readClip = (): Clip => clip;
export const __clearClip = (): void => setClip(null);
function useClip() {
  const [, force] = useState(0);
  // Subscribe on first render so any component using the clipboard
  // re-renders when its contents change (e.g. Paste availability).
  useState(() => {
    const fn = () => force((n) => n + 1);
    clipListeners.add(fn);
    return () => clipListeners.delete(fn);
  });
  return clip;
}

interface FileOps {
  // Caller resolves these via App.tsx — they hit Tauri + refresh tree.
  onCreateAt: (parentDir: string) => void;
  onRename: (path: string) => void;
  onDuplicate: (path: string) => void;
  onDelete: (path: string) => void;
  onPaste: (parentDir: string) => void;
}

interface Props {
  vaultName: string | null;
  tree: TreeNode | null;
  activePath: string | null;
  onPickVault: () => void;
  onOpenFile: (path: string, options?: { newTab?: boolean }) => void;
  onNewFile?: () => void;
  dirtyPaths?: Set<string>;
  promotedPaths?: Set<string>;
  fileOps?: FileOps;
}

// Derive parent directory from a vault-relative path. Empty string means root.
function parentOf(path: string): string {
  const i = path.lastIndexOf("/");
  return i < 0 ? "" : path.slice(0, i);
}

async function copyToClipboard(text: string) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    // Fallback: hidden textarea. The Tauri webview generally allows the
    // async API, but be safe in case of permission quirks.
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.left = "-9999px";
    document.body.appendChild(ta);
    ta.select();
    try {
      document.execCommand("copy");
    } catch {}
    document.body.removeChild(ta);
  }
}

// Files sidebar matching forge_ui/Shell.jsx::FilesSidebar. Header is 36px,
// rows are 28px tall with 14px indent per level, dirty indicator is a 6px
// accent circle, AI-promoted files get a small ✦ glyph.
function Sidebar({
  vaultName,
  tree,
  activePath,
  onPickVault,
  onOpenFile,
  onNewFile,
  dirtyPaths,
  promotedPaths,
  fileOps,
}: Props) {
  // One context-menu instance for the whole tree. Held at this level so
  // right-clicking a different node closes the previous menu before
  // opening the new one without flicker.
  const [menu, setMenu] = useState<{
    x: number;
    y: number;
    items: MenuItem[];
  } | null>(null);
  const closeMenu = () => setMenu(null);

  const clipState = useClip();

  // Right-click on the empty tree background → root-level menu.
  const onTreeBgContext = (e: MouseEvent<HTMLDivElement>) => {
    // Only fire when the click target IS the background container, not
    // a descendant row (rows handle their own context menus).
    if (e.target !== e.currentTarget) return;
    e.preventDefault();
    if (!fileOps) return;
    const items: MenuItem[] = [
      {
        label: "New file in root",
        onClick: () => fileOps.onCreateAt(""),
      },
    ];
    if (clipState) {
      items.push({ kind: "sep" });
      items.push({
        label: clipState.mode === "cut" ? "Paste (move) here" : "Paste here",
        hint: clipState.path.split("/").pop() ?? "",
        onClick: () => fileOps.onPaste(""),
      });
    }
    setMenu({ x: e.clientX, y: e.clientY, items });
  };

  return (
    <aside
      className="nav-files-container"
      style={{
        height: "100%",
        display: "flex",
        flexDirection: "column",
        minWidth: 0,
        overflow: "hidden",
        background: "var(--background-secondary)",
      }}
    >
      {/* Header */}
      <div
        style={{
          height: 36,
          minHeight: 36,
          padding: "0 8px 0 14px",
          display: "flex",
          alignItems: "center",
          gap: 4,
          borderBottom: "1px solid var(--background-modifier-border)",
        }}
      >
        <span
          style={{
            flex: 1,
            fontSize: "var(--font-ui-small)",
            fontWeight: 600,
            color: "var(--text-normal)",
          }}
        >
          Files
        </span>
        <GhostBtn
          icon={<Plus size={14} />}
          label="New file"
          size={24}
          onClick={onNewFile}
        />
        <GhostBtn
          icon={<MoreHorizontal size={14} />}
          label="More"
          size={24}
        />
      </div>

      {/* Tree */}
      <div
        className="nav-folder mod-root"
        onContextMenu={onTreeBgContext}
        style={{
          flex: 1,
          overflowY: "auto",
          overflowX: "hidden",
          padding: "4px 0 8px",
        }}
      >
        {!tree && (
          <div
            style={{
              padding: "24px 12px",
              textAlign: "center",
              fontSize: "var(--font-ui-small)",
              color: "var(--text-faint)",
            }}
          >
            <button
              onClick={onPickVault}
              style={{
                background: "transparent",
                border: "1px solid var(--background-modifier-border)",
                borderRadius: "var(--radius-s)",
                padding: "6px 10px",
                color: "var(--text-muted)",
                cursor: "pointer",
                fontSize: "var(--font-ui-smaller)",
              }}
            >
              Open vault
            </button>
          </div>
        )}
        {tree &&
          tree.children.map((node) => (
            <MemoizedTreeNodeView
              key={node.path}
              node={node}
              depth={0}
              activePath={activePath}
              dirtyPaths={dirtyPaths}
              promotedPaths={promotedPaths}
              onOpen={onOpenFile}
              fileOps={fileOps}
              clip={clipState}
              onMenu={(items, ev) =>
                setMenu({ x: ev.clientX, y: ev.clientY, items })
              }
            />
          ))}
      </div>
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          items={menu.items}
          onClose={closeMenu}
        />
      )}

      {/* Footer */}
      <div
        style={{
          height: 26,
          padding: "0 12px",
          borderTop: "1px solid var(--background-modifier-border)",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          fontSize: "var(--font-ui-small)",
          color: "var(--text-muted)",
          cursor: vaultName ? "pointer" : "default",
        }}
        onClick={vaultName ? onPickVault : undefined}
        title={vaultName ? "Change vault" : undefined}
      >
        <span
          style={{
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {vaultName ?? "No vault"}
        </span>
        <ChevronDown size={12} />
      </div>
    </aside>
  );
}

export default memo(Sidebar);

interface NodeProps {
  node: TreeNode;
  depth: number;
  activePath: string | null;
  dirtyPaths?: Set<string>;
  promotedPaths?: Set<string>;
  onOpen: (path: string, options?: { newTab?: boolean }) => void;
  fileOps?: FileOps;
  clip: Clip;
  onMenu?: (items: MenuItem[], ev: MouseEvent<HTMLDivElement>) => void;
}

function TreeNodeView({
  node,
  depth,
  activePath,
  dirtyPaths,
  promotedPaths,
  onOpen,
  fileOps,
  clip,
  onMenu,
}: NodeProps) {
  const [open, setOpen] = useState(depth === 0);

  const rowBase: CSSProperties = {
    height: 28,
    display: "flex",
    alignItems: "center",
    gap: 4,
    paddingLeft: 8 + depth * 14,
    paddingRight: 12,
    fontSize: "var(--font-ui-medium)",
    cursor: "pointer",
    transition: "background var(--motion-duration-fast) var(--motion-ease)",
    userSelect: "none",
  };

  const onDirContext = (ev: MouseEvent<HTMLDivElement>) => {
    if (!fileOps || !onMenu) return;
    ev.preventDefault();
    ev.stopPropagation();
    const items: MenuItem[] = [
      {
        label: "New file in folder",
        onClick: () => fileOps.onCreateAt(node.path),
      },
    ];
    if (clip) {
      items.push({ kind: "sep" });
      items.push({
        label: clip.mode === "cut" ? "Paste (move) here" : "Paste here",
        hint: clip.path.split("/").pop() ?? "",
        onClick: () => fileOps.onPaste(node.path),
      });
    }
    items.push({ kind: "sep" });
    items.push({
      label: "Copy folder path",
      onClick: () => void copyToClipboard(node.path),
    });
    onMenu(items, ev);
  };

  if (node.is_dir) {
    return (
      <div className="nav-folder">
        <div
          onClick={() => setOpen((v) => !v)}
          onContextMenu={onDirContext}
          style={{
            ...rowBase,
            color: "var(--text-muted)",
            background: "transparent",
          }}
          onMouseEnter={(e: MouseEvent<HTMLDivElement>) => {
            e.currentTarget.style.background =
              "var(--background-modifier-hover)";
          }}
          onMouseLeave={(e: MouseEvent<HTMLDivElement>) => {
            e.currentTarget.style.background = "transparent";
          }}
        >
          <span
            style={{
              width: 14,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              color: "var(--text-muted)",
              flexShrink: 0,
            }}
          >
            {open ? (
              <ChevronDown size={12} />
            ) : (
              <ChevronRight size={12} />
            )}
          </span>
          <span
            style={{
              color: "var(--text-muted)",
              display: "flex",
              flexShrink: 0,
            }}
          >
            {open ? <FolderOpen size={14} /> : <Folder size={14} />}
          </span>
          <span
            style={{
              flex: 1,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              fontWeight: 500,
              color: "var(--text-muted)",
            }}
          >
            {node.name}
          </span>
        </div>
        {open && (
          <div className="nav-folder-children">
            {node.children.map((child) => (
              <MemoizedTreeNodeView
                key={child.path}
                node={child}
                depth={depth + 1}
                activePath={activePath}
                dirtyPaths={dirtyPaths}
                promotedPaths={promotedPaths}
                onOpen={onOpen}
                fileOps={fileOps}
                clip={clip}
                onMenu={onMenu}
              />
            ))}
          </div>
        )}
      </div>
    );
  }

  const isActive = activePath === node.path;
  const isDirty = dirtyPaths?.has(node.path) ?? false;
  const isPromoted = promotedPaths?.has(node.path) ?? false;
  const display = node.name.replace(/\.mdx?$/i, "");

  const onFileContext = (ev: MouseEvent<HTMLDivElement>) => {
    if (!fileOps || !onMenu) return;
    ev.preventDefault();
    ev.stopPropagation();
    const dir = parentOf(node.path);
    const items: MenuItem[] = [
      {
        label: "Open in new tab",
        onClick: () => onOpen(node.path, { newTab: true }),
      },
      { kind: "sep" },
      {
        label: "New file here",
        onClick: () => fileOps.onCreateAt(dir),
      },
      {
        label: "Rename",
        hint: "F2",
        onClick: () => fileOps.onRename(node.path),
      },
      {
        label: "Duplicate",
        onClick: () => fileOps.onDuplicate(node.path),
      },
      { kind: "sep" },
      {
        label: "Copy",
        onClick: () => setClip({ mode: "copy", path: node.path }),
      },
      {
        label: "Cut",
        onClick: () => setClip({ mode: "cut", path: node.path }),
      },
      {
        label: "Copy path",
        onClick: () => void copyToClipboard(node.path),
      },
      { kind: "sep" },
      {
        label: "Delete",
        destructive: true,
        onClick: () => fileOps.onDelete(node.path),
      },
    ];
    onMenu(items, ev);
  };

  return (
    <div
      onClick={(e) =>
        onOpen(node.path, { newTab: e.ctrlKey || e.metaKey })
      }
      onAuxClick={(e) => {
        if (e.button === 1) {
          e.preventDefault();
          onOpen(node.path, { newTab: true });
        }
      }}
      onContextMenu={onFileContext}
      title={node.path}
      style={{
        ...rowBase,
        color: isActive ? "var(--text-accent)" : "var(--text-normal)",
        background: isActive ? "var(--background-modifier-active)" : "transparent",
      }}
      onMouseEnter={(e: MouseEvent<HTMLDivElement>) => {
        if (!isActive)
          e.currentTarget.style.background =
            "var(--background-modifier-hover)";
      }}
      onMouseLeave={(e: MouseEvent<HTMLDivElement>) => {
        if (!isActive) e.currentTarget.style.background = "transparent";
      }}
    >
      {/* chevron slot */}
      <span style={{ width: 14, flexShrink: 0 }} />
      {/* icon */}
      <span
        style={{
          color: "var(--text-muted)",
          display: "flex",
          flexShrink: 0,
        }}
      >
        <FileText size={14} />
      </span>
      {/* promoted glyph */}
      {isPromoted && (
        <span
          style={{
            fontSize: 10,
            color: "var(--text-faint)",
            marginLeft: -2,
          }}
        >
          ✦
        </span>
      )}
      {/* label */}
      <span
        style={{
          flex: 1,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
          fontWeight: 400,
        }}
      >
        {display}
      </span>
      {/* dirty dot, right-edge, 10px margin */}
      {isDirty && (
        <span
          style={{
            width: 6,
            height: 6,
            borderRadius: 999,
            background: "var(--text-accent)",
            flexShrink: 0,
            marginRight: 10 - 12, // row already has 12px right pad; net 10px
          }}
        />
      )}
    </div>
  );
}

const MemoizedTreeNodeView = memo(TreeNodeView);
