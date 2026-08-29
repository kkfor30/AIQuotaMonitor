import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useQueryClient } from "@tanstack/react-query";
import {
  APP_SETTINGS_QUERY_KEY,
  HOVERBAR_PREFERENCES_QUERY_KEY,
  PLATFORM_SUMMARIES_QUERY_KEY,
  RADAR_SNAPSHOT_QUERY_KEY,
} from "./query-client";

/** 主窗口和悬浮窗共用：后端变更后立刻失效对应 Query。预览页没有 Tauri 时静默跳过。 */
export function useBackendQuerySync() {
  const queryClient = useQueryClient();

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    const watch = (
      event: string,
      keys: ReadonlyArray<readonly string[]>,
      shouldInvalidate?: (payload: unknown) => boolean,
    ) => {
      void listen(event, (event) => {
        if (shouldInvalidate && !shouldInvalidate(event.payload)) return;
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

    const isHoverbarWindow = Boolean(window.__HOVERBAR_ANCHOR__ || window.__HOVERBAR_DETAIL__);
    watch(
      "platform-data-changed",
      [PLATFORM_SUMMARIES_QUERY_KEY],
      // 单平台刷新（事件带平台 ID）：主窗口由发起页用命令返回值回填缓存，跳过重复失效；
      // 悬浮窗没有返回值，照常失效。全局事件（无平台 ID）不受影响。
      (payload) => isHoverbarWindow || typeof payload !== "string" || payload.length === 0,
    );
    watch("radar-data-changed", [RADAR_SNAPSHOT_QUERY_KEY]);
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
