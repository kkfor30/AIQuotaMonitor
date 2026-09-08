/**
 * 悬浮球锚点窗口应用（40x40 透明置顶窗口）。
 *
 * 迁移来源：DeepSeek-Monitor-Windows/DeepSeekMonitorWindows/src/main.tsx
 * 的 HoverbarAnchorApp / HoverbarOrb（提交 af6cfe07，MIT，约 602-862 行）。
 * 迁移内容：悬停延迟展开、离开延迟收起、startDragging 原生拖动、
 * onMoved 区分点击与拖动、拖动后抑制窗口、detail-visibility/pointer 事件同步。
 * 变更：保留旧版动画 WebP 与 reduced-motion 静态海报；invoke 调用改为
 * 当前项目 IPC 契约，锚点与详情仍使用独立窗口；增加被动异常/低额度呼吸微光晕
 * 与 Windows 原生右键自适应微菜单。
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { fetchHoverbarPreferences, fetchPlatformSummaries } from "@/lib/ipc";
import { PLATFORM_SUMMARIES_QUERY_KEY } from "@/lib/query-client";
import { capabilityRemainingPercent } from "@/components/ui/QuotaProgress";
import {
  HOVERBAR_DRAG_SUPPRESS_MS,
  HOVERBAR_ENTER_DELAY_MS,
  HOVERBAR_LEAVE_DELAY_MS,
  normalizeHoverbarAnchor,
  type HoverbarAnchor,
} from "./hoverbar-state";
import { preventNativeAssetDrag } from "./hoverbar-interaction";

/** 悬浮小球动态语义光效类型。 */
export type OrbGlowType = "normal" | "warning" | "active" | "danger" | "stale" | "verifying";

export function HoverbarAnchorApp() {
  const [anchor, setAnchor] = useState<HoverbarAnchor>({ edge: "right", ratio: 0.4 });
  const [detailVisible, setDetailVisible] = useState(false);
  const showDetailSeqRef = useRef(0);
  const enterTimer = useRef<number | undefined>(undefined);
  const collapseTimer = useRef<number | undefined>(undefined);
  const dragging = useRef(false);
  const dragFinishedAt = useRef(0);
  const detailPointerInsideRef = useRef(false);

  // 监听平台聚合状态与重置雷达
  const { data: platforms = [] } = useQuery({
    queryKey: PLATFORM_SUMMARIES_QUERY_KEY,
    queryFn: fetchPlatformSummaries,
    staleTime: 10000,
  });


  // 小球告警状态：红圈严格只在 API Key 失效或 401 等异常阻断性故障（aggregateStatus === "error"）时出现，不参与额度情况；额度消耗与告罄由 warning 承接
  const alertTone: "error" | "warning" | null = (() => {
    let hasDanger = false;
    let hasWarning = false;
    for (const p of platforms) {
      if (p.aggregateStatus === "error") hasDanger = true;
      if (p.aggregateStatus === "partial") hasWarning = true;
      for (const c of p.capabilities) {
        const rem = capabilityRemainingPercent(c);
        if (rem !== null && rem <= 15) hasWarning = true;
      }
    }
    if (hasDanger) return "error";
    if (hasWarning) return "warning";
    return null;
  })();

  const orbGlow: OrbGlowType = (() => {
    if (alertTone === "error") return "danger";
    if (alertTone === "warning") return "warning";
    const allCapabilities = platforms.flatMap((p) => p.capabilities);
    if (allCapabilities.length > 0 && allCapabilities.every((c) => c.freshness === "stale")) return "stale";
    return "normal";
  })();

  const isPointerOver = useRef(false);

  const clearEnterTimer = useCallback(() => {
    window.clearTimeout(enterTimer.current);
    enterTimer.current = undefined;
  }, []);
  const clearCollapseTimer = useCallback(() => {
    window.clearTimeout(collapseTimer.current);
    collapseTimer.current = undefined;
  }, []);

  const showDetail = useCallback(() => {
    if (dragging.current) return;
    const reqSeq = ++showDetailSeqRef.current;
    clearEnterTimer();
    clearCollapseTimer();
    void invoke<HoverbarAnchor>("show_hoverbar_detail")
      .then((next) => {
        if (reqSeq !== showDetailSeqRef.current || dragging.current) {
          // 拖动中由后端负责隐藏；松手后才清理迟到的展开回复。
          if (!dragging.current) void invoke("hide_hoverbar_detail_immediately").catch(() => {});
          return;
        }
        setAnchor(normalizeHoverbarAnchor(next));
        setDetailVisible(true);
      })
      .catch((error) => {
        if (!String(error).includes("正在拖拽")) {
          console.error("无法展开悬浮详情", error);
        }
      });
  }, [clearCollapseTimer, clearEnterTimer]);

  const scheduleOpen = useCallback(
    (delay: number = HOVERBAR_ENTER_DELAY_MS) => {
      clearEnterTimer();
      clearCollapseTimer();
      enterTimer.current = window.setTimeout(() => {
        enterTimer.current = undefined;
        if (!isPointerOver.current || detailVisible || dragging.current) return;
        const elapsedSinceDrag = Date.now() - dragFinishedAt.current;
        if (elapsedSinceDrag < HOVERBAR_DRAG_SUPPRESS_MS) {
          const remaining = HOVERBAR_DRAG_SUPPRESS_MS - elapsedSinceDrag;
          // 拖拽抑制期未完全过去，不直接放弃，而是安排补跑剩余毫秒
          enterTimer.current = window.setTimeout(() => {
            enterTimer.current = undefined;
            if (isPointerOver.current && !detailVisible && !dragging.current) {
              showDetail();
            }
          }, remaining + 16);
          return;
        }
        showDetail();
      }, delay);
    },
    [clearCollapseTimer, clearEnterTimer, detailVisible, showDetail],
  );

  const requestHide = useCallback(() => {
    clearEnterTimer();
    clearCollapseTimer();
    void invoke("request_hide_hoverbar_detail").catch((error) =>
      console.error("无法收起悬浮详情", error),
    );
  }, [clearCollapseTimer, clearEnterTimer]);

  const scheduleHide = useCallback(() => {
    if (dragging.current) return;
    clearEnterTimer();
    clearCollapseTimer();
    collapseTimer.current = window.setTimeout(() => {
      // 收起前二次确认：若光标仍在详情卡片内、悬浮球上或正在拖拽，放弃收起
      if (detailPointerInsideRef.current || isPointerOver.current || dragging.current) {
        return;
      }
      requestHide();
    }, HOVERBAR_LEAVE_DELAY_MS);
  }, [clearCollapseTimer, clearEnterTimer, requestHide]);

  // 初始锚点来自持久化偏好
  useEffect(() => {
    void fetchHoverbarPreferences()
      .then((prefs) => setAnchor(normalizeHoverbarAnchor(prefs.anchor)))
      .catch((error) => console.error("无法加载悬浮球锚点", error));
  }, []);

  // 监听吸附事件广播（全窗口实时同步锚点）
  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    void listen<HoverbarAnchor>("hoverbar-anchor-changed", (event) => {
      if (disposed) return;
      setAnchor(normalizeHoverbarAnchor(event.payload));
    }).then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, []);

  // 详情可见性与指针进出事件（与详情窗口联动）
  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    void listen<boolean>("hoverbar-detail-visibility", (event) => {
      if (disposed) return;
      if (dragging.current && event.payload) return;
      setDetailVisible(event.payload);
      if (!event.payload) {
        detailPointerInsideRef.current = false;
        clearCollapseTimer();
      }
    }).then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
    void listen<boolean>("hoverbar-detail-pointer", (event) => {
      if (disposed) return;
      detailPointerInsideRef.current = event.payload;
      if (event.payload) clearCollapseTimer();
      else scheduleHide();
    }).then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [clearCollapseTimer, scheduleHide]);

  const activateOrb = useCallback((event?: React.MouseEvent<HTMLButtonElement>) => {
    // 鼠标点击由完整拖动事务处理；只保留键盘激活和菜单调用，避免松手后二次切换。
    if ((event && event.detail !== 0) || dragging.current) return;
    if (detailVisible) requestHide();
    else showDetail();
  }, [detailVisible, requestHide, showDetail]);

  // 响应来自托盘或原生右键菜单的「展开/收起详情」指令
  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    void listen("hoverbar-action-toggle-detail", () => {
      if (disposed) return;
      activateOrb();
    }).then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [activateOrb]);

  const startDragging = useCallback(
    (event: React.PointerEvent<HTMLButtonElement>) => {
      if (event.button !== 0 || dragging.current) return;
      event.preventDefault();
      event.stopPropagation();

      const detailWasVisible = detailVisible;
      showDetailSeqRef.current++;
      clearEnterTimer();
      clearCollapseTimer();

      // 单一后端事务接管抓取点、隐藏与原生拖动，不并发发送窗口控制请求。
      dragging.current = true;
      setDetailVisible(false);
      detailPointerInsideRef.current = false;
      void invoke<{ anchor: HoverbarAnchor; moved: boolean }>("drag_hoverbar", {
        pointerX: event.clientX,
        pointerY: event.clientY,
      })
        .then(({ anchor: next, moved }) => {
          setAnchor(normalizeHoverbarAnchor(next));
          dragging.current = false;
          dragFinishedAt.current = Date.now();
          clearEnterTimer();
          if (!moved && !detailWasVisible) showDetail();
        })
        .catch((error) => console.error("无法拖动悬浮球", error))
        .finally(() => {
          dragging.current = false;
        });
    },
    [clearCollapseTimer, clearEnterTimer, detailVisible, showDetail],
  );

  const handleContextMenu = useCallback((event: React.MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
  }, []);

  useEffect(
    () => () => {
      clearEnterTimer();
      clearCollapseTimer();
    },
    [clearCollapseTimer, clearEnterTimer],
  );

  return (
    <div
      className="hoverbar-anchor-root relative h-[40px] w-[40px] select-none"
      data-edge={anchor.edge}
      data-state={detailVisible ? "expanded" : "anchor"}
      onPointerEnter={(e) => {
        if (dragging.current) return;
        if (e.pointerType === "mouse" || e.isPrimary) {
          isPointerOver.current = true;
          clearCollapseTimer();
          if (!detailVisible) {
            scheduleOpen(HOVERBAR_ENTER_DELAY_MS);
          }
        }
      }}
      onPointerMove={(e) => {
        if (dragging.current) return;
        if (e.pointerType === "mouse" || e.isPrimary) {
          isPointerOver.current = true;
          if (!detailVisible && !enterTimer.current) {
            scheduleOpen(HOVERBAR_ENTER_DELAY_MS);
          }
        }
      }}
      onPointerLeave={() => {
        isPointerOver.current = false;
        if (dragging.current) {
          clearEnterTimer();
          clearCollapseTimer();
          return;
        }
        if (detailVisible) {
          // 若鼠标已在详情卡片内，绝不误启动收起倒计时
          if (!detailPointerInsideRef.current) {
            scheduleHide();
          }
        } else {
          clearEnterTimer();
        }
      }}
      onContextMenu={handleContextMenu}
    >
      <HoverbarOrb
        edge={anchor.edge}
        active={detailVisible}
        alertTone={alertTone}
        glow={orbGlow}
        ariaLabel={detailVisible ? "收起额度详情并拖动" : "打开额度详情"}
        onActivate={activateOrb}
        onPointerDown={startDragging}
        onContextMenu={handleContextMenu}
      />
    </div>
  );
}

/** 40x40 透明窗口内的 32px 动态玻璃小球（方案 D：深空悬浮透底投影）。 */
export function HoverbarOrb({
  edge,
  active,
  alertTone,
  glow,
  ariaLabel,
  onActivate,
  onPointerDown,
  onContextMenu,
  disabled = false,
  forceState,
}: {
  edge: string;
  active: boolean;
  alertTone?: "error" | "warning" | null;
  glow?: OrbGlowType;
  ariaLabel: string;
  onActivate: () => void;
  onPointerDown: (event: React.PointerEvent<HTMLButtonElement>) => void;
  onContextMenu?: (event: React.MouseEvent) => void;
  disabled?: boolean;
  forceState?: "hover" | "focus" | "active";
}) {
  const effectiveGlow: OrbGlowType =
    glow ??
    (alertTone === "error"
      ? "danger"
      : alertTone === "warning"
      ? "warning"
      : "normal");

  return (
    <button
      type="button"
      aria-label={ariaLabel}
      disabled={disabled}
      onDragStart={preventNativeAssetDrag}
      onClick={onActivate}
      onPointerDown={onPointerDown}
      onContextMenu={onContextMenu}
      className={`hb-orb${active ? " is-detail-open" : ""}${
        effectiveGlow ? ` is-glow-${effectiveGlow}` : ""
      }${forceState ? ` is-${forceState}` : ""}`}
      data-edge={edge}
      data-glow={effectiveGlow}
      data-alert={alertTone && !active ? alertTone : undefined}
    >
      <div className="hb-ambient-bloom" />
      <picture>
        <source media="(prefers-reduced-motion: reduce)" srcSet="/assets/hover-orb-poster.png" />
        <img src="/assets/hover-orb.webp" alt="" draggable={false} />
      </picture>
    </button>
  );
}
