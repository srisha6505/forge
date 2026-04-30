/* global React, Icons, LeftRail, FilesSidebar, EditorPane, ChatDock, ChatHistorySidebar, SettingsModal, AISettingsModal, TweaksPanel, useTweaks */

const TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{"theme":"dark","chatOpen":true,"sidebarOpen":true}/*EDITMODE-END*/;

function App() {
  const [tweaks, setTweak] = typeof useTweaks === "function" ? useTweaks(TWEAK_DEFAULTS) : [TWEAK_DEFAULTS, () => {}];

  const [activeTab, setActiveTab] = React.useState("files");
  const [activeFile, setActiveFile] = React.useState("france-military-comms");
  const [tabs, setTabs] = React.useState([
    { type: "file", id: "france-military-comms" },
    { type: "file", id: "procurement-paths" },
    { type: "file", id: "Untitled 5" },
  ]);
  const [settingsModal, setSettingsModal] = React.useState(null);
  const [tocOpen, setTocOpen] = React.useState(false);

  const themeClass = tweaks.theme === "dark" ? "theme-dark" : "theme-light";
  React.useEffect(() => { document.body.className = themeClass; }, [themeClass]);

  const onOpenFile = (name) => {
    const exists = tabs.find(t => t.type === "file" && t.id === name);
    if (!exists) setTabs([...tabs, { type: "file", id: name }]);
    setActiveFile(name);
  };

  const onOpenChatAsTab = (chatId, title) => {
    const tabId = "chat:" + chatId;
    const exists = tabs.find(t => t.type === "chat" && t.id === chatId);
    if (!exists) setTabs([...tabs, { type: "chat", id: chatId, title }]);
    setActiveFile(tabId);
  };

  const onCloseTab = (tabId) => {
    const isChat = tabId.startsWith("chat:");
    const next = tabs.filter(t => {
      const tid = t.type === "chat" ? "chat:" + t.id : t.id;
      return tid !== tabId;
    });
    setTabs(next);
    if (activeFile === tabId && next.length) {
      const first = next[0];
      setActiveFile(first.type === "chat" ? "chat:" + first.id : first.id);
    }
  };

  const onSelectTab = (tabId) => setActiveFile(tabId);

  const chatOpen = tweaks.chatOpen;
  const sidebarOpen = tweaks.sidebarOpen;
  const showChatHistory = activeTab === "chats";

  const cols = `var(--ribbon-width) ${sidebarOpen ? "var(--sidebar-width)" : "0px"} 1fr ${chatOpen ? "var(--chat-width)" : "0px"}`;

  return React.createElement(React.Fragment, null,
    React.createElement("div", {
      style: {
        display: "grid", gridTemplateColumns: cols,
        gridTemplateRows: "1fr var(--statusbar-height)",
        height: "100vh", width: "100vw",
      }
    },
      React.createElement(LeftRail, {
        activeTab, onTabChange: setActiveTab,
        theme: tweaks.theme,
        onToggleTheme: () => setTweak("theme", tweaks.theme === "dark" ? "light" : "dark"),
        onOpenSettings: () => setSettingsModal("ai"),
      }),
      sidebarOpen && (showChatHistory
        ? React.createElement(ChatHistorySidebar, { onOpenChat: onOpenChatAsTab })
        : React.createElement(FilesSidebar, { activeFile, onOpen: onOpenFile })
      ),
      React.createElement(EditorPane, {
        tabs, active: activeFile, onSelect: onSelectTab, onClose: onCloseTab,
        chatOpen, onToggleChat: () => setTweak("chatOpen", !chatOpen),
        onToggleTOC: () => setTocOpen(!tocOpen), tocOpen,
      }),
      chatOpen && React.createElement(ChatDock, {
        onOpenSettings: () => setSettingsModal("ai"),
        onOpenChatAsTab,
      }),
      /* Status bar */
      React.createElement("div", {
        style: {
          gridColumn: "1 / -1", gridRow: 2, height: "var(--statusbar-height)",
          padding: "0 14px", borderTop: "1px solid var(--background-modifier-border)",
          display: "flex", justifyContent: "space-between", alignItems: "center",
          fontSize: "var(--font-ui-smaller)", color: "var(--text-faint)",
          background: "var(--background-secondary-alt)", fontFamily: "var(--font-monospace)",
        }
      },
        React.createElement("div", { style: { display: "flex", gap: 12, alignItems: "center" } },
          /* Terminal toggle */
          React.createElement("button", {
            title: "Toggle terminal", "aria-label": "Toggle terminal",
            style: {
              display: "inline-flex", alignItems: "center", gap: 4,
              background: "transparent", border: 0, cursor: "pointer",
              color: "var(--text-faint)", fontSize: "var(--font-ui-smaller)",
              fontFamily: "var(--font-monospace)", padding: "2px 6px", borderRadius: "var(--radius-s)",
            },
            onMouseEnter: (e) => { e.currentTarget.style.background = "var(--background-modifier-hover)"; e.currentTarget.style.color = "var(--text-muted)"; },
            onMouseLeave: (e) => { e.currentTarget.style.background = "transparent"; e.currentTarget.style.color = "var(--text-faint)"; },
          }, Icons.Terminal({ size: 12 }), "Terminal"),
          React.createElement("span", null, "my-vault"),
          React.createElement("span", null, (activeFile.startsWith("chat:") ? activeFile.replace("chat:", "") : activeFile) + (activeFile.startsWith("chat:") ? "" : ".md")),
        ),
        React.createElement("div", { style: { display: "flex", gap: 12, alignItems: "center" } },
          /* Dictation indicator */
          React.createElement("button", {
            title: "Voice dictation — works in editor, chat, or anywhere with a text cursor",
            "aria-label": "Dictation",
            style: {
              display: "inline-flex", alignItems: "center", gap: 4,
              background: "transparent", border: 0, cursor: "pointer",
              color: "var(--text-faint)", fontSize: "var(--font-ui-smaller)",
              fontFamily: "var(--font-monospace)", padding: "2px 6px", borderRadius: "var(--radius-s)",
            },
            onMouseEnter: (e) => { e.currentTarget.style.background = "var(--background-modifier-hover)"; e.currentTarget.style.color = "var(--text-muted)"; },
            onMouseLeave: (e) => { e.currentTarget.style.background = "transparent"; e.currentTarget.style.color = "var(--text-faint)"; },
          }, Icons.Mic({ size: 12 }), "Dictate"),
          React.createElement("span", null, "claude-sonnet-4-6"),
          /* Backlinks count */
          !activeFile.startsWith("chat:") && React.createElement("button", {
            title: "Backlinks",
            style: {
              display: "inline-flex", alignItems: "center", gap: 4,
              background: "transparent", border: 0, cursor: "pointer",
              color: "var(--text-faint)", fontSize: "var(--font-ui-smaller)",
              fontFamily: "var(--font-monospace)", padding: "2px 6px", borderRadius: "var(--radius-s)",
            },
            onMouseEnter: (e) => { e.currentTarget.style.background = "var(--background-modifier-hover)"; e.currentTarget.style.color = "var(--text-muted)"; },
            onMouseLeave: (e) => { e.currentTarget.style.background = "transparent"; e.currentTarget.style.color = "var(--text-faint)"; },
          }, Icons.Link2({ size: 12 }), ({"france-military-comms": "3", "procurement-paths": "1"}[activeFile] || "0") + " backlinks"),
          React.createElement("span", null, "$0.024"),
          React.createElement("span", null, "Saved"),
        ),
      ),
    ),
    settingsModal === "general" && React.createElement(SettingsModal, { onClose: () => setSettingsModal(null) }),
    settingsModal === "ai" && React.createElement(AISettingsModal, { onClose: () => setSettingsModal(null) }),
  );
}

ReactDOM.createRoot(document.getElementById("root")).render(React.createElement(App));
