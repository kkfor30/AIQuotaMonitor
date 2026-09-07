import { useQuery } from "@tanstack/react-query";
import { NAV_ITEMS, type NavId } from "@/app/navigation";
import { cn } from "@/lib/cn";
import { fetchPlatformSummaries } from "@/lib/ipc";
import { PLATFORM_SUMMARIES_QUERY_KEY } from "@/lib/query-client";

/**
 * 左侧一级导航（Apple Glass V6 设计稿 01）：
 * 玻璃列 + 新应用 Logo；选中项为浅蓝底 + 左侧蓝色竖条 + 蓝字；
 * 底部展示由真实平台快照推导的监控状态卡与版本信息。
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
      <div className="flex items-center gap-3 px-2 pb-3 pt-2" data-tauri-drag-region>
        <img
          src="/assets/brand/logo-aiquota-liquid-glass.png"
          alt=""
          draggable={false}
          className="h-10 w-10 shrink-0 rounded-[12px] shadow-q-sm"
        />
        <div className="min-w-0" data-tauri-drag-region>
          <p className="truncate text-[15px] font-bold tracking-tight text-q-text-primary">
            AIQuotaMonitor
          </p>
          <p className="truncate text-[11px] leading-4 text-q-text-muted">多平台额度监控中心</p>
        </div>
      </div>

      <div className="flex flex-1 flex-col gap-1">
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
                "group relative flex cursor-pointer items-center gap-3 rounded-q-control py-2.5 pl-4 pr-3 text-left transition-colors duration-150",
                selected
                  ? "bg-q-primary-soft text-q-primary"
                  : "text-q-text-secondary hover:bg-q-surface-hover hover:text-q-text-primary",
              )}
            >
              {selected && (
                <span
                  aria-hidden
                  className="absolute left-0 top-1/2 h-[18px] w-[3px] -translate-y-1/2 rounded-full bg-q-primary"
                />
              )}
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

      {/* 监控状态卡：真实平台聚合推导；材质由 Token 驱动，深浅主题各自成调 */}
      <div className="sidebar-status-card mx-1 mb-2 overflow-hidden rounded-[14px] border border-q-border p-3">
        <div className="flex items-center gap-2">
          <span
            aria-hidden
            className={cn(
              "relative flex h-2 w-2 shrink-0",
              attention > 0 ? "text-q-warning" : "text-q-success",
            )}
          >
            <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-current opacity-40" />
            <span className="relative inline-flex h-2 w-2 rounded-full bg-current" />
          </span>
          <p className="text-[12px] font-semibold text-q-text-primary">监控运行中</p>
        </div>
        <p className="mt-1 text-[11px] leading-4 text-q-text-secondary">
          {connected.length === 0
            ? "尚未接入平台"
            : attention > 0
              ? `${attention} 个平台需要处理`
              : "所有服务正常"}
        </p>
        {/* 迷你柱状装饰：纯装饰性数据图形 */}
        <div aria-hidden className="mt-2.5 flex h-7 items-end gap-1">
          {[9, 14, 11, 18, 24, 16, 21].map((height, index) => (
            <span
              key={index}
              className="sidebar-status-bar w-[7px] rounded-[3px] opacity-80"
              style={{ height }}
            />
          ))}
        </div>
      </div>

      <p className="px-2 pb-1 text-center text-[10px] leading-4 text-q-text-muted">
        数据仅保存在本机 · v0.1.1
      </p>
    </nav>
  );
}
