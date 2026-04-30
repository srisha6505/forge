/* global React, Icons, GhostBtn, SegCtrl, Chip */

/* ── Tab bar + Document + Chat-as-tab ── */
function EditorPane({ tabs, active, onSelect, onClose, chatOpen, onToggleChat, onToggleTOC, tocOpen }) {
  return React.createElement("div", {
    style: { gridColumn: 3, gridRow: 1, background: "var(--background-primary)", display: "flex", flexDirection: "column", minWidth: 0, overflow: "hidden" }
  },
    /* Tab bar */
    React.createElement("div", {
      style: {
        height: 40, minHeight: 40, display: "flex", background: "var(--background-secondary)",
        borderBottom: "1px solid var(--background-modifier-border)", alignItems: "stretch",
      }
    },
      tabs.map(t => {
        const isChat = t.type === "chat";
        const label = isChat ? t.title : t.id;
        const id = isChat ? "chat:" + t.id : t.id;
        const isActive = active === id;
        return React.createElement("div", {
          key: id, onClick: () => onSelect(id),
          style: {
            display: "flex", alignItems: "center", gap: 6, padding: "0 10px 0 12px",
            maxWidth: 220, minWidth: 0, fontSize: "var(--font-ui-small)",
            color: isActive ? "var(--text-normal)" : "var(--text-muted)",
            fontWeight: isActive ? 500 : 400,
            background: isActive ? "var(--background-primary)" : "var(--background-secondary)",
            borderRight: "1px solid var(--background-modifier-border)",
            cursor: "pointer", position: "relative", whiteSpace: "nowrap",
          },
          onMouseEnter: (e) => { if(!isActive) e.currentTarget.style.background = "var(--background-modifier-hover)"; },
          onMouseLeave: (e) => { if(!isActive) e.currentTarget.style.background = "var(--background-secondary)"; },
        },
          isActive && React.createElement("span", { style: { position: "absolute", left: 0, right: 0, bottom: -1, height: 1, background: "var(--background-primary)" } }),
          isChat && React.createElement("span", { style: { color: "var(--text-muted)", display: "flex", flexShrink: 0 } }, Icons.MessageSquare({ size: 12 })),
          React.createElement("span", { style: { flex: 1, overflow: "hidden", textOverflow: "ellipsis" } }, label),
          React.createElement("span", {
            onClick: (e) => { e.stopPropagation(); onClose(id); },
            style: { width: 16, height: 16, display: "inline-flex", alignItems: "center", justifyContent: "center", borderRadius: "var(--radius-s)", color: "var(--text-faint)", opacity: isActive ? 1 : 0 },
          }, Icons.X({ size: 10 })),
        );
      }),
      React.createElement("div", { style: { flex: 1 } }),
      React.createElement("div", { style: { display: "flex", alignItems: "center", gap: 2, paddingRight: 8 } },
        /* Read/Edit mode + readable width — moved into tab bar */
        !active.startsWith("chat:") && React.createElement(React.Fragment, null,
          React.createElement(GhostBtn, { icon: Icons.Eye({ size: 15 }), label: "Read mode", onClick: () => window.__setEditorMode && window.__setEditorMode("read"), size: 28 }),
          React.createElement(GhostBtn, { icon: Icons.PenLine({ size: 15 }), label: "Edit mode", onClick: () => window.__setEditorMode && window.__setEditorMode("edit"), size: 28 }),
          React.createElement("span", { style: { width: 1, height: 16, background: "var(--background-modifier-border)", margin: "0 2px" } }),
          React.createElement(GhostBtn, { icon: Icons.AlignLeft({ size: 15 }), label: "Toggle reading width", onClick: () => window.__toggleReadableWidth && window.__toggleReadableWidth(), size: 28 }),
          React.createElement("span", { style: { width: 1, height: 16, background: "var(--background-modifier-border)", margin: "0 2px" } }),
        ),
        React.createElement(GhostBtn, { icon: Icons.ListTree({ size: 16 }), label: "Table of contents", onClick: onToggleTOC, active: tocOpen, size: 28 }),
        React.createElement(GhostBtn, { icon: Icons.PanelRight({ size: 16 }), label: "Toggle chat", onClick: onToggleChat, active: chatOpen, size: 28 }),
      ),
    ),
    /* Content — either doc or chat-as-tab */
    active.startsWith("chat:") ?
      React.createElement(ChatTabView, { chatId: active.replace("chat:", "") }) :
      React.createElement(DocumentView, { slug: active, tocOpen }),
  );
}

/* ── Document view with optional TOC sidebar ── */
function DocumentView({ slug, tocOpen }) {
  const [mode, setMode] = React.useState("edit"); // "read" | "edit"
  const [readableWidth, setReadableWidth] = React.useState(true);
  const maxW = readableWidth ? 720 : "none";

  const doc = getDoc(slug);
  const headings = doc.body.filter(b => b.type === "h2" || b.type === "h3");

  // Expose setters for tab-bar buttons
  React.useEffect(() => {
    window.__setEditorMode = setMode;
    window.__toggleReadableWidth = () => setReadableWidth(r => !r);
    return () => { delete window.__setEditorMode; delete window.__toggleReadableWidth; };
  }, []);

  return React.createElement("div", { style: { flex: 1, display: "flex", overflow: "hidden" } },
    /* TOC panel */
    tocOpen && React.createElement(TOCPanel, { headings, title: doc.h1 }),
    /* Main doc area */
    React.createElement("div", { style: { flex: 1, display: "flex", flexDirection: "column", overflow: "hidden" } },
      /* Scrollable doc */
      React.createElement("div", { style: { flex: 1, overflowY: "auto", padding: mode === "read" ? "36px 64px 80px" : "28px 64px 80px" } },
        React.createElement("div", { style: { maxWidth: maxW, margin: "0 auto" } },
          React.createElement("h1", {
            style: {
              fontFamily: "var(--font-serif)", fontSize: 34, fontWeight: 600,
              letterSpacing: "-0.01em", color: "var(--text-title-h1)",
              margin: "0 0 6px", lineHeight: 1.2,
            }
          }, doc.h1),
          mode === "read" && React.createElement("div", {
            style: { fontSize: "var(--font-ui-small)", color: "var(--text-faint)", marginBottom: 20 }
          }, "Read-only view"),
          ...doc.body.map(renderBlock),
          React.createElement(BacklinksPanel, { slug }),
        ),
      ),
    ),
  );
}

/* ── Table of Contents panel ── */
function TOCPanel({ headings, title }) {
  const [collapsed, setCollapsed] = React.useState({});
  const toggle = (i) => setCollapsed(c => ({ ...c, [i]: !c[i] }));

  return React.createElement("div", {
    style: {
      width: 220, minWidth: 220, background: "var(--background-primary-alt)",
      borderRight: "1px solid var(--background-modifier-border)",
      overflowY: "auto", padding: "12px 0", fontSize: "var(--font-ui-small)",
    }
  },
    React.createElement("div", {
      style: { padding: "0 12px 8px", fontSize: "var(--font-ui-smaller)", fontWeight: 600, color: "var(--text-muted)", textTransform: "uppercase", letterSpacing: "0.04em" }
    }, "Contents"),
    /* Doc title */
    React.createElement("div", {
      style: { padding: "4px 12px", fontWeight: 600, color: "var(--text-normal)", cursor: "pointer" },
      onMouseEnter: (e) => e.currentTarget.style.background = "var(--background-modifier-hover)",
      onMouseLeave: (e) => e.currentTarget.style.background = "transparent",
    }, title),
    /* Headings */
    headings.map((h, i) => {
      const isH3 = h.type === "h3";
      const isCollapsed = collapsed[i];
      return React.createElement("div", { key: i },
        React.createElement("div", {
          onClick: () => h.type === "h2" && toggle(i),
          style: {
            height: 28, display: "flex", alignItems: "center", gap: 4,
            paddingLeft: isH3 ? 28 : 12, paddingRight: 8,
            color: "var(--text-muted)", cursor: "pointer",
            transition: "background var(--motion-duration-fast) var(--motion-ease)",
          },
          onMouseEnter: (e) => e.currentTarget.style.background = "var(--background-modifier-hover)",
          onMouseLeave: (e) => e.currentTarget.style.background = "transparent",
        },
          h.type === "h2" && React.createElement("span", {
            style: { width: 12, display: "flex", alignItems: "center", justifyContent: "center", flexShrink: 0, transition: "transform var(--motion-duration-fast) var(--motion-ease)", transform: isCollapsed ? "rotate(-90deg)" : "rotate(0)" }
          }, Icons.ChevronDown({ size: 10 })),
          React.createElement("span", {
            style: { flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontWeight: isH3 ? 400 : 500 }
          }, h.text),
        ),
      );
    }),
  );
}

/* ── Backlinks Panel ── */
function BacklinksPanel({ slug }) {
  const [open, setOpen] = React.useState(false);
  const backlinks = {
    "france-military-comms": [
      { file: "procurement-paths.md", context: "…as detailed in [[france-military-comms|France Military Comms]]…" },
      { file: "ops-timeline.md", context: "…Syracuse IV referenced in [[france-military-comms]]…" },
      { file: "meeting-notes.md", context: "…review the [[france-military-comms]] analysis before Thursday…" },
    ],
    "procurement-paths": [
      { file: "france-military-comms.md", context: "…see [[procurement-paths]] for channel details…" },
    ],
  };
  const links = backlinks[slug] || [];

  return React.createElement("div", { style: { marginTop: 48, borderTop: "1px solid var(--hr-color)", paddingTop: 8 } },
    React.createElement("button", {
      onClick: () => setOpen(!open),
      style: {
        display: "flex", alignItems: "center", gap: 6, width: "100%",
        background: "transparent", border: 0, cursor: "pointer", padding: "6px 0",
        color: "var(--text-muted)", fontSize: "var(--font-ui-small)", fontWeight: 500, fontFamily: "var(--font-interface)",
      }
    },
      React.createElement("span", { style: { transition: "transform var(--motion-duration-fast) var(--motion-ease)", transform: open ? "rotate(0)" : "rotate(-90deg)", display: "flex" } },
        Icons.ChevronDown({ size: 12 }),
      ),
      Icons.Link2({ size: 14 }),
      "Backlinks",
      React.createElement("span", { style: { color: "var(--text-faint)", fontWeight: 400 } }, "(" + links.length + ")"),
    ),
    open && links.length > 0 && React.createElement("div", { style: { padding: "4px 0 8px" } },
      links.map((bl, i) => React.createElement("div", {
        key: i,
        style: {
          padding: "6px 8px", borderRadius: "var(--radius-s)", marginBottom: 2, cursor: "pointer",
          transition: "background var(--motion-duration-fast) var(--motion-ease)",
        },
        onMouseEnter: (e) => e.currentTarget.style.background = "var(--background-modifier-hover)",
        onMouseLeave: (e) => e.currentTarget.style.background = "transparent",
      },
        React.createElement("div", { style: { fontSize: "var(--font-ui-small)", fontWeight: 500, color: "var(--text-accent)" } }, bl.file),
        React.createElement("div", { style: { fontSize: "var(--font-ui-smaller)", color: "var(--text-faint)", marginTop: 2 } }, bl.context),
      )),
    ),
    open && links.length === 0 && React.createElement("div", { style: { padding: "8px 0", fontSize: "var(--font-ui-small)", color: "var(--text-faint)" } }, "No backlinks found"),
  );
}

/* ── Chat-as-tab view (full chat in the editor area) ── */
function ChatTabView({ chatId }) {
  const chats = getChatHistory();
  const chat = chats.find(c => c.id === chatId) || chats[0];
  const [input, setInput] = React.useState("");

  return React.createElement("div", { style: { flex: 1, display: "flex", flexDirection: "column", overflow: "hidden" } },
    /* Chat header bar */
    React.createElement("div", {
      style: {
        height: 32, minHeight: 32, padding: "0 16px", display: "flex", alignItems: "center", gap: 8,
        borderBottom: "1px solid var(--background-modifier-border)",
      }
    },
      React.createElement("span", { style: { fontSize: "var(--font-ui-medium)", fontWeight: 600, color: "var(--text-normal)" } }, chat.title),
      React.createElement(Chip, { children: chat.model, active: true }),
      React.createElement("div", { style: { flex: 1 } }),
      React.createElement("span", { style: { fontSize: "var(--font-ui-smaller)", color: "var(--text-faint)" } }, chat.date),
    ),
    /* Messages */
    React.createElement("div", { style: { flex: 1, overflowY: "auto", padding: "20px 64px" } },
      React.createElement("div", { style: { maxWidth: 720, margin: "0 auto" } },
        chat.messages.map((msg, i) =>
          React.createElement("div", { key: i, style: { marginBottom: 16, paddingBottom: 16, borderBottom: i < chat.messages.length - 1 ? "1px solid var(--hr-color)" : "none" } },
            React.createElement("div", { style: { fontSize: "var(--font-ui-smaller)", fontWeight: 500, textTransform: "uppercase", letterSpacing: "0.06em", color: "var(--text-faint)", marginBottom: 6 } }, msg.role === "user" ? "you" : "claude"),
            React.createElement("div", { style: { fontSize: 14, lineHeight: 1.65, color: "var(--text-normal)", fontFamily: "var(--font-text)", whiteSpace: "pre-wrap" } }, msg.text),
          )
        ),
      ),
    ),
    /* Composer */
    React.createElement("div", {
      style: {
        margin: "8px 64px 16px", maxWidth: 720,
        background: "var(--background-primary-alt)", border: "1px solid var(--background-modifier-border)",
        borderRadius: "var(--radius-m)", display: "flex", alignItems: "center", padding: "6px 10px", gap: 8,
      }
    },
      React.createElement("input", {
        value: input, onChange: (e) => setInput(e.target.value),
        placeholder: "Continue conversation...",
        style: { flex: 1, border: 0, background: "transparent", outline: "none", fontSize: "var(--font-ui-medium)", color: "var(--text-normal)", minWidth: 0 },
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

/* ── Shared data ── */
function getDoc(slug) {
  const docs = {
    "france-military-comms": {
      h1: "France Military Communications Analysis",
      body: [
        { type: "p", text: "The French military communications infrastructure underwent significant modernization between 2018 and 2023. The Syracuse IV satellite program, operational since late 2022, replaced the aging Syracuse III constellation with substantially improved throughput and anti-jamming capabilities." },
        { type: "h2", text: "Key Findings" },
        { type: "h3", text: "Communication Channels" },
        { type: "p", text: "Three primary channels of communication were identified across the Theatre-level command structure. The SOCRATE network handles strategic voice and data, while RITA NG provides tactical backbone connectivity. SICS serves as the joint command information system." },
        { type: "table", headers: ["System", "Role", "Status"], rows: [
          ["Syracuse IV", "Strategic satellite", "Operational"],
          ["SOCRATE 2", "Strategic backbone", "Modernizing"],
          ["RITA NG", "Tactical network", "Deployed"],
          ["SICS", "Joint C2", "Active"],
          ["Contact", "Soldier radio", "Fielding"],
        ]},
        { type: "h2", text: "Procurement Implications" },
        { type: "h3", text: "Contact Program" },
        { type: "p", text: "The Contact program represents the largest ongoing modernization effort, equipping infantry sections with software-defined radios from Thales. Budget allocation through the Loi de Programmation Militaire 2024-2030 commits €413 billion over the planning period, with communications infrastructure receiving approximately 8% of the equipment budget." },
        { type: "h3", text: "NATO Interoperability" },
        { type: "p", text: "Interoperability with NATO STANAG 4694 remains a stated objective, though implementation timelines have repeatedly slipped. The DGA (Direction Générale de l'Armement) continues to prioritize sovereign capability for strategic channels while accepting allied standards at the tactical level." },
      ]
    },
    "procurement-paths": {
      h1: "Procurement Paths",
      body: [
        { type: "p", text: "This document tracks the various procurement channels identified across European defense programs. Primary focus areas include communications equipment, ISR platforms, and cyber capabilities." },
        { type: "h2", text: "Direct Government Procurement" },
        { type: "p", text: "Most NATO member states maintain dedicated defense procurement agencies. France uses DGA, Germany uses BAAINBw, and the UK operates through DE&S. Each agency has distinct approval processes, threshold values, and competition requirements." },
      ]
    },
    "Untitled 5": { h1: "Untitled 5", body: [{ type: "p", text: "" }] },
  };
  return docs[slug] || docs["france-military-comms"];
}

function getChatHistory() {
  return [
    { id: "atomic-writes", title: "Atomic writes discussion", model: "claude-sonnet-4-6", date: "Apr 23, 2026",
      messages: [
        { role: "user", text: "What did I decide about atomic writes?" },
        { role: "assistant", text: "You specced it in [[mdeditor#3-save-io|§3 Save IO]]:\n\n1. Write content to sibling `.tmp` file\n2. `fsync` the `.tmp` file\n3. Rename `.tmp` → target (atomic on POSIX)\n4. `fsync` the parent directory\n\nThis guarantees no half-written files on crash." },
        { role: "user", text: "Can we also handle the case where the tmp file already exists from a previous crash?" },
        { role: "assistant", text: "Yes — on startup, scan for orphaned `.tmp` files in the vault root. For each:\n\n1. If the target file exists and is newer, delete the `.tmp`\n2. If the `.tmp` is newer or target is missing, rename `.tmp` → target\n3. Log the recovery to the console" },
      ]
    },
    { id: "design-tokens", title: "Design token review", model: "claude-sonnet-4-6", date: "Apr 22, 2026",
      messages: [
        { role: "user", text: "Review the color tokens in design.md §2" },
        { role: "assistant", text: "The tokens look solid. A few observations:\n\n1. The warm hue range (32-40) is consistent throughout\n2. Dark theme correctly brightens accents while desaturating backgrounds\n3. Missing: info-blue tokens for informational states" },
      ]
    },
    { id: "embedding-search", title: "Embedding search strategy", model: "claude-haiku-4", date: "Apr 21, 2026",
      messages: [
        { role: "user", text: "What embedding model should we use for hybrid search?" },
        { role: "assistant", text: "For a local-first app, I'd recommend nomic-embed-text-v1.5:\n\n- 137M params, runs fast on CPU\n- 768-dim vectors, good retrieval quality\n- Matryoshka support — can truncate to 256-dim for faster search with minimal quality loss" },
      ]
    },
    { id: "voice-input", title: "Voice input architecture", model: "claude-sonnet-4-6", date: "Apr 20, 2026",
      messages: [
        { role: "user", text: "How should voice input work?" },
        { role: "assistant", text: "Two modes:\n\n1. Push-to-talk: hold a key, speak, release → transcribed text inserted at cursor\n2. Continuous: toggle on, VAD detects speech segments, each segment transcribed and inserted\n\nBoth use local Whisper by default." },
      ]
    },
  ];
}

function renderBlock(block, i) {
  if (block.type === "h2") return React.createElement("h2", { key: i, style: { fontFamily: "var(--font-serif)", fontSize: 24, fontWeight: 600, color: "var(--text-title-h2)", margin: "32px 0 8px", lineHeight: 1.3 } }, block.text);
  if (block.type === "h3") return React.createElement("h3", { key: i, style: { fontFamily: "var(--font-serif)", fontSize: 18, fontWeight: 600, color: "var(--text-title-h3)", margin: "22px 0 6px", lineHeight: 1.3 } }, block.text);
  if (block.type === "p") return React.createElement("p", { key: i, style: { fontFamily: "var(--font-text)", fontSize: 16, lineHeight: 1.7, color: "var(--text-normal)", margin: "0 0 14px", textWrap: "pretty" } }, block.text);
  if (block.type === "table") return React.createElement("table", { key: i, style: { borderCollapse: "collapse", width: "100%", margin: "10px 0 16px", fontFamily: "var(--font-interface)", fontSize: 13 } },
    React.createElement("thead", null, React.createElement("tr", null, block.headers.map((h, j) =>
      React.createElement("th", { key: j, style: { textAlign: "left", fontWeight: 500, color: "var(--text-muted)", padding: "6px 10px", borderBottom: "1px solid var(--background-modifier-border)", background: "var(--background-secondary)" } }, h)
    ))),
    React.createElement("tbody", null, block.rows.map((row, j) =>
      React.createElement("tr", { key: j }, row.map((cell, k) =>
        React.createElement("td", { key: k, style: { padding: "6px 10px", borderBottom: "1px solid var(--hr-color)", color: k === 0 ? "var(--text-muted)" : "var(--text-normal)" } }, cell)
      ))
    ))
  );
  return null;
}

Object.assign(window, { EditorPane, DocumentView, ChatTabView, TOCPanel, BacklinksPanel, getDoc, getChatHistory, renderBlock });
