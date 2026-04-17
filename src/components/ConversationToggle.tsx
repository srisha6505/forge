import { useEffect, useRef, useState } from "react";
import { Headphones, PhoneOff, Pause, Play, Radio } from "lucide-react";
import {
  voiceStart,
  voiceStartWake,
  voiceStop,
  voiceInterrupt,
  voiceSetMuted,
  onVoiceState,
  onVoiceTranscript,
  onVoiceAssistantText,
  onVoiceTtsChunk,
  onVoiceError,
  onVoiceStopped,
  onVoiceBargeIn,
} from "../lib/tauri";

interface Props {
  disabled?: boolean;
}

type VoiceState = "off" | "listening" | "transcribing" | "thinking" | "speaking" | "muted" | "wake_active";

export function ConversationToggle({ disabled }: Props) {
  const [state, setState] = useState<VoiceState>("off");
  const [muted, setMuted] = useState(false);
  const audioQueueRef = useRef<string[]>([]);
  const playingRef = useRef(false);
  const currentAudioRef = useRef<HTMLAudioElement | null>(null);
  const abortRef = useRef(false);

  useEffect(() => {
    const subs: Promise<() => void>[] = [];
    subs.push(onVoiceState((s) => setState(s as VoiceState)));
    subs.push(onVoiceTranscript((t) => console.log("[voice] transcript:", t)));
    subs.push(onVoiceAssistantText((t) => console.log("[voice] assistant:", t)));
    subs.push(onVoiceTtsChunk((b64) => {
      if (abortRef.current) return;
      audioQueueRef.current.push(b64);
      void playQueue();
    }));
    subs.push(onVoiceError((e) => {
      console.error("[voice] error:", e);
    }));
    subs.push(onVoiceStopped(() => setState("off")));
    subs.push(onVoiceBargeIn(() => {
      // Wake word heard mid-TTS → immediately kill playback.
      cancelPlayback();
    }));
    return () => {
      subs.forEach((p) => void p.then((un) => un()));
    };
  }, []);

  async function playQueue() {
    if (playingRef.current) return;
    playingRef.current = true;
    try {
      while (audioQueueRef.current.length > 0) {
        if (abortRef.current) { audioQueueRef.current = []; break; }
        const b64 = audioQueueRef.current.shift()!;
        await new Promise<void>((resolve) => {
          const audio = new Audio("data:audio/wav;base64," + b64);
          currentAudioRef.current = audio;
          let done = false;
          const finish = () => { if (!done) { done = true; resolve(); } };
          audio.onended = finish;
          audio.onerror = finish;
          audio.onpause = finish; // cancelPlayback pauses → resolve here
          audio.play().catch(finish);
        });
        currentAudioRef.current = null;
      }
    } finally {
      playingRef.current = false;
    }
  }

  function cancelPlayback() {
    audioQueueRef.current = [];
    if (currentAudioRef.current) {
      try { currentAudioRef.current.pause(); } catch {}
      currentAudioRef.current = null;
    }
    // Abort only during this call; reset immediately so next session isn't
    // blocked. The queue was cleared above; any in-flight chunks arrive
    // after this and play normally.
    abortRef.current = true;
    setTimeout(() => { abortRef.current = false; }, 50);
  }

  const handleClick = async () => {
    if (disabled) return;
    if (state === "off") {
      abortRef.current = false;
      playingRef.current = false;
      audioQueueRef.current = [];
      setMuted(false);
      try {
        await voiceStart();
        setState("listening");
      } catch (e) {
        alert(`Voice start failed: ${e}`);
      }
    } else if (state === "speaking") {
      // First click while speaking = interrupt this reply, go back to listening.
      cancelPlayback();
      try { await voiceInterrupt(); } catch {}
    } else {
      // Any other state = fully stop session.
      cancelPlayback();
      try { await voiceStop(); } catch {}
      setState("off");
    }
  };

  const label = state === "off" ? "Start conversation"
    : state === "speaking" ? "Interrupt"
    : `Stop (${state})`;

  const toggleMute = async () => {
    const next = !muted;
    setMuted(next);
    try { await voiceSetMuted(next); } catch (e) { console.error(e); }
  };

  const startWake = async () => {
    if (disabled || state !== "off") return;
    abortRef.current = false;
    playingRef.current = false;
    audioQueueRef.current = [];
    setMuted(false);
    try {
      await voiceStartWake();
      setState("listening");
    } catch (e) {
      alert(`Wake-word start failed: ${e}`);
    }
  };

  const hint = state === "wake_active" ? "Riva listening…"
    : state === "listening" ? "Say \"Riva\" to activate"
    : state === "muted" ? "Muted"
    : null;

  return (
    <>
      <button
        type="button"
        onClick={handleClick}
        disabled={disabled}
        title={label}
        aria-label={label}
        className={`voice-input-btn conversation-btn conversation-${state}`}
      >
        {state === "off" ? <Headphones size={16} /> : <PhoneOff size={14} />}
      </button>
      {hint && (
        <span className="text-[11px] text-[var(--text-muted)] self-center whitespace-nowrap" style={{ marginLeft: 4 }}>
          {hint}
        </span>
      )}
      {state === "off" && (
        <button
          type="button"
          onClick={startWake}
          disabled={disabled}
          title='Wake-word mode ("Riva ...")'
          aria-label="Wake-word mode"
          className="voice-input-btn"
        >
          <Radio size={16} />
        </button>
      )}
      {state !== "off" && (
        <button
          type="button"
          onClick={toggleMute}
          title={muted ? "Resume listening" : "Pause listening"}
          className={`voice-input-btn ${muted ? "conversation-muted" : ""}`}
        >
          {muted ? <Play size={14} fill="currentColor" /> : <Pause size={14} fill="currentColor" />}
        </button>
      )}
    </>
  );
}
