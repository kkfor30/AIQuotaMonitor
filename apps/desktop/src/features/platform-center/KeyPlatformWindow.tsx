import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { flushSync } from "react-dom";
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
  /** 拖拽开始时该卡所在槽位（锚点，拖拽期间不随实时换位变化） */
  startIndex: number;
  /** 拖拽开始时被拖卡的视觉左缘（指针锚定基准） */
  anchorLeft: number;
  startX: number;
  startY: number;
  moved: boolean;
};

/**
 * 关键平台横向窗口（Apple Glass V6 总览）：
 * - 拖动卡身即可排序：拖动中指针越过相邻卡中心即实时换位（其余卡 FLIP 滑动让位），松手只做落位收尾
 * - 滚轮/触控板、空白处鼠标拖拽、头部两侧箭头与下方位置圆点浏览
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
    // 拖拽进行中不重置顺序：排序持久化后的数据回流不允许打断动画
    if (cardDrag.current) return;
    const next = platforms.map((p) => p.providerId);
    setOrder(next);
    orderRef.current = next;
    initialOrderRef.current = next;
  }, [platforms]);

  const reorderMutation = useMutation({
    mutationFn: (ids: string[]) => reorderPlatforms(ids),
    onSuccess: () => {
      // reorder_platforms 返回 void：失效重拉（重拉期间保留旧数据，不会出现空缓存白屏）
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

  // —— 卡片指针拖拽排序（拖动中实时换位，松手仅落位收尾）——
  const clearCardTransforms = () => {
    for (const el of cardRefs.current.values()) {
      el.style.transform = "";
      el.style.transition = "";
      el.style.zIndex = "";
    }
  };

  /** 槽位 i 的视觉左缘：strip 左内边距(2px) + i*步长 - 滚动量。等宽卡片，纯几何计算不依赖 DOM 顺序。 */
  const slotLeft = (index: number) => {
    const strip = stripRef.current;
    if (!strip) return 0;
    return strip.getBoundingClientRect().left + 2 + index * CARD_STEP - strip.scrollLeft;
  };

  const onCardPointerDown = (event: React.PointerEvent<HTMLDivElement>, id: string) => {
    if (event.button !== 0) return;
    const index = connected.findIndex((p) => p.providerId === id);
    const draggedEl = cardRefs.current.get(id);
    if (index < 0 || !draggedEl) return;
    cardDrag.current = {
      id,
      startIndex: index,
      anchorLeft: draggedEl.getBoundingClientRect().left,
      startX: event.clientX,
      startY: event.clientY,
      moved: false,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const onCardPointerMove = (event: React.PointerEvent<HTMLDivElement>) => {
    const state = cardDrag.current;
    if (!state) return;
    const strip = stripRef.current;
    const draggedEl = cardRefs.current.get(state.id);
    if (!strip || !draggedEl) return;
    const dx = event.clientX - state.startX;
    const dy = event.clientY - state.startY;
    if (!state.moved) {
      if (Math.abs(dx) < 6 && Math.abs(dy) < 6) return;
      state.moved = true;
      setDraggingId(state.id);
    }

    const connectedIds = orderRef.current.filter(
      (pid) => byId.get(pid)?.aggregateStatus !== "setup_required",
    );
    const curIdx = connectedIds.indexOf(state.id);
    if (curIdx < 0) return;

    // 实时换位：指针带动的卡片中心越过相邻卡中心即更新顺序
    const draggedCenter = state.anchorLeft + dx + CARD_WIDTH / 2;
    let target = curIdx;
    for (let i = 0; i < connectedIds.length; i++) {
      if (i === curIdx) continue;
      const center = slotLeft(i) + CARD_WIDTH / 2;
      if (i > curIdx && draggedCenter > center) target = Math.max(target, i);
      else if (i < curIdx && draggedCenter < center) target = Math.min(target, i);
    }
    if (target !== curIdx) {
      const nextConnected = connectedIds.filter((pid) => pid !== state.id);
      nextConnected.splice(target, 0, state.id);
      // FLIP：记录其余卡当前视觉位置（含让位过渡中的偏移），重排后反向补偿滑动到新槽位
      const before = new Map<string, number>();
      for (const [pid, el] of cardRefs.current) {
        if (pid === state.id) continue;
        before.set(pid, el.getBoundingClientRect().left);
      }
      let cursor = 0;
      const nextFull = orderRef.current.map((pid) =>
        byId.get(pid)?.aggregateStatus !== "setup_required" ? nextConnected[cursor++] ?? pid : pid,
      );
      flushSync(() => setOrder(nextFull));
      orderRef.current = nextFull;
      for (const [pid, el] of cardRefs.current) {
        const old = before.get(pid);
        if (old === undefined || pid === state.id) continue;
        const shift = Math.round(old - el.getBoundingClientRect().left);
        if (Math.abs(shift) < 1) continue;
        el.style.transition = "none";
        el.style.transform = `translate(${shift}px, 0)`;
        void el.offsetWidth;
        el.style.transition = "transform 200ms cubic-bezier(0.2, 0.78, 0.24, 1)";
        el.style.transform = "";
      }
    }

    // 被拖卡即时跟随指针：以拖拽开始的视觉位置为锚，槽位变化量直接折进 transform，不跳
    const liveIdx = target !== curIdx ? target : curIdx;
    draggedEl.style.transition = "none";
    draggedEl.style.transform = `translate(${Math.round(state.anchorLeft + dx - slotLeft(liveIdx))}px, ${Math.max(-10, Math.min(10, dy))}px)`;
    draggedEl.style.zIndex = "30";

    // 拖近窗口边缘时自动滚动；滚动量会在下一次 move 的槽位计算中自动吸收
    const rect = strip.getBoundingClientRect();
    if (event.clientX < rect.left + 80) strip.scrollLeft -= 16;
    else if (event.clientX > rect.right - 80) strip.scrollLeft += 16;
  };

  const settleCardDrag = () => {
    const state = cardDrag.current;
    cardDrag.current = null;
    setDraggingId(null);
    if (!state || !state.moved) return;
    const strip = stripRef.current;
    const draggedEl = cardRefs.current.get(state.id);
    if (strip && draggedEl) {
      // 被拖卡从当前视觉位置平滑滑入所属槽位
      const from = draggedEl.getBoundingClientRect();
      draggedEl.style.transition = "none";
      draggedEl.style.transform = "";
      draggedEl.style.zIndex = "";
      const dx0 = Math.round(from.left - draggedEl.getBoundingClientRect().left);
      const dy0 = Math.round(from.top - draggedEl.getBoundingClientRect().top);
      if (dx0 !== 0 || dy0 !== 0) {
        draggedEl.style.transform = `translate(${dx0}px, ${dy0}px)`;
        void strip.offsetWidth;
        draggedEl.style.transition = "transform 220ms cubic-bezier(0.2, 0.78, 0.24, 1)";
        draggedEl.style.transform = "";
        window.setTimeout(() => clearCardTransforms(), 230);
      }
    }
    if (orderRef.current.join("\n") !== initialOrderRef.current.join("\n")) {
      reorderMutation.mutate(orderRef.current);
      initialOrderRef.current = orderRef.current;
    }
  };

  const onCardPointerCancel = () => {
    // 取消与松手同路径：顺序已实时生效，保持并落位，避免与已持久化顺序不一致
    settleCardDrag();
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
      {/* 标题行：箭头放在标题右侧，避免悬浮按钮压住边缘卡片、挡住拖拽起手 */}
      <div className="flex items-center gap-2.5 px-1">
        <h2 className="text-[15px] font-semibold tracking-tight text-q-text-primary">关键平台</h2>
        <span className="inline-flex items-center gap-1 text-[11px] text-q-text-muted">
          <GripVertical size={12} aria-hidden />
          拖动卡片排序
        </span>
        <div className="ml-auto flex items-center gap-1.5">
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
        </div>
      </div>

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
            onPointerUp={settleCardDrag}
            onPointerCancel={onCardPointerCancel}
          />
        ))}
      </div>

      {/* 位置圆点：一卡一点，随滚动高亮 */}
      <div className="flex items-center justify-center gap-1.5" aria-hidden>
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
        "flex h-7 w-7 cursor-pointer items-center justify-center rounded-full border border-q-border bg-white/95 text-q-text-secondary shadow-[0_4px_14px_rgba(16,34,64,0.16)] backdrop-blur transition-all duration-150",
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
  // 多账号平台按账号分行展示窗口额度（Capability 用 accountId 归属，不合并同名窗口）
  const accountWindowRows = platform.accounts
    .map((account) => ({
      account,
      windows: windows.filter((capability) => capability.accountId === account.accountId),
    }))
    .filter((row) => row.windows.length > 0);
  const multiAccountRows = platform.accounts.length > 1 && accountWindowRows.length > 1 ? accountWindowRows : null;
  const balance = platform.capabilities.find(
    (capability) => capability.capabilityId === "balance" && capability.value.primary !== null,
  );
  // 多账号平台余额行标注所属账号，避免多个余额来源时含义不清
  const balanceAccountName = balance && multiAccountRows
    ? (platform.accounts.find((account) => account.accountId === balance.accountId)?.displayName ?? null)
    : null;
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

      {/* 卡身：单账号窗口平台展示 5 小时 / 7 天大号百分比；多账号平台按账号分行，避免同名窗口并列混淆 */}
      <div className="mt-3 flex min-h-0 flex-1 flex-col">
        {multiAccountRows ? (
          <>
            <div className="flex min-h-0 flex-1 flex-col justify-center gap-2.5">
              {multiAccountRows.map((row) => (
                <div key={row.account.accountId} className="flex min-w-0 items-baseline justify-between gap-2">
                  <span className="min-w-0 truncate text-[11px] font-medium text-q-text-muted">
                    {row.account.displayName}
                  </span>
                  <span className="flex shrink-0 flex-wrap items-baseline justify-end gap-x-3 gap-y-0.5">
                    {row.windows.map((capability) => (
                      <span
                        key={capability.capabilityId}
                        className="text-[11px] text-q-text-muted"
                        data-selectable="true"
                      >
                        {windowShortLabel(capability)}
                        <b
                          className={cn(
                            "ml-1 text-[15px] font-bold leading-5 tabular-nums",
                            stale ? "text-q-warning" : "",
                          )}
                          style={stale ? undefined : { color: tone }}
                        >
                          {compactPercentText(capability.value.primary ?? "")}
                        </b>
                        {capability.freshness === "stale" ? " · 缓存" : ""}
                      </span>
                    ))}
                  </span>
                </div>
              ))}
            </div>
            {balance && (
              <div className="mt-auto flex items-baseline justify-between border-t border-q-border pt-2">
                <span className="min-w-0 truncate text-[11px] text-q-text-muted">
                  {balance.displayName}
                  {balanceAccountName ? ` · ${balanceAccountName}` : ""}
                </span>
                <span
                  className="text-[13px] font-bold tabular-nums text-q-text-primary"
                  data-selectable="true"
                >
                  {compactPercentText(balance.value.primary ?? "")}
                </span>
              </div>
            )}
          </>
        ) : windows.length > 0 ? (
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
