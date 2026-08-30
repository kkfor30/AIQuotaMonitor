import { useEffect, useRef, useState } from "react";

/**
 * 统一的内容宽度三档模式（全站唯一阈值来源，禁止组件各自定义冲突阈值）：
 * - wide：实际内容区 ≥ 960px
 * - medium：实际内容区 720 ~ 959px
 * - compact：实际内容区 < 720px
 */
export type ContainerMode = "wide" | "medium" | "compact";

export const CONTAINER_MODE_THRESHOLDS = { medium: 720, wide: 960 } as const;

/** Tibo 主从布局的分栏阈值：低于该宽度切单面板（列表/详情切换），不上下堆叠。 */
export const TIBO_SPLIT_MIN_PX = 680;

export function containerModeOf(width: number): ContainerMode {
  // 首帧未测量（0）时按桌面宽屏处理，避免闪到紧凑布局
  if (width <= 0) return "wide";
  if (width >= CONTAINER_MODE_THRESHOLDS.wide) return "wide";
  if (width >= CONTAINER_MODE_THRESHOLDS.medium) return "medium";
  return "compact";
}

/**
 * 用 ResizeObserver 测量元素实际内容宽度——窗口宽度不等于扣除导航/侧栏后的
 * 内容宽度，页面内部布局一律以组件可用宽度响应，不使用 window.innerWidth。
 */
export function useContainerWidth<T extends HTMLElement>() {
  const ref = useRef<T | null>(null);
  const [width, setWidth] = useState(0);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    setWidth(el.getBoundingClientRect().width);
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) setWidth(entry.contentRect.width);
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  return { ref, width, mode: containerModeOf(width) };
}
