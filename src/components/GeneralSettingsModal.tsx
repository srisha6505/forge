import { useEffect, useState, type CSSProperties, type ReactNode } from "react";
import {
  GhostBtn,
  SecondaryBtn,
  PrimaryBtn,
  SegCtrl,
  Toggle,
  InputField,
  Kbd,
  Divider,
} from "./ui";
import { X } from "./ui/Icons";
import {
  getVaultSettings,
  setVaultSettings,
  type VaultSettings,
} from "../lib/tauri";

type Theme = "light" | "dark";
type ThemeChoice = "light" | "dark" | "system";

interface Props {
  open: boolean;
  onClose: () => void;
  theme: Theme;
  onToggleTheme: () => void;
  // The currently-open vault. Settings are scoped per-vault, so this
  // modal is a read-modify-write against `<vault>/.forge/settings.json`
  // when a vault is open. With no vault, the persistence calls are
  // skipped and the form is purely local — keeps the modal usable on
  // the empty-state shell.
  vaultPath: string | null;
}

type TabId = "appearance" | "vault" | "editor" | "shortcuts" | "about";

const TABS: { id: TabId; label: string }[] = [
  { id: "appearance", label: "Appearance" },
  { id: "vault", label: "Vault" },
  { id: "editor", label: "Editor" },
  { id: "shortcuts", label: "Shortcuts" },
  { id: "about", label: "About" },
];

// General application settings: appearance, vault layout, editor
// defaults, shortcuts, and an about panel. Lightweight — only the
// theme toggle writes back to the app (via onToggleTheme). Everything
// else is local state until the persistence pass; see the
// TODO: persist comments.
export default function GeneralSettingsModal({
  open,
  onClose,
  theme,
  onToggleTheme,
  vaultPath,
}: Props) {
  const [tab, setTab] = useState<TabId>("appearance");

  // Appearance local state
  const [themeChoice, setThemeChoice] = useState<ThemeChoice>(theme);
  const [fontSize, setFontSize] = useState(15);
  const [readableWidth, setReadableWidth] = useState(false);
  const [zoom, setZoom] = useState(100);

  // Vault local state
  const [autoOpen, setAutoOpen] = useState(true);
  const [hideDots, setHideDots] = useState(true);
  const [showChat, setShowChat] = useState(false);
  const [excluded, setExcluded] = useState(".git, node_modules");

  // Editor local state
  const [saveDeb, setSaveDeb] = useState(300);
  const [dirtyInd, setDirtyInd] = useState(true);
  const [atomicWrite, setAtomicWrite] = useState(true);
  const [wikiNewTab, setWikiNewTab] = useState(false);
  const [defaultPose, setDefaultPose] = useState<"read" | "edit">("edit");

  // Cached VaultSettings so save can pass through fields we don't yet
  // surface in the UI (ai, voice, system_prompt, tools_allowed).
  const [vsCache, setVsCache] = useState<VaultSettings | null>(null);

  useEffect(() => {
    if (!open || !vaultPath) return;
    let cancelled = false;
    getVaultSettings(vaultPath)
      .then((vs) => {
        if (cancelled) return;
        setVsCache(vs);
        if (vs.theme === "light" || vs.theme === "dark") {
          setThemeChoice(vs.theme);
        }
      })
      .catch((e) => console.warn("vault settings load failed", e));
    return () => {
      cancelled = true;
    };
  }, [open, vaultPath]);

  if (!open) return null;

  const persistVault = async (next: Partial<VaultSettings>) => {
    if (!vaultPath || !vsCache) return;
    const merged: VaultSettings = { ...vsCache, ...next };
    setVsCache(merged);
    try {
      await setVaultSettings(vaultPath, merged);
    } catch (e) {
      console.warn("vault settings save failed", e);
    }
  };

  const handleThemeChange = (v: string) => {
    const next = v as ThemeChoice;
    setThemeChoice(next);
    // We only actually flip the app when the user explicitly picks the
    // opposite of the current theme. "system" stays visual-only for
    // now (no system-pref wiring yet).
    if (next === "light" && theme === "dark") onToggleTheme();
    if (next === "dark" && theme === "light") onToggleTheme();
    if (next === "light" || next === "dark") {
      // Persist the explicit choice; "system" remains UI-only.
      void persistVault({ theme: next });
    }
  };

  // Each non-theme field is local until close; the parent App also
  // persists theme + panel widths via its debounced VaultSettings push.
  // Surface unused-warnings explicitly until those fields ship to disk.
  void fontSize;
  void readableWidth;
  void zoom;
  void autoOpen;
  void hideDots;
  void showChat;
  void excluded;
  void saveDeb;
  void dirtyInd;
  void atomicWrite;
  void wikiNewTab;
  void defaultPose;

  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed",
        inset: 0,
        background: "var(--modal-backdrop)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: "var(--z-modal-backdrop)" as unknown as number,
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          background: "var(--background-primary)",
          border: "1px solid var(--background-modifier-border)",
          borderRadius: "var(--radius-l)",
          boxShadow: "var(--shadow-l)",
          width: 720,
          maxWidth: "96vw",
          maxHeight: "85vh",
          display: "flex",
          flexDirection: "column",
          zIndex: "var(--z-modal)" as unknown as number,
        }}
      >
        {/* Header */}
        <div
          style={{
            padding: "20px 24px 0",
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
          }}
        >
          <span
            style={{
              fontSize: "var(--font-ui-larger)",
              fontWeight: 600,
              color: "var(--text-normal)",
            }}
          >
            Settings
          </span>
          <GhostBtn icon={<X size={16} />} label="Close" onClick={onClose} />
        </div>

        {/* Tabs */}
        <div
          style={{
            display: "flex",
            gap: 0,
            padding: "0 24px",
            borderBottom: "1px solid var(--background-modifier-border)",
            marginTop: 12,
          }}
        >
          {TABS.map((t) => (
            <TabButton
              key={t.id}
              active={tab === t.id}
              onClick={() => setTab(t.id)}
              label={t.label}
            />
          ))}
        </div>

        {/* Body */}
        <div
          style={{ flex: 1, overflowY: "auto", padding: "16px 24px 20px" }}
        >
          {tab === "appearance" && (
            <>
              <SettingRow label="Theme">
                <SegCtrl
                  options={[
                    { value: "light", label: "Light" },
                    { value: "dark", label: "Dark" },
                    { value: "system", label: "System" },
                  ]}
                  value={themeChoice}
                  onChange={handleThemeChange}
                />
              </SettingRow>
              <Divider />
              <SettingRow label="Interface font">
                <span
                  style={{
                    fontSize: "var(--font-ui-medium)",
                    color: "var(--text-muted)",
                  }}
                >
                  Manrope
                </span>
              </SettingRow>
              <Divider />
              <SettingRow label="Editor font">
                <span
                  style={{
                    fontSize: "var(--font-ui-medium)",
                    color: "var(--text-muted)",
                  }}
                >
                  Manrope
                </span>
              </SettingRow>
              <Divider />
              <SliderRow
                label="Base font size"
                value={fontSize}
                min={12}
                max={24}
                step={1}
                unit="px"
                onChange={setFontSize}
              />
              <Divider />
              <SettingRow
                label="Readable line width"
                description="Limit editor content width to 820px"
              >
                <Toggle on={readableWidth} onChange={setReadableWidth} />
              </SettingRow>
              <Divider />
              <SliderRow
                label="Zoom level"
                value={zoom}
                min={75}
                max={200}
                step={5}
                unit="%"
                onChange={setZoom}
              />
            </>
          )}

          {tab === "vault" && (
            <>
              <SettingRow label="Vault path">
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <span
                    style={{
                      fontSize: "var(--font-ui-small)",
                      color: "var(--text-muted)",
                      fontFamily: "var(--font-monospace)",
                    }}
                  >
                    ~/Documents/my-vault
                  </span>
                  {/* TODO: persist — wire to open_vault picker */}
                  <SecondaryBtn>Change</SecondaryBtn>
                </div>
              </SettingRow>
              <Divider />
              <SettingRow label="Auto-open on launch">
                <Toggle on={autoOpen} onChange={setAutoOpen} />
              </SettingRow>
              <Divider />
              <SettingRow label="Hide dotfiles">
                <Toggle on={hideDots} onChange={setHideDots} />
              </SettingRow>
              <Divider />
              <SettingRow label="Show chat files in sidebar">
                <Toggle on={showChat} onChange={setShowChat} />
              </SettingRow>
              <Divider />
              <SettingRow
                label="Excluded folders"
                description="Comma-separated list"
              >
                <InputField
                  value={excluded}
                  onChange={setExcluded}
                  style={{ width: 200 }}
                />
              </SettingRow>
            </>
          )}

          {tab === "editor" && (
            <>
              <SliderRow
                label="Save debounce"
                value={saveDeb}
                min={100}
                max={2000}
                step={100}
                unit="ms"
                onChange={setSaveDeb}
              />
              <Divider />
              <SettingRow label="Show dirty indicator">
                <Toggle on={dirtyInd} onChange={setDirtyInd} />
              </SettingRow>
              <Divider />
              <SettingRow
                label="Atomic writes"
                description="Write to .tmp then rename (prevents data loss)"
              >
                <Toggle on={atomicWrite} onChange={setAtomicWrite} />
              </SettingRow>
              <Divider />
              <SettingRow label="Wikilinks open in new tab">
                <Toggle on={wikiNewTab} onChange={setWikiNewTab} />
              </SettingRow>
              <Divider />
              <SettingRow label="Default pose for existing files">
                <SegCtrl
                  options={[
                    { value: "read", label: "Read" },
                    { value: "edit", label: "Edit" },
                  ]}
                  value={defaultPose}
                  onChange={(v) => setDefaultPose(v as "read" | "edit")}
                />
              </SettingRow>
            </>
          )}

          {tab === "shortcuts" && <ShortcutsTab />}

          {tab === "about" && <AboutTab />}
        </div>

        {/* Footer */}
        <div
          style={{
            padding: "12px 24px",
            borderTop: "1px solid var(--background-modifier-border)",
            display: "flex",
            justifyContent: "flex-end",
            gap: 8,
          }}
        >
          <SecondaryBtn onClick={onClose}>Cancel</SecondaryBtn>
          {/* TODO: persist — bundle all tab state through setSettings */}
          <PrimaryBtn onClick={onClose}>Save</PrimaryBtn>
        </div>
      </div>
    </div>
  );
}

function TabButton({
  active,
  onClick,
  label,
}: {
  active: boolean;
  onClick: () => void;
  label: string;
}) {
  return (
    <button
      onClick={onClick}
      style={{
        height: 36,
        padding: "0 14px",
        background: "transparent",
        border: 0,
        borderBottom: active
          ? "2px solid var(--text-accent)"
          : "2px solid transparent",
        color: active ? "var(--text-normal)" : "var(--text-muted)",
        fontSize: "var(--font-ui-medium)",
        fontWeight: 500,
        cursor: "pointer",
        transition: "color var(--motion-duration-fast) var(--motion-ease)",
      }}
      onMouseEnter={(e) => {
        if (!active)
          e.currentTarget.style.background =
            "var(--background-modifier-hover)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.background = "transparent";
      }}
    >
      {label}
    </button>
  );
}

function SettingRow({
  label,
  description,
  children,
}: {
  label: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <div
      style={{
        display: "flex",
        justifyContent: "space-between",
        alignItems: "center",
        minHeight: 40,
        padding: "8px 0",
      }}
    >
      <div style={{ flex: 1 }}>
        <div
          style={{
            fontSize: "var(--font-ui-medium)",
            fontWeight: 500,
            color: "var(--text-normal)",
          }}
        >
          {label}
        </div>
        {description && (
          <div
            style={{
              fontSize: "var(--font-ui-small)",
              color: "var(--text-muted)",
              marginTop: 2,
            }}
          >
            {description}
          </div>
        )}
      </div>
      <div style={{ marginLeft: 24, flexShrink: 0 }}>{children}</div>
    </div>
  );
}

function SliderRow({
  label,
  value,
  min,
  max,
  step,
  unit,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  unit?: string;
  onChange: (v: number) => void;
}) {
  return (
    <SettingRow label={label}>
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <input
          type="range"
          min={min}
          max={max}
          step={step}
          value={value}
          onChange={(e) => onChange(Number(e.target.value))}
          style={{ width: 120, accentColor: "var(--interactive-accent)" }}
        />
        <span
          style={{
            fontSize: "var(--font-ui-small)",
            color: "var(--text-muted)",
            minWidth: 40,
            textAlign: "right" as CSSProperties["textAlign"],
          }}
        >
          {value}
          {unit ?? ""}
        </span>
      </div>
    </SettingRow>
  );
}

const SHORTCUTS: { cmd: string; keys: string }[] = [
  { cmd: "Save", keys: "Ctrl S" },
  { cmd: "Toggle sidebar", keys: "Ctrl B" },
  { cmd: "Toggle chat", keys: "Ctrl ⇧ L" },
  { cmd: "New file", keys: "Ctrl N" },
  { cmd: "Open command palette", keys: "Ctrl P" },
  { cmd: "Search vault", keys: "Ctrl ⇧ F" },
  { cmd: "Find in file", keys: "Ctrl F" },
  { cmd: "Open graph", keys: "Ctrl G" },
  { cmd: "Toggle terminal", keys: "Ctrl `" },
  { cmd: "Open settings", keys: "Ctrl ," },
];

function ShortcutsTab() {
  return (
    <div>
      <div
        style={{
          display: "flex",
          height: 32,
          background: "var(--background-secondary)",
          borderRadius: "var(--radius-s)",
          marginBottom: 4,
          padding: "0 10px",
          alignItems: "center",
        }}
      >
        <span
          style={{
            flex: 1,
            fontSize: "var(--font-ui-small)",
            fontWeight: 600,
            color: "var(--text-muted)",
          }}
        >
          Command
        </span>
        <span
          style={{
            width: 120,
            fontSize: "var(--font-ui-small)",
            fontWeight: 600,
            color: "var(--text-muted)",
            textAlign: "right",
          }}
        >
          Binding
        </span>
      </div>
      {SHORTCUTS.map((s, i) => (
        <div
          key={i}
          style={{
            display: "flex",
            height: 32,
            padding: "0 10px",
            alignItems: "center",
            borderBottom: "1px solid var(--hr-color)",
            cursor: "pointer",
            transition: "background var(--motion-duration-fast) var(--motion-ease)",
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.background =
              "var(--background-modifier-hover)";
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = "transparent";
          }}
        >
          <span
            style={{
              flex: 1,
              fontSize: "var(--font-ui-medium)",
              color: "var(--text-normal)",
            }}
          >
            {s.cmd}
          </span>
          <Kbd>{s.keys}</Kbd>
        </div>
      ))}
    </div>
  );
}

function AboutTab() {
  return (
    <div
      style={{
        fontSize: "var(--font-ui-medium)",
        color: "var(--text-muted)",
        lineHeight: 1.7,
      }}
    >
      <div
        style={{
          fontWeight: 600,
          color: "var(--text-normal)",
          fontSize: "var(--font-ui-larger)",
          marginBottom: 12,
        }}
      >
        Forge
      </div>
      <div>Version 0.4.0-alpha</div>
      <div>
        Build:{" "}
        <span
          style={{
            fontFamily: "var(--font-monospace)",
            fontSize: "var(--font-ui-small)",
          }}
        >
          a3b8f2d
        </span>
      </div>
      <div style={{ marginTop: 16 }}>
        <a href="#" style={{ color: "var(--text-link)" }}>
          License
        </a>
        <span style={{ margin: "0 8px", color: "var(--text-faint)" }}>·</span>
        <a href="#" style={{ color: "var(--text-link)" }}>
          Open logs
        </a>
      </div>
      <div style={{ marginTop: 24 }}>
        {/* TODO: persist — reset settings via setSettings with defaults */}
        <SecondaryBtn>Reset settings</SecondaryBtn>
      </div>
    </div>
  );
}
