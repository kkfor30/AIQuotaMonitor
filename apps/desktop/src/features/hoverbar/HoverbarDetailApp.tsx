/**
 * 悬浮详情窗口应用（独立 hoverbar-detail 窗口）。
 *
 * 迁移来源：DeepSeek-Monitor-Windows/DeepSeekMonitorWindows/src/main.tsx 的
 * HoverbarDetail / HoverbarDetailApp（提交 af6cfe07，MIT，约 640-972 行）。
 * 迁移内容：detail-open/detail-close 事件驱动的开合动画状态机、
 * ResizeObserver 内容测高 → set_hoverbar_detail_size 自适应窗口、
 * 指针进出上报 set_hoverbar_detail_pointer_inside。
 * 变更：数据改为当前 Source/Capability 脱敏 ViewModel；卡片按最终稿重排；
 * 增加 GPT 重置信号摘要条与同窗口内的雷达二级页切换。
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ExternalLink, Moon, RefreshCw, SunMedium, X } from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import {
  fetchAppSettings,
  fetchPlatformSummaries,
  fetchRadarSnapshot,
  ipcErrorMessage,
  openMainWindow,
  refreshAllPlatforms,
  runRadarCheck,
} from "@/lib/ipc";
import {
  APP_SETTINGS_QUERY_KEY,
  PLATFORM_SUMMARIES_QUERY_KEY,
  RADAR_SNAPSHOT_QUERY_KEY,
} from "@/lib/query-client";
import { HoverbarPlatformCard } from "./HoverbarPlatformCard";
import { HoverbarRadarDetail } from "./HoverbarRadarDetail";
import { useHoverbarTheme } from "./hoverbar-theme";
import {
  DEFAULT_HOVERBAR_PROVIDER_ORDER,
  filterHoverbarPlatforms,
  HOVERBAR_EXIT_ANIMATION_MS,
  measureHoverbar,
  normalizeHoverbarAnchor,
  sortHoverbarPlatforms,
  summarizeHoverbarStatus,
  type HoverbarAnchor,
  type HoverbarMotionPhase,
} from "./hoverbar-state";

type HoverbarView = "quota" | "radar";

export function HoverbarDetailApp() {
  const queryClient = useQueryClient();
  const { theme, toggleTheme } = useHoverbarTheme();
  const [motionPhase, setMotionPhase] = useState<HoverbarMotionPhase>("anchor");
  const [anchor, setAnchor] = useState<HoverbarAnchor>({ edge: "right", ratio: 0.4 });
  const [view, setView] = useState<HoverbarView>("quota");
  const [contentHeight, setContentHeight] = useState(0);
  const motionPhaseRef = useRef<HoverbarMotionPhase>("anchor");
  const exitTimer = useRef<number | undefined>(undefined);
  const panelRef = useRef<HTMLDivElement>(null);
  const headerRef = useRef<HTMLElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);

  const { data: platforms = [], isFetching } = useQuery({
    queryKey: PLATFORM_SUMMARIES_QUERY_KEY,
    queryFn: fetchPlatformSummaries,
  });
  const refreshPlatforms = useMutation({
    mutationFn: refreshAllPlatforms,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: PLATFORM_SUMMARIES_QUERY_KEY });
    },
  });
  const platformsRefreshing = refreshPlatforms.isPending || isFetching;
  const { data: settings } = useQuery({
    queryKey: APP_SETTINGS_QUERY_KEY,
    queryFn: fetchAppSettings,
    retry: false,
  });
  // GPT 重置雷达：详情页可主动同步；若已开启 AI 辅助分析，刷新时一并重跑。
  const { data: radar, refetch: refetchRadar } = useQuery({
    queryKey: RADAR_SNAPSHOT_QUERY_KEY,
    queryFn: fetchRadarSnapshot,
    retry: false,
  });
  const radarCheck = useMutation({
    mutationFn: () => {
      const readyModel =
        radar?.models.find((item) => item.sourceId === radar.analysisPrefs.sourceId && item.ready) ??
        radar?.models.find((item) => item.ready);
      const analyze = Boolean(radar?.analysisPrefs.analyze && readyModel);
      return runRadarCheck({
        analyze,
        rangeKey: radar?.analysisPrefs.rangeKey || "3d",
        sourceId: readyModel?.sourceId ?? radar?.analysisPrefs.sourceId ?? null,
        model: readyModel?.model ?? null,
      });
    },
    onSuccess: (snapshot) => queryClient.setQueryData(RADAR_SNAPSHOT_QUERY_KEY, snapshot),
  });
  const refreshRadar = useCallback(() => {
    if (!radarCheck.isPending) radarCheck.mutate();
  }, [radarCheck]);
  const radarRefreshError = radarCheck.error
    ? ipcErrorMessage(radarCheck.error, "重置信号刷新失败")
    : null;

  const setMotion = useCallback((phase: HoverbarMotionPhase) => {
    motionPhaseRef.current = phase;
    setMotionPhase(phase);
  }, []);

  const finishClose = useCallback(() => {
    window.clearTimeout(exitTimer.current);
    if (motionPhaseRef.current === "anchor") return;
    setMotion("closing");
    exitTimer.current = window.setTimeout(() => {
      void invoke("finish_hide_hoverbar_detail").finally(() => setMotion("anchor"));
    }, HOVERBAR_EXIT_ANIMATION_MS);
  }, [setMotion]);

  // 打开/收起事件驱动动画状态机；每次重新展开都回到额度列表页
  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    void listen<HoverbarAnchor>("hoverbar-detail-open", (event) => {
      if (disposed) return;
      window.clearTimeout(exitTimer.current);
      setView("quota");
      setAnchor(normalizeHoverbarAnchor(event.payload));
      void refetchRadar();
      setMotion("opening");
      if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
        setMotion("visible");
        return;
      }
      // 双 rAF 确保初始样式已提交后再切换到可见态，动画才能生效
      window.requestAnimationFrame(() => {
        window.requestAnimationFrame(() => {
          if (!disposed && motionPhaseRef.current === "opening") setMotion("visible");
        });
      });
    }).then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
    void listen("hoverbar-detail-close", () => {
      if (!disposed) finishClose();
    }).then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
    return () => {
      disposed = true;
      window.clearTimeout(exitTimer.current);
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [finishClose, refetchRadar, setMotion]);

  // 内容测高：观察头部与内容盒，额度列表与雷达二级页共用同一滚动容器
  useLayoutEffect(() => {
    const updateContentHeight = () => {
      const panel = panelRef.current;
      const header = headerRef.current;
      const content = contentRef.current;
      if (!panel || !header || !content) return;
      const style = window.getComputedStyle(panel);
      const padding =
        (parseFloat(style.paddingTop) || 0) + (parseFloat(style.paddingBottom) || 0);
      const nextHeight = Math.ceil(
        header.getBoundingClientRect().height + content.getBoundingClientRect().height + padding + 2,
      );
      setContentHeight((previous) => (previous === nextHeight ? previous : nextHeight));
    };
    updateContentHeight();
    const frame = window.requestAnimationFrame(updateContentHeight);
    const observer = new ResizeObserver(updateContentHeight);
    if (headerRef.current) observer.observe(headerRef.current);
    if (contentRef.current) observer.observe(contentRef.current);
    return () => {
      window.cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, [view]);

  // 上报期望尺寸，后端 clamp 并重排窗口
  useEffect(() => {
    if (contentHeight <= 0) return;
    const { width, height } = measureHoverbar(anchor.edge, "detail", contentHeight);
    void invoke<HoverbarAnchor>("set_hoverbar_detail_size", { width, height })
      .then((next) => setAnchor(normalizeHoverbarAnchor(next)))
      .catch((error) => console.error("无法调整悬浮详情尺寸", error));
  }, [anchor.edge, contentHeight]);

  const providerOrder =
    platforms.length > 0 ? platforms.map((platform) => platform.providerId) : DEFAULT_HOVERBAR_PROVIDER_ORDER;
  const orderedPlatforms = sortHoverbarPlatforms(
    filterHoverbarPlatforms(platforms),
    providerOrder,
    settings?.hoverbarSortMode === "smart" ? "smart" : "manual",
  );
  const latestUpdate = platforms
    .flatMap((p) => p.capabilities.map((c) => c.capturedAt ?? 0))
    .reduce<number>((max, at) => Math.max(max, at), 0);
  const statusText = summarizeHoverbarStatus(platforms, latestUpdate || null);

  return (
    <div
      className="hb-detail-root h-full w-full"
      data-edge={anchor.edge}
      data-motion={motionPhase}
      onMouseEnter={() => void invoke("set_hoverbar_detail_pointer_inside", { inside: true })}
      onMouseLeave={() => void invoke("set_hoverbar_detail_pointer_inside", { inside: false })}
    >
      <section ref={panelRef} className="hb-panel">
        <header ref={headerRef} className="hb-head">
          <p
            className="hb-refresh-status"
            data-error={Boolean(refreshPlatforms.error) || statusText.includes("失败") || undefined}
          >
            {refreshPlatforms.error
              ? ipcErrorMessage(refreshPlatforms.error, "刷新平台失败")
              : platformsRefreshing
                ? "正在刷新平台额度…"
                : statusText}
          </p>
          <div className="hb-actions">
            <DetailIconButton
              label={platformsRefreshing ? "正在刷新" : "刷新平台额度"}
              title={platformsRefreshing ? "正在刷新各平台额度" : "重新拉取各平台额度"}
              onClick={() => {
                if (!refreshPlatforms.isPending) refreshPlatforms.mutate();
              }}
              disabled={platformsRefreshing}
              loading={platformsRefreshing}
            >
              <RefreshCw size={16} aria-hidden />
            </DetailIconButton>
            <DetailIconButton
              label={theme === "dark" ? "切换到浅色主题" : "切换到深色主题"}
              title={theme === "dark" ? "切换到浅色主题" : "切换到深色主题"}
              onClick={toggleTheme}
            >
              {theme === "dark" ? (
                <SunMedium size={16} aria-hidden />
              ) : (
                <Moon size={16} aria-hidden />
              )}
            </DetailIconButton>
            <DetailIconButton label="打开主窗口" title="打开主窗口" onClick={() => void openMainWindow()}>
              <ExternalLink size={16} aria-hidden />
            </DetailIconButton>
            <DetailIconButton label="收起详情" title="收起详情" onClick={finishClose}>
              <X size={17} aria-hidden />
            </DetailIconButton>
          </div>
        </header>

        <div className="hb-service-list">
          <div ref={contentRef} className="hb-service-scroll">
            {view === "radar" ? (
              <HoverbarRadarDetail
                radar={radar}
                onBack={() => setView("quota")}
                onRefresh={refreshRadar}
                refreshing={radarCheck.isPending}
                refreshError={radarRefreshError}
              />
            ) : orderedPlatforms.length === 0 ? (
              <div className="hb-empty">
                <strong>暂无可展示额度</strong>
                <span>请在主窗口的平台中心完成接入</span>
              </div>
            ) : (
              orderedPlatforms.map((platform) => (
                <HoverbarPlatformCard
                  key={platform.providerId}
                  platform={platform}
                  radar={platform.providerId === "openai" ? radar : undefined}
                  onOpenRadar={platform.providerId === "openai" ? () => setView("radar") : undefined}
                  onRefreshRadar={platform.providerId === "openai" ? refreshRadar : undefined}
                  radarRefreshing={platform.providerId === "openai" ? radarCheck.isPending : false}
                  radarRefreshError={platform.providerId === "openai" ? radarRefreshError : null}
                />
              ))
            )}
          </div>
        </div>

      </section>
    </div>
  );
}

function DetailIconButton({
  label,
  title,
  onClick,
  disabled = false,
  loading = false,
  children,
}: {
  label: string;
  title: string;
  onClick: () => void;
  disabled?: boolean;
  loading?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={title}
      onClick={onClick}
      disabled={disabled}
      className="hb-action-button"
      data-loading={loading || undefined}
    >
      {children}
    </button>
  );
}
