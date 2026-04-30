interface Props {
  label?: string;
}

export function RunningIndicator({ label = "Thinking" }: Props) {
  return (
    <div className="forge-chat__running" role="status" aria-live="polite">
      <span className="forge-chat__running-dot" aria-hidden />
      <span className="forge-chat__running-text">{label}</span>
    </div>
  );
}
