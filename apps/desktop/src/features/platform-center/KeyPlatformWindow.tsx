import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, GripVertical } from "lucide-react";
import { PlatformMark } from "./ProviderRail";
import { AggregateStatusBadge } from "@/components/ui/StatusBadge";
import { EmptyState } from "@/components/ui/EmptyState";
import { compactPercentText } from "@/lib/format";
import { reorderPlatforms } from "@/lib/ipc";
import { PLATFORM_SUMMARIES_QUERY_KEY } from "@/lib/query-client";
import { providerBrand } from "@/lib/provider-brand";
import { cn } from "@/lib/cn";
import type { PlatformSummaryViewModel } from "@/lib/types";

const CARD_WIDTH = 258;
const CARD_GAP = 14;
const CARD_STEP = CARD_WIDTH + CARD_GAP;

/**
 * 关键平台横向窗口（Apple Glass V6 总览）：
 * - 滚轮/触控板、鼠标拖拽（卡身）、两侧悬浮圆形箭头与下方位置圆点
 * - 卡片右上拖拽把手排序，复用 reorder_platforms 持久化，不建第二套排序
 * - 平台增多时只横向滚动，不向下堆叠
 */
export function KeyPlatformWindow({
  platforms,
  onOpenPlatform,
}: {
  platforms: PlatformSummaryViewModel[];
  onOpenPlatform: (providerId: string) => void;
}) {
  const queryClient = useQueryClient();
  const stripRef = useRef<HTMLDivElement>(null);
  const [order, setOrder] = useState<string[]>(() => platforms.map((p) => p.providerId));
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [scrollState, setScrollState] = useState({ atStart: true, atEnd: true, index: 0 });

  // 拖拽滚动状态（非受控，避免重渲染打断惯性）
  const dragScroll = useRef<{ startX: number; startScroll: number; moved: boolean } | null>(null);
  const suppressClick = useRef(false);
  const orderRef = useRef(order);
  const initialOrderRef = useRef(order);

  useEffect(() => {
    const next = platforms.map((p) => p.providerId);
    setOrder(next);
    orderRef.current = next;
    initialOrderRef.current = next;
  }, [platforms]);

  const reorderMutation = useMutation({
    mutationFn: (ids: string[]) => reorderPlatforms(ids),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: PLATFORM_SUMMARIES_QUERY_KEY });
    },
  });

  const updateScrollState = useCallback(() => {
    const el = stripRef.current;
    if (!el) return;
    const max = el.scrollWidth - el.clientWidth;
    const index = max > 0 ? Math.round(el.scrollLeft / CARD_STEP) : 0;
    setScrollState({
      atStart: el.scrollLeft <= 2,
      atEnd: el.scrollWidth - el.clientWidth - el.scrollLeft <= 2,
      index,
    });
  }, []);

  useEffect(() => {
    updateScrollState();
  }, [updateScrollState, order]);

  // 垂直滚轮转横向滚动；仅当窗口确实可继续滚动时消费，否则放行给页面。
  useEffect(() => {
    const el = stripRef.current;
    if (!el) return;
    const onWheel = (event: WheelEvent) => {
      if (Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
      const max = el.scrollWidth - el.clientWidth;
      if (max <= 0) return;
      const canConsume =
        (event.deltaY > 0 && el.scrollLeft < max - 1) ||
        (event.deltaY < 0 && el.scrollLeft > 1);
      if (!canConsume) return;
      event.preventDefault();
      el.scrollLeft += event.deltaY;
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);

  const byId = useMemo(
    () => new Map(platforms.map((platform) => [platform.providerId, platform])),
    [platforms],
  );
  const connected = order
    .map((id) => byId.get(id))
    .filter((p): p is PlatformSummaryViewModel => Boolean(p && p.aggregateStatus !== "setup_required"));

  const scrollByCards = (direction: -1 | 1) => {
    const el = stripRef.current;
    if (!el) return;
    el.scrollBy({ left: direction * CARD_STEP * 2, behavior: "smooth" });
  };

  // —— 鼠标拖拽滚动（卡身与空白；拖拽把手与按钮不参与）——
  const onPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    const el = stripRef.current;
    if (!el) return;
    if ((event.target as HTMLElement).closest("[data-no-strip-drag]")) return;
    dragScroll.current = {
      startX: event.clientX,
      startScroll: el.scrollLeft,
      moved: false,
    };
  };

  const onPointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const state = dragScroll.current;
    const el = stripRef.current;
    if (!state || !el) return;
    const dx = event.clientX - state.startX;
    if (!state.moved && Math.abs(dx) < 5) return;
    state.moved = true;
    el.scrollLeft = state.startScroll - dx;
  };

  const onPointerUp = () => {
    if (dragScroll.current?.moved) suppressClick.current = true;
    dragScroll.current = null;
    // 点击抑制只作用于紧随其后的一次 click
    window.setTimeout(() => {
      suppressClick.current = false;
    }, 0);
  };

  // —— 卡片拖拽排序（拖拽把手触发 HTML5 DnD，乐观更新 + 松手持久化）——
  const commitOrder = (next: string[]) => {
    setOrder(next);
    orderRef.current = next;
  };

  const moveDraggedTo = (draggedId: string, targetId: string) => {
    if (draggedId === targetId) return;
    const current = orderRef.current;
    const from = current.indexOf(draggedId);
    const to = current.indexOf(targetId);
    if (from < 0 || to < 0) return;
    const next = [...current];
    next.splice(from, 1);
    next.splice(to, 0, draggedId);
    commitOrder(next);
  };

  const persistIfChanged = () => {
    if (orderRef.current.join("\n") !== initialOrderRef.current.join("\n")) {
      reorderMutation.mutate(orderRef.current);
      initialOrderRef.current = orderRef.current;
    }
  };

  const onStripDragOver = (event: React.DragEvent<HTMLDivElement>) => {
    if (!draggingId) return;
    event.preventDefault();
    // 拖拽靠近窗口边缘时自动滚动
    const el = stripRef.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    if (event.clientX - rect.left < 72) el.scrollLeft -= 18;
    else if (rect.right - event.clientX < 72) el.scrollLeft += 18;
  };

  if (connected.length === 0) {
    return (
      <section className="flex flex-col gap-2">
        <SectionHeader />
        <EmptyState
          title="暂无已接入平台"
          description="接入平台后此处展示余额、消费与窗口压力摘要。"
        />
      </section>
    );
  }

  return (
    <section className="flex flex-col gap-2.5">
      <SectionHeader />

      <div className="relative">
        <CarouselArrow
          label="向前浏览"
          direction="prev"
          disabled={scrollState.atStart}
          onClick={() => scrollByCards(-1)}
        />
        <CarouselArrow
          label="向后浏览"
          direction="next"
          disabled={scrollState.atEnd}
          onClick={() => scrollByCards(1)}
        />

        <div
          ref={stripRef}
          onScroll={updateScrollState}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerLeave={onPointerUp}
          onDragOver={onStripDragOver}
          className="no-scrollbar flex cursor-grab select-none items-stretch gap-[14px] overflow-x-auto py-1 pl-0.5 pr-0.5 active:cursor-grabbing"
        >
          {connected.map((platform) => (
            <PlatformStripCard
              key={platform.providerId}
              platform={platform}
              dragging={draggingId === platform.providerId}
              onDragStarted={(id) => setDraggingId(id)}
              onDragFinished={() => {
                setDraggingId(null);
                persistIfChanged();
              }}
              onDropOn={(dragged, target) => moveDraggedTo(dragged, target)}
              onOpen={() => {
                if (suppressClick.current) return;
                onOpenPlatform(platform.providerId);
              }}
            />
          ))}
        </div>

        {/* 位置圆点：一卡一点，随滚动高亮 */}
        <div className="mt-2 flex items-center justify-center gap-1.5" aria-hidden>
          {connected.map((platform, index) => (
            <span
              key={platform.providerId}
              className={cn(
                "h-1.5 rounded-full transition-all duration-200",
                index === scrollState.index ? "w-4 bg-q-primary" : "w-1.5 bg-q-border-strong",
              )}
            />
          ))}
        </div>
      </div>
    </section>
  );
}

function SectionHeader() {
  return (
    <div className="flex items-center gap-2.5 px-1">
      <h2 className="text-[15px] font-semibold tracking-tight text-q-text-primary">关键平台</h2>
      <span className="inline-flex items-center gap-1 text-[11px] text-q-text-muted">
        <GripVertical size={12} aria-hidden />
        拖拽排序
      </span>
    </div>
  );
}

/** 悬浮在卡片行两侧的圆形浏览按钮（Apple Glass V6） */
function CarouselArrow({
  label,
  direction,
  disabled,
  onClick,
}: {
  label: string;
  direction: "prev" | "next";
  disabled: boolean;
  onClick: () => void;
}) {
  const Icon = direction === "prev" ? ChevronLeft : ChevronRight;
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "absolute top-1/2 z-10 flex h-8 w-8 -translate-y-1/2 cursor-pointer items-center justify-center rounded-full border border-q-border bg-white/95 text-q-text-secondary shadow-[0_4px_14px_rgba(16,34,64,0.16)] backdrop-blur transition-all duration-150",
        direction === "prev" ? "-left-2" : "-right-2",
        "hover:border-q-border-selected hover:text-q-primary active:scale-95",
        disabled && "pointer-events-none opacity-0",
      )}
    >
      <Icon size={16} aria-hidden />
    </button>
  );
}

function PlatformStripCard({
  platform,
  dragging,
  onDragStarted,
  onDragFinished,
  onDropOn,
  onOpen,
}: {
  platform: PlatformSummaryViewModel;
  dragging: boolean;
  onDragStarted: (id: string) => void;
  onDragFinished: () => void;
  onDropOn: (draggedId: string, targetId: string) => void;
  onOpen: () => void;
}) {
  const cardRef = useRef<HTMLButtonElement>(null);
  const windows = platform.capabilities.filter(
    (capability) =>
      (capability.capabilityId === "quota_window_5h" || capability.capabilityId === "quota_window_7d") &&
      capability.value.progress !== null,
  );
  const balance = platform.capabilities.find(
    (capability) => capability.capabilityId === "balance" && capability.value.primary !== null,
  );
  const tone = providerBrand(platform.providerId).color;
  const stale = windows.some((capability) => capability.freshness === "stale");

  return (
    <button
      ref={cardRef}
      type="button"
      onDragOver={(event) => {
        event.preventDefault();
        event.dataTransfer.dropEffect = "move";
      }}
      onDrop={(event) => {
        event.preventDefault();
        const dragged = event.dataTransfer.getData("text/plain");
        if (dragged) onDropOn(dragged, platform.providerId);
      }}
      onClick={onOpen}
      className={cn(
        "glass-panel group relative flex h-[186px] w-[258px] shrink-0 cursor-pointer flex-col p-4 text-left transition duration-150",
        dragging ? "opacity-45 ring-2 ring-q-primary/50" : "hover:-translate-y-0.5 hover:shadow-q-md",
      )}
      style={{ borderRadius: 16 }}
    >
      {/* 拖拽把手：自身作为 HTML5 拖拽源，拖拽幽灵替换为整卡 */}
      <span
        role="button"
        aria-label={`拖拽排序 ${platform.displayName}`}
        title="拖拽排序"
        draggable
        data-no-strip-drag
        onPointerDown={(event) => event.stopPropagation()}
        onClick={(event) => event.stopPropagation()}
        onDragStart={(event) => {
          event.dataTransfer.effectAllowed = "move";
          event.dataTransfer.setData("text/plain", platform.providerId);
          if (cardRef.current) {
            event.dataTransfer.setDragImage(cardRef.current, 129, 20);
          }
          onDragStarted(platform.providerId);
        }}
        onDragEnd={() => onDragFinished()}
        className="absolute right-2 top-2 inline-flex h-6 w-6 cursor-grab items-center justify-center rounded-[7px] text-q-text-muted opacity-40 transition-opacity duration-150 hover:bg-q-primary-softer hover:text-q-primary group-hover:opacity-100 active:cursor-grabbing"
      >
        <GripVertical size={14} aria-hidden />
      </span>

      <div className="flex items-center gap-2.5 pr-7">
        <PlatformMark providerId={platform.providerId} size={34} />
        <span className="min-w-0 flex-1 truncate text-[13px] font-semibold text-q-text-primary">
          {platform.displayName}
        </span>
        <AggregateStatusBadge status={platform.aggregateStatus} />
      </div>

      {/* 卡身：窗口平台展示 5 小时 / 7 天大号百分比；余额平台展示大号金额 */}
      <div className="mt-3 flex min-h-0 flex-1 flex-col">
        {windows.length > 0 ? (
          <>
            <div className="grid flex-1 grid-cols-2 gap-2">
              {windows.slice(0, 2).map((capability) => (
                <div key={capability.capabilityId} className="flex min-w-0 flex-col gap-0.5">
                  <span className="text-[11px] text-q-text-muted">
                    {capability.capabilityId === "quota_window_5h" ? "5小时" : "7天"}
                    {capability.freshness === "stale" ? " · 缓存" : ""}
                  </span>
                  <span
                    className={cn(
                      "truncate text-[24px] font-bold leading-8 tracking-tight tabular-nums",
                      stale ? "text-q-warning" : "",
                    )}
                    style={stale ? undefined : { color: tone }}
                    data-selectable="true"
                  >
                    {capability.value.primary !== null
                      ? compactPercentText(capability.value.primary)
                      : "—"}
                  </span>
                </div>
              ))}
            </div>
            {balance && (
              <div className="mt-auto flex items-baseline justify-between border-t border-q-border pt-2">
                <span className="text-[11px] text-q-text-muted">余额</span>
                <span
                  className="text-[13px] font-bold tabular-nums text-q-text-primary"
                  data-selectable="true"
                >
                  {compactPercentText(balance.value.primary ?? "")}
                </span>
              </div>
            )}
          </>
        ) : balance ? (
          <div className="flex flex-1 flex-col gap-0.5">
            <span className="text-[11px] text-q-text-muted">余额</span>
            <span
              className="mt-1 truncate text-[26px] font-bold leading-8 tracking-tight tabular-nums text-q-text-primary"
              data-selectable="true"
            >
              {compactPercentText(balance.value.primary ?? "")}
            </span>
            {balance.value.secondary && (
              <span className="mt-auto text-[11px] text-q-text-muted">
                {compactPercentText(balance.value.secondary)}
              </span>
            )}
          </div>
        ) : (
          <p className="flex flex-1 items-center text-[11px] leading-relaxed text-q-text-muted">
            {platform.aggregateStatus === "setup_required"
              ? "配置来源后展示额度"
              : platform.aggregateStatus === "error"
                ? "刷新失败，暂无可用数据"
                : "暂无窗口用量数据"}
          </p>
        )}
      </div>
    </button>
  );
}
