import { useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { fetchAppSettings } from "./ipc";
import { APP_SETTINGS_QUERY_KEY } from "./query-client";

export function applyAppTheme(theme: string | undefined) {
  const root = document.documentElement;
  if (theme === "light" || theme === "dark") {
    root.dataset.theme = theme;
    root.dataset.hoverbarTheme = theme;
    return;
  }
  delete root.dataset.theme;
  delete root.dataset.hoverbarTheme;
}

export function useAppTheme() {
  const { data } = useQuery({
    queryKey: APP_SETTINGS_QUERY_KEY,
    queryFn: fetchAppSettings,
    retry: false,
  });

  useEffect(() => {
    applyAppTheme(data?.theme ?? "system");
  }, [data?.theme]);
}
