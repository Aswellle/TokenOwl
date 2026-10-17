import { Component, type ErrorInfo, type ReactNode } from "react";

/**
 * Last line of defence for the webview.
 *
 * The dashboard and the bubble are two native windows rendering one React
 * tree each; an uncaught render error unmounts the whole tree and leaves an
 * empty webview — a blank window with no way back, which the release gate
 * treats as a blocker. This boundary keeps a readable failure state on screen
 * and offers the one action that actually helps: reload the webview.
 *
 * It deliberately does not log the error object: provider payloads can reach
 * this path and the app's logging contract forbids raw responses.
 */
interface Props {
  children: ReactNode;
  /** Short label for the failed surface ("仪表盘" / "悬浮球"). */
  surface: string;
}

interface State {
  failed: boolean;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { failed: false };

  static getDerivedStateFromError(): State {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // Component stack only — never the message, which may embed response text.
    console.error(`[${this.props.surface}] render failed`, info.componentStack);
    void error;
  }

  render() {
    if (!this.state.failed) return this.props.children;
    return (
      <div className="crash" role="alert">
        <h2 className="crash__title">{this.props.surface} 暂时无法显示</h2>
        <p className="crash__body">
          界面渲染出错。你的账户与密钥不受影响，重新加载即可恢复。
        </p>
        <button
          className="btn btn--primary"
          type="button"
          onClick={() => window.location.reload()}
        >
          重新加载
        </button>
      </div>
    );
  }
}
