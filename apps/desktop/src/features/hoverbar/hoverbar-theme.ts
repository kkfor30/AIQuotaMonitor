/**
 * 悬浮详情主题偏好。
 *
 * 迁移来源：DeepSeek-Monitor-Windows/DeepSeekMonitorWindows
 * src/theme-preference.ts 与 src/main.tsx/useUiTheme（提交 af6cfe07，MIT）。
 * 当前项目尚无全局主题设置，因此仅在悬浮详情作用域内使用 localStorage 持久化。
 */
import { useCallback, useEffect, useState } from "react";

export type HoverbarTheme = "light" | "dark";

const HOVERBAR_THEME_KEY = "ai-quota-monitor:hoverbar-theme";

function loadHoverbarTheme(): HoverbarTheme {
  try {
    const saved = window.localStorage.getItem(HOVERBAR_THEME_KEY);
    if (saved === "light" || saved === "dark") return saved;
  } catch {
    // WebView 禁止本地存储时继续使用系统偏好，不影响详情打开。
  }
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export function useHoverbarTheme() {
  const [theme, setTheme] = useState<HoverbarTheme>(loadHoverbarTheme);

  useEffect(() => {
    document.documentElement.dataset.hoverbarTheme = theme;
    try {
      window.localStorage.setItem(HOVERBAR_THEME_KEY, theme);
    } catch {
      // 持久化失败不回退或禁用主题切换，只保留本次窗口状态。
    }
  }, [theme]);

  const toggleTheme = useCallback(() => {
    setTheme((current) => (current === "dark" ? "light" : "dark"));
  }, []);

  return { theme, toggleTheme };
}
