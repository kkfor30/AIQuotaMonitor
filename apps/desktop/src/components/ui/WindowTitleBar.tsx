import { Check, Minus, Monitor, Moon, Palette, Square, Sun, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { fetchAppSettings, setAppTheme } from "@/lib/ipc";
import { APP_SETTINGS_QUERY_KEY } from "@/lib/query-client";
import { applyAppTheme } from "@/lib/theme";
import { cn } from "@/lib/cn";

/**
 * 自定义窗口标题栏（无边框主窗口）。
 * data-tauri-drag-region 提供原生拖动与双击最大化；
 * 窗口控制按钮左侧提供主题（换肤）快捷入口，与设置页共用同一份主题设置。
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
        <ThemeSwitchButton />
        <span aria-hidden className="mx-1 h-4 w-px bg-q-border" />
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

const THEME_OPTIONS: Array<{ id: string; label: string; icon: typeof Sun }> = [
  { id: "light", label: "浅色", icon: Sun },
  { id: "dark", label: "深色", icon: Moon },
  { id: "system", label: "跟随系统", icon: Monitor },
];

/** 标题栏换肤按钮：下拉选择浅色 / 深色 / 跟随系统，写入同一份主题设置。 */
function ThemeSwitchButton() {
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const { data: settings } = useQuery({
    queryKey: APP_SETTINGS_QUERY_KEY,
    queryFn: fetchAppSettings,
  });

  const themeMutation = useMutation({
    mutationFn: setAppTheme,
    onSuccess: (next) => {
      applyAppTheme(next.theme);
      queryClient.setQueryData(APP_SETTINGS_QUERY_KEY, next);
    },
  });

  // 点击菜单外部关闭
  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    window.addEventListener("mousedown", onPointerDown);
    return () => window.removeEventListener("mousedown", onPointerDown);
  }, [open]);

  const theme = settings?.theme ?? "system";

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        aria-label="切换主题"
        title="切换主题"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        className={cn(
          "inline-flex h-7 w-10 cursor-pointer items-center justify-center rounded-md text-q-text-secondary transition-colors duration-100",
          open
            ? "bg-q-primary-soft text-q-primary"
            : "hover:bg-q-border hover:text-q-text-primary",
        )}
      >
        <Palette size={14} />
      </button>
      {open && (
        <div
          role="menu"
          aria-label="主题"
          className="absolute right-0 top-[calc(100%+6px)] z-50 w-40 rounded-[13px] border border-q-border bg-q-surface-solid p-1.5 shadow-q-lg backdrop-blur"
        >
          {THEME_OPTIONS.map((option) => {
            const Icon = option.icon;
            const selected = theme === option.id;
            return (
              <button
                key={option.id}
                type="button"
                role="menuitemradio"
                aria-checked={selected}
                onClick={() => {
                  themeMutation.mutate(option.id);
                  setOpen(false);
                }}
                className={cn(
                  "flex w-full cursor-pointer items-center gap-2.5 rounded-[9px] px-2.5 py-2 text-left text-[13px] transition-colors duration-100",
                  selected
                    ? "bg-q-primary-soft text-q-primary"
                    : "text-q-text-secondary hover:bg-q-surface-hover hover:text-q-text-primary",
                )}
              >
                <Icon size={15} aria-hidden className="shrink-0" />
                <span className="flex-1">{option.label}</span>
                {selected && <Check size={14} aria-hidden className="shrink-0" />}
              </button>
            );
          })}
        </div>
      )}
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
