import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { AppShell } from "@/components/layout/AppShell";
import { OverviewPage } from "@/features/overview/OverviewPage";
import { PlatformCenterPage } from "@/features/platform-center/PlatformCenterPage";
import { GptRadarPage } from "@/features/radar/GptRadarPage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { ErrorBoundary } from "@/components/ui/ErrorBoundary";
import type { NavId, PlatformCenterTarget } from "@/app/navigation";

/**
 * 主窗口根组件：一级导航切换，默认进入平台中心。
 * 总览关注项与悬浮窗深链点击后携带 platformId/sourceId 定位到平台中心（v5 交互 2）。
 */
export function AppRoot() {
  const [nav, setNav] = useState<NavId>("platform-center");
  const [platformTarget, setPlatformTarget] = useState<PlatformCenterTarget | null>(null);

  const openPlatform = useCallback((target: PlatformCenterTarget) => {
    setPlatformTarget((previous) =>
      // 同一目标重复点击也重新触发定位
      previous?.providerId === target.providerId &&
      previous?.tab === target.tab &&
      previous?.focusSourceId === target.focusSourceId
        ? { ...target }
        : target,
    );
    setNav("platform-center");
  }, []);

  const consumeTarget = useCallback(() => setPlatformTarget(null), []);

  // 接收来自悬浮详情窗口或外部的平台深链跳转指令
  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    void listen<PlatformCenterTarget>("navigate-platform-target", (event) => {
      if (disposed || !event.payload) return;
      openPlatform(event.payload);
    }).then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
    // 悬浮详情「完整雷达」入口：打开主窗口后跳转雷达页
    void listen("navigate-radar", () => {
      if (!disposed) setNav("gpt-radar");
    }).then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [openPlatform]);

  return (
    <ErrorBoundary variant="page" title="应用主界面遇到问题">
      <AppShell active={nav} onNavigate={setNav}>
        {nav === "overview" && (
          <ErrorBoundary variant="page" title="总览页面遇到问题">
            <OverviewPage onOpenPlatform={openPlatform} />
          </ErrorBoundary>
        )}
        {nav === "platform-center" && (
          <ErrorBoundary variant="page" title="平台中心遇到问题">
            <PlatformCenterPage target={platformTarget} onTargetConsumed={consumeTarget} />
          </ErrorBoundary>
        )}
        {nav === "gpt-radar" && (
          <ErrorBoundary variant="page" title="重置雷达遇到问题">
            <GptRadarPage />
          </ErrorBoundary>
        )}
        {nav === "settings" && (
          <ErrorBoundary variant="page" title="设置页面遇到问题">
            <SettingsPage />
          </ErrorBoundary>
        )}
      </AppShell>
    </ErrorBoundary>
  );
}
