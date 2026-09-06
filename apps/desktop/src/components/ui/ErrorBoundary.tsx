import { Component, type ErrorInfo, type ReactNode } from "react";
import { AlertTriangle, ChevronDown, ChevronRight, RefreshCw } from "lucide-react";
import { Button } from "./Button";
import { cn } from "@/lib/cn";

export interface ErrorBoundaryProps {
  children: ReactNode;
  fallback?: ReactNode | ((error: Error, reset: () => void) => ReactNode);
  title?: string;
  variant?: "page" | "panel" | "inline";
  className?: string;
  onReset?: () => void;
  onError?: (error: Error, errorInfo: ErrorInfo) => void;
}

interface ErrorBoundaryState {
  error: Error | null;
  showDetails: boolean;
}

/**
 * 通用 React 错误边界：捕获子树渲染期未处理异常，防止整窗卸载变白屏。
 * 提供 Apple Glass 玻璃质感降级卡片、一键重试与错误栈折叠。
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  override state: ErrorBoundaryState = {
    error: null,
    showDetails: false,
  };

  static getDerivedStateFromError(error: Error): Partial<ErrorBoundaryState> {
    return { error };
  }

  override componentDidCatch(error: Error, errorInfo: ErrorInfo): void {
    console.error("[ErrorBoundary caught error]:", error, errorInfo);
    this.props.onError?.(error, errorInfo);
  }

  handleReset = (): void => {
    this.props.onReset?.();
    this.setState({ error: null, showDetails: false });
  };

  toggleDetails = (): void => {
    this.setState((prev) => ({ showDetails: !prev.showDetails }));
  };

  override render(): ReactNode {
    const { error, showDetails } = this.state;
    if (!error) {
      return this.props.children;
    }

    if (typeof this.props.fallback === "function") {
      return this.props.fallback(error, this.handleReset);
    }

    if (this.props.fallback) {
      return this.props.fallback;
    }

    const {
      title = "页面渲染遇到问题",
      variant = "panel",
      className,
    } = this.props;

    if (variant === "inline") {
      return (
        <div
          role="alert"
          className={cn(
            "flex items-center gap-2 rounded-q-control border border-q-danger/30 bg-q-danger-soft px-3 py-2 text-xs text-q-danger",
            className,
          )}
        >
          <AlertTriangle size={14} className="shrink-0 text-q-danger" aria-hidden />
          <span className="min-w-0 flex-1 truncate font-medium">
            {error.message || "模块渲染异常"}
          </span>
          <button
            type="button"
            onClick={this.handleReset}
            className="cursor-pointer underline underline-offset-2 hover:opacity-80"
          >
            重试
          </button>
        </div>
      );
    }

    return (
      <div
        role="alert"
        className={cn(
          "glass-panel flex min-w-0 flex-1 flex-col gap-3 rounded-[16px] border border-q-danger/30 bg-q-danger-soft/30 p-5 text-left shadow-q-sm animate-fade-in",
          variant === "page" && "m-4 max-w-2xl",
          className,
        )}
      >
        <div className="flex items-start gap-3">
          <span
            aria-hidden
            className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] border border-q-danger/30 bg-q-danger-soft text-q-danger"
          >
            <AlertTriangle size={18} />
          </span>
          <div className="min-w-0 flex-1">
            <h3 className="text-[15px] font-semibold text-q-text-primary">{title}</h3>
            <p className="mt-1 text-xs leading-relaxed text-q-text-secondary">
              组件在呈现数据时发生异常，已拦截以保护其他功能正常工作。您可以尝试重新渲染。
            </p>
          </div>
        </div>

        <div className="rounded-q-control border border-q-danger/20 bg-q-surface-solid/80 p-3 text-xs text-q-danger">
          <p className="font-mono break-all font-medium">
            {error.name}: {error.message || "未知错误"}
          </p>
        </div>

        <div className="flex flex-wrap items-center justify-between gap-2 pt-1">
          <button
            type="button"
            onClick={this.toggleDetails}
            className="inline-flex cursor-pointer items-center gap-1 text-[11.5px] text-q-text-muted hover:text-q-text-secondary"
          >
            {showDetails ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
            {showDetails ? "收起错误堆栈" : "查看技术详情"}
          </button>

          <Button size="sm" onClick={this.handleReset} className="gap-1.5">
            <RefreshCw size={13} aria-hidden />
            重试此区域
          </Button>
        </div>

        {showDetails && error.stack && (
          <pre className="max-h-48 overflow-y-auto rounded-q-control border border-q-border bg-q-surface-muted p-2.5 font-mono text-[10.5px] text-q-text-secondary whitespace-pre-wrap select-text">
            {error.stack}
          </pre>
        )}
      </div>
    );
  }
}
