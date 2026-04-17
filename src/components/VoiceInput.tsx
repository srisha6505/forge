import { useEffect, useRef, useState } from "react";
import { Mic, Square, Loader2 } from "lucide-react";
import { startRecording, stopRecordingAndTranscribe } from "../lib/tauri";

interface VoiceInputProps {
  onTranscript: (text: string) => void;
  disabled?: boolean;
}

type State = "idle" | "recording" | "transcribing";

export function VoiceInput({ onTranscript, disabled }: VoiceInputProps) {
  const [state, setState] = useState<State>("idle");
  const tickRef = useRef<number | null>(null);
  const [pulse, setPulse] = useState(0);

  useEffect(() => {
    return () => {
      if (tickRef.current != null) clearInterval(tickRef.current);
    };
  }, []);

  async function begin() {
    try {
      await startRecording();
      setState("recording");
      tickRef.current = window.setInterval(() => setPulse((p) => p + 1), 500);
    } catch (err) {
      console.error("start_recording", err);
      alert(`Mic start failed: ${err}`);
    }
  }

  async function finish() {
    if (tickRef.current != null) clearInterval(tickRef.current);
    tickRef.current = null;
    setState("transcribing");
    try {
      const text = await stopRecordingAndTranscribe();
      onTranscript(text.trim());
    } catch (err) {
      console.error("transcribe", err);
      alert(`Transcription failed: ${err}`);
    } finally {
      setState("idle");
    }
  }

  const handleClick = () => {
    if (disabled) return;
    if (state === "idle") void begin();
    else if (state === "recording") void finish();
  };

  const label =
    state === "recording" ? "Stop recording" : state === "transcribing" ? "Transcribing…" : "Voice input";

  return (
    <button
      type="button"
      onClick={handleClick}
      disabled={disabled || state === "transcribing"}
      title={label}
      aria-label={label}
      className={`voice-input-btn voice-input-${state}`}
      data-pulse={pulse}
    >
      {state === "idle" && <Mic size={16} />}
      {state === "recording" && <Square size={14} fill="currentColor" />}
      {state === "transcribing" && <Loader2 size={16} className="voice-input-spin" />}
    </button>
  );
}
