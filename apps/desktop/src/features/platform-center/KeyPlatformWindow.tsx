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
import type { CapabilitySnapshotViewModel, PlatformSummaryViewModel } from "@/lib/types";

const CARD_WIDTH = 258;
const CARD_GAP = 14;
const CARD_STEP = CARD_WIDTH + CARD_GAP;

type CardDragState = {
  id: string;
  index: number;
  connectedIds: string[];
  startX: number;
  startY: number;
  moved: boolean;
  /** 各卡在内容坐标系的中心点（拖拽期间顺序不变，基点恒定） */
  centers: Map<string, number>;
};

/**
 * 关键平台横向窗口（Apple Glass V6 总览）：
 * - 拖动卡身即可排序（指针事件自绘拖拽）；卡片不可点击进入详情，详情从平台中心查看
 * - 滚轮/触控板、空白处鼠标拖拽、两侧悬浮圆形箭头与下方位置圆点浏览
 * - 排序复用 reorder_platforms 持久化，不建第二套排序；平台增多只横向滚动
 */
export function KeyPlatformWindow({
  platforms,
}: {
  platforms: PlatformSummaryViewModel[];
}) {
  const queryClient = useQueryClient();
  const stripRef = useRef<HTMLDivElement>(null);
  const cardRefs = useRef(new Map<string, HTMLDivElement>());
  const [order, setOrder] = useState<string[]>(() => platforms.map((p) => p.providerId));
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [scrollState, setScrollState] = useState({ atStart: true, atEnd: true, index: 0 });

  const cardDrag = useRef<CardDragState | null>(null);
  // 空白处拖拽滚动状态（非受控，避免重渲染打断惯性）
  const dragScroll = useRef<{ startX: number; startScroll: number; moved: boolean } | null>(null);
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

  // —— 卡片指针拖拽排序 ——
  const contentX = (clientX: number) => {
    const strip = stripRef.current;
    if (!strip) return clientX;
    const rect = strip.getBoundingClientRect();
    return clientX - rect.left + strip.scrollLeft;
  };

  const clearCardTransforms = () => {
    for (const el of cardRefs.current.values()) {
      el.style.transform = "";
      el.style.transition = "";
      el.style.zIndex = "";
    }
  };

  const onCardPointerDown = (event: React.PointerEvent<HTMLDivElement>, id: string) => {
    if (event.button !== 0) return;
    const index = connected.findIndex((p) => p.providerId === id);
    if (index < 0) return;
    const centers = new Map<string, number>();
    for (const platform of connected) {
      const el = cardRefs.current.get(platform.providerId);
      if (!el) continue;
      const rect = el.getBoundingClientRect();
      centers.set(platform.providerId, rect.left + rect.width / 2 - stripRef.current!.scrollLeft);
    }
    cardDrag.current = {
      id,
      index,
      connectedIds: connected.map((p) => p.providerId),
      startX: event.clientX,
      startY: event.clientY,
      moved: false,
      centers,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const onCardPointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const state = cardDrag.current;
    if (!state) return;
    const dx = event.clientX - state.startX;
    const dy = event.clientY - state.startY;
    if (!state.moved) {
      if (Math.abs(dx) < 6 && Math.abs(dy) < 6) return;
      state.moved = true;
      setDraggingId(state.id);
    }
    const px = contentX(event.clientX);
    for (const [id, el] of cardRefs.current) {
      if (id === state.id) continue;
      const center = state.centers.get(id);
      if (center === undefined) continue;
      const idx = state.connectedIds.indexOf(id);
      let push = 0;
      if (idx > state.index && px > center) push = -1;
      else if (idx < state.index && px < center) push = 1;
      const base = el.style.transition;
      if (!base) el.style.transition = "transform 160ms ease";
      el.style.transform = push !== 0 ? `translate(${push * CARD_STEP}px, 0)` : "";
    }
    const draggedEl = cardRefs.current.get(state.id);
    if (draggedEl) {
      const clampedDy = Math.max(-10, Math.min(10, dy));
      draggedEl.style.transform = `translate(${dx}px, ${clampedDy}px)`;
    }
    // 拖近窗口边缘时自动滚动
    const strip = stripRef.current;
    if (strip) {
      const rect = strip.getBoundingClientRect();
      if (event.clientX < rect.left + 80) strip.scrollLeft -= 16;
      else if (event.clientX > rect.right - 80) strip.scrollLeft += 16;
    }
  };

  const onCardPointerUp = (event: React.PointerEvent<HTMLDivElement>) => {
    const state = cardDrag.current;
    cardDrag.current = null;
    if (!state || !state.moved) return;

    // 计算目标下标：later 卡中心在指针左侧 → 后移；earlier 卡中心在指针右侧 → 前移
    const px = contentX(event.clientX);
    let target = state.index;
    for (const [id, center] of state.centers) {
      if (id === state.id) continue;
      const idx = state.connectedIds.indexOf(id);
      if (idx > state.index && px > center) target += 1;
      else if (idx < state.index && px < center) target -= 1;
    }
    clearCardTransforms();
    setDraggingId(null);

    if (target === state.index) return;
    const nextConnected = state.connectedIds.filter((id) => id !== state.id);
    nextConnected.splice(Math.max(0, Math.min(nextConnected.length, target)), 0, state.id);
    // 以新连接顺序回填完整排序（未接入平台保持原位）
    let cursor = 0;
    const nextFull = orderRef.current.map((id) =>
      state.connectedIds.includes(id) ? nextConnected[cursor++] ?? id : id,
    );
    setOrder(nextFull);
    orderRef.current = nextFull;
    if (nextFull.join("\n") !== initialOrderRef.current.join("\n")) {
      reorderMutation.mutate(nextFull);
      initialOrderRef.current = nextFull;
    }
  };

  const onCardPointerCancel = () => {
    cardDrag.current = null;
    clearCardTransforms();
    setDraggingId(null);
  };

  // —— 空白处拖拽滚动（卡片自身处理排序，互不干扰）——
  const onStripPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    const el = stripRef.current;
    if (!el) return;
    if ((event.target as HTMLElement).closest("[data-strip-card]")) return;
    dragScroll.current = {
      startX: event.clientX,
      startScroll: el.scrollLeft,
      moved: false,
    };
    // 捕获指针：拖出窗口边界后仍能继续滚动
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const onStripPointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const state = dragScroll.current;
    const el = stripRef.current;
    if (!state || !el) return;
    const dx = event.clientX - state.startX;
    if (!state.moved && Math.abs(dx) < 5) return;
    state.moved = true;
    el.scrollLeft = state.startScroll - dx;
  };

  const onStripPointerUp = () => {
    dragScroll.current = null;
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
          onPointerDown={onStripPointerDown}
          onPointerMove={onStripPointerMove}
          onPointerUp={onStripPointerUp}
          onPointerLeave={onStripPointerUp}
          className="no-scrollbar flex items-stretch gap-[14px] overflow-x-auto py-1 pl-0.5 pr-0.5"
        >
          {connected.map((platform) => (
            <PlatformStripCard
              key={platform.providerId}
              platform={platform}
              dragging={draggingId === platform.providerId}
              registerRef={(el) => {
                if (el) cardRefs.current.set(platform.providerId, el);
                else cardRefs.current.delete(platform.providerId);
              }}
              onPointerDown={(event) => onCardPointerDown(event, platform.providerId)}
              onPointerMove={onCardPointerMove}
              onPointerUp={onCardPointerUp}
              onPointerCancel={onCardPointerCancel}
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
        拖动卡片排序
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

const WINDOW_ORDER = ["quota_window_5h", "quota_window_7d", "quota_window_30d"];

function windowOrder(id: string): number {
  const index = WINDOW_ORDER.indexOf(id);
  return index >= 0 ? index : WINDOW_ORDER.length;
}

function windowShortLabel(capability: CapabilitySnapshotViewModel): string {
  if (capability.capabilityId === "quota_window_5h") return "5小时";
  if (capability.capabilityId === "quota_window_7d") return "7天";
  if (capability.capabilityId === "quota_window_30d") return "30天";
  const name = capability.displayName.split("·").at(-1)?.trim() || capability.displayName;
  return name.replace(/窗口$/, "");
}

function PlatformStripCard({
  platform,
  dragging,
  registerRef,
  onPointerDown,
  onPointerMove,
  onPointerUp,
  onPointerCancel,
}: {
  platform: PlatformSummaryViewModel;
  dragging: boolean;
  registerRef: (el: HTMLDivElement | null) => void;
  onPointerDown: (event: React.PointerEvent<HTMLDivElement>) => void;
  onPointerMove: (event: React.PointerEvent<HTMLDivElement>) => void;
  onPointerUp: (event: React.PointerEvent<HTMLDivElement>) => void;
  onPointerCancel: (event: React.PointerEvent<HTMLDivElement>) => void;
}) {
  const windows = platform.capabilities
    .filter(
      (capability) =>
        capability.capabilityId.startsWith("quota_window_") && capability.value.primary !== null,
    )
    .sort((left, right) => windowOrder(left.capabilityId) - windowOrder(right.capabilityId) || left.capabilityId.localeCompare(right.capabilityId));
  const balance = platform.capabilities.find(
    (capability) => capability.capabilityId === "balance" && capability.value.primary !== null,
  );
  const totalSpend = platform.capabilities.find(
    (capability) => capability.capabilityId === "total_spend" && capability.value.primary !== null,
  );
  // 余额卡下的小字：优先展示真实累计消费；无累计消费时退回能力自带的次要说明。
  const balanceFootnote = totalSpend
    ? `累计消费 ${compactPercentText(totalSpend.value.primary ?? "")}`
    : (balance?.value.secondary ?? null);
  const tone = providerBrand(platform.providerId).color;
  const stale = windows.some((capability) => capability.freshness === "stale");

  return (
    <div
      ref={registerRef}
      data-strip-card
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerCancel}
      className={cn(
        "glass-panel group relative flex h-[186px] w-[258px] shrink-0 cursor-grab select-none flex-col p-4",
        dragging
          ? "z-30 scale-[1.03] opacity-90 shadow-q-lg ring-2 ring-q-primary/50"
          : "transition-[transform,box-shadow] duration-150 hover:-translate-y-0.5 hover:shadow-q-md",
      )}
      style={{ borderRadius: 16, touchAction: "none" }}
    >
      {/* 拖拽提示（纯装饰，整卡可拖） */}
      <span
        aria-hidden
        className="pointer-events-none absolute right-2 top-2 inline-flex h-6 w-6 items-center justify-center rounded-[7px] text-q-text-muted opacity-0 transition-opacity duration-150 group-hover:opacity-60"
      >
        <GripVertical size={14} />
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
                    {windowShortLabel(capability)}
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
                <span className="text-[11px] text-q-text-muted">{balance.displayName}</span>
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
            <span className="text-[11px] text-q-text-muted">{balance.displayName}</span>
            <span
              className="mt-1 truncate text-[26px] font-bold leading-8 tracking-tight tabular-nums text-q-text-primary"
              data-selectable="true"
            >
              {compactPercentText(balance.value.primary ?? "")}
            </span>
            {balanceFootnote && (
              <span
                className="mt-auto truncate text-[11px] font-semibold tabular-nums text-q-text-primary"
                data-selectable="true"
              >
                {compactPercentText(balanceFootnote)}
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
    </div>
  );
}
