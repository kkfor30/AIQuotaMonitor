import { SidebarNavigation } from "@/features/navigation/SidebarNavigation";
import { WindowTitleBar } from "@/components/ui/WindowTitleBar";
import type { NavId } from "@/app/navigation";

/**
 * 应用外壳：自定义标题栏 + 左侧一级导航 + 页面内容区。
 * 默认进入「平台中心」。
 */
export function AppShell({
  active,
  onNavigate,
  children,
}: {
  active: NavId;
  onNavigate: (id: NavId) => void;
  children: React.ReactNode;
}) {
  return (
    <div className="app-backdrop flex h-full w-full flex-col">
      <WindowTitleBar />
      <div className="flex min-h-0 flex-1">
        <SidebarNavigation active={active} onSelect={onNavigate} />
        <div className="flex min-h-0 min-w-0 flex-1 flex-col p-4 pl-0">{children}</div>
      </div>
    </div>
  );
}
