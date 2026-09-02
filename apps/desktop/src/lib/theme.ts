import { useEffect } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { fetchAppSettings } from "./ipc";
import { APP_SETTINGS_QUERY_KEY } from "./query-client";

/** 后端在 set_app_theme 落库后向所有窗口广播的主题变更事件。 */
export const APP_THEME_EVENT = "app-theme-changed";

export function applyAppTheme(theme: string | undefined) {
  const root = document.documentElement;
  // 主题只有浅色/深色两档，不再支持跟随系统：非 dark（含旧值 "system"）一律归浅色。
  const resolved = theme === "dark" ? "dark" : "light";
  root.dataset.theme = resolved;
  root.dataset.hoverbarTheme = resolved;
}

export function useAppTheme() {
  const queryClient = useQueryClient();
  const { data } = useQuery({
    queryKey: APP_SETTINGS_QUERY_KEY,
    queryFn: fetchAppSettings,
    retry: false,
  });

  useEffect(() => {
    applyAppTheme(data?.theme ?? "light");
  }, [data?.theme]);

  // 任一窗口（主窗口标题栏/设置页/悬浮球）切换主题：后端落库并广播，本窗口实时跟随，
  // 并刷新设置缓存让设置页/标题栏的选中项同步。
  useEffect(() => {
    const unlisten = listen<{ theme?: string }>(APP_THEME_EVENT, (event) => {
      applyAppTheme(event.payload?.theme);
      void queryClient.invalidateQueries({ queryKey: APP_SETTINGS_QUERY_KEY });
    });
    return () => {
      void unlisten.then((off) => off());
    };
  }, [queryClient]);
}
