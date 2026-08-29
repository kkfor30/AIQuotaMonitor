import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ArrowRight, ChevronLeft, ChevronRight, GripVertical } from "lucide-react";
import { PlatformMark } from "./ProviderRail";
import { AggregateStatusBadge } from "@/components/ui/StatusBadge";
import { EmptyState } from "@/components/ui/EmptyState";
import { compactPercentText } from "@/lib/format";
import { reorderPlatforms } from "@/lib/ipc";
import { PLATFORM_SUMMARIES_QUERY_KEY } from "@/lib/query-client";
import { cn } from "@/lib/cn";
import type {
  CapabilitySnapshotViewModel,
  PlatformSummaryViewModel,
} from "@/lib/types";

const CARD_WIDTH = 240;
const CARD_GAP = 14;
const CARD_STEP = CARD_WIDTH + CARD_GAP;

/**
 * 关键平台横向窗口（Apple Glass V6 总览）：
 * - 滚轮/触控板、鼠标拖拽（空白与卡身）、前后箭头与位置提示
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
  const [armedId, setArmedId] = useState<string | null>(null);
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

  return (
    <section className="glass-panel flex flex-col gap-3 p-4">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-baseline gap-2.5">
          <h2 className="text-[15px] font-semibold tracking-tight text-q-text-primary">关键平台</h2>
          <span className="text-[11px] text-q-text-muted">拖拽卡片可调整顺序 · 滚轮横向浏览</span>
        </div>
        <div className="flex items-center gap-2">
          {connected.length > 9 ? (
            <span className="text-[11px] tabular-nums text-q-text-muted">
              {Math.min(scrollState.index + 1, connected.length)} / {connected.length}
            </span>
          ) : (
            <div className="flex items-center gap-1.5" aria-hidden>
              {connected.map((platform, index) => (
                <span
                  key={platform.providerId}
                  className={cn(
                    "h-1.5 rounded-full transition-all duration-200",
                    index === scrollState.index
                      ? "w-4 bg-q-primary"
                      : "w-1.5 bg-q-border-strong",
                  )}
                />
              ))}
            </div>
          )}
          <StripArrow
            label="向前浏览"
            direction="prev"
            disabled={scrollState.atStart || connected.length === 0}
            onClick={() => scrollByCards(-1)}
          />
          <StripArrow
            label="向后浏览"
            direction="next"
            disabled={scrollState.atEnd || connected.length === 0}
            onClick={() => scrollByCards(1)}
          />
        </div>
      </div>

      {connected.length === 0 ? (
        <EmptyState
          title="暂无已接入平台"
          description="接入平台后此处展示余额、消费与窗口压力摘要。"
        />
      ) : (
        <div
          ref={stripRef}
          onScroll={updateScrollState}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerLeave={onPointerUp}
          onDragOver={onStripDragOver}
          className="no-scrollbar flex min-h-[236px] cursor-grab flex-1 select-none items-stretch gap-[14px] overflow-x-auto p-0.5 active:cursor-grabbing"
        >
          {connected.map((platform) => (
            <PlatformStripCard
              key={platform.providerId}
              platform={platform}
              armed={armedId === platform.providerId}
              dragging={draggingId === platform.providerId}
              onArmDrag={(id) => setArmedId(id)}
              onDragStarted={(id) => setDraggingId(id)}
              onDragFinished={() => {
                setDraggingId(null);
                setArmedId(null);
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
      )}
    </section>
  );
}

function StripArrow({
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
      className="inline-flex h-7 w-7 cursor-pointer items-center justify-center rounded-[9px] border border-q-border bg-q-surface-strong text-q-text-secondary shadow-q-sm transition-colors duration-150 hover:border-q-border-selected hover:text-q-primary disabled:cursor-default disabled:opacity-35 disabled:hover:border-q-border disabled:hover:text-q-text-secondary"
    >
      <Icon size={15} aria-hidden />
    </button>
  );
}

function PlatformStripCard({
  platform,
  armed,
  dragging,
  onArmDrag,
  onDragStarted,
  onDragFinished,
  onDropOn,
  onOpen,
}: {
  platform: PlatformSummaryViewModel;
  armed: boolean;
  dragging: boolean;
  onArmDrag: (id: string) => void;
  onDragStarted: (id: string) => void;
  onDragFinished: (id: string) => void;
  onDropOn: (draggedId: string, targetId: string) => void;
  onOpen: () => void;
}) {
  const windows = platform.capabilities.filter(
    (capability) =>
      (capability.capabilityId === "quota_window_5h" || capability.capabilityId === "quota_window_7d") &&
      capability.value.progress !== null,
  );
  const balance = pickCapability(platform, "balance");
  const month = pickCapability(platform, "month_spend");

  return (
    <button
      type="button"
      draggable={armed}
      onDragStart={(event) => {
        event.dataTransfer.effectAllowed = "move";
        event.dataTransfer.setData("text/plain", platform.providerId);
        onDragStarted(platform.providerId);
      }}
      onDragEnd={() => onDragFinished(platform.providerId)}
      onDragOver={(event) => event.preventDefault()}
      onDrop={(event) => {
        event.preventDefault();
        const dragged = event.dataTransfer.getData("text/plain");
        if (dragged) onDropOn(dragged, platform.providerId);
      }}
      onClick={onOpen}
      className={cn(
        "glass-panel group relative flex w-[240px] shrink-0 cursor-pointer flex-col gap-2.5 p-4 text-left transition duration-150",
        dragging ? "opacity-45 ring-2 ring-q-primary/50" : "hover:-translate-y-0.5 hover:shadow-q-md",
      )}
      style={{ borderRadius: 14 }}
    >
      {/* 拖拽把手：按下后才允许整卡 HTML5 拖拽排序 */}
      <span
        role="button"
        aria-label={`拖拽排序 ${platform.displayName}`}
        title="拖拽排序"
        data-no-strip-drag
        onPointerDown={(event) => {
          event.stopPropagation();
          event.preventDefault();
          onArmDrag(platform.providerId);
        }}
        className="absolute right-2.5 top-2.5 inline-flex h-6 w-6 cursor-grab items-center justify-center rounded-[7px] text-q-text-muted opacity-0 transition-opacity duration-150 hover:bg-q-primary-softer hover:text-q-primary group-hover:opacity-100 active:cursor-grabbing"
      >
        <GripVertical size={14} aria-hidden />
      </span>

      <div className="flex items-center gap-2.5 pr-7">
        <PlatformMark providerId={platform.providerId} size={32} />
        <span className="min-w-0 flex-1 truncate text-[13px] font-semibold text-q-text-primary">
          {platform.displayName}
        </span>
      </div>

      <AggregateStatusBadge status={platform.aggregateStatus} />

      <div className="flex flex-col gap-2">
        {windows.length > 0 ? (
          windows.map((capability) => (
            <WindowMeter key={capability.capabilityId} capability={capability} />
          ))
        ) : (
          <p className="py-1.5 text-[11px] leading-relaxed text-q-text-muted">
            {platform.aggregateStatus === "setup_required"
              ? "配置来源后展示额度"
              : platform.aggregateStatus === "error"
                ? "刷新失败，暂无可用数据"
                : "暂无窗口用量数据"}
          </p>
        )}
      </div>

      <div className="mt-auto flex flex-col gap-1 border-t border-q-border pt-2.5">
        <MetricRow
          label="余额"
          value={balance?.value.primary ? compactPercentText(balance.value.primary) : null}
          tone={balance?.freshness}
        />
        <MetricRow
          label="本月消费"
          value={month?.value.primary ? compactPercentText(month.value.primary) : null}
          tone={month?.freshness}
        />
        <span className="mt-1 inline-flex items-center gap-1 text-[11px] font-medium text-q-primary">
          查看详情
          <ArrowRight size={12} aria-hidden />
        </span>
      </div>
    </button>
  );
}

function pickCapability(
  platform: PlatformSummaryViewModel,
  capabilityId: string,
): CapabilitySnapshotViewModel | undefined {
  return platform.capabilities.find(
    (capability) => capability.capabilityId === capabilityId && capability.value.primary !== null,
  );
}

function WindowMeter({ capability }: { capability: CapabilitySnapshotViewModel }) {
  const progress = Math.min(1, Math.max(0, capability.value.progress ?? 0));
  const tone =
    capability.freshness === "stale" ? "warning" : progress >= 0.85 ? "danger" : "primary";
  const barColor =
    tone === "warning" ? "bg-q-warning" : tone === "danger" ? "bg-q-danger" : "bg-q-primary";
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-baseline justify-between gap-2">
        <span className="truncate text-[11px] text-q-text-muted">{capability.displayName}</span>
        <span
          className={cn(
            "text-[13px] font-semibold tabular-nums",
            tone === "warning"
              ? "text-q-warning"
              : tone === "danger"
                ? "text-q-danger"
                : "text-q-text-primary",
          )}
          data-selectable="true"
        >
          {capability.value.primary !== null ? compactPercentText(capability.value.primary) : "—"}
        </span>
      </div>
      <div
        className="h-1.5 overflow-hidden rounded-full bg-q-primary-softer"
        role="progressbar"
        aria-valuenow={Math.round(progress * 100)}
        aria-valuemin={0}
        aria-valuemax={100}
      >
        <div className={`h-full rounded-full ${barColor}`} style={{ width: `${progress * 100}%` }} />
      </div>
    </div>
  );
}

function MetricRow({
  label,
  value,
  tone,
}: {
  label: string;
  value: string | null;
  tone?: CapabilitySnapshotViewModel["freshness"];
}) {
  return (
    <div className="flex items-baseline justify-between gap-2">
      <span className="text-[11px] text-q-text-muted">{label}</span>
      <span
        className={cn(
          "text-[12px] font-semibold tabular-nums",
          value === null
            ? "text-q-text-muted"
            : tone === "stale"
              ? "text-q-warning"
              : "text-q-text-primary",
        )}
        data-selectable="true"
      >
        {value ?? "未提供"}
      </span>
    </div>
  );
}
