import { cn } from "@/lib/cn";
import type { PlatformSummaryViewModel } from "@/lib/types";
import { AggregateStatusBadge } from "@/components/ui/StatusBadge";
import { Button } from "@/components/ui/Button";

/** 平台品牌标记：首字母 + 品牌色（不使用外部 Logo 素材）。 */
const PLATFORM_MARK: Record<string, { text: string; bg: string }> = {
  deepseek: { text: "D", bg: "bg-gradient-to-br from-[#4d6bfe] to-[#7b5cff]" },
  openai: { text: "G", bg: "bg-gradient-to-br from-[#10a37f] to-[#0d8a6c]" },
  claude_code: { text: "C", bg: "bg-gradient-to-br from-[#d97757] to-[#c2542f]" },
  glm: { text: "Z", bg: "bg-gradient-to-br from-[#3859ff] to-[#2450e6]" },
  glm_intl: { text: "Z", bg: "bg-gradient-to-br from-[#2450e6] to-[#3859ff]" },
  kimi: { text: "K", bg: "bg-gradient-to-br from-[#ff5833] to-[#e03e20]" },
  mimo: { text: "M", bg: "bg-gradient-to-br from-[#8b5cf6] to-[#6d3fe0]" },
  minimax: { text: "X", bg: "bg-gradient-to-br from-[#3f8cff] to-[#1f6fe0]" },
  minimax_intl: { text: "X", bg: "bg-gradient-to-br from-[#1f6fe0] to-[#3f8cff]" },
};

export function PlatformMark({ providerId, size = 36 }: { providerId: string; size?: number }) {
  const mark = PLATFORM_MARK[providerId] ?? { text: providerId.slice(0, 1).toUpperCase(), bg: "bg-q-neutral" };
  return (
    <div
      aria-hidden
      className={cn(
        "flex shrink-0 items-center justify-center rounded-[10px] font-semibold text-white shadow-q-sm",
        mark.bg,
      )}
      style={{ width: size, height: size, fontSize: size * 0.42 }}
    >
      {mark.text}
    </div>
  );
}

/**
 * 平台目录：只展示用户已添加的平台，底部提供「添加平台」。
 * 每项显示平台级聚合状态徽章；当前平台高亮。
 */
export function ProviderRail({
  platforms,
  selectedId,
  onSelect,
  onAdd,
}: {
  platforms: PlatformSummaryViewModel[];
  selectedId: string | null;
  onSelect: (providerId: string) => void;
  onAdd: () => void;
}) {
  return (
    <aside className="flex w-[248px] shrink-0 flex-col gap-2 border-r border-q-border bg-q-surface-muted/60 p-3 backdrop-blur-xl">
      <p className="px-2 pb-1 pt-1 text-xs font-medium tracking-wide text-q-text-muted">平台目录</p>
      <div className="flex flex-1 flex-col gap-1 overflow-y-auto">
        {platforms.map((platform) => {
          const selected = platform.providerId === selectedId;
          return (
            <button
              key={platform.providerId}
              type="button"
              onClick={() => onSelect(platform.providerId)}
              aria-current={selected ? "true" : undefined}
              className={cn(
                "flex cursor-pointer items-center gap-3 rounded-q-control px-2.5 py-2.5 text-left transition-colors duration-150",
                selected
                  ? "border border-q-border-selected bg-white shadow-q-sm"
                  : "border border-transparent hover:bg-q-surface-hover",
              )}
            >
              <PlatformMark providerId={platform.providerId} />
              <div className="flex min-w-0 flex-1 flex-col gap-1">
                <span
                  className={cn(
                    "truncate text-sm font-medium",
                    selected ? "text-q-text-primary" : "text-q-text-primary/90",
                  )}
                  data-selectable="true"
                >
                  {platform.displayName}
                </span>
                <AggregateStatusBadge status={platform.aggregateStatus} />
              </div>
            </button>
          );
        })}
      </div>
      <Button variant="secondary" className="mt-auto" onClick={onAdd}>
        + 添加平台
      </Button>
    </aside>
  );
}
