/* global React, Icons, GhostBtn, SecondaryBtn, PrimaryBtn, SegCtrl, Toggle, InputField, Chip, StatusDot, Divider, Kbd */

/* ── General Settings Modal (§8.5) ── */
function SettingsModal({ onClose }) {
  const [tab, setTab] = React.useState("appearance");
  const [theme, setTheme] = React.useState("dark");
  const [fontSize, setFontSize] = React.useState(15);
  const [readableWidth, setReadableWidth] = React.useState(false);
  const [zoom, setZoom] = React.useState(100);
  const [autoOpen, setAutoOpen] = React.useState(true);
  const [hideDots, setHideDots] = React.useState(true);
  const [showChat, setShowChat] = React.useState(false);
  const [saveDeb, setSaveDeb] = React.useState(300);
  const [dirtyInd, setDirtyInd] = React.useState(true);
  const [atomicWrite, setAtomicWrite] = React.useState(true);
  const [wikiNewTab, setWikiNewTab] = React.useState(false);

  const tabs = [
    { id: "appearance", label: "Appearance" },
    { id: "vault", label: "Vault" },
    { id: "editor", label: "Editor" },
    { id: "shortcuts", label: "Shortcuts" },
    { id: "about", label: "About" },
  ];

  const SettingRow = ({ label, description, children }) =>
    React.createElement("div", { style: { display: "flex", justifyContent: "space-between", alignItems: "center", minHeight: 40, padding: "8px 0" } },
      React.createElement("div", { style: { flex: 1 } },
        React.createElement("div", { style: { fontSize: "var(--font-ui-medium)", fontWeight: 500, color: "var(--text-normal)" } }, label),
        description && React.createElement("div", { style: { fontSize: "var(--font-ui-small)", color: "var(--text-muted)", marginTop: 2 } }, description),
      ),
      React.createElement("div", { style: { marginLeft: 24, flexShrink: 0 } }, children),
    );

  const SliderRow = ({ label, value, min, max, step, unit, onChange }) =>
    React.createElement(SettingRow, { label },
      React.createElement("div", { style: { display: "flex", alignItems: "center", gap: 8 } },
        React.createElement("input", {
          type: "range", min, max, step, value,
          onChange: (e) => onChange(Number(e.target.value)),
          style: { width: 120, accentColor: "var(--interactive-accent)" },
        }),
        React.createElement("span", { style: { fontSize: "var(--font-ui-small)", color: "var(--text-muted)", minWidth: 40, textAlign: "right" } }, value + (unit || "")),
      ),
    );

  const renderAppearance = () => React.createElement(React.Fragment, null,
    React.createElement(SettingRow, { label: "Theme" },
      React.createElement(SegCtrl, {
        options: [{ value: "light", label: "Light" }, { value: "dark", label: "Dark" }, { value: "system", label: "System" }],
        value: theme, onChange: setTheme,
      }),
    ),
    React.createElement(Divider, null),
    React.createElement(SettingRow, { label: "Interface font" },
      React.createElement("span", { style: { fontSize: "var(--font-ui-medium)", color: "var(--text-muted)" } }, "Manrope"),
    ),
    React.createElement(Divider, null),
    React.createElement(SettingRow, { label: "Editor font" },
      React.createElement("span", { style: { fontSize: "var(--font-ui-medium)", color: "var(--text-muted)" } }, "Manrope"),
    ),
    React.createElement(Divider, null),
    React.createElement(SliderRow, { label: "Base font size", value: fontSize, min: 12, max: 24, step: 1, unit: "px", onChange: setFontSize }),
    React.createElement(Divider, null),
    React.createElement(SettingRow, { label: "Readable line width", description: "Limit editor content width to 820px" },
      React.createElement(Toggle, { on: readableWidth, onChange: setReadableWidth }),
    ),
    React.createElement(Divider, null),
    React.createElement(SliderRow, { label: "Zoom level", value: zoom, min: 75, max: 200, step: 5, unit: "%", onChange: setZoom }),
  );

  const renderVault = () => React.createElement(React.Fragment, null,
    React.createElement(SettingRow, { label: "Vault path" },
      React.createElement("div", { style: { display: "flex", alignItems: "center", gap: 8 } },
        React.createElement("span", { style: { fontSize: "var(--font-ui-small)", color: "var(--text-muted)", fontFamily: "var(--font-monospace)" } }, "~/Documents/my-vault"),
        React.createElement(SecondaryBtn, { children: "Change" }),
      ),
    ),
    React.createElement(Divider, null),
    React.createElement(SettingRow, { label: "Auto-open on launch" },
      React.createElement(Toggle, { on: autoOpen, onChange: setAutoOpen }),
    ),
    React.createElement(Divider, null),
    React.createElement(SettingRow, { label: "Hide dotfiles" },
      React.createElement(Toggle, { on: hideDots, onChange: setHideDots }),
    ),
    React.createElement(Divider, null),
    React.createElement(SettingRow, { label: "Show chat files in sidebar" },
      React.createElement(Toggle, { on: showChat, onChange: setShowChat }),
    ),
    React.createElement(Divider, null),
    React.createElement(SettingRow, { label: "Excluded folders", description: "Comma-separated list" },
      React.createElement(InputField, { value: ".git, node_modules", style: { width: 200 } }),
    ),
  );

  const renderEditor = () => React.createElement(React.Fragment, null,
    React.createElement(SliderRow, { label: "Save debounce", value: saveDeb, min: 100, max: 2000, step: 100, unit: "ms", onChange: setSaveDeb }),
    React.createElement(Divider, null),
    React.createElement(SettingRow, { label: "Show dirty indicator" },
      React.createElement(Toggle, { on: dirtyInd, onChange: setDirtyInd }),
    ),
    React.createElement(Divider, null),
    React.createElement(SettingRow, { label: "Atomic writes", description: "Write to .tmp then rename (prevents data loss)" },
      React.createElement(Toggle, { on: atomicWrite, onChange: setAtomicWrite }),
    ),
    React.createElement(Divider, null),
    React.createElement(SettingRow, { label: "Wikilinks open in new tab" },
      React.createElement(Toggle, { on: wikiNewTab, onChange: setWikiNewTab }),
    ),
    React.createElement(Divider, null),
    React.createElement(SettingRow, { label: "Default pose for existing files" },
      React.createElement(SegCtrl, {
        options: [{ value: "read", label: "Read" }, { value: "edit", label: "Edit" }],
        value: "edit", onChange: () => {},
      }),
    ),
  );

  const shortcuts = [
    { cmd: "Open command palette", keys: "⌘ P" },
    { cmd: "Toggle sidebar", keys: "⌘ B" },
    { cmd: "New file", keys: "⌘ N" },
    { cmd: "Save", keys: "⌘ S" },
    { cmd: "Toggle chat", keys: "⌘ ⇧ L" },
    { cmd: "Open graph", keys: "⌘ G" },
    { cmd: "Toggle terminal", keys: "⌘ `" },
    { cmd: "Open settings", keys: "⌘ ," },
    { cmd: "Find in file", keys: "⌘ F" },
    { cmd: "Search vault", keys: "⌘ ⇧ F" },
  ];

  const renderShortcuts = () => React.createElement("div", null,
    React.createElement("div", { style: { display: "flex", height: 32, background: "var(--background-secondary)", borderRadius: "var(--radius-s)", marginBottom: 4, padding: "0 10px", alignItems: "center" } },
      React.createElement("span", { style: { flex: 1, fontSize: "var(--font-ui-small)", fontWeight: 600, color: "var(--text-muted)" } }, "Command"),
      React.createElement("span", { style: { width: 120, fontSize: "var(--font-ui-small)", fontWeight: 600, color: "var(--text-muted)", textAlign: "right" } }, "Binding"),
    ),
    shortcuts.map((s, i) =>
      React.createElement("div", { key: i, style: { display: "flex", height: 32, padding: "0 10px", alignItems: "center", borderBottom: "1px solid var(--hr-color)", cursor: "pointer" },
        onMouseEnter: (e) => e.currentTarget.style.background = "var(--background-modifier-hover)",
        onMouseLeave: (e) => e.currentTarget.style.background = "transparent",
      },
        React.createElement("span", { style: { flex: 1, fontSize: "var(--font-ui-medium)", color: "var(--text-normal)" } }, s.cmd),
        React.createElement(Kbd, null, s.keys),
      )
    )
  );

  const renderAbout = () => React.createElement("div", { style: { fontSize: "var(--font-ui-medium)", color: "var(--text-muted)", lineHeight: 1.7 } },
    React.createElement("div", { style: { fontWeight: 600, color: "var(--text-normal)", fontSize: "var(--font-ui-larger)", marginBottom: 12 } }, "Forge"),
    React.createElement("div", null, "Version 0.4.0-alpha"),
    React.createElement("div", null, "Build: ", React.createElement("span", { style: { fontFamily: "var(--font-monospace)", fontSize: "var(--font-ui-small)" } }, "a3b8f2d")),
    React.createElement("div", { style: { marginTop: 16 } },
      React.createElement("a", { href: "#", style: { color: "var(--text-link)" } }, "License"),
      React.createElement("span", { style: { margin: "0 8px", color: "var(--text-faint)" } }, "·"),
      React.createElement("a", { href: "#", style: { color: "var(--text-link)" } }, "Open logs"),
    ),
    React.createElement("div", { style: { marginTop: 24 } },
      React.createElement(SecondaryBtn, { children: "Reset settings", style: { color: "var(--text-error)" } }),
    ),
  );

  const content = { appearance: renderAppearance, vault: renderVault, editor: renderEditor, shortcuts: renderShortcuts, about: renderAbout };

  return React.createElement("div", {
    onClick: onClose,
    style: { position: "fixed", inset: 0, background: "var(--modal-backdrop)", display: "flex", alignItems: "center", justifyContent: "center", zIndex: "var(--z-modal-backdrop)" }
  },
    React.createElement("div", {
      onClick: (e) => e.stopPropagation(),
      style: {
        background: "var(--background-primary)", border: "1px solid var(--background-modifier-border)",
        borderRadius: "var(--radius-l)", boxShadow: "var(--shadow-l)", width: 720,
        maxHeight: "80vh", display: "flex", flexDirection: "column", zIndex: "var(--z-modal)",
      }
    },
      /* Header */
      React.createElement("div", { style: { padding: "20px 24px 0", display: "flex", justifyContent: "space-between", alignItems: "center" } },
        React.createElement("span", { style: { fontSize: "var(--font-ui-larger)", fontWeight: 600, color: "var(--text-normal)" } }, "Settings"),
        React.createElement(GhostBtn, { icon: Icons.X({ size: 16 }), label: "Close", onClick: onClose }),
      ),
      /* Tabs */
      React.createElement("div", { style: { display: "flex", gap: 0, padding: "0 24px", borderBottom: "1px solid var(--background-modifier-border)", marginTop: 12 } },
        tabs.map(t => React.createElement("button", {
          key: t.id, onClick: () => setTab(t.id),
          style: {
            height: 36, padding: "0 14px", background: "transparent", border: 0,
            borderBottom: tab === t.id ? "2px solid var(--text-accent)" : "2px solid transparent",
            color: tab === t.id ? "var(--text-normal)" : "var(--text-muted)",
            fontSize: "var(--font-ui-medium)", fontWeight: 500, cursor: "pointer",
            transition: "color var(--motion-duration-fast) var(--motion-ease)",
          },
          onMouseEnter: (e) => { if(tab !== t.id) e.currentTarget.style.background = "var(--background-modifier-hover)"; },
          onMouseLeave: (e) => e.currentTarget.style.background = "transparent",
        }, t.label)),
      ),
      /* Body */
      React.createElement("div", { style: { flex: 1, overflowY: "auto", padding: "16px 24px 20px" } },
        content[tab](),
      ),
      /* Footer */
      React.createElement("div", { style: { padding: "12px 24px", borderTop: "1px solid var(--background-modifier-border)", display: "flex", justifyContent: "flex-end", gap: 8 } },
        React.createElement(SecondaryBtn, { onClick: onClose, children: "Cancel" }),
        React.createElement(PrimaryBtn, { children: "Save" }),
      ),
    ),
  );
}

/* ── AI Settings Modal (§8.6) ── */
function AISettingsModal({ onClose }) {
  const [tab, setTab] = React.useState("providers");
  const tabs = [
    { id: "providers", label: "Providers" },
    { id: "routing", label: "Routing" },
    { id: "context", label: "Context" },
    { id: "tools", label: "Tools" },
    { id: "prompts", label: "Prompts" },
    { id: "voice", label: "Voice" },
    { id: "terminal", label: "Terminal" },
    { id: "chatfiles", label: "Chat files" },
  ];

  const ProviderCard = ({ name, status, statusLabel, children }) =>
    React.createElement("div", {
      style: {
        background: "var(--background-primary-alt)", border: "1px solid var(--background-modifier-border)",
        borderRadius: "var(--radius-m)", padding: 16, marginBottom: 12,
      }
    },
      React.createElement("div", { style: { display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 12 } },
        React.createElement("span", { style: { fontSize: "var(--font-ui-medium)", fontWeight: 600, color: "var(--text-normal)" } }, name),
        React.createElement("div", { style: { display: "flex", alignItems: "center", gap: 6, fontSize: "var(--font-ui-small)", color: "var(--text-muted)" } },
          React.createElement(StatusDot, { variant: status }),
          statusLabel,
        ),
      ),
      children,
    );

  const FieldRow = ({ label, children }) =>
    React.createElement("div", { style: { display: "flex", alignItems: "center", gap: 8, marginBottom: 8 } },
      React.createElement("span", { style: { width: 100, fontSize: "var(--font-ui-small)", fontWeight: 500, color: "var(--text-muted)", flexShrink: 0 } }, label),
      children,
    );

  const renderProviders = () => React.createElement(React.Fragment, null,
    React.createElement(ProviderCard, { name: "Anthropic", status: "connected", statusLabel: "connected" },
      React.createElement(FieldRow, { label: "API key" },
        React.createElement(InputField, { value: "sk-ant-•••••••••••••••••••", type: "password", style: { flex: 1 } }),
        React.createElement(SecondaryBtn, { children: "Test" }),
        React.createElement(PrimaryBtn, { children: "Save" }),
      ),
      React.createElement(FieldRow, { label: "Default model" },
        React.createElement("select", { style: { height: 32, padding: "0 10px", borderRadius: "var(--radius-s)", background: "var(--background-modifier-form-field)", border: "1px solid var(--background-modifier-border)", color: "var(--text-normal)", fontSize: "var(--font-ui-medium)", flex: 1 } },
          React.createElement("option", null, "claude-sonnet-4-6"),
          React.createElement("option", null, "claude-opus-4"),
          React.createElement("option", null, "claude-haiku-4"),
        ),
      ),
      React.createElement(FieldRow, { label: "Caching" },
        React.createElement(Toggle, { on: true, onChange: () => {} }),
        React.createElement("span", { style: { fontSize: "var(--font-ui-small)", color: "var(--text-muted)" } }, "Ephemeral on first turn"),
      ),
    ),
    React.createElement(ProviderCard, { name: "OpenAI", status: "idle", statusLabel: "not configured" },
      React.createElement(FieldRow, { label: "API key" }, React.createElement(InputField, { placeholder: "sk-...", style: { flex: 1 } }), React.createElement(SecondaryBtn, { children: "Test" })),
      React.createElement(FieldRow, { label: "Base URL" }, React.createElement(InputField, { value: "https://api.openai.com/v1", style: { flex: 1 } })),
    ),
    React.createElement(ProviderCard, { name: "Gemini", status: "idle", statusLabel: "not configured" },
      React.createElement(FieldRow, { label: "API key" }, React.createElement(InputField, { placeholder: "AI...", style: { flex: 1 } }), React.createElement(SecondaryBtn, { children: "Test" })),
    ),
    React.createElement("div", { style: { display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 } },
      React.createElement(ProviderCard, { name: "OpenRouter", status: "idle", statusLabel: "not configured" },
        React.createElement(FieldRow, { label: "API key" }, React.createElement(InputField, { placeholder: "sk-or-...", style: { flex: 1 } })),
      ),
      React.createElement(ProviderCard, { name: "Copilot", status: "connected", statusLabel: "logged in" },
        React.createElement("div", { style: { fontSize: "var(--font-ui-small)", color: "var(--text-muted)" } }, "Signed in as ", React.createElement("span", { style: { color: "var(--text-normal)", fontWeight: 500 } }, "sinifad65")),
      ),
    ),
    React.createElement(ProviderCard, { name: "OpenAI-compatible (Ollama, LM Studio)", status: "idle", statusLabel: "not configured" },
      React.createElement(FieldRow, { label: "Base URL" }, React.createElement(InputField, { value: "http://localhost:11434/v1", style: { flex: 1 } })),
      React.createElement("div", { style: { fontSize: "var(--font-ui-small)", color: "var(--text-faint)", marginTop: 4 } }, "Model list auto-populated from /api/tags or /v1/models"),
    ),
    React.createElement(ProviderCard, { name: "Local GGUF (in-process)", status: "idle", statusLabel: "no model loaded" },
      React.createElement(FieldRow, { label: "Model" },
        React.createElement("select", { style: { height: 32, padding: "0 10px", borderRadius: "var(--radius-s)", background: "var(--background-modifier-form-field)", border: "1px solid var(--background-modifier-border)", color: "var(--text-normal)", fontSize: "var(--font-ui-medium)", flex: 1 } },
          React.createElement("option", null, "Qwen 2.5 7B Q4"),
        ),
        React.createElement(SecondaryBtn, { children: "Manage catalogue…" }),
      ),
      React.createElement(FieldRow, { label: "GPU layers" },
        React.createElement(InputField, { value: "99", type: "number", style: { width: 80 } }),
      ),
      React.createElement(FieldRow, { label: "Context" },
        React.createElement(InputField, { value: "8192", type: "number", style: { width: 80 } }),
      ),
    ),
  );

  const RoutingCard = ({ slot, provider, model }) =>
    React.createElement("div", { style: { background: "var(--background-primary-alt)", border: "1px solid var(--background-modifier-border)", borderRadius: "var(--radius-m)", padding: 16 } },
      React.createElement("div", { style: { fontSize: "var(--font-ui-medium)", fontWeight: 600, color: "var(--text-normal)", marginBottom: 10 } }, slot),
      React.createElement("div", { style: { display: "flex", gap: 8, marginBottom: 6 } },
        React.createElement("select", { defaultValue: provider, style: { height: 32, flex: 1, padding: "0 10px", borderRadius: "var(--radius-s)", background: "var(--background-modifier-form-field)", border: "1px solid var(--background-modifier-border)", color: "var(--text-normal)", fontSize: "var(--font-ui-medium)" } },
          React.createElement("option", null, "Anthropic"), React.createElement("option", null, "OpenAI"), React.createElement("option", null, "Copilot"), React.createElement("option", null, "Local GGUF"),
        ),
        React.createElement("select", { defaultValue: model, style: { height: 32, flex: 1, padding: "0 10px", borderRadius: "var(--radius-s)", background: "var(--background-modifier-form-field)", border: "1px solid var(--background-modifier-border)", color: "var(--text-normal)", fontSize: "var(--font-ui-medium)" } },
          React.createElement("option", null, model),
        ),
      ),
      React.createElement(SecondaryBtn, { children: "Test", style: { marginTop: 4 } }),
    );

  const renderRouting = () => React.createElement("div", { style: { display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 } },
    React.createElement(RoutingCard, { slot: "Chat", provider: "Anthropic", model: "claude-sonnet-4-6" }),
    React.createElement(RoutingCard, { slot: "Fast", provider: "Anthropic", model: "claude-haiku-4" }),
    React.createElement(RoutingCard, { slot: "Summarise", provider: "Anthropic", model: "claude-haiku-4" }),
    React.createElement(RoutingCard, { slot: "Embed", provider: "Local GGUF", model: "nomic-embed-text-v1.5" }),
  );

  const renderContext = () => React.createElement(React.Fragment, null,
    React.createElement("div", { style: { display: "flex", justifyContent: "space-between", alignItems: "center", minHeight: 40, padding: "8px 0" } },
      React.createElement("div", null,
        React.createElement("div", { style: { fontSize: "var(--font-ui-medium)", fontWeight: 500 } }, "Compaction threshold"),
        React.createElement("div", { style: { fontSize: "var(--font-ui-small)", color: "var(--text-muted)", marginTop: 2 } }, "Percentage of context window before compacting"),
      ),
      React.createElement("div", { style: { display: "flex", alignItems: "center", gap: 8 } },
        React.createElement("input", { type: "range", min: 50, max: 95, defaultValue: 80, style: { width: 120, accentColor: "var(--interactive-accent)" } }),
        React.createElement("span", { style: { fontSize: "var(--font-ui-small)", color: "var(--text-muted)" } }, "80%"),
      ),
    ),
    React.createElement(Divider, null),
    React.createElement("div", { style: { display: "flex", justifyContent: "space-between", alignItems: "center", minHeight: 40, padding: "8px 0" } },
      React.createElement("div", null,
        React.createElement("div", { style: { fontSize: "var(--font-ui-medium)", fontWeight: 500 } }, "Summary block size"),
        React.createElement("div", { style: { fontSize: "var(--font-ui-small)", color: "var(--text-muted)", marginTop: 2 } }, "Number of turns per summary block"),
      ),
      React.createElement(InputField, { value: "8", type: "number", style: { width: 80 } }),
    ),
  );

  const tools = [
    { name: "hybrid_search", desc: "Search vault by content + embeddings", on: true },
    { name: "read_note", desc: "Read a note's full content", on: true },
    { name: "edit_note", desc: "Edit or append to a note", on: true },
    { name: "create_note", desc: "Create a new note", on: true },
    { name: "list_files", desc: "List files in a directory", on: true },
    { name: "shell_exec", desc: "Execute a shell command", on: false },
    { name: "web_search", desc: "Search the web", on: false },
    { name: "web_fetch", desc: "Fetch a URL", on: false },
  ];
  const renderTools = () => React.createElement(React.Fragment, null,
    React.createElement("div", { style: { display: "flex", gap: 8, marginBottom: 12 } },
      React.createElement(SecondaryBtn, { children: "Enable safe-only" }),
      React.createElement(SecondaryBtn, { children: "Enable all" }),
      React.createElement(SecondaryBtn, { children: "Disable all" }),
    ),
    React.createElement("div", { style: { display: "grid", gridTemplateColumns: "1fr 1fr", gap: 1 } },
      tools.map((t, i) => React.createElement("div", { key: i, style: { display: "flex", alignItems: "center", gap: 10, padding: "8px 10px", borderBottom: "1px solid var(--hr-color)" } },
        React.createElement(Toggle, { on: t.on, onChange: () => {} }),
        React.createElement("div", { style: { flex: 1, minWidth: 0 } },
          React.createElement("div", { style: { fontSize: "var(--font-ui-medium)", fontWeight: 500, fontFamily: "var(--font-monospace)", color: "var(--text-normal)" } }, t.name),
          React.createElement("div", { style: { fontSize: "var(--font-ui-small)", color: "var(--text-muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" } }, t.desc),
        ),
      )),
    ),
  );

  const renderPlaceholder = (title) => React.createElement("div", { style: { padding: 20, textAlign: "center", color: "var(--text-faint)" } }, title + " settings");

  const content = {
    providers: renderProviders, routing: renderRouting, context: renderContext, tools: renderTools,
    prompts: () => renderPlaceholder("Prompts"), voice: () => renderPlaceholder("Voice"),
    terminal: () => renderPlaceholder("Terminal"), chatfiles: () => renderPlaceholder("Chat files"),
  };

  return React.createElement("div", {
    onClick: onClose,
    style: { position: "fixed", inset: 0, background: "var(--modal-backdrop)", display: "flex", alignItems: "center", justifyContent: "center", zIndex: "var(--z-modal-backdrop)" }
  },
    React.createElement("div", {
      onClick: (e) => e.stopPropagation(),
      style: {
        background: "var(--background-primary)", border: "1px solid var(--background-modifier-border)",
        borderRadius: "var(--radius-l)", boxShadow: "var(--shadow-l)", width: 800,
        height: "85vh", display: "flex", flexDirection: "column", zIndex: "var(--z-modal)",
      }
    },
      React.createElement("div", { style: { padding: "20px 24px 0", display: "flex", justifyContent: "space-between", alignItems: "center" } },
        React.createElement("span", { style: { fontSize: "var(--font-ui-larger)", fontWeight: 600, color: "var(--text-normal)" } }, "AI Settings"),
        React.createElement(GhostBtn, { icon: Icons.X({ size: 16 }), label: "Close", onClick: onClose }),
      ),
      React.createElement("div", { style: { display: "flex", gap: 0, padding: "0 24px", borderBottom: "1px solid var(--background-modifier-border)", marginTop: 12, overflowX: "auto" } },
        tabs.map(t => React.createElement("button", {
          key: t.id, onClick: () => setTab(t.id),
          style: {
            height: 36, padding: "0 12px", background: "transparent", border: 0,
            borderBottom: tab === t.id ? "2px solid var(--text-accent)" : "2px solid transparent",
            color: tab === t.id ? "var(--text-normal)" : "var(--text-muted)",
            fontSize: "var(--font-ui-medium)", fontWeight: 500, cursor: "pointer", whiteSpace: "nowrap",
          },
          onMouseEnter: (e) => { if(tab !== t.id) e.currentTarget.style.background = "var(--background-modifier-hover)"; },
          onMouseLeave: (e) => e.currentTarget.style.background = "transparent",
        }, t.label)),
      ),
      React.createElement("div", { style: { flex: 1, overflowY: "auto", padding: "16px 24px 20px" } },
        content[tab](),
      ),
    ),
  );
}

Object.assign(window, { SettingsModal, AISettingsModal });
