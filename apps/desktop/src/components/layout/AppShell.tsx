import { SidebarNavigation } from "@/features/navigation/SidebarNavigation";
import { WindowTitleBar } from "@/components/ui/WindowTitleBar";
import { WindowResizeHandles } from "@/components/ui/WindowResizeHandles";
import { ToastContainer } from "@/components/ui/Toast";
import type { NavId } from "@/app/navigation";

/**
 * 应用外壳（Apple Glass V6）：自定义标题栏 + 玻璃导航列 + 页面内容区。
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
      <WindowResizeHandles />
      <WindowTitleBar />
      <div className="flex min-h-0 flex-1">
        <SidebarNavigation active={active} onSelect={onNavigate} />
        <div className="app-content flex min-h-0 min-w-0 flex-1 flex-col">{children}</div>
      </div>
      <ToastContainer />
    </div>
  );
}
