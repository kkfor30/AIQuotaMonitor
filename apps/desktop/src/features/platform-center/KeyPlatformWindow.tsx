import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, GripVertical } from "lucide-react";
import { PlatformMark } from "./ProviderRail";
import { EmptyState } from "@/components/ui/EmptyState";
import { QuotaProgress } from "@/components/ui/QuotaProgress";
import { compactPercentText, formatTime } from "@/lib/format";
import { reorderPlatforms } from "@/lib/ipc";
import { PLATFORM_SUMMARIES_QUERY_KEY } from "@/lib/query-client";
import { cn } from "@/lib/cn";
import { sortWindowCapabilities, windowShortLabel } from "./quota-windows";
import type {
  AccountKind,
  AccountSummaryViewModel,
  CapabilitySnapshotViewModel,
  PlatformSummaryViewModel,
} from "@/lib/types";

const CARD_WIDTH = 258;
const CARD_HEIGHT = 296;
const CARD_GAP = 14;
const CARD_STEP = CARD_WIDTH + CARD_GAP;
/** 条带左内边距（pl-0.5），第一张卡的内容坐标起点 */
const FIRST_SLOT_INSET = 2;
/** 账号类型徽章（本机/默认/额外），V7 中所有账号别名可见 */
const ACCOUNT_KIND_LABEL: Record<AccountKind, string> = {
  local: "本机",
  default: "默认",
  additional: "额外",
};

const SHIFT_TRANSITION = "transform 180ms cubic-bezier(0.2, 0.78, 0.24, 1)";
const DROP_TRANSITION = "transform 220ms cubic-bezier(0.2, 0.78, 0.24, 1)";
/** 拖动阈值：未超过该位移视为点击，不触发排序 */
const DRAG_THRESHOLD_PX = 5;
/** 目标槽位切换的迟滞区：越过相邻槽位中线再超出该距离才换位，避免分界线上抖动 */
const TARGET_HYSTERESIS_PX = 10;
/** 边缘自动滚动：感应区域与渐进速度（px/帧） */
const EDGE_ZONE_PX = 80;
const EDGE_SPEED_MIN = 2;
const EDGE_SPEED_MAX = 12;

type CardDragState = {
  id: string;
  /** 拖拽开始时该卡所在槽位 */
  startIndex: number;
  startX: number;
  startY: number;
  /** 最新指针坐标；pointermove 只写坐标，DOM 更新统一在 rAF 帧里做 */
  clientX: number;
  clientY: number;
  pending: boolean;
  moved: boolean;
  /** 实时换位目标槽位；只决定让位，不影响被拖卡跟手位移 */
  target: number;
  /** 按压点相对卡片左缘的横向偏移（跟手基准） */
  grabOffsetX: number;
  /** 条带视口几何与滚动缓存（pointerdown 时测量，拖拽期间不再读 getBoundingClientRect） */
  stripRectLeft: number;
  stripRectRight: number;
  startScrollLeft: number;
  scrollLeft: number;
  /** 实测卡宽与槽位步长（防响应式布局变化） */
  cardWidth: number;
  step: number;
  /** 拖拽期间固定不变的 connected 平台 id 顺序 */
  connected: string[];
};

/**
 * 关键平台横向窗口（V7 总览设计稿 01）：
 * - 一个 Platform 永远占一个排序槽位；槽位内部按 platform.accounts 构建账号卡组，
 *   前卡显示当前账号，箭头 1/2 切换（到端禁用），前端按平台记忆当前账号。
 * - 拖拽：仅卡头「名称与账号箭头之间」的拖拽带发起，移动整组（翻页按钮与卡内
 *   文字不受影响，文字可选中）。被拖卡位移始终以起始位置为基准（translateX =
 *   指针位移 + 滚动补偿，与 target 无关），target 只决定其余卡的让位；事件走
 *   window 原生监听 + 单帧 rAF 调度，拖拽全程零 React 渲染（拖拽态为命令式
 *   class），松手一次性提交顺序并 FLIP 落位。
 * - 排序复用 reorder_platforms，只提交 platform ids；滚轮/触控板/空白拖拽/边缘自动滚动保留。
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
  const cardRefs = useRef(new Map<string, HTMLDivElement>());
  const [order, setOrder] = useState<string[]>(() => platforms.map((p) => p.providerId));
  const [scrollState, setScrollState] = useState({ atStart: true, atEnd: true, index: 0 });
  /** 每个平台独立维护当前展示的账号；刷新时保留，账号被删才回退第一个 */
  const [currentAccountId, setCurrentAccountId] = useState<Record<string, string>>({});

  const cardDrag = useRef<CardDragState | null>(null);
  const rafRef = useRef<number | null>(null);
  const pendingFrame = useRef(false);
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

  /** 拖拽期间 onScroll 只同步拖拽缓存的 scrollLeft，不触发 React 渲染；结束后统一刷新一次。 */
  const updateScrollState = useCallback(() => {
    const el = stripRef.current;
    if (!el) return;
    if (cardDrag.current) {
      cardDrag.current.scrollLeft = el.scrollLeft;
      return;
    }
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

  // —— 卡头拖拽带排序：坐标模型以起始位置为唯一基准 ——
  // 被拖卡 translateX = 指针位移 + 滚动补偿（与 target 无关）；
  // target 仅由内容坐标里的卡组中心推算（O(1)，带迟滞），驱动其余卡让位。

  /** 每次目标槽位变化时，按 startIndex 与新 target 从零重算全部让位状态，避免反向拖动残留 */
  const applyShifts = (state: CardDragState, nextTarget: number) => {
    if (nextTarget === state.target) return;
    state.target = nextTarget;
    for (let i = 0; i < state.connected.length; i++) {
      const id = state.connected[i];
      if (id === undefined || id === state.id) continue;
      const el = cardRefs.current.get(id);
      if (!el) continue;
      let shift = 0;
      if (nextTarget > state.startIndex && i > state.startIndex && i <= nextTarget) {
        shift = -state.step;
      } else if (nextTarget < state.startIndex && i >= nextTarget && i < state.startIndex) {
        shift = state.step;
      }
      el.style.transition = SHIFT_TRANSITION;
      el.style.transform = shift !== 0 ? `translate3d(${shift}px, 0, 0)` : "translate3d(0, 0, 0)";
    }
  };

  /** 一帧内的位置计算与样式写入：跟手 transform、目标槽位判定（不含边缘滚动） */
  const processPosition = (state: CardDragState) => {
    const draggedEl = cardRefs.current.get(state.id);
    if (!draggedEl) return;
    const dx = state.clientX - state.startX;
    const dy = state.clientY - state.startY;
    if (!state.moved) {
      if (Math.abs(dx) < DRAG_THRESHOLD_PX && Math.abs(dy) < DRAG_THRESHOLD_PX) return;
      state.moved = true;
      draggedEl.classList.add("strip-card-dragging");
    }
    // 跟手：固定以起始位置为基准，自动滚动时加入滚动补偿
    const scrollDelta = state.scrollLeft - state.startScrollLeft;
    const translateX = dx + scrollDelta;
    const translateY = Math.max(-10, Math.min(10, dy));
    draggedEl.style.transition = "none";
    draggedEl.style.transform = `translate3d(${Math.round(translateX)}px, ${translateY}px, 0)`;

    // 目标槽位：内容坐标下被拖卡中心落在哪个槽位（O(1)），越过中线加迟滞才切换
    const pointerContentX = state.clientX - state.stripRectLeft + state.scrollLeft;
    const draggedCenterContent = pointerContentX - state.grabOffsetX + state.cardWidth / 2;
    const relIndex = (draggedCenterContent - (FIRST_SLOT_INSET + state.cardWidth / 2)) / state.step;
    const candidate = Math.max(0, Math.min(state.connected.length - 1, Math.round(relIndex)));
    if (candidate !== state.target && Math.abs(relIndex - state.target) > 0.5 + TARGET_HYSTERESIS_PX / state.step) {
      applyShifts(state, candidate);
    }
  };

  /** 单帧调度：只在有待处理坐标或边缘滚动时安排下一帧，不空转 */
  const scheduleFrame = useCallback(() => {
    if (pendingFrame.current) return;
    pendingFrame.current = true;
    rafRef.current = requestAnimationFrame(() => {
      pendingFrame.current = false;
      rafRef.current = null;
      const state = cardDrag.current;
      const strip = stripRef.current;
      if (!state || !strip) return;
      if (state.pending) {
        state.pending = false;
        processPosition(state);
      }
      // 渐进边缘自动滚动：到达边界（滚动量不再变化）后停止续帧
      const speed = edgeScrollSpeed(state);
      if (speed !== 0) {
        const before = strip.scrollLeft;
        strip.scrollLeft = before + speed;
        if (strip.scrollLeft !== before) {
          state.scrollLeft = strip.scrollLeft;
          processPosition(state);
          scheduleFrame();
        }
      }
    });
  }, []);

  const edgeScrollSpeed = (state: CardDragState): number => {
    const speedFor = (depth: number) =>
      EDGE_SPEED_MIN + (EDGE_SPEED_MAX - EDGE_SPEED_MIN) * Math.max(0, Math.min(1, 1 - depth / EDGE_ZONE_PX));
    const leftDepth = state.clientX - state.stripRectLeft;
    if (leftDepth >= 0 && leftDepth < EDGE_ZONE_PX) return -speedFor(leftDepth);
    const rightDepth = state.stripRectRight - state.clientX;
    if (rightDepth >= 0 && rightDepth < EDGE_ZONE_PX) return speedFor(rightDepth);
    return 0;
  };

  /** 动画结束后清理拖拽残留：inline 样式与拖拽态 class */
  const cleanupCardStyles = (id: string) => {
    const el = cardRefs.current.get(id);
    if (!el) return;
    el.style.transition = "";
    el.style.transform = "";
    el.style.zIndex = "";
    el.classList.remove("strip-card-dragging");
  };

  const detachDragListeners = useCallback(() => {
    window.removeEventListener("pointermove", onWindowPointerMove);
    window.removeEventListener("pointerup", onWindowPointerUp);
    window.removeEventListener("pointercancel", onWindowPointerCancel);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- 处理器均为稳定 useCallback
  }, []);

  const onWindowPointerMove = useCallback((event: PointerEvent) => {
    const state = cardDrag.current;
    if (!state) return;
    state.clientX = event.clientX;
    state.clientY = event.clientY;
    state.pending = true;
    scheduleFrameRef.current();
  }, [scheduleFrame]);

  const finishDrag = useCallback(
    (commit: boolean) => {
      const state = cardDrag.current;
      cardDrag.current = null;
      if (rafRef.current !== null) {
        cancelAnimationFrame(rafRef.current);
        rafRef.current = null;
      }
      pendingFrame.current = false;
      detachDragListeners();
      if (!state) return;
      const draggedEl = cardRefs.current.get(state.id);
      const strip = stripRef.current;

      // 取消路径：卡片平滑回原位、让位卡归零，不提交排序
      if (!commit) {
        if (state.moved) {
          if (draggedEl) {
            draggedEl.style.transition = DROP_TRANSITION;
            draggedEl.style.transform = "translate3d(0, 0, 0)";
          }
          for (const [pid, el] of cardRefs.current) {
            if (pid === state.id) continue;
            if (el.style.transform && el.style.transform !== "none") {
              el.style.transition = SHIFT_TRANSITION;
              el.style.transform = "translate3d(0, 0, 0)";
            }
          }
          window.setTimeout(() => cleanupCardStyles(state.id), 240);
        }
        updateScrollState();
        return;
      }

      // 点击（未超阈值）：直接清理，不触发排序
      if (!state.moved) {
        cleanupCardStyles(state.id);
        updateScrollState();
        return;
      }

      // 松手前先消费最后一次未处理的指针坐标，避免丢帧
      if (state.pending) {
        state.pending = false;
        processPosition(state);
      }
      const fromRect = draggedEl?.getBoundingClientRect();
      const changed = state.target !== state.startIndex;

      // 关闭全部过渡，避免 DOM 顺序调整时让位动画残留干扰
      for (const el of cardRefs.current.values()) {
        el.style.transition = "none";
      }
      if (changed) {
        const nextConnected = state.connected.filter((pid) => pid !== state.id);
        nextConnected.splice(state.target, 0, state.id);
        let cursor = 0;
        const nextFull = orderRef.current.map((pid) =>
          byId.get(pid)?.aggregateStatus !== "setup_required" ? nextConnected[cursor++] ?? pid : pid,
        );
        orderRef.current = nextFull;
        // 松手唯一一次同步渲染：提交顺序后立刻测量新槽位
        flushSync(() => setOrder(nextFull));
      }
      // 清除让位偏移，卡落回各自（新）槽位
      for (const [pid, el] of cardRefs.current) {
        if (pid === state.id) continue;
        el.style.transform = "";
      }
      // 被拖卡从当前视觉位置平滑落入目标槽位；拖拽态 class 待动画结束后清理（期间保住层级）
      if (strip && draggedEl && fromRect) {
        draggedEl.style.transition = "none";
        draggedEl.style.transform = "";
        const dx0 = Math.round(fromRect.left - draggedEl.getBoundingClientRect().left);
        const dy0 = Math.round(fromRect.top - draggedEl.getBoundingClientRect().top);
        if (dx0 !== 0 || dy0 !== 0) {
          draggedEl.style.transform = `translate3d(${dx0}px, ${dy0}px, 0)`;
          void strip.offsetWidth;
          draggedEl.style.transition = DROP_TRANSITION;
          draggedEl.style.transform = "translate3d(0, 0, 0)";
          window.setTimeout(() => cleanupCardStyles(state.id), 240);
        } else {
          cleanupCardStyles(state.id);
        }
      } else {
        cleanupCardStyles(state.id);
      }
      if (changed && orderRef.current.join("\n") !== initialOrderRef.current.join("\n")) {
        reorderMutation.mutate(orderRef.current);
        initialOrderRef.current = orderRef.current;
      }
      updateScrollState();
      // eslint-disable-next-line react-hooks/exhaustive-deps -- reorderMutation/byId/updateScrollState 为稳定引用
    },
    [byId, reorderMutation, detachDragListeners, updateScrollState],
  );

  const finishRef = useRef(finishDrag);
  finishRef.current = finishDrag;

  const onWindowPointerUp = useCallback(() => {
    detachDragListeners();
    finishRef.current(true);
  }, [detachDragListeners]);

  const onWindowPointerCancel = useCallback(() => {
    detachDragListeners();
    finishRef.current(false);
  }, [detachDragListeners]);

  const scheduleFrameRef = useRef(scheduleFrame);
  scheduleFrameRef.current = scheduleFrame;

  const onCardPointerDown = (event: React.PointerEvent<HTMLDivElement>, id: string) => {
    if (event.button !== 0) return;
    const strip = stripRef.current;
    const draggedEl = cardRefs.current.get(id);
    const index = connected.findIndex((p) => p.providerId === id);
    if (!strip || index < 0 || !draggedEl || cardDrag.current) return;
    const stripRect = strip.getBoundingClientRect();
    const cardRect = draggedEl.getBoundingClientRect();
    // 实测卡宽与槽位步长（按下时全部卡片无 transform，测量可靠；邻居在左/右侧取绝对值）
    const cardWidth = cardRect.width;
    let step = CARD_STEP;
    const neighborId =
      connected[0]?.providerId === id ? connected[1]?.providerId : connected[0]?.providerId;
    const neighborEl = neighborId ? cardRefs.current.get(neighborId) : null;
    if (neighborEl) {
      step = Math.max(CARD_GAP + 1, Math.abs(Math.round(neighborEl.getBoundingClientRect().left - cardRect.left)));
    }
    cardDrag.current = {
      id,
      startIndex: index,
      startX: event.clientX,
      startY: event.clientY,
      clientX: event.clientX,
      clientY: event.clientY,
      pending: false,
      moved: false,
      target: index,
      grabOffsetX: event.clientX - cardRect.left,
      stripRectLeft: stripRect.left,
      stripRectRight: stripRect.right,
      startScrollLeft: strip.scrollLeft,
      scrollLeft: strip.scrollLeft,
      cardWidth,
      step,
      connected: orderRef.current.filter(
        (pid) => byId.get(pid)?.aggregateStatus !== "setup_required",
      ),
    };
    // window 原生监听为主路径；capture 仅作移出窗口时的兜底（WebView2 下 capture 可能被重排打断）
    window.addEventListener("pointermove", onWindowPointerMove);
    window.addEventListener("pointerup", onWindowPointerUp);
    window.addEventListener("pointercancel", onWindowPointerCancel);
    try {
      event.currentTarget.setPointerCapture(event.pointerId);
    } catch {
      // capture 失败不影响拖拽：window 监听已覆盖
    }
    scheduleFrameRef.current();
  };

  // 卸载兜底：组件销毁时清理监听与动画帧
  useEffect(() => {
    return () => {
      cardDrag.current = null;
      if (rafRef.current !== null) cancelAnimationFrame(rafRef.current);
      pendingFrame.current = false;
      detachDragListeners();
    };
  }, [detachDragListeners]);

  // —— 空白处拖拽滚动（卡内交互不触发排序，互不干扰）——
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
          description="接入平台后此处展示各账号的剩余额度与余额摘要。"
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
          <PlatformDeckCard
            key={platform.providerId}
            platform={platform}
            currentAccountId={currentAccountId[platform.providerId]}
            registerRef={(el) => {
              if (el) cardRefs.current.set(platform.providerId, el);
              else cardRefs.current.delete(platform.providerId);
            }}
            onCardPointerDown={(event) => onCardPointerDown(event, platform.providerId)}
            onSelectAccount={(accountId) =>
              setCurrentAccountId((prev) => ({ ...prev, [platform.providerId]: accountId }))
            }
            onOpenAll={() => onOpenPlatform(platform.providerId)}
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

function accountWindows(platform: PlatformSummaryViewModel, accountId: string): CapabilitySnapshotViewModel[] {
  return sortWindowCapabilities(
    platform.capabilities.filter(
      (capability) => capability.accountId === accountId && capability.capabilityId.startsWith("quota_window_"),
    ),
  );
}

function accountCapability(
  platform: PlatformSummaryViewModel,
  accountId: string,
  capabilityId: string,
): CapabilitySnapshotViewModel | null {
  return (
    platform.capabilities.find(
      (capability) =>
        capability.accountId === accountId && capability.capabilityId === capabilityId && capability.value.primary !== null,
    ) ?? null
  );
}

/**
 * 一个平台的账号卡组槽位：卡头拖拽带移动整组排序，卡头箭头切换账号（到端禁用），
 * 卡内文字与数值保持可选中。
 */
function PlatformDeckCard({
  platform,
  currentAccountId,
  registerRef,
  onCardPointerDown,
  onSelectAccount,
  onOpenAll,
}: {
  platform: PlatformSummaryViewModel;
  currentAccountId: string | undefined;
  registerRef: (el: HTMLDivElement | null) => void;
  onCardPointerDown: (event: React.PointerEvent<HTMLDivElement>) => void;
  onSelectAccount: (accountId: string) => void;
  onOpenAll: () => void;
}) {
  const accounts = platform.accounts;
  const currentIndex = Math.max(
    0,
    accounts.findIndex((account) => account.accountId === currentAccountId),
  );
  const current = accounts[currentIndex] ?? null;
  const multiAccount = accounts.length > 1 && current !== null;

  return (
    <div
      ref={registerRef}
      data-strip-card
      className="relative shrink-0"
      style={{ width: CARD_WIDTH, height: CARD_HEIGHT }}
    >
      {current && (
        <div
          className="strip-card-face glass-panel absolute flex flex-col p-4"
          style={{ left: 0, top: 0, width: CARD_WIDTH, height: CARD_HEIGHT, borderRadius: 16 }}
        >
          <AccountCardBody
            platform={platform}
            account={current}
            index={currentIndex}
            total={accounts.length}
            multiAccount={multiAccount}
            onCardPointerDown={onCardPointerDown}
            onSelectAccount={onSelectAccount}
            onOpenAll={onOpenAll}
          />
        </div>
      )}
    </div>
  );
}

/** 账户卡内容：卡头（logo + 名称 + 中间拖拽带 + 账号切换）→ 账号别名 → 窗口剩余额度 → 余额块 → 查看全部账户 */
function AccountCardBody({
  platform,
  account,
  index,
  total,
  multiAccount,
  onCardPointerDown,
  onSelectAccount,
  onOpenAll,
}: {
  platform: PlatformSummaryViewModel;
  account: AccountSummaryViewModel;
  index: number;
  total: number;
  multiAccount: boolean;
  onCardPointerDown: (event: React.PointerEvent<HTMLDivElement>) => void;
  onSelectAccount: (accountId: string) => void;
  onOpenAll: () => void;
}) {
  const windows = accountWindows(platform, account.accountId);
  const balance = accountCapability(platform, account.accountId, "balance");
  const totalSpend = accountCapability(platform, account.accountId, "total_spend");

  return (
    <>
      {/* 卡头：只有名称与箭头之间的拖拽带发起整组拖拽，翻页按钮与文字不受影响 */}
      <div className="flex items-center gap-1.5">
        <PlatformMark providerId={platform.providerId} size={30} />
        <span
          className="min-w-0 max-w-[45%] shrink truncate text-[13px] font-semibold text-q-text-primary"
          title={platform.displayName}
        >
          {platform.displayName}
        </span>
        <div
          role="button"
          aria-label={`拖动排序 ${platform.displayName}`}
          title="拖动排序"
          onPointerDown={onCardPointerDown}
          className="flex h-7 min-w-0 flex-1 cursor-grab touch-none items-center justify-center rounded-[8px] text-q-text-muted/60 transition-colors hover:bg-q-primary-softer hover:text-q-text-secondary active:cursor-grabbing"
        >
          <GripVertical size={15} aria-hidden />
        </div>
        {multiAccount && (
          <span className="flex shrink-0 items-center gap-0.5">
            <button
              type="button"
              aria-label="上一个账号"
              disabled={index === 0}
              onClick={() => onSelectAccount(platform.accounts[index - 1].accountId)}
              className="flex h-6 w-6 cursor-pointer items-center justify-center rounded-full text-q-text-secondary transition-colors hover:bg-q-primary-softer hover:text-q-primary disabled:cursor-default disabled:opacity-30 disabled:hover:bg-transparent"
            >
              <ChevronLeft size={14} aria-hidden />
            </button>
            <span className="min-w-[24px] text-center text-[11px] tabular-nums text-q-text-muted">
              {index + 1}/{total}
            </span>
            <button
              type="button"
              aria-label="下一个账号"
              disabled={index >= total - 1}
              onClick={() => onSelectAccount(platform.accounts[index + 1].accountId)}
              className="flex h-6 w-6 cursor-pointer items-center justify-center rounded-full text-q-text-secondary transition-colors hover:bg-q-primary-softer hover:text-q-primary disabled:cursor-default disabled:opacity-30 disabled:hover:bg-transparent"
            >
              <ChevronRight size={14} aria-hidden />
            </button>
          </span>
        )}
      </div>

      {/* 账号别名 + 不可变类型标签 */}
      <div className="mt-2 flex min-w-0 items-center gap-1.5">
        <span className="min-w-0 truncate text-[12px] font-medium text-q-text-primary" title={account.displayName}>
          {account.displayName}
        </span>
        <span className="shrink-0 rounded-q-pill bg-q-neutral-soft px-1.5 py-px text-[10px] font-medium text-q-text-secondary">
          {ACCOUNT_KIND_LABEL[account.kind]}
        </span>
      </div>

      {/* 卡身：窗口剩余额度（5h/7d/30d/其他），missing 灰轨道「未获取」；纯余额账号不制造比例 */}
      <div className="mt-2 flex min-h-0 flex-1 flex-col gap-2 overflow-hidden">
        {windows.length > 0 ? (
          windows.map((capability) => (
            <div key={capability.capabilityId} className="flex min-w-0 flex-col gap-1">
              <QuotaProgress capability={capability} label={windowShortLabel(capability)} />
              <WindowFootnote capability={capability} />
            </div>
          ))
        ) : balance ? (
          <BalanceBlock balance={balance} totalSpend={totalSpend} large />
        ) : (
          <p className="flex flex-1 items-center text-[11px] leading-relaxed text-q-text-muted">
            暂无该账号的额度数据，刷新后展示。
          </p>
        )}
        {windows.length > 0 && balance && (
          <div className="mt-auto">
            <BalanceBlock balance={balance} totalSpend={totalSpend} />
          </div>
        )}
      </div>

      {/* 卡底：进入平台中心额度与用量并定位该平台 */}
      <button
        type="button"
        onClick={onOpenAll}
        className="mt-2 shrink-0 cursor-pointer text-center text-[11px] font-medium text-q-primary transition-colors hover:text-q-primary-hover"
      >
        查看全部账户
      </button>
    </>
  );
}

/** 窗口行的辅助小字：stale 显示「缓存 · 上次成功」，其余展示后端重置说明；missing 不显示。 */
function WindowFootnote({ capability }: { capability: CapabilitySnapshotViewModel }) {
  if (capability.freshness === "missing") return null;
  if (capability.freshness === "stale") {
    return (
      <p className="truncate pl-[52px] text-[10px] leading-3.5 text-q-warning">
        缓存 · 上次成功 {formatTime(capability.lastGoodAt ?? capability.capturedAt)}
      </p>
    );
  }
  if (capability.value.secondary) {
    return (
      <p className="truncate pl-[52px] text-[10px] leading-3.5 text-q-text-muted" title={capability.value.secondary}>
        {capability.value.secondary}
      </p>
    );
  }
  return null;
}

/** 余额块：有窗口账号的紧凑版 / 纯余额账号的大号版；只展示后端金额文本，不画比例。 */
function BalanceBlock({
  balance,
  totalSpend,
  large = false,
}: {
  balance: CapabilitySnapshotViewModel;
  totalSpend: CapabilitySnapshotViewModel | null;
  large?: boolean;
}) {
  return (
    <div className="rounded-[10px] border border-q-border bg-q-surface-muted/60 px-3 py-2">
      <p className="text-[11px] text-q-text-muted">{balance.displayName}</p>
      <p
        className={cn(
          "truncate font-bold leading-7 tabular-nums text-q-text-primary",
          large ? "text-[24px]" : "text-[16px]",
        )}
        data-selectable="true"
      >
        {compactPercentText(balance.value.primary ?? "")}
      </p>
      {totalSpend && (
        <p className="truncate text-[11px] font-semibold tabular-nums text-q-text-primary" data-selectable="true">
          {totalSpend.displayName} {compactPercentText(totalSpend.value.primary ?? "")}
        </p>
      )}
    </div>
  );
}
