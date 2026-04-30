import {
  Suspense,
  lazy,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { CSSProperties, MouseEvent } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { EditorView } from "@codemirror/view";
import { EditorSelection } from "@codemirror/state";
import { wikilinkRescanEffect } from "./lib/cm-wikilinks";
import { ZoomIn, ZoomOut } from "lucide-react";
import {
  Eye,
  PenLine,
  AlignLeft,
  ListTree,
  PanelRight,
  Terminal,
  Link2,
  X,
  MessageSquare,
} from "./components/ui/Icons";
import { GhostBtn } from "./components/ui";
import {
  currentVault,
  deleteFile,
  getAppSettings,
  getSettings,
  getVaultSettings,
  listVaultTree,
  migrateVaultSettings,
  onVaultChanged,
  openVault,
  readFile,
  renameFile,
  setAppSettings,
  setSettings,
  setVaultSettings,
  startRecording,
  stopRecordingAndTranscribe,
  writeFile,
  type AppSettings,
  type TreeNode,
  type VaultSettings,
} from "./lib/tauri";
import LeftRail from "./components/LeftRail";
import type { TabKind } from "./components/LeftRail";
import Sidebar, {
  __readClip as readFileTreeClip,
  __clearClip as clearFileTreeClip,
} from "./components/Sidebar";
import Search from "./components/Search";
import SearchModal from "./components/SearchModal";
import Editor from "./components/Editor";
import LatexViewer from "./components/LatexViewer";
import DocxViewer from "./components/DocxViewer";
import PdfViewer from "./components/PdfViewer";
import ImageViewer from "./components/ImageViewer";
// Lazy-load both settings modals — they're heavy (~1600 lines of provider
// + tab UI) and only render when the user clicks the gear or sparkles
// icon. Splitting them out shrinks the main bundle and means the modal
// chunk is fetched on demand. Subsequent opens are instant (cached).
const GeneralSettingsModal = lazy(
  () => import("./components/GeneralSettingsModal"),
);
const AISettingsModal = lazy(() => import("./components/AISettingsModal"));
import GraphView from "./components/GraphView";
import Chat, { type ChatHandle } from "./components/Chat";
import ChatTabView, { type ChatTabHandle } from "./components/ChatTabView";
import ChatHistorySidebar from "./components/ChatHistorySidebar";
import ResizeHandle from "./components/ResizeHandle";
import TerminalPanel from "./components/Terminal";
import ErrorBoundary from "./components/ErrorBoundary";
import TitleBar from "./components/TitleBar";
import { fileKind, isBinaryKind } from "./lib/file-types";

const MIN_SIDEBAR = 200;
const MAX_SIDEBAR = 480;
const MIN_CHAT = 300;
const MAX_CHAT = 640;
const WRITE_DEBOUNCE_MS = 250;

type SidebarTabKind = "files" | "search" | "chats";
type ThemeKind = "light" | "dark";
type SettingsModalKind = null | "general" | "ai";

type FileTab = {
  type: "file";
  id: string;
  path: string;
  content: string;
  dirty: boolean;
  lastSavedAt: number | null;
};

type ChatTab = {
  type: "chat";
  id: string;
  chatId: string;
  title: string;
};

type Tab = FileTab | ChatTab;

function tabTitle(path: string): string {
  return path.split("/").pop()?.replace(/\.mdx?$/i, "") ?? path;
}

function newId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

function resolveInTree(tree: TreeNode | null, target: string): string | null {
  if (!tree) return null;
  // Strip the section / block fragment Obsidian lets users append:
  //   [[note#heading]]   →  resolve "note"
  //   [[note^block-id]]  →  resolve "note"
  // The fragment is for navigation within the file, not part of the
  // vault path. Without this strip, `note#heading` never matches any
  // file in the tree and the wikilink quietly does nothing.
  const fragmentSplit = target.match(/^([^#^]+)(?:[#^].*)?$/);
  const targetClean = fragmentSplit ? fragmentSplit[1] : target;
  if (targetClean.startsWith("/")) return targetClean;
  const stripped = targetClean.replace(/\.mdx?$/i, "").toLowerCase();
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
  // read the latest values without tearing.
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
  const [readableWidth, setReadableWidth] = useState(false);
  const [readMode, setReadMode] = useState(true);
  const [tocOpen, setTocOpen] = useState(false);
  // Per-vault zoom for markdown content (preview + editor).
  const [mdZoom, setMdZoom] = useState(1);
  // Bumps every time a chat persists so the history sidebar refetches.
  const [chatReloadKey, setChatReloadKey] = useState(0);
  const bumpChatReload = useCallback(() => setChatReloadKey((k) => k + 1), []);
  // Bottom terminal panel.
  const [terminalOpen, setTerminalOpen] = useState(false);
  const [terminalHeight, setTerminalHeight] = useState(240);

  const activeTab = useMemo(
    () => tabs.find((t) => t.id === activeTabId) ?? null,
    [tabs, activeTabId],
  );
  const activeFileTab: FileTab | null =
    activeTab && activeTab.type === "file" ? activeTab : null;

  // Dirty + promoted paths for the Sidebar row decorations.
  const dirtyPaths = useMemo(() => {
    const s = new Set<string>();
    for (const t of tabs)
      if (t.type === "file" && t.dirty) s.add(t.path);
    return s;
  }, [tabs]);
  const promotedPaths = useMemo(() => new Set<string>(), []);

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
    const saved = new Map<string, string>();
    for (const [path, content] of entries) {
      try {
        await writeFile(path, content);
        saved.set(path, content);
      } catch (e) {
        console.error("save failed", path, e);
      }
    }
    if (saved.size > 0) {
      const now = Date.now();
      // Reconcile tab.content with what just hit disk. The hot-path
      // onEditorChangeImpl no longer pushes content into tab state per
      // keystroke (perf), so this is the canonical sync point.
      setTabs((prev) =>
        prev.map((t) => {
          if (t.type !== "file") return t;
          const c = saved.get(t.path);
          if (c === undefined) return t;
          return { ...t, content: c, dirty: false, lastSavedAt: now };
        }),
      );
    }
  }, []);

  // Track the current VaultSettings so we can carry forward fields we
  // don't yet expose in the UI (system_prompt, ai, voice, …) when we
  // persist on a setting change.
  const vaultSettingsRef = useRef<VaultSettings | null>(null);

  // Boot: pick up `last_opened_vault` from AppSettings, fall back to
  // the legacy current_vault state for users mid-migration.
  useEffect(() => {
    (async () => {
      try {
        const app = await getAppSettings().catch(() => null);
        let activeVault: string | null = null;
        if (app && app.last_opened_vault) {
          try {
            await openVault(app.last_opened_vault);
            activeVault = app.last_opened_vault;
          } catch (e) {
            console.warn("last_opened_vault failed to open", e);
          }
        }
        if (!activeVault) {
          activeVault = await currentVault();
        }
        if (activeVault) {
          setVault(activeVault);
          const t = await listVaultTree();
          setTree(t);
          await loadVaultSettings(activeVault);
        }
      } catch (e) {
        console.error("boot error", e);
      }

      // Legacy bridge: until the per-vault VaultSettings learn the
      // open_tabs concept, we still restore the prior tab list from the
      // legacy global Settings. This keeps the editor warm across boots.
      try {
        const s = await getSettings();
        if (Array.isArray(s.open_tabs) && s.open_tabs.length > 0) {
          const restored: Tab[] = [];
          for (const path of s.open_tabs) {
            try {
              const content = isBinaryKind(fileKind(path))
                ? ""
                : await readFile(path);
              restored.push({
                type: "file",
                id: newId(),
                path,
                content,
                dirty: false,
                lastSavedAt: null,
              });
            } catch {
              /* file gone since last session; skip */
            }
          }
          if (restored.length > 0) {
            setTabs(restored);
            const idx =
              typeof s.active_tab === "number" &&
              s.active_tab >= 0 &&
              s.active_tab < restored.length
                ? s.active_tab
                : 0;
            setActiveTabId(restored[idx].id);
          }
        }
      } catch {
        /* no settings yet */
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Migrate (idempotent) + pull VaultSettings, apply to UI state.
  const loadVaultSettings = useCallback(async (vaultPath: string) => {
    try {
      await migrateVaultSettings(vaultPath).catch(() => false);
      const vs = await getVaultSettings(vaultPath);
      vaultSettingsRef.current = vs;
      if (vs.theme === "dark" || vs.theme === "light") setTheme(vs.theme);
      if (vs.sidebar_width > 0) setSidebarWidth(vs.sidebar_width);
      if (vs.chat_panel_width > 0) setChatWidth(vs.chat_panel_width);
    } catch (e) {
      console.warn("vault settings load failed", e);
    }
  }, []);

  // Persist workspace state (debounced 800 ms). Only FileTab paths are
  // persisted — chat tabs can have stale IDs across restarts.
  const settingsCacheRef = useRef<Awaited<ReturnType<typeof getSettings>> | null>(null);
  useEffect(() => {
    getSettings()
      .then((s) => {
        settingsCacheRef.current = s;
      })
      .catch(() => {});
  }, []);

  const filePathsKey = useMemo(
    () =>
      tabs
        .filter((t): t is FileTab => t.type === "file")
        .map((t) => t.path)
        .join("\n"),
    [tabs],
  );
  // Active tab index among FILE tabs only (chat tabs are skipped). null if
  // the active tab is a chat or nothing is open.
  const activeFileIdx = useMemo(() => {
    if (activeTabId === null) return null;
    const fileTabs = tabs.filter((t): t is FileTab => t.type === "file");
    const i = fileTabs.findIndex((t) => t.id === activeTabId);
    return i >= 0 ? i : null;
  }, [activeTabId, tabs]);

  const stateSaveTimer = useRef<number | null>(null);
  useEffect(() => {
    if (!vault) return;
    if (stateSaveTimer.current !== null) {
      window.clearTimeout(stateSaveTimer.current);
    }
    stateSaveTimer.current = window.setTimeout(async () => {
      // Persist legacy global Settings (open_tabs, active_tab) — this
      // table is read on boot until the per-vault format learns tabs.
      const cur = settingsCacheRef.current;
      if (cur) {
        const open_tabs = filePathsKey ? filePathsKey.split("\n") : [];
        const next = {
          ...cur,
          open_tabs,
          active_tab: activeFileIdx,
          sidebar_width: sidebarWidth,
          chat_panel_width: chatWidth,
        };
        if (
          cur.open_tabs.join("\n") !== open_tabs.join("\n") ||
          cur.active_tab !== activeFileIdx ||
          cur.sidebar_width !== sidebarWidth ||
          cur.chat_panel_width !== chatWidth
        ) {
          settingsCacheRef.current = next;
          try {
            await setSettings(next);
          } catch (e) {
            console.warn("legacy settings persist failed", e);
          }
        }
      }

      // Persist VaultSettings (the new per-vault scope). We only mutate
      // fields we own here; AI, voice, system_prompt, tools_allowed
      // pass through untouched so a parallel modal save isn't clobbered.
      const vs = vaultSettingsRef.current;
      if (vs) {
        const recent_files = filePathsKey ? filePathsKey.split("\n") : [];
        const next: VaultSettings = {
          ...vs,
          theme,
          sidebar_width: sidebarWidth,
          chat_panel_width: chatWidth,
          recent_files,
        };
        if (
          vs.theme !== theme ||
          vs.sidebar_width !== sidebarWidth ||
          vs.chat_panel_width !== chatWidth ||
          vs.recent_files.join("\n") !== recent_files.join("\n")
        ) {
          vaultSettingsRef.current = next;
          try {
            await setVaultSettings(vault, next);
          } catch (e) {
            console.warn("vault settings persist failed", e);
          }
        }
      }
    }, 700);
    return () => {
      if (stateSaveTimer.current !== null) {
        window.clearTimeout(stateSaveTimer.current);
      }
    };
  }, [filePathsKey, activeFileIdx, sidebarWidth, chatWidth, theme, vault]);

  useEffect(() => {
    document.body.classList.remove("theme-light", "theme-dark");
    document.body.classList.add(`theme-${theme}`);
  }, [theme]);

  // Reload vault tree on agent writes (debounced 250 ms).
  useEffect(() => {
    let timer: number | null = null;
    const unlisten = onVaultChanged(() => {
      if (timer !== null) window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        timer = null;
        listVaultTree()
          .then((t) => setTree(t))
          .catch((e) => console.warn("tree refresh failed", e));
      }, 250);
    });
    return () => {
      if (timer !== null) window.clearTimeout(timer);
      unlisten.then((fn) => fn()).catch(() => {});
    };
  }, []);

  const [searchModalOpen, setSearchModalOpen] = useState(false);
  const [settingsModal, setSettingsModal] = useState<SettingsModalKind>(null);
  const [graphOpen, setGraphOpen] = useState(false);
  const [dictationActive, setDictationActive] = useState(false);

  // Bridges for the universal dictation flow: chat composers expose an
  // imperative appendToInput; the editor exposes its EditorView.
  const chatRef = useRef<ChatHandle | null>(null);
  const chatTabRef = useRef<ChatTabHandle | null>(null);
  const editorViewRef = useRef<EditorView | null>(null);

  const handleEditorMount = useCallback((view: EditorView | null) => {
    editorViewRef.current = view;
  }, []);

  // When the vault tree finishes loading after the editor has already
  // mounted, force cm-wikilinks to recompute its decorations. Without
  // this, `[short-ref]` style links sit as raw `[text]` until the user
  // types or toggles read/write — because the wikilink StateField only
  // rebuilds on doc-change / active-line-change. The tree-loaded edge
  // is the only async signal that flips `resolveTarget` from "always
  // null" to "actually resolves things".
  useEffect(() => {
    if (!tree) return;
    const view = editorViewRef.current;
    if (!view) return;
    view.dispatch({ effects: wikilinkRescanEffect.of() });
  }, [tree]);

  // Universal dictation router. Priority is:
  //   1. an active chat tab (its composer)
  //   2. the chat sidebar (its composer) when visible
  //   3. the markdown editor at the current caret
  // We capture the active surface at transcript-emit time (not toggle
  // time) so a user can switch tabs while dictating and the next words
  // land in the new surface — matches how OS-level dictation behaves.
  const routeTranscript = useCallback((text: string) => {
    if (!text) return;
    const fragment = text.trim();
    if (!fragment) return;
    const active = tabsRef.current.find(
      (t) => t.id === activeTabIdRef.current,
    );
    if (active && active.type === "chat") {
      chatTabRef.current?.appendToInput(fragment);
      return;
    }
    if (chatVisibleRef.current) {
      chatRef.current?.appendToInput(fragment);
      return;
    }
    if (active && active.type === "file") {
      const view = editorViewRef.current;
      if (view) {
        const pos = view.state.selection.main.head;
        const insert = fragment.endsWith(" ") ? fragment : `${fragment} `;
        view.dispatch({
          changes: { from: pos, insert },
          selection: EditorSelection.cursor(pos + insert.length),
        });
        view.focus();
        return;
      }
    }
    // Last resort: fall through into the chat sidebar even if hidden so
    // the words don't vanish.
    chatRef.current?.appendToInput(fragment);
  }, []);

  // Snapshot of chatVisible for the transcript callback (which doesn't
  // depend on a re-render to stay current).
  const chatVisibleRef = useRef(chatVisible);
  chatVisibleRef.current = chatVisible;

  // Push-to-talk dictation. Click once to start the mic, click again to
  // stop and transcribe. Uses start_recording / stop_recording_and_
  // transcribe — NOT voice_start, which is the full conversational loop
  // (mic → whisper → LLM → piper TTS) and demands piper + a piper voice
  // to be installed. Plain dictation only needs whisper-cli + a whisper
  // model file.
  const handleDictate = useCallback(async () => {
    if (dictationActive) {
      // Stop + transcribe. Surface the result through the same router
      // the editor caret / chat composers consume.
      setDictationActive(false);
      try {
        const transcript = await stopRecordingAndTranscribe();
        if (transcript) routeTranscript(transcript);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        console.error("[dictate] transcription failed:", msg);
        window.alert(
          `Dictation failed: ${msg}\n\nMake sure whisper-cli is installed and a whisper model is set in AI settings → Voice.`,
        );
      }
      return;
    }
    try {
      await startRecording();
      setDictationActive(true);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      console.error("[dictate] failed to start recording:", msg);
      window.alert(
        `Could not start dictation: ${msg}\n\nCheck microphone permission and try again.`,
      );
      setDictationActive(false);
    }
  }, [dictationActive, routeTranscript]);

  // Font families are set via CSS tokens (--font-interface / --font-text /
  // --font-monospace) in src/index.css. Runtime overrides from stale saved
  // settings used to clobber them on boot; removed. If a user-pickable font
  // picker returns in a later phase, re-introduce here.
  //
  // Font size override stays — it's a legitimate user preference.
  useEffect(() => {
    (async () => {
      try {
        const s = await getSettings();
        if (s.font_size && s.font_size > 0) {
          document.documentElement.style.setProperty(
            "--font-text-size",
            `${s.font_size}px`,
          );
        }
      } catch {
        /* settings not available yet */
      }
    })();
  }, [settingsModal]);

  // ── File / tab operations ─────────────────────────────────────────────

  const openFileRef = useRef<
    (path: string, options?: { newTab?: boolean }) => void
  >(() => {});

  const openFile = useCallback(
    async (path: string, options?: { newTab?: boolean }) => {
      await flushPending();
      const existing = tabsRef.current.find(
        (t) => t.type === "file" && t.path === path,
      );
      if (existing) {
        setActiveTabId(existing.id);
        return;
      }
      let content = "";
      if (!isBinaryKind(fileKind(path))) {
        try {
          content = await readFile(path);
        } catch (e) {
          console.error("read failed", path, e);
          return;
        }
      }
      const nextTab: FileTab = {
        type: "file",
        id: newId(),
        path,
        content,
        dirty: false,
        lastSavedAt: null,
      };
      const currentId = activeTabIdRef.current;
      setTabs((prev) => {
        const currentTab = prev.find((t) => t.id === currentId) ?? null;
        // Only replace an existing FILE tab in place; chat tabs aren't
        // swapped out by opening a file.
        if (
          options?.newTab ||
          prev.length === 0 ||
          !currentId ||
          !currentTab ||
          currentTab.type !== "file"
        ) {
          return [...prev, nextTab];
        }
        return prev.map((t) => (t.id === currentId ? nextTab : t));
      });
      setActiveTabId(nextTab.id);
    },
    [flushPending],
  );
  openFileRef.current = openFile;

  const openChatAsTab = useCallback(
    (chatId: string, title: string) => {
      const existing = tabsRef.current.find(
        (t) => t.type === "chat" && t.chatId === chatId,
      );
      if (existing) {
        setActiveTabId(existing.id);
        return;
      }
      const nextTab: ChatTab = {
        type: "chat",
        id: chatId,
        chatId,
        title,
      };
      setTabs((prev) => [...prev, nextTab]);
      setActiveTabId(nextTab.id);
    },
    [],
  );

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

  const flushPendingRef = useRef<() => Promise<void> | void>(() => {});
  const closeTabRef = useRef<(tabId: string) => void>(() => {});

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

  flushPendingRef.current = flushPending;
  closeTabRef.current = closeTab;

  const onEditorChangeImpl = useCallback(
    (newContent: string) => {
      const activeId = activeTabIdRef.current;
      if (!activeId) return;
      const currentTabs = tabsRef.current;
      const tab = currentTabs.find((t) => t.id === activeId);
      if (!tab || tab.type !== "file") return;
      // Hot path: keep live text in a ref, never rebuild the tabs array
      // per keystroke. The previous version cloned the whole tabs list
      // (with the full doc string) on every character, which made
      // App.tsx re-render and forced @uiw/react-codemirror to re-do its
      // O(N) doc.toString() echo check. Now: only flip the dirty bit on
      // the clean→dirty transition. flushPending writes the latest ref
      // value to disk and back into tab.content; tab switches read
      // pendingWrites first so unsaved typing is never lost.
      pendingWrites.current.set(tab.path, newContent);
      if (!tab.dirty) {
        setTabs((prev) =>
          prev.map((t) =>
            t.id === activeId && t.type === "file"
              ? { ...t, dirty: true }
              : t,
          ),
        );
      }
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

  // Synchronous-resolver ref so the cm-wikilinks short-ref check has
  // access to the latest vault tree without forcing extension rebuilds.
  const treeRef = useRef(tree);
  treeRef.current = tree;

  const stable = useMemo(
    () => ({
      onEditorChange: (v: string) => onEditorChangeRef.current(v),
      openByTarget: (t: string) => openByTargetRef.current(t),
      openFile: (p: string, opts?: { newTab?: boolean }) =>
        openFileRef.current(p, opts),
      openChatAsTab: (chatId: string, title: string) =>
        openChatAsTab(chatId, title),
      // Sync resolver — cm-wikilinks calls this on every `[short-ref]`
      // candidate. Returns the absolute path on hit, null on miss. Same
      // tree-walking logic as openByTarget, just exposed synchronously.
      resolveTarget: (t: string) => resolveInTree(treeRef.current, t),
    }),
    [openChatAsTab],
  );

  // ── Shortcuts ─────────────────────────────────────────────────────────

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      if (!mod) return;
      // Ctrl+` toggles the terminal panel. `e.key` for the backtick is
      // literally "`"; some layouts emit "Backquote" via e.code so we
      // accept both.
      if (e.key === "`" || e.code === "Backquote") {
        e.preventDefault();
        setTerminalOpen((v) => !v);
        return;
      }
      const key = e.key.toLowerCase();
      if (key === "s") {
        e.preventDefault();
        flushPendingRef.current();
      } else if (key === "b") {
        e.preventDefault();
        setSidebarVisible((v) => !v);
      } else if (key === "l" && e.shiftKey) {
        e.preventDefault();
        setChatVisible((v) => !v);
      } else if (key === ",") {
        e.preventDefault();
        setSettingsModal("general");
      } else if (key === "g" && e.shiftKey) {
        e.preventDefault();
        setGraphOpen(true);
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
        const id = activeTabIdRef.current;
        if (id) closeTabRef.current(id);
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
  }, []);

  // OS file drag-drop. Tauri (with `dragDropEnabled: true` in
  // tauri.conf.json) emits a drop event when the user drags files
  // from Finder/Explorer onto the Forge window. We forward each
  // dropped path through openFile, which already handles read +
  // tab spawn for any file kind Forge knows.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void (async () => {
      try {
        const { getCurrentWebview } = await import("@tauri-apps/api/webview");
        const fn = await getCurrentWebview().onDragDropEvent((event) => {
          if (event.payload.type !== "drop") return;
          const paths = (event.payload as { paths?: string[] }).paths ?? [];
          for (const p of paths) {
            openFileRef.current(p, { newTab: true });
          }
        });
        if (cancelled) {
          fn();
        } else {
          unlisten = fn;
        }
      } catch (e) {
        console.warn("drag-drop listener init failed:", e);
      }
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

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
      await loadVaultSettings(picked);
      // AppSettings carries the boot-time vault pointer. Persist eagerly
      // so closing/reopening the app comes back to the same place.
      try {
        const cur = await getAppSettings().catch<AppSettings | null>(() => null);
        const next: AppSettings = { ...(cur ?? { last_opened_vault: null }), last_opened_vault: picked };
        await setAppSettings(next);
      } catch (e) {
        console.warn("AppSettings persist failed", e);
      }
    } catch (e) {
      console.error(e);
    }
  };
  const pickVaultRef = useRef(pickVault);
  pickVaultRef.current = pickVault;
  const stablePickVault = useMemo(() => () => pickVaultRef.current(), []);

  // ── Drag handlers ─────────────────────────────────────────────────────

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
    (tab: TabKind) => {
      if (tab === "graph") {
        setGraphOpen(true);
        return;
      }
      // "files" | "search" | "chats" — sidebar tabs
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
    () => (tab: TabKind) => handleRailChangeRef.current(tab),
    [],
  );

  const toggleTheme = useCallback(
    () => setTheme((t) => (t === "light" ? "dark" : "light")),
    [],
  );

  // ── Derived display ────────────────────────────────────────────────

  const vaultName = vault ? (vault.split("/").pop() ?? null) : null;
  const wordCount = useMemo(() => {
    if (!activeFileTab) return 0;
    return activeFileTab.content.trim().split(/\s+/).filter(Boolean).length;
  }, [activeFileTab]);
  const activeTitle =
    activeTab === null
      ? null
      : activeTab.type === "file"
        ? tabTitle(activeTab.path)
        : activeTab.title;
  const saveStatus = activeFileTab?.dirty
    ? "Unsaved"
    : activeFileTab?.lastSavedAt
      ? "Saved"
      : "";

  // Status-bar file name: chat titles render plain, file names get a .md
  // suffix (matches the prototype).
  const statusBarDocLabel =
    activeTab === null
      ? null
      : activeTab.type === "chat"
        ? activeTab.title
        : `${tabTitle(activeTab.path)}.md`;

  const railActive: TabKind =
    sidebarVisible ? sidebarTab : "files";

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100vh",
        width: "100vw",
        background: "var(--background-secondary)",
        color: "var(--text-normal)",
        overflow: "hidden",
      }}
    >
      <TitleBar />
      <div
        className="workspace"
        style={{
          display: "flex",
          flex: 1,
          minHeight: 0,
          width: "100%",
          background: "var(--background-secondary)",
          color: "var(--text-normal)",
          overflow: "hidden",
        }}
      >
      <LeftRail
        active={railActive}
        onChange={stableHandleRailChange}
        onOpenSettings={() => setSettingsModal("general")}
        theme={theme}
        onToggleTheme={toggleTheme}
        onDictate={handleDictate}
        dictationActive={dictationActive}
      />

      {sidebarVisible && (
        <>
          <div
            className="workspace-split"
            style={{
              flexShrink: 0,
              height: "100%",
              overflow: "hidden",
              background: "var(--background-secondary)",
              width: "var(--sidebar-width)",
            }}
          >
            {sidebarTab === "files" && (
              <Sidebar
                vaultName={vaultName}
                tree={tree}
                activePath={activeFileTab?.path ?? null}
                onPickVault={stablePickVault}
                onOpenFile={stable.openFile}
                dirtyPaths={dirtyPaths}
                promotedPaths={promotedPaths}
                fileOps={{
                  onCreateAt: async (parentDir) => {
                    const name = window.prompt(
                      "New file name (e.g. notes/topic.md)",
                      parentDir ? `${parentDir}/untitled.md` : "untitled.md",
                    );
                    if (!name) return;
                    const path = name.endsWith(".md") ? name : `${name}.md`;
                    try {
                      await writeFile(path, "");
                      const t = await listVaultTree();
                      setTree(t);
                      stable.openFile(path);
                    } catch (e) {
                      console.error("create file failed:", e);
                      window.alert(`Create failed: ${e}`);
                    }
                  },
                  onRename: async (path) => {
                    const next = window.prompt(
                      "Rename to (path relative to vault root)",
                      path,
                    );
                    if (!next || next === path) return;
                    const dest = next.endsWith(".md") ? next : `${next}.md`;
                    try {
                      await renameFile(path, dest);
                      const t = await listVaultTree();
                      setTree(t);
                    } catch (e) {
                      console.error("rename failed:", e);
                      window.alert(`Rename failed: ${e}`);
                    }
                  },
                  onDuplicate: async (path) => {
                    // Insert " copy" before the extension. Skip the
                    // .md trailing if any.
                    const dot = path.lastIndexOf(".");
                    const stem = dot > 0 ? path.slice(0, dot) : path;
                    const ext = dot > 0 ? path.slice(dot) : ".md";
                    let dest = `${stem} copy${ext}`;
                    let i = 2;
                    // Naive uniqueness: try "copy", "copy 2", "copy 3"
                    // until a write succeeds. Tauri write_file overwrites,
                    // so we instead read first to check existence.
                    while (true) {
                      try {
                        await readFile(dest);
                        dest = `${stem} copy ${i}${ext}`;
                        i++;
                        if (i > 50) break;
                      } catch {
                        break;
                      }
                    }
                    try {
                      const content = await readFile(path);
                      await writeFile(dest, content);
                      const t = await listVaultTree();
                      setTree(t);
                    } catch (e) {
                      console.error("duplicate failed:", e);
                      window.alert(`Duplicate failed: ${e}`);
                    }
                  },
                  onDelete: async (path) => {
                    if (!window.confirm(`Delete ${path}?`)) return;
                    try {
                      await deleteFile(path);
                      const t = await listVaultTree();
                      setTree(t);
                    } catch (e) {
                      console.error("delete failed:", e);
                      window.alert(`Delete failed: ${e}`);
                    }
                  },
                  onPaste: async (parentDir) => {
                    const c = readFileTreeClip();
                    if (!c) return;
                    const base = c.path.split("/").pop() ?? "untitled.md";
                    const dest = parentDir ? `${parentDir}/${base}` : base;
                    if (dest === c.path) {
                      window.alert("Source and destination are the same.");
                      return;
                    }
                    try {
                      // Existence check: avoid stomping a same-named file.
                      try {
                        await readFile(dest);
                        window.alert(`A file already exists at ${dest}.`);
                        return;
                      } catch {}
                      if (c.mode === "cut") {
                        await renameFile(c.path, dest);
                      } else {
                        const content = await readFile(c.path);
                        await writeFile(dest, content);
                      }
                      clearFileTreeClip();
                      const t = await listVaultTree();
                      setTree(t);
                    } catch (e) {
                      console.error("paste failed:", e);
                      window.alert(`Paste failed: ${e}`);
                    }
                  },
                }}
              />
            )}
            {sidebarTab === "search" && (
              <Search onOpenFile={stable.openFile} />
            )}
            {sidebarTab === "chats" && (
              <ChatHistorySidebar
                vaultPath={vault}
                reloadKey={chatReloadKey}
                onOpenChat={stable.openChatAsTab}
              />
            )}
          </div>
          <ResizeHandle
            onResize={handleSidebarResize}
            onDone={handleSidebarResizeDone}
          />
        </>
      )}

      <main
        className="workspace-split mod-root"
        style={{
          flex: 1,
          minWidth: 0,
          height: "100%",
          display: "flex",
        }}
      >
        <div
          className="workspace-leaf"
          style={{
            flex: 1,
            minWidth: 0,
            height: "100%",
            display: "flex",
            flexDirection: "column",
            background: "var(--background-primary)",
            borderLeft: "1px solid var(--background-modifier-border)",
          }}
        >
          {/* Tab bar */}
          <div
            className="workspace-tab-header-container"
            style={{
              flexShrink: 0,
              display: "flex",
              alignItems: "stretch",
              background: "var(--background-secondary)",
              borderBottom: "1px solid var(--background-modifier-border)",
              height: 40,
              minHeight: 40,
            }}
          >
            <div
              style={{
                flex: 1,
                minWidth: 0,
                display: "flex",
                alignItems: "stretch",
                overflowX: "auto",
                overflowY: "hidden",
              }}
            >
              {tabs.length === 0 && (
                <div
                  style={{
                    padding: "0 16px",
                    display: "flex",
                    alignItems: "center",
                    fontSize: 11,
                    textTransform: "uppercase",
                    letterSpacing: "0.08em",
                    color: "var(--text-faint)",
                  }}
                >
                  No file open
                </div>
              )}
              {tabs.map((tab) => (
                <TabHeader
                  key={tab.id}
                  tab={tab}
                  active={tab.id === activeTabId}
                  onSelect={() => switchTab(tab.id)}
                  onClose={() => closeTab(tab.id)}
                />
              ))}
            </div>

            <TabBarRightActions
              activeIsFile={activeTab?.type === "file"}
              activeIsChat={activeTab?.type === "chat"}
              activeIsMarkdown={
                activeFileTab !== null &&
                fileKind(activeFileTab.path) === "markdown"
              }
              mdZoom={mdZoom}
              onZoomOut={() =>
                setMdZoom((z) => Math.max(0.6, +(z - 0.1).toFixed(2)))
              }
              onZoomReset={() => setMdZoom(1)}
              onZoomIn={() =>
                setMdZoom((z) => Math.min(2.5, +(z + 0.1).toFixed(2)))
              }
              readMode={readMode}
              onToggleReadMode={() => setReadMode((r) => !r)}
              readableWidth={readableWidth}
              onToggleReadableWidth={() => setReadableWidth((r) => !r)}
              tocOpen={tocOpen}
              onToggleToc={() => setTocOpen((t) => !t)}
              chatOpen={chatVisible}
              onToggleChat={() => setChatVisible((v) => !v)}
            />
          </div>

          {/* Content */}
          <ErrorBoundary>
            {(() => {
              if (!activeTab) {
                return (
                  <Editor
                    path={null}
                    title={null}
                    content={""}
                    readableWidth={readableWidth}
                    onChange={stable.onEditorChange}
                    onOpenPath={stable.openByTarget}
                    tocOpen={tocOpen}
                    onEditorMount={handleEditorMount}
                    resolveTarget={stable.resolveTarget}
                  />
                );
              }
              if (activeTab.type === "chat") {
                return (
                  <ChatTabView
                    ref={chatTabRef}
                    key={activeTab.id}
                    vaultPath={vault}
                    chatId={activeTab.chatId}
                    onOpenAiSettings={() => setSettingsModal("ai")}
                    onOpenFile={(p) => stable.openFile(p)}
                    onChatPersisted={bumpChatReload}
                    tocOpen={tocOpen}
                    readableWidth={readableWidth}
                    fontScale={mdZoom}
                  />
                );
              }
              // File tab
              const kind = fileKind(activeTab.path);
              if (kind === "latex") {
                return (
                  <LatexViewer
                    key={activeTab.id}
                    path={activeTab.path}
                    title={activeTitle}
                  />
                );
              }
              if (kind === "pdf") {
                return (
                  <PdfViewer
                    key={activeTab.id}
                    path={activeTab.path}
                    title={activeTitle}
                  />
                );
              }
              if (kind === "image") {
                return (
                  <ImageViewer
                    key={activeTab.id}
                    path={activeTab.path}
                    title={activeTitle}
                  />
                );
              }
              if (kind === "docx") {
                return (
                  <DocxViewer
                    key={activeTab.id}
                    path={activeTab.path}
                    title={activeTitle}
                  />
                );
              }
              // One render pipeline for .md (mdeditor.md §1). Editor
              // always renders; readMode controls the pose. Wikilinks,
              // math, tables, live-preview mark-hides all work the
              // same in both poses.
              return (
                <Editor
                  path={activeTab.path}
                  title={activeTitle}
                  // Prefer in-flight unsaved text (pendingWrites) over
                  // tab.content. Since onEditorChangeImpl no longer
                  // pushes content into tab state per keystroke, a
                  // tab-switch within WRITE_DEBOUNCE_MS of the last
                  // keystroke would otherwise show stale content. The
                  // ref read is O(1) and stays in sync because the
                  // editor's own onChange feeds it directly.
                  content={
                    pendingWrites.current.get(activeTab.path) ??
                    activeTab.content
                  }
                  readableWidth={readableWidth}
                  fontScale={mdZoom}
                  readOnly={readMode}
                  onChange={stable.onEditorChange}
                  onOpenPath={stable.openByTarget}
                  tocOpen={tocOpen}
                  onEditorMount={handleEditorMount}
                  resolveTarget={stable.resolveTarget}
                />
              );
            })()}
          </ErrorBoundary>

          {/* Bottom terminal panel — sits between content and status
              bar. Top edge is a horizontal resize handle. */}
          {terminalOpen && (
            <div
              style={{
                flexShrink: 0,
                height: terminalHeight,
                display: "flex",
                flexDirection: "column",
                borderTop: "1px solid var(--background-modifier-border)",
                background: "var(--background-primary)",
                minHeight: 80,
              }}
            >
              <HorizontalResizeHandle
                onResize={(deltaY) =>
                  setTerminalHeight((h) =>
                    Math.max(80, Math.min(window.innerHeight - 200, h - deltaY)),
                  )
                }
              />
              {/* No key=theme: remounting kills the shell. Theme flips
                  during an active terminal session leave stale palette
                  until next reopen, which is the lesser evil. */}
              <div style={{ flex: 1, minHeight: 0, position: "relative" }}>
                <TerminalPanel vaultPath={vault} />
              </div>
            </div>
          )}

          {/* Status bar */}
          <StatusBar
            vaultName={vaultName}
            docLabel={statusBarDocLabel}
            wordCount={activeFileTab ? wordCount : null}
            saveStatus={saveStatus}
            isFileTab={activeTab?.type === "file"}
            terminalOpen={terminalOpen}
            onToggleTerminal={() => setTerminalOpen((v) => !v)}
          />
        </div>

        {/* Hide the chat sidebar when the user is already inside a chat
            tab — two chat surfaces side-by-side is just noise. */}
        {chatVisible && activeTab?.type !== "chat" && (
          <>
            <ResizeHandle
              onResize={handleChatResize}
              onDone={handleChatResizeDone}
            />
            <aside
              className="workspace-leaf"
              style={{
                flexShrink: 0,
                height: "100%",
                borderLeft: "1px solid var(--background-modifier-border)",
                background: "var(--background-primary)",
                width: "var(--chat-width)",
              }}
            >
              <Chat
                ref={chatRef}
                vaultPath={vault}
                chatId={null}
                onOpenAiSettings={() => setSettingsModal("ai")}
                onOpenChatAsTab={stable.openChatAsTab}
                onOpenFile={(p) => stable.openFile(p)}
                onChatPersisted={bumpChatReload}
              />
            </aside>
          </>
        )}
      </main>

      <SearchModal
        open={searchModalOpen}
        onClose={() => setSearchModalOpen(false)}
        onOpenFile={stable.openFile}
      />
      {/* Both modals are lazy-loaded; Suspense's null fallback keeps
          the user from seeing a fallback flash while the chunk loads. */}
      {settingsModal === "general" && (
        <Suspense fallback={null}>
          <GeneralSettingsModal
            open
            onClose={() => setSettingsModal(null)}
            theme={theme}
            onToggleTheme={toggleTheme}
            vaultPath={vault}
          />
        </Suspense>
      )}
      {settingsModal === "ai" && (
        <Suspense fallback={null}>
          <AISettingsModal
            open
            onClose={() => setSettingsModal(null)}
            vaultPath={vault}
          />
        </Suspense>
      )}
      {/* key={theme} forces a remount on theme flip so the canvas
          re-resolves CSS-var colours. The component reads them once
          per draw via getComputedStyle but caches some via memos —
          remounting is cheaper than threading a watcher through. */}
      <GraphView
        key={theme}
        open={graphOpen}
        theme={theme}
        activePath={activeFileTab?.path ?? null}
        onOpenFile={(p) => {
          setGraphOpen(false);
          stable.openFile(p);
        }}
        onClose={() => setGraphOpen(false)}
      />
      </div>
    </div>
  );
}

// ── Sub-components ───────────────────────────────────────────────────────

interface TabHeaderProps {
  tab: Tab;
  active: boolean;
  onSelect: () => void;
  onClose: () => void;
}

function TabHeader({ tab, active, onSelect, onClose }: TabHeaderProps) {
  const isChat = tab.type === "chat";
  const label = isChat ? tab.title : tabTitle(tab.path);
  const isDirty = tab.type === "file" && tab.dirty;
  const style: CSSProperties = {
    display: "flex",
    alignItems: "center",
    gap: 6,
    padding: "0 10px 0 12px",
    maxWidth: 220,
    minWidth: 0,
    fontSize: "var(--font-ui-small)",
    color: active ? "var(--text-normal)" : "var(--text-muted)",
    fontWeight: active ? 500 : 400,
    background: active
      ? "var(--background-primary)"
      : "var(--background-secondary)",
    borderRight: "1px solid var(--background-modifier-border)",
    cursor: "pointer",
    position: "relative",
    whiteSpace: "nowrap",
    flexShrink: 0,
  };
  return (
    <div
      onClick={onSelect}
      onAuxClick={(e) => {
        if (e.button === 1) {
          e.preventDefault();
          onClose();
        }
      }}
      style={style}
      onMouseEnter={(e: MouseEvent<HTMLDivElement>) => {
        if (!active)
          e.currentTarget.style.background =
            "var(--background-modifier-hover)";
      }}
      onMouseLeave={(e: MouseEvent<HTMLDivElement>) => {
        if (!active)
          e.currentTarget.style.background =
            "var(--background-secondary)";
      }}
    >
      {active && (
        <span
          style={{
            position: "absolute",
            left: 0,
            right: 0,
            bottom: -1,
            height: 1,
            background: "var(--background-primary)",
          }}
        />
      )}
      {isChat && (
        <span
          style={{
            color: "var(--text-muted)",
            display: "flex",
            flexShrink: 0,
          }}
        >
          <MessageSquare size={12} />
        </span>
      )}
      <span
        style={{
          flex: 1,
          overflow: "hidden",
          textOverflow: "ellipsis",
        }}
      >
        {label}
      </span>
      {isDirty && (
        <span
          title="Unsaved changes"
          style={{
            width: 6,
            height: 6,
            borderRadius: 999,
            background: "var(--interactive-accent)",
            flexShrink: 0,
          }}
        />
      )}
      <span
        onClick={(e) => {
          e.stopPropagation();
          onClose();
        }}
        style={{
          width: 16,
          height: 16,
          display: "inline-flex",
          alignItems: "center",
          justifyContent: "center",
          borderRadius: "var(--radius-s)",
          color: "var(--text-faint)",
          opacity: active ? 1 : 0,
          cursor: "pointer",
        }}
      >
        <X size={10} />
      </span>
    </div>
  );
}

interface TabBarRightActionsProps {
  activeIsFile: boolean;
  activeIsChat: boolean;
  activeIsMarkdown: boolean;
  mdZoom: number;
  onZoomOut: () => void;
  onZoomReset: () => void;
  onZoomIn: () => void;
  readMode: boolean;
  onToggleReadMode: () => void;
  readableWidth: boolean;
  onToggleReadableWidth: () => void;
  tocOpen: boolean;
  onToggleToc: () => void;
  chatOpen: boolean;
  onToggleChat: () => void;
}

function TabBarRightActions(props: TabBarRightActionsProps) {
  const {
    activeIsFile,
    activeIsChat,
    activeIsMarkdown,
    mdZoom,
    onZoomOut,
    onZoomReset,
    onZoomIn,
    readMode,
    onToggleReadMode,
    readableWidth,
    onToggleReadableWidth,
    tocOpen,
    onToggleToc,
    chatOpen,
    onToggleChat,
  } = props;
  // Zoom + readable-width + TOC apply to anything text-shaped: markdown
  // notes AND chat tabs (each user prompt becomes a TOC entry).
  const showZoom = activeIsMarkdown || activeIsChat;
  const showWidth = activeIsFile || activeIsChat;
  const showToc = activeIsFile || activeIsChat;

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 2,
        paddingRight: 8,
        flexShrink: 0,
      }}
    >
      {showZoom && (
        <>
          <GhostBtn
            icon={<ZoomOut size={15} strokeWidth={1.8} />}
            label="Zoom out (Ctrl+-)"
            onClick={onZoomOut}
            size={28}
          />
          <button
            onClick={onZoomReset}
            title={`Reset zoom (current ${Math.round(mdZoom * 100)}%)`}
            style={{
              height: 28,
              minWidth: 40,
              padding: "0 6px",
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              background: "transparent",
              border: 0,
              borderRadius: "var(--radius-s)",
              color: "var(--text-muted)",
              fontSize: 10,
              fontVariantNumeric: "tabular-nums",
              cursor: "pointer",
            }}
            onMouseEnter={(e: MouseEvent<HTMLButtonElement>) => {
              e.currentTarget.style.background =
                "var(--background-modifier-hover)";
              e.currentTarget.style.color = "var(--text-normal)";
            }}
            onMouseLeave={(e: MouseEvent<HTMLButtonElement>) => {
              e.currentTarget.style.background = "transparent";
              e.currentTarget.style.color = "var(--text-muted)";
            }}
          >
            {Math.round(mdZoom * 100)}%
          </button>
          <GhostBtn
            icon={<ZoomIn size={15} strokeWidth={1.8} />}
            label="Zoom in (Ctrl+=)"
            onClick={onZoomIn}
            size={28}
          />
          <Separator />
        </>
      )}
      {activeIsFile && (
        <>
          {/* Single toggle: icon reflects the CURRENT pose. Clicking
              flips to the other pose. Eye = currently reading (click
              to edit); PenLine = currently editing (click to read). */}
          <GhostBtn
            icon={
              readMode ? <Eye size={15} /> : <PenLine size={15} />
            }
            label={
              readMode
                ? "Reading — click to edit (Ctrl+E)"
                : "Editing — click to read (Ctrl+E)"
            }
            onClick={onToggleReadMode}
            size={28}
          />
          <Separator />
        </>
      )}
      {showWidth && (
        <>
          <GhostBtn
            icon={<AlignLeft size={15} />}
            label="Toggle reading width"
            onClick={onToggleReadableWidth}
            active={readableWidth}
            size={28}
          />
          <Separator />
        </>
      )}
      {showToc && (
        <GhostBtn
          icon={<ListTree size={16} />}
          label="Table of contents"
          onClick={onToggleToc}
          active={tocOpen}
          size={28}
        />
      )}
      <GhostBtn
        icon={<PanelRight size={16} />}
        label="Toggle chat"
        onClick={onToggleChat}
        active={chatOpen}
        size={28}
      />
    </div>
  );
}

function Separator() {
  return (
    <span
      style={{
        width: 1,
        height: 16,
        background: "var(--background-modifier-border)",
        margin: "0 2px",
      }}
    />
  );
}

// Vertical-axis drag handle for the bottom terminal panel. The shared
// ResizeHandle is hard-coded for column-resize, so a small dedicated
// version lives here rather than refactoring that shared component.
function HorizontalResizeHandle({ onResize }: { onResize: (deltaY: number) => void }) {
  const lastY = useRef<number | null>(null);
  const dragging = useRef(false);

  useEffect(() => {
    // The `MouseEvent` symbol in this file is React's synthetic alias;
    // listeners on `window` need the DOM type, hence the explicit cast.
    const move = (e: globalThis.MouseEvent) => {
      if (!dragging.current || lastY.current === null) return;
      const delta = e.clientY - lastY.current;
      lastY.current = e.clientY;
      onResize(delta);
    };
    const up = () => {
      if (!dragging.current) return;
      dragging.current = false;
      lastY.current = null;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
    return () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
  }, [onResize]);

  return (
    <div
      onMouseDown={(e) => {
        e.preventDefault();
        dragging.current = true;
        lastY.current = e.clientY;
        document.body.style.cursor = "row-resize";
        document.body.style.userSelect = "none";
      }}
      style={{
        height: 4,
        cursor: "row-resize",
        flexShrink: 0,
        background: "transparent",
      }}
    />
  );
}

interface StatusBarProps {
  vaultName: string | null;
  docLabel: string | null;
  wordCount: number | null;
  saveStatus: string;
  isFileTab: boolean;
  terminalOpen: boolean;
  onToggleTerminal: () => void;
}

function StatusBar({
  vaultName,
  docLabel,
  wordCount,
  saveStatus,
  isFileTab,
  terminalOpen,
  onToggleTerminal,
}: StatusBarProps) {
  const inlineBtn: CSSProperties = {
    display: "inline-flex",
    alignItems: "center",
    gap: 4,
    background: "transparent",
    border: 0,
    cursor: "pointer",
    color: "var(--text-faint)",
    fontSize: "var(--font-ui-smaller)",
    fontFamily: "var(--font-monospace)",
    padding: "2px 6px",
    borderRadius: "var(--radius-s)",
  };
  const hover = (e: MouseEvent<HTMLButtonElement>) => {
    e.currentTarget.style.background = "var(--background-modifier-hover)";
    e.currentTarget.style.color = "var(--text-muted)";
  };
  const unhover = (e: MouseEvent<HTMLButtonElement>) => {
    e.currentTarget.style.background = "transparent";
    e.currentTarget.style.color = "var(--text-faint)";
  };

  return (
    <div
      className="workspace-statusbar"
      style={{
        flexShrink: 0,
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
        height: 26,
        padding: "0 14px",
        borderTop: "1px solid var(--background-modifier-border)",
        fontSize: "var(--font-ui-smaller)",
        color: "var(--text-faint)",
        background: "var(--background-secondary-alt)",
        fontFamily: "var(--font-monospace)",
      }}
    >
      <div
        style={{
          display: "flex",
          gap: 12,
          alignItems: "center",
          minWidth: 0,
        }}
      >
        <button
          title="Toggle terminal (Ctrl+`)"
          aria-label="Toggle terminal"
          onClick={onToggleTerminal}
          style={{
            ...inlineBtn,
            color: terminalOpen ? "var(--text-normal)" : inlineBtn.color,
            background: terminalOpen
              ? "var(--background-modifier-hover)"
              : inlineBtn.background,
          }}
          onMouseEnter={hover}
          onMouseLeave={(e) => {
            if (terminalOpen) {
              e.currentTarget.style.background =
                "var(--background-modifier-hover)";
              e.currentTarget.style.color = "var(--text-normal)";
            } else {
              unhover(e);
            }
          }}
        >
          <Terminal size={12} />
          Terminal
        </button>
        {vaultName && <span>{vaultName}</span>}
        {docLabel && (
          <span
            style={{
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              minWidth: 0,
            }}
          >
            {docLabel}
          </span>
        )}
        {wordCount !== null && isFileTab && (
          <>
            <span style={{ opacity: 0.4 }}>·</span>
            <span style={{ fontVariantNumeric: "tabular-nums" }}>
              {wordCount} words
            </span>
          </>
        )}
      </div>
      <div
        style={{
          display: "flex",
          gap: 12,
          alignItems: "center",
        }}
      >
        {/* Model + cost chips moved up into the chat surface; the bottom
            Dictate indicator moved into the LeftRail mic button. The
            status bar now just owns the file metadata + save state. */}
        {isFileTab && (
          <button
            title="Backlinks"
            style={inlineBtn}
            onMouseEnter={hover}
            onMouseLeave={unhover}
          >
            <Link2 size={12} />
            0 backlinks
          </button>
        )}
        {saveStatus && <span>{saveStatus}</span>}
      </div>
    </div>
  );
}
