/**
 * 悬浮球锚点窗口应用（40x40 透明置顶窗口）。
 *
 * 迁移来源：DeepSeek-Monitor-Windows/DeepSeekMonitorWindows/src/main.tsx
 * 的 HoverbarAnchorApp / HoverbarOrb（提交 af6cfe07，MIT，约 602-862 行）。
 * 迁移内容：悬停延迟展开、离开延迟收起、startDragging 原生拖动、
 * onMoved 区分点击与拖动、拖动后抑制窗口、detail-visibility/pointer 事件同步。
 * 变更：保留旧版动画 WebP 与 reduced-motion 静态海报；invoke 调用改为
 * 当前项目 IPC 契约，锚点与详情仍使用独立窗口。
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useRef, useState } from "react";
import { fetchHoverbarPreferences } from "@/lib/ipc";
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
    if (dragging.current || Date.now() - dragFinishedAt.current < HOVERBAR_DRAG_SUPPRESS_MS) return;
    clearEnterTimer();
    clearCollapseTimer();
    void snapToEdge()
      .then(() => invoke<HoverbarAnchor>("show_hoverbar_detail"))
      .then((next) => {
        setAnchor(normalizeHoverbarAnchor(next));
        setDetailVisible(true);
      })
      .catch((error) => console.error("无法展开悬浮详情", error));
  }, [clearCollapseTimer, clearEnterTimer, snapToEdge]);

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
              if (moved) {
                dragFinishedAt.current = Date.now();
              } else if (!detailWasVisible) {
                showDetail();
              }
            });
        });
    },
    [clearCollapseTimer, clearEnterTimer, detailVisible, requestHide, showDetail, snapToEdge],
  );

  const activateOrb = useCallback(() => {
    if (dragMoved.current || dragging.current) return;
    if (detailVisible) requestHide();
    else showDetail();
  }, [detailVisible, requestHide, showDetail]);

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
      onMouseEnter={() => {
        clearCollapseTimer();
        if (detailVisible || dragging.current) return;
        clearEnterTimer();
        enterTimer.current = window.setTimeout(showDetail, HOVERBAR_ENTER_DELAY_MS);
      }}
      onMouseLeave={() => {
        if (detailVisible) scheduleHide();
        else clearEnterTimer();
      }}
    >
      <HoverbarOrb
        edge={anchor.edge}
        active={detailVisible}
        ariaLabel={detailVisible ? "收起额度详情并拖动" : "打开额度详情"}
        onActivate={activateOrb}
        onPointerDown={startDragging}
      />
    </div>
  );
}

/** 40x40 透明窗口内的 32px 动态玻璃小球。 */
export function HoverbarOrb({
  edge,
  active,
  ariaLabel,
  onActivate,
  onPointerDown,
  disabled = false,
  forceState,
}: {
  edge: string;
  active: boolean;
  ariaLabel: string;
  onActivate: () => void;
  onPointerDown: (event: React.PointerEvent<HTMLButtonElement>) => void;
  disabled?: boolean;
  forceState?: "hover" | "focus" | "active";
}) {
  return (
    <button
      type="button"
      aria-label={ariaLabel}
      title={ariaLabel}
      disabled={disabled}
      onDragStart={preventNativeAssetDrag}
      onClick={onActivate}
      onPointerDown={onPointerDown}
      className={`hb-orb${active ? " is-detail-open" : ""}${forceState ? ` is-${forceState}` : ""}`}
      data-edge={edge}
    >
      <picture>
        <source media="(prefers-reduced-motion: reduce)" srcSet="/assets/hover-orb-poster.png" />
        <img src="/assets/hover-orb.webp" alt="" draggable={false} />
      </picture>
    </button>
  );
}
