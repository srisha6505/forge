import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { BookOpen, Columns, FileEdit, X } from "lucide-react";
import {
  currentVault,
  getSettings,
  listVaultTree,
  onVaultChanged,
  openVault,
  readFile,
  writeFile,
  type TreeNode,
} from "./lib/tauri";
import LeftRail from "./components/LeftRail";
import Sidebar from "./components/Sidebar";
import Search from "./components/Search";
import SearchModal from "./components/SearchModal";
import Editor from "./components/Editor";
import MarkdownPreview from "./components/MarkdownPreview";
import Chat from "./components/Chat";
import ResizeHandle from "./components/ResizeHandle";
import ErrorBoundary from "./components/ErrorBoundary";

const MIN_SIDEBAR = 200;
const MAX_SIDEBAR = 480;
const MIN_CHAT = 300;
const MAX_CHAT = 640;
const WRITE_DEBOUNCE_MS = 250;

type SidebarTabKind = "files" | "search" | "chat" | "settings";
type ThemeKind = "light" | "dark";

interface Tab {
  id: string;
  path: string;
  content: string;
  dirty: boolean;
  lastSavedAt: number | null;
}

function tabTitle(path: string): string {
  return path.split("/").pop()?.replace(/\.mdx?$/i, "") ?? path;
}

function newId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function resolveInTree(tree: TreeNode | null, target: string): string | null {
  if (!tree) return null;
  if (target.startsWith("/")) return target;
  const stripped = target.replace(/\.mdx?$/i, "").toLowerCase();
  const files: { path: string; name: string; rel: string }[] = [];
  const walk = (node: TreeNode, relPath: string) => {
    if (node.is_dir) {
      for (const c of node.children) {
        walk(c, relPath ? `${relPath}/${c.name}` : c.name);
      }
    } else {
      const baseName = node.name.replace(/\.mdx?$/i, "").toLowerCase();
      files.push({
        path: node.path,
        name: baseName,
        rel: relPath.toLowerCase(),
      });
    }
  };
  for (const c of tree.children) walk(c, c.name);
  const byRel = files.find((f) => f.rel === stripped);
  if (byRel) return byRel.path;
  const byName = files
    .filter((f) => f.name === stripped)
    .sort((a, b) => a.path.length - b.path.length);
  if (byName.length > 0) return byName[0].path;
  return null;
}

function getCssVarPx(name: string, fallback: number): number {
  const root = document.documentElement;
  const raw = root.style.getPropertyValue(name).trim();
  if (raw.endsWith("px")) {
    const n = parseFloat(raw);
    if (!Number.isNaN(n)) return n;
  }
  return fallback;
}

function setCssVarPx(name: string, value: number) {
  document.documentElement.style.setProperty(name, `${value}px`);
}

export default function App() {
  const [vault, setVault] = useState<string | null>(null);
  const [tree, setTree] = useState<TreeNode | null>(null);
  const [tabs, setTabs] = useState<Tab[]>([]);
  const [activeTabId, setActiveTabId] = useState<string | null>(null);
  // Synchronous mirrors of tabs / activeTabId so ref-wrapped callbacks can
  // read the latest values without tearing. Refs are safe to write during
  // render — they don't trigger re-renders.
  const tabsRef = useRef<Tab[]>([]);
  const activeTabIdRef = useRef<string | null>(null);
  tabsRef.current = tabs;
  activeTabIdRef.current = activeTabId;
  const [sidebarTab, setSidebarTab] = useState<SidebarTabKind>("files");
  const [sidebarVisible, setSidebarVisible] = useState(true);
  const [chatVisible, setChatVisible] = useState(true);
  const [sidebarWidth, setSidebarWidth] = useState(260);
  const [chatWidth, setChatWidth] = useState(420);
  const [theme, setTheme] = useState<ThemeKind>("light");
  const [readableWidth, setReadableWidth] = useState(true);
  const [readMode, setReadMode] = useState(false);

  const activeTab = useMemo(
    () => tabs.find((t) => t.id === activeTabId) ?? null,
    [tabs, activeTabId],
  );

  // Bootstrap CSS variables for pane widths on mount (no-op on subsequent
  // state-driven updates; the drag handlers below write the var directly
  // without changing state, so there's no React re-render during a drag).
  useLayoutEffect(() => {
    setCssVarPx("--sidebar-width", sidebarWidth);
  }, [sidebarWidth]);
  useLayoutEffect(() => {
    setCssVarPx("--chat-width", chatWidth);
  }, [chatWidth]);

  // Debounced save bookkeeping.
  const writeTimer = useRef<number | null>(null);
  const pendingWrites = useRef<Map<string, string>>(new Map());

  const flushPending = useCallback(async () => {
    if (writeTimer.current !== null) {
      window.clearTimeout(writeTimer.current);
      writeTimer.current = null;
    }
    if (pendingWrites.current.size === 0) return;
    const entries = Array.from(pendingWrites.current.entries());
    pendingWrites.current.clear();
    const savedPaths: string[] = [];
    for (const [path, content] of entries) {
      try {
        await writeFile(path, content);
        savedPaths.push(path);
      } catch (e) {
        console.error("save failed", path, e);
      }
    }
    if (savedPaths.length > 0) {
      const now = Date.now();
      setTabs((prev) =>
        prev.map((t) =>
          savedPaths.includes(t.path)
            ? { ...t, dirty: false, lastSavedAt: now }
            : t,
        ),
      );
    }
  }, []);

  // Boot: load vault + theme.
  useEffect(() => {
    (async () => {
      try {
        const existing = await currentVault();
        if (existing) {
          setVault(existing);
          const t = await listVaultTree();
          setTree(t);
        }
      } catch (e) {
        console.error("boot error", e);
      }
      try {
        const s = await getSettings();
        if (s.theme === "dark" || s.theme === "light") setTheme(s.theme);
      } catch {
        /* no settings yet */
      }
    })();
  }, []);

  useEffect(() => {
    document.body.classList.remove("theme-light", "theme-dark");
    document.body.classList.add(`theme-${theme}`);
  }, [theme]);

  // Reload the vault tree whenever the agent writes/edits/renames/deletes
  // a file. The backend emits `vault://changed` after such tool calls.
  useEffect(() => {
    const unlisten = onVaultChanged(() => {
      listVaultTree()
        .then((t) => setTree(t))
        .catch((e) => console.warn("tree refresh failed", e));
    });
    return () => {
      unlisten.then((fn) => fn()).catch(() => {});
    };
  }, []);

  // Search modal toggle (Ctrl+Shift+F).
  const [searchModalOpen, setSearchModalOpen] = useState(false);

  // ── File / tab operations ─────────────────────────────────────────────

  const openFileRef = useRef<
    (path: string, options?: { newTab?: boolean }) => void
  >(() => {});

  const openFile = useCallback(
    async (path: string, options?: { newTab?: boolean }) => {
      await flushPending();
      const existing = tabsRef.current.find((t) => t.path === path);
      if (existing) {
        setActiveTabId(existing.id);
        return;
      }
      let content = "";
      try {
        content = await readFile(path);
      } catch (e) {
        console.error("read failed", path, e);
        return;
      }
      const nextTab: Tab = {
        id: newId(),
        path,
        content,
        dirty: false,
        lastSavedAt: null,
      };
      const currentId = activeTabIdRef.current;
      setTabs((prev) => {
        if (options?.newTab || prev.length === 0 || !currentId) {
          return [...prev, nextTab];
        }
        return prev.map((t) => (t.id === currentId ? nextTab : t));
      });
      setActiveTabId(nextTab.id);
    },
    [flushPending],
  );
  openFileRef.current = openFile;

  const openByTargetRef = useRef<(target: string) => void>(() => {});
  const openByTarget = useCallback(
    (target: string) => {
      const resolved = resolveInTree(tree, target);
      if (resolved) {
        openFile(resolved);
      } else {
        console.warn("wikilink target not found in vault:", target);
      }
    },
    [tree, openFile],
  );
  openByTargetRef.current = openByTarget;

  const closeTab = useCallback(
    (tabId: string) => {
      flushPending();
      const currentTabs = tabsRef.current;
      const idx = currentTabs.findIndex((t) => t.id === tabId);
      if (idx === -1) return;
      const next = currentTabs.filter((t) => t.id !== tabId);
      setTabs(next);
      if (tabId === activeTabIdRef.current) {
        if (next.length === 0) {
          setActiveTabId(null);
        } else {
          const newIdx = Math.min(idx, next.length - 1);
          setActiveTabId(next[newIdx].id);
        }
      }
    },
    [flushPending],
  );

  const switchTab = useCallback(
    (tabId: string) => {
      flushPending();
      setActiveTabId(tabId);
    },
    [flushPending],
  );

  // Editor change handler — ref-wrapped so Editor's memo works.
  const onEditorChangeImpl = useCallback(
    (newContent: string) => {
      const activeId = activeTabIdRef.current;
      if (!activeId) return;
      const currentTabs = tabsRef.current;
      const tab = currentTabs.find((t) => t.id === activeId);
      if (!tab) return;
      setTabs((prev) =>
        prev.map((t) =>
          t.id === activeId
            ? { ...t, content: newContent, dirty: true }
            : t,
        ),
      );
      pendingWrites.current.set(tab.path, newContent);
      if (writeTimer.current !== null) {
        window.clearTimeout(writeTimer.current);
      }
      writeTimer.current = window.setTimeout(() => {
        flushPending();
      }, WRITE_DEBOUNCE_MS);
    },
    [flushPending],
  );
  const onEditorChangeRef = useRef(onEditorChangeImpl);
  onEditorChangeRef.current = onEditorChangeImpl;

  // Stable wrappers — identity never changes. Editor / Sidebar / LeftRail
  // receive these so React.memo can skip re-renders on unrelated state
  // updates (pane drag, tab switch, etc).
  const stable = useMemo(
    () => ({
      onEditorChange: (v: string) => onEditorChangeRef.current(v),
      openByTarget: (t: string) => openByTargetRef.current(t),
      openFile: (p: string, opts?: { newTab?: boolean }) =>
        openFileRef.current(p, opts),
    }),
    [],
  );

  // ── Shortcuts ─────────────────────────────────────────────────────────

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      if (!mod) return;
      const key = e.key.toLowerCase();
      if (key === "s") {
        e.preventDefault();
        flushPending();
      } else if (key === "b") {
        e.preventDefault();
        setSidebarVisible((v) => !v);
      } else if (key === "l" && e.shiftKey) {
        e.preventDefault();
        setChatVisible((v) => !v);
      } else if (key === ",") {
        e.preventDefault();
        setSidebarTab("settings");
        setSidebarVisible(true);
      } else if (key === "p" && e.shiftKey) {
        e.preventDefault();
        setSidebarTab("files");
        setSidebarVisible(true);
      } else if (key === "f" && e.shiftKey) {
        e.preventDefault();
        setSearchModalOpen((v) => !v);
      } else if (key === "r" && e.shiftKey) {
        e.preventDefault();
        setReadableWidth((r) => !r);
      } else if (key === "e" && !e.shiftKey) {
        e.preventDefault();
        setReadMode((r) => !r);
      } else if (key === "w") {
        e.preventDefault();
        if (activeTabId) closeTab(activeTabId);
      } else if (key === "tab") {
        e.preventDefault();
        setTabs((prev) => {
          if (prev.length <= 1) return prev;
          setActiveTabId((current) => {
            const idx = prev.findIndex((t) => t.id === current);
            if (idx === -1) return current;
            const dir = e.shiftKey ? -1 : 1;
            return prev[(idx + dir + prev.length) % prev.length].id;
          });
          return prev;
        });
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [flushPending, activeTabId, closeTab]);

  // Flush on unmount.
  useEffect(
    () => () => {
      flushPending();
    },
    [flushPending],
  );

  const pickVault = async () => {
    const picked = await open({
      directory: true,
      multiple: false,
      title: "Open vault folder",
    });
    if (typeof picked !== "string") return;
    try {
      await flushPending();
      await openVault(picked);
      setVault(picked);
      const t = await listVaultTree();
      setTree(t);
      setTabs([]);
      setActiveTabId(null);
    } catch (e) {
      console.error(e);
    }
  };
  const pickVaultRef = useRef(pickVault);
  pickVaultRef.current = pickVault;
  const stablePickVault = useMemo(() => () => pickVaultRef.current(), []);

  // ── Drag handlers: direct CSS-variable writes, zero re-renders ────────

  const handleSidebarResize = useCallback((delta: number) => {
    const current = getCssVarPx("--sidebar-width", 260);
    const next = Math.max(MIN_SIDEBAR, Math.min(MAX_SIDEBAR, current + delta));
    setCssVarPx("--sidebar-width", next);
  }, []);
  const handleSidebarResizeDone = useCallback(() => {
    setSidebarWidth(getCssVarPx("--sidebar-width", 260));
  }, []);

  const handleChatResize = useCallback((delta: number) => {
    const current = getCssVarPx("--chat-width", 420);
    const next = Math.max(MIN_CHAT, Math.min(MAX_CHAT, current - delta));
    setCssVarPx("--chat-width", next);
  }, []);
  const handleChatResizeDone = useCallback(() => {
    setChatWidth(getCssVarPx("--chat-width", 420));
  }, []);

  // ── Rail tab handling ─────────────────────────────────────────────────

  const handleRailChange = useCallback(
    (tab: SidebarTabKind) => {
      if (tab === sidebarTab) {
        setSidebarVisible((v) => !v);
      } else {
        setSidebarTab(tab);
        setSidebarVisible(true);
      }
    },
    [sidebarTab],
  );
  const handleRailChangeRef = useRef(handleRailChange);
  handleRailChangeRef.current = handleRailChange;
  const stableHandleRailChange = useMemo(
    () => (tab: SidebarTabKind) => handleRailChangeRef.current(tab),
    [],
  );

  const toggleChat = useCallback(
    () => setChatVisible((v) => !v),
    [],
  );
  const toggleTheme = useCallback(
    () => setTheme((t) => (t === "light" ? "dark" : "light")),
    [],
  );

  // ── Derived display ────────────────────────────────────────────────

  const vaultName = vault ? (vault.split("/").pop() ?? null) : null;
  const wordCount = useMemo(() => {
    if (!activeTab) return 0;
    return activeTab.content.trim().split(/\s+/).filter(Boolean).length;
  }, [activeTab]);
  const activeTitle = activeTab ? tabTitle(activeTab.path) : null;
  const saveStatus = activeTab?.dirty
    ? "Unsaved"
    : activeTab?.lastSavedAt
      ? "Saved"
      : "";

  return (
    <div className="workspace flex h-screen bg-[var(--background-secondary)] text-[var(--text-normal)] overflow-hidden">
      <LeftRail
        active={sidebarTab}
        onChange={stableHandleRailChange}
        onToggleChat={toggleChat}
        theme={theme}
        onToggleTheme={toggleTheme}
      />

      {sidebarVisible && (
        <>
          <div className="workspace-split flex-shrink-0 h-full overflow-hidden bg-[var(--background-secondary)] w-[var(--sidebar-width)]">
            {sidebarTab === "files" && (
              <Sidebar
                vaultName={vaultName}
                tree={tree}
                activePath={activeTab?.path ?? null}
                onPickVault={stablePickVault}
                onOpenFile={stable.openFile}
              />
            )}
            {sidebarTab === "search" && (
              <Search onOpenFile={stable.openFile} />
            )}
            {sidebarTab === "chat" && (
              <div className="h-full flex flex-col items-center justify-center gap-2 px-6 text-center">
                <div className="text-[12px] font-medium text-[var(--text-muted)]">
                  Chat
                </div>
                <div className="text-[11px] text-[var(--text-faint)]">
                  Chat lives in the right panel. Use Ctrl+Shift+L to toggle
                  it.
                </div>
              </div>
            )}
            {sidebarTab === "settings" && (
              <div className="h-full flex flex-col items-center justify-center gap-2 px-6 text-center">
                <div className="text-[12px] font-medium text-[var(--text-muted)]">
                  Settings
                </div>
                <div className="text-[11px] text-[var(--text-faint)]">
                  Coming soon.
                </div>
              </div>
            )}
          </div>
          <ResizeHandle
            onResize={handleSidebarResize}
            onDone={handleSidebarResizeDone}
          />
        </>
      )}

      <main className="workspace-split mod-root flex-1 min-w-0 h-full flex">
        <div className="workspace-leaf flex-1 min-w-0 h-full flex flex-col bg-[var(--background-primary)] border-l border-[var(--background-modifier-border)]">
          {/* Tab bar */}
          <div
            className="workspace-tab-header-container flex-shrink-0 flex items-end justify-between bg-[var(--background-secondary)] border-b border-[var(--background-modifier-border)]"
            style={{ height: "var(--tab-height)" }}
          >
            <div className="flex-1 min-w-0 flex items-end overflow-x-auto overflow-y-hidden">
              {tabs.length === 0 && (
                <div className="px-4 h-full flex items-center text-[11px] uppercase tracking-wider text-[var(--text-faint)]">
                  No file open
                </div>
              )}
              {tabs.map((tab) => {
                const isActive = tab.id === activeTabId;
                return (
                  <div
                    key={tab.id}
                    onClick={() => switchTab(tab.id)}
                    onAuxClick={(e) => {
                      if (e.button === 1) {
                        e.preventDefault();
                        closeTab(tab.id);
                      }
                    }}
                    className={`workspace-tab-header group flex items-center gap-2 px-4 h-full cursor-pointer border-r border-[var(--background-modifier-border)] min-w-0 flex-shrink-0 ${
                      isActive
                        ? "is-active bg-[var(--background-primary)] text-[var(--text-normal)] -mb-px"
                        : "bg-[var(--background-secondary)] text-[var(--text-muted)] hover:text-[var(--text-normal)] hover:bg-[var(--background-modifier-hover)]"
                    }`}
                  >
                    <span className="text-[13px] font-medium truncate max-w-[200px]">
                      {tabTitle(tab.path)}
                    </span>
                    {tab.dirty && (
                      <span
                        className="w-1.5 h-1.5 rounded-full bg-[var(--interactive-accent)]"
                        title="Unsaved changes"
                      />
                    )}
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        closeTab(tab.id);
                      }}
                      className="ml-1 text-[var(--text-faint)] hover:text-[var(--text-normal)] opacity-70 group-hover:opacity-100 transition-opacity"
                      title="Close"
                    >
                      <X size={14} strokeWidth={2.2} />
                    </button>
                  </div>
                );
              })}
            </div>

            <div className="flex items-center gap-1 px-3 h-full flex-shrink-0">
              <button
                onClick={() => setReadableWidth((r) => !r)}
                title={`${readableWidth ? "Disable" : "Enable"} readable width (Ctrl+Shift+R)`}
                className={`w-8 h-8 flex items-center justify-center rounded-md transition-colors ${
                  readableWidth
                    ? "text-[var(--text-accent)] bg-[var(--background-modifier-active)]"
                    : "text-[var(--icon-color)] hover:text-[var(--icon-color-hover)] hover:bg-[var(--background-modifier-hover)]"
                }`}
              >
                <Columns size={16} strokeWidth={1.8} />
              </button>
              <button
                onClick={() => setReadMode((r) => !r)}
                title={`Switch to ${readMode ? "edit" : "read"} mode (Ctrl+E)`}
                className={`w-8 h-8 flex items-center justify-center rounded-md transition-colors ${
                  readMode
                    ? "text-[var(--text-accent)] bg-[var(--background-modifier-active)]"
                    : "text-[var(--icon-color)] hover:text-[var(--icon-color-hover)] hover:bg-[var(--background-modifier-hover)]"
                }`}
              >
                {readMode ? (
                  <FileEdit size={16} strokeWidth={1.8} />
                ) : (
                  <BookOpen size={16} strokeWidth={1.8} />
                )}
              </button>
            </div>
          </div>

          {/* Editor / Preview */}
          <ErrorBoundary>
            {readMode && activeTab ? (
              <MarkdownPreview
                title={activeTitle}
                content={activeTab.content}
                readableWidth={readableWidth}
              />
            ) : (
              <Editor
                path={activeTab?.path ?? null}
                title={activeTitle}
                content={activeTab?.content ?? ""}
                readableWidth={readableWidth}
                onChange={stable.onEditorChange}
                onOpenPath={stable.openByTarget}
              />
            )}
          </ErrorBoundary>

          {/* Status bar */}
          <div
            className="workspace-statusbar flex-shrink-0 flex items-center justify-between px-4 bg-[var(--background-secondary)] border-t border-[var(--background-modifier-border)] text-[10px] text-[var(--text-faint)] font-medium uppercase tracking-wider"
            style={{ height: "var(--statusbar-height)" }}
          >
            <div className="flex items-center gap-3">
              {vaultName && <span>{vaultName}</span>}
              {activeTab && (
                <>
                  <span className="text-[var(--text-faint)] opacity-40">
                    ·
                  </span>
                  <span className="tabular-nums">{wordCount} words</span>
                </>
              )}
              {tabs.length > 0 && (
                <>
                  <span className="text-[var(--text-faint)] opacity-40">
                    ·
                  </span>
                  <span className="tabular-nums">
                    {tabs.length} tab{tabs.length === 1 ? "" : "s"}
                  </span>
                </>
              )}
            </div>
            <div className="flex items-center gap-3">
              {saveStatus && <span>{saveStatus}</span>}
            </div>
          </div>
        </div>

        {chatVisible && (
          <>
            <ResizeHandle
              onResize={handleChatResize}
              onDone={handleChatResizeDone}
            />
            <aside className="workspace-leaf flex-shrink-0 h-full border-l border-[var(--background-modifier-border)] bg-[var(--background-primary)] w-[var(--chat-width)]">
              <Chat />
            </aside>
          </>
        )}
      </main>

      <SearchModal
        open={searchModalOpen}
        onClose={() => setSearchModalOpen(false)}
        onOpenFile={stable.openFile}
      />
    </div>
  );
}
