/** パネル単位のエラー境界（担当: WS-F）。1 つのパネルの例外でアプリ全体が落ちないようにする */
import { Component, type ReactNode } from "react";

interface Props {
  label: string;
  children: ReactNode;
}

export class ErrorBoundary extends Component<Props, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error) {
    console.error(`[${this.props.label}]`, error);
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <div className="empty">
        <div className="banner danger" style={{ maxWidth: 560 }}>
          <span className="grow selectable">
            <b>{this.props.label}</b> の表示中にエラーが発生しました
            <br />
            <span className="mono">{error.message}</span>
          </span>
        </div>
        <button onClick={() => this.setState({ error: null })}>再表示</button>
      </div>
    );
  }
}
