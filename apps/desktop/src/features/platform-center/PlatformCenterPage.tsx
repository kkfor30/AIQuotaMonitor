import { useCallback, useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { PlatformTabs, type PlatformTabId } from "./PlatformTabs";
import { ProviderHeader } from "./ProviderHeader";
import { ProviderRail } from "./ProviderRail";
import { SourcesView } from "./SourcesView";
import { UsageView } from "./UsageView";
import { fetchPlatformSummaries } from "@/lib/ipc";
import { PLATFORM_SUMMARIES_QUERY_KEY } from "@/lib/query-client";
import type { PlatformCenterTarget } from "@/app/navigation";

/**
 * 平台中心：平台目录 + 页面头部 + 内部双 Tab。
 * - 切换平台保留当前 Tab
 * - 刷新按钮并行请求该平台全部 Source（阶段一为静态占位反馈）
 * - 支持从总览携带 providerId/sourceId/tab 定位（v5 交互 2）
 */
export function PlatformCenterPage({
  target,
  onTargetConsumed,
}: {
  target?: PlatformCenterTarget | null;
  onTargetConsumed?: () => void;
}) {
  const queryClient = useQueryClient();
  const { data: platforms = [], isLoading } = useQuery({
    queryKey: PLATFORM_SUMMARIES_QUERY_KEY,
    queryFn: fetchPlatformSummaries,
  });

  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [tab, setTab] = useState<PlatformTabId>("usage");
  const [focusSourceId, setFocusSourceId] = useState<string | undefined>(undefined);

  // 消费总览下发的定位目标（一次性）
  useEffect(() => {
    if (!target) return;
    if (platforms.some((p) => p.providerId === target.providerId)) {
      setSelectedId(target.providerId);
      setTab(target.tab);
      setFocusSourceId(target.focusSourceId);
    }
    onTargetConsumed?.();
  }, [target, platforms, onTargetConsumed]);

  const effectiveId = useMemo(() => {
    if (platforms.length === 0) return null;
    if (selectedId && platforms.some((p) => p.providerId === selectedId)) return selectedId;
    // 默认选中第一个已接入（非需配置）的平台，否则第一个
    return (platforms.find((p) => p.aggregateStatus !== "setup_required") ?? platforms[0])
      .providerId;
  }, [platforms, selectedId]);

  const platform = platforms.find((p) => p.providerId === effectiveId) ?? null;

  const refreshMutation = useMutation({
    mutationFn: async () => {
      // 阶段一：模拟并行刷新全部 Source 的短暂反馈；阶段二替换为真实命令
      await new Promise((resolve) => setTimeout(resolve, 800));
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: PLATFORM_SUMMARIES_QUERY_KEY });
    },
  });

  const handleSelect = useCallback((providerId: string) => {
    setSelectedId(providerId);
    setFocusSourceId(undefined);
  }, []);

  if (isLoading) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-q-text-muted">
        正在加载平台数据…
      </div>
    );
  }

  if (!platform) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-q-text-muted">
        暂无平台模板
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1">
      <ProviderRail platforms={platforms} selectedId={platform.providerId} onSelect={handleSelect} />

      <main className="flex min-h-0 min-w-0 flex-1 flex-col gap-4 p-5">
        <ProviderHeader
          platform={platform}
          refreshing={refreshMutation.isPending}
          onRefresh={() => refreshMutation.mutate()}
        />
        <PlatformTabs value={tab} onChange={setTab} />
        {tab === "usage" ? (
          <UsageView key={`${platform.providerId}-usage`} platform={platform} />
        ) : (
          <SourcesView
            key={`${platform.providerId}-sources`}
            platform={platform}
            focusSourceId={focusSourceId}
          />
        )}
      </main>
    </div>
  );
}
