/**
 * 应用入口：按 Rust 注入的窗口标记分发视图。
 * - window.__HOVERBAR_ANCHOR__：悬浮球 40x40 锚点窗口
 * - window.__HOVERBAR_DETAIL__：悬浮详情窗口
 * - 其余：主窗口
 *
 * 迁移来源：DeepSeekMonitorWindows-final/src/main.tsx 的 App 入口分发
 * （提交 f3ab3ec6，MIT）。主窗口与悬浮球 UI 均为新实现，不迁移旧组件树。
 */
import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";
import { AppRoot } from "@/app/AppRoot";
import { HoverbarAnchorApp } from "@/features/hoverbar/HoverbarAnchorApp";
import { HoverbarDetailApp } from "@/features/hoverbar/HoverbarDetailApp";
import { createQueryClient } from "@/lib/query-client";
import "@/styles/global.css";

declare global {
  interface Window {
    __HOVERBAR_ANCHOR__?: boolean;
    __HOVERBAR_DETAIL__?: boolean;
  }
}

const isHoverbarAnchor = Boolean(window.__HOVERBAR_ANCHOR__);
const isHoverbarDetail = Boolean(window.__HOVERBAR_DETAIL__);

if (isHoverbarAnchor || isHoverbarDetail) {
  document.documentElement.classList.add("hoverbar-window");
}

const queryClient = createQueryClient();

function RootApp() {
  if (isHoverbarAnchor) return <HoverbarAnchorApp />;
  if (isHoverbarDetail) return <HoverbarDetailApp />;
  return <AppRoot />;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <RootApp />
    </QueryClientProvider>
  </React.StrictMode>,
);
