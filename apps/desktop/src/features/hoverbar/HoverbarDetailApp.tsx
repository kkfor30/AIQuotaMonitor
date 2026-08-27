/**
 * 悬浮详情窗口应用（独立 hoverbar-detail 窗口）。
 *
 * 迁移来源：DeepSeekMonitorWindows-final/src/main.tsx 的 HoverbarDetailApp
 * （提交 f3ab3ec6，MIT，约 864-972 行）
 * 迁移内容：detail-open/detail-close 事件驱动的开合动画状态机、
 * ResizeObserver 内容测高 → set_hoverbar_detail_size 自适应窗口、
 * 指针进出上报 set_hoverbar_detail_pointer_inside。
 * 变更：UI 按新版设计 Token 重写（不迁移旧 hb-* 样式与主题切换）；
 * 数据改为 TanStack Query 消费与主窗口相同的静态 ViewModel。
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useQuery } from "@tanstack/react-query";
import { ExternalLink, PanelTopClose } from "lucide-react";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { fetchPlatformSummaries, openMainWindow } from "@/lib/ipc";
import { PLATFORM_SUMMARIES_QUERY_KEY } from "@/lib/query-client";
import { HoverbarPlatformCard } from "./HoverbarPlatformCard";
import {
  DEFAULT_HOVERBAR_PROVIDER_ORDER,
  HOVERBAR_EXIT_ANIMATION_MS,
  measureHoverbar,
  normalizeHoverbarAnchor,
  sortHoverbarPlatforms,
  summarizeHoverbarStatus,
  type HoverbarAnchor,
  type HoverbarMotionPhase,
} from "./hoverbar-state";

export function HoverbarDetailApp() {
  const [motionPhase, setMotionPhase] = useState<HoverbarMotionPhase>("anchor");
  const [anchor, setAnchor] = useState<HoverbarAnchor>({ edge: "right", ratio: 0.4 });
  const [contentHeight, setContentHeight] = useState(0);
  const motionPhaseRef = useRef<HoverbarMotionPhase>("anchor");
  const exitTimer = useRef<number | undefined>(undefined);
  const panelRef = useRef<HTMLDivElement>(null);
  const headerRef = useRef<HTMLElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const { data: platforms = [] } = useQuery({
    queryKey: PLATFORM_SUMMARIES_QUERY_KEY,
    queryFn: fetchPlatformSummaries,
  });

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

  // 打开/收起事件驱动动画状态机
  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    void listen<HoverbarAnchor>("hoverbar-detail-open", (event) => {
      if (disposed) return;
      window.clearTimeout(exitTimer.current);
      setAnchor(normalizeHoverbarAnchor(event.payload));
      setMotion("opening");
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
  }, [finishClose, setMotion]);

  // 内容测高：驱动窗口尺寸自适应
  useLayoutEffect(() => {
    const updateContentHeight = () => {
      const panel = panelRef.current;
      const header = headerRef.current;
      const list = listRef.current;
      if (!panel || !header || !list) return;
      const style = window.getComputedStyle(panel);
      const padding =
        (parseFloat(style.paddingTop) || 0) + (parseFloat(style.paddingBottom) || 0);
      const nextHeight = Math.ceil(
        header.getBoundingClientRect().height + list.scrollHeight + padding + 2,
      );
      setContentHeight((previous) => (previous === nextHeight ? previous : nextHeight));
    };
    updateContentHeight();
    const frame = window.requestAnimationFrame(updateContentHeight);
    const observer = new ResizeObserver(updateContentHeight);
    if (headerRef.current) observer.observe(headerRef.current);
    if (listRef.current) observer.observe(listRef.current);
    return () => {
      window.cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, [platforms]);

  // 上报期望尺寸，后端 clamp 并重排窗口
  useEffect(() => {
    if (contentHeight <= 0) return;
    const { width, height } = measureHoverbar(anchor.edge, "detail", contentHeight);
    void invoke<HoverbarAnchor>("set_hoverbar_detail_size", { width, height })
      .then((next) => setAnchor(normalizeHoverbarAnchor(next)))
      .catch((error) => console.error("无法调整悬浮详情尺寸", error));
  }, [anchor.edge, contentHeight]);

  const orderedPlatforms = sortHoverbarPlatforms(
    platforms,
    DEFAULT_HOVERBAR_PROVIDER_ORDER,
    "manual",
  );
  const latestUpdate = platforms
    .flatMap((p) => p.capabilities.map((c) => c.capturedAt ?? 0))
    .reduce<number>((max, at) => Math.max(max, at), 0);
  const statusText = summarizeHoverbarStatus(platforms, latestUpdate || null);

  return (
    <div
      className="hb-detail-root h-full w-full select-none"
      data-edge={anchor.edge}
      data-motion={motionPhase}
      onMouseEnter={() => void invoke("set_hoverbar_detail_pointer_inside", { inside: true })}
      onMouseLeave={() => void invoke("set_hoverbar_detail_pointer_inside", { inside: false })}
    >
      <div ref={panelRef} className="hb-panel glass-panel flex max-h-full flex-col gap-2 p-3">
        <header ref={headerRef} className="flex items-center justify-between gap-2 px-1 pb-1">
          <p className="truncate text-[12px] font-medium text-q-text-secondary">{statusText}</p>
          <div className="flex items-center gap-1">
            <DetailIconButton label="打开主窗口" title="打开主窗口" onClick={() => void openMainWindow()}>
              <ExternalLink size={14} aria-hidden />
            </DetailIconButton>
            <DetailIconButton label="收起详情" title="收起详情" onClick={finishClose}>
              <PanelTopClose size={15} aria-hidden />
            </DetailIconButton>
          </div>
        </header>

        <div ref={listRef} className="hb-list flex flex-col gap-2 overflow-y-auto">
          {orderedPlatforms.length === 0 ? (
            <div className="rounded-q-control border border-dashed border-q-border-strong px-3 py-4 text-center text-[12px] text-q-text-muted">
              暂无可展示额度
            </div>
          ) : (
            orderedPlatforms.map((platform) => (
              <HoverbarPlatformCard key={platform.providerId} platform={platform} />
            ))
          )}
        </div>

        <p className="px-1 pt-0.5 text-[10px] leading-relaxed text-q-text-muted">
          静态演示数据 · 与主窗口共享同一份数据源
        </p>
      </div>
    </div>
  );
}

function DetailIconButton({
  label,
  title,
  onClick,
  children,
}: {
  label: string;
  title: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={title}
      onClick={onClick}
      className="grid h-7 w-7 cursor-pointer place-items-center rounded-q-control text-q-text-secondary transition-colors duration-150 hover:bg-q-primary-softer hover:text-q-primary"
    >
      {children}
    </button>
  );
}
