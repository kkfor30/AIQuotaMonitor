import { useEffect } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { emit, listen } from "@tauri-apps/api/event";
import { fetchAppSettings } from "./ipc";
import { APP_SETTINGS_QUERY_KEY } from "./query-client";

/** 跨窗口主题同步事件：任一窗口切换主题后广播，其余窗口实时跟随。 */
export const APP_THEME_EVENT = "app-theme-changed";

export function applyAppTheme(theme: string | undefined, broadcast = false) {
  const root = document.documentElement;
  // 主题只有浅色/深色两档，不再支持跟随系统：非 dark（含旧值 "system"）一律归浅色。
  const resolved = theme === "dark" ? "dark" : "light";
  root.dataset.theme = resolved;
  root.dataset.hoverbarTheme = resolved;
  if (broadcast) {
    void emit(APP_THEME_EVENT, { theme: resolved }).catch(() => undefined);
  }
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

  // 其他窗口（悬浮球等）切换主题时实时跟随，并刷新设置缓存让设置页/标题栏同步。
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
