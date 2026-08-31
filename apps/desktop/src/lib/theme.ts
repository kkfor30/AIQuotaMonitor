import { useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { fetchAppSettings } from "./ipc";
import { APP_SETTINGS_QUERY_KEY } from "./query-client";

export function applyAppTheme(theme: string | undefined) {
  const root = document.documentElement;
  // 主题只有浅色/深色两档，不再支持跟随系统：非 dark（含旧值 "system"）一律归浅色。
  const resolved = theme === "dark" ? "dark" : "light";
  root.dataset.theme = resolved;
  root.dataset.hoverbarTheme = resolved;
}

export function useAppTheme() {
  const { data } = useQuery({
    queryKey: APP_SETTINGS_QUERY_KEY,
    queryFn: fetchAppSettings,
    retry: false,
  });

  useEffect(() => {
    applyAppTheme(data?.theme ?? "light");
  }, [data?.theme]);
}
