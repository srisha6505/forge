/* global React, Icons, GhostBtn */

/* ── Left Rail ── */
function LeftRail({ activeTab, onTabChange, theme, onToggleTheme, onOpenSettings }) {
  const top = [
    { id: "files", icon: Icons.Files, label: "Files" },
    { id: "search", icon: Icons.Search, label: "Search" },
    { id: "chats", icon: Icons.MessageSquare, label: "Chats" },
    { id: "graph", icon: Icons.GitFork, label: "Graph" },
  ];
  const RailBtn = ({ id, icon, label, onClick, active }) =>
    React.createElement("button", {
      onClick: onClick || (() => onTabChange(id)), title: label, "aria-label": label,
      style: {
        width: "100%", height: 36, display: "flex", alignItems: "center", justifyContent: "center",
        background: active ? "var(--background-modifier-active)" : "transparent",
        border: 0, color: active ? "var(--icon-color-active)" : "var(--icon-color)",
        cursor: "pointer", position: "relative",
        transition: "background var(--motion-duration-fast) var(--motion-ease), color var(--motion-duration-fast) var(--motion-ease)",
      },
      onMouseEnter: (e) => { if(!active) e.currentTarget.style.background = "var(--background-modifier-hover)"; },
      onMouseLeave: (e) => { if(!active) e.currentTarget.style.background = "transparent"; },
    },
      active && React.createElement("span", { style: { position: "absolute", left: 0, top: 6, bottom: 6, width: 2, background: "var(--interactive-accent)", borderRadius: "0 2px 2px 0" } }),
      icon({ size: 18 })
    );

  return React.createElement("div", {
    style: {
      gridColumn: 1, gridRow: "1/3", background: "var(--background-secondary-alt)",
      borderRight: "1px solid var(--background-modifier-border)",
      display: "flex", flexDirection: "column", alignItems: "stretch", padding: "8px 0",
    }
  },
    ...top.map(t => React.createElement(RailBtn, { key: t.id, ...t, active: activeTab === t.id })),
    React.createElement("div", { style: { flex: 1 } }),
    React.createElement(RailBtn, { id: "dictate", icon: Icons.Mic, label: "Dictation — universal voice input", active: false }),
    React.createElement(RailBtn, { id: "theme", icon: theme === "dark" ? Icons.Sun : Icons.Moon, label: "Toggle theme", onClick: onToggleTheme, active: false }),
    React.createElement(RailBtn, { id: "settings", icon: Icons.Settings, label: "Settings", onClick: onOpenSettings, active: false }),
  );
}

/* ── Sidebar: Files ── */
function FilesSidebar({ activeFile, onOpen }) {
  const tree = [
    { type: "folder", name: "my-vault", level: 0, open: true },
    { type: "folder", name: "projects", level: 1, open: true },
    { type: "file", name: "france-military-comms.md", level: 2 },
    { type: "file", name: "procurement-paths.md", level: 2, dirty: true },
    { type: "file", name: "ops-timeline.md", level: 2 },
    { type: "folder", name: "research", level: 1, open: false },
    { type: "folder", name: "archive", level: 1, open: false },
    { type: "file", name: "atomic-writes.md", level: 1, promoted: true },
    { type: "file", name: "design-system.md", level: 1 },
    { type: "file", name: "meeting-notes.md", level: 1, dirty: true },
  ];

  const slug = (n) => n.replace(".md", "");

  return React.createElement("div", {
    style: {
      gridColumn: 2, gridRow: 1, background: "var(--background-secondary)",
      borderRight: "1px solid var(--background-modifier-border)",
      display: "flex", flexDirection: "column", minWidth: 0, overflow: "hidden",
    }
  },
    /* Header */
    React.createElement("div", {
      style: {
        height: 36, minHeight: 36, padding: "0 8px 0 14px", display: "flex",
        alignItems: "center", gap: 4, borderBottom: "1px solid var(--background-modifier-border)",
      }
    },
      React.createElement("span", { style: { flex: 1, fontSize: "var(--font-ui-small)", fontWeight: 600, color: "var(--text-normal)" } }, "Files"),
      React.createElement(GhostBtn, { icon: Icons.Plus({ size: 14 }), label: "New file", size: 24 }),
      React.createElement(GhostBtn, { icon: Icons.MoreHorizontal({ size: 14 }), label: "More", size: 24 }),
    ),
    /* Tree */
    React.createElement("div", { style: { flex: 1, overflowY: "auto", padding: "4px 0 8px" } },
      tree.map((item, i) => {
        const isActive = item.type === "file" && slug(item.name) === activeFile;
        return React.createElement("div", {
          key: i,
          onClick: item.type === "file" ? () => onOpen(slug(item.name)) : undefined,
          style: {
            height: 28, display: "flex", alignItems: "center", gap: 4,
            paddingLeft: 8 + item.level * 14, paddingRight: 12,
            fontSize: "var(--font-ui-medium)", cursor: "pointer",
            color: isActive ? "var(--text-accent)" : "var(--text-normal)",
            background: isActive ? "var(--background-modifier-active)" : "transparent",
            transition: "background var(--motion-duration-fast) var(--motion-ease)",
            userSelect: "none",
          },
          onMouseEnter: (e) => { if(!isActive) e.currentTarget.style.background = "var(--background-modifier-hover)"; },
          onMouseLeave: (e) => { if(!isActive) e.currentTarget.style.background = isActive ? "var(--background-modifier-active)" : "transparent"; },
        },
          /* chevron */
          item.type === "folder"
            ? React.createElement("span", { style: { width: 14, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--text-muted)" } }, item.open ? Icons.ChevronDown({ size: 12 }) : Icons.ChevronRight({ size: 12 }))
            : React.createElement("span", { style: { width: 14 } }),
          /* icon */
          React.createElement("span", { style: { color: "var(--text-muted)", display: "flex" } },
            item.type === "folder" ? (item.open ? Icons.FolderOpen({ size: 14 }) : Icons.Folder({ size: 14 })) : Icons.FileText({ size: 14 })
          ),
          /* promoted glyph */
          item.promoted && React.createElement("span", { style: { fontSize: 10, color: "var(--text-faint)", marginLeft: -2 } }, "✦"),
          /* label */
          React.createElement("span", {
            style: {
              flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
              fontWeight: item.type === "folder" ? 500 : 400,
              color: item.type === "folder" ? "var(--text-muted)" : undefined,
            }
          }, item.name),
          /* dirty dot */
          item.dirty && React.createElement("span", { style: { width: 6, height: 6, borderRadius: 999, background: "var(--text-accent)", flexShrink: 0 } }),
        );
      })
    ),
    /* Footer */
    React.createElement("div", {
      style: {
        height: 26, padding: "0 12px", borderTop: "1px solid var(--background-modifier-border)",
        display: "flex", alignItems: "center", justifyContent: "space-between",
        fontSize: "var(--font-ui-small)", color: "var(--text-muted)",
      }
    },
      React.createElement("span", null, "my-vault"),
      Icons.ChevronDown({ size: 12 }),
    ),
  );
}

Object.assign(window, { LeftRail, FilesSidebar });
