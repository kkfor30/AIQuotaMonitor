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
import { getCurrentWindow } from "@tauri-apps/api/window";
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

export function HoverbarAnchorApp() {
  const [anchor, setAnchor] = useState<HoverbarAnchor>({ edge: "right", ratio: 0.4 });
  const [detailVisible, setDetailVisible] = useState(false);
  const enterTimer = useRef<number | undefined>(undefined);
  const collapseTimer = useRef<number | undefined>(undefined);
  const dragging = useRef(false);
  const dragMoved = useRef(false);
  const dragFinishedAt = useRef(0);

  // 监听平台聚合状态，驱动小球被动呼吸微光晕
  const { data: platforms = [] } = useQuery({
    queryKey: PLATFORM_SUMMARIES_QUERY_KEY,
    queryFn: fetchPlatformSummaries,
    staleTime: 10000,
  });

  const alertTone: "error" | "warning" | null = (() => {
    let hasError = false;
    let hasLowQuota = false;
    for (const p of platforms) {
      if (p.aggregateStatus === "error") {
        hasError = true;
        break;
      }
      if (p.aggregateStatus === "partial") {
        hasLowQuota = true;
      }
      for (const c of p.capabilities) {
        const rem = capabilityRemainingPercent(c);
        if (rem !== null && rem <= 15) {
          hasLowQuota = true;
        }
      }
    }
    if (hasError) return "error";
    if (hasLowQuota) return "warning";
    return null;
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

  const snapToEdge = useCallback(async () => {
    const next = await invoke<HoverbarAnchor>("snap_hoverbar_to_edge");
    const normalized = normalizeHoverbarAnchor(next);
    setAnchor(normalized);
    return normalized;
  }, []);

  const showDetail = useCallback(() => {
    if (dragging.current) return;
    clearEnterTimer();
    clearCollapseTimer();
    void invoke<HoverbarAnchor>("show_hoverbar_detail")
      .then((next) => {
        setAnchor(normalizeHoverbarAnchor(next));
        setDetailVisible(true);
      })
      .catch((error) => console.error("无法展开悬浮详情", error));
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
    clearEnterTimer();
    clearCollapseTimer();
    collapseTimer.current = window.setTimeout(requestHide, HOVERBAR_LEAVE_DELAY_MS);
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
      setDetailVisible(event.payload);
      if (!event.payload) clearCollapseTimer();
    }).then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
    void listen<boolean>("hoverbar-detail-pointer", (event) => {
      if (disposed) return;
      if (event.payload) clearCollapseTimer();
      else scheduleHide();
    }).then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [clearCollapseTimer, scheduleHide]);

  // onMoved 标记真实位移：区分「点击展开」与「拖动后吸附」
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWindow()
      .onMoved(() => {
        if (!dragging.current) return;
        dragMoved.current = true;
      })
      .then((nextUnlisten) => {
        if (disposed) nextUnlisten();
        else unlisten = nextUnlisten;
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const activateOrb = useCallback(() => {
    if (dragMoved.current || dragging.current) return;
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
      const detailWasVisible = detailVisible;
      clearEnterTimer();
      clearCollapseTimer();
      if (detailVisible) requestHide();
      dragging.current = true;
      dragMoved.current = false;
      void getCurrentWindow()
        .startDragging()
        .catch((error) => console.error("无法拖动悬浮球", error))
        .finally(() => {
          void snapToEdge()
            .catch((error) => console.error("无法吸附悬浮球", error))
            .finally(() => {
              const moved = dragMoved.current;
              dragging.current = false;
              dragMoved.current = false;
              if (moved) {
                dragFinishedAt.current = Date.now();
                // 拖拽完成时，若用户光标仍停留在小球上，直接唤醒悬停展开倒计时
                if (isPointerOver.current) {
                  scheduleOpen(HOVERBAR_ENTER_DELAY_MS);
                }
              } else if (!detailWasVisible) {
                showDetail();
              }
            });
        });
    },
    [clearCollapseTimer, clearEnterTimer, detailVisible, requestHide, scheduleOpen, showDetail, snapToEdge],
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
        if (e.pointerType === "mouse" || e.isPrimary) {
          isPointerOver.current = true;
          clearCollapseTimer();
          if (!detailVisible && !dragging.current) {
            scheduleOpen(HOVERBAR_ENTER_DELAY_MS);
          }
        }
      }}
      onPointerMove={(e) => {
        if (e.pointerType === "mouse" || e.isPrimary) {
          isPointerOver.current = true;
          if (!detailVisible && !dragging.current && !enterTimer.current) {
            scheduleOpen(HOVERBAR_ENTER_DELAY_MS);
          }
        }
      }}
      onPointerLeave={() => {
        isPointerOver.current = false;
        if (detailVisible) scheduleHide();
        else clearEnterTimer();
      }}
      onContextMenu={handleContextMenu}
    >
      <HoverbarOrb
        edge={anchor.edge}
        active={detailVisible}
        alertTone={alertTone}
        ariaLabel={detailVisible ? "收起额度详情并拖动" : "打开额度详情"}
        onActivate={activateOrb}
        onPointerDown={startDragging}
        onContextMenu={handleContextMenu}
      />
    </div>
  );
}

/** 40x40 透明窗口内的 32px 动态玻璃小球。 */
export function HoverbarOrb({
  edge,
  active,
  alertTone,
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
  ariaLabel: string;
  onActivate: () => void;
  onPointerDown: (event: React.PointerEvent<HTMLButtonElement>) => void;
  onContextMenu?: (event: React.MouseEvent) => void;
  disabled?: boolean;
  forceState?: "hover" | "focus" | "active";
}) {
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
        alertTone && !active ? ` is-alert-${alertTone}` : ""
      }${forceState ? ` is-${forceState}` : ""}`}
      data-edge={edge}
      data-alert={alertTone && !active ? alertTone : undefined}
    >
      <picture>
        <source media="(prefers-reduced-motion: reduce)" srcSet="/assets/hover-orb-poster.png" />
        <img src="/assets/hover-orb.webp" alt="" draggable={false} />
      </picture>
    </button>
  );
}
