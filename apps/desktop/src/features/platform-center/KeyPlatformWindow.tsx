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
/** 账号类型徽章（本机/默认/额外），V7 中所有账号别名可见 */
const ACCOUNT_KIND_LABEL: Record<AccountKind, string> = {
  local: "本机",
  default: "默认",
  additional: "额外",
};

const SHIFT_TRANSITION = "transform 200ms cubic-bezier(0.2, 0.78, 0.24, 1)";
const DROP_TRANSITION = "transform 220ms cubic-bezier(0.2, 0.78, 0.24, 1)";

type CardDragState = {
  id: string;
  /** 拖拽开始时该卡所在槽位（锚点，拖拽期间不变化） */
  startIndex: number;
  /** 拖拽开始时该卡的视觉左缘（指针锚定基准） */
  anchorLeft: number;
  startX: number;
  startY: number;
  /** 最新指针位置；pointermove 只写坐标，DOM 更新统一在 rAF 帧里做 */
  clientX: number;
  clientY: number;
  pending: boolean;
  moved: boolean;
  /** 实时换位目标槽位；期间只写 DOM transform，不触发 React 渲染 */
  target: number;
  /** 条带几何缓存：拖拽全程不再读 getBoundingClientRect（避免强制布局） */
  stripLeft: number;
  stripRight: number;
  scrollLeft: number;
  /** 拖拽期间固定不变的 connected 平台 id 顺序 */
  connected: string[];
};

/**
 * 关键平台横向窗口（V7 总览设计稿 01）：
 * - 一个 Platform 永远占一个排序槽位；槽位内部按 platform.accounts 构建账号卡组，
 *   前卡显示当前账号，箭头 1/2 切换（到端禁用），前端按平台记忆当前账号。
 * - 拖拽：按住卡身任意空白处即可拖动整组（卡内按钮不受影响）。指针事件走 window
 *   原生监听 + pointer capture 双保险，DOM 更新在 rAF 帧里做，拖拽全程零 React
 *   渲染（拖拽态为命令式 class），松手一次性提交顺序并 FLIP 落位。
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
    // 边缘自动滚动改变 scrollLeft 时，同步拖拽中的几何缓存
    if (cardDrag.current) cardDrag.current.scrollLeft = el.scrollLeft;
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

  // —— 卡身拖拽排序：window 原生监听 + rAF 帧同步，拖拽期间零 React 渲染 ——
  const applyShifts = (state: CardDragState, nextTarget: number) => {
    const prevTarget = state.target;
    if (nextTarget === prevTarget) return;
    const lo = Math.min(prevTarget, nextTarget);
    const hi = Math.max(prevTarget, nextTarget);
    for (let i = lo; i <= hi; i++) {
      const id = state.connected[i];
      if (id === undefined || id === state.id) continue;
      const el = cardRefs.current.get(id);
      if (!el) continue;
      const shift = i > state.startIndex ? -CARD_STEP : i < state.startIndex ? CARD_STEP : 0;
      el.style.transition = shift !== 0 ? SHIFT_TRANSITION : "none";
      el.style.transform = shift !== 0 ? `translate(${shift}px, 0)` : "";
    }
    state.target = nextTarget;
  };

  /** 每帧最多一次的拖拽处理：跟手 transform、实时换位、边缘自动滚动 */
  const dragFrame = useCallback(() => {
    const state = cardDrag.current;
    if (!state) return;
    if (!state.pending) {
      rafRef.current = requestAnimationFrame(dragFrame);
      return;
    }
    state.pending = false;
    const strip = stripRef.current;
    const draggedEl = cardRefs.current.get(state.id);
    if (!strip || !draggedEl) return;
    const dx = state.clientX - state.startX;
    const dy = state.clientY - state.startY;
    if (!state.moved) {
      if (Math.abs(dx) < 5 && Math.abs(dy) < 5) return;
      state.moved = true;
      draggedEl.classList.add("strip-card-dragging");
    }

    // 实时换位：被拖卡中心越过相邻卡中心即更新目标；只写 transform，不渲染。
    // 让位卡（(startIndex, target] 左移一位、[target, startIndex) 右移一位）的判定中心同样要加上自身偏移。
    const draggedLeft = state.anchorLeft + dx;
    const draggedCenter = draggedLeft + CARD_WIDTH / 2;
    let target = state.target;
    for (let i = 0; i < state.connected.length; i++) {
      if (i === state.target) continue;
      const shift =
        i > state.startIndex && i <= state.target
          ? -CARD_STEP
          : i < state.startIndex && i >= state.target
            ? CARD_STEP
            : 0;
      const center = state.stripLeft + 2 + i * CARD_STEP - state.scrollLeft + CARD_WIDTH / 2 + shift;
      if (i > state.target && draggedCenter > center) target = Math.max(target, i);
      else if (i < state.target && draggedCenter < center) target = Math.min(target, i);
    }
    if (target !== state.target) applyShifts(state, target);

    // 被拖卡即时跟随指针（每帧一次 style 写入）
    const base = state.stripLeft + 2 + state.target * CARD_STEP - state.scrollLeft;
    draggedEl.style.transition = "none";
    draggedEl.style.transform = `translate(${Math.round(draggedLeft - base)}px, ${Math.max(-10, Math.min(10, dy))}px)`;
    draggedEl.style.zIndex = "30";

    // 拖近窗口边缘时自动滚动；滚动量经 scroll 事件同步进几何缓存
    if (state.clientX < state.stripLeft + 80) strip.scrollLeft -= 16;
    else if (state.clientX > state.stripRight - 80) strip.scrollLeft += 16;
    state.scrollLeft = strip.scrollLeft;
    rafRef.current = requestAnimationFrame(dragFrame);
  }, []);

  const settleCardDrag = useCallback(() => {
    const state = cardDrag.current;
    cardDrag.current = null;
    if (rafRef.current !== null) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
    if (!state || !state.moved) return;
    const strip = stripRef.current;
    const draggedEl = cardRefs.current.get(state.id);
    if (draggedEl) draggedEl.classList.remove("strip-card-dragging");
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
      el.style.zIndex = "";
    }
    // 被拖卡从当前视觉位置平滑滑入所属槽位
    if (strip && draggedEl) {
      const from = draggedEl.getBoundingClientRect();
      draggedEl.style.transform = "";
      draggedEl.style.zIndex = "";
      const dx0 = Math.round(from.left - draggedEl.getBoundingClientRect().left);
      const dy0 = Math.round(from.top - draggedEl.getBoundingClientRect().top);
      if (dx0 !== 0 || dy0 !== 0) {
        draggedEl.style.transform = `translate(${dx0}px, ${dy0}px)`;
        void strip.offsetWidth;
        draggedEl.style.transition = DROP_TRANSITION;
        draggedEl.style.transform = "";
        window.setTimeout(() => {
          draggedEl.style.transition = "";
        }, 230);
      }
    }
    if (changed && orderRef.current.join("\n") !== initialOrderRef.current.join("\n")) {
      reorderMutation.mutate(orderRef.current);
      initialOrderRef.current = orderRef.current;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reorderMutation/byId 为稳定引用或仅读 ref
  }, [byId, reorderMutation]);

  const settleRef = useRef(settleCardDrag);
  settleRef.current = settleCardDrag;

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
  }, []);

  const onWindowPointerUp = useCallback(() => {
    detachDragListeners();
    settleRef.current();
  }, [detachDragListeners]);

  const onWindowPointerCancel = useCallback(() => {
    // 取消与松手同路径：顺序已实时生效，保持并落位
    onWindowPointerUp();
  }, [onWindowPointerUp]);

  const onCardPointerDown = (event: React.PointerEvent<HTMLDivElement>, id: string) => {
    if (event.button !== 0) return;
    // 卡内交互控件（箭头/查看全部账户等）不发起拖拽
    if ((event.target as HTMLElement).closest("button, a, input, select, textarea")) return;
    const strip = stripRef.current;
    const draggedEl = cardRefs.current.get(id);
    const index = connected.findIndex((p) => p.providerId === id);
    if (!strip || index < 0 || !draggedEl || cardDrag.current) return;
    const rect = strip.getBoundingClientRect();
    cardDrag.current = {
      id,
      startIndex: index,
      anchorLeft: draggedEl.getBoundingClientRect().left,
      startX: event.clientX,
      startY: event.clientY,
      clientX: event.clientX,
      clientY: event.clientY,
      pending: false,
      moved: false,
      target: index,
      stripLeft: rect.left,
      stripRight: rect.right,
      scrollLeft: strip.scrollLeft,
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
    rafRef.current = requestAnimationFrame(dragFrame);
  };

  // 卸载兜底：组件销毁时清理监听与动画帧
  useEffect(() => {
    return () => {
      cardDrag.current = null;
      if (rafRef.current !== null) cancelAnimationFrame(rafRef.current);
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
 * 一个平台的账号卡组槽位：按住卡身任意空白处拖动整组排序（卡内按钮除外），
 * 卡头箭头切换账号（到端禁用）。
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
      onPointerDown={onCardPointerDown}
      className="relative shrink-0 cursor-grab select-none"
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
            onSelectAccount={onSelectAccount}
            onOpenAll={onOpenAll}
          />
        </div>
      )}
    </div>
  );
}

/** 账户卡内容：卡头（手柄提示 + 账号切换）→ 账号别名 → 窗口剩余额度 → 余额块 → 查看全部账户 */
function AccountCardBody({
  platform,
  account,
  index,
  total,
  multiAccount,
  onSelectAccount,
  onOpenAll,
}: {
  platform: PlatformSummaryViewModel;
  account: AccountSummaryViewModel;
  index: number;
  total: number;
  multiAccount: boolean;
  onSelectAccount: (accountId: string) => void;
  onOpenAll: () => void;
}) {
  const windows = accountWindows(platform, account.accountId);
  const balance = accountCapability(platform, account.accountId, "balance");
  const totalSpend = accountCapability(platform, account.accountId, "total_spend");

  return (
    <>
      {/* 卡头：手柄为拖拽提示（整卡可拖）；多账号时箭头到端禁用 */}
      <div className="flex items-center gap-2">
        <PlatformMark providerId={platform.providerId} size={30} />
        <span className="min-w-0 flex-1 truncate text-[13px] font-semibold text-q-text-primary">
          {platform.displayName}
        </span>
        <span
          aria-hidden
          title="拖动排序"
          className="flex h-6 w-6 items-center justify-center rounded-[7px] text-q-text-muted/70 transition-colors hover:bg-q-primary-softer hover:text-q-text-secondary"
        >
          <GripVertical size={14} />
        </span>
        {multiAccount && (
          <span className="flex shrink-0 items-center gap-0.5">
            <button
              type="button"
              aria-label="上一个账号"
              disabled={index === 0}
              onClick={() => onSelectAccount(platform.accounts[index - 1].accountId)}
              className="flex h-5 w-5 cursor-pointer items-center justify-center rounded-full text-q-text-secondary transition-colors hover:bg-q-primary-softer hover:text-q-primary disabled:cursor-default disabled:opacity-30 disabled:hover:bg-transparent"
            >
              <ChevronLeft size={13} aria-hidden />
            </button>
            <span className="min-w-[24px] text-center text-[11px] tabular-nums text-q-text-muted">
              {index + 1}/{total}
            </span>
            <button
              type="button"
              aria-label="下一个账号"
              disabled={index >= total - 1}
              onClick={() => onSelectAccount(platform.accounts[index + 1].accountId)}
              className="flex h-5 w-5 cursor-pointer items-center justify-center rounded-full text-q-text-secondary transition-colors hover:bg-q-primary-softer hover:text-q-primary disabled:cursor-default disabled:opacity-30 disabled:hover:bg-transparent"
            >
              <ChevronRight size={13} aria-hidden />
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
