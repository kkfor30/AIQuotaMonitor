import { Trash2 } from "lucide-react";
import { Boxes } from "lucide-react";
import { cn } from "@/lib/cn";
import { providerBrand } from "@/lib/provider-brand";
import type { PlatformSummaryViewModel } from "@/lib/types";
import { AggregateStatusBadge } from "@/components/ui/StatusBadge";
import { Button } from "@/components/ui/Button";

/**
 * 平台品牌标记：官方 Logo（白色圆角玻璃底），未知平台回退中性图标。
 * 禁止字母方块或自绘品牌图形。
 */
export function PlatformMark({ providerId, size = 36 }: { providerId: string; size?: number }) {
  const brand = providerBrand(providerId);
  const radius = Math.round(size * 0.3);
  return (
    <div
      aria-hidden
      className={cn(
        "flex shrink-0 items-center justify-center border border-q-border bg-white shadow-q-sm",
      )}
      style={{ width: size, height: size, borderRadius: radius }}
    >
      {brand.logo ? (
        <img
          src={brand.logo}
          alt=""
          draggable={false}
          className="pointer-events-none select-none"
          style={{ width: size * 0.72, height: size * 0.72, objectFit: "contain" }}
        />
      ) : (
        <Boxes size={size * 0.5} className="text-q-neutral" aria-hidden />
      )}
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
  onRemove,
}: {
  platforms: PlatformSummaryViewModel[];
  selectedId: string | null;
  onSelect: (providerId: string) => void;
  onAdd: () => void;
  onRemove?: (providerId: string) => void;
}) {
  return (
    <aside className="mr-3 flex w-[232px] shrink-0 flex-col gap-2 rounded-[18px] border border-q-border bg-q-surface p-3 shadow-q-sm backdrop-blur-xl">
      <p className="px-2 pb-1 pt-1 text-xs font-medium tracking-wide text-q-text-muted">平台目录</p>
      <div className="flex flex-1 flex-col gap-1 overflow-y-auto">
        {platforms.map((platform) => {
          const selected = platform.providerId === selectedId;
          return (
            <div
              key={platform.providerId}
              className={cn(
                "group relative flex items-center gap-1 rounded-q-control border transition-colors duration-150",
                selected
                  ? "border-q-border-selected bg-q-surface-solid shadow-q-sm"
                  : "border-transparent hover:bg-q-surface-hover",
              )}
            >
              {selected && (
                <span
                  aria-hidden
                  className="absolute left-0 top-1/2 h-[22px] w-[3px] -translate-y-1/2 rounded-full bg-q-primary"
                />
              )}
              <button
                type="button"
                onClick={() => onSelect(platform.providerId)}
                aria-current={selected ? "true" : undefined}
                className="flex min-w-0 flex-1 cursor-pointer items-center gap-3 px-2.5 py-2.5 text-left"
              >
                <PlatformMark providerId={platform.providerId} />
                <div className="flex min-w-0 flex-1 flex-col gap-1">
                  <span
                    className={cn(
                      "truncate text-sm font-medium",
                      selected ? "text-q-text-primary" : "text-q-text-primary/90",
                    )}
                  >
                    {platform.displayName}
                  </span>
                  <AggregateStatusBadge status={platform.aggregateStatus} />
                </div>
              </button>
              {onRemove && (
                <button
                  type="button"
                  aria-label={`移除 ${platform.displayName}`}
                  title="移除平台"
                  className="mr-1 inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-q-text-muted opacity-0 transition-opacity hover:bg-q-danger-soft hover:text-q-danger group-hover:opacity-100"
                  onClick={(event) => {
                    event.stopPropagation();
                    onRemove(platform.providerId);
                  }}
                >
                  <Trash2 size={14} aria-hidden />
                </button>
              )}
            </div>
          );
        })}
      </div>
      <Button variant="secondary" className="mt-auto" onClick={onAdd}>
        + 添加平台
      </Button>
    </aside>
  );
}
