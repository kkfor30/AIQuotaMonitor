import { Minus, Square, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * 自定义窗口标题栏（无边框主窗口）。
 * data-tauri-drag-region 提供原生拖动与双击最大化。
 */
export function WindowTitleBar() {
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

  const minimize = useCallback(() => void appWindow.minimize(), [appWindow]);
  const toggleMaximize = useCallback(
    () => void appWindow.toggleMaximize(),
    [appWindow],
  );
  const close = useCallback(() => void appWindow.close(), [appWindow]);

  return (
    <div className="flex h-10 shrink-0 items-center justify-end pr-2" data-tauri-drag-region>
      <div className="flex items-center gap-0.5">
        <TitleBarButton label="最小化" onClick={minimize}>
          <Minus size={14} />
        </TitleBarButton>
        <TitleBarButton label={maximized ? "还原" : "最大化"} onClick={toggleMaximize}>
          <Square size={12} />
        </TitleBarButton>
        <TitleBarButton label="关闭" onClick={close} danger>
          <X size={15} />
        </TitleBarButton>
      </div>
    </div>
  );
}

function TitleBarButton({
  label,
  onClick,
  danger = false,
  children,
}: {
  label: string;
  onClick: () => void;
  danger?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={`inline-flex h-7 w-10 cursor-pointer items-center justify-center rounded-md text-q-text-secondary transition-colors duration-100 ${
        danger ? "hover:bg-q-danger hover:text-white" : "hover:bg-q-border hover:text-q-text-primary"
      }`}
    >
      {children}
    </button>
  );
}
