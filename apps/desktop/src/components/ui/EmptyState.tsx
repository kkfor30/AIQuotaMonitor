import type { ReactNode } from "react";
import { Inbox, type LucideIcon } from "lucide-react";
import { cn } from "@/lib/cn";

export type EmptyStateTone = "primary" | "neutral" | "warning" | "success";
export type EmptyStateVariant = "default" | "compact" | "inline";

const TONE_STYLES: Record<
  EmptyStateTone,
  {
    iconBg: string;
    iconBorder: string;
    iconText: string;
  }
> = {
  primary: {
    iconBg: "bg-q-primary-soft",
    iconBorder: "border-q-border-selected",
    iconText: "text-q-primary",
  },
  neutral: {
    iconBg: "bg-q-surface-muted",
    iconBorder: "border-q-border",
    iconText: "text-q-text-muted",
  },
  warning: {
    iconBg: "bg-q-warning-soft",
    iconBorder: "border-q-warning/30",
    iconText: "text-q-warning",
  },
  success: {
    iconBg: "bg-q-success-soft",
    iconBorder: "border-q-success/30",
    iconText: "text-q-success",
  },
};

/**
 * 统一空状态组件：
 * - 适用于未配置平台、无刷新记录、无关注项、图表无点等各种场景
 * - 支持 default（全宽卡片居中）、compact（内嵌卡片）、inline（紧凑行）
 */
export function EmptyState({
  title,
  description,
  action,
  icon: Icon = Inbox,
  tone = "neutral",
  variant = "default",
  className,
}: {
  title: string;
  description?: string;
  action?: ReactNode;
  icon?: LucideIcon | ReactNode;
  tone?: EmptyStateTone;
  variant?: EmptyStateVariant;
  className?: string;
}) {
  const toneStyle = TONE_STYLES[tone];
  const isComponentIcon = typeof Icon === "function";

  if (variant === "inline") {
    return (
      <div
        className={cn(
          "flex items-center gap-2.5 px-3 py-2.5 text-xs text-q-text-muted",
          className,
        )}
      >
        <span
          aria-hidden
          className={cn(
            "flex h-6 w-6 shrink-0 items-center justify-center rounded-md border",
            toneStyle.iconBg,
            toneStyle.iconBorder,
            toneStyle.iconText,
          )}
        >
          {isComponentIcon ? <Icon size={13} aria-hidden /> : Icon}
        </span>
        <span className="font-medium text-q-text-secondary">{title}</span>
        {description && <span className="text-q-text-muted">· {description}</span>}
        {action && <div className="ml-auto">{action}</div>}
      </div>
    );
  }

  if (variant === "compact") {
    return (
      <div
        className={cn(
          "flex flex-col items-center justify-center gap-2 p-6 text-center",
          className,
        )}
      >
        <div
          aria-hidden
          className={cn(
            "flex h-9 w-9 items-center justify-center rounded-q-control border shadow-q-sm",
            toneStyle.iconBg,
            toneStyle.iconBorder,
            toneStyle.iconText,
          )}
        >
          {isComponentIcon ? <Icon size={17} aria-hidden /> : Icon}
        </div>
        <p className="text-[13px] font-medium text-q-text-primary">{title}</p>
        {description && (
          <p className="max-w-xs text-[11.5px] leading-relaxed text-q-text-secondary">
            {description}
          </p>
        )}
        {action && <div className="mt-1">{action}</div>}
      </div>
    );
  }

  return (
    <div
      className={cn(
        "glass-panel flex flex-1 flex-col items-center justify-center gap-3.5 p-10 text-center animate-fade-in",
        className,
      )}
    >
      <div
        aria-hidden
        className={cn(
          "flex h-12 w-12 items-center justify-center rounded-[16px] border shadow-q-sm backdrop-blur-sm transition-transform duration-200 hover:scale-105",
          toneStyle.iconBg,
          toneStyle.iconBorder,
          toneStyle.iconText,
        )}
      >
        {isComponentIcon ? <Icon size={22} aria-hidden /> : Icon}
      </div>
      <div className="flex flex-col items-center gap-1">
        <p className="text-[15px] font-semibold tracking-tight text-q-text-primary">
          {title}
        </p>
        {description && (
          <p className="max-w-sm text-[13px] leading-relaxed text-q-text-secondary">
            {description}
          </p>
        )}
      </div>
      {action && <div className="mt-1 flex items-center gap-2">{action}</div>}
    </div>
  );
}
