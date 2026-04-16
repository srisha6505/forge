import { Component, type ReactNode } from "react";

interface Props {
  children: ReactNode;
  fallback?: (error: Error) => ReactNode;
}

interface State {
  error: Error | null;
}

// Minimal error boundary. Wraps heavy components (CodeMirror, MarkdownPreview)
// so that a runtime throw shows a readable error in place of a white screen.
export default class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error("[ErrorBoundary]", error, info.componentStack);
  }

  reset = () => this.setState({ error: null });

  render() {
    if (this.state.error) {
      if (this.props.fallback) return this.props.fallback(this.state.error);
      return (
        <div className="flex-1 min-h-0 flex flex-col items-center justify-center gap-3 p-8 text-center">
          <div className="text-[14px] font-semibold text-[var(--text-error)]">
            Something crashed in this view
          </div>
          <pre className="text-[11px] text-[var(--text-muted)] whitespace-pre-wrap max-w-[600px] font-mono">
            {this.state.error.message}
          </pre>
          <button
            onClick={this.reset}
            className="mt-2 px-3 py-1.5 text-[11px] rounded-md border border-[var(--background-modifier-border)] hover:border-[var(--interactive-accent)] text-[var(--text-muted)] hover:text-[var(--text-accent)] transition-colors"
          >
            Try again
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
