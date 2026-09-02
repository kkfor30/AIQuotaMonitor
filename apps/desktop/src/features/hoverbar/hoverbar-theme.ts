/**
 * 悬浮详情主题偏好。
 *
 * 迁移来源：DeepSeek-Monitor-Windows/DeepSeekMonitorWindows
 * src/theme-preference.ts 与 src/main.tsx/useUiTheme（提交 afc6fe07，MIT）。
 * 设置页的浅色/深色会覆盖本地偏好；详情内按钮可临时切换，两者只取浅色/深色两档，
 * 不再支持跟随系统。切换会落库并通过 APP_THEME_EVENT 广播，主窗口实时跟随；
 * 其他窗口切换时本悬浮窗也通过同一事件实时跟随。
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { fetchAppSettings, setAppTheme } from "@/lib/ipc";
import { APP_THEME_EVENT } from "@/lib/theme";

export type HoverbarTheme = "light" | "dark";

const HOVERBAR_THEME_KEY = "ai-quota-monitor:hoverbar-theme";

function loadHoverbarTheme(): HoverbarTheme {
  try {
    const saved = window.localStorage.getItem(HOVERBAR_THEME_KEY);
    if (saved === "light" || saved === "dark") return saved;
  } catch {
    // WebView 禁止本地存储时继续使用默认浅色，不影响详情打开。
  }
  return "light";
}

export function useHoverbarTheme() {
  const [theme, setTheme] = useState<HoverbarTheme>(loadHoverbarTheme);
  // 用户手动切换后，挂载期的设置回读不得覆盖本地选择（否则首次点击会被异步回读抵消）。
  const userAdjustedRef = useRef(false);

  useEffect(() => {
    let cancelled = false;
    void fetchAppSettings()
      .then((settings) => {
        if (cancelled || userAdjustedRef.current) return;
        if (settings.theme === "light" || settings.theme === "dark") {
          setTheme(settings.theme);
        }
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    document.documentElement.dataset.hoverbarTheme = theme;
    try {
      window.localStorage.setItem(HOVERBAR_THEME_KEY, theme);
    } catch {
      // 持久化失败不回退或禁用主题切换，只保留本次窗口状态。
    }
    // 仅用户手动切换时落库并广播；响应其他窗口广播的变更不再转发，避免回环。
    if (userAdjustedRef.current) {
      void setAppTheme(theme)
        .then(() => emit(APP_THEME_EVENT, { theme }))
        .catch(() => undefined);
    }
  }, [theme]);

  // 主窗口标题栏/设置页切换主题时，本悬浮窗实时跟随。
  useEffect(() => {
    const unlisten = listen<{ theme?: string }>(APP_THEME_EVENT, (event) => {
      const next = event.payload?.theme;
      if (next === "light" || next === "dark") setTheme(next);
    });
    return () => {
      void unlisten.then((off) => off());
    };
  }, []);

  const toggleTheme = useCallback(() => {
    userAdjustedRef.current = true;
    setTheme((current) => (current === "dark" ? "light" : "dark"));
  }, []);

  return { theme, toggleTheme };
}
