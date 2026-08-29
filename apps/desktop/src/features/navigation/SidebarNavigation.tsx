import { useQuery } from "@tanstack/react-query";
import { NAV_ITEMS, type NavId } from "@/app/navigation";
import { cn } from "@/lib/cn";
import { fetchPlatformSummaries } from "@/lib/ipc";
import { PLATFORM_SUMMARIES_QUERY_KEY } from "@/lib/query-client";

/**
 * 左侧一级导航（Apple Glass V6）：玻璃列 + 新应用 Logo + 蓝色药丸选中态。
 * 底部展示由真实平台快照推导的监控状态与版本号。
 */
export function SidebarNavigation({
  active,
  onSelect,
}: {
  active: NavId;
  onSelect: (id: NavId) => void;
}) {
  const { data: platforms = [] } = useQuery({
    queryKey: PLATFORM_SUMMARIES_QUERY_KEY,
    queryFn: fetchPlatformSummaries,
  });

  const connected = platforms.filter((p) => p.aggregateStatus !== "setup_required");
  const attention = connected.filter(
    (p) => p.aggregateStatus === "partial" || p.aggregateStatus === "error",
  ).length;

  return (
    <nav className="mr-3 flex h-full w-[228px] shrink-0 flex-col rounded-[18px] border border-q-border bg-q-surface p-3 shadow-q-sm backdrop-blur-2xl">
      {/* Logo 区（同时作为窗口拖动区域的一部分） */}
      <div className="flex items-center gap-2.5 px-2 pb-3 pt-2" data-tauri-drag-region>
        <img
          src="/assets/brand/logo-aiquota-liquid-glass.png"
          alt=""
          draggable={false}
          className="h-9 w-9 shrink-0 rounded-[11px] shadow-q-sm"
        />
        <div className="min-w-0" data-tauri-drag-region>
          <p className="truncate text-[14px] font-semibold tracking-tight text-q-text-primary">
            AIQuotaMonitor
          </p>
          <p className="truncate text-[11px] leading-4 text-q-text-muted">多平台额度监控中心</p>
        </div>
      </div>

      <div className="flex flex-1 flex-col gap-1 px-1">
        {NAV_ITEMS.map((item) => {
          const Icon = item.icon;
          const selected = item.id === active;
          return (
            <button
              key={item.id}
              type="button"
              onClick={() => onSelect(item.id)}
              aria-current={selected ? "page" : undefined}
              title={item.description}
              className={cn(
                "group flex cursor-pointer items-center gap-3 rounded-q-control px-3 py-2.5 text-left transition-colors duration-150",
                selected
                  ? "bg-gradient-to-r from-[#0a66ff] to-[#3b82f6] text-white shadow-[0_6px_16px_rgba(10,102,255,0.32)]"
                  : "text-q-text-secondary hover:bg-q-surface-hover hover:text-q-text-primary",
              )}
            >
              <Icon
                size={18}
                aria-hidden
                className={selected ? "" : "text-q-text-muted group-hover:text-q-primary"}
              />
              <span className="flex-1 truncate text-sm font-medium">{item.label}</span>
            </button>
          );
        })}
      </div>

      <div className="flex flex-col gap-2 px-1 pb-1">
        <div className="rounded-q-control border border-q-border bg-q-surface-muted px-3 py-2.5">
          <div className="flex items-center gap-2">
            <span
              aria-hidden
              className={cn(
                "h-1.5 w-1.5 shrink-0 rounded-full",
                connected.length === 0
                  ? "bg-q-neutral"
                  : attention > 0
                    ? "bg-q-warning"
                    : "bg-q-success",
              )}
            />
            <p className="truncate text-[12px] font-medium text-q-text-primary">
              {connected.length === 0
                ? "尚未接入平台"
                : attention > 0
                  ? `${attention} 个平台需要处理`
                  : "所有服务正常"}
            </p>
          </div>
          <p className="mt-1 text-[11px] leading-4 text-q-text-muted">
            已接入 {connected.length} / {platforms.length} · v0.1.0
          </p>
        </div>
      </div>
    </nav>
  );
}
