import { AlertCircle, CheckCircle2, CircleX, Settings2 } from "lucide-react";
import type { ReactNode } from "react";
import type { DataFreshness, PlatformAggregateStatus } from "@/lib/types";
import { AGGREGATE_STATUS_META, FRESHNESS_META } from "@/lib/types";

type Tone = "success" | "warning" | "danger" | "neutral" | "primary";

const TONE_CLASS: Record<Tone, string> = {
  success: "bg-q-success-soft text-q-success-strong",
  warning: "bg-q-warning-soft text-q-warning",
  danger: "bg-q-danger-soft text-q-danger",
  neutral: "bg-q-neutral-soft text-q-neutral",
  primary: "bg-q-primary-soft text-q-primary",
};

const DOT_CLASS: Record<Tone, string> = {
  success: "bg-q-success",
  warning: "bg-q-warning",
  danger: "bg-q-danger",
  neutral: "bg-q-neutral",
  primary: "bg-q-primary",
};

/**
 * 通用状态徽章：圆点 + 文字（必要时附图标），状态表达永不依赖单一颜色。
 */
export function StatusBadge({
  tone,
  children,
  withIcon = false,
  icon,
}: {
  tone: Tone;
  children: ReactNode;
  withIcon?: boolean;
  icon?: ReactNode;
}) {
  return (
    <span
      className={`inline-flex shrink-0 items-center gap-1.5 whitespace-nowrap rounded-q-pill px-2.5 py-[3px] text-xs font-medium ${TONE_CLASS[tone]}`}
    >
      <span aria-hidden className={`h-1.5 w-1.5 rounded-full ${DOT_CLASS[tone]}`} />
      {withIcon && icon}
      {children}
    </span>
  );
}

const AGGREGATE_ICON = {
  "circle-check": <CheckCircle2 size={13} aria-hidden />,
  "circle-alert": <AlertCircle size={13} aria-hidden />,
  "circle-x": <CircleX size={13} aria-hidden />,
  settings: <Settings2 size={13} aria-hidden />,
} as const;

/** 平台聚合状态徽章。 */
export function AggregateStatusBadge({ status }: { status: PlatformAggregateStatus }) {
  const meta = (status && AGGREGATE_STATUS_META[status]) ?? {
    label: "需配置",
    icon: "settings" as const,
    tone: "neutral" as const,
  };
  return (
    <StatusBadge tone={meta.tone} withIcon icon={AGGREGATE_ICON[meta.icon]}>
      {meta.label}
    </StatusBadge>
  );
}

/** 数据新鲜度徽章：「实时」用低饱和蓝灰胶囊（新鲜度语义，不是主操作/额度状态），其余沿用状态语义色。 */
export function FreshnessTag({ freshness }: { freshness: DataFreshness }) {
  const meta = (freshness && FRESHNESS_META[freshness]) ?? {
    label: "暂无数据",
    tone: "neutral" as const,
  };
  if (freshness === "fresh") {
    return (
      <span className="live-pill">
        <span aria-hidden className="live-pill-dot" />
        {meta.label}
      </span>
    );
  }
  return <StatusBadge tone={meta.tone}>{meta.label}</StatusBadge>;
}
