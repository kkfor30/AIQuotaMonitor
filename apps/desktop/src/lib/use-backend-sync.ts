import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useQueryClient } from "@tanstack/react-query";
import {
  APP_SETTINGS_QUERY_KEY,
  HOVERBAR_PREFERENCES_QUERY_KEY,
  PLATFORM_SUMMARIES_QUERY_KEY,
} from "./query-client";

/** 主窗口和悬浮窗共用：后端变更后立刻失效对应 Query。预览页没有 Tauri 时静默跳过。 */
export function useBackendQuerySync() {
  const queryClient = useQueryClient();

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    const watch = (event: string, keys: ReadonlyArray<readonly string[]>) => {
      void listen(event, () => {
        for (const key of keys) {
          void queryClient.invalidateQueries({ queryKey: key });
        }
      })
        .then((unlisten) => {
          if (disposed) unlisten();
          else unlisteners.push(unlisten);
        })
        .catch(() => undefined);
    };

    watch("platform-data-changed", [PLATFORM_SUMMARIES_QUERY_KEY]);
    watch("source-credential-updated", [PLATFORM_SUMMARIES_QUERY_KEY]);
    watch("app-settings-changed", [
      APP_SETTINGS_QUERY_KEY,
      PLATFORM_SUMMARIES_QUERY_KEY,
      HOVERBAR_PREFERENCES_QUERY_KEY,
    ]);

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [queryClient]);
}
