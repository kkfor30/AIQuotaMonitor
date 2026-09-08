/**
 * Tauri 运行时事件工具。
 *
 * 同时兼容 Tauri 事件与浏览器 CustomEvent，非 Tauri 环境下自动降级。
 */

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function safeListenEvent<T>(eventName: string, handler: (payload: T) => void): () => void {
  let unlistenTauri: (() => void) | undefined;

  if (isTauri()) {
    import("@tauri-apps/api/event")
      .then(({ listen }) => listen<T>(eventName, (e) => handler(e.payload)))
      .then((unsub) => {
        unlistenTauri = unsub;
      })
      .catch(() => {});
  }

  const customHandler = (e: Event) => {
    const custom = e as CustomEvent<T>;
    handler(custom.detail);
  };

  if (typeof window !== "undefined") {
    window.addEventListener(eventName, customHandler);
  }

  return () => {
    unlistenTauri?.();
    if (typeof window !== "undefined") {
      window.removeEventListener(eventName, customHandler);
    }
  };
}
