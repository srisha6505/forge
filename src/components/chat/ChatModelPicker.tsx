import { useEffect, useMemo, useRef, useState } from "react";
import { Check, ChevronDown } from "lucide-react";

// Provider-agnostic model row. Adapted from Copilot's CopilotModel,
// OpenAI/Anthropic/Gemini's ProviderModel, and the local GGUF ModelInfo
// in ChatTabView before being passed in. Keeps this component decoupled
// from the Tauri layer.
export interface PickerModel {
  id: string;
  name: string;
  vendor: string;
}

interface Props {
  models: PickerModel[];
  currentId: string;
  onSelect: (modelId: string) => void;
  disabled?: boolean;
  loading?: boolean;
}

// Vendor display order — matches GitHub Copilot's chat model list and
// puts the most-used providers first for non-Copilot pickers too.
const VENDOR_ORDER = ["OpenAI", "Anthropic", "Google", "xAI", "Local"];

function vendorRank(v: string): number {
  const i = VENDOR_ORDER.indexOf(v);
  return i === -1 ? VENDOR_ORDER.length : i;
}

// Lightweight popover: opens below the trigger, closes on outside click /
// Escape / blur. No portal — anchored to the trigger via absolute
// positioning inside a relative wrapper.
export function ChatModelPicker({
  models,
  currentId,
  onSelect,
  disabled = false,
  loading = false,
}: Props) {
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const popoverRef = useRef<HTMLDivElement | null>(null);

  const grouped = useMemo(() => {
    const byVendor = new Map<string, PickerModel[]>();
    for (const m of models) {
      const v = m.vendor || "Other";
      const arr = byVendor.get(v) ?? [];
      arr.push(m);
      byVendor.set(v, arr);
    }
    const entries = Array.from(byVendor.entries());
    entries.sort(([a], [b]) => vendorRank(a) - vendorRank(b));
    for (const [, arr] of entries) arr.sort((a, b) => a.name.localeCompare(b.name));
    return entries;
  }, [models]);

  const current = useMemo(
    () => models.find((m) => m.id === currentId) ?? null,
    [models, currentId],
  );

  useEffect(() => {
    if (!open) return;
    const onDocClick = (e: MouseEvent) => {
      const t = e.target as Node | null;
      if (!t) return;
      if (wrapRef.current?.contains(t)) return;
      if (popoverRef.current?.contains(t)) return;
      setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDocClick);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDocClick);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const label = loading
    ? "loading…"
    : current
      ? current.name
      : currentId || "pick model";

  return (
    <div ref={wrapRef} className="forge-modelpicker">
      <button
        type="button"
        className="forge-modelpicker__trigger"
        onClick={() => !disabled && setOpen((v) => !v)}
        disabled={disabled}
        title="Switch model"
        aria-haspopup="listbox"
        aria-expanded={open}
      >
        <span className="forge-modelpicker__trigger-label">{label}</span>
        <ChevronDown size={12} className="forge-modelpicker__trigger-chevron" />
      </button>
      {open && (
        <div ref={popoverRef} className="forge-modelpicker__popover" role="listbox">
          {grouped.length === 0 ? (
            <div className="forge-modelpicker__empty">
              {loading ? "Loading models…" : "No models available."}
            </div>
          ) : (
            grouped.map(([vendor, list]) => (
              <div key={vendor} className="forge-modelpicker__group">
                <div className="forge-modelpicker__group-header">{vendor}</div>
                {list.map((m) => {
                  const selected = m.id === currentId;
                  return (
                    <button
                      key={m.id}
                      type="button"
                      role="option"
                      aria-selected={selected}
                      className={
                        "forge-modelpicker__item" +
                        (selected ? " forge-modelpicker__item--selected" : "")
                      }
                      onClick={() => {
                        onSelect(m.id);
                        setOpen(false);
                      }}
                    >
                      <span className="forge-modelpicker__item-check">
                        {selected && <Check size={12} />}
                      </span>
                      <span className="forge-modelpicker__item-name">{m.name}</span>
                      <span className="forge-modelpicker__item-id">{m.id}</span>
                    </button>
                  );
                })}
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}
