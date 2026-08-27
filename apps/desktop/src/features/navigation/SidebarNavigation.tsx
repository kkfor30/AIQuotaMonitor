import { NAV_ITEMS, type NavId } from "@/app/navigation";
import { cn } from "@/lib/cn";

/**
 * 左侧一级导航：Logo 区 + 四个导航项 + 底部版本信息。
 * 选中项使用主蓝色高亮背景；状态表达同时含图标与文字。
 */
export function SidebarNavigation({
  active,
  onSelect,
}: {
  active: NavId;
  onSelect: (id: NavId) => void;
}) {
  return (
    <nav className="flex h-full w-[220px] shrink-0 flex-col border-r border-q-border bg-q-surface-muted backdrop-blur-xl">
      {/* Logo 区（同时作为窗口拖动区域的一部分） */}
      <div className="flex items-center gap-2.5 px-5 pb-2 pt-2.5" data-tauri-drag-region>
        <div className="flex h-8 w-8 items-center justify-center rounded-[10px] bg-gradient-to-br from-q-primary to-[#3188fd] shadow-q-sm">
          <span className="text-[15px] font-bold leading-none text-white">Q</span>
        </div>
        <div className="min-w-0">
          <p className="truncate text-[15px] font-semibold tracking-tight text-q-text-primary">
            AIQuotaMonitor
          </p>
          <p className="truncate text-[11px] text-q-text-muted">多平台额度监控中心</p>
        </div>
      </div>

      <div className="mt-3 flex flex-1 flex-col gap-1 px-3">
        {NAV_ITEMS.map((item) => {
          const Icon = item.icon;
          const selected = item.id === active;
          return (
            <button
              key={item.id}
              type="button"
              onClick={() => onSelect(item.id)}
              aria-current={selected ? "page" : undefined}
              className={cn(
                "group flex cursor-pointer items-center gap-3 rounded-q-control px-3 py-2.5 text-left transition-colors duration-150",
                selected
                  ? "bg-q-primary text-white shadow-q-sm"
                  : "text-q-text-secondary hover:bg-q-surface-hover hover:text-q-text-primary",
              )}
            >
              <Icon size={18} aria-hidden className={selected ? "" : "text-q-text-muted group-hover:text-q-primary"} />
              <span className="flex-1 truncate text-sm font-medium">{item.label}</span>
            </button>
          );
        })}
      </div>

      <div className="px-5 py-4">
        <div className="rounded-q-control border border-q-border bg-q-surface px-3 py-2.5">
          <p className="text-[11px] leading-relaxed text-q-text-muted">
            阶段一 · 静态数据验证
          </p>
          <p className="text-[11px] leading-relaxed text-q-text-muted">v0.1.0</p>
        </div>
      </div>
    </nav>
  );
}
