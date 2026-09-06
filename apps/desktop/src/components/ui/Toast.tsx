import { useEffect, useState } from "react";
import { AlertCircle, AlertTriangle, CheckCircle2, Info, X } from "lucide-react";
import { cn } from "@/lib/cn";

export type ToastTone = "success" | "error" | "info" | "warning";

export interface ToastItem {
  id: string;
  tone: ToastTone;
  title: string;
  description?: string;
  duration?: number;
}

type ToastListener = (toasts: ToastItem[]) => void;

let listeners: ToastListener[] = [];
let currentToasts: ToastItem[] = [];

function notify() {
  listeners.forEach((fn) => fn([...currentToasts]));
}

export const toast = {
  show: (
    options:
      | {
          title: string;
          description?: string;
          tone?: ToastTone;
          duration?: number;
        }
      | string,
  ) => {
    const opts = typeof options === "string" ? { title: options } : options;
    const id = Math.random().toString(36).substring(2, 9);
    const item: ToastItem = {
      id,
      tone: opts.tone ?? "info",
      title: opts.title,
      description: opts.description,
      duration: opts.duration ?? 2800,
    };
    currentToasts = [item, ...currentToasts.slice(0, 3)];
    notify();

    if (item.duration && item.duration > 0) {
      window.setTimeout(() => {
        toast.dismiss(id);
      }, item.duration);
    }
    return id;
  },

  success: (title: string, description?: string, duration?: number) =>
    toast.show({ tone: "success", title, description, duration }),

  error: (title: string, description?: string, duration?: number) =>
    toast.show({ tone: "error", title, description, duration }),

  info: (title: string, description?: string, duration?: number) =>
    toast.show({ tone: "info", title, description, duration }),

  warning: (title: string, description?: string, duration?: number) =>
    toast.show({ tone: "warning", title, description, duration }),

  dismiss: (id: string) => {
    currentToasts = currentToasts.filter((t) => t.id !== id);
    notify();
  },
};

const TONE_CONFIG: Record<
  ToastTone,
  {
    icon: typeof CheckCircle2;
    iconClass: string;
    borderClass: string;
  }
> = {
  success: {
    icon: CheckCircle2,
    iconClass: "text-q-success bg-q-success-soft border-q-success/30",
    borderClass: "hover:border-q-success/40",
  },
  error: {
    icon: AlertCircle,
    iconClass: "text-q-danger bg-q-danger-soft border-q-danger/30",
    borderClass: "hover:border-q-danger/40",
  },
  warning: {
    icon: AlertTriangle,
    iconClass: "text-q-warning bg-q-warning-soft border-q-warning/30",
    borderClass: "hover:border-q-warning/40",
  },
  info: {
    icon: Info,
    iconClass: "text-q-primary bg-q-primary-soft border-q-border-selected",
    borderClass: "hover:border-q-primary/40",
  },
};

/**
 * 全局毛玻璃 Toast 浮层容器。
 * 挂载于 App 外壳顶层，支持多条通知排队与自动平滑出场。
 */
export function ToastContainer() {
  const [items, setItems] = useState<ToastItem[]>([]);

  useEffect(() => {
    const listener = (next: ToastItem[]) => setItems(next);
    listeners.push(listener);
    return () => {
      listeners = listeners.filter((l) => l !== listener);
    };
  }, []);

  if (items.length === 0) return null;

  return (
    <div
      aria-live="polite"
      aria-atomic="true"
      className="fixed top-11 right-5 z-[100] flex w-full max-w-[340px] flex-col gap-2.5 pointer-events-none"
    >
      {items.map((item) => (
        <ToastCard key={item.id} item={item} onDismiss={() => toast.dismiss(item.id)} />
      ))}
    </div>
  );
}

function ToastCard({
  item,
  onDismiss,
}: {
  item: ToastItem;
  onDismiss: () => void;
}) {
  const config = TONE_CONFIG[item.tone];
  const Icon = config.icon;

  return (
    <div
      role="status"
      className={cn(
        "pointer-events-auto flex items-start gap-3 rounded-[15px] border border-q-border-strong bg-q-surface-strong/95 p-3.5 shadow-q-lg backdrop-blur-2xl animate-slide-in-top transition-all duration-200",
        config.borderClass,
      )}
      style={{
        boxShadow: "inset 0 1px 0 0 rgba(255, 255, 255, 0.15), var(--q-shadow-lg)",
      }}
    >
      <span
        aria-hidden
        className={cn(
          "flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border",
          config.iconClass,
        )}
      >
        <Icon size={16} />
      </span>

      <div className="min-w-0 flex-1 pt-0.5">
        <p className="text-[13px] font-semibold tracking-tight text-q-text-primary">
          {item.title}
        </p>
        {item.description && (
          <p className="mt-0.5 text-[11.5px] leading-relaxed text-q-text-secondary">
            {item.description}
          </p>
        )}
      </div>

      <button
        type="button"
        aria-label="关闭通知"
        onClick={onDismiss}
        className="flex h-5 w-5 shrink-0 items-center justify-center rounded-md text-q-text-muted transition-colors hover:bg-q-surface-hover hover:text-q-text-primary"
      >
        <X size={13} />
      </button>
    </div>
  );
}
