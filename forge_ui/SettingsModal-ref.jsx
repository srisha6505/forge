/* global React, Icons */

function Radio({ active, title, subtitle, onClick }) {
  return React.createElement("div", {
    className: `forge-radio ${active ? "active" : ""}`,
    onClick,
    style: {
      display: "flex", alignItems: "flex-start", gap: 10,
      padding: "10px 12px",
      border: `1px solid ${active ? "var(--accent)" : "var(--border-2)"}`,
      background: active ? "var(--accent-subtle)" : "var(--bg-2)",
      borderRadius: 6,
      cursor: "pointer",
      fontFamily: "var(--font-sans)", fontSize: 13,
      color: "var(--fg-2)",
      marginBottom: 8,
    },
  },
    React.createElement("div", {
      style: {
        width: 12, height: 12, borderRadius: 999, marginTop: 4, flexShrink: 0,
        border: `1.5px solid ${active ? "var(--accent)" : "var(--border-strong)"}`,
        position: "relative",
        background: active ? "var(--bg-2)" : "transparent",
      }
    }, active && React.createElement("div", { style: { position: "absolute", inset: 2, borderRadius: 999, background: "var(--accent)" } })),
    React.createElement("div", { style: { flex: 1 } },
      React.createElement("div", { style: { fontWeight: 500, color: "var(--fg-1)", marginBottom: 2 } }, title),
      subtitle && React.createElement("div", { style: { color: "var(--fg-4)", fontSize: 12 } }, subtitle)
    )
  );
}

function DlButton({ label = "Download" }) {
  return React.createElement("button", {
    style: {
      background: "var(--accent)", color: "#fff", fontFamily: "var(--font-sans)", fontSize: 12,
      fontWeight: 500, padding: "5px 14px", borderRadius: 4, border: 0, cursor: "pointer",
    },
  }, label);
}

function ModelRow({ name, meta, action = "Download" }) {
  return React.createElement("div", {
    style: {
      display: "flex", alignItems: "center", justifyContent: "space-between",
      padding: "10px 12px", border: "1px solid var(--border-2)", borderRadius: 6,
      background: "var(--bg-2)", marginBottom: 8, gap: 12,
    }
  },
    React.createElement("div", null,
      React.createElement("div", { style: { fontFamily: "var(--font-sans)", fontSize: 13, fontWeight: 500, color: "var(--fg-1)" } }, name),
      React.createElement("div", { style: { fontFamily: "var(--font-sans)", fontSize: 12, color: "var(--fg-4)", marginTop: 2 } }, meta)
    ),
    action === "Installed"
      ? React.createElement("div", { style: { display: "flex", alignItems: "center", gap: 10 } },
          React.createElement("span", { style: { fontFamily: "var(--font-sans)", fontSize: 12, color: "var(--fg-4)", cursor: "pointer" } }, "Delete"),
          React.createElement("span", { style: { fontFamily: "var(--font-sans)", fontSize: 12, color: "var(--olive-500, #6E7A36)", fontWeight: 500 } }, "Installed")
        )
      : React.createElement(DlButton, { label: action })
  );
}

function SettingsModal({ onClose }) {
  const [llm, setLlm] = React.useState("copilot");
  const [stt, setStt] = React.useState("local");

  return React.createElement("div", { className: "forge-modal-backdrop", onClick: onClose },
    React.createElement("div", { className: "forge-modal", onClick: (e) => e.stopPropagation() },
      React.createElement("div", { className: "forge-modal-head" },
        React.createElement("div", { className: "forge-modal-title" }, "Settings"),
        React.createElement("div", { style: { display: "flex", alignItems: "center", gap: 12 } },
          React.createElement("span", { className: "forge-modal-sub" }, "GPU: CUDA runtime detected  MEDIA: VAFace FFT XINE XBC   18/26"),
          React.createElement("span", { style: { color: "var(--fg-4)", cursor: "pointer", fontSize: 14 }, onClick: onClose }, "×")
        )
      ),
      React.createElement("div", { className: "forge-modal-body" },

        React.createElement("div", { style: { fontFamily: "var(--font-sans)", fontSize: 12, fontWeight: 600, textTransform: "uppercase", letterSpacing: "0.06em", color: "var(--fg-3)", marginBottom: 10 } }, "Language Model (LLM)"),
        React.createElement("div", { style: { fontFamily: "var(--font-sans)", fontSize: 11, fontWeight: 500, textTransform: "uppercase", letterSpacing: "0.06em", color: "var(--fg-4)", marginBottom: 8 } }, "Set Provider"),

        React.createElement(Radio, { active: llm === "local", title: "Local (GGUF)", subtitle: "Download a GGUF-format model, runs on your machine.", onClick: () => setLlm("local") }),
        React.createElement(Radio, { active: llm === "copilot", title: "GitHub Copilot", subtitle: "Sign in with GitHub. Needs Copilot subscription.", onClick: () => setLlm("copilot") }),
        React.createElement(Radio, { active: llm === "anthropic", title: "Anthropic API", subtitle: "Bring your own API key.", onClick: () => setLlm("anthropic") }),

        React.createElement("div", { style: { fontFamily: "var(--font-sans)", fontSize: 12, color: "var(--fg-3)", margin: "14px 0 4px" } }, "Signed in as ", React.createElement("span", { style: { color: "var(--fg-1)", fontWeight: 500 } }, "sinifad65"), React.createElement("span", { style: { float: "right", color: "var(--fg-4)", cursor: "pointer" } }, "Sign out")),
        React.createElement("input", {
          value: "gpt-4o", readOnly: true,
          style: { width: "100%", padding: "6px 10px", border: "1px solid var(--border-2)", borderRadius: 4, background: "var(--bg-2)", fontFamily: "var(--font-mono)", fontSize: 13, color: "var(--fg-1)", marginTop: 4 },
        }),

        React.createElement("div", { style: { fontFamily: "var(--font-sans)", fontSize: 11, fontWeight: 500, textTransform: "uppercase", letterSpacing: "0.06em", color: "var(--fg-4)", margin: "16px 0 8px" } }, "Download models"),
        React.createElement(ModelRow, { name: "Gemma 3 4B Instruct (Q4_K_M)", meta: "Google Gemma 3, 4B params, quantized to Q4_K_M. 2.2 GB. Good balance of quality and CPU speed." }),
        React.createElement(ModelRow, { name: "Gemma 3 1B Instruct (Q4_K_M)", meta: "Smallest Gemma 3. 1B params, quantized. 900 MB. Fastest, limited reasoning." }),
        React.createElement(ModelRow, { name: "Qwen 2.5 3B Instruct (Q4_K_M)", meta: "Alibaba Qwen 2.5. 3B params, quantized. 2 GB. Strong multilingual, low-resource." }),

        React.createElement("div", { style: { fontFamily: "var(--font-sans)", fontSize: 12, fontWeight: 600, textTransform: "uppercase", letterSpacing: "0.06em", color: "var(--fg-3)", margin: "18px 0 10px" } }, "Speech-to-text (STT)"),
        React.createElement("div", { style: { fontFamily: "var(--font-sans)", fontSize: 11, fontWeight: 500, textTransform: "uppercase", letterSpacing: "0.06em", color: "var(--fg-4)", marginBottom: 8 } }, "Set Provider"),
        React.createElement(Radio, { active: stt === "local", title: "Local (Whisper + Pyper)", subtitle: "Runs offline. Download Whisper + pyper voices below.", onClick: () => setStt("local") }),
        React.createElement(Radio, { active: stt === "deepgram", title: "Deepgram (cloud)", subtitle: "Fastest. Needs internet + API key.", onClick: () => setStt("deepgram") }),

        React.createElement("div", { style: { fontFamily: "var(--font-sans)", fontSize: 12, color: "var(--fg-3)", marginTop: 12, marginBottom: 4 } }, "Whisper model"),
        React.createElement("input", {
          value: "Whisper base (multilingual)", readOnly: true,
          style: { width: "100%", padding: "6px 10px", border: "1px solid var(--border-2)", borderRadius: 4, background: "var(--bg-2)", fontFamily: "var(--font-sans)", fontSize: 13, color: "var(--fg-1)" },
        }),

        React.createElement("div", { style: { fontFamily: "var(--font-sans)", fontSize: 11, fontWeight: 500, textTransform: "uppercase", letterSpacing: "0.06em", color: "var(--fg-4)", margin: "14px 0 8px" } }, "Download Whisper Models"),
        React.createElement(ModelRow, { name: "Whisper tiny (multilingual)", meta: "75 MB. Fastest whisper, rough accuracy. 15-30x realtime on CPU." }),
        React.createElement(ModelRow, { name: "Whisper base (multilingual)", meta: "142 MB. Better than tiny. 10-20x realtime on CPU.", action: "Installed" })
      ),
      React.createElement("div", { className: "forge-modal-foot" },
        React.createElement("span", { style: { fontFamily: "var(--font-sans)", fontSize: 11, color: "var(--fg-4)" } }, "All saved"),
        React.createElement("div", { style: { display: "flex", gap: 8 } },
          React.createElement("button", { onClick: onClose, style: { fontFamily: "var(--font-sans)", fontSize: 13, padding: "5px 12px", border: "1px solid var(--border-2)", borderRadius: 4, background: "var(--bg-2)", color: "var(--fg-1)", cursor: "pointer" } }, "Close"),
          React.createElement("button", { style: { fontFamily: "var(--font-sans)", fontSize: 13, padding: "5px 12px", border: 0, borderRadius: 4, background: "var(--accent)", color: "#fff", cursor: "pointer", fontWeight: 500 } }, "Save")
        )
      )
    )
  );
}

window.SettingsModal = SettingsModal;
