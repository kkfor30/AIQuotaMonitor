import { Minus, Moon, Square, SunMedium, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { fetchAppSettings, setAppTheme } from "@/lib/ipc";
import { APP_SETTINGS_QUERY_KEY } from "@/lib/query-client";
import { applyAppTheme } from "@/lib/theme";
import { cn } from "@/lib/cn";

/**
 * 自定义窗口标题栏（无边框主窗口）。
 * data-tauri-drag-region 提供原生拖动与双击最大化；
 * 窗口控制按钮左侧提供主题（换肤）快捷入口，与设置页共用同一份主题设置。
 */
export function WindowTitleBar() {
  const [maximized, setMaximized] = useState(false);
  const appWindow = getCurrentWindow();

  useEffect(() => {
    let disposed = false;
    const unlisten = appWindow.onResized(async () => {
      const next = await appWindow.isMaximized().catch(() => false);
      if (!disposed) setMaximized(next);
    });
    void appWindow
      .isMaximized()
      .then((next) => {
        if (!disposed) setMaximized(next);
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      void unlisten.then((fn) => fn());
    };
  }, [appWindow]);

  const minimize = useCallback(() => void appWindow.minimize(), [appWindow]);
  const toggleMaximize = useCallback(
    () => void appWindow.toggleMaximize(),
    [appWindow],
  );
  const close = useCallback(() => void appWindow.close(), [appWindow]);

  return (
    <div className="flex h-10 shrink-0 items-center justify-end pr-2" data-tauri-drag-region>
      <div className="flex items-center gap-0.5">
        <ThemeSwitchButton />
        <span aria-hidden className="mx-1 h-4 w-px bg-q-border" />
        <TitleBarButton label="最小化" onClick={minimize}>
          <Minus size={14} />
        </TitleBarButton>
        <TitleBarButton label={maximized ? "还原" : "最大化"} onClick={toggleMaximize}>
          <Square size={12} />
        </TitleBarButton>
        <TitleBarButton label="关闭" onClick={close} danger>
          <X size={15} />
        </TitleBarButton>
      </div>
    </div>
  );
}

/**
 * 标题栏换肤按钮：单击在浅色 / 深色之间切换（与悬浮详情同款），写入同一份主题设置。
 * 不再提供「跟随系统」。
 */
function ThemeSwitchButton() {
  const queryClient = useQueryClient();
  const { data: settings } = useQuery({
    queryKey: APP_SETTINGS_QUERY_KEY,
    queryFn: fetchAppSettings,
  });

  const theme = settings?.theme === "dark" ? "dark" : "light";

  const themeMutation = useMutation({
    mutationFn: setAppTheme,
    onSuccess: (next) => {
      applyAppTheme(next.theme, true);
      queryClient.setQueryData(APP_SETTINGS_QUERY_KEY, next);
    },
  });

  // 深色时显示太阳（点击回到浅色）；浅色时显示月亮（点击进入深色）
  return (
    <button
      type="button"
      aria-label={theme === "dark" ? "切换到浅色主题" : "切换到深色主题"}
      title={theme === "dark" ? "切换到浅色主题" : "切换到深色主题"}
      onClick={() => {
        if (themeMutation.isPending) return;
        themeMutation.mutate(theme === "dark" ? "light" : "dark");
      }}
      className={cn(
        "inline-flex h-7 w-10 cursor-pointer items-center justify-center rounded-md text-q-text-secondary transition-colors duration-100",
        "hover:bg-q-border hover:text-q-text-primary",
      )}
    >
      {theme === "dark" ? <SunMedium size={14} /> : <Moon size={14} />}
    </button>
  );
}

function TitleBarButton({
  label,
  onClick,
  danger = false,
  children,
}: {
  label: string;
  onClick: () => void;
  danger?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={`inline-flex h-7 w-10 cursor-pointer items-center justify-center rounded-md text-q-text-secondary transition-colors duration-100 ${
        danger ? "hover:bg-q-danger hover:text-white" : "hover:bg-q-border hover:text-q-text-primary"
      }`}
    >
      {children}
    </button>
  );
}
