import { QueryClient } from "@tanstack/react-query";

/**
 * 共享 Query Client 工厂：主窗口与悬浮球窗口各自持有一个实例，
 * 但 queryKey 与后端命令一致，保证两窗口消费同一份数据源。
 */
export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        // 静态 ViewModel 阶段：数据进程内恒定，长缓存即可
        staleTime: 5 * 60 * 1000,
        gcTime: 30 * 60 * 1000,
        retry: 1,
        refetchOnWindowFocus: false,
      },
    },
  });
}

export const PLATFORM_SUMMARIES_QUERY_KEY = ["platform-summaries"] as const;
export const HOVERBAR_PREFERENCES_QUERY_KEY = ["hoverbar-preferences"] as const;
