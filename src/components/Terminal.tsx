import { useEffect, useRef } from "react";
import { Terminal as XTerm } from "xterm";
import { FitAddon } from "xterm-addon-fit";
import "xterm/css/xterm.css";
import {
  killTerminal,
  onTerminalOutput,
  resizeTerminal,
  spawnTerminal,
  writeTerminal,
} from "../lib/tauri";

interface Props {
  vaultPath: string | null;
}

// Decode base64 to Uint8Array. PTY output is raw bytes (ANSI escapes,
// UTF-8 multibyte sequences, etc.) so we forward to xterm verbatim.
function b64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

function readCssVar(name: string, fallback: string): string {
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return v || fallback;
}

export default function Terminal({ vaultPath }: Props) {
  const hostRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    let term: XTerm | null = null;
    let fitAddon: FitAddon | null = null;
    let sessionId: number | null = null;
    let unlistenOutput: (() => void) | null = null;
    let resizeObs: ResizeObserver | null = null;
    let dataDisposer: { dispose: () => void } | null = null;
    let cancelled = false;
    let pendingChunks: Uint8Array[] = [];

    // Read palette so the terminal blends with Forge's chrome rather
    // than xterm's stock black box.
    const bg = readCssVar("--background-primary", "#ffffff");
    const fg = readCssVar("--text-normal", "#202020");
    const accent = readCssVar("--interactive-accent", "#c08a2e");
    const muted = readCssVar("--text-muted", "#6b6b6b");
    const error = readCssVar("--text-error", "#d05a3a");
    const success = readCssVar("--text-success", "#638a3e");
    const info = readCssVar("--text-info", "#3a8ad0");
    const link = readCssVar("--text-link", "#c08a2e");
    const codeKw = readCssVar("--code-keyword", "#9b59b6");

    // Try Nerd Fonts first — most Linux setups have at least one
    // installed for terminal use. Fallback to Forge's monospace token.
    // Without a Nerd Font, the user's powerline-style prompt characters
    // (segment glyphs, chevrons) render as boxes; xterm has no way to
    // ship them.
    const userMonospace =
      getComputedStyle(document.documentElement)
        .getPropertyValue("--font-monospace")
        .trim() || "monospace";
    const fontFamily = `"JetBrainsMono Nerd Font", "FiraCode Nerd Font", "Hack Nerd Font", "MesloLGS NF", ${userMonospace}`;

    term = new XTerm({
      fontFamily,
      fontSize: 13,
      // No blink — consistent with the editor caret. CM6's pattern was
      // already to keep the caret solid, and a blinking terminal cursor
      // alongside a solid editor caret feels mismatched.
      cursorBlink: false,
      cursorStyle: "block",
      allowProposedApi: true,
      // Full 16-colour ANSI palette mapped to the Forge tokens so the
      // user's coloured prompt + ls / git output blend with the rest of
      // the chrome instead of fighting it.
      theme: {
        background: bg,
        foreground: fg,
        cursor: accent,
        cursorAccent: bg,
        selectionBackground: accent + "55",
        black: bg,
        red: error,
        green: success,
        yellow: link,
        blue: info,
        magenta: codeKw,
        cyan: info,
        white: fg,
        brightBlack: muted,
        brightRed: error,
        brightGreen: success,
        brightYellow: link,
        brightBlue: info,
        brightMagenta: codeKw,
        brightCyan: info,
        brightWhite: fg,
      },
    });
    fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(host);

    try {
      fitAddon.fit();
    } catch {
      /* host might not be measured yet — ResizeObserver retries below */
    }

    (async () => {
      try {
        const id = await spawnTerminal(vaultPath);
        if (cancelled) {
          // Unmounted before the spawn returned — kill the orphan.
          killTerminal(id).catch(() => {});
          return;
        }
        sessionId = id;
        // Drain anything that arrived before id was known.
        for (const bytes of pendingChunks) term?.write(bytes);
        pendingChunks = [];
        // Push the initial size to the PTY so the prompt formats right.
        if (term && fitAddon) {
          try {
            fitAddon.fit();
            await resizeTerminal(id, term.cols, term.rows);
          } catch {
            /* ignored */
          }
        }
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        term?.write(`\r\n\x1b[31mfailed to spawn terminal: ${msg}\x1b[0m\r\n`);
      }
    })();

    // Subscribe BEFORE spawn returns so we don't miss the welcome banner.
    onTerminalOutput((ev) => {
      if (cancelled) return;
      if (sessionId !== null && ev.id !== sessionId) return;
      const bytes = b64ToBytes(ev.bytes_b64);
      if (sessionId === null) {
        // Buffer until we know our id; otherwise we'd accept events
        // belonging to other sessions.
        pendingChunks.push(bytes);
      } else {
        term?.write(bytes);
      }
    })
      .then((un) => {
        if (cancelled) {
          un();
        } else {
          unlistenOutput = un;
        }
      })
      .catch(() => {});

    dataDisposer = term.onData((data) => {
      if (sessionId === null) return;
      writeTerminal(sessionId, data).catch(() => {});
    });

    resizeObs = new ResizeObserver(() => {
      if (!term || !fitAddon) return;
      try {
        fitAddon.fit();
        if (sessionId !== null) {
          resizeTerminal(sessionId, term.cols, term.rows).catch(() => {});
        }
      } catch {
        /* ignore measure-during-layout exceptions */
      }
    });
    resizeObs.observe(host);

    return () => {
      cancelled = true;
      try {
        dataDisposer?.dispose();
      } catch {
        /* ignored */
      }
      try {
        resizeObs?.disconnect();
      } catch {
        /* ignored */
      }
      try {
        unlistenOutput?.();
      } catch {
        /* ignored */
      }
      if (sessionId !== null) {
        killTerminal(sessionId).catch(() => {});
      }
      try {
        term?.dispose();
      } catch {
        /* ignored */
      }
    };
    // We intentionally only re-mount on vaultPath / themeKey change via
    // the parent's `key=` prop; mid-life vault swaps would orphan the
    // shell anyway.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div
      ref={hostRef}
      className="forge-terminal"
      style={{
        width: "100%",
        height: "100%",
        background: "var(--background-primary)",
        padding: "6px 8px",
        boxSizing: "border-box",
        overflow: "hidden",
      }}
    />
  );
}
