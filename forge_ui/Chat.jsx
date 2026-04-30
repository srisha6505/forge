/* global React, Icons, GhostBtn, Chip, getChatHistory */

function ChatDock({ onOpenSettings, onOpenChatAsTab }) {
  const [input, setInput] = React.useState("");
  const [messages] = React.useState([
    { role: "user", text: "What did I decide about atomic writes?" },
    { role: "tool", name: "hybrid_search", summary: '3 results for "atomic write"' },
    { role: "assistant", text: 'You specced it in [[mdeditor#3-save-io|§3 Save IO]]:\n\n1. Write content to sibling `.tmp` file\n2. `fsync` the `.tmp` file\n3. Rename `.tmp` → target (atomic on POSIX)\n4. `fsync` the parent directory\n\nThis guarantees no half-written files on crash. The setting is exposed as a toggle in Editor settings, default on. You noted that Windows `MoveFileEx` with `MOVEFILE_REPLACE_EXISTING` is the closest equivalent but not truly atomic — acceptable given the target audience.' },
    { role: "user", text: "Can we also handle the case where the tmp file already exists from a previous crash?" },
    { role: "assistant", text: "Yes — on startup, scan for orphaned `.tmp` files in the vault root. For each:\n\n1. If the target file exists and is newer, delete the `.tmp`\n2. If the `.tmp` is newer or target is missing, rename `.tmp` → target\n3. Log the recovery to the console\n\nThis is cheap (single readdir) and handles the crash-during-write case. I'd add it to the vault-open initialization sequence." },
  ]);

  const renderMsg = (msg, i) => {
    if (msg.role === "tool") {
      return React.createElement("div", { key: i, style: { marginBottom: 12 } },
        React.createElement("div", {
          style: {
            display: "flex", alignItems: "center", gap: 6, padding: "6px 10px",
            background: "var(--background-modifier-message)", borderRadius: "var(--radius-m)",
            fontSize: "var(--font-ui-small)", color: "var(--text-muted)", cursor: "pointer",
          }
        },
          Icons.Search({ size: 14 }),
          React.createElement("span", { style: { fontFamily: "var(--font-monospace)", fontWeight: 500 } }, msg.name),
          React.createElement("span", { style: { color: "var(--text-faint)" } }, msg.summary),
          React.createElement("span", { style: { marginLeft: "auto" } }, Icons.ChevronRight({ size: 12 })),
        )
      );
    }
    return React.createElement("div", { key: i, style: { marginBottom: 16, paddingBottom: 16, borderBottom: i < messages.length - 1 ? "1px solid var(--hr-color)" : "none" } },
      React.createElement("div", {
        style: { fontSize: "var(--font-ui-smaller)", fontWeight: 500, textTransform: "uppercase", letterSpacing: "0.06em", color: "var(--text-faint)", marginBottom: 6 }
      }, msg.role === "user" ? "you" : "claude"),
      React.createElement("div", {
        style: { fontSize: 13.5, lineHeight: 1.6, color: "var(--text-normal)", fontFamily: "var(--font-text)", whiteSpace: "pre-wrap" }
      }, msg.text),
      msg.role === "assistant" && React.createElement("div", {
        style: { display: "flex", gap: 4, marginTop: 8, opacity: 0.7 }
      },
        React.createElement(GhostBtn, { icon: Icons.Copy({ size: 14 }), label: "Copy", size: 24 }),
        React.createElement(GhostBtn, { icon: Icons.ExternalLink({ size: 14 }), label: "Export to note", size: 24 }),
        React.createElement(GhostBtn, { icon: Icons.RotateCcw({ size: 14 }), label: "Regenerate", size: 24 }),
        /* Open chat as tab */
        React.createElement(GhostBtn, { icon: Icons.ArrowUpRight({ size: 14 }), label: "Open as tab", onClick: () => onOpenChatAsTab && onOpenChatAsTab("atomic-writes", "Atomic writes discussion"), size: 24 }),
      ),
    );
  };

  return React.createElement("div", {
    style: {
      gridColumn: 4, gridRow: 1, background: "var(--background-primary)",
      borderLeft: "1px solid var(--background-modifier-border)",
      display: "flex", flexDirection: "column", minWidth: 0,
    }
  },
    /* Header */
    React.createElement("div", {
      style: {
        height: 36, minHeight: 36, padding: "0 6px 0 14px", display: "flex",
        alignItems: "center", gap: 6, borderBottom: "1px solid var(--background-modifier-border)",
      }
    },
      React.createElement("span", { style: { flex: 1, fontSize: "var(--font-ui-small)", fontWeight: 500, color: "var(--text-normal)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" } }, "Atomic writes discussion"),
      React.createElement(Chip, { children: "claude-sonnet-4-6", active: true }),
      React.createElement(GhostBtn, { icon: Icons.MessageSquarePlus({ size: 14 }), label: "New conversation", size: 24 }),
      React.createElement(GhostBtn, { icon: Icons.MoreHorizontal({ size: 14 }), label: "More", size: 24 }),
    ),
    /* Messages */
    React.createElement("div", { style: { flex: 1, overflowY: "auto", padding: "14px 16px" } },
      messages.map(renderMsg),
    ),
    /* Composer */
    React.createElement("div", {
      style: {
        margin: "8px 12px 10px", background: "var(--background-primary-alt)",
        border: "1px solid var(--background-modifier-border)", borderRadius: "var(--radius-m)",
        display: "flex", alignItems: "center", padding: "6px 10px", gap: 8,
      }
    },
      React.createElement(Chip, { children: "claude-sonnet-4-6", active: false, onClick: onOpenSettings }),
      React.createElement("input", {
        value: input, onChange: (e) => setInput(e.target.value),
        placeholder: "Ask anything...",
        style: {
          flex: 1, border: 0, background: "transparent", outline: "none",
          fontSize: "var(--font-ui-medium)", color: "var(--text-normal)", minWidth: 0,
        }
      }),
      React.createElement("button", {
        style: {
          background: input ? "var(--interactive-accent)" : "var(--background-modifier-border)",
          color: input ? "var(--text-on-accent)" : "var(--text-faint)",
          border: 0, borderRadius: "var(--radius-s)", width: 28, height: 28,
          display: "flex", alignItems: "center", justifyContent: "center", cursor: input ? "pointer" : "default",
        }
      }, Icons.Send({ size: 14 })),
    ),
  );
}

/* ── Chat History Sidebar ── */
function ChatHistorySidebar({ onOpenChat }) {
  const chats = getChatHistory();
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
      React.createElement("span", { style: { flex: 1, fontSize: "var(--font-ui-small)", fontWeight: 600, color: "var(--text-normal)" } }, "Chats"),
      React.createElement(GhostBtn, { icon: Icons.MessageSquarePlus({ size: 14 }), label: "New chat", size: 24 }),
    ),
    /* Chat list */
    React.createElement("div", { style: { flex: 1, overflowY: "auto", padding: "4px 0 8px" } },
      chats.map(chat => React.createElement("div", {
        key: chat.id,
        onClick: () => onOpenChat(chat.id, chat.title),
        style: {
          padding: "8px 14px", cursor: "pointer",
          transition: "background var(--motion-duration-fast) var(--motion-ease)",
        },
        onMouseEnter: (e) => e.currentTarget.style.background = "var(--background-modifier-hover)",
        onMouseLeave: (e) => e.currentTarget.style.background = "transparent",
      },
        React.createElement("div", { style: { display: "flex", alignItems: "center", gap: 6 } },
          React.createElement("span", { style: { color: "var(--text-muted)", display: "flex" } }, Icons.MessageSquare({ size: 14 })),
          React.createElement("span", { style: { flex: 1, fontSize: "var(--font-ui-medium)", fontWeight: 500, color: "var(--text-normal)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" } }, chat.title),
        ),
        React.createElement("div", { style: { display: "flex", alignItems: "center", gap: 6, marginTop: 2, paddingLeft: 20 } },
          React.createElement("span", { style: { fontSize: "var(--font-ui-smaller)", color: "var(--text-faint)" } }, chat.date),
          React.createElement("span", { style: { fontSize: "var(--font-ui-smaller)", color: "var(--text-faint)" } }, "·"),
          React.createElement("span", { style: { fontSize: "var(--font-ui-smaller)", color: "var(--text-faint)" } }, chat.model),
        ),
      )),
    ),
    /* Footer */
    React.createElement("div", {
      style: {
        height: 26, padding: "0 12px", borderTop: "1px solid var(--background-modifier-border)",
        display: "flex", alignItems: "center",
        fontSize: "var(--font-ui-small)", color: "var(--text-muted)",
      }
    }, chats.length + " conversations"),
  );
}

Object.assign(window, { ChatDock, ChatHistorySidebar });
