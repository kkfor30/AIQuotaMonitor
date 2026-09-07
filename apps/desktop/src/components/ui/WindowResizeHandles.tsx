import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

type ResizeDirection = Parameters<ReturnType<typeof getCurrentWindow>["startResizeDragging"]>[0];

/**
 * 无边框主窗口（decorations: false）四周与四角调整大小热区。
 * 解决 Windows 下自定义标题栏与 Webview 内容区阻断原生窗体对角线缩放的问题，
 * 特别保障左上角（NorthWest）与右下角（SouthEast）横向与纵向同时缩放。
 * 最大化状态下自动注销，避免影响界面全屏操作。
 */
export function WindowResizeHandles() {
  const [maximized, setMaximized] = useState(false);
  const appWindow = getCurrentWindow();

  useEffect(() => {
    let disposed = false;
    const unlisten = appWindow.onResized(async () => {
      const next = await appWindow.isMaximized().catch(() => false);
      if (!disposed) setMaximized(next);
    });
    void appWindow
      .isMaximized()
      .then((next) => {
        if (!disposed) setMaximized(next);
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      void unlisten.then((fn) => fn());
    };
  }, [appWindow]);

  const handleMouseDown = useCallback(
    (direction: ResizeDirection) => (e: React.MouseEvent) => {
      if (e.button !== 0) return; // 仅左键响应拖拽缩放
      e.preventDefault();
      e.stopPropagation();
      void appWindow.startResizeDragging(direction).catch((err) => {
        console.warn(`[WindowResize] Failed to start resize dragging (${direction}):`, err);
      });
    },
    [appWindow],
  );

  // 窗口最大化时关闭缩放热区
  if (maximized) return null;

  return (
    <div aria-hidden className="pointer-events-none fixed inset-0 z-50 overflow-hidden select-none">
      {/* 4 个对角缩放区（热区 16x16，支持横向与纵向同时拉大或缩小） */}
      {/* 左上角（NorthWest） */}
      <div
        onMouseDown={handleMouseDown("NorthWest")}
        className="pointer-events-auto absolute top-0 left-0 h-4 w-4 cursor-nwse-resize"
        title="双向缩放窗口"
      />
      {/* 右下角（SouthEast） */}
      <div
        onMouseDown={handleMouseDown("SouthEast")}
        className="pointer-events-auto absolute right-0 bottom-0 h-4 w-4 cursor-nwse-resize"
        title="双向缩放窗口"
      />
      {/* 右上角（NorthEast） */}
      <div
        onMouseDown={handleMouseDown("NorthEast")}
        className="pointer-events-auto absolute top-0 right-0 h-3.5 w-3.5 cursor-nesw-resize"
        title="双向缩放窗口"
      />
      {/* 左下角（SouthWest） */}
      <div
        onMouseDown={handleMouseDown("SouthWest")}
        className="pointer-events-auto absolute bottom-0 left-0 h-4 w-4 cursor-nesw-resize"
        title="双向缩放窗口"
      />

      {/* 4 条边缘线性缩放区 */}
      {/* 顶边 */}
      <div
        onMouseDown={handleMouseDown("North")}
        className="pointer-events-auto absolute top-0 right-3.5 left-4 h-1.5 cursor-ns-resize"
      />
      {/* 底边 */}
      <div
        onMouseDown={handleMouseDown("South")}
        className="pointer-events-auto absolute right-4 bottom-0 left-4 h-1.5 cursor-ns-resize"
      />
      {/* 左边 */}
      <div
        onMouseDown={handleMouseDown("West")}
        className="pointer-events-auto absolute top-4 bottom-4 left-0 w-1.5 cursor-ew-resize"
      />
      {/* 右边 */}
      <div
        onMouseDown={handleMouseDown("East")}
        className="pointer-events-auto absolute top-3.5 bottom-4 right-0 w-1.5 cursor-ew-resize"
      />
    </div>
  );
}
